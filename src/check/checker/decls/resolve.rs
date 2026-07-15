use super::*;
use crate::binder::declaration::TypeGroupId;
use crate::binder::namespace::QualifiedTypePathResolution;
use crate::binder::scope::ScopeId;
use crate::check::checker::classes::application::{
    build_class_application, complete_class_arguments, ClassApplicationKind,
    ClassApplicationRequest, ClassTypeParameter, ClassTypeParameterDefault, ExplicitClassArgument,
    SourceClassArguments,
};
use crate::check::checker::classes::surface_types::SurfaceTypeFactory;
use crate::check::checker::type_groups::{
    PublishedTypeGroupSurface, PublishedTypeGroupTerminal, PublishedTypeParameterDefault,
};
use crate::class_semantics::{
    ClassApplicationArguments, ClassConstructionState, DemandOutcome, Exhaustion,
};
use crate::diagnostics::{
    qualified_type_incomplete, qualified_type_topology_diagnostic, Diagnostic,
};
use crate::span::Span;
use crate::types::repr::{TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::substitute;
use oxc_ast::ast::{TSTypeName, TSTypeParameterInstantiation};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct QualifiedTypeSegment<'a> {
    pub(super) name: &'a str,
    pub(super) span: Span,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum QualifiedTypeDisposition {
    Ready(TypeId),
    RecoveredTopologyFailure,
    IncompleteEndpoint,
    UnavailableEndpoint,
}

fn flatten_qualified_type_name<'a>(
    type_name: &'a TSTypeName<'_>,
    segments: &mut Vec<QualifiedTypeSegment<'a>>,
) -> bool {
    match type_name {
        TSTypeName::IdentifierReference(identifier) => {
            segments.push(QualifiedTypeSegment {
                name: identifier.name.as_str(),
                span: Span::from_oxc(identifier.span),
            });
            true
        }
        TSTypeName::QualifiedName(qualified) => {
            if !flatten_qualified_type_name(&qualified.left, segments) {
                return false;
            }
            segments.push(QualifiedTypeSegment {
                name: qualified.right.name.as_str(),
                span: Span::from_oxc(qualified.right.span),
            });
            true
        }
        TSTypeName::ThisExpression(_) => false,
    }
}

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Resolve one type declaration to a memoized `TypeId`.
    /// Surface alias cycles report `TK2456` at every alias in the cycle and become
    /// error types; legal recursion through members uses seeded reserved ids instead
    /// of re-entering resolution.
    pub(super) fn resolve_type_decl_inner(
        &mut self,
        _scope: ScopeId,
        decl_id: TypeGroupId,
    ) -> TypeId {
        let error_ty = self.interner.well_known().error;

        // Trusted host markers are seeded before lowering so their syntax still records
        // coverage while the host-provided terminal wins. Other filled groups return.
        let existing = self.type_resolved.get(decl_id.index()).copied().flatten();
        let trusted_seed = existing.filter(|existing| {
            self.interner
                .well_known()
                .is_string_intrinsic_marker(*existing)
                || *existing == self.interner.well_known().this_type
                || *existing == self.interner.well_known().omit_this_parameter
        });
        if let Some(existing) = existing {
            if trusted_seed.is_none() || !self.type_group_construction_is_pending(decl_id) {
                return existing;
            }
        }

        let index = decl_id.index();

        // Same-depth re-entry is a surface alias cycle: report `TK2456` for the cycle.
        // Deeper re-entry came through a type constructor and is legal recursion, so
        // silently error-type it; seeded object aliases handle member self-reference.
        if matches!(
            self.type_decls.get(index),
            Some(TypeDecl::Alias {
                resolving: true,
                ..
            })
        ) {
            let start_depth = self
                .resolving_alias_stack
                .iter()
                .find(|(id, ..)| *id == decl_id)
                .map(|&(_, _, _, depth)| depth);
            return match start_depth {
                Some(depth) if self.alias_indirection_depth == depth => {
                    self.report_surface_cycle(decl_id)
                }
                _ => error_ty,
            };
        }

        // Capture the alias annotation and its (M9) type-parameter frame inputs before
        // mutating, so the body is lowered with the parameters in scope. The name +
        // name span feed the M26 `resolving_alias` context (mapped `TK2456`) and the B29
        // cycle stack.
        let (scope, annotation, param_decl, params, name, name_span) =
            match self.type_decls.get(index) {
                Some(TypeDecl::Alias {
                    scope,
                    annotation,
                    param_decl,
                    params,
                    resolving: false,
                    name,
                    name_span,
                    ..
                }) => (
                    *scope,
                    *annotation,
                    *param_decl,
                    params.clone(),
                    name.clone(),
                    *name_span,
                ),
                // An interface with no seeded id, or an out-of-range id: defensive.
                _ => return error_ty,
            };

        // Mark in-progress so a transitive self-reference is caught above.
        self.begin_type_group_construction(decl_id);
        if let Some(TypeDecl::Alias { resolving, .. }) = self.type_decls.get_mut(index) {
            *resolving = true;
        }

        // M26: record which alias is being resolved (save/restore — alias resolution
        // nests), so a mapped key source that surface-references THIS alias is `TK2456`
        // at the declaration rather than a silent error-type key source. B29: also push
        // the resolving-alias stack, so a surface cycle can name every alias in it.
        let prev_resolving_alias = self.resolving_alias.take();
        self.resolving_alias = Some((decl_id, name_span, name.clone()));
        self.resolving_alias_stack
            .push((decl_id, name_span, name, self.alias_indirection_depth));

        // M9: lower the annotation with the alias's type parameters in scope, so a
        // reference to `A`/`B` in `type Pair<A, B> = { … }` resolves to the parameter
        // type. The frame is popped before returning (a parameter does not leak).
        let frame = self.build_type_param_frame(param_decl, &params);
        let target = self
            .with_type_params(frame, |pass| {
                // M24: lower the parameters' `extends` constraints with the frame active.
                pass.lower_annotation(scope, annotation)
            })
            .unwrap_or(error_ty);

        self.resolving_alias_stack.pop();
        self.resolving_alias = prev_resolving_alias;
        if let Some(TypeDecl::Alias { resolving, .. }) = self.type_decls.get_mut(index) {
            *resolving = false;
        }
        // B29: a confirmed surface-cycle member is the error type (final, not provisional
        // — a detected cycle is settled), so downstream stays silent (M22).
        let final_ty = if self.circular_aliases.contains(&index) {
            error_ty
        } else {
            trusted_seed.unwrap_or(target)
        };
        if let Some(slot) = self.type_resolved.get_mut(index) {
            *slot = Some(final_ty);
        }
        self.freeze_type_group(decl_id);
        final_ty
    }

    /// B29 — a surface cycle re-entered `decl_id`. Report `TK2456` at every alias from
    /// `decl_id`'s position on the resolving-alias stack up to the top (the whole cycle),
    /// deduped via `circular_aliases`, and return the error type. Cloning the slice keeps
    /// the diagnostic/set writes off the stack borrow.
    fn report_surface_cycle(&mut self, decl_id: TypeGroupId) -> TypeId {
        let cycle: Vec<(TypeGroupId, Span, String)> = self
            .resolving_alias_stack
            .iter()
            .position(|(id, ..)| *id == decl_id)
            .map(|pos| {
                self.resolving_alias_stack[pos..]
                    .iter()
                    .map(|(id, span, name, _)| (*id, *span, name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (id, span, name) in cycle {
            if self.circular_aliases.insert(id.index()) {
                self.emit_type_decl_diagnostic(id, Diagnostic::circular_type_alias(span, &name));
            }
        }
        self.interner.well_known().error
    }

    /// Resolve a `TSTypeReference` to a `TypeId`.
    /// Resolution order is in-scope type parameter, generic instantiation, then bare
    /// named type. Truly unresolved simple names report `TK2304` and become error
    /// types; value-as-type, applied type parameters, and qualified names are deferred.
    pub(in crate::check::checker) fn resolve_type_reference(
        &mut self,
        scope: ScopeId,
        type_name: &TSTypeName<'_>,
        type_arguments: Option<&TSTypeParameterInstantiation<'_>>,
        reference_span: Span,
    ) -> Option<TypeId> {
        let ident = match type_name {
            TSTypeName::IdentifierReference(ident) => ident,
            TSTypeName::QualifiedName(qualified) => {
                return self.resolve_qualified_type_reference(
                    scope,
                    qualified,
                    type_arguments,
                    reference_span,
                );
            }
            TSTypeName::ThisExpression(this_name) => {
                self.record_incomplete(
                    "annotation-lower/type-name/this",
                    Span::from_oxc(this_name.span),
                    "this type name not resolved",
                );
                return None;
            }
        };
        let name = ident.name.as_str();
        let ref_span = Span::from_oxc(ident.span);

        // M25: active `infer` binders shadow named types and take no arguments.
        // Cross-binder references resolve but poison intervening nodes; names in no
        // active frame fall through to `TK2304`.
        if type_arguments.is_none() {
            if let Some(infer_ty) = self.resolve_infer_reference(name) {
                return Some(infer_ty);
            }
        }

        // 1. A type parameter in scope shadows any named type and takes no arguments.
        if type_arguments.is_none() {
            if let Some(param_ty) = self.lookup_type_param(name) {
                return Some(param_ty);
            }
        }

        if self.static_class_type_param_reference(name) {
            self.emit_diagnostic(Diagnostic::static_member_references_class_type_parameter(
                ref_span,
            ));
            return Some(self.interner.well_known().error);
        }

        // M17: intercept built-in `Array<T>` without `lib.d.ts`; user shadowing and
        // wrong arity are deferred. Every `Array` path returns here so bad arity never
        // falls through to the unresolved-name arm and emits `TK2304`.
        if name == "Array" {
            match type_arguments {
                Some(args) if args.params.len() == 1 => {
                    let element = self.lower_annotation(scope, &args.params[0])?;
                    return Some(self.interner.intern_array(element));
                }
                Some(args) => {
                    for argument in &args.params {
                        let _ = self.lower_annotation(scope, argument);
                    }
                    self.emit_diagnostic(Diagnostic::generic_type_requires_arguments(
                        ref_span, "Array<T>", 1,
                    ));
                    return Some(self.interner.well_known().error);
                }
                // `Array` IS a recognized built-in, so a bare `Array` or a wrong type-argument
                // count is a type-argument-count error (tsc TS2314, deferred) — NOT "cannot find
                // name". Degrade to the error type silently (matching M17), rather than falling
                // through to the M22 unresolved-name arm below.
                None => {
                    self.emit_diagnostic(Diagnostic::generic_type_requires_arguments(
                        ref_span, "Array<T>", 1,
                    ));
                    return Some(self.interner.well_known().error);
                }
            }
        }

        let decl_id = match type_decl_id(self.binder, scope, name) {
            Some(id) => id,
            None => {
                // Report `TK2304` only for truly undeclared names. Value-as-type,
                // applied type parameters, and qualified names are found/deferred cases,
                // not "cannot find name".
                let found_in_some_space = self.binder.resolve_type(scope, name).is_some()
                    || self.binder.resolve_value(scope, name).is_some()
                    || self.lookup_type_param(name).is_some();
                if !found_in_some_space {
                    let span = Span::from_oxc(ident.span);
                    self.emit_diagnostic(Diagnostic::cannot_find_name(span, name));
                }
                // Still lower any type arguments so an unresolved name INSIDE them is reported
                // too (tsc flags `Lost<AlsoGone>` on BOTH). Results are discarded — the whole
                // reference degrades to the error type (any-like, which suppresses cascade so
                // `const a: Foo = 5` is only TK2304, never also TK2322).
                if let Some(args) = type_arguments {
                    for arg in &args.params {
                        let _ = self.lower_annotation(scope, arg);
                    }
                }
                return Some(self.interner.well_known().error);
            }
        };

        let stable_endpoint = self
            .type_environment
            .resolution_environment()
            .groups()
            .get(decl_id)
            .is_some()
            || matches!(
                self.type_decls.get(decl_id.index()),
                Some(TypeDecl::Class { .. })
            );
        let resolved = self.resolve_type_group_reference(
            scope,
            decl_id,
            name,
            reference_span,
            type_arguments,
        )?;
        if stable_endpoint {
            Some(resolved)
        } else {
            self.maybe_evaluate(resolved, ref_span)
        }
    }

    fn resolve_qualified_type_reference(
        &mut self,
        scope: ScopeId,
        qualified: &oxc_ast::ast::TSQualifiedName<'_>,
        type_arguments: Option<&TSTypeParameterInstantiation<'_>>,
        reference_span: Span,
    ) -> Option<TypeId> {
        let qualified_span = Span::from_oxc(qualified.span);
        let mut segments = Vec::new();
        if !flatten_qualified_type_name(&qualified.left, &mut segments) {
            self.record_incomplete(
                "annotation-lower/type-name/qualified-name",
                qualified_span,
                "qualified type name A.B not resolved",
            );
            return None;
        }
        segments.push(QualifiedTypeSegment {
            name: qualified.right.name.as_str(),
            span: Span::from_oxc(qualified.right.span),
        });

        match self.classify_qualified_type_path(
            scope,
            &segments,
            qualified_span,
            reference_span,
            type_arguments,
        ) {
            QualifiedTypeDisposition::Ready(ty) => Some(ty),
            QualifiedTypeDisposition::RecoveredTopologyFailure => {
                Some(self.interner.well_known().error)
            }
            QualifiedTypeDisposition::IncompleteEndpoint
            | QualifiedTypeDisposition::UnavailableEndpoint => None,
        }
    }

    pub(super) fn classify_qualified_type_path(
        &mut self,
        scope: ScopeId,
        segments: &[QualifiedTypeSegment<'_>],
        qualified_span: Span,
        reference_span: Span,
        type_arguments: Option<&TSTypeParameterInstantiation<'_>>,
    ) -> QualifiedTypeDisposition {
        let names = segments
            .iter()
            .map(|segment| segment.name)
            .collect::<Vec<_>>();
        let spans = segments
            .iter()
            .map(|segment| segment.span)
            .collect::<Vec<_>>();
        let mut resolution = self.binder.resolve_qualified_type_path(scope, &names);
        if matches!(resolution, QualifiedTypePathResolution::MissingRoot { .. })
            && self.checker_local_qualified_root(names[0])
        {
            resolution = QualifiedTypePathResolution::TypeOnlyRoot { segment: 0 };
        }

        if let QualifiedTypePathResolution::TypeGroup(group) = resolution {
            let name = names.last().copied().unwrap_or_default();
            return match self.resolve_type_group_reference(
                scope,
                group,
                name,
                reference_span,
                type_arguments,
            ) {
                Some(ty) => QualifiedTypeDisposition::Ready(ty),
                None => QualifiedTypeDisposition::UnavailableEndpoint,
            };
        }

        let disposition = if matches!(resolution, QualifiedTypePathResolution::Unavailable { .. }) {
            QualifiedTypeDisposition::UnavailableEndpoint
        } else if let Some(incomplete) = qualified_type_incomplete(resolution) {
            self.record_incomplete(incomplete.id, qualified_span, incomplete.context);
            QualifiedTypeDisposition::IncompleteEndpoint
        } else {
            let diagnostic =
                qualified_type_topology_diagnostic(resolution, &names, &spans, qualified_span)
                    .expect("every non-incomplete qualified outcome is a topology diagnostic");
            self.emit_diagnostic(diagnostic);
            QualifiedTypeDisposition::RecoveredTopologyFailure
        };

        if let Some(arguments) = type_arguments {
            for argument in &arguments.params {
                let _ = self.lower_annotation(scope, argument);
            }
        }
        disposition
    }

    pub(super) fn resolve_type_group_reference(
        &mut self,
        scope: ScopeId,
        group: TypeGroupId,
        name: &str,
        span: Span,
        arguments: Option<&TSTypeParameterInstantiation<'_>>,
    ) -> Option<TypeId> {
        assert!(
            !self.type_environment.is_published()
                || self
                    .type_environment
                    .published()
                    .groups()
                    .get(group)
                    .is_some(),
            "published registry missing terminal group {:?} (published {}, bound {})",
            group,
            self.type_environment
                .resolution_environment()
                .groups()
                .len(),
            self.binder.type_groups.len(),
        );
        if self
            .type_environment
            .resolution_environment()
            .groups()
            .get(group)
            .is_some()
        {
            self.resolve_published_type_group_reference(scope, group, name, span, arguments)
        } else if matches!(
            self.type_decls.get(group.index()),
            Some(TypeDecl::Class { .. })
        ) {
            self.resolve_class_type_reference(scope, group, name, span, arguments)
        } else {
            let mut instantiate_omitted_defaults = false;
            if matches!(
                self.type_decls.get(group.index()),
                Some(TypeDecl::Interface { .. } | TypeDecl::Alias { .. })
            ) {
                let parameters = self.type_decl_params(group);
                let defaults = self.type_decl_defaults(group);
                let parameter_names = self.type_decl_parameter_names(group);
                if !self.type_group_arity_is_valid(
                    scope,
                    name,
                    name,
                    span,
                    arguments,
                    &parameters,
                    &defaults,
                    &parameter_names,
                ) {
                    return Some(self.interner.well_known().error);
                }
                instantiate_omitted_defaults = arguments.is_none() && !parameters.is_empty();
            }
            if let Some(arguments) = arguments {
                self.instantiate_type_reference(scope, group, arguments)
            } else if instantiate_omitted_defaults {
                self.instantiate_type_group_arguments(scope, group, Vec::new(), span)
            } else {
                Some(self.resolve_type_decl(scope, group))
            }
        }
    }

    pub(super) fn validate_type_group_application_without_resolution(
        &mut self,
        scope: ScopeId,
        group: TypeGroupId,
        name: &str,
        span: Span,
        arguments: Option<&TSTypeParameterInstantiation<'_>>,
    ) {
        let parameters = self.type_decl_params(group);
        let defaults = self.type_decl_defaults(group);
        let parameter_names = self.type_decl_parameter_names(group);
        if !self.type_group_arity_is_valid(
            scope,
            name,
            name,
            span,
            arguments,
            &parameters,
            &defaults,
            &parameter_names,
        ) {
            return;
        }
        let Some(arguments) = arguments else {
            return;
        };
        let mut lowered = Vec::with_capacity(arguments.params.len());
        let mut unavailable = false;
        for argument in &arguments.params {
            if let Some(ty) = self.lower_annotation(scope, argument) {
                lowered.push((ty, Span::from_oxc(argument.span())));
            } else {
                unavailable = true;
            }
        }
        if unavailable {
            return;
        }
        let substitutions = parameters
            .iter()
            .copied()
            .zip(lowered.iter().map(|(argument, _)| *argument))
            .collect();
        self.check_type_argument_constraints(&parameters, &lowered, &substitutions);
    }

    fn resolve_published_type_group_reference(
        &mut self,
        scope: ScopeId,
        group: TypeGroupId,
        name: &str,
        span: Span,
        arguments: Option<&TSTypeParameterInstantiation<'_>>,
    ) -> Option<TypeId> {
        let terminal = self
            .type_environment
            .resolution_environment()
            .groups()
            .get(group)?
            .clone();
        let PublishedTypeGroupTerminal::Ready(published) = terminal else {
            if let Some(arguments) = arguments {
                for argument in &arguments.params {
                    let _ = self.lower_annotation(scope, argument);
                }
            }
            return None;
        };
        if let PublishedTypeGroupSurface::Class(class_id) = published.surface {
            let defaults: Vec<bool> = published
                .parameter_defaults
                .iter()
                .map(|default| *default != PublishedTypeParameterDefault::Absent)
                .collect();
            return self.resolve_class_endpoint(
                scope,
                class_id,
                &published.parameters,
                &defaults,
                &published.parameter_names,
                name,
                span,
                arguments,
            );
        }

        if !self.type_group_arity_is_valid(
            scope,
            name,
            &published.name,
            span,
            arguments,
            &published.parameters,
            &published.parameter_defaults,
            &published.parameter_names,
        ) {
            return Some(self.interner.well_known().error);
        }
        match arguments {
            Some(arguments) => {
                let instantiated = self.instantiate_type_reference(scope, group, arguments)?;
                self.maybe_evaluate(instantiated, span)
            }
            None => match published.surface {
                PublishedTypeGroupSurface::Template(template)
                    if published.parameters.is_empty() =>
                {
                    self.maybe_evaluate(template, span)
                }
                PublishedTypeGroupSurface::Template(_) => {
                    self.instantiate_type_group_arguments(scope, group, Vec::new(), span)
                }
                PublishedTypeGroupSurface::Class(_) => unreachable!(),
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn type_group_arity_is_valid(
        &mut self,
        scope: ScopeId,
        reference_name: &str,
        declaration_name: &str,
        span: Span,
        arguments: Option<&TSTypeParameterInstantiation<'_>>,
        parameters: &[TypeParamId],
        defaults: &[PublishedTypeParameterDefault],
        parameter_names: &[String],
    ) -> bool {
        let actual = arguments.map_or(0, |arguments| arguments.params.len());
        let expected_max = parameters.len();
        let expected_min = defaults
            .iter()
            .rposition(|default| *default == PublishedTypeParameterDefault::Absent)
            .map_or(0, |index| index + 1);
        let diagnostic = if expected_max == 0 && actual > 0 {
            Some(Diagnostic::type_is_not_generic(span, reference_name))
        } else if actual < expected_min || actual > expected_max {
            let display = if parameter_names.is_empty() {
                declaration_name.to_string()
            } else {
                format!("{}<{}>", declaration_name, parameter_names.join(", "))
            };
            Some(if expected_min == expected_max {
                Diagnostic::generic_type_requires_arguments(span, &display, expected_max)
            } else {
                Diagnostic::generic_type_requires_argument_range(
                    span,
                    &display,
                    expected_min,
                    expected_max,
                )
            })
        } else {
            None
        };
        let Some(diagnostic) = diagnostic else {
            return true;
        };
        self.emit_diagnostic(diagnostic);
        if let Some(arguments) = arguments {
            for argument in &arguments.params {
                let _ = self.lower_annotation(scope, argument);
            }
        }
        false
    }

    fn checker_local_qualified_root(&self, name: &str) -> bool {
        name == "Array"
            || self.lookup_type_param(name).is_some()
            || self
                .cond_frames
                .iter()
                .rev()
                .filter(|frame| frame.active)
                .any(|frame| frame.binders.contains_key(name))
            || self
                .mapped_frames
                .iter()
                .rev()
                .any(|frame| frame.key_name == name)
    }

    pub(super) fn resolve_class_type_reference(
        &mut self,
        scope: ScopeId,
        decl_id: TypeGroupId,
        name: &str,
        span: Span,
        arguments: Option<&TSTypeParameterInstantiation<'_>>,
    ) -> Option<TypeId> {
        let (class_id, params, defaults, parameter_names) = {
            let Some(TypeDecl::Class {
                class_id,
                params,
                param_decl,
                ..
            }) = self.type_decls.get(decl_id.index())
            else {
                return None;
            };
            let defaults: Vec<bool> = (0..params.len())
                .map(|index| {
                    param_decl
                        .and_then(|declaration| declaration.params.get(index))
                        .and_then(|parameter| parameter.default.as_ref())
                        .is_some()
                })
                .collect();
            let parameter_names = param_decl
                .iter()
                .flat_map(|declaration| declaration.params.iter())
                .map(|parameter| parameter.name.name.to_string())
                .collect::<Vec<_>>();
            (*class_id, params.clone(), defaults, parameter_names)
        };
        self.resolve_class_endpoint(
            scope,
            class_id,
            &params,
            &defaults,
            &parameter_names,
            name,
            span,
            arguments,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_class_endpoint(
        &mut self,
        scope: ScopeId,
        class_id: crate::types::repr::ClassId,
        params: &[TypeParamId],
        defaults: &[bool],
        parameter_names: &[String],
        name: &str,
        span: Span,
        arguments: Option<&TSTypeParameterInstantiation<'_>>,
    ) -> Option<TypeId> {
        let expected_max = params.len();
        let expected_min = defaults
            .iter()
            .position(|has_default| *has_default)
            .unwrap_or(expected_max);

        let mut lowered = Vec::new();
        if let Some(arguments) = arguments {
            lowered.reserve(arguments.params.len());
            for argument in &arguments.params {
                lowered.push((
                    self.lower_annotation(scope, argument),
                    Span::from_oxc(argument.span()),
                ));
            }
        }
        let actual = lowered.len();
        if actual > expected_max || actual < expected_min {
            let display = if params.is_empty() {
                name.to_string()
            } else {
                let names = parameter_names.join(", ");
                format!("{name}<{names}>")
            };
            let diagnostic = if expected_max == 0 && actual > 0 {
                Diagnostic::type_is_not_generic(span, name)
            } else if expected_min == expected_max {
                Diagnostic::generic_type_requires_arguments(span, &display, expected_max)
            } else {
                Diagnostic::generic_type_requires_argument_range(
                    span,
                    &display,
                    expected_min,
                    expected_max,
                )
            };
            self.emit_diagnostic(diagnostic);
            return Some(self.interner.well_known().error);
        }
        let explicit: Vec<ExplicitClassArgument> = lowered
            .iter()
            .map(|(argument, _)| match argument {
                Some(argument) if *argument != self.interner.well_known().error => {
                    ExplicitClassArgument::Ready(*argument)
                }
                Some(_) | None => ExplicitClassArgument::Unavailable,
            })
            .collect();

        let Some(descriptors) = self.class_application_parameters.get(&class_id).cloned() else {
            if let Some(exhaustion) = self.missing_class_application_descriptor_exhaustion(class_id)
            {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), span);
                return None;
            }
            let default_owners = self.lexical_events.classes().iter().find(|reservation| {
                reservation
                    .binding
                    .as_ref()
                    .is_some_and(|binding| binding.class_id == class_id)
            });
            let parameters = params
                .iter()
                .copied()
                .zip(defaults.iter().copied())
                .enumerate()
                .map(|(index, (id, has_default))| ClassTypeParameter {
                    id,
                    default: if has_default {
                        let owner = default_owners
                            .and_then(|reservation| {
                                reservation
                                    .defaults
                                    .iter()
                                    .find(|default| default.parameter_index == index)
                            })
                            .map(|default| default.owner)
                            .expect("every source class default has a lexical owner");
                        ClassTypeParameterDefault::Unsupported(owner)
                    } else {
                        ClassTypeParameterDefault::Absent
                    },
                })
                .collect::<Vec<_>>();
            return match complete_class_arguments(ClassApplicationRequest {
                class: class_id,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&explicit),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            }) {
                DemandOutcome::Ready(arguments) => {
                    Some(self.interner.intern_class_instance(class_id, arguments))
                }
                DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                    ClassApplicationArguments::UnsupportedDefault { .. },
                )) => {
                    self.record_incomplete(
                        "annotation-lower/type-reference/class-default-argument",
                        span,
                        "class type-parameter default unavailable at application",
                    );
                    None
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), span);
                    None
                }
            };
        };
        let error = self.interner.well_known().error;
        let substitutions = descriptors
            .iter()
            .zip(&lowered)
            .filter_map(|(descriptor, (argument, _))| match argument {
                Some(argument) if *argument != error => {
                    Some((descriptor.application().id, *argument))
                }
                Some(_) | None => None,
            })
            .collect::<FxHashMap<_, _>>();
        let checks = descriptors
            .iter()
            .zip(&lowered)
            .filter_map(|(descriptor, (argument, span))| match argument {
                Some(argument) if *argument != error => {
                    Some((descriptor.constraint(), *argument, *span))
                }
                Some(_) | None => None,
            })
            .collect::<Vec<_>>();
        self.check_constraint_arguments(&checks, &substitutions);
        let parameters = descriptors
            .iter()
            .map(|descriptor| *descriptor.application())
            .collect::<Vec<_>>();
        let source_arguments = arguments.map_or(SourceClassArguments::Omitted, |_| {
            SourceClassArguments::Explicit(&explicit)
        });
        let classes = match &self.type_environment {
            super::super::type_groups::TypeEnvironmentState::Constructing { inherited, drafts } => {
                drafts
                    .as_ref()
                    .and_then(|drafts| drafts.staged_published_classes.clone())
                    .or_else(|| {
                        inherited
                            .as_ref()
                            .map(|environment| environment.classes().clone())
                    })
                    .expect("construction has one inherited or staged class registry")
            }
            super::super::type_groups::TypeEnvironmentState::Published(environment) => {
                environment.classes().clone()
            }
        };
        let outcome = build_class_application(
            &mut SurfaceTypeFactory::new(self.interner),
            &classes,
            ClassApplicationRequest {
                class: class_id,
                parameters: &parameters,
                source_arguments,
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );
        match outcome {
            DemandOutcome::Ready(application) => Some(application),
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::UnsupportedDefault { .. },
            )) => {
                self.record_incomplete(
                    "annotation-lower/type-reference/class-default-argument",
                    span,
                    "class type-parameter default unavailable at application",
                );
                None
            }
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), span);
                None
            }
        }
    }

    fn missing_class_application_descriptor_exhaustion(
        &self,
        class: crate::types::repr::ClassId,
    ) -> Option<Exhaustion> {
        let published = match &self.type_environment {
            super::super::type_groups::TypeEnvironmentState::Published(_) => true,
            super::super::type_groups::TypeEnvironmentState::Constructing {
                drafts: Some(drafts),
                ..
            } => drafts.staged_published_classes.is_some(),
            super::super::type_groups::TypeEnvironmentState::Constructing {
                drafts: None, ..
            } => true,
        };
        published.then_some(Exhaustion::ClassNotPublished {
            class,
            state: ClassConstructionState::Published,
        })
    }

    /// Instantiate a generic type reference by substituting lowered args into its template.
    /// Wrong arity is graceful and diagnostic-free in this slice: pairs are zipped and
    /// unmapped parameters survive. Generic interface instances remain substituted
    /// structural types, not nominal per-instantiation identities.
    pub(super) fn instantiate_type_reference(
        &mut self,
        scope: ScopeId,
        decl_id: TypeGroupId,
        args: &TSTypeParameterInstantiation<'_>,
    ) -> Option<TypeId> {
        // Lower the type arguments first (in the referencing scope, where any nested
        // type names / parameters live), keeping each one's span for a constraint
        // diagnostic. A non-lowerable argument aborts.
        let mut arg_infos: Vec<(TypeId, Span)> = Vec::with_capacity(args.params.len());
        let mut unavailable = false;
        for arg in &args.params {
            match self.lower_annotation(scope, arg) {
                Some(lowered) => arg_infos.push((lowered, Span::from_oxc(arg.span()))),
                None => unavailable = true,
            }
        }
        if unavailable {
            return None;
        }
        self.instantiate_type_group_arguments(scope, decl_id, arg_infos, Span::from_oxc(args.span))
    }

    fn instantiate_type_group_arguments(
        &mut self,
        scope: ScopeId,
        decl_id: TypeGroupId,
        mut arg_infos: Vec<(TypeId, Span)>,
        application_span: Span,
    ) -> Option<TypeId> {
        // The declaration's template (its body with parameter types embedded) and its
        // ordered parameter ids.
        let published = self
            .type_environment
            .resolution_environment()
            .groups()
            .get(decl_id)
            .cloned();
        let is_published = published.is_some();
        let (template, params, defaults) = match published {
            Some(PublishedTypeGroupTerminal::Ready(published)) => match published.surface {
                PublishedTypeGroupSurface::Template(template) => {
                    (template, published.parameters, published.parameter_defaults)
                }
                PublishedTypeGroupSurface::Class(_) => return None,
            },
            Some(PublishedTypeGroupTerminal::Unavailable(_)) => return None,
            None => {
                let params = self.type_decl_params(decl_id);
                (
                    self.resolve_type_decl(scope, decl_id),
                    params,
                    self.type_decl_defaults(decl_id),
                )
            }
        };

        // Build the substitution, zipping parameters to arguments up to the shorter
        // list (graceful on an arity mismatch — no panic, no spurious diagnostic).
        let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
        for (&param, &(arg, _)) in params.iter().zip(&arg_infos) {
            map.insert(param, arg);
        }

        // M24: each explicit type argument must satisfy its parameter's constraint
        // (`IBox<string>`, `TA<number>`). The bad argument still instantiates below.
        self.check_type_argument_constraints(&params, &arg_infos, &map);

        for (index, param) in params.iter().copied().enumerate().skip(arg_infos.len()) {
            let default = match defaults
                .get(index)
                .copied()
                .unwrap_or(PublishedTypeParameterDefault::Absent)
            {
                PublishedTypeParameterDefault::Ready(default) => {
                    substitute(self.interner, default, &map)
                }
                PublishedTypeParameterDefault::Unsupported => {
                    self.record_incomplete(
                        "annotation-lower/type-reference/default-argument",
                        application_span,
                        "type-parameter default unavailable at application",
                    );
                    return None;
                }
                PublishedTypeParameterDefault::Absent => break,
            };
            map.insert(param, default);
            arg_infos.push((default, application_span));
        }

        // Conditional, mapped, trusted intrinsic, and in-flight generic interface
        // templates instantiate lazily. A self-reference sees the interface's reserved
        // object before it is filled; eager substitution would collapse the recursive
        // edge to that empty template and lose its type arguments.
        let template_tag = self.interner.store().tag(template);
        let unfinished_interface = !is_published
            && matches!(
                self.type_decls.get(decl_id.index()),
                Some(TypeDecl::Interface { .. })
            )
            && !self.type_group_construction_is_frozen(decl_id);
        if unfinished_interface
            || template_tag == TypeTag::Conditional
            || template_tag == TypeTag::Mapped
            || self
                .interner
                .well_known()
                .is_string_intrinsic_marker(template)
            || template == self.interner.well_known().this_type
            || template == self.interner.well_known().omit_this_parameter
        {
            let args: Vec<(TypeParamId, TypeId)> = params
                .iter()
                .filter_map(|param| map.get(param).copied().map(|arg| (*param, arg)))
                .collect();
            return Some(self.interner.intern_instantiation(template, args));
        }

        Some(substitute(self.interner, template, &map))
    }

    fn type_decl_defaults(&self, decl_id: TypeGroupId) -> Vec<PublishedTypeParameterDefault> {
        if let Some(TypeDecl::Interface {
            recovery_defaults, ..
        }) = self.type_decls.get(decl_id.index())
        {
            return recovery_defaults.clone();
        }
        let (param_decl, lowered) = match self.type_decls.get(decl_id.index()) {
            Some(TypeDecl::Alias {
                param_decl,
                defaults,
                ..
            }) => (*param_decl, defaults.as_slice()),
            _ => return Vec::new(),
        };
        param_decl
            .iter()
            .flat_map(|declaration| declaration.params.iter())
            .enumerate()
            .map(
                |(index, parameter)| match (parameter.default.as_ref(), lowered.get(index)) {
                    (None, _) => PublishedTypeParameterDefault::Absent,
                    (Some(_), Some(Some(default))) => {
                        PublishedTypeParameterDefault::Ready(*default)
                    }
                    (Some(_), _) => PublishedTypeParameterDefault::Unsupported,
                },
            )
            .collect()
    }

    fn type_decl_parameter_names(&self, decl_id: TypeGroupId) -> Vec<String> {
        match self.type_decls.get(decl_id.index()) {
            Some(TypeDecl::Interface { recovery_names, .. }) => recovery_names.clone(),
            Some(TypeDecl::Alias { .. }) => Vec::new(),
            _ => Vec::new(),
        }
    }

    /// The ordered type-parameter ids of a type declaration (M9), or an empty list for
    /// a non-generic one / an unknown legacy type-storage id.
    fn type_decl_params(&self, decl_id: TypeGroupId) -> Vec<TypeParamId> {
        if let Some(PublishedTypeGroupTerminal::Ready(published)) = self
            .type_environment
            .resolution_environment()
            .groups()
            .get(decl_id)
        {
            return published.parameters.clone();
        }
        match self.type_decls.get(decl_id.index()) {
            Some(TypeDecl::Interface {
                recovery_params, ..
            }) => recovery_params.clone(),
            Some(TypeDecl::Alias { params, .. })
            // M16: a generic class carries its type-parameter ids just like an interface,
            // so `Box<number>` used as a type instantiates the class's instance template.
            | Some(TypeDecl::Class { params, .. })
            | Some(TypeDecl::UnsupportedClassInterface { params, .. })
            // M28: a prelude declaration resolved in the prelude pass keeps only its
            // ordered parameter ids — exactly what instantiation needs.
            | Some(TypeDecl::Resolved { params }) => params.clone(),
            Some(TypeDecl::Unavailable { .. }) | None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod qualified_name_tests {
    use crate::class_semantics::{ClassConstructionState, Exhaustion};
    use crate::diagnostics::DiagnosticCode;
    use crate::driver::{check_project, check_source, CheckOutput, FileInput};
    use crate::types::repr::ClassId;
    use crate::types::Interner;

    fn checked(source: &str) -> CheckOutput {
        let output = check_source(source);
        assert!(
            output.parse_errors.is_empty(),
            "unexpected parse errors: {:?}",
            output.parse_errors
        );
        output
    }

    fn span_text(source: &str, span: crate::span::Span) -> &str {
        let start = usize::try_from(span.start).expect("source span start fits usize");
        let end = usize::try_from(span.end).expect("source span end fits usize");
        &source[start..end]
    }

    #[test]
    fn published_class_missing_application_descriptors_recovers_without_draft_access() {
        let prelude_allocator = oxc_allocator::Allocator::default();
        let user_allocator = oxc_allocator::Allocator::default();
        let prelude =
            oxc_parser::Parser::new(&prelude_allocator, "", oxc_span::SourceType::ts()).parse();
        let user = oxc_parser::Parser::new(&user_allocator, "", oxc_span::SourceType::ts()).parse();
        let binder = crate::binder::bind_module_with_prelude(&prelude.program, &user.program);
        let mut interner = Interner::with_intrinsics();
        let resolved_len = binder.type_groups.len();
        let mut pass = super::super::super::build_pass(
            &mut interner,
            &binder,
            Vec::new(),
            vec![None; resolved_len],
            super::super::super::context::DeclTypes::new(binder.decl_count),
            0,
        );
        pass.type_environment = super::super::super::type_groups::TypeEnvironmentState::Published(
            super::super::super::type_groups::PublishedTypeEnvironment::empty(),
        );

        assert_eq!(
            pass.missing_class_application_descriptor_exhaustion(ClassId(90_300)),
            Some(Exhaustion::ClassNotPublished {
                class: ClassId(90_300),
                state: ClassConstructionState::Published,
            })
        );
        assert_eq!(
            pass.resolve_class_endpoint(
                binder.module,
                ClassId(90_300),
                &[],
                &[],
                &[],
                "MissingDescriptors",
                crate::span::Span::new(0, 0),
                None,
            ),
            None
        );
    }

    fn diagnostic_rows(
        output: &CheckOutput,
        source: &str,
    ) -> Vec<(DiagnosticCode, String, String)> {
        output
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code,
                    diagnostic.message.clone(),
                    span_text(source, diagnostic.span).to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn qualified_topology_diagnostics_match_tsc_messages_spans_and_order() {
        let source = "\
type AliasRoot = {};
let alias: AliasRoot.Member;
class ClassRoot {}
let classRef: ClassRoot.Member;
const ValueRoot = 1;
let valueRoot: ValueRoot.Member;
namespace Root {
  export const ValueMiddle = 1;
  export interface TypeMiddle {}
  export namespace NamespaceLeaf {}
  export namespace Child {}
}
let missing: Root.Missing.Leaf;
let valueMiddle: Root.ValueMiddle.Leaf;
let typeMiddle: Root.TypeMiddle.Leaf;
let childMissing: Root.Child.ParentLeaf;
let namespaceLeaf: Root.NamespaceLeaf;
let valueLeaf: Root.ValueMiddle;
";
        let output = checked(source);
        assert_eq!(
            diagnostic_rows(&output, source),
            vec![
                (
                    DiagnosticCode::TK2702,
                    "'AliasRoot' only refers to a type, but is being used as a namespace here."
                        .to_string(),
                    "AliasRoot".to_string(),
                ),
                (
                    DiagnosticCode::TK2702,
                    "'ClassRoot' only refers to a type, but is being used as a namespace here."
                        .to_string(),
                    "ClassRoot".to_string(),
                ),
                (
                    DiagnosticCode::TK2503,
                    "Cannot find namespace 'ValueRoot'.".to_string(),
                    "ValueRoot".to_string(),
                ),
                (
                    DiagnosticCode::TK2694,
                    "Namespace 'Root' has no exported member 'Missing'.".to_string(),
                    "Missing".to_string(),
                ),
                (
                    DiagnosticCode::TK2694,
                    "Namespace 'Root' has no exported member 'ValueMiddle'.".to_string(),
                    "ValueMiddle".to_string(),
                ),
                (
                    DiagnosticCode::TK2713,
                    "Cannot access 'TypeMiddle.Leaf' because 'TypeMiddle' is a type, but not a namespace. Did you mean to retrieve the type of the property 'Leaf' in 'TypeMiddle' with 'TypeMiddle[\"Leaf\"]'?".to_string(),
                    "Leaf".to_string(),
                ),
                (
                    DiagnosticCode::TK2694,
                    "Namespace 'Root.Child' has no exported member 'ParentLeaf'.".to_string(),
                    "ParentLeaf".to_string(),
                ),
                (
                    DiagnosticCode::TK2694,
                    "Namespace 'Root' has no exported member 'NamespaceLeaf'.".to_string(),
                    "NamespaceLeaf".to_string(),
                ),
                (
                    DiagnosticCode::TK2749,
                    "'Root.ValueMiddle' refers to a value, but is being used as a type here. Did you mean 'typeof Root.ValueMiddle'?".to_string(),
                    "Root.ValueMiddle".to_string(),
                ),
            ]
        );
        assert!(
            output
                .incomplete
                .iter()
                .all(|record| record.id != "annotation-lower/type-name/qualified-name"),
            "failed paths must not retain the generic qualified-name incomplete"
        );
    }

    #[test]
    fn qualified_failure_precedes_each_nested_type_argument_diagnostic() {
        let source = "namespace Root {}\nlet value: Root.Missing<First, Second>;";
        let output = checked(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.message.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (
                    DiagnosticCode::TK2694,
                    "Namespace 'Root' has no exported member 'Missing'.",
                ),
                (DiagnosticCode::TK2304, "Cannot find name 'First'"),
                (DiagnosticCode::TK2304, "Cannot find name 'Second'"),
            ]
        );
    }

    #[test]
    fn qualified_successful_endpoint_arity_diagnostics_own_the_full_reference_span() {
        let source = "\
namespace Root {
  export interface Plain {}
  export interface Generic<T> {}
  export class PlainClass {}
  export class RequiredClass<T> {}
}
type InterfaceNotGeneric = Root.Plain<number>;
type InterfaceWrongArity = Root.Generic<string, number>;
type ClassNotGeneric = Root.PlainClass<string>;
type ClassMissingArgument = Root.RequiredClass;
";
        let output = checked(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, span_text(source, diagnostic.span)))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticCode::TK2315, "Root.Plain<number>"),
                (DiagnosticCode::TK2314, "Root.Generic<string, number>"),
                (DiagnosticCode::TK2315, "Root.PlainClass<string>"),
                (DiagnosticCode::TK2314, "Root.RequiredClass"),
            ]
        );
    }

    #[test]
    fn successful_qualified_type_group_enforces_published_generic_arity() {
        let source =
            "namespace Root { export interface Member {} }\nlet value: Root.Member<Missing>;";
        let output = checked(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![DiagnosticCode::TK2315, DiagnosticCode::TK2304]
        );
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    }

    #[test]
    fn deferred_qualified_enum_endpoint_has_one_backlog_42_incomplete() {
        let source = "enum DeferredRoot { Item }\nlet value: DeferredRoot.Member;";
        let output = checked(source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let records = output
            .incomplete
            .iter()
            .filter(|record| record.id == "annotation-lower/type-name/qualified-enum")
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(
            records[0].context,
            "qualified enum type resolution deferred to backlog 42"
        );
        assert_eq!(span_text(source, records[0].span), "DeferredRoot.Member");
    }

    #[test]
    fn missing_local_ambient_export_aliases_diagnose_once_at_their_local_spans() {
        let source = "\
declare namespace AliasOwner {
  interface Local {}
  export {
    MissingUnused as UnusedAlias,
    MissingUsed as UsedAlias,
  };
}
let diagnosedAliasStaysOpaque: AliasOwner.UsedAlias;
";
        let output = checked(source);
        assert_eq!(
            diagnostic_rows(&output, source),
            vec![
                (
                    DiagnosticCode::TK2304,
                    "Cannot find name 'MissingUnused'".to_string(),
                    "MissingUnused".to_string(),
                ),
                (
                    DiagnosticCode::TK2304,
                    "Cannot find name 'MissingUsed'".to_string(),
                    "MissingUsed".to_string(),
                ),
            ],
            "each missing local alias has one declaration-owned diagnostic and no use-site diagnostic"
        );
        assert_eq!(
            output
                .incomplete
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            Vec::<&str>::new(),
            "the diagnosed alias endpoint must add no qualified use-site incomplete"
        );
    }

    #[test]
    fn ambient_alias_outputs_are_rejected_order_independently_and_namespace_aliases_resolve() {
        let source = "\
declare namespace AliasOutputForward {
  interface Local { forward: true }
  export { Local as A };
  export { A as B };
}
let diagnosedForwardAliasUse: AliasOutputForward.B;
declare namespace AliasOutputReverse {
  interface Local { reverse: true }
  export { A as B };
  export { Local as A };
}
let diagnosedReverseAliasUse: AliasOutputReverse.B;
declare namespace GenuineLocalControl {
  interface Local { aliasTarget: true }
  export { Local as A };
  export { A as B };
  interface A { genuineLocal: true }
}
let genuineLocalAliasUse: GenuineLocalControl.B;
declare namespace A {
  namespace N { export interface X {} }
  export { type N as TN };
}
type TypeOnlyNamespaceAlias = A.TN.X;
";
        let output = checked(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.code.as_str(),
                    diagnostic.message.as_str(),
                    span_text(source, diagnostic.span),
                ))
                .collect::<Vec<_>>(),
            [
                (
                    "TK2661",
                    "Cannot export 'A'. Only local declarations can be exported from a module",
                    "A",
                ),
                (
                    "TK2661",
                    "Cannot export 'A'. Only local declarations can be exported from a module",
                    "A",
                ),
            ],
            "alias-only locals diagnose once at their declarations in either source order"
        );

        let module_records = output
            .incomplete
            .iter()
            .filter(|record| record.id == "decl/module-declaration/self")
            .count();
        assert_eq!(module_records, 0, "top-level ambient module records");

        let qualified_spans = output
            .incomplete
            .iter()
            .filter(|record| record.id == "annotation-lower/type-name/qualified-name")
            .map(|record| span_text(source, record.span))
            .collect::<Vec<_>>();
        assert_eq!(
            qualified_spans,
            Vec::<&str>::new(),
            "published alias and namespace endpoints add no use-site incomplete"
        );
        assert_eq!(
            output.incomplete.len(),
            module_records + qualified_spans.len(),
            "the aliases must not add any other recovery record"
        );
    }

    #[test]
    fn project_alias_diagnostics_keep_original_module_owners_after_dependency_ordering() {
        let reports = check_project(vec![
            FileInput {
                name: "a.ts".to_string(),
                source: "import { marker } from './z'; declare namespace A { export { MissingA as PublicA }; } let a: A.PublicA; marker;".to_string(),
            },
            FileInput {
                name: "z.ts".to_string(),
                source: "export const marker = 1; declare namespace Z { export { MissingZ as PublicZ }; } let z: Z.PublicZ;".to_string(),
            },
        ]);
        assert_eq!(reports.len(), 2);
        for (report, missing) in reports.iter().zip(["MissingA", "MissingZ"]) {
            assert!(
                report.output.parse_errors.is_empty(),
                "{}: {:?}",
                report.name,
                report.output.parse_errors
            );
            assert_eq!(
                diagnostic_rows(&report.output, &report.source),
                vec![(
                    DiagnosticCode::TK2304,
                    format!("Cannot find name '{missing}'"),
                    missing.to_string(),
                )],
                "{} must retain exactly its declaration-owned alias diagnostic",
                report.name
            );
        }
    }

    #[test]
    fn project_non_local_alias_diagnostics_keep_source_owners_after_dependency_ordering() {
        let reports = check_project(vec![
            FileInput {
                name: "a.ts".to_string(),
                source: "import { marker } from './z'; declare namespace A { interface Local {} export { Local as AliasA }; export { AliasA as PublicA }; } let a: A.PublicA; marker;".to_string(),
            },
            FileInput {
                name: "z.ts".to_string(),
                source: "export const marker = 1; declare namespace Z { interface Local {} export { AliasZ as PublicZ }; export { Local as AliasZ }; } let z: Z.PublicZ;".to_string(),
            },
        ]);
        assert_eq!(reports.len(), 2);
        for (report, local_name) in reports.iter().zip(["AliasA", "AliasZ"]) {
            assert!(
                report.output.parse_errors.is_empty(),
                "{}: {:?}",
                report.name,
                report.output.parse_errors
            );
            assert_eq!(
                report
                    .output
                    .diagnostics
                    .iter()
                    .map(|diagnostic| (
                        diagnostic.code.as_str(),
                        diagnostic.message.as_str(),
                        span_text(&report.source, diagnostic.span),
                    ))
                    .collect::<Vec<_>>(),
                [(
                    "TK2661",
                    if local_name == "AliasA" {
                        "Cannot export 'AliasA'. Only local declarations can be exported from a module"
                    } else {
                        "Cannot export 'AliasZ'. Only local declarations can be exported from a module"
                    },
                    local_name,
                )],
                "{} must retain exactly its declaration-owned non-local alias diagnostic",
                report.name
            );
            assert!(
                report
                    .output
                    .incomplete
                    .iter()
                    .all(|record| { record.id != "annotation-lower/type-name/qualified-name" }),
                "the diagnosed endpoint must add no qualified use-site incomplete"
            );
        }
    }

    #[test]
    fn qualified_declaration_and_variable_errors_replay_at_source_owners() {
        let source = "\
namespace Root {}
interface Derived { base: Root.Base }
let value: Root.Member;
";
        let output = checked(source);
        assert_eq!(
            diagnostic_rows(&output, source)
                .into_iter()
                .map(|(code, _, span)| (code, span))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticCode::TK2694, "Base".to_string()),
                (DiagnosticCode::TK2694, "Member".to_string()),
            ]
        );
    }

    #[test]
    fn simple_type_name_resolution_and_nested_argument_order_are_unchanged() {
        let source = "let value: Missing<AlsoMissing>;";
        let output = checked(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.message.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticCode::TK2304, "Cannot find name 'Missing'"),
                (DiagnosticCode::TK2304, "Cannot find name 'AlsoMissing'"),
            ]
        );
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    }

    #[test]
    fn checker_local_roots_and_namespace_precedence_match_tsc() {
        let source = "\
declare function functionRoot<FunctionT>(): FunctionT.Member;
interface MethodRoot { method<MethodT>(): MethodT.Member }
type InferRoot<S> = S extends infer InferT ? InferT.Member : never;
type MappedRoot<S> = { [MappedK in keyof S]: MappedK.Member };
type BuiltinRoot = Array.Member;
abstract class ClassRoot<ClassT> { abstract field: ClassT.Member }
class StaticRoot<StaticT> { static field: StaticT.Member }
namespace T { export interface Member {} }
declare function namespaceFirst<T>(): T.Member;
namespace U { export interface Member {} }
type InferNamespace<S> = S extends infer U ? U.Member : never;
namespace K { export interface Member {} }
type MappedNamespace<S> = { [K in keyof S]: K.Member };
";
        let output = checked(source);
        assert_eq!(
            diagnostic_rows(&output, source)
                .into_iter()
                .map(|(code, _, span)| (code, span))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticCode::TK2702, "FunctionT".to_string()),
                (DiagnosticCode::TK2702, "MethodT".to_string()),
                (DiagnosticCode::TK2702, "InferT".to_string()),
                (DiagnosticCode::TK2702, "MappedK".to_string()),
                (DiagnosticCode::TK2702, "Array".to_string()),
                (DiagnosticCode::TK2702, "ClassT".to_string()),
                (DiagnosticCode::TK2503, "StaticT".to_string()),
            ]
        );
        assert_eq!(
            output
                .incomplete
                .iter()
                .filter(|record| record.id == "annotation-lower/type-name/qualified-name")
                .count(),
            0,
            "namespace slots must win over local T/U/K binders"
        );
    }

    #[test]
    fn qualified_topology_recovery_visits_every_independent_context() {
        let source = "\
namespace N {}
type U = N.UnionLeft | N.UnionRight;
type I = N.IntersectionLeft & N.IntersectionRight;
type T = [N.TupleFirst, N.TupleSecond];
type X = N.IndexedObject[N.IndexedKey];
type C = N.Check extends N.Extends ? N.TrueBranch : N.FalseBranch;
type M = { [K in keyof N.MappedKeys]: N.MappedValue };
type F = (value: N.FunctionParameter) => N.FunctionReturn;
type R = new (value: N.ConstructorParameter) => N.ConstructorReturn;
type L = `${N.TemplateFirst}${N.TemplateSecond}`;
type Call = { (value: N.CallParameter): N.CallReturn };
type Construct = { new (value: N.ConstructParameter): N.ConstructReturn };
type Method = { method<P extends N.SignatureConstraint>(value: N.SignatureParameter): N.SignatureReturn };
abstract class K<C extends N.ClassConstraint> {
  abstract field: N.ClassField;
  abstract method<P extends N.MethodConstraint>(value: N.MethodParameter): N.MethodReturn;
}
";
        let output = checked(source);
        let expected = [
            "UnionLeft",
            "UnionRight",
            "IntersectionLeft",
            "IntersectionRight",
            "TupleFirst",
            "TupleSecond",
            "IndexedObject",
            "IndexedKey",
            "Check",
            "Extends",
            "TrueBranch",
            "FalseBranch",
            "MappedKeys",
            "MappedValue",
            "FunctionParameter",
            "FunctionReturn",
            "ConstructorParameter",
            "ConstructorReturn",
            "TemplateFirst",
            "TemplateSecond",
            "CallParameter",
            "CallReturn",
            "ConstructParameter",
            "ConstructReturn",
            "SignatureConstraint",
            "SignatureParameter",
            "SignatureReturn",
            "ClassConstraint",
            "ClassField",
            "MethodConstraint",
            "MethodParameter",
            "MethodReturn",
        ];
        assert_eq!(output.diagnostics.len(), expected.len());
        for (diagnostic, expected_span) in output.diagnostics.iter().zip(expected) {
            assert_eq!(diagnostic.code, DiagnosticCode::TK2694);
            assert_eq!(span_text(source, diagnostic.span), expected_span);
            assert_eq!(
                diagnostic.message,
                format!("Namespace 'N' has no exported member '{expected_span}'.")
            );
        }
    }

    #[test]
    fn qualified_topology_error_type_suppresses_assignment_cascade() {
        let source = "namespace N {}\nconst value: N.Missing = 1;";
        let output = checked(source);
        assert_eq!(output.diagnostics.len(), 1, "{:?}", output.diagnostics);
        assert_eq!(output.diagnostics[0].code, DiagnosticCode::TK2694);
        assert_eq!(span_text(source, output.diagnostics[0].span), "Missing");
    }
}
