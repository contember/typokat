//! Direct compilation-wide class-surface lowering and publication.

use super::super::type_groups::PublishedTypeParameterDefault;
use super::application::{
    build_open_class_application, complete_class_arguments, ClassApplicationKind,
    ClassApplicationRequest, ClassTypeParameter, ClassTypeParameterDefault, ExplicitClassArgument,
    SourceClassArguments,
};
use super::body::{BodyClassView, BodyMemberMetadata};
use super::construction::{
    ClassConstruction, ClassRecoveryOrder, ClassSurfaceLowerer, DraftClassTypeParameter,
    HeritageDependency, ReservedRootKind,
};
use super::initializer::{
    EarlierMethodSurface, InitializerInference, SurfaceInitializerContext,
    SurfaceInitializerInferer,
};
use super::retained::{
    CallableOverloadRole, RetainedCallableTypeParameter, RetainedClassCallable,
    RetainedParameterProperty,
};
use super::surface_types::SurfaceTypeFactory;
use super::type_syntax::{
    LoweredCallableSyntax, SurfaceNameResolution, SurfaceTypeFailure, SurfaceTypeResolver,
    TypeSyntaxLowerer,
};
use super::visibility::{has_public_constructor, lower_visibility};
use crate::binder::declaration::DeclarationKind;
use crate::binder::scope::ScopeId;
use crate::check::checker::context::{
    CheckerEffects, ClassNamespacePropertyPayload, ClassNamespacePropertySourceOrder,
    ConstraintCheckObligation, OverrideCheck, Pass, PublishedClassNewMetadata, TypeDecl,
};
use crate::check::checker::decls::type_decl_id;
use crate::check::checker::events::UserRecordTicket;
use crate::check::checker::lexical_events::{ClassReservation, LexicalReservations};
use crate::check::checker::reporting_record::CheckerRecord;
use crate::check::query::SemanticQueryCoordinator;
use crate::class_semantics::{ClassApplicationArguments, DemandOutcome, Exhaustion};
use crate::diagnostics::{render_type, Diagnostic, IncompleteSurface};
use crate::relate::RelationOutcome;
use crate::source::ModuleOrdinal;
use crate::span::Span as CheckSpan;
use crate::types::repr::{
    ClassId, FunctionType, ObjectType, PropertyType, TypeParamId, Visibility,
};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Expression, MethodDefinitionKind, MethodDefinitionType,
    PropertyDefinitionType, PropertyKey, TSAccessibility, TSType, TSTypeName,
};
use oxc_span::{GetSpan, Span};
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};

fn class_member_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::PrivateIdentifier(identifier) => Some(format!("#{}", identifier.name)),
        _ => key.static_name().map(|name| name.into_owned()),
    }
}

fn class_instance_method_names(class: &Class<'_>) -> BTreeSet<String> {
    class
        .body
        .body
        .iter()
        .filter_map(|element| match element {
            ClassElement::MethodDefinition(method)
                if !method.r#static
                    && !method.computed
                    && method.kind == MethodDefinitionKind::Method =>
            {
                class_member_name(&method.key)
            }
            _ => None,
        })
        .collect()
}

#[derive(Clone)]
struct ExplicitStaticMember {
    kind: ExplicitStaticMemberKind,
    owner: UserRecordTicket,
    span: CheckSpan,
    source_order: ClassNamespacePropertySourceOrder,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum ExplicitStaticMemberKind {
    Property,
    Method,
    Getter,
    Setter,
    AccessorProperty,
}

fn explicit_static_member(
    element: &ClassElement<'_>,
) -> Option<(String, CheckSpan, ExplicitStaticMemberKind)> {
    match element {
        ClassElement::MethodDefinition(method)
            if method.r#static && method.kind != MethodDefinitionKind::Constructor =>
        {
            Some((
                class_member_name(&method.key)?,
                CheckSpan::from_oxc(method.span),
                match method.kind {
                    MethodDefinitionKind::Method => ExplicitStaticMemberKind::Method,
                    MethodDefinitionKind::Get => ExplicitStaticMemberKind::Getter,
                    MethodDefinitionKind::Set => ExplicitStaticMemberKind::Setter,
                    MethodDefinitionKind::Constructor => unreachable!(),
                },
            ))
        }
        ClassElement::PropertyDefinition(property) if property.r#static => Some((
            class_member_name(&property.key)?,
            CheckSpan::from_oxc(property.span),
            ExplicitStaticMemberKind::Property,
        )),
        ClassElement::AccessorProperty(property) if property.r#static => Some((
            class_member_name(&property.key)?,
            CheckSpan::from_oxc(property.span),
            ExplicitStaticMemberKind::AccessorProperty,
        )),
        _ => None,
    }
}

