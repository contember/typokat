//! Closed, syntax-only inference for unannotated instance fields.

use crate::class_semantics::DemandOutcome;
use crate::types::repr::{
    FunctionType, LiteralValue, ObjectType, ParameterType, PropertyType, TupleType, TypeTag,
};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    ArrayExpressionElement, BindingPattern, Expression, ObjectPropertyKind, PropertyKind, TSType,
    UnaryOperator,
};
use rustc_hash::FxHashMap;

use super::surface_types::SurfaceTypeFactory;
use crate::check::checker::context::{InitializerValidationBoundary, InitializerValidationRows};
use crate::check::checker::expr::{
    const_assertion_operand_kind, contextual_literal_target, contextual_shape_members,
    is_const_assertion_type, object_element_access, satisfies_arrow_needs_context,
    ConstAssertionOperandKind, ElementAccessLookup,
};
use crate::check::checker::literal_publication::LiteralPublicationRole;
use crate::span::Span;
use oxc_span::GetSpan;

#[cfg(test)]
#[path = "../tests/literal_publication.rs"]
mod literal_publication_tests;

#[cfg(test)]
#[path = "../tests/publication_role_repair.rs"]
mod publication_role_repair_tests;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum InitializerInference {
    Inferred(TypeId),
    Unsupported,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct EarlierMethodSurface {
    pub function: TypeId,
    pub signatures: usize,
}

pub(in crate::check::checker) trait SurfaceInitializerContext {
    fn lower_annotation(&mut self, annotation: &TSType<'_>) -> DemandOutcome<TypeId>;
    fn earlier_field(&self, name: &str) -> Option<TypeId>;
    fn earlier_method(&self, name: &str) -> Option<EarlierMethodSurface>;
    fn lexical_value(&self, reference_start: u32, name: &str) -> Option<TypeId>;
}

pub(in crate::check::checker) struct SurfaceInitializerInferer<'a, 'b, C> {
    factory: &'a mut SurfaceTypeFactory<'b>,
    context: &'a mut C,
    locals: Vec<FxHashMap<String, TypeId>>,
    validation: Option<InitializerValidationRows>,
}

impl<'a, 'b, C: SurfaceInitializerContext> SurfaceInitializerInferer<'a, 'b, C> {
    pub(in crate::check::checker) fn new(
        factory: &'a mut SurfaceTypeFactory<'b>,
        context: &'a mut C,
    ) -> Self {
        SurfaceInitializerInferer {
            factory,
            context,
            locals: Vec::new(),
            validation: Some(InitializerValidationRows::default()),
        }
    }

    pub(in crate::check::checker) fn infer(
        &mut self,
        expression: &Expression<'_>,
        readonly: bool,
        is_static: bool,
    ) -> InitializerInference {
        if is_static {
            self.validation = None;
            return InitializerInference::Unsupported;
        }
        let inferred = self.infer_expression(expression, readonly).map_or(
            InitializerInference::Unsupported,
            InitializerInference::Inferred,
        );
        if inferred == InitializerInference::Unsupported {
            self.validation = None;
        }
        inferred
    }

    pub(in crate::check::checker) fn into_validation_sources(
        self,
    ) -> Option<InitializerValidationRows> {
        self.validation
    }

    fn retain_validation_source(
        &mut self,
        expression: &Expression<'_>,
        boundary: InitializerValidationBoundary,
        source: TypeId,
    ) {
        if let Some(rows) = &mut self.validation {
            rows.insert(Span::from_oxc(expression.span()), boundary, source);
        }
    }

    fn infer_expression(&mut self, expression: &Expression<'_>, readonly: bool) -> Option<TypeId> {
        let role = if readonly {
            LiteralPublicationRole::ModeRetaining
        } else {
            LiteralPublicationRole::MutableMember
        };
        self.infer_expression_with_context(expression, role, None, false)
    }

