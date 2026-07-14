//! Query-free lowering of class surface type syntax.

use crate::class_semantics::{ClassApplicationArguments, DemandOutcome, Exhaustion};
use crate::types::repr::{
    ClassId, ConditionalType, FunctionType, GenericTypeParam, LiteralValue, MappedType, ModifierOp,
    ObjectType, ParameterType, PropertyType, TemplateType, TupleRestType, TupleType, TypeParamId,
    TypeTag,
};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    BindingPattern, Expression, FormalParameters, TSLiteral, TSMappedTypeModifierOperator,
    TSSignature, TSThisParameter, TSTupleElement, TSType, TSTypeName, TSTypeOperatorOperator,
    TSTypeParameterDeclaration, UnaryOperator,
};
use oxc_span::Span;
use rustc_hash::FxHashMap;

use super::application::{
    complete_class_arguments, ClassApplicationKind, ClassApplicationRequest, ClassTypeParameter,
    ExplicitClassArgument, SourceClassArguments,
};
use super::surface_types::SurfaceTypeFactory;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum SurfaceTypeFailure<Ticket> {
    Unsupported(Ticket),
    TypeQuery {
        span: Span,
    },
    ThisType {
        span: Span,
    },
    Poisoned(Ticket),
    Unresolved {
        owner: Ticket,
        span: Span,
        name: String,
    },
    WrongArity {
        span: Span,
        name: String,
        expected_min: usize,
        expected_max: usize,
    },
    Exhausted(Exhaustion),
}

pub(in crate::check::checker) type SurfaceTypeResult<Ticket> =
    Result<TypeId, SurfaceTypeFailure<Ticket>>;

/// Query-free callable syntax retained even when one of its children is unavailable.
/// A failed callable never interns a partial `FunctionType`, but its independent body
/// can still run with every successfully lowered binder and parameter.
#[derive(Clone, Debug)]
pub(in crate::check::checker) struct LoweredCallableSyntax<Ticket> {
    pub type_params: Vec<GenericTypeParam>,
    pub receiver: Option<TypeId>,
    pub params: Vec<Option<ParameterType>>,
    pub declared_return: Option<TypeId>,
    pub failure: Option<SurfaceTypeFailure<Ticket>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum SurfaceNameResolution<Ticket> {
    Direct(TypeId),
    Alias {
        template: TypeId,
        parameters: Vec<TypeParamId>,
    },
    Class {
        class: ClassId,
        parameters: Vec<ClassTypeParameter<Ticket>>,
    },
    Poisoned(Ticket),
    Unavailable(Ticket),
}

/// Narrow reservation/name capability. Implementations only read identities and
/// preallocated tickets; they cannot evaluate, project, relate, or allocate events.
pub(in crate::check::checker) trait SurfaceTypeResolver<Ticket> {
    fn resolve_name(&mut self, name: &str) -> SurfaceNameResolution<Ticket>;

    fn signature_type_parameter(
        &mut self,
        callable_source_start: u32,
        ordinal: usize,
        name: &str,
    ) -> Option<TypeParamId>;

