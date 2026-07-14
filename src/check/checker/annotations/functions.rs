use super::super::calls::{parameter_count, parameter_syntaxes, ParameterSyntax};
use super::super::decls::alloc_type_param_ids;
use super::*;
use oxc_ast::ast::TSTypeParameterDeclaration;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower a constructor-type annotation (`new (x: T) => U`) to an object carrying
    /// a single construct signature, making it equivalent to `{ new (x: T): U }` in
    /// relation while preserving named object members for the object-literal form.
    pub(super) fn lower_constructor_type(
        &mut self,
        scope: ScopeId,
        ctor: &TSConstructorType<'_>,
    ) -> Option<TypeId> {
        if ctor.r#abstract {
            return None;
        }
        if ctor.type_parameters.is_none()
            && !self.signature_annotations_are_locally_resolvable(
                scope,
                &ctor.params,
                &ctor.return_type,
            )
        {
            return None;
        }
        let signature = self.lower_generic_strict_construct_function_type(
            scope,
            ctor.type_parameters.as_deref(),
            &ctor.params,
            &ctor.return_type,
        )?;
        Some(self.interner.intern_object(ObjectType {
            construct_signatures: vec![signature],
            ..Default::default()
        }))
    }

    pub(super) fn lower_generic_strict_construct_function_type(
        &mut self,
        scope: ScopeId,
        type_parameter_decl: Option<&TSTypeParameterDeclaration<'_>>,
        params: &FormalParameters<'_>,
        return_type: &TSTypeAnnotation<'_>,
    ) -> Option<TypeId> {
        let ids = alloc_type_param_ids(type_parameter_decl, &mut self.next_type_param);
        let frame = self.build_type_param_frame(type_parameter_decl, &ids);
        self.with_type_params(frame, |pass| {
            let type_params = pass.lower_signature_type_params(scope, type_parameter_decl, &ids);
            let params = pass.lower_strict_signature_parameters(scope, params, true)?;
            let ret =
                pass.with_indirection(|p| p.lower_annotation(scope, &return_type.type_annotation))?;
            Some(pass.interner.intern_function(FunctionType {
                type_params,
                receiver: None,
                params,
                ret,
            }))
        })
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
        }) && params.rest.as_ref().is_none_or(|rest| {
            rest.type_annotation.as_ref().is_some_and(|ann| {
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
            TSType::TSTupleType(tuple) => tuple
                .element_types
                .iter()
                .all(|element| self.tuple_element_refs_are_locally_resolvable(scope, element)),
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

    pub(super) fn lower_strict_signature_parameters(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        with_indirection: bool,
    ) -> Option<Vec<ParameterType>> {
        let mut lowered = Vec::with_capacity(parameter_count(params));
        for syntax in parameter_syntaxes(params) {
            let name = parameter_name(syntax.pattern())?;
            let annotation = match syntax {
                ParameterSyntax::Fixed { parameter, .. } => parameter.type_annotation.as_ref()?,
                ParameterSyntax::Rest { parameter } => parameter.type_annotation.as_ref()?,
            };
            let ty = if with_indirection {
                self.with_indirection(|p| p.lower_annotation(scope, &annotation.type_annotation))?
            } else {
                self.lower_annotation(scope, &annotation.type_annotation)?
            };
            lowered.push(syntax.with_type(name, ty));
        }
        Some(lowered)
    }

    fn tuple_element_refs_are_locally_resolvable(
        &self,
        scope: ScopeId,
        element: &TSTupleElement<'_>,
    ) -> bool {
        match element {
            TSTupleElement::TSRestType(rest) => {
                self.annotation_type_refs_are_locally_resolvable(scope, &rest.type_annotation)
            }
            TSTupleElement::TSOptionalType(_) => false,
            _ => match element.as_ts_type() {
                Some(TSType::TSNamedTupleMember(_)) | None => false,
                Some(ty) => self.annotation_type_refs_are_locally_resolvable(scope, ty),
            },
        }
    }
}
