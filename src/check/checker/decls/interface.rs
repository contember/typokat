use super::resolve::QualifiedTypeSegment;
use super::*;
use crate::binder::scope::ScopeId;
use crate::span::Span;
use crate::types::repr::{ObjectType, PropertyType};
use crate::types::store::TypeId;
use oxc_ast::ast::{Expression, TSInterfaceHeritage, TSSignature};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Default)]
struct InterfaceMethodOverloadAccumulator {
    call_signatures: Vec<TypeId>,
    unsupported: bool,
    unavailable: bool,
}

fn flatten_qualified_heritage_expression<'a>(
    expression: &'a Expression<'_>,
    segments: &mut Vec<QualifiedTypeSegment<'a>>,
) -> bool {
    match expression {
        Expression::Identifier(identifier) => {
            segments.push(QualifiedTypeSegment {
                name: identifier.name.as_str(),
                span: Span::from_oxc(identifier.span),
            });
            true
        }
        Expression::StaticMemberExpression(member) => {
            if !flatten_qualified_heritage_expression(&member.object, segments) {
                return false;
            }
            segments.push(QualifiedTypeSegment {
                name: member.property.name.as_str(),
                span: Span::from_oxc(member.property.span),
            });
            true
        }
        _ => false,
    }
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    pub(super) fn interface_heritage_root_replay<'name>(
        &self,
        scope: ScopeId,
        heritage: &'name TSInterfaceHeritage<'_>,
    ) -> Option<(crate::binder::declaration::TypeGroupId, &'name str, Span)> {
        match &heritage.expression {
            Expression::Identifier(identifier) => Some((
                self.type_decl_id_replay(scope, identifier.name.as_str())?,
                identifier.name.as_str(),
                Span::from_oxc(identifier.span),
            )),
            Expression::StaticMemberExpression(member) => {
                let mut segments = Vec::new();
                if !flatten_qualified_heritage_expression(&heritage.expression, &mut segments) {
                    return None;
                }
                let names = segments
                    .iter()
                    .map(|segment| segment.name)
                    .collect::<Vec<_>>();
                let crate::binder::namespace::QualifiedTypePathResolution::TypeGroup(group) =
                    self.resolve_qualified_type_path_replay(scope, &names)
                else {
                    return None;
                };
                Some((group, segments.last()?.name, Span::from_oxc(member.span)))
            }
            _ => None,
        }
    }

    pub(super) fn validate_interface_heritage_application_without_resolution(
        &mut self,
        scope: ScopeId,
        heritage: &TSInterfaceHeritage<'_>,
    ) {
        let Some((group, name, span)) = self.interface_heritage_root_replay(scope, heritage) else {
            return;
        };
        self.validate_type_group_application_without_resolution(
            scope,
            group,
            name,
            span,
            heritage.type_arguments.as_deref(),
        );
    }

    pub(super) fn record_opaque_interface_heritage(
        &mut self,
        scope: ScopeId,
        heritage: &TSInterfaceHeritage<'_>,
    ) {
        if let Some(arguments) = &heritage.type_arguments {
            for argument in &arguments.params {
                let _ = self.lower_annotation(scope, argument);
            }
        }
        self.record_incomplete(
            "interface/heritage/topology",
            Span::from_oxc(heritage.span),
            "heritage topology is outside the alias-transparent interface model",
        );
    }

    pub(super) fn diagnose_poisoned_interface_heritage(
        &mut self,
        scope: ScopeId,
        heritage: &TSInterfaceHeritage<'_>,
    ) {
        match &heritage.expression {
            Expression::Identifier(identifier) => {
                if let Some(group) = self.type_decl_id_replay(scope, identifier.name.as_str()) {
                    let _ = self.resolve_type_group_reference(
                        scope,
                        group,
                        identifier.name.as_str(),
                        Span::from_oxc(identifier.span),
                        heritage.type_arguments.as_deref(),
                    );
                    return;
                }
                if self
                    .resolve_type_replay(scope, identifier.name.as_str())
                    .is_none()
                    && self
                        .resolve_value_replay(scope, identifier.name.as_str())
                        .is_none()
                {
                    self.emit_diagnostic(crate::diagnostics::Diagnostic::cannot_find_name(
                        Span::from_oxc(identifier.span),
                        identifier.name.as_str(),
                    ));
                }
                if let Some(arguments) = &heritage.type_arguments {
                    for argument in &arguments.params {
                        let _ = self.lower_annotation(scope, argument);
                    }
                }
            }
            Expression::StaticMemberExpression(member) => {
                let mut segments = Vec::new();
                if flatten_qualified_heritage_expression(&heritage.expression, &mut segments) {
                    self.classify_qualified_type_path(
                        scope,
                        &segments,
                        Span::from_oxc(member.span),
                        Span::from_oxc(heritage.span),
                        heritage.type_arguments.as_deref(),
                    );
                }
            }
            _ => {
                if let Some(arguments) = &heritage.type_arguments {
                    for argument in &arguments.params {
                        let _ = self.lower_annotation(scope, argument);
                    }
                }
            }
        }
    }

    /// Merge source-ordered fragments of one interface identity. Ordinary members
    /// recover to the first declaration; callable members accumulate overloads in
    /// source order so identical-applicability ties select the earlier signature.
    pub(super) fn merge_interface_fragment_members(
        &mut self,
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
            let mut overloads = match self.interner.store().tag(existing.ty) {
                crate::types::repr::TypeTag::Function => vec![existing.ty],
                crate::types::repr::TypeTag::Object => self
                    .interner
                    .store()
                    .object_type(existing.ty)
                    .filter(|object| !object.call_signatures.is_empty())
                    .map(|object| object.call_signatures.clone())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let appended = match self.interner.store().tag(property.ty) {
                crate::types::repr::TypeTag::Function => Some(vec![property.ty]),
                crate::types::repr::TypeTag::Object => self
                    .interner
                    .store()
                    .object_type(property.ty)
                    .filter(|object| !object.call_signatures.is_empty())
                    .map(|object| object.call_signatures.clone()),
                _ => None,
            };
            if !overloads.is_empty() {
                if let Some(appended) = appended {
                    overloads.extend(appended);
                    existing.ty = self.interner.intern_object(ObjectType {
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

    /// Resolve and project one already-terminal heritage endpoint without invoking
    /// the semantic query coordinator. Construction may only read frozen roots.
    pub(super) fn resolve_interface_heritage_object(
        &mut self,
        scope: ScopeId,
        heritage: &TSInterfaceHeritage<'_>,
    ) -> Option<ObjectType> {
        let base_ty = self.resolve_heritage_type(scope, heritage)?;
        self.project_interface_heritage_type(base_ty)
    }

    fn project_interface_heritage_type(&mut self, ty: TypeId) -> Option<ObjectType> {
        if let Some(object) = self.interner.store().object_type(ty).cloned() {
            return Some(object);
        }
        if let Some(members) = self
            .interner
            .store()
            .intersection_members(ty)
            .map(<[_]>::to_vec)
        {
            let objects = members
                .into_iter()
                .map(|member| self.project_interface_heritage_type(member))
                .collect::<Option<Vec<_>>>()?;
            return Some(merge_intersection_objects(self.interner, objects));
        }
        let application = self.interner.store().class_instance_type(ty).cloned()?;
        if let Some(trace) = &self.replay_trace {
            let _observation = trace.observe_typed_demand("interface-class-projection");
            trace.demand_at(
                super::super::replay_index::ReplayOwner::Class(application.class),
                "interface-class-projection",
            );
        }
        let crate::class_semantics::DemandOutcome::Ready(surface) = self
            .staged_published_classes
            .as_ref()
            .expect("class registry is frozen before interface heritage construction")
            .published_class(application.class)
        else {
            return None;
        };
        let substitutions: FxHashMap<_, _> = surface
            .type_params()
            .iter()
            .copied()
            .zip(application.args)
            .collect();
        let projected =
            crate::types::substitute(self.interner, surface.instance_template(), &substitutions);
        self.interner.store().object_type(projected).cloned()
    }

    /// B28 — resolve a heritage clause's base to its `TypeId`: a bare interface/alias
    /// reference resolves through `type_resolved`; a generic base (`extends Base<T>`)
    /// instantiates its template with the lowered arguments. Non-identifier bases (out of
    /// subset) yield `None`.
    pub(super) fn resolve_heritage_type(
        &mut self,
        scope: ScopeId,
        heritage: &TSInterfaceHeritage<'_>,
    ) -> Option<TypeId> {
        let (decl_id, name, span) = match &heritage.expression {
            Expression::Identifier(ident) => (
                self.type_decl_id_replay(scope, ident.name.as_str())?,
                ident.name.as_str(),
                Span::from_oxc(ident.span),
            ),
            Expression::StaticMemberExpression(member) => {
                let mut segments = Vec::new();
                if flatten_qualified_heritage_expression(&heritage.expression, &mut segments) {
                    let names: Vec<&str> = segments.iter().map(|segment| segment.name).collect();
                    if let crate::binder::namespace::QualifiedTypePathResolution::TypeGroup(group) =
                        self.resolve_qualified_type_path_replay(scope, &names)
                    {
                        (group, segments.last()?.name, Span::from_oxc(member.span))
                    } else {
                        self.classify_qualified_type_path(
                            scope,
                            &segments,
                            Span::from_oxc(member.span),
                            Span::from_oxc(heritage.span),
                            heritage.type_arguments.as_deref(),
                        );
                        return None;
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        self.resolve_type_group_reference(
            scope,
            decl_id,
            name,
            span,
            heritage.type_arguments.as_deref(),
        )
    }

    /// Lower interface members to the reserved nominal object's `ObjectType`.
    /// Unsupported or unlowerable members are skipped; the interface keeps the
    /// expressible subset.
    pub(super) fn lower_interface_members(
        &mut self,
        scope: ScopeId,
        members: &[TSSignature<'_>],
    ) -> ObjectType {
        self.lower_interface_members_inner(scope, members, None)
    }

    pub(super) fn lower_interface_declaration_members(
        &mut self,
        declaration: crate::binder::declaration::DeclId,
        scope: ScopeId,
        members: &[TSSignature<'_>],
    ) -> ObjectType {
        self.lower_interface_members_inner(scope, members, Some(declaration))
    }

    fn lower_interface_members_inner(
        &mut self,
        scope: ScopeId,
        members: &[TSSignature<'_>],
        declaration: Option<crate::binder::declaration::DeclId>,
    ) -> ObjectType {
        let mut object = ObjectType::default();
        let overloaded_method_names = self.overloaded_method_names(members);
        let mut overloads: FxHashMap<String, InterfaceMethodOverloadAccumulator> =
            FxHashMap::default();
        let mut overload_order = Vec::new();
        for member in members {
            let mut lower = |pass: &mut Self| match member {
                TSSignature::TSPropertySignature(sig) => {
                    if sig.computed {
                        pass.record_property_signature_computed_key(&sig.key);
                        if let Some(annotation) = sig.type_annotation.as_ref() {
                            pass.lower_annotation(scope, &annotation.type_annotation);
                        }
                        return;
                    }
                    let Some(name) = sig.key.static_name() else {
                        pass.record_property_signature_computed_key(&sig.key);
                        if let Some(annotation) = sig.type_annotation.as_ref() {
                            pass.lower_annotation(scope, &annotation.type_annotation);
                        }
                        return;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        let name = name.into_owned();
                        if !overloads.contains_key(&name) {
                            overload_order.push(name.clone());
                        }
                        let overload = overloads.entry(name).or_default();
                        overload.unsupported = true;
                        let lowered = sig.type_annotation.as_ref().and_then(|annotation| {
                            #[cfg(test)]
                            super::super::declaration_surface_measure::record_eager_interface_property_root();
                            pass.lower_annotation(scope, &annotation.type_annotation)
                        });
                        if lowered.is_none() {
                            overload.unavailable = true;
                        }
                        return;
                    }
                    let ty = match sig.type_annotation.as_ref() {
                        Some(annotation) => {
                            if let Some(ty) = pass.try_plan_declared_interface_property_annotation(
                                scope,
                                &annotation.type_annotation,
                            ) {
                                #[cfg(test)]
                                super::super::declaration_surface_measure::record_planned_interface_property_root();
                                ty
                            } else {
                                #[cfg(test)]
                                super::super::declaration_surface_measure::record_eager_interface_property_root();
                                let Some(ty) =
                                    pass.lower_annotation(scope, &annotation.type_annotation)
                                else {
                                    return;
                                };
                                ty
                            }
                        }
                        // tsc treats annotationless interface properties as `any`.
                        None => pass.interner.well_known().any,
                    };
                    // Optional properties are real members with `| undefined` baked in
                    // while interning is available. The read-only relation engine then
                    // uses existing union-target logic, and `keyof`/indexed access see
                    // the key.
                    let ty = if sig.optional {
                        let undefined = pass.interner.well_known().undefined;
                        pass.interner.union(vec![ty, undefined])
                    } else {
                        ty
                    };
                    let mut prop = PropertyType::public(name.into_owned(), ty);
                    prop.optional = sig.optional;
                    // Preserve `readonly` on interface members. It is hashed into
                    // structural identity, ignored for assignability, and gates only
                    // assignment targets (`TK2540`).
                    prop.readonly = sig.readonly;
                    object.properties.push(prop);
                }
                // M19: an index signature on an interface — lowered into the
                // string/number slot. An unsupported one (non-`string`/`number` key,
                // un-lowerable value) is **skipped** (lenient, like an out-of-subset
                // property), so the interface keeps the members it can express.
                TSSignature::TSIndexSignature(sig) => {
                    let _ = pass.lower_index_signature(scope, sig, &mut object);
                }
                TSSignature::TSMethodSignature(sig) => {
                    if sig.computed {
                        pass.record_method_signature_computed_key(&sig.key);
                        pass.lower_generic_strict_signature_function_type(
                            scope,
                            sig.type_parameters.as_deref(),
                            sig.this_param.as_deref(),
                            &sig.params,
                            sig.return_type.as_deref(),
                        );
                        return;
                    }
                    let Some(name) = sig.key.static_name() else {
                        pass.record_method_signature_computed_key(&sig.key);
                        pass.lower_generic_strict_signature_function_type(
                            scope,
                            sig.type_parameters.as_deref(),
                            sig.this_param.as_deref(),
                            &sig.params,
                            sig.return_type.as_deref(),
                        );
                        return;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        let name = name.into_owned();
                        if !overloads.contains_key(&name) {
                            overload_order.push(name.clone());
                        }
                        let overload = overloads.entry(name).or_default();
                        let signature = pass.lower_generic_strict_signature_function_type(
                            scope,
                            sig.type_parameters.as_deref(),
                            sig.this_param.as_deref(),
                            &sig.params,
                            sig.return_type.as_deref(),
                        );
                        if sig.kind != oxc_ast::ast::TSMethodSignatureKind::Method || sig.optional {
                            overload.unsupported = true;
                        }
                        match signature {
                            Some(signature)
                                if sig.kind == oxc_ast::ast::TSMethodSignatureKind::Method
                                    && !sig.optional =>
                            {
                                overload.call_signatures.push(signature);
                            }
                            Some(_) => {}
                            None => overload.unavailable = true,
                        }
                        return;
                    }
                    if let Some(prop) = pass.lower_method_signature_property(scope, sig) {
                        object.properties.push(prop);
                    }
                }
                TSSignature::TSCallSignatureDeclaration(sig) => {
                    if let Some(signature) = pass.lower_call_signature(scope, sig) {
                        object.call_signatures.push(signature);
                    }
                }
                TSSignature::TSConstructSignatureDeclaration(sig) => {
                    if let Some(signature) = pass.lower_construct_signature(scope, sig) {
                        object.construct_signatures.push(signature);
                    }
                }
            };
            if let Some(declaration) = declaration {
                let owner = self
                    .lexical_events
                    .interface_occurrence_owner(
                        declaration,
                        InterfaceOccurrenceKind::Member,
                        member.span().start,
                    )
                    .expect("interface member has one exact preallocated owner");
                self.with_ticket_effects(owner, lower);
            } else {
                lower(self);
            }
        }
        for name in overload_order {
            let overload = overloads
                .remove(&name)
                .expect("every interface overload retains its accumulator");
            if overload.unavailable || overload.call_signatures.is_empty() {
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
        object
    }
}

/// Compose an interface's own surface over its already validated heritage surface.
pub(super) fn merge_object_members_overlay(base: ObjectType, overlay: ObjectType) -> ObjectType {
    let mut properties = base.properties;
    for prop in overlay.properties {
        match properties.iter_mut().find(|p| p.name == prop.name) {
            Some(existing) => *existing = prop,
            None => properties.push(prop),
        }
    }
    let mut call_signatures = overlay.call_signatures;
    call_signatures.extend(base.call_signatures);
    let mut construct_signatures = overlay.construct_signatures;
    construct_signatures.extend(base.construct_signatures);
    ObjectType {
        properties,
        string_index: overlay.string_index.or(base.string_index),
        number_index: overlay.number_index.or(base.number_index),
        call_signatures,
        construct_signatures,
    }
}

#[cfg(test)]
thread_local! {
    static HERITAGE_BASE_NAME_PROBES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// One probe per member name tested for membership in the composed base surface. Nothing in
/// the output can see the difference between a scan and a lookup, so the probe count is the
/// only witness that composing a heritage chain stays linear.
#[cfg(test)]
fn record_heritage_base_name_probes_for_test(probes: usize) {
    let probes = u64::try_from(probes).unwrap_or(u64::MAX);
    HERITAGE_BASE_NAME_PROBES.with(|counter| counter.set(counter.get().saturating_add(probes)));
}

#[cfg(test)]
pub(in crate::check::checker) struct HeritageBaseNameProbeScopeForTest(u64);

#[cfg(test)]
impl HeritageBaseNameProbeScopeForTest {
    pub(in crate::check::checker) fn start() -> Self {
        Self(HERITAGE_BASE_NAME_PROBES.with(std::cell::Cell::get))
    }

    pub(in crate::check::checker) fn finish(self) -> u64 {
        HERITAGE_BASE_NAME_PROBES
            .with(std::cell::Cell::get)
            .saturating_sub(self.0)
    }
}

/// Compose base surfaces in source order without letting later conflicts replace the first.
///
/// One name set spans the whole chain. Folding the bases pairwise instead re-tests every
/// member already accumulated, which is quadratic in a composed surface that reaches
/// hundreds of members.
pub(super) fn compose_base_members_first(bases: &[&ObjectType]) -> ObjectType {
    let member_count = bases.iter().map(|base| base.properties.len()).sum();
    let mut names = FxHashSet::with_capacity_and_hasher(member_count, Default::default());
    let mut properties = Vec::with_capacity(member_count);
    let mut string_index = None;
    let mut number_index = None;
    let mut call_signatures = Vec::new();
    let mut construct_signatures = Vec::new();
    for base in bases {
        for property in &base.properties {
            #[cfg(test)]
            record_heritage_base_name_probes_for_test(1);
            if names.insert(property.name.as_str()) {
                properties.push(property.clone());
            }
        }
        string_index = string_index.or(base.string_index);
        number_index = number_index.or(base.number_index);
        call_signatures.extend(base.call_signatures.iter().copied());
        construct_signatures.extend(base.construct_signatures.iter().copied());
    }
    ObjectType {
        properties,
        string_index,
        number_index,
        call_signatures,
        construct_signatures,
    }
}

/// Materialize the apparent object of an intersection without a semantic query.
/// This is the construction-time counterpart of `intersection_apparent_object`:
/// duplicate member/index types intersect and every distinct member is retained.
pub(super) fn merge_intersection_objects(
    interner: &mut Interner,
    objects: Vec<ObjectType>,
) -> ObjectType {
    struct PropertyAccumulator {
        base: PropertyType,
        types: Vec<TypeId>,
        write_types: Vec<TypeId>,
        has_write_type: bool,
        all_optional: bool,
        any_readonly: bool,
        any_accessor: bool,
    }

    let mut order = Vec::new();
    let mut properties: BTreeMap<String, PropertyAccumulator> = BTreeMap::new();
    let mut string_indices = Vec::new();
    let mut number_indices = Vec::new();
    let mut call_signatures = Vec::new();
    let mut construct_signatures = Vec::new();
    for object in objects {
        for property in object.properties {
            let write_type = property.write_ty.unwrap_or(property.ty);
            if let Some(existing) = properties.get_mut(&property.name) {
                existing.types.push(property.ty);
                existing.write_types.push(write_type);
                existing.has_write_type |= property.write_ty.is_some();
                existing.all_optional &= property.optional;
                existing.any_readonly |= property.readonly;
                existing.any_accessor |= property.is_accessor;
            } else {
                order.push(property.name.clone());
                properties.insert(
                    property.name.clone(),
                    PropertyAccumulator {
                        all_optional: property.optional,
                        any_readonly: property.readonly,
                        any_accessor: property.is_accessor,
                        types: vec![property.ty],
                        write_types: vec![write_type],
                        has_write_type: property.write_ty.is_some(),
                        base: property,
                    },
                );
            }
        }
        string_indices.extend(object.string_index);
        number_indices.extend(object.number_index);
        call_signatures.extend(object.call_signatures);
        construct_signatures.extend(object.construct_signatures);
    }

    let properties = order
        .into_iter()
        .filter_map(|name| {
            let accumulator = properties.remove(&name)?;
            let ty = interner.intersection(accumulator.types);
            let write_ty = accumulator
                .has_write_type
                .then(|| interner.intersection(accumulator.write_types));
            Some(PropertyType {
                ty,
                write_ty,
                optional: accumulator.all_optional,
                readonly: accumulator.any_readonly,
                is_accessor: accumulator.any_accessor,
                ..accumulator.base
            })
        })
        .collect();
    ObjectType {
        properties,
        string_index: (!string_indices.is_empty()).then(|| interner.intersection(string_indices)),
        number_index: (!number_indices.is_empty()).then(|| interner.intersection(number_indices)),
        call_signatures,
        construct_signatures,
    }
}

#[cfg(test)]
mod qualified_heritage_tests {
    use crate::diagnostics::DiagnosticCode;
    use crate::driver::{check_source, CheckOutput};
    use crate::span::Span;

    fn checked(source: &str) -> CheckOutput {
        let output = check_source(source);
        assert!(
            output.parse_errors.is_empty(),
            "unexpected parse errors: {:?}",
            output.parse_errors
        );
        output
    }

    fn span_text(source: &str, span: Span) -> &str {
        let start = usize::try_from(span.start).expect("source span start fits usize");
        let end = usize::try_from(span.end).expect("source span end fits usize");
        &source[start..end]
    }

    #[test]
    fn failed_qualified_heritage_replays_path_then_type_argument_at_interface_owner() {
        let source = "interface D extends Missing.Root<Unknown> {}";
        let output = checked(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.code,
                    diagnostic.message.as_str(),
                    span_text(source, diagnostic.span),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    DiagnosticCode::TK2503,
                    "Cannot find namespace 'Missing'.",
                    "Missing",
                ),
                (
                    DiagnosticCode::TK2304,
                    "Cannot find name 'Unknown'",
                    "Unknown",
                ),
            ]
        );
        assert!(output
            .incomplete
            .iter()
            .all(|record| record.id != "annotation-lower/type-name/qualified-name"));
    }

    #[test]
    fn successful_qualified_heritage_uses_the_published_type_endpoint() {
        let source = "\
namespace HeritageNs { export interface Base {} }
interface D extends HeritageNs.Base {}
";
        let output = checked(source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    }

    #[test]
    fn qualified_heritage_member_lookup_never_falls_back_to_parent_namespace() {
        let source = "\
namespace Root {
  export interface ParentLeaf {}
  export namespace Child {}
}
interface D extends Root.Child.ParentLeaf {}
";
        let output = checked(source);
        assert_eq!(output.diagnostics.len(), 1);
        let diagnostic = &output.diagnostics[0];
        assert_eq!(diagnostic.code, DiagnosticCode::TK2694);
        assert_eq!(
            diagnostic.message,
            "Namespace 'Root.Child' has no exported member 'ParentLeaf'."
        );
        assert_eq!(span_text(source, diagnostic.span), "ParentLeaf");
        assert!(output
            .incomplete
            .iter()
            .all(|record| record.id != "annotation-lower/type-name/qualified-name"));
    }

    #[test]
    fn computed_generic_method_children_keep_constraint_parameter_return_order() {
        let source = r#"
declare const computed: "computed";
interface I {
  [computed]<T extends MissingConstraint.Member>(value: MissingParameter.Member): MissingReturn.Member;
}
"#;
        let output = checked(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.code,
                    span_text(source, diagnostic.span).to_string(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticCode::TK2503, "MissingConstraint".to_string()),
                (DiagnosticCode::TK2503, "MissingParameter".to_string()),
                (DiagnosticCode::TK2503, "MissingReturn".to_string()),
            ]
        );
        assert!(output.incomplete.iter().any(|record| {
            record.id == "signature/method-signature/computed-key"
                && span_text(source, record.span) == "computed"
        }));
    }
}
