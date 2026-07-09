use super::functions::parameter_from_shape;
use super::*;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower a WU1 method signature: non-generic, non-`this`, non-accessor,
    /// static name. Optional methods stay out of subset.
    pub(in crate::check::checker) fn lower_method_signature_property(
        &mut self,
        scope: ScopeId,
        sig: &TSMethodSignature<'_>,
    ) -> Option<PropertyType> {
        if sig.kind != TSMethodSignatureKind::Method
            || sig.type_parameters.is_some()
            || sig.this_param.is_some()
            || sig.optional
        {
            return None;
        }

        let name = sig.key.static_name()?;
        let ty = self.lower_strict_signature_function_type(
            scope,
            &sig.params,
            sig.return_type.as_deref(),
        )?;
        Some(PropertyType::public(name.into_owned(), ty))
    }

    /// Lower a WU2 call signature: non-generic, non-`this`, with represented
    /// optional/default/rest parameter shape.
    /// Other signatures stay out of subset and do not create callability.
    pub(in crate::check::checker) fn lower_call_signature(
        &mut self,
        scope: ScopeId,
        sig: &TSCallSignatureDeclaration<'_>,
    ) -> Option<TypeId> {
        if sig.type_parameters.is_some() || sig.this_param.is_some() {
            return None;
        }
        self.lower_strict_signature_function_type(scope, &sig.params, sig.return_type.as_deref())
    }

    /// Lower a WU3 construct signature: non-generic with a representable instance
    /// type. Other signatures do not create constructability.
    pub(in crate::check::checker) fn lower_construct_signature(
        &mut self,
        scope: ScopeId,
        sig: &TSConstructSignatureDeclaration<'_>,
    ) -> Option<TypeId> {
        if sig.type_parameters.is_some() {
            return None;
        }
        let return_type = sig.return_type.as_deref()?;
        if !self.signature_annotations_are_locally_resolvable(scope, &sig.params, return_type) {
            return None;
        }
        self.lower_strict_construct_function_type(scope, &sig.params, return_type)
    }

    /// Lower a function-like signature. Bad parameter annotations become the error
    /// type to suppress cascades; missing returns become `void`.
    pub(in crate::check::checker) fn lower_signature_function_type(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: Option<&TSTypeAnnotation<'_>>,
    ) -> Option<TypeId> {
        let void_ty = self.interner.well_known().void;
        let params = self.lower_signature_parameters(scope, params);
        let ret = match return_type {
            Some(ann) => self.lower_annotation(scope, &ann.type_annotation)?,
            None => void_ty,
        };
        Some(self.interner.intern_function(FunctionType { params, ret }))
    }

    /// Lower signature parameters for function-typed properties and class
    /// method/constructor signatures.
    pub(in crate::check::checker) fn lower_signature_parameters(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
    ) -> Vec<ParameterType> {
        let error_ty = self.interner.well_known().error;
        let mut lowered: Vec<ParameterType> =
            Vec::with_capacity(params.items.len() + usize::from(params.rest.is_some()));
        for param in &params.items {
            let name = parameter_name(&param.pattern).unwrap_or_default();
            let ty = match param.type_annotation.as_ref() {
                Some(ann) => self
                    .lower_annotation(scope, &ann.type_annotation)
                    .unwrap_or(error_ty),
                None => error_ty,
            };
            lowered.push(parameter_from_shape(
                name,
                ty,
                param.optional,
                param.initializer.is_some(),
            ));
        }
        if let Some(rest) = &params.rest {
            let name = parameter_name(&rest.rest.argument).unwrap_or_default();
            let ty = match rest.type_annotation.as_ref() {
                Some(ann) => self
                    .lower_annotation(scope, &ann.type_annotation)
                    .unwrap_or(error_ty),
                None => error_ty,
            };
            lowered.push(ParameterType::rest(name, ty));
        }
        lowered
    }

    pub(in crate::check::checker) fn overloaded_method_names(
        &self,
        members: &[TSSignature<'_>],
    ) -> FxHashSet<String> {
        let mut counts: FxHashMap<String, (usize, bool)> = FxHashMap::default();
        for member in members {
            let (name, is_method) = match member {
                TSSignature::TSPropertySignature(sig) => (sig.key.static_name(), false),
                TSSignature::TSMethodSignature(sig) => (sig.key.static_name(), true),
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

    pub(in crate::check::checker) fn call_signatures_overloaded(
        &self,
        members: &[TSSignature<'_>],
    ) -> bool {
        members
            .iter()
            .filter(|member| matches!(member, TSSignature::TSCallSignatureDeclaration(_)))
            .count()
            > 1
    }

    pub(in crate::check::checker) fn construct_signatures_overloaded(
        &self,
        members: &[TSSignature<'_>],
    ) -> bool {
        members
            .iter()
            .filter(|member| matches!(member, TSSignature::TSConstructSignatureDeclaration(_)))
            .count()
            > 1
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
        let [param] = sig.parameters.as_slice() else {
            return None;
        };
        let key = self.lower_annotation(scope, &param.type_annotation.type_annotation)?;
        // B29: an index-signature VALUE is a legal-recursion boundary (the canonical
        // `type Json = … | { [k: string]: Json }`), so it lowers one indirection deeper.
        // The key stays at surface depth (recursion through a key is never legal).
        let value = self.with_indirection(|p| {
            p.lower_annotation(scope, &sig.type_annotation.type_annotation)
        })?;
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
