//! decls module (extracted from checker/mod.rs).

use super::context::*;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::binder::Binder;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{ClassId, ObjectType, PropertyType, TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::{substitute, Interner};
use oxc_ast::ast::{
    Class, Declaration, Expression, Program, Statement, TSInterfaceDeclaration, TSSignature,
    TSInterfaceHeritage, TSTypeAliasDeclaration, TSTypeName, TSTypeParameterDeclaration,
    TSTypeParameterInstantiation,
};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Fill named type declarations so later annotation reads are plain id lookups.
    ///
    /// Interfaces fill before aliases because alias instantiation must substitute over
    /// an already-filled generic interface template; reverse dependencies stay lazy.
    pub(in crate::check::checker) fn fill_type_decls(&mut self, scope: ScopeId) {
        self.fill_type_decls_range(scope, 0, self.type_decls.len());
    }

    pub(in crate::check::checker) fn fill_type_decls_range(
        &mut self,
        scope: ScopeId,
        start: usize,
        end: usize,
    ) {
        // Template lowering keeps conditionals lazy until value-position demand.
        self.building_template = true;

        // Fill interfaces first so generic interface templates are ready to instantiate.
        for index in start..end {
            self.ensure_interface_filled(scope, index);
        }

        // Fill conditional-alias placeholders before ordinary aliases can instantiate them.
        for index in start..end {
            let (placeholder, params, param_decl, annotation, name, name_span) =
                match &self.type_decls[index] {
                    TypeDecl::Alias {
                        conditional_template: Some(placeholder),
                        params,
                        param_decl,
                        annotation,
                        name,
                        name_span,
                        ..
                    } => (
                        *placeholder,
                        params.clone(),
                        *param_decl,
                        *annotation,
                        name.clone(),
                        *name_span,
                    ),
                    _ => continue,
                };
            let decl_id = DeclId(index as u32);
            let frame = self.build_type_param_frame(param_decl, &params);
            self.resolving_conditional_alias = Some((decl_id, name_span, name));
            let lowered = self.with_type_params(frame, |pass| {
                pass.lower_type_param_constraints(scope, param_decl, &params);
                pass.lower_annotation(scope, annotation)
            });
            self.resolving_conditional_alias = None;

            let error_ty = self.interner.well_known().error;
            match lowered {
                Some(id) if self.interner.store().tag(id) == TypeTag::Conditional => {
                    // Copy the freshly-lowered conditional's body into the reserved
                    // template id, so self-recursive instantiations point at a filled node.
                    if let Some(cond) = self.interner.store().conditional_type(id).copied() {
                        self.interner.fill_conditional(placeholder, cond);
                    }
                }
                // Circular check (`TK2456`) or out-of-subset body → the alias is the error
                // type (silent downstream, m22 discipline).
                _ => {
                    if let Some(slot) = self.type_resolved.get_mut(index) {
                        *slot = Some(error_ty);
                    }
                }
            }
        }

        // Fill mapped-alias placeholders before ordinary aliases can instantiate them.
        for index in start..end {
            let (placeholder, params, param_decl, annotation, name, name_span) =
                match &self.type_decls[index] {
                    TypeDecl::Alias {
                        mapped_template: Some(placeholder),
                        params,
                        param_decl,
                        annotation,
                        name,
                        name_span,
                        ..
                    } => (
                        *placeholder,
                        params.clone(),
                        *param_decl,
                        *annotation,
                        name.clone(),
                        *name_span,
                    ),
                    _ => continue,
                };
            let decl_id = DeclId(index as u32);
            let frame = self.build_type_param_frame(param_decl, &params);
            let prev_resolving_alias = self.resolving_alias.take();
            self.resolving_alias = Some((decl_id, name_span, name.clone()));
            self.resolving_alias_stack
                .push((decl_id, name_span, name, self.alias_indirection_depth));
            let lowered = self.with_type_params(frame, |pass| {
                pass.lower_type_param_constraints(scope, param_decl, &params);
                pass.lower_annotation(scope, annotation)
            });
            self.resolving_alias_stack.pop();
            self.resolving_alias = prev_resolving_alias;

            let error_ty = self.interner.well_known().error;
            match lowered {
                Some(id) if self.interner.store().tag(id) == TypeTag::Mapped => {
                    // Copy the freshly-lowered mapped body into the reserved template
                    // id, so self-recursive instantiations point at a filled node.
                    if let Some(mapped) = self.interner.store().mapped_type(id).copied() {
                        self.interner.fill_mapped(placeholder, mapped);
                    }
                }
                // Circular key source (`TK2456`) or out-of-subset body → the alias is
                // the error type (silent downstream, M22 discipline; overwrites the
                // seeded reserved id).
                _ => {
                    if let Some(slot) = self.type_resolved.get_mut(index) {
                        *slot = Some(error_ty);
                    }
                }
            }
        }

        // Fill seeded object aliases so legal member recursion resolves to the reserved id.
        for index in start..end {
            self.ensure_object_alias_filled(scope, index);
        }

        // Touch remaining aliases to resolve the whole memoized DAG.
        for index in start..end {
            if matches!(self.type_decls[index], TypeDecl::Alias { .. }) {
                self.resolve_type_decl(scope, DeclId(index as u32));
            }
        }

        // Fill class types after named annotations are available; bodies are checked later.
        for index in start..end {
            if matches!(self.type_decls[index], TypeDecl::Class { .. }) {
                self.ensure_class_filled(scope, index);
            }
        }

        // Value-position annotations may evaluate conditionals after fill.
        self.building_template = false;
    }

    /// Fill one interface's reserved object with its composed type.
    /// Bases are filled first in any declaration order; `Filling` breaks out-of-scope
    /// `extends` cycles without a diagnostic.
    fn ensure_interface_filled(&mut self, scope: ScopeId, index: usize) {
        match self.template_fill.get(index).copied() {
            // Already filled, or filling (an `extends` cycle re-entered) — do not recurse.
            Some(ClassFillState::Done) | Some(ClassFillState::Filling) => return,
            Some(ClassFillState::Pending) => {}
            // Out of range: nothing to fill.
            None => return,
        }
        let TypeDecl::Interface {
            reserved,
            ref params,
            param_decl,
            members,
            extends,
        } = self.type_decls[index]
        else {
            // Not an interface (a Pending object-template alias belongs to
            // [`ensure_object_alias_filled`]) — leave the state untouched.
            return;
        };
        let params = params.clone();

        // Mark in-progress before touching a base, so a cyclic `extends` hits the
        // `Filling` guard above rather than looping.
        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Filling;
        }

        // Fill each base first — interface, class, or (through a transparent alias) a
        // seeded object-literal alias — outside this interface's type-param frame, so a
        // base's parameters never leak into (or capture) this interface's frame.
        for heritage in extends {
            self.ensure_heritage_base_filled(scope, heritage);
        }

        let frame = self.build_type_param_frame(param_decl, &params);
        let object = self.with_type_params(frame, |pass| {
            // M24: lower the parameters' `extends` constraints with the frame active.
            pass.lower_type_param_constraints(scope, param_decl, &params);
            let own = pass.lower_interface_members(scope, members);
            pass.compose_interface_heritage(scope, own, extends)
        });
        self.interner.fill_object(reserved, object);

        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Done;
        }
    }

    /// Fill one seeded object-literal alias's reserved object with lowered members.
    /// Runs on demand in `template_fill`; `resolving_alias` stays set so nested
    /// mapped self-references still report `TK2456`.
    fn ensure_object_alias_filled(&mut self, scope: ScopeId, index: usize) {
        match self.template_fill.get(index).copied() {
            Some(ClassFillState::Done) | Some(ClassFillState::Filling) => return,
            Some(ClassFillState::Pending) => {}
            None => return,
        }
        let (reserved, members, name, name_span) = match &self.type_decls[index] {
            TypeDecl::Alias {
                object_template: Some(reserved),
                annotation: oxc_ast::ast::TSType::TSTypeLiteral(lit),
                name,
                name_span,
                ..
            } => (*reserved, &lit.members, name.clone(), *name_span),
            // Not a seeded object alias (a Pending interface belongs to
            // [`ensure_interface_filled`]) — leave the state untouched.
            _ => return,
        };
        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Filling;
        }
        let decl_id = DeclId(index as u32);
        let prev_resolving_alias = self.resolving_alias.take();
        self.resolving_alias = Some((decl_id, name_span, name.clone()));
        self.resolving_alias_stack
            .push((decl_id, name_span, name, self.alias_indirection_depth));
        let object = self.lower_interface_members(scope, members);
        self.resolving_alias_stack.pop();
        self.resolving_alias = prev_resolving_alias;
        self.interner.fill_object(reserved, object);
        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Done;
        }
    }

    /// Force-fill a heritage base before composition reads its members.
    /// Interfaces recurse, classes use class fill, and aliases resolve then fill any
    /// reserved template they land on. Generic alias heritage resolves on demand at
    /// instantiation time.
    fn ensure_heritage_base_filled(&mut self, scope: ScopeId, heritage: &TSInterfaceHeritage<'_>) {
        let Expression::Identifier(ident) = &heritage.expression else {
            return;
        };
        let Some(decl_id) = type_decl_id(self.binder, scope, ident.name.as_str()) else {
            return;
        };
        match self.type_decls.get(decl_id.index()) {
            Some(TypeDecl::Interface { .. }) => self.ensure_interface_filled(scope, decl_id.index()),
            Some(TypeDecl::Class { .. }) => self.ensure_class_filled(scope, decl_id.index()),
            Some(TypeDecl::Alias { .. }) if heritage.type_arguments.is_none() => {
                let ty = self.resolve_type_decl(scope, decl_id);
                self.ensure_reserved_template_filled(scope, ty);
            }
            _ => {}
        }
    }

    /// Fill the reserved-template declaration owned by a resolved base `TypeId`, if any.
    /// Transparent alias chains then compose the filled target in any declaration order.
    fn ensure_reserved_template_filled(&mut self, scope: ScopeId, ty: TypeId) {
        let target = self.type_decls.iter().position(|decl| match decl {
            TypeDecl::Interface { reserved, .. } | TypeDecl::Class { reserved, .. } => {
                *reserved == ty
            }
            TypeDecl::Alias {
                object_template: Some(reserved),
                ..
            } => *reserved == ty,
            _ => false,
        });
        let Some(index) = target else {
            return;
        };
        match self.type_decls.get(index) {
            Some(TypeDecl::Interface { .. }) => self.ensure_interface_filled(scope, index),
            Some(TypeDecl::Class { .. }) => self.ensure_class_filled(scope, index),
            Some(TypeDecl::Alias { .. }) => self.ensure_object_alias_filled(scope, index),
            _ => {}
        }
    }

    /// Fold `extends` bases into `own`, then return the composed object.
    /// Bases merge left-to-right, own members override inherited ones, and non-object
    /// bases contribute nothing in this deferred slice.
    fn compose_interface_heritage(
        &mut self,
        scope: ScopeId,
        own: ObjectType,
        extends: &[TSInterfaceHeritage<'_>],
    ) -> ObjectType {
        if extends.is_empty() {
            return own;
        }
        let mut base = ObjectType::default();
        for heritage in extends {
            let Some(base_ty) = self.resolve_heritage_type(scope, heritage) else {
                continue;
            };
            let Some(base_obj) = self.interner.store().object_type(base_ty).cloned() else {
                continue;
            };
            base = merge_object_members(base, base_obj);
        }
        merge_object_members(base, own)
    }

    /// B28 — resolve a heritage clause's base to its `TypeId`: a bare interface/alias
    /// reference resolves through `type_resolved`; a generic base (`extends Base<T>`)
    /// instantiates its template with the lowered arguments. Non-identifier bases (out of
    /// subset) yield `None`.
    fn resolve_heritage_type(
        &mut self,
        scope: ScopeId,
        heritage: &TSInterfaceHeritage<'_>,
    ) -> Option<TypeId> {
        let Expression::Identifier(ident) = &heritage.expression else {
            return None;
        };
        let decl_id = type_decl_id(self.binder, scope, ident.name.as_str())?;
        match heritage.type_arguments.as_deref() {
            Some(args) => self.instantiate_type_reference(scope, decl_id, args),
            None => Some(self.resolve_type_decl(scope, decl_id)),
        }
    }

    /// Resolve one type declaration to a memoized `TypeId`.
    /// Surface alias cycles report `TK2456` at every alias in the cycle and become
    /// error types; legal recursion through members uses seeded reserved ids instead
    /// of re-entering resolution.
    fn resolve_type_decl(&mut self, scope: ScopeId, decl_id: DeclId) -> TypeId {
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
    fn instantiate_type_reference(
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

    /// Lower interface members to the reserved nominal object's `ObjectType`.
    /// Unsupported or unlowerable members are skipped; the interface keeps the
    /// expressible subset.
    fn lower_interface_members(
        &mut self,
        scope: ScopeId,
        members: &[TSSignature<'_>],
    ) -> ObjectType {
        let mut object = ObjectType::default();
        let overloaded_method_names = self.overloaded_method_names(members);
        let call_signatures_overloaded = self.call_signatures_overloaded(members);
        let construct_signatures_overloaded = self.construct_signatures_overloaded(members);
        for member in members {
            match member {
                TSSignature::TSPropertySignature(sig) => {
                    let Some(name) = sig.key.static_name() else {
                        continue;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        continue;
                    }
                    let ty = match sig.type_annotation.as_ref() {
                        Some(annotation) => {
                            let Some(ty) = self.lower_annotation(scope, &annotation.type_annotation)
                            else {
                                continue;
                            };
                            ty
                        }
                        // tsc treats annotationless interface properties as `any`.
                        None => self.interner.well_known().any,
                    };
                    // Optional properties are real members with `| undefined` baked in
                    // while interning is available. The read-only relation engine then
                    // uses existing union-target logic, and `keyof`/indexed access see
                    // the key.
                    let ty = if sig.optional {
                        let undefined = self.interner.well_known().undefined;
                        self.interner.union(vec![ty, undefined])
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
                    let _ = self.lower_index_signature(scope, sig, &mut object);
                }
                TSSignature::TSMethodSignature(sig) => {
                    let Some(name) = sig.key.static_name() else {
                        continue;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        continue;
                    }
                    if let Some(prop) = self.lower_method_signature_property(scope, sig) {
                        object.properties.push(prop);
                    }
                }
                TSSignature::TSCallSignatureDeclaration(sig) => {
                    if call_signatures_overloaded {
                        continue;
                    }
                    if let Some(signature) = self.lower_call_signature(scope, sig) {
                        object.call_signatures.push(signature);
                    }
                }
                TSSignature::TSConstructSignatureDeclaration(sig) => {
                    if construct_signatures_overloaded {
                        continue;
                    }
                    if let Some(signature) = self.lower_construct_signature(scope, sig) {
                        object.construct_signatures.push(signature);
                    }
                }
            }
        }
        object
    }
}

/// Merge `overlay` onto `base`, with overlay members/signatures winning conflicts.
/// Used for interface `extends` and own-member composition.
fn merge_object_members(base: ObjectType, overlay: ObjectType) -> ObjectType {
    let mut properties = base.properties;
    for prop in overlay.properties {
        match properties.iter_mut().find(|p| p.name == prop.name) {
            Some(existing) => *existing = prop,
            None => properties.push(prop),
        }
    }
    ObjectType {
        properties,
        string_index: overlay.string_index.or(base.string_index),
        number_index: overlay.number_index.or(base.number_index),
        call_signatures: if overlay.call_signatures.is_empty() {
            base.call_signatures
        } else {
            overlay.call_signatures
        },
        construct_signatures: if overlay.construct_signatures.is_empty() {
            base.construct_signatures
        } else {
            overlay.construct_signatures
        },
    }
}

/// Reserve top-level type declarations by type-space `DeclId`.
/// Interfaces get ids before bodies resolve, enabling self/sibling references.
/// Reserve runs per compilation unit; append order matches binder `DeclId` order so
/// prelude and user declarations stay index-aligned.
#[allow(clippy::too_many_arguments)] // Two counters + two appended tables — irreducible reserve state.
pub(in crate::check::checker) fn reserve_type_decls<'ast>(
    interner: &mut Interner,
    binder: &Binder,
    scope: ScopeId,
    program: &'ast Program<'ast>,
    next_type_param: &mut u32,
    next_class_id: &mut u32,
    decls: &mut Vec<TypeDecl<'ast>>,
    resolved: &mut [Option<TypeId>],
) {
    // Build by walking declarations in source order; the binder assigned type
    // `DeclId`s in that same order (`bind_type_declarations`), so pushing in order
    // keeps the decl table index-aligned with the `DeclId`s.
    for stmt in &program.body {
        match top_type_decl(stmt) {
            Some(TopTypeDecl::Interface(iface)) => {
                let reserved = interner.reserve_object();
                let decl_id = type_decl_id(binder, scope, iface.id.name.as_str());
                if let Some(decl_id) = decl_id {
                    if let Some(slot) = resolved.get_mut(decl_id.index()) {
                        *slot = Some(reserved);
                    }
                    // M9: allocate one id per declared type parameter (in source order).
                    let params =
                        alloc_type_param_ids(iface.type_parameters.as_deref(), next_type_param);
                    place_type_decl(
                        decls,
                        decl_id.index(),
                        TypeDecl::Interface {
                            reserved,
                            params,
                            param_decl: iface.type_parameters.as_deref(),
                            members: &iface.body.body,
                            extends: &iface.extends,
                        },
                    );
                }
            }
            Some(TopTypeDecl::Alias(alias)) => {
                let decl_id = type_decl_id(binder, scope, alias.id.name.as_str());
                let params =
                    alloc_type_param_ids(alias.type_parameters.as_deref(), next_type_param);
                // M25: a top-level conditional-type body reserves a conditional template
                // id and seeds `type_resolved`, so a self-recursive reference resolves to
                // it (as a lazy instantiation) rather than expanding at lowering. The
                // placeholder is filled in the fill step.
                let conditional_template =
                    if matches!(alias.type_annotation, oxc_ast::ast::TSType::TSConditionalType(_)) {
                        let reserved = interner.reserve_conditional();
                        // M28 round 3: name the reserved row so a deferred
                        // instantiation renders by alias NAME, not the raw body.
                        interner.set_template_name(reserved, alias.id.name.as_str());
                        if let Some(decl_id) = decl_id {
                            if let Some(slot) = resolved.get_mut(decl_id.index()) {
                                *slot = Some(reserved);
                            }
                        }
                        Some(reserved)
                    } else {
                        None
                    };
                // Top-level mapped aliases reserve a template id so self-recursive
                // references resolve as lazy instantiations, not error types.
                let mapped_template =
                    if matches!(alias.type_annotation, oxc_ast::ast::TSType::TSMappedType(_)) {
                        let reserved = interner.reserve_mapped();
                        // M28 round 3: named for rendering, like the conditional row.
                        interner.set_template_name(reserved, alias.id.name.as_str());
                        if let Some(decl_id) = decl_id {
                            if let Some(slot) = resolved.get_mut(decl_id.index()) {
                                *slot = Some(reserved);
                            }
                        }
                        Some(reserved)
                    } else {
                        None
                    };
                // Non-generic object-literal aliases reserve an object id so member
                // self-references are legal recursion. Generic object aliases remain
                // structural templates instantiated by substitution, so they are not seeded.
                let object_template = if alias.type_parameters.is_none()
                    && matches!(alias.type_annotation, oxc_ast::ast::TSType::TSTypeLiteral(_))
                {
                    let reserved = interner.reserve_object();
                    if let Some(decl_id) = decl_id {
                        if let Some(slot) = resolved.get_mut(decl_id.index()) {
                            *slot = Some(reserved);
                        }
                    }
                    Some(reserved)
                } else {
                    None
                };
                if let Some(decl_id) = decl_id {
                    place_type_decl(
                        decls,
                        decl_id.index(),
                        TypeDecl::Alias {
                            annotation: &alias.type_annotation,
                            params,
                            param_decl: alias.type_parameters.as_deref(),
                            resolving: false,
                            conditional_template,
                            mapped_template,
                            object_template,
                            name: alias.id.name.to_string(),
                            name_span: Span::from_oxc(alias.id.span),
                        },
                    );
                }
            }
            // Named classes reserve their instance type id before fill, enabling own
            // and sibling references. Binder/source order keeps `decls` aligned with
            // type `DeclId`s; anonymous classes have no type name to reserve.
            Some(TopTypeDecl::Class(class)) if class.id.is_some() => {
                let reserved = interner.reserve_object();
                // M13: a fresh stable `ClassId` for this declaration (source order),
                // stamped onto its members in `fill_class`.
                let class_id = ClassId(*next_class_id);
                *next_class_id += 1;
                if let Some(id) = &class.id {
                    let decl_id = type_decl_id(binder, scope, id.name.as_str());
                    if let Some(decl_id) = decl_id {
                        if let Some(slot) = resolved.get_mut(decl_id.index()) {
                            *slot = Some(reserved);
                        }
                        // M16: allocate one id per declared type parameter (in source order),
                        // paired with their names later when the class body is lowered with the
                        // parameter frame in scope (`fill_class`) — exactly like an interface.
                        let params =
                            alloc_type_param_ids(class.type_parameters.as_deref(), next_type_param);
                        place_type_decl(
                            decls,
                            decl_id.index(),
                            TypeDecl::Class {
                                reserved,
                                class_id,
                                params,
                                param_decl: class.type_parameters.as_deref(),
                                class,
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn place_type_decl<'ast>(decls: &mut Vec<TypeDecl<'ast>>, index: usize, decl: TypeDecl<'ast>) {
    while decls.len() < index {
        decls.push(TypeDecl::Resolved { params: Vec::new() });
    }
    if decls.len() == index {
        decls.push(decl);
    } else if let Some(slot) = decls.get_mut(index) {
        *slot = decl;
    }
}

enum TopTypeDecl<'ast> {
    Interface(&'ast TSInterfaceDeclaration<'ast>),
    Alias(&'ast TSTypeAliasDeclaration<'ast>),
    Class(&'ast Class<'ast>),
}

fn top_type_decl<'ast>(stmt: &'ast Statement<'ast>) -> Option<TopTypeDecl<'ast>> {
    match stmt {
        Statement::TSInterfaceDeclaration(iface) => Some(TopTypeDecl::Interface(iface)),
        Statement::TSTypeAliasDeclaration(alias) => Some(TopTypeDecl::Alias(alias)),
        Statement::ClassDeclaration(class) => Some(TopTypeDecl::Class(class)),
        Statement::ExportNamedDeclaration(export) => match &export.declaration {
            Some(Declaration::TSInterfaceDeclaration(iface)) => Some(TopTypeDecl::Interface(iface)),
            Some(Declaration::TSTypeAliasDeclaration(alias)) => Some(TopTypeDecl::Alias(alias)),
            Some(Declaration::ClassDeclaration(class)) => Some(TopTypeDecl::Class(class)),
            _ => None,
        },
        _ => None,
    }
}

/// Allocate fresh type-parameter ids in source order.
/// Names are paired later when lowering with a parameter frame in scope.
pub(in crate::check::checker) fn alloc_type_param_ids(
    decl: Option<&TSTypeParameterDeclaration<'_>>,
    next_type_param: &mut u32,
) -> Vec<TypeParamId> {
    let Some(decl) = decl else {
        return Vec::new();
    };
    decl.params
        .iter()
        .map(|_| {
            let id = TypeParamId(*next_type_param);
            *next_type_param += 1;
            id
        })
        .collect()
}

/// The type-space `DeclId` a name resolves to from `scope` (binder type slot), if
/// any. Walks the scope graph like value resolution, then reads the `ty` slot.
pub(in crate::check::checker) fn type_decl_id(
    binder: &Binder,
    scope: ScopeId,
    name: &str,
) -> Option<DeclId> {
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.ty)
}

/// The **value**-space `DeclId` a name resolves to from `scope` (binder value slot),
/// if any (M11 — the class constructor side). Mirrors [`type_decl_id`] for the value
/// space.
pub(in crate::check::checker) fn value_decl_id(
    binder: &Binder,
    scope: ScopeId,
    name: &str,
) -> Option<DeclId> {
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}
