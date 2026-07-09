use super::*;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower a constructor-type annotation (`new (x: T) => U`) to an object carrying
    /// a single construct signature, making it equivalent to `{ new (x: T): U }` in
    /// relation while preserving named object members for the object-literal form.
    pub(super) fn lower_constructor_type(
        &mut self,
        scope: ScopeId,
        ctor: &TSConstructorType<'_>,
    ) -> Option<TypeId> {
        if ctor.r#abstract || ctor.type_parameters.is_some() {
            return None;
        }
        if !self.signature_annotations_are_locally_resolvable(
            scope,
            &ctor.params,
            &ctor.return_type,
        ) {
            return None;
        }
        let signature =
            self.lower_strict_construct_function_type(scope, &ctor.params, &ctor.return_type)?;
        Some(self.interner.intern_object(ObjectType {
            construct_signatures: vec![signature],
            ..Default::default()
        }))
    }

    pub(super) fn lower_strict_construct_function_type(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: &TSTypeAnnotation<'_>,
    ) -> Option<TypeId> {
        self.signature_params_in_subset(params)?;
        let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
        for param in &params.items {
            let name = parameter_name(&param.pattern)?;
            let annotation = param.type_annotation.as_ref()?;
            // B29: a parameter/return is a legal-recursion boundary.
            let ty =
                self.with_indirection(|p| p.lower_annotation(scope, &annotation.type_annotation))?;
            lowered.push(ParameterType::required(name, ty));
        }
        let ret =
            self.with_indirection(|p| p.lower_annotation(scope, &return_type.type_annotation))?;
        Some(self.interner.intern_function(FunctionType {
            params: lowered,
            ret,
        }))
    }

    pub(super) fn signature_annotations_are_locally_resolvable(
        &self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: &TSTypeAnnotation<'_>,
    ) -> bool {
        let params_ok = params.items.iter().all(|param| {
            param.type_annotation.as_ref().is_some_and(|ann| {
                self.annotation_type_refs_are_locally_resolvable(scope, &ann.type_annotation)
            })
        });
        params_ok
            && self.annotation_type_refs_are_locally_resolvable(scope, &return_type.type_annotation)
    }

    fn annotation_type_refs_are_locally_resolvable(&self, scope: ScopeId, ty: &TSType<'_>) -> bool {
        match ty {
            TSType::TSAnyKeyword(_)
            | TSType::TSUnknownKeyword(_)
            | TSType::TSNeverKeyword(_)
            | TSType::TSVoidKeyword(_)
            | TSType::TSNullKeyword(_)
            | TSType::TSUndefinedKeyword(_)
            | TSType::TSBooleanKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSStringKeyword(_)
            | TSType::TSLiteralType(_) => true,
            TSType::TSTypeReference(reference) => {
                let TSTypeName::IdentifierReference(ident) = &reference.type_name else {
                    return false;
                };
                let name = ident.name.as_str();
                if name == "Array" {
                    let Some(args) = reference.type_arguments.as_deref() else {
                        return false;
                    };
                    let [arg] = args.params.as_slice() else {
                        return false;
                    };
                    return self.annotation_type_refs_are_locally_resolvable(scope, arg);
                }
                let found = self.lookup_type_param(name).is_some()
                    || type_decl_id(self.binder, scope, name).is_some();
                found
                    && reference.type_arguments.as_ref().is_none_or(|args| {
                        args.params
                            .iter()
                            .all(|arg| self.annotation_type_refs_are_locally_resolvable(scope, arg))
                    })
            }
            TSType::TSTypeLiteral(lit) => lit.members.iter().all(|member| match member {
                TSSignature::TSPropertySignature(sig) => {
                    sig.type_annotation.as_ref().is_some_and(|ann| {
                        self.annotation_type_refs_are_locally_resolvable(
                            scope,
                            &ann.type_annotation,
                        )
                    })
                }
                TSSignature::TSIndexSignature(sig) => {
                    sig.parameters.iter().all(|param| {
                        self.annotation_type_refs_are_locally_resolvable(
                            scope,
                            &param.type_annotation.type_annotation,
                        )
                    }) && self.annotation_type_refs_are_locally_resolvable(
                        scope,
                        &sig.type_annotation.type_annotation,
                    )
                }
                TSSignature::TSMethodSignature(sig) => {
                    sig.return_type.as_ref().is_some_and(|ret| {
                        self.signature_annotations_are_locally_resolvable(scope, &sig.params, ret)
                    })
                }
                TSSignature::TSCallSignatureDeclaration(sig) => {
                    sig.return_type.as_ref().is_some_and(|ret| {
                        self.signature_annotations_are_locally_resolvable(scope, &sig.params, ret)
                    })
                }
                TSSignature::TSConstructSignatureDeclaration(sig) => {
                    sig.return_type.as_ref().is_some_and(|ret| {
                        self.signature_annotations_are_locally_resolvable(scope, &sig.params, ret)
                    })
                }
            }),
            TSType::TSFunctionType(func) => self.signature_annotations_are_locally_resolvable(
                scope,
                &func.params,
                &func.return_type,
            ),
            TSType::TSConstructorType(ctor) => {
                !ctor.r#abstract
                    && ctor.type_parameters.is_none()
                    && self.signature_annotations_are_locally_resolvable(
                        scope,
                        &ctor.params,
                        &ctor.return_type,
                    )
            }
            TSType::TSUnionType(union) => union
                .types
                .iter()
                .all(|member| self.annotation_type_refs_are_locally_resolvable(scope, member)),
            // M31: an intersection is locally resolvable iff every member is (mirrors union).
            TSType::TSIntersectionType(intersection) => intersection
                .types
                .iter()
                .all(|member| self.annotation_type_refs_are_locally_resolvable(scope, member)),
            TSType::TSParenthesizedType(paren) => {
                self.annotation_type_refs_are_locally_resolvable(scope, &paren.type_annotation)
            }
            TSType::TSArrayType(array) => {
                self.annotation_type_refs_are_locally_resolvable(scope, &array.element_type)
            }
            TSType::TSTupleType(tuple) => tuple.element_types.iter().all(|element| {
                element
                    .as_ts_type()
                    .is_some_and(|ty| self.annotation_type_refs_are_locally_resolvable(scope, ty))
            }),
            TSType::TSTypeOperatorType(op) => {
                op.operator == TSTypeOperatorOperator::Keyof
                    && self.annotation_type_refs_are_locally_resolvable(scope, &op.type_annotation)
            }
            TSType::TSIndexedAccessType(access) => {
                self.annotation_type_refs_are_locally_resolvable(scope, &access.object_type)
                    && self.annotation_type_refs_are_locally_resolvable(scope, &access.index_type)
            }
            _ => false,
        }
    }

    /// Lower object/interface signatures only when every parameter and return type
    /// is representable. Unlike class/free-function lowering, bad parameters do
    /// not become the error type because that would create fake callability.
    pub(super) fn lower_strict_signature_function_type(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: Option<&TSTypeAnnotation<'_>>,
    ) -> Option<TypeId> {
        self.signature_params_in_subset(params)?;
        let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
        for param in &params.items {
            let name = parameter_name(&param.pattern)?;
            let annotation = param.type_annotation.as_ref()?;
            let ty = self.lower_annotation(scope, &annotation.type_annotation)?;
            lowered.push(ParameterType::required(name, ty));
        }
        let ret = match return_type {
            Some(ann) => self.lower_annotation(scope, &ann.type_annotation)?,
            None => self.interner.well_known().void,
        };
        Some(self.interner.intern_function(FunctionType {
            params: lowered,
            ret,
        }))
    }

    /// Required-only parameter subset shared by WU2 call signatures and the
    /// function-type annotation lowerer. Rest and optional parameters need
    /// different arity/relation rules, so accepting them as required would
    /// mis-state the type.
    pub(super) fn signature_params_in_subset(&self, params: &FormalParameters<'_>) -> Option<()> {
        if params.rest.is_some() {
            return None;
        }
        for param in &params.items {
            if param.optional || param.initializer.is_some() {
                return None;
            }
        }
        Some(())
    }
}