    fn unsupported_ticket(&mut self, span: Span) -> Ticket;
}

#[derive(Default)]
struct InferFrame {
    active: bool,
    binders: FxHashMap<String, u32>,
    poisoned: bool,
}

struct MappedFrame {
    key: String,
    captured_source: Option<TypeId>,
}

/// Produces immutable raw/deferred nodes only. Child failures are retained so
/// their independent lexical owners can still be completed when the parent fails.
pub(in crate::check::checker) struct TypeSyntaxLowerer<'a, 'b, R, Ticket> {
    factory: &'a mut SurfaceTypeFactory<'b>,
    resolver: &'a mut R,
    type_param_frames: Vec<FxHashMap<String, TypeId>>,
    infer_frames: Vec<InferFrame>,
    mapped_frames: Vec<MappedFrame>,
    child_failures: Vec<SurfaceTypeFailure<Ticket>>,
}

impl<'a, 'b, R, Ticket> TypeSyntaxLowerer<'a, 'b, R, Ticket>
where
    R: SurfaceTypeResolver<Ticket>,
    Ticket: Copy,
{
    pub(in crate::check::checker) fn new(
        factory: &'a mut SurfaceTypeFactory<'b>,
        resolver: &'a mut R,
    ) -> Self {
        TypeSyntaxLowerer {
            factory,
            resolver,
            type_param_frames: Vec::new(),
            infer_frames: Vec::new(),
            mapped_frames: Vec::new(),
            child_failures: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn lower(
        &mut self,
        annotation: &TSType<'_>,
    ) -> SurfaceTypeResult<Ticket> {
        self.lower_type(annotation)
    }

    pub(in crate::check::checker) fn lower_with_type_parameters(
        &mut self,
        annotation: &TSType<'_>,
        parameters: impl IntoIterator<Item = (String, TypeId)>,
    ) -> SurfaceTypeResult<Ticket> {
        self.type_param_frames
            .push(parameters.into_iter().collect());
        let result = self.lower_type(annotation);
        self.type_param_frames.pop();
        result
    }

    pub(in crate::check::checker) fn lower_callable_syntax_with_type_parameters(
        &mut self,
        type_parameters: Option<&TSTypeParameterDeclaration<'_>>,
        this_param: Option<&TSThisParameter<'_>>,
        parameters: &FormalParameters<'_>,
        return_type: Option<&TSType<'_>>,
        span: Span,
        outer_parameters: impl IntoIterator<Item = (String, TypeId)>,
    ) -> LoweredCallableSyntax<Ticket> {
        self.type_param_frames
            .push(outer_parameters.into_iter().collect());
        let result =
            self.lower_function_syntax(type_parameters, this_param, parameters, return_type, span);
        self.type_param_frames.pop();
        result
    }

    pub(in crate::check::checker) fn take_child_failures(
        &mut self,
    ) -> Vec<SurfaceTypeFailure<Ticket>> {
        std::mem::take(&mut self.child_failures)
    }

    fn unsupported(&mut self, span: Span) -> SurfaceTypeFailure<Ticket> {
        SurfaceTypeFailure::Unsupported(self.resolver.unsupported_ticket(span))
    }

    fn capture_child(
        &mut self,
        result: SurfaceTypeResult<Ticket>,
        first_failure: &mut Option<SurfaceTypeFailure<Ticket>>,
    ) -> Option<TypeId> {
        match result {
            Ok(value) => Some(value),
            Err(failure) => {
                if first_failure.is_none() {
                    *first_failure = Some(failure);
                } else {
                    self.child_failures.push(failure);
                }
                None
            }
        }
    }

    fn capture_failure(
        &mut self,
        failure: SurfaceTypeFailure<Ticket>,
        first_failure: &mut Option<SurfaceTypeFailure<Ticket>>,
    ) {
        if first_failure.is_none() {
            *first_failure = Some(failure);
        } else {
            self.child_failures.push(failure);
        }
    }

    fn lower_type(&mut self, annotation: &TSType<'_>) -> SurfaceTypeResult<Ticket> {
        let wk = self.factory.well_known();
        match annotation {
            TSType::TSAnyKeyword(_) => Ok(wk.any),
            TSType::TSUnknownKeyword(_) => Ok(wk.unknown),
            TSType::TSNeverKeyword(_) => Ok(wk.never),
            TSType::TSVoidKeyword(_) => Ok(wk.void),
            TSType::TSNullKeyword(_) => Ok(wk.null),
            TSType::TSUndefinedKeyword(_) => Ok(wk.undefined),
            TSType::TSBooleanKeyword(_) => Ok(wk.boolean),
            TSType::TSNumberKeyword(_) => Ok(wk.number),
            TSType::TSStringKeyword(_) => Ok(wk.string),
            TSType::TSLiteralType(literal) => self.lower_literal(&literal.literal, literal.span),
            TSType::TSParenthesizedType(parenthesized) => {
                self.lower_type(&parenthesized.type_annotation)
            }
            TSType::TSUnionType(union) => {
                let mut members = Vec::with_capacity(union.types.len());
                let mut failure = None;
                for member in &union.types {
                    let lowered = self.lower_type(member);
                    if let Some(member) = self.capture_child(lowered, &mut failure) {
                        members.push(member);
                    }
                }
                match failure {
                    Some(failure) => Err(failure),
                    None => Ok(self.factory.union(members)),
                }
            }
            TSType::TSIntersectionType(intersection) => {
                let mut members = Vec::with_capacity(intersection.types.len());
                let mut failure = None;
                for member in &intersection.types {
                    let lowered = self.lower_type(member);
                    if let Some(member) = self.capture_child(lowered, &mut failure) {
                        members.push(member);
                    }
                }
                match failure {
                    Some(failure) => Err(failure),
                    None => Ok(self.factory.intersection(members)),
                }
            }
            TSType::TSArrayType(array) => {
                let element = self.lower_type(&array.element_type)?;
                Ok(self.factory.intern_array(element))
            }
            TSType::TSTupleType(tuple) => self.lower_tuple(&tuple.element_types, tuple.span),
            TSType::TSTypeLiteral(literal) => self.lower_object(&literal.members, literal.span),
            TSType::TSFunctionType(function) => self.lower_function(
                function.type_parameters.as_deref(),
                function.this_param.as_deref(),
                &function.params,
                Some(&function.return_type.type_annotation),
                function.span,
            ),
            TSType::TSConstructorType(constructor) => {
                let signature = self.lower_function(
                    constructor.type_parameters.as_deref(),
                    None,
                    &constructor.params,
                    Some(&constructor.return_type.type_annotation),
                    constructor.span,
                )?;
                Ok(self.factory.intern_object(ObjectType {
                    construct_signatures: vec![signature],
                    ..Default::default()
                }))
            }
            TSType::TSTypeReference(reference) => self.lower_reference(
                &reference.type_name,
                reference.type_arguments.as_deref(),
                reference.span,
            ),
            TSType::TSTypeOperatorType(operator) => match operator.operator {
                TSTypeOperatorOperator::Keyof => {
                    let operand = self.lower_type(&operator.type_annotation)?;
                    Ok(self.factory.intern_keyof(operand))
                }
                TSTypeOperatorOperator::Readonly => {
                    let operand = self.lower_type(&operator.type_annotation)?;
                    if matches!(
                        self.factory.store().tag(operand),
                        TypeTag::Array | TypeTag::Tuple
                    ) {
                        Ok(self.factory.intern_readonly(operand))
                    } else {
                        Err(self.unsupported(operator.span))
                    }
                }
                TSTypeOperatorOperator::Unique => Err(self.unsupported(operator.span)),
            },
            TSType::TSIndexedAccessType(access) => {
                self.lower_indexed_access(&access.object_type, &access.index_type)
            }
            TSType::TSConditionalType(conditional) => self.lower_conditional(conditional),
            TSType::TSInferType(infer) => self.lower_infer(infer),
            TSType::TSMappedType(mapped) => self.lower_mapped(mapped),
            TSType::TSTemplateLiteralType(template) => self.lower_template(template),

            // Explicitly unsupported: adding an oxc variant must fail this match.
            TSType::TSBigIntKeyword(keyword) => Err(self.unsupported(keyword.span)),
            TSType::TSIntrinsicKeyword(keyword) => Err(self.unsupported(keyword.span)),
            TSType::TSObjectKeyword(keyword) => Err(self.unsupported(keyword.span)),
            TSType::TSSymbolKeyword(keyword) => Err(self.unsupported(keyword.span)),
            TSType::TSImportType(import) => Err(self.unsupported(import.span)),
            TSType::TSNamedTupleMember(named) => Err(self.unsupported(named.span)),
            TSType::TSThisType(this_type) => Err(SurfaceTypeFailure::ThisType {
                span: this_type.span,
            }),
            TSType::TSTypePredicate(predicate) => Err(self.unsupported(predicate.span)),
            TSType::TSTypeQuery(query) => Err(SurfaceTypeFailure::TypeQuery { span: query.span }),
            TSType::JSDocNullableType(jsdoc) => Err(self.unsupported(jsdoc.span)),
            TSType::JSDocNonNullableType(jsdoc) => Err(self.unsupported(jsdoc.span)),
            TSType::JSDocUnknownType(jsdoc) => Err(self.unsupported(jsdoc.span)),
        }
    }

    fn lower_literal(&mut self, literal: &TSLiteral<'_>, span: Span) -> SurfaceTypeResult<Ticket> {
        let value = match literal {
            TSLiteral::StringLiteral(literal) => LiteralValue::String(literal.value.to_string()),
            TSLiteral::NumericLiteral(literal) => LiteralValue::Number(literal.value),
            TSLiteral::BooleanLiteral(literal) => LiteralValue::Boolean(literal.value),
            TSLiteral::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation
                    && matches!(unary.argument, Expression::NumericLiteral(_)) =>
            {
                let Expression::NumericLiteral(literal) = &unary.argument else {
                    return Err(self.unsupported(span));
                };
                let negated = -literal.value;
                LiteralValue::Number(if negated == 0.0 { 0.0 } else { negated })
            }
            TSLiteral::BigIntLiteral(_)
            | TSLiteral::TemplateLiteral(_)
            | TSLiteral::UnaryExpression(_) => return Err(self.unsupported(span)),
        };
        Ok(self.factory.intern_literal(value))
    }

    fn lower_tuple(
        &mut self,
        elements: &[TSTupleElement<'_>],
        span: Span,
    ) -> SurfaceTypeResult<Ticket> {
        let mut fixed = Vec::with_capacity(elements.len());
        let mut rest = None;
        let mut failure = None;
        for element in elements {
            match element {
                TSTupleElement::TSRestType(rest_type) if rest.is_none() => {
                    let lowered = self.lower_type(&rest_type.type_annotation);
                    if let Some(ty) = self.capture_child(lowered, &mut failure) {
                        rest = Some(TupleRestType::new(fixed.len(), ty));
                    }
                }
                TSTupleElement::TSOptionalType(optional) => {
                    let unsupported = self.unsupported(optional.span);
                    self.capture_failure(unsupported, &mut failure);
                    let lowered = self.lower_type(&optional.type_annotation);
                    self.capture_child(lowered, &mut failure);
                }
                TSTupleElement::TSNamedTupleMember(named) => {
                    let unsupported = self.unsupported(named.span);
                    self.capture_failure(unsupported, &mut failure);
                    self.visit_tuple_element(&named.element_type, &mut failure);
                }
                TSTupleElement::TSRestType(rest_type) => {
                    let unsupported = self.unsupported(rest_type.span);
                    self.capture_failure(unsupported, &mut failure);
                    let lowered = self.lower_type(&rest_type.type_annotation);
                    self.capture_child(lowered, &mut failure);
                }
                _ => {
                    if let Some(ty) = element.as_ts_type() {
                        let lowered = self.lower_type(ty);
                        if let Some(ty) = self.capture_child(lowered, &mut failure) {
                            fixed.push(ty);
                        }
                    } else {
                        let unsupported = self.unsupported(span);
                        self.capture_failure(unsupported, &mut failure);
                    }
                }
            }
        }
        match failure {
            Some(failure) => Err(failure),
            None => Ok(self.factory.intern_tuple(match rest {
                Some(rest) => TupleType::with_rest(fixed, rest),
                None => TupleType::fixed(fixed),
            })),
        }
    }

    fn visit_tuple_element(
        &mut self,
        element: &TSTupleElement<'_>,
        failure: &mut Option<SurfaceTypeFailure<Ticket>>,
    ) {
        match element {
            TSTupleElement::TSRestType(rest) => {
                let lowered = self.lower_type(&rest.type_annotation);
                self.capture_child(lowered, failure);
            }
            TSTupleElement::TSOptionalType(optional) => {
                let lowered = self.lower_type(&optional.type_annotation);
                self.capture_child(lowered, failure);
            }
            TSTupleElement::TSNamedTupleMember(named) => {
                self.visit_tuple_element(&named.element_type, failure);
            }
            _ => {
                if let Some(ty) = element.as_ts_type() {
                    let lowered = self.lower_type(ty);
                    self.capture_child(lowered, failure);
                }
            }
        }
    }

    fn lower_reference(
        &mut self,
        name: &TSTypeName<'_>,
        arguments: Option<&oxc_ast::ast::TSTypeParameterInstantiation<'_>>,
        span: Span,
    ) -> SurfaceTypeResult<Ticket> {
        let TSTypeName::IdentifierReference(identifier) = name else {
            return Err(self.unsupported(span));
        };
        let name = identifier.name.as_str();
        if arguments.is_none() {
            if let Some(ty) = self
                .type_param_frames
                .iter()
                .rev()
                .find_map(|frame| frame.get(name).copied())
            {
                return Ok(ty);
            }
            if let Some(ty) = self.resolve_infer_name(name) {
                return Ok(ty);
            }
        }

        let mut explicit = Vec::new();
        if let Some(arguments) = arguments {
            explicit.reserve(arguments.params.len());
            for child in &arguments.params {
                match self.lower_type(child) {
                    Ok(ty) => explicit.push(ExplicitClassArgument::Ready(ty)),
                    Err(failure) => {
                        self.child_failures.push(failure);
                        explicit.push(ExplicitClassArgument::Unavailable);
                    }
                }
            }
        }
        let source_arguments = arguments.map_or(SourceClassArguments::Omitted, |_| {
            SourceClassArguments::Explicit(&explicit)
        });

        match self.resolver.resolve_name(name) {
            SurfaceNameResolution::Direct(ty) => {
                if explicit.is_empty() {
                    Ok(ty)
                } else {
                    Err(reference_exhaustion(
                        name,
                        span,
                        Exhaustion::ClassApplicationArguments(
                            ClassApplicationArguments::WrongArity {
                                expected_min: 0,
                                expected_max: 0,
                                actual: explicit.len(),
                            },
                        ),
                    ))
                }
            }
            SurfaceNameResolution::Alias {
                template,
                parameters,
            } => {
                if explicit.len() != parameters.len()
                    || explicit
                        .iter()
                        .any(|argument| matches!(argument, ExplicitClassArgument::Unavailable))
                {
                    let reason = if explicit.len() != parameters.len() {
                        ClassApplicationArguments::WrongArity {
                            expected_min: parameters.len(),
                            expected_max: parameters.len(),
                            actual: explicit.len(),
                        }
                    } else {
                        let index = explicit
                            .iter()
                            .position(|argument| {
                                matches!(argument, ExplicitClassArgument::Unavailable)
                            })
                            .unwrap_or(0);
                        ClassApplicationArguments::UnavailableExplicitArgument { index }
                    };
                    return Err(reference_exhaustion(
                        name,
                        span,
                        Exhaustion::ClassApplicationArguments(reason),
                    ));
                }
                let substitutions = parameters
                    .into_iter()
                    .zip(explicit)
                    .filter_map(|(parameter, argument)| match argument {
                        ExplicitClassArgument::Ready(ty) => Some((parameter, ty)),
                        ExplicitClassArgument::Unavailable => None,
                    })
                    .collect();
                Ok(self.factory.intern_instantiation(template, substitutions))
            }
            SurfaceNameResolution::Class { class, parameters } => {
                let request = ClassApplicationRequest {
                    class,
                    parameters: &parameters,
                    source_arguments,
                    inferred: &[],
                    kind: ClassApplicationKind::TypeReference,
                };
                match complete_class_arguments(request) {
                    DemandOutcome::Ready(arguments) => {
                        Ok(self.factory.intern_class_instance(class, arguments))
                    }
                    DemandOutcome::Exhausted(reason) => {
                        Err(reference_exhaustion(name, span, reason))
                    }
                }
            }
            SurfaceNameResolution::Poisoned(ticket) => Err(SurfaceTypeFailure::Poisoned(ticket)),
            SurfaceNameResolution::Unavailable(ticket) => Err(SurfaceTypeFailure::Unresolved {
                owner: ticket,
                span: identifier.span,
                name: name.to_string(),
            }),
        }
    }

    fn lower_conditional(
        &mut self,
        conditional: &oxc_ast::ast::TSConditionalType<'_>,
    ) -> SurfaceTypeResult<Ticket> {
        self.infer_frames.push(InferFrame::default());
        let check = self.lower_type(&conditional.check_type);
        let distributive = check
            .as_ref()
            .is_ok_and(|ty| self.factory.store().tag(*ty) == TypeTag::TypeParam);
        if let Some(frame) = self.infer_frames.last_mut() {
            frame.active = true;
        }
        let extends_ty = self.lower_type(&conditional.extends_type);
        let true_branch = self.lower_type(&conditional.true_type);
        if let Some(frame) = self.infer_frames.last_mut() {
            frame.active = false;
        }
        let false_branch = self.lower_type(&conditional.false_type);
        let frame = self.infer_frames.pop().unwrap_or_default();
        let mut failure = None;
        let check = self.capture_child(check, &mut failure);
        let extends_ty = self.capture_child(extends_ty, &mut failure);
        let true_branch = self.capture_child(true_branch, &mut failure);
        let false_branch = self.capture_child(false_branch, &mut failure);
        match (failure, check, extends_ty, true_branch, false_branch) {
            (Some(failure), _, _, _, _) => Err(failure),
            (None, Some(check), Some(extends_ty), Some(true_branch), Some(false_branch)) => {
                Ok(self.factory.intern_conditional(ConditionalType {
                    check,
                    extends_ty,
                    true_branch,
                    false_branch,
                    infer_count: frame.binders.len() as u32,
                    distributive,
                    poisoned: frame.poisoned,
                }))
            }
            (None, _, _, _, _) => unreachable!("all successful conditional children are retained"),
        }
    }

    fn lower_infer(&mut self, infer: &oxc_ast::ast::TSInferType<'_>) -> SurfaceTypeResult<Ticket> {
        let Some(frame) = self
            .infer_frames
            .iter_mut()
            .rev()
            .find(|frame| frame.active)
        else {
            return Err(self.unsupported(infer.span));
        };
        let name = infer.type_parameter.name.name.as_str();
        let next = frame.binders.len() as u32;
        let index = *frame.binders.entry(name.to_string()).or_insert(next);
        Ok(self.factory.intern_infer(index))
    }

    fn resolve_infer_name(&mut self, name: &str) -> Option<TypeId> {
        let innermost = self.infer_frames.len().checked_sub(1)?;
        let (owner, index) = self
            .infer_frames
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, frame)| frame.active)
            .find_map(|(owner, frame)| frame.binders.get(name).map(|index| (owner, *index)))?;
        if owner != innermost {
            for frame in &mut self.infer_frames[owner..] {
                frame.poisoned = true;
            }
        }
        Some(self.factory.intern_infer(index))
    }

    fn lower_mapped(
        &mut self,
        mapped: &oxc_ast::ast::TSMappedType<'_>,
    ) -> SurfaceTypeResult<Ticket> {
        let mut failure = None;
        if mapped.name_type.is_some() {
            let unsupported = self.unsupported(mapped.span);
            self.capture_failure(unsupported, &mut failure);
            if let Some(name_type) = mapped.name_type.as_ref() {
                let lowered = self.lower_type(name_type);
                self.capture_child(lowered, &mut failure);
            }
        }
        let (homomorphic, key_surface) = match &mapped.constraint {
            TSType::TSTypeOperatorType(operator)
                if operator.operator == TSTypeOperatorOperator::Keyof =>
            {
                (true, &operator.type_annotation)
            }
            other => (false, other),
        };
        let key_source = self.lower_type(key_surface);
        self.mapped_frames.push(MappedFrame {
            key: mapped.key.name.to_string(),
            captured_source: None,
        });
        let value = match mapped.type_annotation.as_ref() {
            Some(value) => self.lower_type(value),
            None => Err(self.unsupported(mapped.span)),
        };
        let frame = self.mapped_frames.pop();
        let key_source = self.capture_child(key_source, &mut failure);
        let value_template = self.capture_child(value, &mut failure);
        match (failure, key_source, value_template) {
            (Some(failure), _, _) => Err(failure),
            (None, Some(key_source), Some(value_template)) => {
                Ok(self.factory.intern_mapped(MappedType {
                    homomorphic,
                    key_source,
                    value_template,
                    modifiers_source: if homomorphic {
                        None
                    } else {
                        frame.and_then(|frame| frame.captured_source)
                    },
                    optional_modifier: modifier(mapped.optional),
                    readonly_modifier: modifier(mapped.readonly),
                }))
            }
            (None, _, _) => unreachable!("all successful mapped children are retained"),
        }
    }

    fn lower_indexed_access(
        &mut self,
        object: &TSType<'_>,
        index: &TSType<'_>,
    ) -> SurfaceTypeResult<Ticket> {
        if self.index_is_active_mapped_key(index) {
            let object = self.lower_type(object)?;
            if let Some(frame) = self.mapped_frames.last_mut() {
                frame.captured_source.get_or_insert(object);
            }
            return Ok(self.factory.intern_mapped_value());
        }
        let object = self.lower_type(object);
        let index = self.lower_type(index);
        let mut failure = None;
        let object = self.capture_child(object, &mut failure);
        let index = self.capture_child(index, &mut failure);
        match (failure, object, index) {
            (Some(failure), _, _) => Err(failure),
            (None, Some(object), Some(index)) => {
                Ok(self.factory.intern_deferred_indexed_access(object, index))
            }
            (None, _, _) => unreachable!("all successful indexed-access children are retained"),
        }
    }

    fn index_is_active_mapped_key(&self, index: &TSType<'_>) -> bool {
        let Some(frame) = self.mapped_frames.last() else {
            return false;
        };
        let TSType::TSTypeReference(reference) = index else {
            return false;
        };
        let TSTypeName::IdentifierReference(identifier) = &reference.type_name else {
            return false;
        };
        reference.type_arguments.is_none() && identifier.name.as_str() == frame.key
    }

    fn lower_template(
        &mut self,
        template: &oxc_ast::ast::TSTemplateLiteralType<'_>,
    ) -> SurfaceTypeResult<Ticket> {
        let mut texts = Vec::with_capacity(template.quasis.len());
        let mut failure = None;
        for quasi in &template.quasis {
            if let Some(cooked) = quasi.value.cooked.as_ref() {
                texts.push(cooked.to_string());
            } else {
                let unsupported = self.unsupported(template.span);
                self.capture_failure(unsupported, &mut failure);
            }
        }
        let mut holes = Vec::with_capacity(template.types.len());
        for hole in &template.types {
            let lowered = self.lower_type(hole);
            if let Some(hole) = self.capture_child(lowered, &mut failure) {
                holes.push(hole);
            }
        }
        if let Some(failure) = failure {
            return Err(failure);
        }
        for index in 1..holes.len() {
            if texts.get(index).is_some_and(String::is_empty)
                && (self.factory.store().tag(holes[index - 1]) == TypeTag::Infer
                    || self.factory.store().tag(holes[index]) == TypeTag::Infer)
            {
                if let Some(frame) = self
                    .infer_frames
                    .iter_mut()
                    .rev()
                    .find(|frame| frame.active)
                {
                    frame.poisoned = true;
                }
            }
        }
        Ok(self.factory.intern_template(TemplateType { texts, holes }))
    }

    fn lower_object(
        &mut self,
        members: &[TSSignature<'_>],
        _span: Span,
    ) -> SurfaceTypeResult<Ticket> {
        let mut object = ObjectType::default();
        let mut methods: FxHashMap<String, Vec<TypeId>> = FxHashMap::default();
        let mut failure = None;
        for member in members {
            match member {
                TSSignature::TSPropertySignature(property) => {
                    let name = property.key.static_name().map(|name| name.into_owned());
                    if name.is_none() {
                        let unsupported = self.unsupported(property.span);
                        self.capture_failure(unsupported, &mut failure);
                    }
                    let ty = if let Some(annotation) = property.type_annotation.as_ref() {
                        let lowered = self.lower_type(&annotation.type_annotation);
                        self.capture_child(lowered, &mut failure)
                    } else {
                        let unsupported = self.unsupported(property.span);
                        self.capture_failure(unsupported, &mut failure);
                        None
                    };
                    if let (Some(name), Some(mut ty)) = (name, ty) {
                        if property.optional {
                            ty = self
                                .factory
                                .union(vec![ty, self.factory.well_known().undefined]);
                        }
                        let mut lowered = PropertyType::public(name, ty);
                        lowered.optional = property.optional;
                        lowered.readonly = property.readonly;
                        object.properties.push(lowered);
                    }
                }
                TSSignature::TSIndexSignature(index) => {
                    if index.readonly || index.parameters.len() != 1 {
                        let unsupported = self.unsupported(index.span);
                        self.capture_failure(unsupported, &mut failure);
                    }
                    let mut keys = Vec::with_capacity(index.parameters.len());
                    for parameter in &index.parameters {
                        let lowered = self.lower_type(&parameter.type_annotation.type_annotation);
                        if let Some(key) = self.capture_child(lowered, &mut failure) {
                            keys.push(key);
                        }
                    }
                    let value = self.lower_type(&index.type_annotation.type_annotation);
                    let value = self.capture_child(value, &mut failure);
                    if let ([key], Some(value)) = (keys.as_slice(), value) {
                        let wk = self.factory.well_known();
                        if *key == wk.string {
                            object.string_index = Some(value);
                        } else if *key == wk.number {
                            object.number_index = Some(value);
                        } else {
                            let unsupported = self.unsupported(index.span);
                            self.capture_failure(unsupported, &mut failure);
                        }
                    }
                }
                TSSignature::TSMethodSignature(method) => {
                    let valid_shape = !method.optional
                        && method.kind == oxc_ast::ast::TSMethodSignatureKind::Method;
                    if !valid_shape {
                        let unsupported = self.unsupported(method.span);
                        self.capture_failure(unsupported, &mut failure);
                    }
                    let name = method.key.static_name().map(|name| name.into_owned());
                    if name.is_none() {
                        let unsupported = self.unsupported(method.span);
                        self.capture_failure(unsupported, &mut failure);
                    }
                    let signature = self.lower_function(
                        method.type_parameters.as_deref(),
                        method.this_param.as_deref(),
                        &method.params,
                        method
                            .return_type
                            .as_deref()
                            .map(|annotation| &annotation.type_annotation),
                        method.span,
                    );
                    let signature = self.capture_child(signature, &mut failure);
                    if valid_shape {
                        if let (Some(name), Some(signature)) = (name, signature) {
                            methods.entry(name).or_default().push(signature);
                        }
                    }
                }
                TSSignature::TSCallSignatureDeclaration(call) => {
                    let signature = self.lower_function(
                        call.type_parameters.as_deref(),
                        call.this_param.as_deref(),
                        &call.params,
                        call.return_type
                            .as_deref()
                            .map(|annotation| &annotation.type_annotation),
                        call.span,
                    );
                    if let Some(signature) = self.capture_child(signature, &mut failure) {
                        object.call_signatures.push(signature);
                    }
                }
                TSSignature::TSConstructSignatureDeclaration(construct) => {
                    let signature = self.lower_function(
                        construct.type_parameters.as_deref(),
                        None,
                        &construct.params,
                        construct
                            .return_type
                            .as_deref()
                            .map(|annotation| &annotation.type_annotation),
                        construct.span,
                    );
                    if let Some(signature) = self.capture_child(signature, &mut failure) {
                        object.construct_signatures.push(signature);
                    }
                }
            }
        }
        if let Some(failure) = failure {
            return Err(failure);
        }
        for (name, signatures) in methods {
            let ty = if signatures.len() == 1 {
                signatures[0]
            } else {
                self.factory.intern_object(ObjectType {
                    call_signatures: signatures,
                    ..Default::default()
                })
            };
            object.properties.push(PropertyType::public(name, ty));
        }
        Ok(self.factory.intern_object(object))
    }

    fn lower_function(
        &mut self,
        type_parameters: Option<&TSTypeParameterDeclaration<'_>>,
        this_param: Option<&TSThisParameter<'_>>,
        parameters: &FormalParameters<'_>,
        return_type: Option<&TSType<'_>>,
        span: Span,
    ) -> SurfaceTypeResult<Ticket> {
        let syntax =
            self.lower_function_syntax(type_parameters, this_param, parameters, return_type, span);
        if let Some(failure) = syntax.failure {
            return Err(failure);
        }
        let params = syntax
            .params
            .into_iter()
            .map(|parameter| parameter.expect("successful callable retains every parameter"))
            .collect();
        let ret = syntax
            .declared_return
            .expect("successful callable retains its return type");
        Ok(self.factory.intern_function(FunctionType {
            type_params: syntax.type_params,
            receiver: syntax.receiver,
            params,
            ret,
        }))
    }

    fn lower_function_syntax(
        &mut self,
        type_parameters: Option<&TSTypeParameterDeclaration<'_>>,
        this_param: Option<&TSThisParameter<'_>>,
        parameters: &FormalParameters<'_>,
        return_type: Option<&TSType<'_>>,
        span: Span,
    ) -> LoweredCallableSyntax<Ticket> {
        let mut frame = FxHashMap::default();
        let mut ids = Vec::new();
        let mut binder_failure = None;
        if let Some(type_parameters) = type_parameters {
            ids.reserve(type_parameters.params.len());
            let reserved = type_parameters
                .params
                .iter()
                .enumerate()
                .map(|(ordinal, parameter)| {
                    let name = parameter.name.name.as_str();
                    (
                        name,
                        self.resolver
                            .signature_type_parameter(span.start, ordinal, name),
                    )
                })
                .collect::<Vec<_>>();
            if reserved.iter().any(|(_, id)| id.is_none()) {
                binder_failure = Some(self.unsupported(span));
                let error = self.factory.well_known().error;
                frame.extend(
                    reserved
                        .into_iter()
                        .map(|(name, _)| (name.to_string(), error)),
                );
            } else {
                for (name, id) in reserved
                    .into_iter()
                    .filter_map(|(name, id)| id.map(|id| (name, id)))
                {
                    let ty = self.factory.intern_type_param(id, name);
                    frame.insert(name.to_string(), ty);
                    ids.push(id);
                }
            }
        }
        self.type_param_frames.push(frame);
        let mut result =
            self.lower_function_inside(type_parameters, &ids, this_param, parameters, return_type);
        self.type_param_frames.pop();
        result.failure = binder_failure.or(result.failure);
        result
    }

    fn lower_function_inside(
        &mut self,
        type_parameters: Option<&TSTypeParameterDeclaration<'_>>,
        ids: &[TypeParamId],
        this_param: Option<&TSThisParameter<'_>>,
        parameters: &FormalParameters<'_>,
        return_type: Option<&TSType<'_>>,
    ) -> LoweredCallableSyntax<Ticket> {
        let mut generic = Vec::with_capacity(ids.len());
        let mut failure = None;
        if let Some(type_parameters) = type_parameters {
            for (parameter, id) in type_parameters.params.iter().zip(ids.iter().copied()) {
                let constraint = parameter.constraint.as_ref().and_then(|constraint| {
                    let lowered = self.lower_type(constraint);
                    self.capture_child(lowered, &mut failure)
                });
                let default = parameter.default.as_ref().and_then(|default| {
                    let lowered = self.lower_type(default);
                    self.capture_child(lowered, &mut failure)
                });
                generic.push(GenericTypeParam {
                    id,
                    constraint,
                    default,
                });
            }
        }
        let receiver = match this_param {
            Some(this_param) => {
                if let Some(annotation) = this_param.type_annotation.as_ref() {
                    let lowered = self.lower_type(&annotation.type_annotation);
                    self.capture_child(lowered, &mut failure)
                } else {
                    let unsupported = self.unsupported(this_param.span);
                    self.capture_failure(unsupported, &mut failure);
                    None
                }
            }
            None => None,
        };
        let mut params =
            Vec::with_capacity(parameters.items.len() + usize::from(parameters.rest.is_some()));
        for parameter in &parameters.items {
            let name = match &parameter.pattern {
                BindingPattern::BindingIdentifier(identifier) => Some(identifier.name.to_string()),
                _ => {
                    let unsupported = self.unsupported(parameter.span);
                    self.capture_failure(unsupported, &mut failure);
                    None
                }
            };
            let ty = if let Some(annotation) = parameter.type_annotation.as_ref() {
                let lowered = self.lower_type(&annotation.type_annotation);
                self.capture_child(lowered, &mut failure)
            } else {
                Some(self.factory.well_known().any)
            };
            params.push(match (name, ty) {
                (Some(name), Some(ty)) => Some(if parameter.initializer.is_some() {
                    ParameterType::defaulted(name, ty)
                } else if parameter.optional {
                    ParameterType::optional(name, ty)
                } else {
                    ParameterType::required(name, ty)
                }),
                _ => None,
            });
        }
        if let Some(rest) = &parameters.rest {
            let name = match &rest.rest.argument {
                BindingPattern::BindingIdentifier(identifier) => Some(identifier.name.to_string()),
                _ => {
                    let unsupported = self.unsupported(rest.span);
                    self.capture_failure(unsupported, &mut failure);
                    None
                }
            };
            let ty = if let Some(annotation) = rest.type_annotation.as_ref() {
                let lowered = self.lower_type(&annotation.type_annotation);
                self.capture_child(lowered, &mut failure)
            } else {
                Some(self.factory.well_known().any)
            };
            params.push(match (name, ty) {
                (Some(name), Some(ty)) => Some(ParameterType::rest(name, ty)),
                _ => None,
            });
        }
        let ret = match return_type {
            Some(return_type) => {
                let lowered = self.lower_type(return_type);
                self.capture_child(lowered, &mut failure)
            }
            None => Some(self.factory.well_known().void),
        };
        LoweredCallableSyntax {
            type_params: generic,
            receiver,
            params,
            declared_return: ret,
            failure,
        }
    }
}

fn reference_exhaustion<Ticket>(
    name: &str,
    span: Span,
    exhaustion: Exhaustion,
) -> SurfaceTypeFailure<Ticket> {
    match exhaustion {
        Exhaustion::ClassApplicationArguments(ClassApplicationArguments::WrongArity {
            expected_min,
            expected_max,
            ..
        }) => SurfaceTypeFailure::WrongArity {
            span,
            name: name.to_string(),
            expected_min,
            expected_max,
        },
        exhaustion => SurfaceTypeFailure::Exhausted(exhaustion),
    }
}

fn modifier(modifier: Option<TSMappedTypeModifierOperator>) -> ModifierOp {
    match modifier {
        None => ModifierOp::Keep,
        Some(TSMappedTypeModifierOperator::True) | Some(TSMappedTypeModifierOperator::Plus) => {
            ModifierOp::Add
        }
        Some(TSMappedTypeModifierOperator::Minus) => ModifierOp::Remove,
    }
}

#[cfg(test)]
mod tests {
    use super::super::application::ClassTypeParameterDefault;
    use super::*;
    use crate::types::Interner;
    use oxc_allocator::Allocator;
    use oxc_ast::ast::{ClassElement, Function, Statement};
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    #[derive(Default)]
    struct Resolver {
        names: FxHashMap<String, SurfaceNameResolution<u32>>,
        next_parameter: u32,
    }

    impl SurfaceTypeResolver<u32> for Resolver {
        fn resolve_name(&mut self, name: &str) -> SurfaceNameResolution<u32> {
            self.names
                .get(name)
                .cloned()
                .unwrap_or(SurfaceNameResolution::Unavailable(900))
        }

        fn signature_type_parameter(
            &mut self,
            _source_start: u32,
            _ordinal: usize,
            _name: &str,
        ) -> Option<TypeParamId> {
            let id = TypeParamId(self.next_parameter);
            self.next_parameter += 1;
            Some(id)
        }

        fn unsupported_ticket(&mut self, span: Span) -> u32 {
            span.start
        }
    }

    struct MissingCallableResolver;

    impl SurfaceTypeResolver<u32> for MissingCallableResolver {
        fn resolve_name(&mut self, _name: &str) -> SurfaceNameResolution<u32> {
            SurfaceNameResolution::Unavailable(900)
        }

        fn signature_type_parameter(
            &mut self,
            _source_start: u32,
            _ordinal: usize,
            _name: &str,
        ) -> Option<TypeParamId> {
            None
        }

        fn unsupported_ticket(&mut self, span: Span) -> u32 {
            span.start
        }
    }

    fn with_alias_type<R>(source: &str, check: impl FnOnce(&TSType<'_>) -> R) -> R {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let Statement::TSTypeAliasDeclaration(alias) = &parsed.program.body[0] else {
            panic!("expected a type alias");
        };
        check(&alias.type_annotation)
    }

    fn with_class_method<R>(source: &str, check: impl FnOnce(&Function<'_>) -> R) -> R {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let Statement::ClassDeclaration(class) = &parsed.program.body[0] else {
            panic!("expected a class declaration");
        };
        let ClassElement::MethodDefinition(method) = &class.body.body[0] else {
            panic!("expected a method definition");
        };
        check(&method.value)
    }

    fn class_parameter(id: u32) -> ClassTypeParameter<u32> {
        ClassTypeParameter {
            id: TypeParamId(id),
            default: ClassTypeParameterDefault::Absent,
        }
    }

    #[test]
    fn unannotated_identifier_parameters_lower_to_real_any_without_failure() {
        with_class_method("class C { method(value, ...rest) {} }", |function| {
            let mut interner = Interner::with_intrinsics();
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let any = factory.well_known().any;
            let mut resolver = Resolver::default();
            let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);

            let syntax = lowerer.lower_callable_syntax_with_type_parameters(
                function.type_parameters.as_deref(),
                function.this_param.as_deref(),
                &function.params,
                function
                    .return_type
                    .as_deref()
                    .map(|annotation| &annotation.type_annotation),
                function.span,
                std::iter::empty(),
            );

            assert_eq!(syntax.failure, None);
            assert!(lowerer.take_child_failures().is_empty());
            assert_eq!(syntax.params.len(), 2);
            assert_eq!(syntax.params[0].as_ref().map(|param| param.ty), Some(any));
            assert_eq!(syntax.params[1].as_ref().map(|param| param.ty), Some(any));
            assert!(!syntax.params[0].as_ref().unwrap().rest);
            assert!(syntax.params[1].as_ref().unwrap().rest);
        });
    }

    #[test]
    fn missing_nested_callable_binders_fail_without_interning_a_partial_signature() {
        with_alias_type("type C = { new<K, V>(): any }", |annotation| {
            let mut interner = Interner::with_intrinsics();
            let before = interner.store().len();
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let mut resolver = MissingCallableResolver;
            let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);

            assert!(matches!(
                lowerer.lower(annotation),
                Err(SurfaceTypeFailure::Unsupported(_))
            ));
            drop(lowerer);
            assert_eq!(factory.store().len(), before);
        });
    }

    #[test]
    fn class_reference_interns_only_a_complete_available_vector() {
        with_alias_type("type X = Box<number>;", |annotation| {
            let mut interner = Interner::with_intrinsics();
            let mut resolver = Resolver::default();
            resolver.names.insert(
                "Box".to_string(),
                SurfaceNameResolution::Class {
                    class: ClassId(1),
                    parameters: vec![class_parameter(1)],
                },
            );
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);
            let application = lowerer.lower(annotation).unwrap();
            let stored = factory.store().class_instance_type(application).unwrap();
            assert_eq!(stored.class, ClassId(1));
            assert_eq!(stored.args, [factory.well_known().number]);
        });
    }

