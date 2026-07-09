use super::*;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::substitute;
use oxc_ast::ast::{TSTypeName, TSTypeParameterInstantiation};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Resolve one type declaration to a memoized `TypeId`.
    /// Surface alias cycles report `TK2456` at every alias in the cycle and become
    /// error types; legal recursion through members uses seeded reserved ids instead
    /// of re-entering resolution.
    pub(super) fn resolve_type_decl(&mut self, scope: ScopeId, decl_id: DeclId) -> TypeId {
        let error_ty = self.interner.well_known().error;

        // Already resolved (interface reserved id, a seeded object/conditional template,
        // or a previously-resolved alias).
        if let Some(existing) = self.type_resolved.get(decl_id.index()).copied().flatten() {
            return existing;
        }

        let index = decl_id.index();

        // Same-depth re-entry is a surface alias cycle: report `TK2456` for the cycle.
        // Deeper re-entry came through a type constructor and is legal recursion, so
        // silently error-type it; seeded object aliases handle member self-reference.
        if matches!(
            self.type_decls.get(index),
            Some(TypeDecl::Alias { resolving: true, .. })
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
        let (annotation, param_decl, params, name, name_span) = match self.type_decls.get(index) {
            Some(TypeDecl::Alias {
                annotation,
                param_decl,
                params,
                resolving: false,
                name,
                name_span,
                ..
            }) => (
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
                pass.lower_type_param_constraints(scope, param_decl, &params);
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
            target
        };
        if let Some(slot) = self.type_resolved.get_mut(index) {
            *slot = Some(final_ty);
        }
        final_ty
    }

    /// B29 — a surface cycle re-entered `decl_id`. Report `TK2456` at every alias from
    /// `decl_id`'s position on the resolving-alias stack up to the top (the whole cycle),
    /// deduped via `circular_aliases`, and return the error type. Cloning the slice keeps
    /// the diagnostic/set writes off the stack borrow.
    fn report_surface_cycle(&mut self, decl_id: DeclId) -> TypeId {
        let cycle: Vec<(usize, Span, String)> = self
            .resolving_alias_stack
            .iter()
            .position(|(id, ..)| *id == decl_id)
            .map(|pos| {
                self.resolving_alias_stack[pos..]
                    .iter()
                    .map(|(id, span, name, _)| (id.index(), *span, name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (idx, span, name) in cycle {
            if self.circular_aliases.insert(idx) {
                self.diagnostics
                    .push(Diagnostic::circular_type_alias(span, &name));
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
    ) -> Option<TypeId> {
        let TSTypeName::IdentifierReference(ident) = type_name else {
            return None;
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

        // M17: intercept built-in `Array<T>` without `lib.d.ts`; user shadowing and
        // wrong arity are deferred. Every `Array` path returns here so bad arity never
        // falls through to the unresolved-name arm and emits `TK2304`.
        if name == "Array" {
            match type_arguments {
                Some(args) if args.params.len() == 1 => {
                    let element = self.lower_annotation(scope, &args.params[0])?;
                    return Some(self.interner.intern_array(element));
                }
                // `Array` IS a recognized built-in, so a bare `Array` or a wrong type-argument
                // count is a type-argument-count error (tsc TS2314, deferred) — NOT "cannot find
                // name". Degrade to the error type silently (matching M17), rather than falling
                // through to the M22 unresolved-name arm below.
                _ => return Some(self.interner.well_known().error),
            }
        }

        let decl_id = match type_decl_id(self.binder, scope, name) {
            Some(id) => id,
            None => {
                // Report `TK2304` only for truly undeclared names. Value-as-type,
                // applied type parameters, and qualified names are found/deferred cases,
                // not "cannot find name".
                let found_in_some_space = self.binder.graph.resolve(scope, name).is_some()
                    || self.lookup_type_param(name).is_some();
                if !found_in_some_space {
                    let span = Span::from_oxc(ident.span);
                    self.diagnostics
                        .push(Diagnostic::cannot_find_name(span, name));
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

        // 2. With type arguments: instantiate the generic declaration by substitution
        //    (M25: a conditional template instantiates lazily), then evaluate at a
        //    value-position demand site.
        if let Some(args) = type_arguments {
            let instantiated = self.instantiate_type_reference(scope, decl_id, args)?;
            return Some(self.maybe_evaluate(instantiated, ref_span));
        }

        // 3. Bare named type (M5 behaviour). M25: a bare reference to a non-generic
        //    conditional alias resolves to its (concrete) conditional template — evaluated
        //    here at a value-position demand site.
        let resolved = self.resolve_type_decl(scope, decl_id);
        Some(self.maybe_evaluate(resolved, ref_span))
    }

    /// Instantiate a generic type reference by substituting lowered args into its template.
    /// Wrong arity is graceful and diagnostic-free in this slice: pairs are zipped and
    /// unmapped parameters survive. Generic interface instances remain substituted
    /// structural types, not nominal per-instantiation identities.
    pub(super) fn instantiate_type_reference(
        &mut self,
        scope: ScopeId,
        decl_id: DeclId,
        args: &TSTypeParameterInstantiation<'_>,
    ) -> Option<TypeId> {
        // Lower the type arguments first (in the referencing scope, where any nested
        // type names / parameters live), keeping each one's span for a constraint
        // diagnostic. A non-lowerable argument aborts.
        let mut arg_infos: Vec<(TypeId, Span)> = Vec::with_capacity(args.params.len());
        for arg in &args.params {
            arg_infos.push((self.lower_annotation(scope, arg)?, Span::from_oxc(arg.span())));
        }

        // The declaration's template (its body with parameter types embedded) and its
        // ordered parameter ids.
        let template = self.resolve_type_decl(scope, decl_id);
        let params = self.type_decl_params(decl_id);

        // Build the substitution, zipping parameters to arguments up to the shorter
        // list (graceful on an arity mismatch — no panic, no spurious diagnostic).
        let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
        for (&param, &(arg, _)) in params.iter().zip(&arg_infos) {
            map.insert(param, arg);
        }

        // M24: each explicit type argument must satisfy its parameter's constraint
        // (`IBox<string>`, `TA<number>`). The bad argument still instantiates below.
        self.check_type_argument_constraints(&params, &arg_infos, &map);

        // Conditional, mapped, and string-intrinsic templates instantiate lazily.
        // Eager substitution would loop on recursive conditionals/mapped aliases or
        // erase intrinsic identity; non-recursive mapped aliases remain equivalent via
        // evaluator-side expansion.
        let template_tag = self.interner.store().tag(template);
        if template_tag == TypeTag::Conditional
            || template_tag == TypeTag::Mapped
            || self
                .interner
                .well_known()
                .is_string_intrinsic_marker(template)
        {
            let args: Vec<(TypeParamId, TypeId)> = params
                .iter()
                .zip(&arg_infos)
                .map(|(&param, &(arg, _))| (param, arg))
                .collect();
            return Some(self.interner.intern_instantiation(template, args));
        }

        Some(substitute(self.interner, template, &map))
    }

    /// The ordered type-parameter ids of a type declaration (M9), or an empty list for
    /// a non-generic one / an unknown `DeclId`.
    fn type_decl_params(&self, decl_id: DeclId) -> Vec<TypeParamId> {
        match self.type_decls.get(decl_id.index()) {
            Some(TypeDecl::Interface { params, .. })
            | Some(TypeDecl::Alias { params, .. })
            // M16: a generic class carries its type-parameter ids just like an interface,
            // so `Box<number>` used as a type instantiates the class's instance template.
            | Some(TypeDecl::Class { params, .. })
            // M28: a prelude declaration resolved in the prelude pass keeps only its
            // ordered parameter ids — exactly what instantiation needs.
            | Some(TypeDecl::Resolved { params }) => params.clone(),
            None => Vec::new(),
        }
    }
}
