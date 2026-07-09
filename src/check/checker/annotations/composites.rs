use super::*;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower `A | B | …` to a canonical interned union. Any unlowerable member
    /// aborts the whole annotation; dropping it would mis-state the union.
    pub(super) fn lower_union_annotation(
        &mut self,
        scope: ScopeId,
        members: &[TSType<'_>],
    ) -> Option<TypeId> {
        let mut lowered: Vec<TypeId> = Vec::with_capacity(members.len());
        for member in members {
            lowered.push(self.lower_annotation(scope, member)?);
        }
        Some(self.interner.union(lowered))
    }

    /// Lower `A & B & …` to a canonical interned intersection. Any unlowerable
    /// member aborts the whole annotation; dropping it would mis-state the type.
    pub(super) fn lower_intersection_annotation(
        &mut self,
        scope: ScopeId,
        members: &[TSType<'_>],
    ) -> Option<TypeId> {
        let mut lowered: Vec<TypeId> = Vec::with_capacity(members.len());
        for member in members {
            lowered.push(self.lower_annotation(scope, member)?);
        }
        Some(self.interner.intersection(lowered))
    }

    /// Lower `[A, B, …]` to an ordered interned tuple; `[]` is the empty tuple.
    /// Optional and named tuple members are out of subset and abort the
    /// annotation rather than silently mis-shaping the tuple.
    pub(super) fn lower_tuple_annotation(
        &mut self,
        scope: ScopeId,
        elements: &[TSTupleElement<'_>],
    ) -> Option<TypeId> {
        let mut lowered: Vec<TypeId> = Vec::with_capacity(elements.len());
        let mut rest: Option<TupleRestType> = None;
        for element in elements {
            match element {
                TSTupleElement::TSRestType(rest_type) => {
                    if rest.is_some() {
                        return None;
                    }
                    let ty = self.with_indirection(|p| {
                        p.lower_annotation(scope, &rest_type.type_annotation)
                    })?;
                    rest = Some(TupleRestType::new(lowered.len(), ty));
                }
                TSTupleElement::TSOptionalType(_) => return None,
                _ => {
                    let ts_type = element.as_ts_type()?;
                    if matches!(ts_type, TSType::TSNamedTupleMember(_)) {
                        return None;
                    }
                    lowered.push(self.with_indirection(|p| p.lower_annotation(scope, ts_type))?);
                }
            }
        }
        if let Some(rest) = rest {
            Some(
                self.interner
                    .intern_tuple_type(TupleType::with_rest(lowered, rest)),
            )
        } else {
            Some(self.interner.intern_tuple(lowered))
        }
    }

    /// Lower a literal type to its hash-consed literal id, including unary-minus
    /// numeric literals. Bigint/template/other unary literals are out of subset and
    /// abort the enclosing annotation.
    pub(super) fn lower_literal_type(&mut self, literal: &TSLiteral<'_>) -> Option<TypeId> {
        let value = match literal {
            TSLiteral::StringLiteral(s) => LiteralValue::String(s.value.to_string()),
            TSLiteral::NumericLiteral(n) => LiteralValue::Number(n.value),
            TSLiteral::BooleanLiteral(b) => LiteralValue::Boolean(b.value),
            // `-<numeric literal>` is a negative number literal type. tsc interns `-0`
            // and `0` to the same literal (SameValueZero), so collapse `-0.0` to `0.0`.
            TSLiteral::UnaryExpression(unary) if unary.operator == UnaryOperator::UnaryNegation => {
                let Expression::NumericLiteral(n) = &unary.argument else {
                    return None;
                };
                let negated = -n.value;
                LiteralValue::Number(if negated == 0.0 { 0.0 } else { negated })
            }
            // `bigint`, template-literal types, and other unary literal types (`+1`,
            // `-1n`, `~1`) are out of the M8 subset.
            TSLiteral::BigIntLiteral(_)
            | TSLiteral::TemplateLiteral(_)
            | TSLiteral::UnaryExpression(_) => return None,
        };
        Some(self.interner.intern_literal(value))
    }

    /// Lower object type literal members to a structural object. Optional members
    /// intern `T | undefined` here; string/number indexes coexist with named
    /// properties; unsupported or unlowerable members abort the whole object.
    pub(super) fn lower_object_annotation(
        &mut self,
        scope: ScopeId,
        members: &[TSSignature<'_>],
    ) -> Option<TypeId> {
        let mut object = ObjectType::default();
        let overloaded_method_names = self.overloaded_method_names(members);
        let call_signatures_overloaded = self.call_signatures_overloaded(members);
        let construct_signatures_overloaded = self.construct_signatures_overloaded(members);
        for member in members {
            match member {
                TSSignature::TSPropertySignature(sig) => {
                    let name = sig.key.static_name()?;
                    if overloaded_method_names.contains(name.as_ref()) {
                        continue;
                    }
                    let annotation = sig.type_annotation.as_ref()?;
                    // B29: an object member is a legal-recursion boundary (`type W = { a: W
                    // } | null`), so lower it at a deeper indirection level.
                    let ty = self.with_indirection(|p| {
                        p.lower_annotation(scope, &annotation.type_annotation)
                    })?;
                    // M21: optional properties intern `T | undefined` here, matching
                    // interface members and keeping this out of the relation engine.
                    let ty = if sig.optional {
                        let undefined = self.interner.well_known().undefined;
                        self.interner.union(vec![ty, undefined])
                    } else {
                        ty
                    };
                    let mut prop = PropertyType::public(name.into_owned(), ty);
                    prop.optional = sig.optional;
                    // F5/backlog-03: `readonly` is structural identity and gates
                    // assignment targets (`TK2540`), but does not affect assignability.
                    prop.readonly = sig.readonly;
                    object.properties.push(prop);
                }
                // M19: an index signature `[k: string]: T` / `[i: number]: T`.
                TSSignature::TSIndexSignature(sig) => {
                    self.lower_index_signature(scope, sig, &mut object)?;
                }
                TSSignature::TSMethodSignature(sig) => {
                    let name = sig.key.static_name()?;
                    if overloaded_method_names.contains(name.as_ref()) {
                        return None;
                    }
                    let prop = self.lower_method_signature_property(scope, sig)?;
                    object.properties.push(prop);
                }
                TSSignature::TSCallSignatureDeclaration(sig) => {
                    if call_signatures_overloaded {
                        return None;
                    }
                    let signature = self.lower_call_signature(scope, sig)?;
                    object.call_signatures.push(signature);
                }
                TSSignature::TSConstructSignatureDeclaration(sig) => {
                    if construct_signatures_overloaded {
                        return None;
                    }
                    let signature = self.lower_construct_signature(scope, sig)?;
                    object.construct_signatures.push(signature);
                }
            }
        }
        Some(self.interner.intern_object(object))
    }

    /// Lower a function type annotation to an interned function. Missing or
    /// unlowerable parameters abort rather than silently mis-stating the signature.
    pub(super) fn lower_function_annotation(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: &TSType<'_>,
    ) -> Option<TypeId> {
        let lowered = self.lower_strict_signature_parameters(scope, params, true)?;
        let ret = self.with_indirection(|p| p.lower_annotation(scope, return_type))?;
        Some(self.interner.intern_function(FunctionType {
            params: lowered,
            ret,
        }))
    }

    pub(super) fn lower_readonly_array_or_tuple(
        &mut self,
        scope: ScopeId,
        operand: &TSType<'_>,
    ) -> Option<TypeId> {
        match operand {
            TSType::TSArrayType(array) => {
                let element =
                    self.with_indirection(|p| p.lower_annotation(scope, &array.element_type))?;
                Some(self.intern_readonly_array(element))
            }
            TSType::TSTupleType(tuple) => {
                let tuple = self.lower_tuple_annotation(scope, &tuple.element_types)?;
                Some(self.interner.intern_readonly(tuple))
            }
            _ => None,
        }
    }

    fn intern_readonly_array(&mut self, element: TypeId) -> TypeId {
        let array = self.interner.intern_array(element);
        self.interner.intern_readonly(array)
    }
}
