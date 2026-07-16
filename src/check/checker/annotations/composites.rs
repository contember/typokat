use super::super::decls::alloc_type_param_ids;
use super::*;
use oxc_ast::ast::TSTypeParameterDeclaration;

#[derive(Default)]
struct MethodOverloadAccumulator {
    call_signatures: Vec<TypeId>,
    unsupported: bool,
    unavailable: bool,
}

enum LoweredTupleElement {
    Fixed(TypeId),
    Rest(TypeId),
}

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower `A | B | …` to a canonical interned union. Any unlowerable member
    /// aborts the whole annotation; dropping it would mis-state the union.
    pub(super) fn lower_union_annotation(
        &mut self,
        scope: ScopeId,
        members: &[TSType<'_>],
    ) -> Option<TypeId> {
        let mut lowered: Vec<TypeId> = Vec::with_capacity(members.len());
        let mut unavailable = false;
        for member in members {
            match self.lower_annotation(scope, member) {
                Some(member) => lowered.push(member),
                None => unavailable = true,
            }
        }
        if unavailable {
            return None;
        }
        Some(self.interner.union(lowered))
    }

    /// Lower `A & B & …` to a canonical interned intersection. A contextual
    /// `ThisType<T>` marker retains only the first syntactic occurrence before
    /// canonical interning erases member order; later markers are intentionally
    /// transparent in the bounded B70 model.
    pub(super) fn lower_intersection_annotation(
        &mut self,
        scope: ScopeId,
        members: &[TSType<'_>],
    ) -> Option<TypeId> {
        let mut structural: Vec<TypeId> = Vec::with_capacity(members.len());
        let mut this_type = None;
        let mut unavailable = false;
        for member in members {
            match self.lower_annotation(scope, member) {
                Some(lowered_member) => self.extract_contextual_this_members(
                    lowered_member,
                    &mut structural,
                    &mut this_type,
                ),
                None => unavailable = true,
            }
        }
        if unavailable {
            return None;
        }
        if let Some(this_type) = this_type {
            structural.push(this_type);
        }
        Some(self.interner.intersection(structural))
    }

    /// Flatten only contextual-intersection operands before canonical interning so
    /// the first syntactic `ThisType` survives aliases and nested intersections.
    fn extract_contextual_this_members(
        &self,
        ty: TypeId,
        structural: &mut Vec<TypeId>,
        this_type: &mut Option<TypeId>,
    ) {
        if self
            .interner
            .well_known()
            .this_type_operand(self.interner.store(), ty)
            .is_some()
        {
            if this_type.is_none() {
                *this_type = Some(ty);
            }
            return;
        }
        if let Some(members) = self.interner.store().intersection_members(ty) {
            for &member in members {
                self.extract_contextual_this_members(member, structural, this_type);
            }
            return;
        }
        structural.push(ty);
    }

    /// Lower `[A, B, …]` to an ordered interned tuple; `[]` is the empty tuple.
    /// Labels are erased while optional tuple members abort the annotation.
    pub(super) fn lower_tuple_annotation(
        &mut self,
        scope: ScopeId,
        elements: &[TSTupleElement<'_>],
    ) -> Option<TypeId> {
        let mut lowered: Vec<TypeId> = Vec::with_capacity(elements.len());
        let mut rest: Option<TupleRestType> = None;
        let mut seen_rest = false;
        let mut unavailable = false;
        let mut recorded_optional = false;
        for element in elements {
            match self.lower_tuple_element_annotation(scope, element, &mut recorded_optional) {
                Some(LoweredTupleElement::Rest(ty)) => {
                    if seen_rest {
                        unavailable = true;
                    }
                    seen_rest = true;
                    if rest.is_none() {
                        rest = Some(TupleRestType::new(lowered.len(), ty));
                    }
                }
                Some(LoweredTupleElement::Fixed(ty)) => lowered.push(ty),
                None => unavailable = true,
            }
        }
        if unavailable {
            return None;
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

    fn lower_tuple_element_annotation(
        &mut self,
        scope: ScopeId,
        element: &TSTupleElement<'_>,
        recorded_optional: &mut bool,
    ) -> Option<LoweredTupleElement> {
        match element {
            TSTupleElement::TSRestType(rest) => self
                .lower_tuple_rest_annotation(
                    scope,
                    rest.span,
                    &rest.type_annotation,
                    recorded_optional,
                )
                .map(LoweredTupleElement::Rest),
            TSTupleElement::TSOptionalType(optional) => {
                self.record_optional_tuple_element(optional.span, recorded_optional);
                self.with_indirection(|p| p.lower_annotation(scope, &optional.type_annotation));
                None
            }
            TSTupleElement::TSNamedTupleMember(named) => {
                if named.optional {
                    self.record_optional_tuple_element(named.span, recorded_optional);
                    self.visit_tuple_element_annotation(scope, &named.element_type);
                    None
                } else {
                    self.lower_tuple_element_annotation(
                        scope,
                        &named.element_type,
                        recorded_optional,
                    )
                }
            }
            _ => element.as_ts_type().and_then(|ty| {
                self.with_indirection(|p| p.lower_annotation(scope, ty))
                    .map(LoweredTupleElement::Fixed)
            }),
        }
    }

    fn lower_tuple_rest_annotation(
        &mut self,
        scope: ScopeId,
        span: oxc_span::Span,
        annotation: &TSType<'_>,
        recorded_optional: &mut bool,
    ) -> Option<TypeId> {
        let infer = Self::tuple_rest_infer_declaration(annotation);
        let lowered = if let TSType::TSNamedTupleMember(named) = annotation {
            if named.optional {
                self.record_optional_tuple_element(named.span, recorded_optional);
                self.visit_tuple_element_annotation(scope, &named.element_type);
                return None;
            }
            self.lower_tuple_element_annotation(scope, &named.element_type, recorded_optional)
                .map(|element| match element {
                    LoweredTupleElement::Fixed(ty) | LoweredTupleElement::Rest(ty) => ty,
                })
        } else {
            self.with_indirection(|p| p.lower_annotation(scope, annotation))
        };
        if infer.is_some_and(|infer| infer.type_parameter.constraint.is_some()) {
            return None;
        }
        if let Some(lowered) = lowered {
            let container_is_array_like =
                infer.is_some() || self.tuple_rest_container_is_array_like(lowered);
            if container_is_array_like {
                return Some(lowered);
            }
        }
        self.record_incomplete(
            "annotation-lower/tuple-rest-element/non-array",
            Span::from_oxc(span),
            "tuple rest element is not provably array-like",
        );
        None
    }

    fn tuple_rest_infer_declaration<'node, 'source>(
        annotation: &'node TSType<'source>,
    ) -> Option<&'node TSInferType<'source>> {
        match annotation {
            TSType::TSInferType(infer) => Some(infer),
            TSType::TSParenthesizedType(parenthesized) => {
                Self::tuple_rest_infer_declaration(&parenthesized.type_annotation)
            }
            TSType::TSNamedTupleMember(named) => {
                Self::tuple_element_infer_declaration(&named.element_type)
            }
            _ => None,
        }
    }

    fn tuple_element_infer_declaration<'node, 'source>(
        element: &'node TSTupleElement<'source>,
    ) -> Option<&'node TSInferType<'source>> {
        match element {
            TSTupleElement::TSNamedTupleMember(named) => {
                Self::tuple_element_infer_declaration(&named.element_type)
            }
            _ => element
                .as_ts_type()
                .and_then(Self::tuple_rest_infer_declaration),
        }
    }

    fn tuple_rest_container_is_array_like(&self, ty: TypeId) -> bool {
        let store = self.interner.store();
        let mut current = ty;
        let mut seen = FxHashSet::default();
        loop {
            match store.tag(current) {
                TypeTag::Array | TypeTag::Tuple => return true,
                TypeTag::Readonly => {
                    let Some(operand) = store.readonly_operand(current) else {
                        return false;
                    };
                    current = operand;
                }
                TypeTag::TypeParam => {
                    let Some(parameter) = store.type_param(current) else {
                        return false;
                    };
                    if !seen.insert(parameter.id) {
                        return false;
                    }
                    let Some(constraint) = store.type_param_constraint(parameter.id) else {
                        return false;
                    };
                    current = constraint;
                }
                _ => return false,
            }
        }
    }

    fn record_optional_tuple_element(&mut self, span: oxc_span::Span, recorded: &mut bool) {
        if *recorded {
            return;
        }
        self.record_incomplete(
            "annotation-lower/tuple-optional-element/self",
            Span::from_oxc(span),
            "optional tuple element aborts tuple lowering",
        );
        *recorded = true;
    }

    fn visit_tuple_element_annotation(&mut self, scope: ScopeId, element: &TSTupleElement<'_>) {
        match element {
            TSTupleElement::TSRestType(rest) => {
                self.with_indirection(|p| p.lower_annotation(scope, &rest.type_annotation));
            }
            TSTupleElement::TSOptionalType(optional) => {
                self.with_indirection(|p| p.lower_annotation(scope, &optional.type_annotation));
            }
            TSTupleElement::TSNamedTupleMember(named) => {
                self.visit_tuple_element_annotation(scope, &named.element_type);
            }
            _ => {
                if let Some(ty) = element.as_ts_type() {
                    self.with_indirection(|p| p.lower_annotation(scope, ty));
                }
            }
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
            // `bigint` and template-literal types are out of the M8 subset (WU5
            // accounting); other unary literal types (`+1`, `~1`) have no distinct id.
            TSLiteral::BigIntLiteral(lit) => {
                self.record_incomplete(
                    "annotation-lower/literal-type/bigint",
                    Span::from_oxc(lit.span),
                    "bigint literal type aborts annotation lowering",
                );
                return None;
            }
            TSLiteral::TemplateLiteral(lit) => {
                self.record_incomplete(
                    "annotation-lower/literal-type/template",
                    Span::from_oxc(lit.span),
                    "template literal in TSLiteral position aborts lowering",
                );
                return None;
            }
            TSLiteral::UnaryExpression(_) => return None,
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
        let mut overloads: FxHashMap<String, MethodOverloadAccumulator> = FxHashMap::default();
        let mut overload_order = Vec::new();
        let mut unavailable = false;
        for member in members {
            match member {
                TSSignature::TSPropertySignature(sig) => {
                    if sig.computed {
                        self.record_property_signature_computed_key(&sig.key);
                        if let Some(annotation) = sig.type_annotation.as_ref() {
                            self.with_indirection(|p| {
                                p.lower_annotation(scope, &annotation.type_annotation)
                            });
                        }
                        unavailable = true;
                        continue;
                    }
                    let Some(name) = sig.key.static_name() else {
                        self.record_property_signature_computed_key(&sig.key);
                        if let Some(annotation) = sig.type_annotation.as_ref() {
                            self.with_indirection(|p| {
                                p.lower_annotation(scope, &annotation.type_annotation)
                            });
                        }
                        unavailable = true;
                        continue;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        let name = name.into_owned();
                        if !overloads.contains_key(&name) {
                            overload_order.push(name.clone());
                        }
                        let overload = overloads.entry(name).or_default();
                        overload.unsupported = true;
                        let lowered = sig.type_annotation.as_ref().and_then(|annotation| {
                            self.with_indirection(|p| {
                                p.lower_annotation(scope, &annotation.type_annotation)
                            })
                        });
                        if lowered.is_none() {
                            overload.unavailable = true;
                        }
                        continue;
                    }
                    let Some(annotation) = sig.type_annotation.as_ref() else {
                        unavailable = true;
                        continue;
                    };
                    // B29: an object member is a legal-recursion boundary (`type W = { a: W
                    // } | null`), so lower it at a deeper indirection level.
                    let ty = self.with_indirection(|p| {
                        p.lower_annotation(scope, &annotation.type_annotation)
                    });
                    let Some(ty) = ty else {
                        unavailable = true;
                        continue;
                    };
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
                    if self
                        .lower_index_signature(scope, sig, &mut object)
                        .is_none()
                    {
                        unavailable = true;
                    }
                }
                TSSignature::TSMethodSignature(sig) => {
                    if sig.computed {
                        self.record_method_signature_computed_key(&sig.key);
                        self.lower_generic_strict_signature_function_type(
                            scope,
                            sig.type_parameters.as_deref(),
                            sig.this_param.as_deref(),
                            &sig.params,
                            sig.return_type.as_deref(),
                        );
                        unavailable = true;
                        continue;
                    }
                    let Some(name) = sig.key.static_name() else {
                        self.record_method_signature_computed_key(&sig.key);
                        self.lower_generic_strict_signature_function_type(
                            scope,
                            sig.type_parameters.as_deref(),
                            sig.this_param.as_deref(),
                            &sig.params,
                            sig.return_type.as_deref(),
                        );
                        unavailable = true;
                        continue;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        let name = name.into_owned();
                        if !overloads.contains_key(&name) {
                            overload_order.push(name.clone());
                        }
                        let overload = overloads.entry(name).or_default();
                        let signature = self.lower_generic_strict_signature_function_type(
                            scope,
                            sig.type_parameters.as_deref(),
                            sig.this_param.as_deref(),
                            &sig.params,
                            sig.return_type.as_deref(),
                        );
                        if sig.kind != TSMethodSignatureKind::Method || sig.optional {
                            overload.unsupported = true;
                        }
                        match signature {
                            Some(signature)
                                if sig.kind == TSMethodSignatureKind::Method && !sig.optional =>
                            {
                                overload.call_signatures.push(signature);
                            }
                            Some(_) => {}
                            None => overload.unavailable = true,
                        }
                        continue;
                    }
                    match self.lower_method_signature_property(scope, sig) {
                        Some(prop) => object.properties.push(prop),
                        None => unavailable = true,
                    }
                }
                TSSignature::TSCallSignatureDeclaration(sig) => {
                    match self.lower_call_signature(scope, sig) {
                        Some(signature) => object.call_signatures.push(signature),
                        None => unavailable = true,
                    }
                }
                TSSignature::TSConstructSignatureDeclaration(sig) => {
                    match self.lower_construct_signature(scope, sig) {
                        Some(signature) => object.construct_signatures.push(signature),
                        None => unavailable = true,
                    }
                }
            }
        }
        for name in overload_order {
            let overload = overloads
                .remove(&name)
                .expect("every overload name retains its accumulator");
            if overload.unavailable || overload.call_signatures.is_empty() {
                unavailable = true;
                continue;
            }
            let ty = if overload.unsupported {
                self.interner.well_known().never
            } else {
                self.interner.intern_object(ObjectType {
                    call_signatures: overload.call_signatures,
                    ..Default::default()
                })
            };
            object.properties.push(PropertyType::public(name, ty));
        }
        if unavailable {
            return None;
        }
        Some(self.interner.intern_object(object))
    }

    /// Lower a function type annotation to an interned function. Its binders own
    /// a nested frame, so generic annotation parameters cannot capture or leak.
    pub(super) fn lower_generic_function_annotation(
        &mut self,
        scope: ScopeId,
        type_parameter_decl: Option<&TSTypeParameterDeclaration<'_>>,
        this_param: Option<&TSThisParameter<'_>>,
        params: &FormalParameters<'_>,
        return_type: &TSType<'_>,
    ) -> Option<TypeId> {
        let ids = alloc_type_param_ids(type_parameter_decl, &mut self.next_type_param);
        let frame = self.build_type_param_frame(type_parameter_decl, &ids);
        self.with_type_params(frame, |pass| {
            let type_params = pass.lower_signature_type_params(scope, type_parameter_decl, &ids);
            let receiver = pass.lower_this_parameter(scope, this_param);
            let params = pass.lower_strict_signature_parameters(scope, params, true);
            let ret = pass.with_indirection(|p| p.lower_annotation(scope, return_type));
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