    #[test]
    fn poisoned_declaration_never_becomes_a_composite_operand() {
        with_alias_type("type X = { value: Broken };", |annotation| {
            let mut interner = Interner::with_intrinsics();
            let mut resolver = Resolver::default();
            resolver
                .names
                .insert("Broken".to_string(), SurfaceNameResolution::Poisoned(41));
            let before = interner.store().len();
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);

            assert_eq!(
                lowerer.lower(annotation),
                Err(SurfaceTypeFailure::Poisoned(41))
            );
            assert_eq!(factory.store().len(), before);
        });
    }

    #[test]
    fn wrong_arity_precedes_unavailable_child_without_partial_instance() {
        with_alias_type("type X = Box<Missing, number>;", |annotation| {
            let mut interner = Interner::with_intrinsics();
            let mut resolver = Resolver::default();
            resolver.names.insert(
                "Box".to_string(),
                SurfaceNameResolution::Class {
                    class: ClassId(2),
                    parameters: vec![class_parameter(2)],
                },
            );
            resolver.names.insert(
                "Missing".to_string(),
                SurfaceNameResolution::Unavailable(77),
            );
            let before = interner.store().len();
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);
            assert!(matches!(
                lowerer.lower(annotation),
                Err(SurfaceTypeFailure::WrongArity {
                    expected_min: 1,
                    expected_max: 1,
                    ..
                })
            ));
            assert!(matches!(
                lowerer.take_child_failures().as_slice(),
                [SurfaceTypeFailure::Unresolved {
                    owner: 77,
                    name,
                    ..
                }] if name == "Missing"
            ));
            assert_eq!(factory.store().len(), before);
        });
    }

    #[test]
    fn failed_callable_visits_parameter_and_return_without_interning_a_function() {
        with_alias_type(
            "type X = (value: MissingParameter) => MissingReturn;",
            |annotation| {
                let mut interner = Interner::with_intrinsics();
                let mut resolver = Resolver::default();
                let before = interner.store().len();
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);

                assert!(matches!(
                    lowerer.lower(annotation),
                    Err(SurfaceTypeFailure::Unresolved { name, .. })
                        if name == "MissingParameter"
                ));
                assert!(matches!(
                    lowerer.take_child_failures().as_slice(),
                    [SurfaceTypeFailure::Unresolved { name, .. }]
                        if name == "MissingReturn"
                ));
                assert_eq!(factory.store().len(), before);
            },
        );
    }

    #[test]
    fn failed_composite_visits_every_child_without_interning_the_container() {
        with_alias_type("type X = MissingLeft | MissingRight;", |annotation| {
            let mut interner = Interner::with_intrinsics();
            let mut resolver = Resolver::default();
            let before = interner.store().len();
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);

            assert!(matches!(
                lowerer.lower(annotation),
                Err(SurfaceTypeFailure::Unresolved { name, .. }) if name == "MissingLeft"
            ));
            assert!(matches!(
                lowerer.take_child_failures().as_slice(),
                [SurfaceTypeFailure::Unresolved { name, .. }] if name == "MissingRight"
            ));
            assert_eq!(factory.store().len(), before);
        });
    }

    #[test]
    fn conditional_keyof_and_indexed_access_stay_raw() {
        with_alias_type(
            "type X = number extends string ? keyof Box<number> : Box<number>[\"x\"];",
            |annotation| {
                let mut interner = Interner::with_intrinsics();
                let mut resolver = Resolver::default();
                resolver.names.insert(
                    "Box".to_string(),
                    SurfaceNameResolution::Class {
                        class: ClassId(3),
                        parameters: vec![class_parameter(3)],
                    },
                );
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);
                let root = lowerer.lower(annotation).unwrap();
                assert_eq!(factory.store().tag(root), TypeTag::Conditional);
                let conditional = factory.store().conditional_type(root).unwrap();
                assert_eq!(factory.store().tag(conditional.true_branch), TypeTag::Keyof);
                assert_eq!(
                    factory.store().tag(conditional.false_branch),
                    TypeTag::DeferredIndexedAccess
                );
            },
        );
    }

    #[test]
    fn mapped_source_and_value_placeholder_are_not_evaluated() {
        with_alias_type(
            "type X = { [K in keyof Box<number>]: Box<number>[K] };",
            |annotation| {
                let mut interner = Interner::with_intrinsics();
                let mut resolver = Resolver::default();
                resolver.names.insert(
                    "Box".to_string(),
                    SurfaceNameResolution::Class {
                        class: ClassId(4),
                        parameters: vec![class_parameter(4)],
                    },
                );
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);
                let root = lowerer.lower(annotation).unwrap();
                let mapped = factory.store().mapped_type(root).unwrap();
                assert!(mapped.homomorphic);
                assert_eq!(
                    factory.store().tag(mapped.key_source),
                    TypeTag::ClassInstance
                );
                assert_eq!(
                    factory.store().tag(mapped.value_template),
                    TypeTag::MappedValue
                );
            },
        );
    }

    #[test]
    fn alias_application_remains_symbolic() {
        with_alias_type("type X = Alias<number>;", |annotation| {
            let mut interner = Interner::with_intrinsics();
            let template = interner.intern_keyof(interner.well_known().string);
            let mut resolver = Resolver::default();
            resolver.names.insert(
                "Alias".to_string(),
                SurfaceNameResolution::Alias {
                    template,
                    parameters: vec![TypeParamId(8)],
                },
            );
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);
            let root = lowerer.lower(annotation).unwrap();
            assert_eq!(factory.store().tag(root), TypeTag::Instantiation);
            assert_eq!(
                factory.store().instantiation_type(root).unwrap().base,
                template
            );
        });
    }

    #[test]
    fn template_holes_remain_symbolic_class_applications() {
        with_alias_type("type X = `a${Box<number>}b`;", |annotation| {
            let mut interner = Interner::with_intrinsics();
            let mut resolver = Resolver::default();
            resolver.names.insert(
                "Box".to_string(),
                SurfaceNameResolution::Class {
                    class: ClassId(5),
                    parameters: vec![class_parameter(5)],
                },
            );
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            let mut lowerer = TypeSyntaxLowerer::new(&mut factory, &mut resolver);
            let root = lowerer.lower(annotation).unwrap();
            let template = factory.store().template_type(root).unwrap();
            assert_eq!(template.texts, ["a", "b"]);
            assert_eq!(
                factory.store().tag(template.holes[0]),
                TypeTag::ClassInstance
            );
        });
    }
}
