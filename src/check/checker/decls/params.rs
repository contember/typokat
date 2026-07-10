use super::*;
use crate::binder::scope::ScopeId;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::TypeParamId;
use crate::types::store::TypeId;
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
            if let Some(default) = param.default.as_ref() {
                self.record_incomplete(
                    "annotation-lower/type-parameter-default/self",
                    Span::from_oxc(default.span()),
                    "type-parameter default not lowered",
                );
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

    /// Look a type name up in the in-scope type-parameter frames, innermost first
    /// (M9). Returns the interned [`TypeTag::TypeParam`] id if the name is a type
    /// parameter currently in scope, so it shadows a same-named named type **inside**
    /// the generic. `None` falls through to the binder's type slot.
    pub(in crate::check::checker) fn lookup_type_param(&self, name: &str) -> Option<TypeId> {
        self.type_param_scopes
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
    }
}
