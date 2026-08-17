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
use crate::check::checker::expr::{
    contextual_literal_target, contextual_shape_members, satisfies_arrow_needs_context,
    satisfies_context_preserves_literal,
};

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
        }
    }

    pub(in crate::check::checker) fn infer(
        &mut self,
        expression: &Expression<'_>,
        readonly: bool,
        is_static: bool,
    ) -> InitializerInference {
        if is_static {
            return InitializerInference::Unsupported;
        }
        self.infer_expression(expression, readonly).map_or(
            InitializerInference::Unsupported,
            InitializerInference::Inferred,
        )
    }

    fn infer_expression(&mut self, expression: &Expression<'_>, readonly: bool) -> Option<TypeId> {
        self.infer_expression_with_context(expression, readonly, None)
    }

    fn infer_expression_with_context(
        &mut self,
        expression: &Expression<'_>,
        readonly: bool,
        context: Option<TypeId>,
    ) -> Option<TypeId> {
        let wk = self.factory.well_known();
        let context =
            context.map(|context| contextual_literal_target(self.factory.store(), context));
        let preserve_literal = context.is_some_and(|context| {
            satisfies_context_preserves_literal(self.factory.store(), expression, context)
        });
        match expression {
            Expression::BooleanLiteral(literal) => Some(if readonly || preserve_literal {
                self.factory
                    .intern_literal(LiteralValue::Boolean(literal.value))
            } else {
                wk.boolean
            }),
            Expression::NullLiteral(_) => Some(wk.null),
            Expression::NumericLiteral(literal) => Some(if readonly || preserve_literal {
                self.factory
                    .intern_literal(LiteralValue::Number(literal.value))
            } else {
                wk.number
            }),
            Expression::StringLiteral(literal) => Some(if readonly || preserve_literal {
                self.factory
                    .intern_literal(LiteralValue::String(literal.value.to_string()))
            } else {
                wk.string
            }),
            Expression::ParenthesizedExpression(parenthesized) => {
                self.infer_expression_with_context(&parenthesized.expression, readonly, context)
            }
            Expression::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation
                    && matches!(unary.argument, Expression::NumericLiteral(_)) =>
            {
                let Expression::NumericLiteral(literal) = &unary.argument else {
                    return None;
                };
                Some(if readonly || preserve_literal {
                    self.factory
                        .intern_literal(LiteralValue::Number(-literal.value))
                } else {
                    wk.number
                })
            }
            Expression::TSAsExpression(assertion) => {
                self.infer_assertion(&assertion.expression, &assertion.type_annotation)
            }
            Expression::TSTypeAssertion(assertion) => {
                self.infer_assertion(&assertion.expression, &assertion.type_annotation)
            }
            Expression::TSSatisfiesExpression(satisfies) => {
                self.infer_satisfies(&satisfies.expression, &satisfies.type_annotation, readonly)
            }
            Expression::ArrayExpression(array) => {
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
                    elements.push(self.infer_expression_with_context(
                        expression,
                        false,
                        element_context,
                    )?);
                }
                if tuple_contexts.is_some() {
                    Some(self.factory.intern_tuple(TupleType::fixed(elements)))
                } else {
                    let element = self.factory.union(elements);
                    Some(self.factory.intern_array(element))
                }
            }
            Expression::ObjectExpression(object) => {
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
                        false,
                        property_context,
                    )?;
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
                        None => contextual.as_ref()?.params.get(index)?.ty,
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
                let ret = self.infer_expression_with_context(body, false, body_context);
                self.locals.pop();
                Some(self.factory.intern_function(FunctionType {
                    type_params: Vec::new(),
                    receiver: None,
                    params: parameters,
                    ret: declared_return.or(ret)?,
                }))
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
        }
    }

    fn infer_assertion(
        &mut self,
        operand: &Expression<'_>,
        annotation: &TSType<'_>,
    ) -> Option<TypeId> {
        self.infer_expression(operand, false)?;
        match self.context.lower_annotation(annotation) {
            DemandOutcome::Ready(ty) => Some(ty),
            DemandOutcome::Exhausted(_) => None,
        }
    }

    fn infer_satisfies(
        &mut self,
        operand: &Expression<'_>,
        annotation: &TSType<'_>,
        readonly: bool,
    ) -> Option<TypeId> {
        let DemandOutcome::Ready(target) = self.context.lower_annotation(annotation) else {
            return None;
        };
        self.infer_expression_with_context(operand, readonly, Some(target))
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
            assert_eq!(factory.store().tag(readonly), TypeTag::Literal);
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
                .infer_expression_with_context(expression, false, Some(target))
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
}
