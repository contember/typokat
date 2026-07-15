use super::*;
use crate::binder::scope::ScopeId;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{GenericTypeParam, TypeParamId};
use crate::types::store::TypeId;
use crate::types::substitute;
use oxc_ast::ast::TSTypeParameterDeclaration;
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};

pub(in crate::check::checker) struct LoweredSignatureTypeParams {
    pub(in crate::check::checker) params: Vec<GenericTypeParam>,
    pub(in crate::check::checker) unavailable: bool,
}

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Build a source-name to type-parameter-id frame from pre-allocated ids.
    /// Unnameable parameters are skipped; named ones resolve to stable ids in bodies.
    pub(in crate::check::checker) fn build_type_param_frame(
        &mut self,
        param_decl: Option<&TSTypeParameterDeclaration<'_>>,
        ids: &[TypeParamId],
    ) -> FxHashMap<String, TypeId> {
        let mut frame = FxHashMap::default();
        let Some(param_decl) = param_decl else {
            return frame;
        };
        for (param, &id) in param_decl.params.iter().zip(ids) {
            let name = param.name.name.as_str();
            let interned = self.interner.intern_type_param(id, name);
            frame.insert(name.to_string(), interned);
        }
        frame
    }

    /// Lower type-parameter `extends` constraints into the store column.
    /// Must run with the parameter frame active so earlier parameters resolve. Real
    /// lowered constraints are recorded; circular bare/union/intersection chains
    /// report `TK2313` and are cleared before relation queries.
    pub(in crate::check::checker) fn lower_type_param_constraints(
        &mut self,
        scope: ScopeId,
        param_decl: Option<&TSTypeParameterDeclaration<'_>>,
        ids: &[TypeParamId],
    ) {
        self.lower_type_param_constraints_inner(scope, param_decl, ids, true);
    }

    /// Lower type-group constraints and raw defaults while the declaration
    /// graph is private. Default constraint relations are captured as obligations
    /// and run only after both semantic registries publish.
    pub(super) fn lower_type_group_parameter_descriptors(
        &mut self,
        scope: ScopeId,
        param_decl: Option<&TSTypeParameterDeclaration<'_>>,
        ids: &[TypeParamId],
        validate_locally: bool,
    ) -> TypeGroupParameterDescriptors {
        self.lower_type_group_parameter_descriptors_inner(
            scope,
            param_decl,
            ids,
            true,
            validate_locally,
        )
    }

    pub(super) fn lower_interface_fragment_parameter_descriptors(
        &mut self,
        scope: ScopeId,
        param_decl: Option<&TSTypeParameterDeclaration<'_>>,
        ids: &[TypeParamId],
    ) -> TypeGroupParameterDescriptors {
        self.lower_type_group_parameter_descriptors_inner(scope, param_decl, ids, false, false)
    }

    fn lower_type_group_parameter_descriptors_inner(
        &mut self,
        scope: ScopeId,
        param_decl: Option<&TSTypeParameterDeclaration<'_>>,
        ids: &[TypeParamId],
        publish_constraints: bool,
        validate_locally: bool,
    ) -> TypeGroupParameterDescriptors {
        let Some(param_decl) = param_decl else {
            return TypeGroupParameterDescriptors {
                constraints: Vec::new(),
                defaults: Vec::new(),
            };
        };
        let error = self.interner.well_known().error;
        let mut constraints = param_decl
            .params
            .iter()
            .map(|parameter| {
                let Some(constraint) = parameter.constraint.as_ref() else {
                    return TypeParameterMetadataState::Absent;
                };
                match self.lower_annotation(scope, constraint) {
                    Some(lowered) if lowered == error => TypeParameterMetadataState::Poisoned,
                    Some(lowered) => TypeParameterMetadataState::Ready(lowered),
                    None => TypeParameterMetadataState::Unsupported,
                }
            })
            .collect::<Vec<_>>();
        let constraint_overlay = ids
            .iter()
            .copied()
            .zip(constraints.iter().copied())
            .filter_map(|(id, constraint)| constraint.ready().map(|constraint| (id, constraint)))
            .collect::<FxHashMap<_, _>>();
        for (index, ((parameter, &id), constraint)) in param_decl
            .params
            .iter()
            .zip(ids)
            .zip(constraints.clone())
            .enumerate()
        {
            let TypeParameterMetadataState::Ready(constraint) = constraint else {
                continue;
            };
            if validate_locally
                && self.constraint_chain_revisits_with_overlay(id, &constraint_overlay)
            {
                self.emit_diagnostic(Diagnostic::circular_constraint(
                    Span::from_oxc(
                        parameter
                            .constraint
                            .as_ref()
                            .expect("lowered constraint has syntax")
                            .span(),
                    ),
                    parameter.name.name.as_str(),
                ));
                constraints[index] = TypeParameterMetadataState::Poisoned;
            } else if publish_constraints {
                self.interner.set_type_param_constraint(id, constraint);
            }
        }
        let mut defaults = Vec::with_capacity(ids.len());
        let mut checks = Vec::new();
        let mut optional_seen = false;
        for (index, parameter) in param_decl.params.iter().enumerate() {
            if parameter.default.is_none() && optional_seen {
                self.emit_diagnostic(Diagnostic::required_type_parameter_after_optional(
                    Span::from_oxc(parameter.name.span),
                ));
            }
            optional_seen |= parameter.default.is_some();
            let default = match parameter.default.as_ref() {
                None => TypeParameterMetadataState::Absent,
                Some(default) => match self.lower_annotation(scope, default) {
                    None => TypeParameterMetadataState::Unsupported,
                    Some(lowered) if lowered == error => TypeParameterMetadataState::Poisoned,
                    Some(lowered)
                        if self.default_references_nonpreceding_binder(index, ids, lowered) =>
                    {
                        self.emit_diagnostic(Diagnostic::type_parameter_default_forward_reference(
                            Span::from_oxc(default.span()),
                        ));
                        TypeParameterMetadataState::Poisoned
                    }
                    Some(lowered) => {
                        if validate_locally {
                            checks.push((
                                constraints
                                    .get(index)
                                    .copied()
                                    .and_then(|state| state.ready()),
                                lowered,
                                Span::from_oxc(default.span()),
                            ));
                        }
                        TypeParameterMetadataState::Ready(lowered)
                    }
                },
            };
            defaults.push(default);
        }
        self.check_constraint_arguments(&checks, &FxHashMap::default());
        TypeGroupParameterDescriptors {
            constraints,
            defaults,
        }
    }

    /// Lower the persistent binder descriptors of a function-like signature.
    /// The caller must have pushed this signature's frame, nested inside any
    /// enclosing class/interface frame, before invoking this helper.
    pub(in crate::check::checker) fn lower_signature_type_params(
        &mut self,
        scope: ScopeId,
        param_decl: Option<&TSTypeParameterDeclaration<'_>>,
        ids: &[TypeParamId],
    ) -> LoweredSignatureTypeParams {
        let Some(param_decl) = param_decl else {
            return LoweredSignatureTypeParams {
                params: Vec::new(),
                unavailable: false,
            };
        };
        let error_ty = self.interner.well_known().error;
        let mut unavailable = false;
        let mut type_params: Vec<GenericTypeParam> = Vec::with_capacity(ids.len());
        for (index, (param, &id)) in param_decl.params.iter().zip(ids).enumerate() {
            let constraint = match param.constraint.as_ref() {
                Some(constraint) => match self.lower_annotation(scope, constraint) {
                    Some(ty) if ty != error_ty => {
                        self.interner.set_type_param_constraint(id, ty);
                        Some(ty)
                    }
                    Some(_) | None => {
                        unavailable = true;
                        None
                    }
                },
                None => None,
            };
            let default = param.default.as_ref().and_then(|default| {
                let Some(lowered) = self.lower_annotation(scope, default) else {
                    unavailable = true;
                    return None;
                };
                if lowered == error_ty {
                    unavailable = true;
                    return None;
                }
                if self.default_references_nonpreceding_binder(index, ids, lowered) {
                    self.emit_diagnostic(Diagnostic::type_parameter_default_forward_reference(
                        Span::from_oxc(default.span()),
                    ));
                    unavailable = true;
                    return None;
                }
                Some(lowered)
            });
            type_params.push(GenericTypeParam {
                id,
                constraint,
                default,
            });
        }
        let mut circular = Vec::new();
        for (index, (param, &id)) in param_decl.params.iter().zip(ids).enumerate() {
            let Some(constraint) = param.constraint.as_ref() else {
                continue;
            };
            if self.constraint_chain_revisits(id) {
                circular.push((
                    index,
                    id,
                    Span::from_oxc(constraint.span()),
                    param.name.name.to_string(),
                ));
            }
        }
        for (index, id, span, name) in circular {
            self.emit_diagnostic(Diagnostic::circular_constraint(span, &name));
            self.interner.remove_type_param_constraint(id);
            if let Some(type_param) = type_params.get_mut(index) {
                type_param.constraint = None;
            }
            unavailable = true;
        }
        self.validate_signature_type_param_defaults(&type_params, param_decl);
        LoweredSignatureTypeParams {
            params: type_params,
            unavailable,
        }
    }

    /// Signature defaults see earlier binders only, while constraints retain their
    /// TypeScript-wide declaration visibility. Detect a later binder after lowering
    /// under the existing full frame, then discard that default so it cannot become
    /// a permissive call-site fallback.
    fn default_references_nonpreceding_binder(
        &mut self,
        index: usize,
        ids: &[TypeParamId],
        default: TypeId,
    ) -> bool {
        let mut replacements: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
        let marker = self.interner.well_known().error;
        for &nonpreceding in &ids[index..] {
            replacements.insert(nonpreceding, marker);
        }
        !replacements.is_empty() && substitute(self.interner, default, &replacements) != default
    }

    /// Validate function-like declaration defaults in source order. A default may
    /// reference earlier binders, so each descriptor's constraint is checked only
    /// after those prior defaults have been substituted into the working map.
    fn validate_signature_type_param_defaults(
        &mut self,
        type_params: &[GenericTypeParam],
        param_decl: &TSTypeParameterDeclaration<'_>,
    ) {
        let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
        let mut checks: Vec<(Option<TypeId>, TypeId, Span)> = Vec::new();
        for (type_param, param) in type_params.iter().zip(&param_decl.params) {
            let Some(default) = type_param.default else {
                continue;
            };
            let Some(default_ast) = param.default.as_ref() else {
                continue;
            };
            let default = substitute(self.interner, default, &map);
            map.insert(type_param.id, default);
            checks.push((
                type_param.constraint,
                default,
                Span::from_oxc(default_ast.span()),
            ));
        }
        self.check_constraint_arguments(&checks, &map);
    }

    fn lower_type_param_constraints_inner(
        &mut self,
        scope: ScopeId,
        param_decl: Option<&TSTypeParameterDeclaration<'_>>,
        ids: &[TypeParamId],
        report_defaults: bool,
    ) {
        let Some(param_decl) = param_decl else {
            return;
        };
        let error_ty = self.interner.well_known().error;
        // Pass 1: lower and record every constraint in the list.
        for (param, &id) in param_decl.params.iter().zip(ids) {
            // WU7-E F3c, record-only: type-parameter DEFAULTS are never lowered
            // (divergences.md `constraints/type-parameter-defaults`), so an unresolved
            // name inside one was a silent false-clean. Dedup by span keeps repeated
            // constraint passes (fill + resolve, per-call generic sites) at one record.
            if report_defaults {
                if let Some(default) = param.default.as_ref() {
                    self.record_incomplete(
                        "annotation-lower/type-parameter-default/self",
                        Span::from_oxc(default.span()),
                        "type-parameter default not lowered",
                    );
                }
            }
            let Some(constraint) = param.constraint.as_ref() else {
                continue;
            };
            if let Some(ty) = self.lower_annotation(scope, constraint) {
                if ty != error_ty {
                    self.interner.set_type_param_constraint(id, ty);
                }
            }
        }
        // Collect all circular parameters before clearing any mutual-cycle participant.
        let mut circular: Vec<(TypeParamId, Span, String)> = Vec::new();
        for (param, &id) in param_decl.params.iter().zip(ids) {
            let Some(constraint) = param.constraint.as_ref() else {
                continue;
            };
            if self.constraint_chain_revisits(id) {
                circular.push((
                    id,
                    Span::from_oxc(constraint.span()),
                    param.name.name.to_string(),
                ));
            }
        }
        for (id, span, name) in circular {
            self.emit_diagnostic(Diagnostic::circular_constraint(span, &name));
            self.interner.remove_type_param_constraint(id);
        }
    }

    /// Whether `start`'s constraint chain revisits itself (`TK2313`).
    /// The DFS follows bare type-parameter successors, including union members; other
    /// shapes end the branch because structural recursion is relation-cycle handled.
    /// Visited-set termination avoids flagging cycles that do not pass through `start`.
    fn constraint_chain_revisits(&self, start: TypeParamId) -> bool {
        self.constraint_chain_revisits_from_sources(start, &FxHashMap::default(), true)
    }

    pub(super) fn constraint_chain_revisits_with_overlay(
        &self,
        start: TypeParamId,
        overlay: &FxHashMap<TypeParamId, TypeId>,
    ) -> bool {
        self.constraint_chain_revisits_from_sources(start, overlay, false)
    }

    fn constraint_chain_revisits_from_sources(
        &self,
        start: TypeParamId,
        overlay: &FxHashMap<TypeParamId, TypeId>,
        include_store: bool,
    ) -> bool {
        let store = self.interner.store();
        let mut visited: FxHashSet<TypeParamId> = FxHashSet::default();
        let mut stack: Vec<TypeParamId> = vec![start];
        while let Some(param) = stack.pop() {
            let Some(constraint) = overlay.get(&param).copied().or_else(|| {
                if include_store {
                    store.type_param_constraint(param)
                } else {
                    None
                }
            }) else {
                continue;
            };
            // One-step bare-parameter successors: the constraint itself, or the
            // members of a union constraint (canonical unions are flat, so one level
            // of members is exhaustive). Non-parameter shapes end the branch.
            let direct = store.type_param(constraint).map(|p| p.id);
            let members = store
                .union_members(constraint)
                .into_iter()
                .flatten()
                .filter_map(|&member| store.type_param(member).map(|p| p.id));
            // M31: an intersection constraint (`<T extends T & { x: number }>`) branches
            // through its bare-`TypeParam` members too — the dual of the union branch, so
            // `T extends T & X` is a circular constraint (TK2313).
            let intersection = store
                .intersection_members(constraint)
                .into_iter()
                .flatten()
                .filter_map(|&member| store.type_param(member).map(|p| p.id));
            for next in direct.into_iter().chain(members).chain(intersection) {
                if next == start {
                    return true;
                }
                if visited.insert(next) {
                    stack.push(next);
                }
            }
        }
        false
    }

    /// Run `body` with `frame` pushed, then pop unconditionally.
    /// This keeps type parameters scoped to their declaration; empty frames keep call
    /// sites uniform.
    pub(in crate::check::checker) fn with_type_params<R>(
        &mut self,
        frame: FxHashMap<String, TypeId>,
        body: impl FnOnce(&mut Pass) -> R,
    ) -> R {
        self.type_param_scopes.push(frame);
        let result = body(self);
        self.type_param_scopes.pop();
        result
    }

    /// Hide the containing class's binders for one static member. An own static
    /// method frame still sits above this barrier and therefore resolves normally.
    pub(in crate::check::checker) fn with_static_class_type_param_barrier<R>(
        &mut self,
        class_type_params: &[TypeParamId],
        body: impl FnOnce(&mut Pass) -> R,
    ) -> R {
        self.static_class_type_param_barriers
            .push(class_type_params.iter().copied().collect());
        let result = body(self);
        self.static_class_type_param_barriers.pop();
        result
    }

    /// Whether the innermost matching binder is one hidden by a static-member
    /// barrier. This preserves ordinary name lookup while reporting `TK2302`
    /// rather than degrading the class binder into an unresolved name.
    pub(in crate::check::checker) fn static_class_type_param_reference(&self, name: &str) -> bool {
        let Some(ty) = self
            .type_param_scopes
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
        else {
            return false;
        };
        self.is_static_class_type_param(ty)
    }

    /// Look a type name up in the in-scope type-parameter frames, innermost first
    /// (M9). Returns the interned [`TypeTag::TypeParam`] id if the name is a type
    /// parameter currently in scope, so it shadows a same-named named type **inside**
    /// the generic. `None` falls through to the binder's type slot.
    pub(in crate::check::checker) fn lookup_type_param(&self, name: &str) -> Option<TypeId> {
        self.type_param_scopes
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
            .filter(|&ty| !self.is_static_class_type_param(ty))
    }

    fn is_static_class_type_param(&self, ty: TypeId) -> bool {
        let Some(id) = self.interner.store().type_param(ty).map(|param| param.id) else {
            return false;
        };
        self.static_class_type_param_barriers
            .iter()
            .rev()
            .any(|barrier| barrier.contains(&id))
    }
}
