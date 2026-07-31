//! Closed, syntax-only inference for unannotated instance fields.

use crate::class_semantics::DemandOutcome;
use crate::types::repr::{FunctionType, LiteralValue, ObjectType, ParameterType, PropertyType};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    ArrayExpressionElement, BindingPattern, Expression, ObjectPropertyKind, PropertyKind, TSType,
    UnaryOperator,
};
use rustc_hash::FxHashMap;

use super::surface_types::SurfaceTypeFactory;

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
    fn lexical_value(&self, name: &str) -> Option<TypeId>;
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
        let wk = self.factory.well_known();
        match expression {
            Expression::BooleanLiteral(literal) => Some(if readonly {
                self.factory
                    .intern_literal(LiteralValue::Boolean(literal.value))
            } else {
                wk.boolean
            }),
            Expression::NullLiteral(_) => Some(wk.null),
            Expression::NumericLiteral(literal) => Some(if readonly {
                self.factory
                    .intern_literal(LiteralValue::Number(literal.value))
            } else {
                wk.number
            }),
            Expression::StringLiteral(literal) => Some(if readonly {
                self.factory
                    .intern_literal(LiteralValue::String(literal.value.to_string()))
            } else {
                wk.string
            }),
            Expression::ParenthesizedExpression(parenthesized) => {
                self.infer_expression(&parenthesized.expression, readonly)
            }
            Expression::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation
                    && matches!(unary.argument, Expression::NumericLiteral(_)) =>
            {
                let Expression::NumericLiteral(literal) = &unary.argument else {
                    return None;
                };
                Some(if readonly {
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
            Expression::ArrayExpression(array) => {
                let mut elements = Vec::with_capacity(array.elements.len());
                for element in &array.elements {
                    let expression = match element {
                        ArrayExpressionElement::SpreadElement(_)
                        | ArrayExpressionElement::Elision(_) => return None,
                        _ => element.as_expression()?,
                    };
                    elements.push(self.infer_expression(expression, false)?);
                }
                let element = self.factory.union(elements);
                Some(self.factory.intern_array(element))
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
                    let ty = self.infer_expression(&property.value, false)?;
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
                let mut frame = FxHashMap::default();
                let mut parameters = Vec::with_capacity(arrow.params.items.len());
                for parameter in &arrow.params.items {
                    if parameter.initializer.is_some() {
                        return None;
                    }
                    let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern else {
                        return None;
                    };
                    let annotation = parameter.type_annotation.as_ref()?;
                    let DemandOutcome::Ready(ty) =
                        self.context.lower_annotation(&annotation.type_annotation)
                    else {
                        return None;
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
                let ret = self.infer_expression(body, false);
                self.locals.pop();
                Some(self.factory.intern_function(FunctionType {
                    type_params: Vec::new(),
                    receiver: None,
                    params: parameters,
                    ret: ret?,
                }))
            }
            Expression::Identifier(identifier) => self
                .locals
                .iter()
                .rev()
                .find_map(|frame| frame.get(identifier.name.as_str()).copied())
                .or_else(|| self.context.lexical_value(identifier.name.as_str())),

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
            | Expression::TSSatisfiesExpression(_)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_semantics::Exhaustion;
    use crate::types::repr::{FunctionType, TypeTag};
    use crate::types::Interner;
    use oxc_allocator::Allocator;
    use oxc_ast::ast::{ClassElement, Statement};
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    #[derive(Default)]
    struct Context {
        number: Option<TypeId>,
        string: Option<TypeId>,
        fields: FxHashMap<String, TypeId>,
        methods: FxHashMap<String, EarlierMethodSurface>,
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
                _ => DemandOutcome::Exhausted(Exhaustion::EvaluationBudget),
            }
        }

        fn earlier_field(&self, name: &str) -> Option<TypeId> {
            self.fields.get(name).copied()
        }

        fn earlier_method(&self, name: &str) -> Option<EarlierMethodSurface> {
            self.methods.get(name).copied()
        }

        fn lexical_value(&self, _name: &str) -> Option<TypeId> {
            None
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
    fn unsupported_boundary_never_falls_through_to_inference() {
        for source in [
            "+1",
            "[,,]",
            "[...[]]",
            "({ x })",
            "({...{}})",
            "`x`",
            "1!",
            "1 satisfies number",
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