    fn infer_expression_with_context(
        &mut self,
        expression: &Expression<'_>,
        role: LiteralPublicationRole,
        context: Option<TypeId>,
        const_context: bool,
    ) -> Option<TypeId> {
        let wk = self.factory.well_known();
        let context =
            context.map(|context| contextual_literal_target(self.factory.store(), context));
        let ty = match expression {
            Expression::BooleanLiteral(literal) => Some(
                self.factory
                    .intern_ordinary_literal(LiteralValue::Boolean(literal.value)),
            ),
            Expression::NullLiteral(_) => Some(wk.null),
            Expression::NumericLiteral(literal) => Some(
                self.factory
                    .intern_ordinary_literal(LiteralValue::Number(literal.value)),
            ),
            Expression::StringLiteral(literal) => Some(
                self.factory
                    .intern_ordinary_literal(LiteralValue::String(literal.value.to_string())),
            ),
            Expression::ParenthesizedExpression(parenthesized) => self
                .infer_expression_with_context(
                    &parenthesized.expression,
                    role,
                    context,
                    const_context,
                ),
            Expression::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation
                    && matches!(unary.argument, Expression::NumericLiteral(_)) =>
            {
                let Expression::NumericLiteral(literal) = &unary.argument else {
                    return None;
                };
                Some(
                    self.factory
                        .intern_ordinary_literal(LiteralValue::Number(-literal.value)),
                )
            }
            Expression::TSAsExpression(assertion) => {
                self.infer_assertion(&assertion.expression, &assertion.type_annotation)
            }
            Expression::TSTypeAssertion(assertion) => {
                self.infer_assertion(&assertion.expression, &assertion.type_annotation)
            }
            Expression::TSSatisfiesExpression(satisfies) => {
                let operand =
                    self.infer_satisfies(&satisfies.expression, &satisfies.type_annotation)?;
                self.retain_validation_source(
                    expression,
                    InitializerValidationBoundary::SatisfiesOperand,
                    operand,
                );
                Some(operand)
            }
            Expression::ArrayExpression(array) => {
                if const_context {
                    let mut elements = Vec::with_capacity(array.elements.len());
                    for element in &array.elements {
                        let expression = match element {
                            ArrayExpressionElement::SpreadElement(_)
                            | ArrayExpressionElement::Elision(_) => return None,
                            _ => element.as_expression()?,
                        };
                        elements.push(self.infer_expression_with_context(
                            expression,
                            LiteralPublicationRole::ConstOwned,
                            None,
                            true,
                        )?);
                    }
                    let tuple = self.factory.intern_tuple(TupleType::fixed(elements));
                    return Some(self.factory.intern_readonly(tuple));
                }
                let tuple_contexts = context.and_then(|context| {
                    contextual_shape_members(self.factory.store(), context, TypeTag::Tuple)
                });
                let array_contexts = context.and_then(|context| {
                    contextual_shape_members(self.factory.store(), context, TypeTag::Array)
                });
                let mut elements = Vec::with_capacity(array.elements.len());
                for (index, element) in array.elements.iter().enumerate() {
                    let expression = match element {
                        ArrayExpressionElement::SpreadElement(_)
                        | ArrayExpressionElement::Elision(_) => return None,
                        _ => element.as_expression()?,
                    };
                    let element_context = if let Some(contexts) = &tuple_contexts {
                        let candidates = contexts
                            .iter()
                            .filter_map(|context| {
                                self.factory
                                    .store()
                                    .tuple_type(*context)
                                    .and_then(|tuple| tuple.elements.get(index))
                                    .copied()
                            })
                            .collect();
                        self.union_context(candidates)
                    } else if let Some(contexts) = &array_contexts {
                        let candidates = contexts
                            .iter()
                            .filter_map(|context| {
                                self.factory
                                    .store()
                                    .array_type(*context)
                                    .map(|array| array.element)
                            })
                            .collect();
                        self.union_context(candidates)
                    } else {
                        None
                    };
                    let contribution = self.infer_expression_with_context(
                        expression,
                        LiteralPublicationRole::MutableMember,
                        element_context,
                        false,
                    )?;
                    self.retain_validation_source(
                        expression,
                        InitializerValidationBoundary::Contribution,
                        contribution,
                    );
                    elements.push(contribution);
                }
                if tuple_contexts.is_some() {
                    Some(self.factory.intern_tuple(TupleType::fixed(elements)))
                } else {
                    let element = self.factory.union(elements);
                    Some(self.factory.intern_array(element))
                }
            }
            Expression::ObjectExpression(object) => {
                if const_context {
                    let mut properties = Vec::with_capacity(object.properties.len());
                    for property in &object.properties {
                        let ObjectPropertyKind::ObjectProperty(property) = property else {
                            return None;
                        };
                        if property.computed
                            || property.method
                            || property.kind != PropertyKind::Init
                        {
                            return None;
                        }
                        let name = property.key.static_name()?.into_owned();
                        let ty = self.infer_expression_with_context(
                            &property.value,
                            LiteralPublicationRole::ConstOwned,
                            None,
                            true,
                        )?;
                        let mut property = PropertyType::public(name, ty);
                        property.readonly = true;
                        properties.push(property);
                    }
                    return Some(self.factory.intern_object(ObjectType {
                        properties,
                        ..Default::default()
                    }));
                }
                let mut properties = Vec::with_capacity(object.properties.len());
                for property in &object.properties {
                    let ObjectPropertyKind::ObjectProperty(property) = property else {
                        return None;
                    };
                    if property.computed
                        || property.method
                        || property.shorthand
                        || property.kind != PropertyKind::Init
                    {
                        return None;
                    }
                    let name = property.key.static_name()?.into_owned();
                    let property_context =
                        context.and_then(|context| self.object_property_context(context, &name));
                    let ty = self.infer_expression_with_context(
                        &property.value,
                        LiteralPublicationRole::MutableMember,
                        property_context,
                        false,
                    )?;
                    self.retain_validation_source(
                        &property.value,
                        InitializerValidationBoundary::Contribution,
                        ty,
                    );
                    properties.push(PropertyType::public(name, ty));
                }
                Some(self.factory.intern_object(ObjectType {
                    properties,
                    ..Default::default()
                }))
            }
            Expression::StaticMemberExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) && !member.optional =>
            {
                self.context.earlier_field(member.property.name.as_str())
            }
            Expression::CallExpression(call)
                if call.arguments.is_empty()
                    && call.type_arguments.is_none()
                    && !call.optional
                    && matches!(
                        call.callee,
                        Expression::StaticMemberExpression(ref member)
                            if matches!(member.object, Expression::ThisExpression(_))
                                && !member.optional
                    ) =>
            {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return None;
                };
                let method = self.context.earlier_method(member.property.name.as_str())?;
                if method.signatures != 1 {
                    return None;
                }
                let function = self.factory.store().function_type(method.function)?;
                (function.type_params.is_empty() && function.params.is_empty())
                    .then_some(function.ret)
            }
            Expression::ArrowFunctionExpression(arrow)
                if arrow.expression && !arrow.r#async && arrow.type_parameters.is_none() =>
            {
                if arrow.params.rest.is_some() {
                    return None;
                }
                let target = context
                    .and_then(|context| self.factory.store().function_type(context).cloned());
                let contextual = target
                    .as_ref()
                    .filter(|_| satisfies_arrow_needs_context(arrow));
                let declared_return = match arrow.return_type.as_ref() {
                    Some(annotation) => {
                        let DemandOutcome::Ready(ty) =
                            self.context.lower_annotation(&annotation.type_annotation)
                        else {
                            return None;
                        };
                        Some(ty)
                    }
                    None => None,
                };
                let mut frame = FxHashMap::default();
                let mut parameters = Vec::with_capacity(arrow.params.items.len());
                for (index, parameter) in arrow.params.items.iter().enumerate() {
                    if parameter.initializer.is_some() {
                        return None;
                    }
                    let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern else {
                        return None;
                    };
                    let ty = match parameter.type_annotation.as_ref() {
                        Some(annotation) => {
                            let DemandOutcome::Ready(ty) =
                                self.context.lower_annotation(&annotation.type_annotation)
                            else {
                                return None;
                            };
                            ty
                        }
                        None => self.factory.ready_contextual_parameter_target(
                            &contextual.as_ref()?.params,
                            index,
                            arrow.params.items.len(),
                        )?,
                    };
                    frame.insert(identifier.name.to_string(), ty);
                    parameters.push(if parameter.optional {
                        ParameterType::optional(identifier.name.to_string(), ty)
                    } else {
                        ParameterType::required(identifier.name.to_string(), ty)
                    });
                }
                self.locals.push(frame);
                let body = arrow.get_expression()?;
                let body_context =
                    declared_return.or_else(|| target.as_ref().map(|target| target.ret));
                let declared_validation = declared_return.and_then(|_| self.validation.take());
                let ret = self.infer_expression_with_context(
                    body,
                    LiteralPublicationRole::ModeRetaining,
                    body_context,
                    false,
                );
                if declared_return.is_some() {
                    self.validation = declared_validation;
                } else if let Some(source) = ret {
                    self.retain_validation_source(
                        body,
                        InitializerValidationBoundary::InferredArrowBody,
                        source,
                    );
                }
                let ret = ret.map(|source| {
                    self.factory.finalize_literal_type(
                        source,
                        body_context.map(|context| {
                            contextual_literal_target(self.factory.store(), context)
                        }),
                        LiteralPublicationRole::InferredReturn,
                    )
                });
                self.locals.pop();
                Some(self.factory.intern_function(FunctionType {
                    type_params: Vec::new(),
                    receiver: None,
                    params: parameters,
                    ret: declared_return.or(ret)?,
                }))
            }
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
                if const_context =>
            {
                self.infer_const_member_reference(expression)
            }
            Expression::Identifier(identifier) => self
                .locals
                .iter()
                .rev()
                .find_map(|frame| frame.get(identifier.name.as_str()).copied())
                .or_else(|| {
                    self.context
                        .lexical_value(identifier.span.start, identifier.name.as_str())
                }),