fn merge_class_owned_fragment(
    factory: &mut SurfaceTypeFactory<'_>,
    base: ObjectType,
    overlay: ObjectType,
    first_method_members: &mut BTreeSet<String>,
    overlay_methods: &BTreeSet<String>,
) -> ObjectType {
    let mut properties = base.properties;
    for property in overlay.properties {
        let Some(existing) = properties
            .iter_mut()
            .find(|existing| existing.name == property.name)
        else {
            if overlay_methods.contains(&property.name) {
                first_method_members.insert(property.name.clone());
            }
            properties.push(property);
            continue;
        };
        if !first_method_members.contains(&property.name)
            || !overlay_methods.contains(&property.name)
        {
            continue;
        }
        let mut overloads = match factory.store().tag(existing.ty) {
            crate::types::repr::TypeTag::Function => vec![existing.ty],
            crate::types::repr::TypeTag::Object => factory
                .store()
                .object_type(existing.ty)
                .filter(|object| !object.call_signatures.is_empty())
                .map(|object| object.call_signatures.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let appended = match factory.store().tag(property.ty) {
            crate::types::repr::TypeTag::Function => Some(vec![property.ty]),
            crate::types::repr::TypeTag::Object => factory
                .store()
                .object_type(property.ty)
                .filter(|object| !object.call_signatures.is_empty())
                .map(|object| object.call_signatures.clone()),
            _ => None,
        };
        if !overloads.is_empty() {
            if let Some(appended) = appended {
                overloads.extend(appended);
                existing.ty = factory.intern_object(ObjectType {
                    call_signatures: overloads,
                    ..Default::default()
                });
            }
        }
    }
    let mut call_signatures = base.call_signatures;
    call_signatures.extend(overlay.call_signatures);
    let mut construct_signatures = base.construct_signatures;
    construct_signatures.extend(overlay.construct_signatures);
    ObjectType {
        properties,
        string_index: base.string_index.or(overlay.string_index),
        number_index: base.number_index.or(overlay.number_index),
        call_signatures,
        construct_signatures,
    }
}

fn overlay_class_owned_members(base: ObjectType, own: ObjectType) -> ObjectType {
    let mut properties = base.properties;
    for property in own.properties {
        match properties
            .iter_mut()
            .find(|existing| existing.name == property.name)
        {
            Some(existing) => *existing = property,
            None => properties.push(property),
        }
    }
    let mut call_signatures = own.call_signatures;
    call_signatures.extend(base.call_signatures);
    let mut construct_signatures = own.construct_signatures;
    construct_signatures.extend(base.construct_signatures);
    ObjectType {
        properties,
        string_index: own.string_index.or(base.string_index),
        number_index: own.number_index.or(base.number_index),
        call_signatures,
        construct_signatures,
    }
}

fn class_member_visibility(
    key: &PropertyKey<'_>,
    accessibility: Option<TSAccessibility>,
) -> Visibility {
    if key.is_private_identifier() {
        Visibility::Private
    } else {
        lower_visibility(accessibility)
    }
}

struct TicketRecord {
    owner: UserRecordTicket,
    source_start: u32,
    record: CheckerRecord,
}

pub(in crate::check::checker) struct StagedClassValidation {
    records: Vec<TicketRecord>,
    default_checks: Vec<(UserRecordTicket, TypeId, TypeId, CheckSpan)>,
    application_checks: Vec<StagedClassApplicationCheck>,
}

struct StagedClassApplicationCheck {
    owner: UserRecordTicket,
    parameters: Vec<TypeParamId>,
    arguments: Vec<TypeId>,
    explicit_spans: Vec<CheckSpan>,
}

impl TicketRecord {
    fn diagnostic(owner: UserRecordTicket, diagnostic: Diagnostic) -> Self {
        Self {
            owner,
            source_start: diagnostic.span.start,
            record: CheckerRecord::Diagnostic(diagnostic),
        }
    }

    fn incomplete(owner: UserRecordTicket, incomplete: IncompleteSurface) -> Self {
        Self {
            owner,
            source_start: incomplete.span.start,
            record: CheckerRecord::Incomplete(incomplete),
        }
    }
}

struct Resolver<'a, 'ast> {
    binder: &'a crate::binder::Binder,
    scope: ScopeId,
    declarations: &'a [TypeDecl<'ast>],
    resolved: &'a [Option<TypeId>],
    reservations: &'a LexicalReservations,
    module: ModuleOrdinal,
    fallback: UserRecordTicket,
    error: TypeId,
    qualified_outer_type_parameters_visible: bool,
    application_checks: Vec<StagedClassApplicationCheck>,
}

impl Resolver<'_, '_> {
    fn class_default_owner(&self, class: ClassId, index: usize) -> UserRecordTicket {
        self.reservations
            .classes()
            .iter()
            .find(|reservation| {
                reservation
                    .binding
                    .as_ref()
                    .is_some_and(|binding| binding.class_id == class)
            })
            .and_then(|reservation| {
                reservation
                    .defaults
                    .iter()
                    .find(|default| default.parameter_index == index)
            })
            .map_or(self.fallback, |default| default.owner)
    }

    fn class_parameters(
        &self,
        declaration: &TypeDecl<'_>,
    ) -> Vec<ClassTypeParameter<UserRecordTicket>> {
        let TypeDecl::Class {
            class_id,
            params,
            recovery_defaults,
            param_decl,
            interfaces,
            ..
        } = declaration
        else {
            return Vec::new();
        };
        params
            .iter()
            .enumerate()
            .map(|(index, id)| ClassTypeParameter {
                id: *id,
                default: match if interfaces.is_empty() {
                    param_decl
                        .and_then(|declaration| declaration.params.get(index))
                        .and_then(|parameter| parameter.default.as_ref())
                        .map_or(PublishedTypeParameterDefault::Absent, |_| {
                            PublishedTypeParameterDefault::Unsupported
                        })
                } else {
                    recovery_defaults
                        .get(index)
                        .copied()
                        .unwrap_or(PublishedTypeParameterDefault::Absent)
                } {
                    PublishedTypeParameterDefault::Absent => ClassTypeParameterDefault::Absent,
                    PublishedTypeParameterDefault::Ready(default) => {
                        ClassTypeParameterDefault::Ready(default)
                    }
                    PublishedTypeParameterDefault::Unsupported => {
                        ClassTypeParameterDefault::Unsupported(
                            self.class_default_owner(*class_id, index),
                        )
                    }
                },
            })
            .collect()
    }

    fn alias_parameters(
        &self,
        params: &[TypeParamId],
        defaults: &[Option<TypeId>],
        declaration: Option<&oxc_ast::ast::TSTypeParameterDeclaration<'_>>,
    ) -> Vec<ClassTypeParameter<UserRecordTicket>> {
        params
            .iter()
            .enumerate()
            .map(|(index, id)| ClassTypeParameter {
                id: *id,
                default: declaration
                    .and_then(|declaration| declaration.params.get(index))
                    .and_then(|parameter| parameter.default.as_ref())
                    .map_or(ClassTypeParameterDefault::Absent, |_| {
                        defaults.get(index).copied().flatten().map_or(
                            ClassTypeParameterDefault::Unsupported(self.fallback),
                            ClassTypeParameterDefault::Ready,
                        )
                    }),
            })
            .collect()
    }

    fn interface_parameters(
        &self,
        params: &[TypeParamId],
        defaults: &[PublishedTypeParameterDefault],
    ) -> Vec<ClassTypeParameter<UserRecordTicket>> {
        params
            .iter()
            .enumerate()
            .map(|(index, id)| ClassTypeParameter {
                id: *id,
                default: match defaults
                    .get(index)
                    .copied()
                    .unwrap_or(PublishedTypeParameterDefault::Absent)
                {
                    PublishedTypeParameterDefault::Absent => ClassTypeParameterDefault::Absent,
                    PublishedTypeParameterDefault::Ready(default) => {
                        ClassTypeParameterDefault::Ready(default)
                    }
                    PublishedTypeParameterDefault::Unsupported => {
                        ClassTypeParameterDefault::Unsupported(self.fallback)
                    }
                },
            })
            .collect()
    }

    fn resolve_group(
        &self,
        id: crate::binder::declaration::TypeGroupId,
    ) -> SurfaceNameResolution<UserRecordTicket> {
        let Some(declaration) = self.declarations.get(id.index()) else {
            return SurfaceNameResolution::Unavailable(self.fallback);
        };
        match declaration {
            TypeDecl::Class { class_id, .. } => SurfaceNameResolution::Class {
                class: *class_id,
                parameters: self.class_parameters(declaration),
            },
            TypeDecl::Interface {
                reserved,
                recovery_params,
                recovery_defaults,
                ..
            } => {
                if recovery_params.is_empty() {
                    SurfaceNameResolution::Direct(*reserved)
                } else {
                    SurfaceNameResolution::Alias {
                        template: *reserved,
                        parameters: self.interface_parameters(recovery_params, recovery_defaults),
                    }
                }
            }
            TypeDecl::Alias {
                params,
                defaults,
                param_decl,
                ..
            } => {
                let Some(template) = self.resolved.get(id.index()).copied().flatten() else {
                    return SurfaceNameResolution::Poisoned(self.fallback);
                };
                if template == self.error {
                    return SurfaceNameResolution::Poisoned(self.fallback);
                }
                if params.is_empty() {
                    SurfaceNameResolution::Direct(template)
                } else {
                    SurfaceNameResolution::Alias {
                        template,
                        parameters: self.alias_parameters(params, defaults, *param_decl),
                    }
                }
            }
            TypeDecl::Resolved { params } => {
                let Some(template) = self.resolved.get(id.index()).copied().flatten() else {
                    return SurfaceNameResolution::Poisoned(self.fallback);
                };
                if template == self.error {
                    return SurfaceNameResolution::Poisoned(self.fallback);
                }
                if params.is_empty() {
                    SurfaceNameResolution::Direct(template)
                } else {
                    SurfaceNameResolution::Alias {
                        template,
                        parameters: params
                            .iter()
                            .map(|id| ClassTypeParameter {
                                id: *id,
                                default: ClassTypeParameterDefault::Absent,
                            })
                            .collect(),
                    }
                }
            }
            TypeDecl::Unavailable { .. } => SurfaceNameResolution::FoundUnavailable(self.fallback),
        }
    }
}

impl SurfaceTypeResolver<UserRecordTicket> for Resolver<'_, '_> {
    fn resolve_name(&mut self, name: &str) -> SurfaceNameResolution<UserRecordTicket> {
        let Some(id) = type_decl_id(self.binder, self.scope, name) else {
            return SurfaceNameResolution::Unavailable(self.fallback);
        };
        self.resolve_group(id)
    }

    fn resolve_qualified_name(
        &mut self,
        segments: &[&str],
    ) -> SurfaceNameResolution<UserRecordTicket> {
        let resolution = self
            .binder
            .resolve_qualified_type_path(self.scope, segments);
        if let crate::binder::namespace::QualifiedTypePathResolution::TypeGroup(group) = resolution
        {
            let endpoint = self.resolve_group(group);
            if matches!(
                endpoint,
                SurfaceNameResolution::Direct(_)
                    | SurfaceNameResolution::Alias { .. }
                    | SurfaceNameResolution::Class { .. }
                    | SurfaceNameResolution::FoundUnavailable(_)
            ) {
                return endpoint;
            }
        }
        SurfaceNameResolution::Qualified {
            owner: self.fallback,
            resolution,
        }
    }

    fn qualified_outer_type_parameters_visible(&self) -> bool {
        self.qualified_outer_type_parameters_visible
    }

    fn record_type_argument_constraints(
        &mut self,
        parameters: &[TypeParamId],
        arguments: &[TypeId],
        explicit_spans: &[CheckSpan],
    ) {
        if explicit_spans.is_empty() {
            return;
        }
        self.application_checks.push(StagedClassApplicationCheck {
            owner: self.fallback,
            parameters: parameters.to_vec(),
            arguments: arguments.to_vec(),
            explicit_spans: explicit_spans.to_vec(),
        });
    }

    fn signature_type_parameter(
        &mut self,
        callable_source_start: u32,
        ordinal: usize,
        _name: &str,
    ) -> Option<TypeParamId> {
        self.reservations
            .callable_at(self.module, callable_source_start)
            .and_then(|site| self.reservations.callable(site))
            .and_then(|reservation| reservation.binding.as_ref())
            .and_then(|binding| binding.type_params.get(ordinal))
            .copied()
    }

    fn unsupported_ticket(&mut self, _span: Span) -> UserRecordTicket {
        self.fallback
    }
}

#[derive(Default)]
struct InitializerContext {
    annotations: FxHashMap<u32, TypeId>,
    fields: FxHashMap<String, TypeId>,
    methods: FxHashMap<String, EarlierMethodSurface>,
}

impl SurfaceInitializerContext for InitializerContext {
    fn lower_annotation(&mut self, annotation: &TSType<'_>) -> DemandOutcome<TypeId> {
        self.annotations
            .get(&annotation.span().start)
            .copied()
            .map_or_else(
                || DemandOutcome::Exhausted(crate::class_semantics::Exhaustion::EvaluationBudget),
                DemandOutcome::Ready,
            )
    }

    fn earlier_field(&self, name: &str) -> Option<TypeId> {
        self.fields.get(name).copied()
    }

    fn earlier_method(&self, name: &str) -> Option<EarlierMethodSurface> {
        self.methods.get(name).copied()
    }
}

impl<'a, 'ast> Pass<'a, 'ast> {
    pub(in crate::check::checker) fn install_class_namespace_payload(
        &mut self,
        group: crate::binder::declaration::TypeGroupId,
        properties: Vec<ClassNamespacePropertyPayload>,
    ) -> bool {
        self.class_namespace_payloads
            .insert(group, properties)
            .is_none()
    }

    pub(in crate::check::checker) fn lower_namespace_type_surface(
        &mut self,
        scope: ScopeId,
        annotation: &TSType<'_>,
        owner: UserRecordTicket,
    ) -> (
        Result<TypeId, SurfaceTypeFailure<UserRecordTicket>>,
        Vec<SurfaceTypeFailure<UserRecordTicket>>,
    ) {
        let error = self.interner.well_known().error;
        let type_decls = self.type_decls.clone();
        let type_resolved = self.type_resolved.clone();
        let (result, child_failures, application_checks) = {
            let mut resolver = Resolver {
                binder: self.binder,
                scope,
                declarations: &type_decls,
                resolved: &type_resolved,
                reservations: &self.lexical_events,
                module: self.current_module_ordinal,
                fallback: owner,
                error,
                qualified_outer_type_parameters_visible: true,
                application_checks: Vec::new(),
            };
            let mut factory = SurfaceTypeFactory::new(self.interner);
            let (result, child_failures) = lower_type(&mut factory, &mut resolver, annotation, &[]);
            (result, child_failures, resolver.application_checks)
        };
        self.stage_namespace_surface_application_checks(application_checks);
        (result, child_failures)
    }

    pub(in crate::check::checker) fn lower_namespace_callable_surface(
        &mut self,
        scope: ScopeId,
        function: &oxc_ast::ast::Function<'_>,
        owner: UserRecordTicket,
    ) -> (
        LoweredCallableSyntax<UserRecordTicket>,
        Vec<SurfaceTypeFailure<UserRecordTicket>>,
    ) {
        let error = self.interner.well_known().error;
        let type_decls = self.type_decls.clone();
        let type_resolved = self.type_resolved.clone();
        let (result, child_failures, application_checks) = {
            let mut resolver = Resolver {
                binder: self.binder,
                scope,
                declarations: &type_decls,
                resolved: &type_resolved,
                reservations: &self.lexical_events,
                module: self.current_module_ordinal,
                fallback: owner,
                error,
                qualified_outer_type_parameters_visible: true,
                application_checks: Vec::new(),
            };
            let mut factory = SurfaceTypeFactory::new(self.interner);
            let (result, child_failures) =
                lower_callable(&mut factory, &mut resolver, function, &[]);
            (result, child_failures, resolver.application_checks)
        };
        self.stage_namespace_surface_application_checks(application_checks);
        (result, child_failures)
    }

    fn stage_namespace_surface_application_checks(
        &mut self,
        application_checks: Vec<StagedClassApplicationCheck>,
    ) {
        for check in application_checks {
            let substitutions = check
                .parameters
                .iter()
                .copied()
                .zip(check.arguments.iter().copied())
                .collect();
            let checks = check
                .parameters
                .iter()
                .zip(&check.arguments)
                .zip(&check.explicit_spans)
                .map(|((&parameter, &argument), &span)| {
                    (
                        self.interner.store().type_param_constraint(parameter),
                        argument,
                        span,
                    )
                })
                .collect();
            self.effect_stack
                .last_mut()
                .expect("namespace surface constraint requires a lexical owner")
                .constraint_checks
                .push(ConstraintCheckObligation {
                    checks,
                    substitutions,
                });
        }
    }

    pub(in crate::check::checker) fn record_namespace_surface_failure(
        &mut self,
        failure: SurfaceTypeFailure<UserRecordTicket>,
        owner: UserRecordTicket,
        span: CheckSpan,
    ) {
        let (record_owner, record) = own_surface_failure(
            failure,
            owner,
            owner,
            span,
            "annotation-lower/type-syntax/unsupported",
            "namespace member type syntax could not be lowered",
        );
        let Some(record) = record else {
            return;
        };
        self.with_ticket_effects(record_owner, |pass| match record.record {
            CheckerRecord::Diagnostic(diagnostic) => pass.emit_diagnostic(diagnostic),
            CheckerRecord::Incomplete(incomplete) => pass.emit_incomplete(incomplete),
        });
    }

    pub(in crate::check::checker) fn publish_class_surfaces(
        &mut self,
        _scopes: &[(ModuleOrdinal, ScopeId)],
    ) {
        let prepared_interface_groups = self.prepare_class_interface_groups();
        let class_groups: Vec<crate::binder::declaration::TypeGroupId> = self
            .type_decls
            .iter()
            .enumerate()
            .filter(|(_, declaration)| matches!(declaration, TypeDecl::Class { .. }))
            .map(|(index, _)| {
                crate::binder::declaration::TypeGroupId(
                    u32::try_from(index).expect("type group index fits u32"),
                )
            })
            .collect();
        for group in &class_groups {
            self.begin_type_group_construction(*group);
        }
        let type_decls = self.type_decls.clone();
        let type_resolved = self.type_resolved.clone();
        let reservations: Vec<ClassReservation> = self.lexical_events.classes().to_vec();
        let mut construction = ClassConstruction::default();
        let mut default_checks = Vec::new();
        let mut application_checks = Vec::new();
        let mut records = Vec::new();
        let mut heritage_spans = BTreeMap::new();
        let mut class_conflict_surfaces = BTreeMap::new();

        {
            let error = self.interner.well_known().error;
            let mut factory = SurfaceTypeFactory::new(self.interner);
            register_reserved_surface_roots(
                &mut construction,
                &mut factory,
                self.binder,
                &type_decls,
                &type_resolved,
                &self.lexical_events,
                error,
            );
        }

        for reservation in reservations {
            let Some(binding) = reservation.binding.as_ref() else {
                continue;
            };
            let Some(TypeDecl::Class {
                declaration,
                scope,
                class_id,
                params,
                class_params,
                recovery_names,
                recovery_defaults,
                param_decl,
                class,
                interfaces,
                ..
            }) = type_decls.get(binding.type_decl.index())
            else {
                continue;
            };
            let scope = *scope;
            let declaration = *declaration;
            let class_id = *class_id;
            let params = params.clone();
            let class_params = class_params.clone();
            let recovery_names = recovery_names.clone();
            let recovery_defaults = recovery_defaults.clone();
            let merged_header = !interfaces.is_empty();
            let class = *class;
            let class_source = self
                .binder
                .type_groups
                .get(binding.type_decl)
                .and_then(|group| {
                    group
                        .fragments
                        .iter()
                        .find(|fragment| fragment.declaration == declaration)
                })
                .map_or(
                    crate::binder::namespace::SourceUnitKey::SINGLE_SOURCE,
                    |fragment| fragment.source,
                );
            let mut explicit_static_members: BTreeMap<String, Vec<ExplicitStaticMember>> =
                BTreeMap::new();
            for (index, element) in class.body.body.iter().enumerate() {
                let Some((name, span, kind)) = explicit_static_member(element) else {
                    continue;
                };
                let owner = reservation
                    .members
                    .get(index)
                    .and_then(|member| self.lexical_events.member(*member))
                    .map_or(reservation.tickets.immediate, |member| {
                        member.tickets.immediate
                    });
                let candidate = ExplicitStaticMember {
                    kind,
                    owner,
                    span,
                    source_order: ClassNamespacePropertySourceOrder {
                        source: class_source,
                        source_start: span.start,
                        declaration_ordinal: declaration.0,
                    },
                };
                explicit_static_members
                    .entry(name)
                    .or_default()
                    .push(candidate);
            }
            for members in explicit_static_members.values_mut() {
                members.sort_by_key(|member| member.source_order);
            }
            for implements in &class.implements {
                records.push(TicketRecord::incomplete(
                    reservation.tickets.incomplete,
                    IncompleteSurface::new(
                        "class/implements-clause/self",
                        CheckSpan::from_oxc(implements.span),
                        "implements clause is not checked",
                    ),
                ));
            }
            if let Some(heritage) = class.super_class.as_ref() {
                heritage_spans.insert(
                    reservation.tickets.deferred,
                    CheckSpan::from_oxc(heritage.span()),
                );
            }

            let error = self.interner.well_known().error;
            let mut resolver = Resolver {
                binder: self.binder,
                scope,
                declarations: &type_decls,
                resolved: &type_resolved,
                reservations: &self.lexical_events,
                module: reservation.source.module_ordinal,
                fallback: reservation.tickets.incomplete,
                error,
                qualified_outer_type_parameters_visible: true,
                application_checks: Vec::new(),
            };
            let mut class_conflict_surface = None;
            let mut class_heritage_conflict_surface = None;
            {
                let mut factory = SurfaceTypeFactory::new(self.interner);
                let frame: Vec<(String, TypeId)> = param_decl
                    .iter()
                    .flat_map(|declaration| declaration.params.iter())
                    .zip(class_params.iter().copied())
                    .map(|(parameter, id)| {
                        (
                            parameter.name.name.to_string(),
                            factory.intern_type_param(id, parameter.name.name.as_str()),
                        )
                    })
                    .collect();
                let application_types = params
                    .iter()
                    .copied()
                    .zip(recovery_names.iter())
                    .map(|(id, name)| factory.intern_type_param(id, name))
                    .collect::<Vec<_>>();

                let mut publication_surface_poison = Vec::new();
                let mut constraints = if merged_header {
                    params
                        .iter()
                        .map(|parameter| factory.store().type_param_constraint(*parameter))
                        .collect::<Vec<_>>()
                } else {
                    Vec::with_capacity(params.len())
                };
                if !merged_header {
                    if let Some(declaration) = param_decl {
                        for (index, parameter) in declaration.params.iter().enumerate() {
                            let constraint_ty =
                                if let Some(constraint) = parameter.constraint.as_ref() {
                                    let owner = reservation
                                        .constraints
                                        .iter()
                                        .find(|reserved| reserved.parameter_index == index)
                                        .map_or(reservation.tickets.immediate, |reserved| {
                                            reserved.owner
                                        });
                                    resolver.fallback = owner;
                                    resolver.qualified_outer_type_parameters_visible = true;
                                    let (lowered, child_failures) =
                                        lower_type(&mut factory, &mut resolver, constraint, &frame);
                                    let constraint_ty = lowered.as_ref().ok().copied();
                                    for failure in lowered.err().into_iter().chain(child_failures) {
                                        let recovered_topology = matches!(
                                            &failure,
                                            SurfaceTypeFailure::QualifiedTopology { .. }
                                        );
                                        let (owner, record) = own_surface_failure(
                                            failure,
                                            owner,
                                            owner,
                                            CheckSpan::from_oxc(constraint.span()),
                                            "annotation-lower/type-parameter-constraint/self",
                                            "class type-parameter constraint could not be lowered",
                                        );
                                        if let Some(record) = record {
                                            records.push(record);
                                        }
                                        if !recovered_topology {
                                            publication_surface_poison.push(owner);
                                        }
                                    }
                                    constraint_ty
                                } else {
                                    None
                                };
                            constraints.push(constraint_ty);
                            if let Some(default) = parameter.default.as_ref() {
                                let owner = reservation
                                    .defaults
                                    .iter()
                                    .find(|reserved| reserved.parameter_index == index)
                                    .map_or(reservation.tickets.incomplete, |reserved| {
                                        reserved.owner
                                    });
                                resolver.fallback = owner;
                                resolver.qualified_outer_type_parameters_visible = true;
                                let (lowered, child_failures) =
                                    lower_type(&mut factory, &mut resolver, default, &frame);
                                for failure in lowered.err().into_iter().chain(child_failures) {
                                    if !matches!(
                                        &failure,
                                        SurfaceTypeFailure::QualifiedTopology { .. }
                                            | SurfaceTypeFailure::QualifiedIncomplete { .. }
                                    ) {
                                        continue;
                                    }
                                    let (_, record) = own_surface_failure(
                                        failure,
                                        owner,
                                        owner,
                                        CheckSpan::from_oxc(default.span()),
                                        "annotation-lower/type-parameter-default/self",
                                        "class type-parameter default could not be lowered",
                                    );
                                    if let Some(record) = record {
                                        records.push(record);
                                    }
                                }
                                records.push(TicketRecord::incomplete(
                                    owner,
                                    IncompleteSurface::new(
                                        "annotation-lower/type-parameter-default/self",
                                        CheckSpan::from_oxc(default.span()),
                                        "type-parameter default not lowered",
                                    ),
                                ));
                            }
                        }
                    }
                }
                for (id, constraint) in params.iter().copied().zip(constraints.iter().copied()) {
                    if let Some(constraint) = constraint {
                        if merged_header {
                            continue;
                        }
                        assert!(
                            factory.set_type_param_constraint(id, constraint),
                            "class type-parameter constraint is assigned exactly once"
                        );
                    }
                }

                let type_parameters: Vec<DraftClassTypeParameter<UserRecordTicket>> =
                    if merged_header {
                        params
                            .iter()
                            .enumerate()
                            .map(|(index, id)| {
                                let default = match recovery_defaults
                                    .get(index)
                                    .copied()
                                    .unwrap_or(PublishedTypeParameterDefault::Absent)
                                {
                                    PublishedTypeParameterDefault::Absent => {
                                        ClassTypeParameterDefault::Absent
                                    }
                                    PublishedTypeParameterDefault::Ready(default) => {
                                        ClassTypeParameterDefault::Ready(default)
                                    }
                                    PublishedTypeParameterDefault::Unsupported => {
                                        ClassTypeParameterDefault::Unsupported(
                                            reservation.tickets.incomplete,
                                        )
                                    }
                                };
                                DraftClassTypeParameter::merged(
                                    *id,
                                    constraints.get(index).copied().flatten(),
                                    default,
                                )
                            })
                            .collect()
                    } else {
                        params
                            .iter()
                            .enumerate()
                            .map(|(index, id)| {
                                DraftClassTypeParameter::source(
                                    *id,
                                    constraints.get(index).copied().flatten(),
                                    param_decl
                                        .and_then(|declaration| declaration.params.get(index))
                                        .and_then(|parameter| parameter.default.as_ref())
                                        .map(|_| {
                                            reservation
                                                .defaults
                                                .iter()
                                                .find(|default| default.parameter_index == index)
                                                .map_or(reservation.tickets.incomplete, |default| {
                                                    default.owner
                                                })
                                        }),
                                )
                            })
                            .collect()
                    };
                let application_parameters = type_parameters
                    .iter()
                    .map(|parameter: &DraftClassTypeParameter<UserRecordTicket>| {
                        *parameter.application()
                    })
                    .collect::<Vec<_>>();

                let LoweredClassSurface {
                    mut instance,
                    mut static_side,
                    mut body_view,
                    constructor,
                    initializer_poison,
                    mut surface_poison,
                    callables,
                    records: produced,
                    default_checks: checks,
                } = lower_class(
                    &mut factory,
                    &mut resolver,
                    ClassLoweringInput {
                        class_id,
                        application_parameters: &application_parameters,
                        application_types: &application_types,
                        class,
                        frame: &frame,
                        reservation: &reservation,
                        reservations: &self.lexical_events,
                    },
                );
                let namespace_payload = self.class_namespace_payloads.remove(&binding.type_decl);
                let has_namespace_payload = namespace_payload.is_some();
                if let Some(properties) = namespace_payload {
                    let mut static_object = factory
                        .store()
                        .object_type(static_side)
                        .cloned()
                        .expect("lowered class static side is an object");
                    let mut diagnosed_class_members = BTreeSet::new();
                    for payload in properties {
                        debug_assert_eq!(
                            payload.source_order.declaration_ordinal,
                            payload.declaration.0
                        );
                        let name = payload.property.name.clone();
                        if name == "prototype" {
                            records.push(TicketRecord::diagnostic(
                                payload.owner,
                                Diagnostic::duplicate_identifier(payload.owner_span, &name),
                            ));
                            continue;
                        }
                        if let Some(class_members) = explicit_static_members.get(&name) {
                            let first_class_member = class_members
                                .first()
                                .expect("explicit static member group is non-empty");
                            let has_getter = class_members
                                .iter()
                                .any(|member| member.kind == ExplicitStaticMemberKind::Getter);
                            let has_setter = class_members
                                .iter()
                                .any(|member| member.kind == ExplicitStaticMemberKind::Setter);
                            let namespace_is_variable = self
                                .binder
                                .declarations
                                .get(payload.declaration)
                                .is_some_and(|declaration| {
                                    declaration.kind == DeclarationKind::Variable
                                });
                            let block_scoped_accessor_collision = namespace_is_variable
                                && has_getter
                                && has_setter
                                && payload.source_order < first_class_member.source_order;
                            for class_member in class_members {
                                if !diagnosed_class_members.insert((
                                    name.clone(),
                                    class_member.source_order,
                                    class_member.span.end,
                                )) {
                                    continue;
                                }
                                records.push(TicketRecord::diagnostic(
                                    class_member.owner,
                                    if block_scoped_accessor_collision {
                                        Diagnostic::cannot_redeclare_block_scoped_variable(
                                            class_member.span,
                                            &name,
                                        )
                                    } else {
                                        Diagnostic::duplicate_identifier(class_member.span, &name)
                                    },
                                ));
                            }
                            records.push(TicketRecord::diagnostic(
                                payload.owner,
                                if block_scoped_accessor_collision {
                                    Diagnostic::cannot_redeclare_block_scoped_variable(
                                        payload.owner_span,
                                        &name,
                                    )
                                } else {
                                    Diagnostic::duplicate_identifier(payload.owner_span, &name)
                                },
                            ));
                            if payload.source_order < first_class_member.source_order {
                                if let Some(existing) = static_object
                                    .properties
                                    .iter_mut()
                                    .find(|property| property.name == name)
                                {
                                    *existing = payload.property;
                                } else {
                                    static_object.properties.push(payload.property);
                                }
                            }
                        } else if static_object.property(&name).is_none() {
                            static_object.properties.push(payload.property);
                        }
                    }
                    static_side = factory.intern_object(static_object);
                }
                if let Some(interface_fragments) = prepared_interface_groups.get(&binding.type_decl)
                {
                    let class_object = factory
                        .store()
                        .object_type(instance)
                        .cloned()
                        .expect("lowered class instance is an object");
                    class_conflict_surface = Some(class_object.clone());
                    let static_object = factory
                        .store()
                        .object_type(static_side)
                        .cloned()
                        .expect("lowered class static side is an object");
                    let class_start = self
                        .binder
                        .declarations
                        .get(match type_decls.get(binding.type_decl.index()) {
                            Some(TypeDecl::Class { declaration, .. }) => *declaration,
                            _ => unreachable!(),
                        })
                        .map(|declaration| declaration.site.binding_span.start)
                        .unwrap_or(class.span.start);
                    let mut fragments = interface_fragments
                        .iter()
                        .map(|prepared| {
                            let source_start = self
                                .binder
                                .declarations
                                .get(prepared.fragment.declaration)
                                .map(|declaration| declaration.site.binding_span.start)
                                .unwrap_or(0);
                            (
                                source_start,
                                prepared.object.clone(),
                                prepared.method_names.clone(),
                            )
                        })
                        .collect::<Vec<_>>();
                    fragments.push((
                        class_start,
                        class_object,
                        class_instance_method_names(class),
                    ));
                    fragments.sort_by_key(|(source_start, ..)| *source_start);
                    let mut composed = ObjectType::default();
                    let mut first_method_members = BTreeSet::new();
                    for (_, fragment, method_names) in fragments {
                        composed = merge_class_owned_fragment(
                            &mut factory,
                            composed,
                            fragment,
                            &mut first_method_members,
                            &method_names,
                        );
                    }
                    class_heritage_conflict_surface = Some(composed.clone());
                    let mut heritage = ObjectType::default();
                    for prepared in interface_fragments {
                        for (_, base) in &prepared.heritage_surfaces {
                            heritage = merge_class_owned_fragment(
                                &mut factory,
                                heritage,
                                base.clone(),
                                &mut BTreeSet::new(),
                                &BTreeSet::new(),
                            );
                        }
                    }
                    composed = overlay_class_owned_members(heritage, composed);
                    instance = factory.intern_object(composed.clone());
                    body_view = BodyClassView::from_objects(
                        &composed,
                        &static_object,
                        class.super_class.is_none(),
                    );
                    retain_body_member_slots(class_id, class, &mut body_view);
                } else if has_namespace_payload {
                    let instance_object = factory
                        .store()
                        .object_type(instance)
                        .cloned()
                        .expect("lowered class instance is an object");
                    let static_object = factory
                        .store()
                        .object_type(static_side)
                        .cloned()
                        .expect("lowered class static side is an object");
                    body_view = BodyClassView::from_objects(
                        &instance_object,
                        &static_object,
                        class.super_class.is_none(),
                    );
                    retain_body_member_slots(class_id, class, &mut body_view);
                }
                records.extend(produced);
                default_checks.extend(checks);
                application_checks.append(&mut resolver.application_checks);
                surface_poison.extend(publication_surface_poison);
                self.class_body_views.insert(class_id, body_view);
                if let Some(value_decl) = binding.value_decl {
                    self.decl_types.set(value_decl, static_side);
                }
                if let Some(name) = class.id.as_ref() {
                    self.class_names.insert(class_id, name.name.to_string());
                }
                resolver.fallback = reservation.tickets.immediate;
                resolver.qualified_outer_type_parameters_visible = true;
                let (heritage, heritage_surface_failures) = lower_heritage_application(
                    &mut factory,
                    &mut resolver,
                    self.binder,
                    scope,
                    &type_decls,
                    class,
                    &frame,
                );
                let mut heritage_failures = Vec::new();
                let heritage_span = class.super_type_arguments.as_deref().map_or_else(
                    || {
                        class
                            .super_class
                            .as_ref()
                            .map_or(CheckSpan::from_oxc(class.span), |heritage| {
                                CheckSpan::from_oxc(heritage.span())
                            })
                    },
                    |arguments| CheckSpan::from_oxc(arguments.span),
                );
                for failure in heritage_surface_failures {
                    let generic_unsupported = class.super_type_arguments.is_some()
                        && matches!(&failure, SurfaceTypeFailure::Unsupported(_));
                    let (owner, record) = own_surface_failure(
                        failure,
                        reservation.tickets.immediate,
                        reservation.tickets.incomplete,
                        heritage_span,
                        "class/class-heritage/type-arguments",
                        "class heritage type arguments could not be lowered",
                    );
                    if let Some(record) = record.filter(|_| !generic_unsupported) {
                        records.push(record);
                    }
                    heritage_failures.push(owner);
                }
                let mut lowerer = ClassSurfaceLowerer::new(
                    class_id,
                    ClassRecoveryOrder {
                        original_module: reservation.source.module_ordinal.index(),
                        binding_start: reservation.source.source_start,
                        declaration_ordinal: declaration.0,
                    },
                    type_parameters,
                    instance,
                    static_side,
                    constructor,
                );
                if let Some((parent, base)) = heritage {
                    lowerer.set_heritage(HeritageDependency {
                        target: parent,
                        identity_root: base,
                        owner: reservation.tickets.deferred,
                    });
                    self.class_parents.insert(class_id, parent);
                }
                if let Some(interface_fragments) = prepared_interface_groups.get(&binding.type_decl)
                {
                    for heritage in interface_fragments
                        .iter()
                        .flat_map(|prepared| &prepared.instance_heritage)
                    {
                        lowerer.add_instance_heritage(heritage.dependency);
                        heritage_spans.insert(heritage.dependency.owner, heritage.span);
                    }
                }
                for owner in heritage_failures {
                    lowerer.unsupported_heritage_surface(owner);
                }
                for owner in initializer_poison {
                    lowerer.unsupported_initializer(owner);
                }
                for owner in surface_poison {
                    lowerer.unsupported_surface(owner);
                }
                for callable in callables {
                    lowerer.retain_callable(callable);
                }
                construction
                    .register(lowerer.finish())
                    .expect("one class draft per class declaration");
            }
            drop(resolver);
            if let (Some(class_object), Some(heritage_own), Some(interface_fragments)) = (
                class_conflict_surface.as_ref(),
                class_heritage_conflict_surface.as_ref(),
                prepared_interface_groups.get(&binding.type_decl),
            ) {
                self.validate_class_interface_member_conflicts(
                    binding.type_decl,
                    class_object,
                    heritage_own,
                    interface_fragments,
                );
                class_conflict_surfaces.insert(binding.type_decl, heritage_own.clone());
            }
        }

        let publication = construction
            .finish(self.interner)
            .expect("class publication must preserve reserved identities");
        self.staged_published_classes = Some(publication.published);
        self.class_application_parameters = publication.type_parameters;
        for (group, class_object) in &class_conflict_surfaces {
            let Some(interface_fragments) = prepared_interface_groups.get(group) else {
                continue;
            };
            let derived_name = self
                .binder
                .type_groups
                .get(*group)
                .map(|group| group.name.clone())
                .unwrap_or_else(|| "<class>".to_string());
            for prepared in interface_fragments {
                for heritage in &prepared.instance_heritage {
                    let surface = match self
                        .staged_published_classes
                        .as_ref()
                        .expect("class registry is staged before heritage validation")
                        .published_class(heritage.dependency.target)
                    {
                        DemandOutcome::Ready(surface) => surface.clone(),
                        DemandOutcome::Exhausted(_) => continue,
                    };
                    let Some(application) = self
                        .interner
                        .store()
                        .class_instance_type(heritage.dependency.identity_root)
                        .cloned()
                    else {
                        continue;
                    };
                    let substitutions: FxHashMap<_, _> = surface
                        .type_params()
                        .iter()
                        .copied()
                        .zip(application.args)
                        .collect();
                    let projected = crate::types::substitute(
                        self.interner,
                        surface.instance_template(),
                        &substitutions,
                    );
                    let Some(base) = self.interner.store().object_type(projected).cloned() else {
                        continue;
                    };
                    self.validate_class_interface_heritage_surface(
                        *group,
                        class_object,
                        prepared,
                        &derived_name,
                        &heritage.base_name,
                        &base,
                    );
                }
            }
        }
        for obligation in publication.obligations {
            match obligation {
                super::construction::PendingSurfaceObligation::InitializerOrigin { .. }
                | super::construction::PendingSurfaceObligation::SurfaceOrigin { .. }
                | super::construction::PendingSurfaceObligation::Deferred { .. } => {}
                super::construction::PendingSurfaceObligation::HeritageCycle { owner, .. } => {
                    let span = heritage_spans
                        .get(&owner)
                        .copied()
                        .expect("heritage-cycle owner must retain its extends span");
                    records.push(TicketRecord::incomplete(
                        owner,
                        IncompleteSurface::new(
                            "class/class-heritage/cycle",
                            span,
                            "class heritage cycle poisons the published surface",
                        ),
                    ));
                }
                super::construction::PendingSurfaceObligation::PoisonedBase { owner, .. } => {
                    let span = heritage_spans
                        .get(&owner)
                        .copied()
                        .expect("poisoned-base owner must retain its extends span");
                    records.push(TicketRecord::incomplete(
                        owner,
                        IncompleteSurface::new(
                            "class/class-heritage/poisoned-base",
                            span,
                            "class heritage base surface is poisoned",
                        ),
                    ));
                }
            }
        }
        self.retained_class_callables = publication.retained_callables;
        self.class_super_constructors = publication.heritage_constructors;
        for group in class_groups {
            self.freeze_type_group(group);
        }
        for reservation in self.lexical_events.classes() {
            let Some(binding) = reservation.binding.as_ref() else {
                continue;
            };
            if let DemandOutcome::Ready(surface) = self
                .staged_published_classes
                .as_ref()
                .expect("class registry remains staged until type publication")
                .published_class(binding.class_id)
            {
                if let Some(value_decl) = binding.value_decl {
                    self.decl_types.set(value_decl, surface.static_template());
                }
            }
        }
        records.extend(abstract_completeness_records(
            &self.lexical_events,
            &type_decls,
            &self.class_parents,
        ));
        for declaration in &type_decls {
            let TypeDecl::Class {
                class_id, class, ..
            } = declaration
            else {
                continue;
            };
            let (ctor_visibility, ctor_declaring_class) =
                effective_constructor_access(*class_id, class, &type_decls, &self.class_parents);
            self.class_new_metadata.insert(
                *class_id,
                PublishedClassNewMetadata {
                    is_abstract: class.r#abstract,
                    ctor_visibility,
                    ctor_declaring_class,
                    has_source_overloads: super::visibility::constructor_declaration_count(class)
                        > 1,
                },
            );
        }
        self.collect_override_checks(&type_decls);
        assert!(self.staged_class_validation.is_none());
        self.staged_class_validation = Some(StagedClassValidation {
            records,
            default_checks,
            application_checks,
        });
    }

    pub(in crate::check::checker) fn validate_published_class_surfaces(&mut self) {
        assert!(
            self.type_environment.is_published(),
            "class validation requires the complete immutable type-group registry"
        );
        let StagedClassValidation {
            mut records,
            default_checks,
            application_checks,
        } = self
            .staged_class_validation
            .take()
            .expect("class validation is consumed exactly once");
        for check in application_checks {
            let substitutions = check
                .parameters
                .iter()
                .copied()
                .zip(check.arguments.iter().copied())
                .collect();
            let checks = check
                .parameters
                .iter()
                .zip(&check.arguments)
                .zip(&check.explicit_spans)
                .map(|((&parameter, &argument), &span)| {
                    (
                        self.interner.store().type_param_constraint(parameter),
                        argument,
                        span,
                    )
                })
                .collect();
            let mut effects = CheckerEffects::new(check.owner);
            effects.constraint_checks.push(ConstraintCheckObligation {
                checks,
                substitutions,
            });
            self.enqueue_effects(effects);
        }
        for (owner, source, target, span) in default_checks {
            let outcome = SemanticQueryCoordinator::new(
                self.interner,
                self.type_environment.published().classes(),
                &mut self.semantic_queries,
                &mut self.next_type_param,
            )
            .is_assignable(source, target);
            let failed = match outcome {
                RelationOutcome::Yes => false,
                RelationOutcome::No(_) => true,
                RelationOutcome::Exhausted(exhaustion) => {
                    let (id, context) = if exhaustion
                        == crate::class_semantics::Exhaustion::ClassProjectionBudget
                    {
                        (
                            "relation/class-projection-budget",
                            "class projection budget exhausted while checking a default constraint",
                        )
                    } else {
                        (
                            "relation/class-default-constraint-exhausted",
                            "class default constraint could not be decided conservatively",
                        )
                    };
                    records.push(TicketRecord::incomplete(
                        owner,
                        IncompleteSurface::new(id, span, context),
                    ));
                    false
                }
            };
            if failed {
                let source = render_type(self.interner.store(), source, false);
                let target = render_type(self.interner.store(), target, false);
                records.push(TicketRecord::diagnostic(
                    owner,
                    Diagnostic::constraint_not_satisfied(span, &source, &target),
                ));
            }
        }
        records.sort_by_key(|record| (record.owner, record.source_start));
        for record in records {
            self.enqueue_ticket_record(record.owner, record.record);
        }
    }

    fn collect_override_checks(&mut self, type_decls: &[TypeDecl<'_>]) {
        let class_defs: FxHashMap<ClassId, &Class<'_>> = type_decls
            .iter()
            .filter_map(|declaration| match declaration {
                TypeDecl::Class {
                    class_id, class, ..
                } => Some((*class_id, *class)),
                _ => None,
            })
            .collect();
        let reservations = self.lexical_events.classes().to_vec();
        for reservation in reservations {
            let Some(binding) = reservation.binding.as_ref() else {
                continue;
            };
            let Some(TypeDecl::Class {
                scope,
                class,
                class_id,
                ..
            }) = type_decls.get(binding.type_decl.index())
            else {
                continue;
            };
            let scope = *scope;
            let class_id = *class_id;
            let class = *class;
            let Some(parent) = self.class_parents.get(&class_id).copied() else {
                continue;
            };
            let derived_is_generic = class
                .type_parameters
                .as_deref()
                .is_some_and(|parameters| !parameters.params.is_empty());
            let base_is_generic = class_defs
                .get(&parent)
                .and_then(|base| base.type_parameters.as_deref())
                .is_some_and(|parameters| !parameters.params.is_empty());
            if derived_is_generic || base_is_generic {
                continue;
            }
            let (derived_object, base_object) = match (
                self.staged_published_classes
                    .as_ref()
                    .expect("override candidates use the frozen staged class registry")
                    .published_class(class_id),
                self.staged_published_classes
                    .as_ref()
                    .expect("override candidates use the frozen staged class registry")
                    .published_class(parent),
            ) {
                (DemandOutcome::Ready(derived), DemandOutcome::Ready(base)) => (
                    self.interner
                        .store()
                        .object_type(derived.instance_template())
                        .cloned(),
                    self.interner
                        .store()
                        .object_type(base.instance_template())
                        .cloned(),
                ),
                _ => continue,
            };
            let (Some(derived_object), Some(base_object)) = (derived_object, base_object) else {
                continue;
            };
            let derived_name = self
                .class_names
                .get(&class_id)
                .cloned()
                .unwrap_or_else(|| format!("class#{}", class_id.0));
            let base_name = self
                .class_names
                .get(&parent)
                .cloned()
                .unwrap_or_else(|| format!("class#{}", parent.0));
            for (index, element) in class.body.body.iter().enumerate() {
                let (name, span) = match element {
                    ClassElement::PropertyDefinition(property)
                        if !property.r#static && !property.computed =>
                    {
                        let Some(name) = class_member_name(&property.key) else {
                            continue;
                        };
                        (name, CheckSpan::from_oxc(property.key.span()))
                    }
                    ClassElement::MethodDefinition(method)
                        if !method.r#static
                            && !method.computed
                            && method.kind != MethodDefinitionKind::Constructor =>
                    {
                        let Some(name) = class_member_name(&method.key) else {
                            continue;
                        };
                        (name, CheckSpan::from_oxc(method.key.span()))
                    }
                    _ => continue,
                };
                let (Some(own), Some(base)) =
                    (derived_object.property(&name), base_object.property(&name))
                else {
                    continue;
                };
                let error = self.interner.well_known().error;
                if own.visibility != Visibility::Public
                    || base.visibility != Visibility::Public
                    || own.ty == error
                    || base.ty == error
                {
                    continue;
                }
                if own.is_accessor != base.is_accessor
                    && !(class_member_has_explicit_surface(
                        class_id,
                        &name,
                        &class_defs,
                        &self.class_parents,
                    ) && class_member_has_explicit_surface(
                        parent,
                        &name,
                        &class_defs,
                        &self.class_parents,
                    ))
                {
                    continue;
                }
                let Some(owner) = reservation
                    .members
                    .get(index)
                    .and_then(|site| self.lexical_events.member(*site))
                    .map(|member| member.tickets.deferred)
                else {
                    continue;
                };
                let _ = scope;
                let base_is_method =
                    class_member_is_method(parent, &name, &class_defs, &self.class_parents);
                if base_is_method
                    && self
                        .interner
                        .store()
                        .function_type(own.ty)
                        .zip(self.interner.store().function_type(base.ty))
                        .is_some_and(|(own, base)| own.params.len() != base.params.len())
                {
                    continue;
                }
                self.with_ticket_effects(owner, |pass| {
                    pass.schedule_override(OverrideCheck {
                        own_ty: own.ty,
                        base_ty: base.ty,
                        name,
                        derived: derived_name.clone(),
                        base: base_name.clone(),
                        span,
                        base_is_method,
                    });
                });
            }
        }
    }
}

fn effective_constructor_access(
    class_id: ClassId,
    class: &Class<'_>,
    declarations: &[TypeDecl<'_>],
    parents: &FxHashMap<ClassId, ClassId>,
) -> (Visibility, ClassId) {
    if super::visibility::constructor_declaration_count(class) > 0 {
        return (super::visibility::constructor_visibility(class), class_id);
    }
    let mut visited = rustc_hash::FxHashSet::default();
    let mut current = class_id;
    while visited.insert(current) {
        let Some(parent) = parents.get(&current).copied() else {
            break;
        };
        let parent_class = declarations
            .iter()
            .find_map(|declaration| match declaration {
                TypeDecl::Class {
                    class_id, class, ..
                } if *class_id == parent => Some(*class),
                _ => None,
            });
        let Some(parent_class) = parent_class else {
            break;
        };
        if super::visibility::constructor_declaration_count(parent_class) > 0 {
            return (
                super::visibility::constructor_visibility(parent_class),
                parent,
            );
        }
        current = parent;
    }
    (Visibility::Public, class_id)
}

type DefaultConstraintCheck = (UserRecordTicket, TypeId, TypeId, CheckSpan);

struct ClassLoweringInput<'a, 'ast> {
    class_id: ClassId,
    application_parameters: &'a [ClassTypeParameter<UserRecordTicket>],
    application_types: &'a [TypeId],
    class: &'a Class<'ast>,
    frame: &'a [(String, TypeId)],
    reservation: &'a ClassReservation,
    reservations: &'a LexicalReservations,
}

struct LoweredClassSurface {
    instance: TypeId,
    static_side: TypeId,
    body_view: BodyClassView,
    constructor: Option<TypeId>,
    initializer_poison: Vec<UserRecordTicket>,
    surface_poison: Vec<UserRecordTicket>,
    callables: Vec<RetainedClassCallable<UserRecordTicket>>,
    records: Vec<TicketRecord>,
    default_checks: Vec<DefaultConstraintCheck>,
}

struct AccessorSurface {
    getter: Option<TypeId>,
    setter: Option<TypeId>,
    visibility: Visibility,
    is_static: bool,
}

fn lower_class<'ast>(
    factory: &mut SurfaceTypeFactory<'_>,
    resolver: &mut Resolver<'_, '_>,
    input: ClassLoweringInput<'_, 'ast>,
) -> LoweredClassSurface {
    let ClassLoweringInput {
        class_id,
        application_parameters,
        application_types,
        class,
        frame,
        reservation,
        reservations,
    } = input;
    let open_application = match build_open_class_application(
        factory,
        class_id,
        application_parameters,
        application_types,
    ) {
        DemandOutcome::Ready(application) => application,
        DemandOutcome::Exhausted(_) => {
            unreachable!("class parameter frame is aligned with its reserved binders")
        }
    };
    let mut instance = ObjectType::default();
    let mut static_side = ObjectType::default();
    let mut initializer = InitializerContext::default();
    let mut initializer_poison = Vec::new();
    let mut surface_poison = Vec::new();
    let mut constructor = None;
    let mut retained = Vec::new();
    let mut records = Vec::new();
    let mut default_checks = Vec::new();
    let class_parameter_names: std::collections::BTreeSet<&str> =
        frame.iter().map(|(name, _)| name.as_str()).collect();
    let mut accessors: FxHashMap<String, AccessorSurface> = FxHashMap::default();
    let mut method_counts: BTreeMap<(String, bool), usize> = BTreeMap::new();
    let mut constructor_count = 0usize;
    for element in &class.body.body {
        let ClassElement::MethodDefinition(method) = element else {
            continue;
        };
        if method.computed {
            continue;
        }
        match method.kind {
            MethodDefinitionKind::Method => {
                if let Some(name) = class_member_name(&method.key) {
                    *method_counts.entry((name, method.r#static)).or_default() += 1;
                }
            }
            MethodDefinitionKind::Constructor => constructor_count += 1,
            MethodDefinitionKind::Get | MethodDefinitionKind::Set => {}
        }
    }
    let mut method_overloads: BTreeMap<(String, bool), (Visibility, Vec<TypeId>)> = BTreeMap::new();
    let mut constructor_overloads = Vec::new();

    for (index, element) in class.body.body.iter().enumerate() {
        let member = reservation
            .members
            .get(index)
            .and_then(|id| reservations.member(*id));
        let incomplete_owner = member.map_or(reservation.tickets.incomplete, |member| {
            member.tickets.incomplete
        });
        let immediate_owner = member.map_or(reservation.tickets.immediate, |member| {
            member.tickets.immediate
        });
        match element {
            ClassElement::PropertyDefinition(property) if !property.computed => {
                let Some(name) = class_member_name(&property.key) else {
                    continue;
                };
                if property.r#static {
                    if let Some(annotation) = property.type_annotation.as_ref() {
                        let mut diagnostics = Vec::new();
                        collect_static_class_parameter_diagnostics(
                            &annotation.type_annotation,
                            &class_parameter_names,
                            &mut diagnostics,
                        );
                        if !diagnostics.is_empty() {
                            surface_poison.push(immediate_owner);
                        }
                        records.extend(diagnostics.into_iter().map(|diagnostic| {
                            TicketRecord::diagnostic(immediate_owner, diagnostic)
                        }));
                    }
                }
                let ty = if let Some(annotation) = property.type_annotation.as_ref() {
                    resolver.fallback = immediate_owner;
                    resolver.qualified_outer_type_parameters_visible = !property.r#static;
                    let (lowered, child_failures) =
                        lower_type(factory, resolver, &annotation.type_annotation, frame);
                    let annotation_ty = lowered.as_ref().ok().copied();
                    for failure in lowered.err().into_iter().chain(child_failures) {
                        let recovered_topology =
                            matches!(&failure, SurfaceTypeFailure::QualifiedTopology { .. });
                        let (owner, record) = own_surface_failure(
                            failure,
                            immediate_owner,
                            incomplete_owner,
                            CheckSpan::from_oxc(annotation.type_annotation.span()),
                            "class/property-definition/type-annotation",
                            "property type annotation could not be lowered",
                        );
                        if let Some(record) = record {
                            records.push(record);
                        }
                        if !recovered_topology {
                            surface_poison.push(owner);
                        }
                    }
                    annotation_ty
                } else if let Some(value) = property.value.as_ref() {
                    seed_initializer_annotations(factory, resolver, value, frame, &mut initializer);
                    match SurfaceInitializerInferer::new(factory, &mut initializer).infer(
                        value,
                        property.readonly,
                        property.r#static,
                    ) {
                        InitializerInference::Inferred(ty) => Some(ty),
                        InitializerInference::Unsupported => {
                            records.push(TicketRecord::incomplete(
                                immediate_owner,
                                IncompleteSurface::new(
                                    "class/property-definition/initializer-inference",
                                    CheckSpan::from_oxc(value.span()),
                                    "unannotated field initializer cannot be inferred during class surface construction",
                                ),
                            ));
                            initializer_poison.push(immediate_owner);
                            None
                        }
                    }
                } else {
                    records.push(TicketRecord::incomplete(
                        immediate_owner,
                        IncompleteSurface::new(
                            "class/property-definition/implicit-any",
                            CheckSpan::from_oxc(property.span),
                            "class property without an annotation or initializer has implicit any type",
                        ),
                    ));
                    surface_poison.push(immediate_owner);
                    None
                };
                let Some(mut ty) = ty else {
                    continue;
                };
                if property.optional {
                    ty = factory.union(vec![ty, factory.well_known().undefined]);
                }
                let lowered = PropertyType {
                    name: name.clone(),
                    ty,
                    write_ty: None,
                    optional: property.optional,
                    visibility: class_member_visibility(&property.key, property.accessibility),
                    declaring_class: Some(class_id),
                    readonly: property.readonly,
                    is_accessor: false,
                };
                if property.r#static {
                    static_side.properties.push(lowered);
                } else {
                    initializer.fields.insert(name, ty);
                    instance.properties.push(lowered);
                }
            }
            ClassElement::MethodDefinition(method) => {
                if method.computed {
                    records.push(TicketRecord::incomplete(
                        incomplete_owner,
                        IncompleteSurface::new(
                            "class/method-definition/computed-key",
                            CheckSpan::from_oxc(method.key.span()),
                            "computed method key is not collected",
                        ),
                    ));
                }
                let method_key = (method.kind == MethodDefinitionKind::Method)
                    .then(|| class_member_name(&method.key).map(|name| (name, method.r#static)))
                    .flatten();
                let is_method_overload = method_key
                    .as_ref()
                    .is_some_and(|key| method_counts.get(key).copied().unwrap_or_default() > 1);
                let is_constructor_overload =
                    method.kind == MethodDefinitionKind::Constructor && constructor_count > 1;
                let overload = if is_method_overload || is_constructor_overload {
                    if method.value.body.is_some() {
                        CallableOverloadRole::Implementation {
                            ordinal: index,
                            hidden_from_public: true,
                        }
                    } else {
                        CallableOverloadRole::Signature { ordinal: index }
                    }
                } else {
                    CallableOverloadRole::Single
                };
                if method.r#static {
                    let own_parameters: std::collections::BTreeSet<&str> = method
                        .value
                        .type_parameters
                        .as_deref()
                        .into_iter()
                        .flat_map(|declaration| declaration.params.iter())
                        .map(|parameter| parameter.name.name.as_str())
                        .collect();
                    let visible_class_parameters = class_parameter_names
                        .iter()
                        .copied()
                        .filter(|name| !own_parameters.contains(name))
                        .collect();
                    let mut diagnostics = Vec::new();
                    for annotation in method
                        .value
                        .type_parameters
                        .as_deref()
                        .into_iter()
                        .flat_map(|declaration| declaration.params.iter())
                        .flat_map(|parameter| {
                            parameter.constraint.iter().chain(parameter.default.iter())
                        })
                        .chain(
                            method
                                .value
                                .params
                                .items
                                .iter()
                                .filter_map(|parameter| parameter.type_annotation.as_ref())
                                .map(|annotation| &annotation.type_annotation),
                        )
                        .chain(
                            method
                                .value
                                .return_type
                                .as_deref()
                                .map(|annotation| &annotation.type_annotation),
                        )
                    {
                        collect_static_class_parameter_diagnostics(
                            annotation,
                            &visible_class_parameters,
                            &mut diagnostics,
                        );
                    }
                    if !method.computed && !diagnostics.is_empty() {
                        surface_poison.push(immediate_owner);
                    }
                    records.extend(
                        diagnostics.into_iter().map(|diagnostic| {
                            TicketRecord::diagnostic(immediate_owner, diagnostic)
                        }),
                    );
                }
                resolver.fallback = immediate_owner;
                resolver.qualified_outer_type_parameters_visible = !method.r#static;
                let (syntax, failures) = lower_callable(factory, resolver, &method.value, frame);
                for failure in syntax.failure.iter().cloned().chain(failures) {
                    let recovered_topology =
                        matches!(&failure, SurfaceTypeFailure::QualifiedTopology { .. });
                    match failure {
                        SurfaceTypeFailure::Unresolved {
                            owner: _,
                            span,
                            name,
                        } => {
                            records.push(TicketRecord::diagnostic(
                                immediate_owner,
                                Diagnostic::cannot_find_name(CheckSpan::from_oxc(span), &name),
                            ));
                            if !method.computed {
                                surface_poison.push(immediate_owner);
                            }
                        }
                        other => {
                            let (owner, record) = own_surface_failure(
                                other,
                                immediate_owner,
                                incomplete_owner,
                                CheckSpan::from_oxc(method.value.span),
                                "class/method-definition/signature",
                                "method signature could not be lowered",
                            );
                            if let Some(record) = record {
                                records.push(record);
                            }
                            if !method.computed && !recovered_topology {
                                surface_poison.push(owner);
                            }
                        }
                    }
                }
                let callable = if syntax.failure.is_none() {
                    Some(
                        factory.intern_function(FunctionType {
                            type_params: syntax.type_params.clone(),
                            receiver: syntax.receiver,
                            params: syntax
                                .params
                                .iter()
                                .cloned()
                                .map(|parameter| {
                                    parameter.expect(
                                        "successful callable retains every source parameter",
                                    )
                                })
                                .collect(),
                            ret: syntax
                                .declared_return
                                .expect("successful callable retains its return type"),
                        }),
                    )
                } else {
                    None
                };
                if let Some(member) = reservation
                    .members
                    .get(index)
                    .and_then(|id| reservations.member(*id))
                {
                    if let Some(site) = member.callable {
                        if let Some(parameters) = method.value.type_parameters.as_deref() {
                            for (parameter, generic) in
                                parameters.params.iter().zip(&syntax.type_params)
                            {
                                if let (Some(default), Some(constraint), Some(default_syntax)) = (
                                    generic.default,
                                    generic.constraint,
                                    parameter.default.as_ref(),
                                ) {
                                    default_checks.push((
                                        member.tickets.immediate,
                                        default,
                                        constraint,
                                        CheckSpan::from_oxc(default_syntax.span()),
                                    ));
                                }
                            }
                        }
                        for generic in &syntax.type_params {
                            if let Some(constraint) = generic.constraint {
                                assert!(
                                    factory.set_type_param_constraint(generic.id, constraint),
                                    "callable type-parameter constraint is assigned exactly once"
                                );
                            }
                        }
                        let type_param_frame = method
                            .value
                            .type_parameters
                            .as_deref()
                            .into_iter()
                            .flat_map(|declaration| declaration.params.iter())
                            .zip(syntax.type_params.iter())
                            .map(|(parameter, generic)| {
                                (
                                    parameter.name.name.to_string(),
                                    factory.intern_type_param(
                                        generic.id,
                                        parameter.name.name.as_str(),
                                    ),
                                )
                            })
                            .collect();
                        let type_parameters = method
                            .value
                            .type_parameters
                            .as_deref()
                            .into_iter()
                            .flat_map(|declaration| declaration.params.iter())
                            .zip(syntax.type_params.iter())
                            .map(|(source, generic)| RetainedCallableTypeParameter {
                                id: generic.id,
                                constraint: generic.constraint,
                                default: match (source.default.as_ref(), generic.default) {
                                    (_, Some(default)) => ClassTypeParameterDefault::Ready(default),
                                    (Some(_), None) => ClassTypeParameterDefault::Unsupported(
                                        member.tickets.incomplete,
                                    ),
                                    (None, None) => ClassTypeParameterDefault::Absent,
                                },
                            })
                            .collect();
                        let parameter_properties = method
                            .value
                            .params
                            .items
                            .iter()
                            .zip(syntax.params.iter())
                            .enumerate()
                            .filter_map(|(parameter_index, (parameter, lowered))| {
                                if parameter.accessibility.is_none() && !parameter.readonly {
                                    return None;
                                }
                                let BindingPattern::BindingIdentifier(identifier) =
                                    &parameter.pattern
                                else {
                                    return None;
                                };
                                let public_type = lowered.as_ref()?.ty;
                                Some(RetainedParameterProperty {
                                    parameter_index,
                                    property_name: identifier.name.to_string(),
                                    public_type,
                                    owner: member.tickets.deferred,
                                })
                            })
                            .collect();
                        retained.push(RetainedClassCallable {
                            site,
                            tickets: reservations
                                .callable(site)
                                .expect("class callable must retain its reservation")
                                .tickets,
                            type_params: syntax.type_params.clone(),
                            type_param_frame,
                            receiver: syntax.receiver,
                            params: syntax.params.clone(),
                            declared_return: method
                                .value
                                .return_type
                                .as_ref()
                                .and(syntax.declared_return),
                            public_type: callable,
                            type_parameters,
                            overload,
                            parameter_properties,
                        });
                    }
                }
                if method.computed {
                    continue;
                }
                if method.kind == MethodDefinitionKind::Constructor {
                    let constructor_surface = callable
                        .and_then(|callable| factory.store().function_type(callable))
                        .cloned();
                    constructor = callable;
                    if is_constructor_overload && method.value.body.is_none() {
                        if let Some(callable) = callable {
                            constructor_overloads.push(callable);
                        }
                    }
                    for (parameter_index, parameter) in method.value.params.items.iter().enumerate()
                    {
                        if parameter.accessibility.is_none() && !parameter.readonly {
                            continue;
                        }
                        let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern
                        else {
                            continue;
                        };
                        let Some(ty) = constructor_surface
                            .as_ref()
                            .and_then(|surface| surface.params.get(parameter_index))
                            .map(|parameter| parameter.ty)
                        else {
                            continue;
                        };
                        instance.properties.push(PropertyType {
                            name: identifier.name.to_string(),
                            ty,
                            write_ty: None,
                            optional: parameter.optional,
                            visibility: lower_visibility(parameter.accessibility),
                            declaring_class: Some(class_id),
                            readonly: parameter.readonly,
                            is_accessor: false,
                        });
                    }
                    continue;
                }
                let Some(name) = class_member_name(&method.key) else {
                    continue;
                };
                let Some(callable) = callable else {
                    continue;
                };
                if method.kind == MethodDefinitionKind::Method {
                    if is_method_overload {
                        if method.value.body.is_none() {
                            let key = method_key
                                .clone()
                                .expect("overloaded method retains its static name");
                            method_overloads
                                .entry(key)
                                .or_insert_with(|| {
                                    (
                                        class_member_visibility(&method.key, method.accessibility),
                                        Vec::new(),
                                    )
                                })
                                .1
                                .push(callable);
                        }
                        continue;
                    }
                    let lowered = PropertyType {
                        name: name.clone(),
                        ty: callable,
                        write_ty: None,
                        optional: false,
                        visibility: class_member_visibility(&method.key, method.accessibility),
                        declaring_class: Some(class_id),
                        readonly: false,
                        is_accessor: false,
                    };
                    if method.r#static {
                        static_side.properties.push(lowered);
                    } else {
                        initializer.methods.insert(
                            name,
                            EarlierMethodSurface {
                                function: callable,
                                signatures: 1,
                            },
                        );
                        instance.properties.push(lowered);
                    }
                } else if matches!(
                    method.kind,
                    MethodDefinitionKind::Get | MethodDefinitionKind::Set
                ) {
                    let entry = accessors.entry(name).or_insert(AccessorSurface {
                        getter: None,
                        setter: None,
                        visibility: class_member_visibility(&method.key, method.accessibility),
                        is_static: method.r#static,
                    });
                    if method.kind == MethodDefinitionKind::Get {
                        entry.getter = factory
                            .store()
                            .function_type(callable)
                            .map(|function| function.ret);
                        entry.visibility =
                            class_member_visibility(&method.key, method.accessibility);
                    } else {
                        entry.setter = factory
                            .store()
                            .function_type(callable)
                            .and_then(|function| function.params.first())
                            .map(|parameter| parameter.ty);
                    }
                }
            }
            ClassElement::PropertyDefinition(property) if property.computed => {
                records.push(TicketRecord::incomplete(
                    incomplete_owner,
                    IncompleteSurface::new(
                        "class/property-definition/computed-key",
                        CheckSpan::from_oxc(property.key.span()),
                        "computed property key is not collected",
                    ),
                ));
                if let Some(annotation) = property.type_annotation.as_ref() {
                    resolver.fallback = incomplete_owner;
                    resolver.qualified_outer_type_parameters_visible = !property.r#static;
                    let (lowered, child_failures) =
                        lower_type(factory, resolver, &annotation.type_annotation, frame);
                    for failure in lowered.err().into_iter().chain(child_failures) {
                        let (_, record) = own_surface_failure(
                            failure,
                            incomplete_owner,
                            incomplete_owner,
                            CheckSpan::from_oxc(annotation.type_annotation.span()),
                            "class/property-definition/type-annotation",
                            "computed property type annotation could not be lowered",
                        );
                        if let Some(record) = record {
                            records.push(record);
                        }
                    }
                }
            }
            ClassElement::StaticBlock(block) => {
                records.push(TicketRecord::incomplete(
                    incomplete_owner,
                    IncompleteSurface::new(
                        "class/static-block/self",
                        CheckSpan::from_oxc(block.span),
                        "static block is not checked",
                    ),
                ));
            }
            ClassElement::AccessorProperty(accessor) => {
                records.push(TicketRecord::incomplete(
                    incomplete_owner,
                    IncompleteSurface::new(
                        "class/accessor-property/self",
                        CheckSpan::from_oxc(accessor.span),
                        "auto-accessor property is not checked",
                    ),
                ));
            }
            ClassElement::TSIndexSignature(index) => {
                records.push(TicketRecord::incomplete(
                    incomplete_owner,
                    IncompleteSurface::new(
                        "class/class-index-signature/self",
                        CheckSpan::from_oxc(index.span),
                        "class index signature is not collected",
                    ),
                ));
            }
            _ => {}
        }
    }

    for (name, accessor) in accessors {
        let Some(ty) = accessor.getter.or(accessor.setter) else {
            continue;
        };
        let property = PropertyType {
            name,
            ty,
            write_ty: accessor.setter,
            optional: false,
            visibility: accessor.visibility,
            declaring_class: Some(class_id),
            readonly: accessor.setter.is_none(),
            is_accessor: true,
        };
        if accessor.is_static {
            static_side.properties.push(property);
        } else {
            instance.properties.push(property);
        }
    }

    for ((name, is_static), (visibility, signatures)) in method_overloads {
        if signatures.is_empty() {
            continue;
        }
        let ty = factory.intern_object(ObjectType {
            call_signatures: signatures,
            ..Default::default()
        });
        let property = PropertyType {
            name: name.clone(),
            ty,
            write_ty: None,
            optional: false,
            visibility,
            declaring_class: Some(class_id),
            readonly: false,
            is_accessor: false,
        };
        if is_static {
            static_side.properties.push(property);
        } else {
            initializer.methods.insert(
                name,
                EarlierMethodSurface {
                    function: ty,
                    signatures: factory
                        .store()
                        .object_type(ty)
                        .map_or(0, |object| object.call_signatures.len()),
                },
            );
            instance.properties.push(property);
        }
    }

    if !class.r#abstract && has_public_constructor(class) {
        let public_constructors = if constructor_overloads.is_empty() {
            constructor.into_iter().collect::<Vec<_>>()
        } else {
            constructor_overloads
        };
        for constructor in public_constructors {
            if let Some(function) = factory.store().function_type(constructor).cloned() {
                static_side
                    .construct_signatures
                    .push(factory.intern_function(FunctionType {
                        type_params: Vec::new(),
                        receiver: None,
                        params: function.params,
                        ret: open_application,
                    }));
            }
        }
    }

    static_side.properties.push(PropertyType {
        name: "prototype".to_string(),
        ty: open_application,
        write_ty: None,
        optional: false,
        visibility: Visibility::Public,
        declaring_class: None,
        readonly: false,
        is_accessor: false,
    });

    let mut body_view =
        BodyClassView::from_objects(&instance, &static_side, class.super_class.is_none());
    retain_body_member_slots(class_id, class, &mut body_view);
    let instance = factory.intern_object(instance);
    let static_side = factory.intern_object(static_side);
    LoweredClassSurface {
        instance,
        static_side,
        body_view,
        constructor,
        initializer_poison,
        surface_poison,
        callables: retained,
        records,
        default_checks,
    }
}

fn retain_body_member_slots(class_id: ClassId, class: &Class<'_>, view: &mut BodyClassView) {
    for element in &class.body.body {
        match element {
            ClassElement::PropertyDefinition(property) if !property.computed => {
                let Some(name) = class_member_name(&property.key) else {
                    continue;
                };
                let target = if property.r#static {
                    &mut view.static_side
                } else {
                    &mut view.instance
                };
                target.retain_declaration(
                    name,
                    BodyMemberMetadata::new(
                        class_member_visibility(&property.key, property.accessibility),
                        Some(class_id),
                        property.readonly,
                        false,
                    ),
                    true,
                );
            }
            ClassElement::MethodDefinition(method) if !method.computed => {
                if method.kind == MethodDefinitionKind::Constructor {
                    for parameter in &method.value.params.items {
                        if parameter.accessibility.is_none() && !parameter.readonly {
                            continue;
                        }
                        let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern
                        else {
                            continue;
                        };
                        view.instance.retain_declaration(
                            identifier.name.to_string(),
                            BodyMemberMetadata::new(
                                lower_visibility(parameter.accessibility),
                                Some(class_id),
                                parameter.readonly,
                                false,
                            ),
                            true,
                        );
                    }
                    continue;
                }
                let Some(name) = class_member_name(&method.key) else {
                    continue;
                };
                let target = if method.r#static {
                    &mut view.static_side
                } else {
                    &mut view.instance
                };
                target.retain_declaration(
                    name,
                    BodyMemberMetadata::new(
                        class_member_visibility(&method.key, method.accessibility),
                        Some(class_id),
                        method.kind == MethodDefinitionKind::Get,
                        matches!(
                            method.kind,
                            MethodDefinitionKind::Get | MethodDefinitionKind::Set
                        ),
                    ),
                    method.kind != MethodDefinitionKind::Get || method.value.return_type.is_some(),
                );
            }
            ClassElement::AccessorProperty(accessor) if !accessor.computed => {
                let Some(name) = class_member_name(&accessor.key) else {
                    continue;
                };
                let target = if accessor.r#static {
                    &mut view.static_side
                } else {
                    &mut view.instance
                };
                target.retain_declaration(
                    name,
                    BodyMemberMetadata::new(
                        class_member_visibility(&accessor.key, accessor.accessibility),
                        Some(class_id),
                        false,
                        true,
                    ),
                    false,
                );
            }
            _ => {}
        }
    }
}

