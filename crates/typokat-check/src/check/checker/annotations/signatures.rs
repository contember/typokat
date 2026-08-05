use super::super::decls::alloc_type_param_ids;
use super::*;
use oxc_ast::ast::TSTypeParameterDeclaration;

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    pub(in crate::check::checker) fn signature_property_key(
        &self,
        scope: ScopeId,
        key: &oxc_ast::ast::PropertyKey<'_>,
        computed: bool,
    ) -> Option<PropertyKey> {
        if !computed {
            return key
                .static_name()
                .map(|name| PropertyKey::String(name.into_owned()));
        }
        let oxc_ast::ast::PropertyKey::StaticMemberExpression(member) = key else {
            return None;
        };
        let symbol = well_known_symbol_from_name(member.property.name.as_str())?;
        self.authenticate_well_known_symbol_object(scope, &member.object, symbol)
    }

    pub(in crate::check::checker) fn authenticated_well_known_symbol_expression_key(
        &self,
        scope: ScopeId,
        expression: &Expression<'_>,
    ) -> Option<PropertyKey> {
        let expression = unparenthesized_expression(expression);
        let (object, symbol) = match expression {
            Expression::StaticMemberExpression(member) => (
                &member.object,
                well_known_symbol_from_name(member.property.name.as_str())?,
            ),
            Expression::ComputedMemberExpression(member) => (
                &member.object,
                well_known_symbol_from_key_expression(&member.expression)?,
            ),
            _ => return None,
        };
        self.authenticate_well_known_symbol_object(
            scope,
            unparenthesized_expression(object),
            symbol,
        )
    }

    fn authenticate_well_known_symbol_object(
        &self,
        scope: ScopeId,
        object: &Expression<'_>,
        symbol: WellKnownSymbol,
    ) -> Option<PropertyKey> {
        let Expression::Identifier(object) = object else {
            return None;
        };
        if object.name != "Symbol" {
            return None;
        }
        let certified = self.certified_library_values.symbol?;
        let crate::binder::bind::ValueResolution::Resolved {
            symbol: binding, ..
        } = self.resolve_value_binding_replay(scope, "Symbol")
        else {
            return None;
        };
        if self
            .binder
            .symbols
            .get(binding)
            .and_then(|binding| binding.value)
            != Some(certified)
        {
            return None;
        }
        Some(PropertyKey::WellKnownSymbol(symbol))
    }

    pub(in crate::check::checker) fn public_signature_property(
        key: PropertyKey,
        ty: TypeId,
    ) -> PropertyType {
        match key {
            PropertyKey::String(name) => PropertyType::public(name, ty),
            PropertyKey::WellKnownSymbol(symbol) => PropertyType::well_known_symbol(symbol, ty),
        }
    }

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
        let key = self.signature_property_key(scope, &sig.key, sig.computed);
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
        let (Some(key), Some(ty)) = (key, ty) else {
            return None;
        };
        Some(Self::public_signature_property(key, ty))
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
                Some(ann) => pass.lower_callable_annotation(scope, &ann.type_annotation, false),
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
        Some(Some(self.lower_callable_annotation(
            scope,
            &annotation.type_annotation,
            false,
        )?))
    }

    pub(in crate::check::checker) fn overloaded_method_keys(
        &self,
        scope: ScopeId,
        members: &[TSSignature<'_>],
    ) -> FxHashSet<PropertyKey> {
        let mut counts: FxHashMap<PropertyKey, (usize, bool)> = FxHashMap::default();
        for member in members {
            let (key, is_method) = match member {
                TSSignature::TSPropertySignature(sig) => (
                    self.signature_property_key(scope, &sig.key, sig.computed),
                    false,
                ),
                TSSignature::TSMethodSignature(sig) => (
                    self.signature_property_key(scope, &sig.key, sig.computed),
                    true,
                ),
                _ => (None, false),
            };
            let Some(key) = key else {
                continue;
            };
            let entry = counts.entry(key).or_insert((0, false));
            entry.0 += 1;
            entry.1 |= is_method;
        }
        counts
            .into_iter()
            .filter_map(|(key, (count, has_method))| {
                if count > 1 && has_method {
                    Some(key)
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

pub(in crate::check::checker) fn well_known_symbol_from_name(
    name: &str,
) -> Option<WellKnownSymbol> {
    match name {
        "iterator" => Some(WellKnownSymbol::Iterator),
        "toStringTag" => Some(WellKnownSymbol::ToStringTag),
        "asyncIterator" => Some(WellKnownSymbol::AsyncIterator),
        "species" => Some(WellKnownSymbol::Species),
        "toPrimitive" => Some(WellKnownSymbol::ToPrimitive),
        "replace" => Some(WellKnownSymbol::Replace),
        "unscopables" => Some(WellKnownSymbol::Unscopables),
        "split" => Some(WellKnownSymbol::Split),
        "search" => Some(WellKnownSymbol::Search),
        "match" => Some(WellKnownSymbol::Match),
        "matchAll" => Some(WellKnownSymbol::MatchAll),
        "hasInstance" => Some(WellKnownSymbol::HasInstance),
        _ => None,
    }
}

fn well_known_symbol_from_key_expression(expression: &Expression<'_>) -> Option<WellKnownSymbol> {
    match unparenthesized_expression(expression) {
        Expression::StringLiteral(literal) => well_known_symbol_from_name(literal.value.as_str()),
        _ => None,
    }
}

fn unparenthesized_expression<'node, 'ast>(
    expression: &'node Expression<'ast>,
) -> &'node Expression<'ast> {
    let mut expression = expression;
    while let Expression::ParenthesizedExpression(parenthesized) = expression {
        expression = &parenthesized.expression;
    }
    expression
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_symbol_name_mapping_is_exact() {
        let expected = [
            ("iterator", WellKnownSymbol::Iterator),
            ("toStringTag", WellKnownSymbol::ToStringTag),
            ("asyncIterator", WellKnownSymbol::AsyncIterator),
            ("species", WellKnownSymbol::Species),
            ("toPrimitive", WellKnownSymbol::ToPrimitive),
            ("replace", WellKnownSymbol::Replace),
            ("unscopables", WellKnownSymbol::Unscopables),
            ("split", WellKnownSymbol::Split),
            ("search", WellKnownSymbol::Search),
            ("match", WellKnownSymbol::Match),
            ("matchAll", WellKnownSymbol::MatchAll),
            ("hasInstance", WellKnownSymbol::HasInstance),
        ];
        for (name, symbol) in expected {
            assert_eq!(well_known_symbol_from_name(name), Some(symbol));
        }
        assert_eq!(well_known_symbol_from_name("Iterator"), None);
        assert_eq!(well_known_symbol_from_name("local"), None);
    }
}