            // Explicitly unsupported. Keeping every oxc variant in this match makes
            // an AST upgrade a compile-time classification failure.
            Expression::BigIntLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::MetaProperty(_)
            | Expression::Super(_)
            | Expression::AssignmentExpression(_)
            | Expression::AwaitExpression(_)
            | Expression::BinaryExpression(_)
            | Expression::CallExpression(_)
            | Expression::ChainExpression(_)
            | Expression::ClassExpression(_)
            | Expression::ConditionalExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::ArrowFunctionExpression(_)
            | Expression::ImportExpression(_)
            | Expression::LogicalExpression(_)
            | Expression::NewExpression(_)
            | Expression::SequenceExpression(_)
            | Expression::TaggedTemplateExpression(_)
            | Expression::ThisExpression(_)
            | Expression::UnaryExpression(_)
            | Expression::UpdateExpression(_)
            | Expression::YieldExpression(_)
            | Expression::PrivateInExpression(_)
            | Expression::JSXElement(_)
            | Expression::JSXFragment(_)
            | Expression::TSNonNullExpression(_)
            | Expression::TSInstantiationExpression(_)
            | Expression::V8IntrinsicExpression(_)
            | Expression::ComputedMemberExpression(_)
            | Expression::StaticMemberExpression(_)
            | Expression::PrivateFieldExpression(_) => None,
        }?;
        let published = self.factory.finalize_literal_type(ty, context, role);
        if matches!(
            expression,
            Expression::ObjectExpression(_)
                | Expression::ArrayExpression(_)
                | Expression::ArrowFunctionExpression(_)
        ) {
            self.retain_validation_source(
                expression,
                InitializerValidationBoundary::CompletedContainer,
                published,
            );
        }
        Some(published)
    }

    fn infer_assertion(
        &mut self,
        operand: &Expression<'_>,
        annotation: &TSType<'_>,
    ) -> Option<TypeId> {
        if is_const_assertion_type(annotation) {
            return match const_assertion_operand_kind(operand) {
                ConstAssertionOperandKind::Owned => self.infer_expression_with_context(
                    operand,
                    LiteralPublicationRole::ConstOwned,
                    None,
                    true,
                ),
                ConstAssertionOperandKind::EnumCandidate => {
                    self.infer_const_member_reference(operand)
                }
                ConstAssertionOperandKind::Invalid => None,
            };
        }
        self.infer_expression(operand, false)?;
        match self.context.lower_annotation(annotation) {
            DemandOutcome::Ready(ty) => Some(ty),
            DemandOutcome::Exhausted(_) => None,
        }
    }

    fn infer_const_member_reference(&mut self, operand: &Expression<'_>) -> Option<TypeId> {
        match operand {
            Expression::ParenthesizedExpression(parenthesized) => {
                self.infer_const_member_reference(&parenthesized.expression)
            }
            Expression::StaticMemberExpression(member) if !member.optional => {
                let base = self.infer_expression_with_context(
                    &member.object,
                    LiteralPublicationRole::ConstOwned,
                    None,
                    true,
                )?;
                self.factory
                    .store()
                    .object_type(base)?
                    .property(member.property.name.as_str())
                    .map(|property| property.ty)
            }
            Expression::ComputedMemberExpression(member) if !member.optional => {
                let base = self.infer_expression_with_context(
                    &member.object,
                    LiteralPublicationRole::ConstOwned,
                    None,
                    true,
                )?;
                if let Some(name) = member.static_property_name() {
                    if let Some(ty) = self
                        .factory
                        .store()
                        .object_type(base)
                        .and_then(|object| object.property(name.as_str()))
                        .map(|property| property.ty)
                    {
                        return Some(ty);
                    }
                }
                let key = self.infer_expression_with_context(
                    &member.expression,
                    LiteralPublicationRole::ConstOwned,
                    None,
                    true,
                )?;
                match object_element_access(self.factory.store(), base, Some(key)) {
                    ElementAccessLookup::Found(ty) => Some(ty),
                    ElementAccessLookup::MissingObjectKey
                    | ElementAccessLookup::NullishReceiver
                    | ElementAccessLookup::UnsupportedReceiver => None,
                }
            }
            _ => None,
        }
    }

