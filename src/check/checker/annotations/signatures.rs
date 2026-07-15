use super::super::decls::alloc_type_param_ids;
use super::*;
use oxc_ast::ast::TSTypeParameterDeclaration;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Record the incomplete surface for a skipped computed property-signature key
    /// (`{ [expr]: T }`, owner 75). Shared by object-type and interface member
    /// collection so a computed key is accounted before the member is dropped (WU5).
    pub(in crate::check::checker) fn record_property_signature_computed_key(
        &mut self,
        key: &oxc_ast::ast::PropertyKey<'_>,
    ) {
        self.record_incomplete(
            "signature/property-signature/computed-key",
            Span::from_oxc(key.span()),
            "computed property signature key not visited",
        );
    }

    /// Record the incomplete surface for a skipped computed method-signature key
    /// (`{ [expr](): T }`, owner 75) — the method twin of the helper above (WU7-E F1).
    pub(in crate::check::checker) fn record_method_signature_computed_key(
        &mut self,
        key: &oxc_ast::ast::PropertyKey<'_>,
    ) {
        self.record_incomplete(
            "signature/method-signature/computed-key",
            Span::from_oxc(key.span()),
            "computed method signature key not visited",
        );
    }

    /// Lower a method signature with a fresh nested method frame. Optional methods
    /// remain deferred.
    pub(in crate::check::checker) fn lower_method_signature_property(
        &mut self,
        scope: ScopeId,
        sig: &TSMethodSignature<'_>,
    ) -> Option<PropertyType> {
        let name = sig.key.static_name();
        let ty = self.lower_generic_strict_signature_function_type(
            scope,
            sig.type_parameters.as_deref(),
            sig.this_param.as_deref(),
            &sig.params,
            sig.return_type.as_deref(),
        );
        if sig.kind != TSMethodSignatureKind::Method || sig.optional {
            return None;
        }
        let (Some(name), Some(ty)) = (name, ty) else {
            return None;
        };
        Some(PropertyType::public(name.into_owned(), ty))
    }

    /// Lower a call signature with represented optional/default/rest shape.
    pub(in crate::check::checker) fn lower_call_signature(
        &mut self,
        scope: ScopeId,
        sig: &TSCallSignatureDeclaration<'_>,
    ) -> Option<TypeId> {
        self.lower_generic_strict_signature_function_type(
            scope,
            sig.type_parameters.as_deref(),
            sig.this_param.as_deref(),
            &sig.params,
            sig.return_type.as_deref(),
        )
    }

    /// Lower a construct signature with a representable instance type.
    pub(in crate::check::checker) fn lower_construct_signature(
        &mut self,
        scope: ScopeId,
        sig: &TSConstructSignatureDeclaration<'_>,
    ) -> Option<TypeId> {
        self.lower_generic_strict_construct_function_type(
            scope,
            sig.type_parameters.as_deref(),
            &sig.params,
            sig.return_type.as_deref(),
        )
    }

    pub(in crate::check::checker) fn lower_generic_strict_signature_function_type(
        &mut self,
        scope: ScopeId,
        type_parameter_decl: Option<&TSTypeParameterDeclaration<'_>>,
        this_param: Option<&TSThisParameter<'_>>,
        params: &FormalParameters<'_>,
        return_type: Option<&TSTypeAnnotation<'_>>,
    ) -> Option<TypeId> {
        let ids = alloc_type_param_ids(type_parameter_decl, &mut self.next_type_param);
        let frame = self.build_type_param_frame(type_parameter_decl, &ids);
        self.with_type_params(frame, |pass| {
            let type_params = pass.lower_signature_type_params(scope, type_parameter_decl, &ids);
            let receiver = pass.lower_this_parameter(scope, this_param);
            let params = pass.lower_strict_signature_parameters(scope, params, false);
            let ret = match return_type {
                Some(ann) => pass.lower_annotation(scope, &ann.type_annotation),
                None => Some(pass.interner.well_known().void),
            };
            let (Some(receiver), Some(params), Some(ret)) = (receiver, params, ret) else {
                return None;
            };
            if type_params.unavailable {
                return None;
            }
            Some(pass.interner.intern_function(FunctionType {
                type_params: type_params.params,
                receiver,
                params,
                ret,
            }))
        })
    }

    /// Lower the non-positional explicit receiver of a function-like signature.
    pub(in crate::check::checker) fn lower_this_parameter(
        &mut self,
        scope: ScopeId,
        this_param: Option<&TSThisParameter<'_>>,
    ) -> Option<Option<TypeId>> {
        let Some(this_param) = this_param else {
            return Some(None);
        };
        let annotation = this_param.type_annotation.as_ref()?;
        Some(Some(
            self.lower_annotation(scope, &annotation.type_annotation)?,
        ))
    }

    pub(in crate::check::checker) fn overloaded_method_names(
        &self,
        members: &[TSSignature<'_>],
    ) -> FxHashSet<String> {
        let mut counts: FxHashMap<String, (usize, bool)> = FxHashMap::default();
        for member in members {
            let (name, is_method) = match member {
                TSSignature::TSPropertySignature(sig) if !sig.computed => {
                    (sig.key.static_name(), false)
                }
                TSSignature::TSMethodSignature(sig) if !sig.computed => {
                    (sig.key.static_name(), true)
                }
                _ => (None, false),
            };
            let Some(name) = name else {
                continue;
            };
            let entry = counts.entry(name.into_owned()).or_insert((0, false));
            entry.0 += 1;
            entry.1 |= is_method;
        }
        counts
            .into_iter()
            .filter_map(|(name, (count, has_method))| {
                if count > 1 && has_method {
                    Some(name)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Lower an M19 index signature into `object`. Only `[k: string]: T` and
    /// `[i: number]: T` are represented; malformed, unsupported-key, or unlowerable
    /// signatures abort the enclosing annotation. `readonly` indexes are deferred.
    pub(in crate::check::checker) fn lower_index_signature(
        &mut self,
        scope: ScopeId,
        sig: &oxc_ast::ast::TSIndexSignature<'_>,
        object: &mut ObjectType,
    ) -> Option<()> {
        // Exactly one key parameter (`[k: string]`); anything else is malformed.
        let key = match sig.parameters.as_slice() {
            [param] => self.lower_annotation(scope, &param.type_annotation.type_annotation),
            params => {
                for param in params {
                    self.lower_annotation(scope, &param.type_annotation.type_annotation);
                }
                None
            }
        };
        // B29: an index-signature VALUE is a legal-recursion boundary (the canonical
        // `type Json = … | { [k: string]: Json }`), so it lowers one indirection deeper.
        // The key stays at surface depth (recursion through a key is never legal).
        let value = self
            .with_indirection(|p| p.lower_annotation(scope, &sig.type_annotation.type_annotation));
        let (Some(key), Some(value)) = (key, value) else {
            return None;
        };
        let wk = self.interner.well_known();
        if key == wk.string {
            object.string_index = Some(value);
            Some(())
        } else if key == wk.number {
            object.number_index = Some(value);
            Some(())
        } else {
            // A symbol / template-literal / other index-key type is out of the M19
            // subset → abort the enclosing annotation.
            None
        }
    }
}