fn collect_static_class_parameter_diagnostics(
    ty: &TSType<'_>,
    class_parameters: &std::collections::BTreeSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match ty {
        TSType::TSTypeReference(reference) => {
            if let TSTypeName::IdentifierReference(identifier) = &reference.type_name {
                if class_parameters.contains(identifier.name.as_str()) {
                    diagnostics.push(Diagnostic::static_member_references_class_type_parameter(
                        CheckSpan::from_oxc(identifier.span),
                    ));
                }
            }
            if let Some(arguments) = reference.type_arguments.as_deref() {
                for argument in &arguments.params {
                    collect_static_class_parameter_diagnostics(
                        argument,
                        class_parameters,
                        diagnostics,
                    );
                }
            }
        }
        TSType::TSParenthesizedType(parenthesized) => collect_static_class_parameter_diagnostics(
            &parenthesized.type_annotation,
            class_parameters,
            diagnostics,
        ),
        TSType::TSArrayType(array) => collect_static_class_parameter_diagnostics(
            &array.element_type,
            class_parameters,
            diagnostics,
        ),
        TSType::TSUnionType(union) => {
            for member in &union.types {
                collect_static_class_parameter_diagnostics(member, class_parameters, diagnostics);
            }
        }
        TSType::TSIntersectionType(intersection) => {
            for member in &intersection.types {
                collect_static_class_parameter_diagnostics(member, class_parameters, diagnostics);
            }
        }
        _ => {}
    }
}

