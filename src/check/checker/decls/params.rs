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

    /// Lower the persistent binder descriptors of a function-like signature.
    /// The caller must have pushed this signature's frame, nested inside any
    /// enclosing class/interface frame, before invoking this helper.
    pub(in crate::check::checker) fn lower_signature_type_params(
        &mut self,
        scope: ScopeId,
        param_decl: Option<&TSTypeParameterDeclaration<'_>>,
        ids: &[TypeParamId],
    ) -> Vec<GenericTypeParam> {
        self.lower_type_param_constraints_inner(scope, param_decl, ids, false);
        let Some(param_decl) = param_decl else {
            return Vec::new();
        };
        let mut type_params: Vec<GenericTypeParam> = Vec::with_capacity(ids.len());
        for (index, (param, &id)) in param_decl.params.iter().zip(ids).enumerate() {
            let default = param.default.as_ref().and_then(|default| {
                let lowered = self.lower_annotation(scope, default)?;
                if self.default_references_later_signature_binder(index, param, ids, lowered) {
                    self.diagnostics
                        .push(Diagnostic::type_parameter_default_forward_reference(
                            Span::from_oxc(default.span()),
                        ));
                    return None;
                }
                Some(lowered)
            });
            type_params.push(GenericTypeParam {
                id,
                constraint: self.interner.store().type_param_constraint(id),
                default,
            });
        }
        self.validate_signature_type_param_defaults(&type_params, param_decl);
        type_params
    }

    /// Signature defaults see earlier binders only, while constraints retain their
    /// TypeScript-wide declaration visibility. Detect a later binder after lowering
    /// under the existing full frame, then discard that default so it cannot become
    /// a permissive call-site fallback.
    fn default_references_later_signature_binder(
        &mut self,
        index: usize,
        param: &oxc_ast::ast::TSTypeParameter<'_>,
        ids: &[TypeParamId],
        default: TypeId,
    ) -> bool {
        let Some(current) = self.lookup_type_param(param.name.name.as_str()) else {
            return false;
        };
        let mut replacements: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
        for &later in &ids[index.saturating_add(1)..] {
            replacements.insert(later, current);
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
            self.diagnostics
                .push(Diagnostic::circular_constraint(span, &name));
            self.interner.remove_type_param_constraint(id);
        }
    }

    /// Whether `start`'s constraint chain revisits itself (`TK2313`).
    /// The DFS follows bare type-parameter successors, including union members; other
    /// shapes end the branch because structural recursion is relation-cycle handled.
    /// Visited-set termination avoids flagging cycles that do not pass through `start`.
    fn constraint_chain_revisits(&self, start: TypeParamId) -> bool {
        let store = self.interner.store();
        let mut visited: FxHashSet<TypeParamId> = FxHashSet::default();
        let mut stack: Vec<TypeParamId> = vec![start];
        while let Some(param) = stack.pop() {
            let Some(constraint) = store.type_param_constraint(param) else {
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