    fn infer_satisfies(
        &mut self,
        operand: &Expression<'_>,
        annotation: &TSType<'_>,
    ) -> Option<TypeId> {
        let DemandOutcome::Ready(target) = self.context.lower_annotation(annotation) else {
            return None;
        };
        self.infer_expression_with_context(
            operand,
            LiteralPublicationRole::ModeRetaining,
            Some(target),
            false,
        )
    }

    fn object_property_context(&mut self, context: TypeId, name: &str) -> Option<TypeId> {
        let contexts = contextual_shape_members(self.factory.store(), context, TypeTag::Object)?;
        let candidates = contexts
            .iter()
            .filter_map(|context| {
                let target = self.factory.store().object_type(*context)?;
                target
                    .property(name)
                    .map(|property| property.ty)
                    .or(target.string_index)
            })
            .collect();
        self.union_context(candidates)
    }

    fn union_context(&mut self, candidates: Vec<TypeId>) -> Option<TypeId> {
        (!candidates.is_empty()).then(|| self.factory.union(candidates))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_semantics::Exhaustion;
    use crate::types::repr::{FunctionType, TypeTag};
    use crate::types::Interner;
    use oxc_allocator::Allocator;
    use oxc_ast::ast::{ClassElement, Statement, TSLiteral};
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    #[derive(Default)]
    struct Context {
        number: Option<TypeId>,
        string: Option<TypeId>,
        boolean: Option<TypeId>,
        object: Option<TypeId>,
        function: Option<TypeId>,
        string_literals: FxHashMap<String, TypeId>,
        fields: FxHashMap<String, TypeId>,
        methods: FxHashMap<String, EarlierMethodSurface>,
        lexical_values: FxHashMap<String, TypeId>,
    }

    impl SurfaceInitializerContext for Context {
        fn lower_annotation(&mut self, annotation: &TSType<'_>) -> DemandOutcome<TypeId> {
            match annotation {
                TSType::TSNumberKeyword(_) => self.number.map_or_else(
                    || DemandOutcome::Exhausted(Exhaustion::EvaluationBudget),
                    DemandOutcome::Ready,
                ),
                TSType::TSStringKeyword(_) => self.string.map_or_else(
                    || DemandOutcome::Exhausted(Exhaustion::EvaluationBudget),
                    DemandOutcome::Ready,
                ),
                TSType::TSBooleanKeyword(_) => self.boolean.map_or_else(
                    || DemandOutcome::Exhausted(Exhaustion::EvaluationBudget),
                    DemandOutcome::Ready,
                ),
                TSType::TSTypeLiteral(_) => self.object.map_or_else(
                    || DemandOutcome::Exhausted(Exhaustion::EvaluationBudget),
                    DemandOutcome::Ready,
                ),
                TSType::TSFunctionType(_) => self.function.map_or_else(
                    || DemandOutcome::Exhausted(Exhaustion::EvaluationBudget),
                    DemandOutcome::Ready,
                ),
                TSType::TSLiteralType(literal) => match &literal.literal {
                    TSLiteral::StringLiteral(literal) => self
                        .string_literals
                        .get(literal.value.as_str())
                        .copied()
                        .map_or_else(
                            || DemandOutcome::Exhausted(Exhaustion::EvaluationBudget),
                            DemandOutcome::Ready,
                        ),
                    _ => DemandOutcome::Exhausted(Exhaustion::EvaluationBudget),
                },
                _ => DemandOutcome::Exhausted(Exhaustion::EvaluationBudget),
            }
        }

        fn earlier_field(&self, name: &str) -> Option<TypeId> {
            self.fields.get(name).copied()
        }

        fn earlier_method(&self, name: &str) -> Option<EarlierMethodSurface> {
            self.methods.get(name).copied()
        }

        fn lexical_value(&self, _reference_start: u32, name: &str) -> Option<TypeId> {
            self.lexical_values.get(name).copied()
        }
    }

    fn with_initializer<R>(source: &str, check: impl FnOnce(&Expression<'_>) -> R) -> R {
        let allocator = Allocator::default();
        let source = format!("class C {{ field = {source}; }}");
        let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let Statement::ClassDeclaration(class) = &parsed.program.body[0] else {
            panic!("expected class declaration");
        };
        let ClassElement::PropertyDefinition(property) = &class.body.body[0] else {
            panic!("expected property definition");
        };
        check(property.value.as_ref().unwrap())
    }

    #[test]
    fn literals_follow_mutable_and_readonly_widening() {
        with_initializer("1", |expression| {
            let mut interner = Interner::with_intrinsics();
            let wk = interner.well_known();
            let mut context = Context {
                number: Some(wk.number),
                string: Some(wk.string),
                ..Default::default()
            };
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let mutable = SurfaceInitializerInferer::new(&mut factory, &mut context)
                .infer(expression, false, false);
            assert_eq!(mutable, InitializerInference::Inferred(wk.number));

            let readonly = SurfaceInitializerInferer::new(&mut factory, &mut context)
                .infer(expression, true, false);
            let InitializerInference::Inferred(readonly) = readonly else {
                panic!("readonly literal must be inferred");
            };
            assert_eq!(factory.store().tag(readonly), TypeTag::WideningLiteral);
            assert_eq!(
                factory.store().literal_value(readonly),
                Some(&LiteralValue::Number(1.0))
            );
        });
    }

    #[test]
    fn recursive_pure_shapes_assertions_and_arrows_are_supported() {
        for source in [
            "([1, \"x\"])",
            "({ a: 1, \"b\": true })",
            "(1 as number)",
            "((x: number) => x)",
        ] {
            with_initializer(source, |expression| {
                let mut interner = Interner::with_intrinsics();
                let wk = interner.well_known();
                let mut context = Context {
                    number: Some(wk.number),
                    string: Some(wk.string),
                    ..Default::default()
                };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                assert!(matches!(
                    SurfaceInitializerInferer::new(&mut factory, &mut context)
                        .infer(expression, false, false),
                    InitializerInference::Inferred(_)
                ));
            });
        }
    }

    #[test]
    fn this_reads_only_lexically_earlier_successful_surfaces() {
        with_initializer("this.before", |expression| {
            let mut interner = Interner::with_intrinsics();
            let wk = interner.well_known();
            let mut context = Context {
                fields: FxHashMap::from_iter([("before".to_string(), wk.number)]),
                ..Default::default()
            };
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            assert_eq!(
                SurfaceInitializerInferer::new(&mut factory, &mut context)
                    .infer(expression, false, false),
                InitializerInference::Inferred(wk.number)
            );
        });

        with_initializer("this.method()", |expression| {
            let mut interner = Interner::with_intrinsics();
            let wk = interner.well_known();
            let method = interner.intern_function(FunctionType {
                type_params: Vec::new(),
                receiver: None,
                params: Vec::new(),
                ret: wk.string,
            });
            let mut context = Context {
                methods: FxHashMap::from_iter([(
                    "method".to_string(),
                    EarlierMethodSurface {
                        function: method,
                        signatures: 1,
                    },
                )]),
                ..Default::default()
            };
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            assert_eq!(
                SurfaceInitializerInferer::new(&mut factory, &mut context)
                    .infer(expression, false, false),
                InitializerInference::Inferred(wk.string)
            );
        });
    }

    #[test]
    fn every_unannotated_static_initializer_is_unsupported() {
        with_initializer("1", |expression| {
            let mut interner = Interner::with_intrinsics();
            let mut context = Context::default();
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            assert_eq!(
                SurfaceInitializerInferer::new(&mut factory, &mut context)
                    .infer(expression, false, true),
                InitializerInference::Unsupported
            );
        });
    }

    #[test]
    fn satisfies_retains_the_mutable_source_type() {
        with_initializer("1 satisfies number", |expression| {
            let mut interner = Interner::with_intrinsics();
            let wk = interner.well_known();
            let mut context = Context {
                number: Some(wk.number),
                ..Default::default()
            };
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            assert_eq!(
                SurfaceInitializerInferer::new(&mut factory, &mut context)
                    .infer(expression, false, false),
                InitializerInference::Inferred(wk.number)
            );
        });
    }

    #[test]
    fn satisfies_class_surface_preserves_source_literal_provenance_recursively() {
        with_initializer(
            r#"({
                raw: "raw",
                count: 1,
                flag: true,
                typed: typed,
                asserted: "asserted" as "asserted",
                tuple: ["raw", typed],
                array: [typed],
                callback: (value) => "fixed",
            }) satisfies {
                raw: string;
                count: number;
                flag: boolean;
                typed: string;
                asserted: string;
                tuple: [string, string];
                array: string[];
                callback: (value: number) => "fixed";
            }"#,
            |expression| {
                let mut interner = Interner::with_intrinsics();
                let wk = interner.well_known();
                let typed = interner.intern_literal(LiteralValue::String("typed".to_owned()));
                let asserted = interner.intern_literal(LiteralValue::String("asserted".to_owned()));
                let fixed = interner.intern_literal(LiteralValue::String("fixed".to_owned()));
                let tuple_target = interner.intern_tuple(vec![wk.string, wk.string]);
                let array_target = interner.intern_array(wk.string);
                let callback_target = interner.intern_function(FunctionType {
                    type_params: Vec::new(),
                    receiver: None,
                    params: vec![ParameterType::required("value", wk.number)],
                    ret: fixed,
                });
                let target = interner.intern_object(ObjectType {
                    properties: vec![
                        PropertyType::public("raw", wk.string),
                        PropertyType::public("count", wk.number),
                        PropertyType::public("flag", wk.boolean),
                        PropertyType::public("typed", wk.string),
                        PropertyType::public("asserted", wk.string),
                        PropertyType::public("tuple", tuple_target),
                        PropertyType::public("array", array_target),
                        PropertyType::public("callback", callback_target),
                    ],
                    ..Default::default()
                });
                let mut context = Context {
                    number: Some(wk.number),
                    string: Some(wk.string),
                    boolean: Some(wk.boolean),
                    object: Some(target),
                    string_literals: FxHashMap::from_iter([("asserted".to_owned(), asserted)]),
                    lexical_values: FxHashMap::from_iter([("typed".to_owned(), typed)]),
                    ..Default::default()
                };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let InitializerInference::Inferred(source) =
                    SurfaceInitializerInferer::new(&mut factory, &mut context)
                        .infer(expression, false, false)
                else {
                    panic!("satisfies source must publish");
                };
                let source = factory.store().object_type(source).expect("object source");
                assert_eq!(
                    source.property("raw").map(|property| property.ty),
                    Some(wk.string)
                );
                assert_eq!(
                    source.property("count").map(|property| property.ty),
                    Some(wk.number)
                );

                let flag = source.property("flag").expect("flag").ty;
                assert_eq!(
                    factory.store().literal_value(flag),
                    Some(&LiteralValue::Boolean(true))
                );
                assert_eq!(
                    source.property("typed").map(|property| property.ty),
                    Some(typed)
                );
                let asserted_source = source.property("asserted").expect("asserted").ty;
                assert_eq!(
                    factory.store().literal_value(asserted_source),
                    Some(&LiteralValue::String("asserted".to_owned()))
                );

                let tuple = source.property("tuple").expect("tuple").ty;
                let tuple = factory.store().tuple_type(tuple).expect("tuple source");
                assert_eq!(tuple.elements, vec![wk.string, typed]);
                let array = source.property("array").expect("array").ty;
                assert_eq!(
                    factory.store().array_type(array).map(|array| array.element),
                    Some(typed)
                );
                let callback = source.property("callback").expect("callback").ty;
                let callback = factory
                    .store()
                    .function_type(callback)
                    .expect("callback source");
                assert_eq!(callback.ret, fixed);
            },
        );
    }