struct AbstractClassInfo<'ast> {
    class: &'ast Class<'ast>,
    owner: UserRecordTicket,
}

fn abstract_completeness_records<'ast>(
    reservations: &LexicalReservations,
    declarations: &[TypeDecl<'ast>],
    parents: &FxHashMap<ClassId, ClassId>,
) -> Vec<TicketRecord> {
    let mut classes = FxHashMap::default();
    for reservation in reservations.classes() {
        let Some(binding) = reservation.binding.as_ref() else {
            continue;
        };
        let Some(TypeDecl::Class {
            class_id, class, ..
        }) = declarations.get(binding.type_decl.index())
        else {
            continue;
        };
        classes.insert(
            *class_id,
            AbstractClassInfo {
                class,
                owner: reservation.tickets.deferred,
            },
        );
    }

    fn pending_for(
        class_id: ClassId,
        classes: &FxHashMap<ClassId, AbstractClassInfo<'_>>,
        parents: &FxHashMap<ClassId, ClassId>,
        visiting: &mut BTreeSet<ClassId>,
        memo: &mut FxHashMap<ClassId, Vec<String>>,
    ) -> Vec<String> {
        if let Some(pending) = memo.get(&class_id) {
            return pending.clone();
        }
        if !visiting.insert(class_id) {
            return Vec::new();
        }
        let Some(info) = classes.get(&class_id) else {
            visiting.remove(&class_id);
            return Vec::new();
        };
        let (own_abstract, own_concrete) = own_abstract_members(info.class);
        let mut pending = own_abstract.clone();
        if let Some(parent) = parents.get(&class_id).copied() {
            for member in pending_for(parent, classes, parents, visiting, memo) {
                if !own_concrete.contains(&member) && !own_abstract.contains(&member) {
                    pending.push(member);
                }
            }
        }
        visiting.remove(&class_id);
        memo.insert(class_id, pending.clone());
        pending
    }

    let mut memo = FxHashMap::default();
    let mut records = Vec::new();
    let mut ids: Vec<ClassId> = classes.keys().copied().collect();
    ids.sort();
    for class_id in ids {
        let Some(info) = classes.get(&class_id) else {
            continue;
        };
        if info.class.r#abstract {
            continue;
        }
        let pending = pending_for(class_id, &classes, parents, &mut BTreeSet::new(), &mut memo);
        let Some(name) = info.class.id.as_ref() else {
            continue;
        };
        let Some(base_name) = info.class.super_class.as_ref().and_then(|base| match base {
            Expression::Identifier(identifier) => Some(identifier.name.as_str()),
            _ => None,
        }) else {
            continue;
        };
        let diagnostic = match pending.as_slice() {
            [] => continue,
            [member] => Diagnostic::missing_abstract_member(
                CheckSpan::from_oxc(name.span),
                name.name.as_str(),
                member,
                base_name,
            ),
            members => Diagnostic::missing_abstract_members(
                CheckSpan::from_oxc(name.span),
                name.name.as_str(),
                members,
                base_name,
            ),
        };
        records.push(TicketRecord::diagnostic(info.owner, diagnostic));
    }
    records
}

fn own_abstract_members(class: &Class<'_>) -> (Vec<String>, BTreeSet<String>) {
    let mut abstract_members = Vec::new();
    let mut concrete_members = BTreeSet::new();
    let mut push_abstract = |name: String| {
        if !abstract_members.contains(&name) {
            abstract_members.push(name);
        }
    };
    for element in &class.body.body {
        match element {
            ClassElement::PropertyDefinition(property) => {
                if property.computed || property.r#static {
                    continue;
                }
                let Some(name) = property.key.static_name().map(|name| name.into_owned()) else {
                    continue;
                };
                if property.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition {
                    push_abstract(name);
                } else {
                    concrete_members.insert(name);
                }
            }
            ClassElement::MethodDefinition(method) => {
                if method.computed || method.r#static {
                    continue;
                }
                if method.kind == MethodDefinitionKind::Constructor {
                    for parameter in &method.value.params.items {
                        if parameter.accessibility.is_none() && !parameter.readonly {
                            continue;
                        }
                        if let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern {
                            concrete_members.insert(identifier.name.to_string());
                        }
                    }
                    continue;
                }
                let Some(name) = method.key.static_name().map(|name| name.into_owned()) else {
                    continue;
                };
                if method.r#type == MethodDefinitionType::TSAbstractMethodDefinition {
                    push_abstract(name);
                } else {
                    concrete_members.insert(name);
                }
            }
            _ => {}
        }
    }
    (abstract_members, concrete_members)
}