    #[test]
    fn explicit_arrow_return_annotation_is_the_class_surface_authority() {
        with_initializer(r#"((value: number): "declared" => "body")"#, |expression| {
            let mut interner = Interner::with_intrinsics();
            let wk = interner.well_known();
            let declared = interner.intern_literal(LiteralValue::String("declared".to_owned()));
            let target = interner.intern_function(FunctionType {
                type_params: Vec::new(),
                receiver: None,
                params: vec![ParameterType::required("value", wk.number)],
                ret: wk.string,
            });
            let mut context = Context {
                number: Some(wk.number),
                string_literals: FxHashMap::from_iter([("declared".to_owned(), declared)]),
                ..Default::default()
            };
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let source = SurfaceInitializerInferer::new(&mut factory, &mut context)
                .infer_expression_with_context(
                    expression,
                    LiteralPublicationRole::MutableMember,
                    Some(target),
                    false,
                )
                .expect("annotated arrow surface");
            let source = factory
                .store()
                .function_type(source)
                .expect("function surface");

            assert_eq!(source.ret, declared);
        });
    }

    #[test]
    fn unsupported_boundary_never_falls_through_to_inference() {
        for source in [
            "+1",
            "[,,]",
            "[...[]]",
            "({ x })",
            "({...{}})",
            "`x`",
            "1!",
            "(() => { return 1; })",
            "this.later",
            "this.method(1)",
        ] {
            with_initializer(source, |expression| {
                let mut interner = Interner::with_intrinsics();
                let wk = interner.well_known();
                let mut context = Context {
                    number: Some(wk.number),
                    methods: FxHashMap::from_iter([(
                        "method".to_string(),
                        EarlierMethodSurface {
                            function: wk.never,
                            signatures: 1,
                        },
                    )]),
                    ..Default::default()
                };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                assert_eq!(
                    SurfaceInitializerInferer::new(&mut factory, &mut context)
                        .infer(expression, false, false),
                    InitializerInference::Unsupported,
                    "{source}"
                );
            });
        }
    }

    #[test]
    fn validation_capture_retains_body_modes_without_changing_function_or_class_graph() {
        use crate::check::checker::classes::construction::{
            ClassConstruction, ClassRecoveryOrder, ClassSurfaceLowerer,
        };
        use crate::source::{ModuleOrdinal, SourceOrdinal};
        use crate::types::repr::ClassId;
        with_initializer(r#"(() => ("a")) satisfies () => string"#, |expression| {
            let mut interner = Interner::with_intrinsics();
            let wk = interner.well_known();
            let ordinary = interner.intern_ordinary_literal(LiteralValue::String("a".into()));
            let function = interner.intern_function(FunctionType {
                type_params: vec![],
                receiver: None,
                params: vec![],
                ret: wk.string,
            });
            let mut context = Context {
                function: Some(function),
                ..Default::default()
            };
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let mut inferer = SurfaceInitializerInferer::new(&mut factory, &mut context);
            assert_eq!(
                inferer.infer(expression, false, false),
                InitializerInference::Inferred(function)
            );
            let rows = inferer
                .into_validation_sources()
                .expect("successful source");
            let Expression::TSSatisfiesExpression(satisfies) = expression else {
                panic!("satisfies")
            };
            let Expression::ParenthesizedExpression(parenthesized) = &satisfies.expression else {
                panic!("parentheses")
            };
            let Expression::ArrowFunctionExpression(arrow) = &parenthesized.expression else {
                panic!("arrow")
            };
            let body = arrow.get_expression().unwrap();
            assert_eq!(
                rows.get(
                    Span::from_oxc(body.span()),
                    InitializerValidationBoundary::InferredArrowBody
                ),
                Some(ordinary)
            );
            assert_eq!(
                rows.get(
                    Span::from_oxc(arrow.span),
                    InitializerValidationBoundary::CompletedContainer
                ),
                Some(function)
            );
            assert_eq!(
                rows.get(
                    Span::from_oxc(expression.span()),
                    InitializerValidationBoundary::SatisfiesOperand
                ),
                Some(function)
            );
            assert_ne!(ordinary, wk.string);
            let template = interner.intern_object(ObjectType {
                properties: vec![PropertyType::public("field", function)],
                ..Default::default()
            });
            let publish = |interner: &mut Interner| {
                let lowerer: ClassSurfaceLowerer<()> = ClassSurfaceLowerer::new(
                    ClassId(0),
                    ClassRecoveryOrder {
                        source: SourceOrdinal::User(ModuleOrdinal::new(0)),
                        binding_start: 0,
                        declaration_ordinal: 0,
                    },
                    vec![],
                    template,
                    template,
                    None,
                );
                let mut construction = ClassConstruction::default();
                construction.register(lowerer.finish()).unwrap();
                construction.finish(interner).unwrap()
            };
            let before = publish(&mut interner);
            drop(rows);
            let after = publish(&mut interner);
            assert_eq!(before.graph, after.graph);
            assert_eq!(before.dependency_first_sccs, after.dependency_first_sccs);
        });
    }

    #[test]
    fn validation_capture_discards_failed_fields_and_excludes_declared_bodies() {
        for source in [
            r#"({ ready: "a" satisfies "a", unsupported: true ? 1 : 2 })"#,
            r#"({ unsupported: true ? 1 : 2, ready: "a" satisfies "a" })"#,
        ] {
            with_initializer(source, |expression| {
                let mut interner = Interner::with_intrinsics();
                let exact = interner.intern_literal(LiteralValue::String("a".into()));
                let mut context = Context {
                    string_literals: FxHashMap::from_iter([("a".into(), exact)]),
                    ..Default::default()
                };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let mut inferer = SurfaceInitializerInferer::new(&mut factory, &mut context);
                assert_eq!(
                    inferer.infer(expression, false, false),
                    InitializerInference::Unsupported
                );
                assert!(inferer.into_validation_sources().is_none());
            });
        }
        with_initializer(
            r#"((): string => ("a" satisfies string)) satisfies () => string"#,
            |expression| {
                let mut interner = Interner::with_intrinsics();
                let wk = interner.well_known();
                let function = interner.intern_function(FunctionType {
                    type_params: vec![],
                    receiver: None,
                    params: vec![],
                    ret: wk.string,
                });
                let mut context = Context {
                    string: Some(wk.string),
                    function: Some(function),
                    ..Default::default()
                };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let mut inferer = SurfaceInitializerInferer::new(&mut factory, &mut context);
                assert_eq!(
                    inferer.infer(expression, false, false),
                    InitializerInference::Inferred(function)
                );
                let rows = inferer.into_validation_sources().unwrap();
                let Expression::TSSatisfiesExpression(satisfies) = expression else {
                    panic!("satisfies")
                };
                let Expression::ParenthesizedExpression(parenthesized) = &satisfies.expression
                else {
                    panic!("parentheses")
                };
                let Expression::ArrowFunctionExpression(arrow) = &parenthesized.expression else {
                    panic!("arrow")
                };
                let body = arrow.get_expression().unwrap();
                assert_eq!(
                    rows.get(
                        Span::from_oxc(body.span()),
                        InitializerValidationBoundary::InferredArrowBody
                    ),
                    None
                );
                let Expression::ParenthesizedExpression(body) = body else {
                    panic!("body parentheses")
                };
                assert_eq!(
                    rows.get(
                        Span::from_oxc(body.expression.span()),
                        InitializerValidationBoundary::SatisfiesOperand
                    ),
                    None
                );
                assert_eq!(
                    rows.get(
                        Span::from_oxc(expression.span()),
                        InitializerValidationBoundary::SatisfiesOperand
                    ),
                    Some(function)
                );
            },
        );
    }

    #[test]
    fn rest_projection_class_sources_keep_positional_parameter_and_body_ids() {
        use crate::types::repr::{TupleRestType, TypeParamId};
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let symbolic = interner.intern_type_param(TypeParamId(13), "T");
        let array = interner.intern_array(wk.number);
        let symbols = interner.intern_array(symbolic);
        let tuple = interner.intern_tuple(vec![wk.number, wk.string]);
        let alternatives = interner.union(vec![array, symbols]);
        let union_element = interner.union(vec![wk.number, symbolic]);
        let nested = interner.intern_tuple_type(TupleType::with_rest(
            vec![wk.string],
            TupleRestType::new(1, symbols),
        ));
        let suffix = interner.intern_tuple_type(TupleType::with_rest(
            vec![wk.number],
            TupleRestType::new(0, nested),
        ));
        let explicit_undefined = interner.union(vec![wk.number, wk.undefined]);
        for (source, params, expected) in [
            ("((value) => value) satisfies (...args: number[]) => number", vec![ParameterType::rest("args", array)], vec![wk.number]),
            ("((first, second, third) => third) satisfies (...args: number[]) => number", vec![ParameterType::rest("args", array)], vec![wk.number; 3]),
            ("((value) => value) satisfies (...args: [number, string]) => number", vec![ParameterType::rest("args", tuple)], vec![wk.number]),
            ("((first, second) => second) satisfies (...args: [number, string]) => string", vec![ParameterType::rest("args", tuple)], vec![wk.number, wk.string]),
            ("((first, second) => second) satisfies (first: string, ...args: number[]) => number", vec![ParameterType::required("first", wk.string), ParameterType::rest("args", array)], vec![wk.string, wk.number]),
            ("((value) => value) satisfies (...args: T[]) => T", vec![ParameterType::rest("args", symbols)], vec![symbolic]),
            ("((value) => value) satisfies (value: T) => T", vec![ParameterType::required("value", symbolic)], vec![symbolic]),
            ("((value) => value) satisfies (...args: number[] | T[]) => number | T", vec![ParameterType::rest("args", alternatives)], vec![union_element]),
            ("((first, second, third) => third) satisfies (...args: [...[string, ...T[]], number]) => number", vec![ParameterType::rest("args", suffix)], vec![wk.string, symbolic, wk.number]),
            ("((value) => value) satisfies (value?: number | undefined) => number | undefined", vec![ParameterType::optional("value", explicit_undefined)], vec![explicit_undefined]),
        ] {
            with_initializer(source, |expression| {
                let returned = *expected.last().unwrap();
                let target = interner.intern_function(FunctionType {
                    type_params: vec![],
                    receiver: None,
                    params,
                    ret: returned,
                });
                let mut context = Context {
                    function: Some(target),
                    ..Default::default()
                };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let mut inferer = SurfaceInitializerInferer::new(&mut factory, &mut context);
                let InitializerInference::Inferred(function) =
                    inferer.infer(expression, false, false)
                else {
                    panic!("ready rest source")
                };
                let rows = inferer.into_validation_sources().unwrap();
                let Expression::TSSatisfiesExpression(satisfies) = expression else {
                    panic!("satisfies")
                };
                let Expression::ParenthesizedExpression(parenthesized) = &satisfies.expression
                else {
                    panic!("parentheses")
                };
                let Expression::ArrowFunctionExpression(arrow) = &parenthesized.expression else {
                    panic!("arrow")
                };
                let body = arrow.get_expression().unwrap();
                let surface = factory.store().function_type(function).unwrap();
                assert_eq!(surface.params.len(), arrow.params.items.len());
                assert_eq!(surface.params.iter().map(|parameter| parameter.ty).collect::<Vec<_>>(), expected);
                assert!(surface.params.iter().all(ParameterType::is_fixed));
                assert_eq!(surface.ret, returned);
                assert_eq!(
                    rows.get(
                        Span::from_oxc(body.span()),
                        InitializerValidationBoundary::InferredArrowBody
                    ),
                    Some(returned)
                );
                assert_eq!(
                    rows.get(
                        Span::from_oxc(arrow.span),
                        InitializerValidationBoundary::CompletedContainer
                    ),
                    Some(function)
                );
                assert_eq!(
                    rows.get(
                        Span::from_oxc(expression.span()),
                        InitializerValidationBoundary::SatisfiesOperand
                    ),
                    Some(function)
                );
            });
        }
    }

    #[test]
    fn rest_projection_class_annotation_precedes_readiness_and_failure_discards_rows() {
        use crate::types::repr::TypeParamId;
        for annotated in [true, false] {
            let source = if annotated {
                "((value: number) => 1) satisfies (...args: T) => number"
            } else {
                "((value) => 1) satisfies (...args: T) => number"
            };
            with_initializer(source, |expression| {
                let mut interner = Interner::with_intrinsics();
                let wk = interner.well_known();
                let opaque = interner.intern_type_param(TypeParamId(3), "T");
                let array = interner.intern_array(wk.number);
                assert!(interner.set_type_param_constraint(TypeParamId(3), array));
                let function = interner.intern_function(FunctionType {
                    type_params: vec![],
                    receiver: None,
                    params: vec![ParameterType::rest("args", opaque)],
                    ret: wk.number,
                });
                let mut context = Context {
                    number: Some(wk.number),
                    function: Some(function),
                    ..Default::default()
                };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let mut inferer = SurfaceInitializerInferer::new(&mut factory, &mut context);
                let inferred = inferer.infer(expression, false, false);
                let rows = inferer.into_validation_sources();
                if annotated {
                    let InitializerInference::Inferred(function) = inferred else {
                        panic!("declared parameter source")
                    };
                    assert_eq!(
                        factory.store().function_type(function).unwrap().params[0].ty,
                        wk.number
                    );
                    assert!(rows.is_some());
                } else {
                    assert_eq!(inferred, InitializerInference::Unsupported);
                    assert!(rows.is_none());
                }
            });
        }
    }

    #[test]
    fn fixed_slot_sources_survive_unused_opaque_union_rest() {
        use crate::types::repr::TypeParamId;
        for (source, parameter_count, literal_body) in [
            ("((value) => value) satisfies (value: number, ...args: T | number[]) => number", 1, false),
            ("((value) => 1) satisfies (value: number, ...args: T | number[]) => number", 1, true),
            ("((first, second: number) => 1) satisfies (first: number, ...args: T | number[]) => number", 2, true),
        ] {
            with_initializer(source, |expression| {
                let mut interner = Interner::with_intrinsics();
                let wk = interner.well_known();
                let symbolic = interner.intern_type_param(TypeParamId(41), "T");
                let numbers = interner.intern_array(wk.number);
                let rest = interner.union(vec![symbolic, numbers]);
                let body_id = if literal_body {
                    interner.intern_ordinary_literal(LiteralValue::Number(1.0))
                } else {
                    wk.number
                };
                let target = interner.intern_function(FunctionType {
                    type_params: vec![], receiver: None,
                    params: vec![ParameterType::required("value", wk.number), ParameterType::rest("args", rest)],
                    ret: wk.number,
                });
                let mut context = Context { number: Some(wk.number), function: Some(target), ..Default::default() };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let mut inferer = SurfaceInitializerInferer::new(&mut factory, &mut context);
                let InitializerInference::Inferred(function) = inferer.infer(expression, false, false) else { panic!("ready fixed source") };
                let rows = inferer.into_validation_sources().unwrap();
                let Expression::TSSatisfiesExpression(satisfies) = expression else { panic!("satisfies") };
                let Expression::ParenthesizedExpression(parenthesized) = &satisfies.expression else { panic!("parentheses") };
                let Expression::ArrowFunctionExpression(arrow) = &parenthesized.expression else { panic!("arrow") };
                let surface = factory.store().function_type(function).unwrap();
                assert_eq!(surface.params.len(), parameter_count);
                assert!(surface.params.iter().all(|parameter| parameter.is_fixed() && parameter.ty == wk.number));
                assert_eq!(surface.ret, wk.number);
                assert_eq!(rows.get(Span::from_oxc(arrow.get_expression().unwrap().span()), InitializerValidationBoundary::InferredArrowBody), Some(body_id));
                assert_eq!(rows.get(Span::from_oxc(arrow.span), InitializerValidationBoundary::CompletedContainer), Some(function));
                assert_eq!(rows.get(Span::from_oxc(expression.span()), InitializerValidationBoundary::SatisfiesOperand), Some(function));
            });
        }
    }

    #[test]
    fn validation_capture_keeps_contextual_template_binder_identity() {
        use crate::types::repr::TypeParamId;
        with_initializer(
            "((value) => value) satisfies (value: T) => T",
            |expression| {
                let mut interner = Interner::with_intrinsics();
                let parameter = interner.intern_type_param(TypeParamId(7), "T");
                let function = interner.intern_function(FunctionType {
                    type_params: vec![],
                    receiver: None,
                    params: vec![ParameterType::required("value", parameter)],
                    ret: parameter,
                });
                let mut context = Context {
                    function: Some(function),
                    ..Default::default()
                };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let mut inferer = SurfaceInitializerInferer::new(&mut factory, &mut context);
                assert_eq!(
                    inferer.infer(expression, false, false),
                    InitializerInference::Inferred(function)
                );
                let rows = inferer.into_validation_sources().unwrap();
                let Expression::TSSatisfiesExpression(satisfies) = expression else {
                    panic!("satisfies")
                };
                let Expression::ParenthesizedExpression(parenthesized) = &satisfies.expression
                else {
                    panic!("parentheses")
                };
                let Expression::ArrowFunctionExpression(arrow) = &parenthesized.expression else {
                    panic!("arrow")
                };
                assert_eq!(
                    rows.get(
                        Span::from_oxc(arrow.get_expression().unwrap().span()),
                        InitializerValidationBoundary::InferredArrowBody
                    ),
                    Some(parameter)
                );
                assert_eq!(
                    factory.store().function_type(function).unwrap().params[0].ty,
                    parameter
                );
                assert_eq!(
                    factory.store().type_param(parameter).unwrap().id,
                    TypeParamId(7)
                );
            },
        );
    }
}