fn class_member_is_method(
    mut class_id: ClassId,
    name: &str,
    classes: &FxHashMap<ClassId, &Class<'_>>,
    parents: &FxHashMap<ClassId, ClassId>,
) -> bool {
    let mut visited = BTreeSet::new();
    while visited.insert(class_id) {
        let Some(class) = classes.get(&class_id) else {
            return false;
        };
        for element in class.body.body.iter().rev() {
            match element {
                ClassElement::PropertyDefinition(property)
                    if !property.computed
                        && !property.r#static
                        && property.key.static_name().as_deref() == Some(name) =>
                {
                    return false;
                }
                ClassElement::MethodDefinition(method)
                    if !method.computed
                        && !method.r#static
                        && method.kind != MethodDefinitionKind::Constructor
                        && method.key.static_name().as_deref() == Some(name) =>
                {
                    return method.kind == MethodDefinitionKind::Method;
                }
                _ => {}
            }
        }
        let Some(parent) = parents.get(&class_id).copied() else {
            return false;
        };
        class_id = parent;
    }
    false
}

fn class_member_has_explicit_surface(
    mut class_id: ClassId,
    name: &str,
    classes: &FxHashMap<ClassId, &Class<'_>>,
    parents: &FxHashMap<ClassId, ClassId>,
) -> bool {
    let mut visited = BTreeSet::new();
    while visited.insert(class_id) {
        let Some(class) = classes.get(&class_id) else {
            return false;
        };
        for element in class.body.body.iter().rev() {
            match element {
                ClassElement::PropertyDefinition(property)
                    if !property.computed
                        && !property.r#static
                        && property.key.static_name().as_deref() == Some(name) =>
                {
                    return property.type_annotation.is_some();
                }
                ClassElement::MethodDefinition(method)
                    if !method.computed
                        && !method.r#static
                        && method.kind != MethodDefinitionKind::Constructor
                        && method.key.static_name().as_deref() == Some(name) =>
                {
                    return match method.kind {
                        MethodDefinitionKind::Method | MethodDefinitionKind::Get => {
                            method.value.return_type.is_some()
                        }
                        MethodDefinitionKind::Set => method
                            .value
                            .params
                            .items
                            .first()
                            .is_some_and(|parameter| parameter.type_annotation.is_some()),
                        MethodDefinitionKind::Constructor => false,
                    };
                }
                _ => {}
            }
        }
        let Some(parent) = parents.get(&class_id).copied() else {
            return false;
        };
        class_id = parent;
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn register_reserved_surface_roots<'ast>(
    construction: &mut ClassConstruction<UserRecordTicket>,
    factory: &mut SurfaceTypeFactory<'_>,
    binder: &crate::binder::Binder,
    declarations: &[TypeDecl<'ast>],
    resolved: &[Option<TypeId>],
    reservations: &LexicalReservations,
    error: TypeId,
) {
    for (index, declaration) in declarations.iter().enumerate() {
        match declaration {
            TypeDecl::Alias { .. } => {
                let Some(root) = resolved.get(index).copied().flatten() else {
                    continue;
                };
                if root != error {
                    let _ = construction.roots_mut().register(
                        root,
                        ReservedRootKind::Alias,
                        Vec::new(),
                    );
                }
            }
            TypeDecl::Interface {
                declaration,
                scope,
                reserved,
                params,
                param_decl,
                extends,
                ..
            } => {
                let Some(source) = reservations.declaration_source(*declaration) else {
                    let _ = construction.roots_mut().register(
                        *reserved,
                        ReservedRootKind::Interface,
                        Vec::new(),
                    );
                    continue;
                };
                let scope = *scope;
                let fallback = reservations
                    .declaration_owner(*declaration)
                    .expect("reserved type declaration retains its lexical owner")
                    .ticket;
                let mut resolver = Resolver {
                    binder,
                    scope,
                    declarations,
                    resolved,
                    reservations,
                    module: source.module_ordinal,
                    fallback,
                    error,
                    qualified_outer_type_parameters_visible: true,
                    application_checks: Vec::new(),
                };
                let frame = param_decl
                    .iter()
                    .flat_map(|declaration| declaration.params.iter())
                    .zip(params.iter().copied())
                    .map(|(parameter, id)| {
                        (
                            parameter.name.name.to_string(),
                            factory.intern_type_param(id, parameter.name.name.as_str()),
                        )
                    })
                    .collect::<Vec<_>>();
                let mut children = Vec::new();
                for heritage in *extends {
                    let Expression::Identifier(identifier) = &heritage.expression else {
                        continue;
                    };
                    let Some(target_id) = type_decl_id(binder, scope, identifier.name.as_str())
                    else {
                        continue;
                    };
                    let Some(target) = declarations.get(target_id.index()) else {
                        continue;
                    };
                    let child = match target {
                        TypeDecl::Class { class_id, .. } => {
                            let parameters = resolver.class_parameters(target);
                            let mut explicit = Vec::new();
                            let mut unavailable = false;
                            if let Some(arguments) = heritage.type_arguments.as_deref() {
                                for argument in &arguments.params {
                                    let (lowered, child_failures) =
                                        lower_type(factory, &mut resolver, argument, &frame);
                                    if !child_failures.is_empty() {
                                        unavailable = true;
                                    }
                                    match lowered {
                                        Ok(argument) => {
                                            explicit.push(ExplicitClassArgument::Ready(argument));
                                        }
                                        Err(_) => unavailable = true,
                                    }
                                }
                            }
                            if unavailable {
                                None
                            } else {
                                match complete_class_arguments(
                                    factory,
                                    ClassApplicationRequest {
                                        class: *class_id,
                                        parameters: &parameters,
                                        source_arguments: SourceClassArguments::Explicit(&explicit),
                                        inferred: &[],
                                        kind: ClassApplicationKind::TypeReference,
                                    },
                                ) {
                                    DemandOutcome::Ready(arguments) => {
                                        Some(factory.intern_class_instance(*class_id, arguments))
                                    }
                                    DemandOutcome::Exhausted(_) => None,
                                }
                            }
                        }
                        TypeDecl::Interface { reserved, .. } => Some(*reserved),
                        TypeDecl::Alias { .. } | TypeDecl::Resolved { .. } => resolved
                            .get(target_id.index())
                            .copied()
                            .flatten()
                            .filter(|root| *root != error),
                        TypeDecl::Unavailable { .. } => None,
                    };
                    children.extend(child);
                }
                let _ = construction.roots_mut().register(
                    *reserved,
                    ReservedRootKind::Interface,
                    children,
                );
            }
            TypeDecl::Class { .. } | TypeDecl::Resolved { .. } | TypeDecl::Unavailable { .. } => {}
        }
    }
}

fn lower_type(
    factory: &mut SurfaceTypeFactory<'_>,
    resolver: &mut Resolver<'_, '_>,
    annotation: &TSType<'_>,
    frame: &[(String, TypeId)],
) -> (
    Result<TypeId, SurfaceTypeFailure<UserRecordTicket>>,
    Vec<SurfaceTypeFailure<UserRecordTicket>>,
) {
    let mut lowerer = TypeSyntaxLowerer::new(factory, resolver);
    let result = lowerer.lower_with_type_parameters(annotation, frame.iter().cloned());
    (result, lowerer.take_child_failures())
}

fn lower_callable(
    factory: &mut SurfaceTypeFactory<'_>,
    resolver: &mut Resolver<'_, '_>,
    function: &oxc_ast::ast::Function<'_>,
    frame: &[(String, TypeId)],
) -> (
    LoweredCallableSyntax<UserRecordTicket>,
    Vec<SurfaceTypeFailure<UserRecordTicket>>,
) {
    let mut lowerer = TypeSyntaxLowerer::new(factory, resolver);
    let result = lowerer.lower_callable_syntax_with_type_parameters(
        function.type_parameters.as_deref(),
        function.this_param.as_deref(),
        &function.params,
        function
            .return_type
            .as_deref()
            .map(|annotation| &annotation.type_annotation),
        function.span,
        frame.iter().cloned(),
    );
    (result, lowerer.take_child_failures())
}

fn seed_initializer_annotations(
    factory: &mut SurfaceTypeFactory<'_>,
    resolver: &mut Resolver<'_, '_>,
    expression: &Expression<'_>,
    frame: &[(String, TypeId)],
    context: &mut InitializerContext,
) {
    match expression {
        Expression::TSAsExpression(assertion) => {
            if let Ok(ty) = lower_type(factory, resolver, &assertion.type_annotation, frame).0 {
                context
                    .annotations
                    .insert(assertion.type_annotation.span().start, ty);
            }
        }
        Expression::TSTypeAssertion(assertion) => {
            if let Ok(ty) = lower_type(factory, resolver, &assertion.type_annotation, frame).0 {
                context
                    .annotations
                    .insert(assertion.type_annotation.span().start, ty);
            }
        }
        Expression::ArrowFunctionExpression(arrow) => {
            for parameter in &arrow.params.items {
                if let Some(annotation) = parameter.type_annotation.as_ref() {
                    if let Ok(ty) =
                        lower_type(factory, resolver, &annotation.type_annotation, frame).0
                    {
                        context
                            .annotations
                            .insert(annotation.type_annotation.span().start, ty);
                    }
                }
            }
        }
        _ => {}
    }
}

fn resolve_parent(
    binder: &crate::binder::Binder,
    scope: ScopeId,
    declarations: &[TypeDecl<'_>],
    class: &Class<'_>,
) -> Option<ClassId> {
    let Expression::Identifier(identifier) = class.super_class.as_ref()? else {
        return None;
    };
    let id = type_decl_id(binder, scope, identifier.name.as_str())?;
    match declarations.get(id.index()) {
        Some(TypeDecl::Class { class_id, .. }) => Some(*class_id),
        _ => None,
    }
}

type HeritageApplicationResult = (
    Option<(ClassId, TypeId)>,
    Vec<SurfaceTypeFailure<UserRecordTicket>>,
);

fn lower_heritage_application(
    factory: &mut SurfaceTypeFactory<'_>,
    resolver: &mut Resolver<'_, '_>,
    binder: &crate::binder::Binder,
    scope: ScopeId,
    declarations: &[TypeDecl<'_>],
    class: &Class<'_>,
    frame: &[(String, TypeId)],
) -> HeritageApplicationResult {
    if class.super_class.is_none() {
        return (None, Vec::new());
    }
    let parent = resolve_parent(binder, scope, declarations, class);
    let mut explicit = Vec::new();
    let mut failures = Vec::new();
    if let Some(source) = class.super_type_arguments.as_deref() {
        for argument in &source.params {
            let (lowered, nested_failures) = lower_type(factory, resolver, argument, frame);
            match lowered {
                Ok(argument) => explicit.push(ExplicitClassArgument::Ready(argument)),
                Err(failure) => failures.push(failure),
            }
            failures.extend(nested_failures);
        }
    }
    if !failures.is_empty() {
        return (None, failures);
    }
    let Some(parent) = parent else {
        return (None, failures);
    };
    let parameters = declarations
        .iter()
        .find(|declaration| {
            matches!(declaration, TypeDecl::Class { class_id, .. } if *class_id == parent)
        })
        .map(|declaration| resolver.class_parameters(declaration))
        .unwrap_or_default();
    let arguments = match complete_class_arguments(
        factory,
        ClassApplicationRequest {
            class: parent,
            parameters: &parameters,
            source_arguments: SourceClassArguments::Explicit(&explicit),
            inferred: &[],
            kind: ClassApplicationKind::TypeReference,
        },
    ) {
        DemandOutcome::Ready(arguments) => arguments,
        DemandOutcome::Exhausted(reason) => {
            failures.push(SurfaceTypeFailure::Exhausted(reason));
            return (None, failures);
        }
    };
    (
        Some((parent, factory.intern_class_instance(parent, arguments))),
        failures,
    )
}

fn own_surface_failure(
    failure: SurfaceTypeFailure<UserRecordTicket>,
    diagnostic_owner: UserRecordTicket,
    incomplete_owner: UserRecordTicket,
    fallback_span: CheckSpan,
    incomplete_id: &str,
    incomplete_context: &str,
) -> (UserRecordTicket, Option<TicketRecord>) {
    match failure {
        SurfaceTypeFailure::Unresolved { span, name, .. } => {
            let diagnostic = Diagnostic::cannot_find_name(CheckSpan::from_oxc(span), &name);
            (
                diagnostic_owner,
                Some(TicketRecord::diagnostic(diagnostic_owner, diagnostic)),
            )
        }
        SurfaceTypeFailure::QualifiedTopology { owner, diagnostic } => {
            (owner, Some(TicketRecord::diagnostic(owner, diagnostic)))
        }
        SurfaceTypeFailure::QualifiedIncomplete {
            owner,
            span,
            id,
            context,
        } => (
            owner,
            Some(TicketRecord::incomplete(
                owner,
                IncompleteSurface::new(id, CheckSpan::from_oxc(span), context),
            )),
        ),
        SurfaceTypeFailure::WrongArity {
            span,
            name,
            expected_min,
            expected_max,
        } => {
            let diagnostic = if expected_max == 0 {
                Diagnostic::type_is_not_generic(CheckSpan::from_oxc(span), &name)
            } else if expected_min == expected_max {
                Diagnostic::generic_type_requires_arguments(
                    CheckSpan::from_oxc(span),
                    &name,
                    expected_max,
                )
            } else {
                Diagnostic::generic_type_requires_argument_range(
                    CheckSpan::from_oxc(span),
                    &name,
                    expected_min,
                    expected_max,
                )
            };
            (
                diagnostic_owner,
                Some(TicketRecord::diagnostic(diagnostic_owner, diagnostic)),
            )
        }
        SurfaceTypeFailure::Unsupported(_) => {
            let incomplete =
                IncompleteSurface::new(incomplete_id, fallback_span, incomplete_context);
            (
                incomplete_owner,
                Some(TicketRecord::incomplete(incomplete_owner, incomplete)),
            )
        }
        SurfaceTypeFailure::TypeQuery { span } => (
            incomplete_owner,
            Some(TicketRecord::incomplete(
                incomplete_owner,
                IncompleteSurface::new(
                    "annotation-lower/type-query/typeof",
                    CheckSpan::from_oxc(span),
                    "typeof type query not lowered",
                ),
            )),
        ),
        SurfaceTypeFailure::ThisType { span } => (
            incomplete_owner,
            Some(TicketRecord::incomplete(
                incomplete_owner,
                IncompleteSurface::new(
                    "annotation-lower/this-type/self",
                    CheckSpan::from_oxc(span),
                    "this type annotation not modeled",
                ),
            )),
        ),
        SurfaceTypeFailure::Poisoned(_) => (incomplete_owner, None),
        SurfaceTypeFailure::Exhausted(Exhaustion::ClassApplicationArguments(
            ClassApplicationArguments::UnsupportedDefault { .. },
        )) => (
            incomplete_owner,
            Some(TicketRecord::incomplete(
                incomplete_owner,
                IncompleteSurface::new(
                    "annotation-lower/type-reference/class-default-argument",
                    fallback_span,
                    "class type-parameter default unavailable at application",
                ),
            )),
        ),
        SurfaceTypeFailure::Exhausted(_) => (incomplete_owner, None),
    }
}
