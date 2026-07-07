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
    /// Phase 0b — **fill**. Fill every interface body in place, then force every alias
    /// to resolve. After this returns, `pass.type_resolved` is complete, so every
    /// `TSTypeReference` encountered in the obligation walk is a plain id lookup.
    ///
    /// **Interfaces are filled BEFORE aliases are resolved** (M9): a generic alias body
    /// can *instantiate* a generic interface (`type Wrap<T> = Box<T>`), and
    /// instantiation substitutes over the interface's **template** — which must already
    /// be filled, not the empty reserved object. An interface body that references an
    /// alias still resolves that alias lazily on demand (`resolve_type_decl` is
    /// memoized), so the reverse dependency is unaffected by the order. (M5 had the
    /// opposite order purely to pre-warm aliases; correctness never depended on it,
    /// because resolution is lazy in both directions.)
    pub(in crate::check::checker) fn fill_type_decls(&mut self, scope: ScopeId) {
        self.fill_type_decls_range(scope, 0, self.type_decls.len());
    }

    pub(in crate::check::checker) fn fill_type_decls_range(
        &mut self,
        scope: ScopeId,
        start: usize,
        end: usize,
    ) {
        // M25: everything lowered in phase 0 is a **template** (an alias / interface /
        // class body), so a concrete conditional must stay its interned node rather than
        // evaluate eagerly — evaluation is a value-position (phase 1) demand. Restored to
        // `false` before the check walk.
        self.building_template = true;

        // Fill each interface's reserved id with its lowered members. Members are
        // lowered with the full resolver available; a self/sibling reference resolves
        // to a reserved/resolved id (stored, never inlined); a referenced alias is
        // resolved lazily right here.
        //
        // M9: a **generic** interface is filled with its type parameters in scope, so a
        // member referencing `T` (`value: T`) carries the parameter type. The reserved
        // id then holds a structural **template** (`{ value: T }`); an instantiation
        // `Box<number>` substitutes it. A non-generic interface fills with an empty
        // frame and stays nominal (filled in place, never re-interned).
        for index in start..end {
            self.ensure_interface_filled(scope, index);
        }

        // M25: fill each **conditional-alias** template (its reserved conditional id)
        // BEFORE resolving ordinary aliases, so a plain alias instantiating a conditional
        // one (`type WU = Wrap<U>`) sees the filled template. The body is lowered with the
        // parameter frame active and `resolving_conditional_alias` set (so a check that
        // surface-references the alias is `TK2456`); a self-recursive reference in a
        // branch resolves to the reserved id as a lazy instantiation (never expands).
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

        // M28: fill each **mapped-alias** template (its reserved mapped id), mirroring
        // the conditional-template step above. The body is lowered with the parameter
        // frame active and the `resolving_alias` context set (so a key source that
        // surface-references the alias itself — `type M = { [K in keyof M]: number }` —
        // stays the M26 `TK2456`); a self-recursive reference in the VALUE template
        // resolves to the reserved id as a lazy instantiation (never expands, never the
        // error type).
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

        // B29: fill each **object-literal alias** template (its reserved object id) BEFORE
        // resolving ordinary aliases, mirroring the conditional-template step. The reserved
        // id is already seeded in `type_resolved`, so a self-reference through a member
        // (`type X = { a: X | null }`, mutual `Even`/`Odd`) resolves to the stable id — the
        // m5 named-recursive representation — never re-entering into the error type.
        // Idempotent per index — an interface whose heritage named the alias already
        // force-filled it ([`ensure_heritage_base_filled`]).
        for index in start..end {
            self.ensure_object_alias_filled(scope, index);
        }

        // Resolve every remaining alias (interfaces are now filled, so a generic alias
        // instantiating an interface substitutes over the filled template). Resolution
        // is on-demand and idempotent, so touching every alias resolves the whole DAG.
        for index in start..end {
            if matches!(self.type_decls[index], TypeDecl::Alias { .. }) {
                self.resolve_type_decl(scope, DeclId(index as u32));
            }
        }

        // M11/M12: fill each class's reserved **instance type** with its fields + methods
        // (M12: composed with its base's members), and register its constructor signature
        // under the class's *value* `DeclId`. Done after interfaces/aliases so a
        // field/method/constructor annotation referencing a named type resolves to a
        // filled id; a self/sibling class reference resolves to a reserved class id
        // (stored, never inlined — so a recursive field like `next: Node | null` lowers).
        //
        // M12: a derived class needs its base filled first. [`ensure_class_filled`] fills
        // the base on demand (in any declaration order) and is idempotent + cycle-guarded,
        // so iterating every class index fills the whole `extends` DAG exactly once.
        // Method/constructor **bodies** are checked later, in the statement walk
        // ([`check_class`]) where `this`/`super` are set and obligations are collected;
        // this step builds **types only**.
        for index in start..end {
            if matches!(self.type_decls[index], TypeDecl::Class { .. }) {
                self.ensure_class_filled(scope, index);
            }
        }

        // M25: template building is done — value-position annotations (phase 1) now
        // evaluate their conditionals.
        self.building_template = false;
    }

    /// B28 — fill one interface's reserved object with its **composed** type (own
    /// members plus everything inherited through `extends`), on demand and exactly once.
    /// Mirrors [`ensure_class_filled`]: the base interfaces are filled **first** (so a
    /// derived interface reads their fully-composed members regardless of declaration
    /// order), the `Filling` guard breaks an `extends` cycle (out-of-scope TS2310 — no
    /// diagnostic, terminates), and a non-interface / out-of-range index is a no-op.
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

    /// B29 — fill one **seeded object-literal alias**'s reserved object with its lowered
    /// members, on demand and exactly once (mirroring [`ensure_interface_filled`], same
    /// [`template_fill`](Pass::template_fill) array). Members are lowered with the
    /// `resolving_alias` context set, so a nested mapped key source that
    /// surface-references the alias still hits the M26 `TK2456` path. A non-seeded /
    /// out-of-range index is a no-op.
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

    /// B28 — force-fill a heritage clause's base so the compose step reads real members
    /// regardless of declaration/fill order. Dispatches on the named declaration's kind:
    /// an **interface** fills recursively; a **class** fills via the (idempotent,
    /// cycle-guarded) class machinery — tsc allows `interface extends Class`, composing
    /// the instance type; a bare **alias** resolves (memoized) and whatever reserved
    /// template it lands on — an interface, a class instance, or a B29 seeded
    /// object-literal alias — is filled via [`ensure_reserved_template_filled`]. A
    /// generic alias heritage (`Alias<Args>`) needs no pre-fill: instantiation resolves
    /// its template on demand at compose time.
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

    /// B28 — given a resolved base `TypeId`, fill the declaration whose reserved
    /// template it is (interface / class instance / seeded object-literal alias), if
    /// any. This is what makes a transparent alias chain (`type A = RealBase`) compose
    /// the *filled* target in any declaration order. A `TypeId` owned by no declaration
    /// (a structural type, the error type) needs no fill.
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

    /// B28 — fold every `extends` base's members into `own` and return the composed
    /// object. Bases are merged left-to-right (a later base overriding an earlier one on
    /// a name conflict, matching tsc's read order); `own` members override any inherited
    /// one by name. Index signatures and (subset) call/construct signatures inherit when
    /// `own` does not declare them. A base that does not resolve to an object type
    /// contributes nothing (no diagnostic — over/under-report guarded by scope).
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

    /// Resolve a single type declaration to its `TypeId`, memoizing the result in
    /// `pass.type_resolved`. Interfaces are already seeded (their reserved id); an
    /// object-literal alias is seeded with a reserved object id (B29); other aliases are
    /// lowered on first request.
    ///
    /// B29: a **surface cycle** — an alias whose own surface (its body, a union member,
    /// or a mutual partner's surface) re-enters it, reachable without descending through a
    /// member — is a circular alias. It reports `TK2456` at **every** alias in the cycle
    /// (via the resolving-alias stack) and error-types them (the M22 silent-downstream
    /// discipline then holds). Legal recursion **through a member** never reaches this
    /// path: an object-literal alias is seeded, so a member self-reference resolves to its
    /// reserved id (the m5 named-recursive representation) rather than re-entering.
    fn resolve_type_decl(&mut self, scope: ScopeId, decl_id: DeclId) -> TypeId {
        let error_ty = self.interner.well_known().error;

        // Already resolved (interface reserved id, a seeded object/conditional template,
        // or a previously-resolved alias).
        if let Some(existing) = self.type_resolved.get(decl_id.index()).copied().flatten() {
            return existing;
        }

        let index = decl_id.index();

        // A reference re-entered this alias while it is mid-resolution. If the re-entry is
        // at the alias's own surface depth it is a **surface cycle** (`type Y = Y | null`,
        // mutual pairs) — report `TK2456` for the whole cycle and error-type it. If it came
        // through a type constructor (a greater depth: `type Arr = Arr[]`, `type W = { a: W
        // } | null`) it is **legal recursion** — silently error-type (no `TK2456`; a seeded
        // object-literal alias resolves such reads correctly instead of re-entering).
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

    /// Resolve a `TSTypeReference` to a `TypeId` (M5, extended for M9).
    ///
    /// Resolution order over a plain identifier name:
    ///
    ///  1. **type parameter in scope** (M9): if the name is a type parameter of an
    ///     enclosing generic declaration ([`lookup_type_param`]), it resolves to that
    ///     parameter's interned [`TypeTag::TypeParam`] type. A type parameter never
    ///     takes type arguments (`T<…>` is nonsense), so this only fires without args.
    ///  2. **type arguments present** (`Box<number>`, M9): instantiate the referenced
    ///     generic declaration by substituting its parameters with the lowered
    ///     arguments ([`instantiate_type_reference`]).
    ///  3. **bare named type** (M5): the declaration's (reserved or lazily-resolved)
    ///     id, via the binder's type slot.
    ///
    /// An **unresolved simple-identifier** type name reports `TK2304` ("Cannot find
    /// name", M22) and degrades to the **error type** (any-like — which suppresses any
    /// cascade, so `const a: Foo = 5` is only `TK2304`, never also `TK2322`); the
    /// diagnostic fires only when the name resolves to *no* space, so a value used as a
    /// type (tsc `TS2749`) and a type parameter applied with arguments (tsc `TS2315`)
    /// stay silent (distinct, deferred). A qualified name (`A.B`, out of subset) still
    /// yields `None` (the caller aborts the enclosing lowering, matching object / union /
    /// function lowering).
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

        // M25: an in-scope `infer` binder shadows a named type and takes no arguments.
        // An own-binder reference (this node's extends/true) resolves normally; an OUTER
        // node's binder resolves as cross-binder — no TK2304, but it poisons the nodes in
        // between (backlog 26 stopgap). A name in no active frame (e.g. this node's own
        // binder referenced from its false branch) falls through → `TK2304`.
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

        // M17: the built-in `Array<T>`. With no `lib.d.ts`, `Array` is not a declared
        // type, so it is intercepted here: `Array<T>` (exactly one type argument) lowers
        // to the same array type as `T[]`. User-shadowing of `Array` is deferred (no
        // fixture declares a type named `Array`), so the built-in name always wins. A
        // wrong type-argument count (`Array`, `Array<A, B>`) degrades to the error type
        // silently: `Array` IS a recognized built-in, so a bad arity is a deferred
        // type-argument-count error (tsc TS2314), NOT "cannot find name" — so this branch
        // must return for EVERY `Array` path rather than fall through to the M22
        // unresolved-name arm below (which would wrongly emit TK2304).
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
                // The name resolves to no TYPE. Report TK2304 ONLY when it resolves to nothing
                // in any space (truly undeclared). A name found in the VALUE space (value used
                // as a type — tsc TS2749) or an in-scope TYPE PARAMETER used with arguments
                // (tsc TS2315) is FOUND — those are distinct, deferred diagnostics, not
                // "cannot find name" — so stay silent. (A qualified name `A.B` returned `None`
                // at the top of this fn, out of subset — also silent.)
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

    /// Instantiate a generic type reference `Name<Arg, …>` by substitution (M9).
    ///
    /// Lowers each type argument (in the *referencing* scope), then substitutes the
    /// referenced declaration's type parameters with them into its **template**
    /// (`type_resolved[decl_id]`, built with the parameter types embedded). For a
    /// generic interface the template is its structural body, so `Box<number>`
    /// instantiates to `{ value: number }` — structural assignability then applies (see
    /// the FLAG below). Equal instantiations share one interned `TypeId`
    /// (`Box<number>` is consistent; `Box<number>` ≠ `Box<string>`).
    ///
    /// Type-argument **arity**: M9 assumes correct arity (the fixtures supply it). A
    /// wrong count is handled **gracefully** — the parameter/argument pairs are zipped
    /// to the shorter list, so a surplus on either side is ignored rather than
    /// panicking; an unmapped parameter simply survives the substitution. No diagnostic
    /// is emitted (the `TK2558` wrong-arity check is a future milestone, out of M9
    /// scope). An argument that cannot be lowered (out of subset) aborts with `None`,
    /// matching the other lowerings.
    ///
    /// FLAG (structural vs nominal generic instances): a generic interface instance is
    /// the **substituted structural type**, not a distinct nominal type per
    /// instantiation. M9 only needs structural assignability for the fixtures, and the
    /// task explicitly allows this; nominal generic instances (so that `Box<number>`
    /// and a structurally-equal `{ value: number }` are *not* interchangeable) are a
    /// later concern.
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

        // M25: a **conditional** template instantiates **lazily** — as an interned
        // [`InstantiationType`] the evaluator applies (and distributes) on demand — rather
        // than eager substitution. This keeps a self-recursive conditional alias from
        // expanding at lowering (which would loop) and lets distribution derive per-member
        // branches from the naked check parameter.
        //
        // M28: a **mapped** template instantiates lazily too (faithful mirror of the
        // conditional machinery): a self-recursive mapped alias (`DeepPartial`) must
        // not expand at lowering — its reserved template row may not even be filled yet
        // while its own body lowers. Behavior-equivalent for non-recursive mapped
        // aliases (the evaluator's plain expansion runs the same `substitute`, and
        // both the pre- and post-expansion forms relate conservatively while
        // deferred). A **string-intrinsic marker** template (`Uppercase` — the prelude
        // seeding) also instantiates lazily, as the symbolic form the evaluator
        // intercepts by identity; eager substitution would erase it to the bare marker.
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

    /// Build a parameter frame mapping each declared type parameter's **source name**
    /// to its interned [`TypeTag::TypeParam`] type, pairing the pre-allocated `ids`
    /// (source order) with the names from `param_decl`. A parameter with no resolvable
    /// name (out of subset) is skipped; the frame only holds the parameters it can
    /// name. Interning here is what makes a body reference `T` resolve to a stable
    /// type-parameter id (see [`resolve_type_reference`]).
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

    /// Lower each type parameter's `extends` **constraint** (M24) into the store-side
    /// constraint column, keyed by [`TypeParamId`]. MUST be called with the parameter
    /// frame **already active** (inside [`with_type_params`]) so a constraint that
    /// references an earlier parameter of the same list — `<T, U extends T>` — resolves
    /// `T` to its parameter type. Every generic declaration site (functions, classes,
    /// interfaces, aliases) calls this once.
    ///
    /// The annotation's diagnostics surface normally: an unresolved constraint name
    /// (`T extends Bogus`) reports `TK2304` at the annotation, exactly like a
    /// value-position annotation (tsc parity — suppressing it would be a false negative,
    /// the unsafe direction; see the spec amendment, 162a78b). Only the *recording* is
    /// gated: a constraint that does not lower to a real type (unresolved name → error
    /// type, or out of subset → `None`) records **no constraint** — so explicit type
    /// arguments are then unchecked (no `TK2344` cascade off an error-type constraint)
    /// and inference proceeds, while a missing constraint never rejects an argument or
    /// drops a member. The constraint is a side column, not part of the interned
    /// `TypeParamType` identity, so it survives the de Bruijn re-key (invariants §2).
    ///
    /// **Circularity (`TK2313`, spec amendments 02f58a5 + c54bd47).** After the whole
    /// list is lowered, each parameter's constraint chain is followed through **bare
    /// type-parameter constraints and the bare-parameter MEMBERS of union constraints**
    /// (`<T extends T | number>` is circular too — the union-source assume-true rule
    /// would otherwise make it assignable-to-anything). Structural indirection —
    /// `<T extends { self: T }>` — ends a branch and stays legal; intersection
    /// composites are out of the type model (backlog 25). A chain that revisits the
    /// parameter itself (`<T extends T>`, the mutual `<T extends U, U extends T>` —
    /// BOTH flagged) reports `TK2313` at the constraint annotation and the parameter
    /// records **no constraint**. Without this, the degenerate cycle would hit the
    /// relation engine's assume-true stack and make `T` assignable to *everything* — a
    /// dropped error. Detection runs on the recorded columns **after** the full list is
    /// lowered (a later param's constraint isn't recorded yet mid-list), and clearing
    /// happens **after** all detections (so the mutual cycle is seen from both sides).
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
        // Pass 1 — lower + record every constraint in the list.
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
        // Pass 2 — circularity detection over the now-complete columns. Collect ALL
        // circular parameters before clearing any, so `<T extends U, U extends T>`
        // flags both (clearing `T` first would hide the cycle from `U`'s walk).
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

    /// Whether `start`'s constraint chain revisits `start` itself (M24 `TK2313`). The
    /// chain follows, per step, the **bare-parameter successors** of a constraint: a
    /// bare `TypeParam` constraint continues to that parameter, and a **union**
    /// constraint continues through each of its bare-`TypeParam` MEMBERS
    /// (`<T extends T | number>` is circular — spec amendment c54bd47; a union can
    /// branch, so this is a DFS, not a single-successor walk). Any other constraint
    /// shape (object, array, function, …) ends that branch: structural self-reference
    /// is legal and terminates via the relation engine's cycle stack. A cycle that does
    /// NOT pass through `start` (the walk dead-ends into some other pair's loop) stops
    /// via the visited set without flagging `start` — the parameters *on* that loop
    /// flag themselves when their own walks run. Because the successors are read off
    /// the **lowered** column, a transparent alias that collapses to a bare parameter
    /// (`type Id<X> = X; <T extends Id<T>>`) is caught too.
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

    /// Run `body` with the type-parameter frame `frame` pushed onto the scope stack,
    /// popping it afterwards (so the parameters are in scope **only** for `body`). The
    /// pop runs unconditionally, so a type parameter never leaks past its declaration.
    /// An empty frame (a non-generic declaration) is still pushed/popped — harmless and
    /// keeps the call sites uniform.
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

    /// Lower an interface body's members to a nominal `ObjectType` (M5). Mirrors
    /// [`lower_object_annotation`] but returns the `ObjectType` (the caller fills the
    /// reserved nominal id rather than interning a fresh structural object). A member
    /// that is not a plain property/index signature or WU1 method signature, or whose
    /// type cannot be lowered, is skipped — the interface keeps the members it can
    /// express (a partial interface is more useful than none, and the unsupported
    /// members are out of the current subset).
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
                    let Some(annotation) = sig.type_annotation.as_ref() else {
                        continue;
                    };
                    let Some(ty) = self.lower_annotation(scope, &annotation.type_annotation) else {
                        continue;
                    };
                    // M21: an optional property (`b?: T`) is a real member whose effective
                    // type bakes in `| undefined` (model `exactOptionalPropertyTypes` OFF).
                    // Unioning here (where `&mut Interner` is available) is what keeps the
                    // relation engine — which borrows `&Store` read-only and cannot intern —
                    // unchanged: a present source value is then related against `T | undefined`
                    // by the existing union-target logic, and a missing optional target is
                    // simply allowed in `relate_objects`. With it stored as a normal member,
                    // `keyof`/indexed-access include the key and excess no longer trips on it.
                    let ty = if sig.optional {
                        let undefined = self.interner.well_known().undefined;
                        self.interner.union(vec![ty, undefined])
                    } else {
                        ty
                    };
                    let mut prop = PropertyType::public(name.into_owned(), ty);
                    prop.optional = sig.optional;
                    // F5/backlog-03: carry the `readonly` modifier onto interface members
                    // (`interface I { readonly k: T }`). Previously dropped, so assigning to such
                    // a member was silently allowed. Part of the property's structural identity
                    // (hashed by the interner) but ignored by the relation engine for
                    // assignability; it gates the assignment target only (`TK2540`).
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

/// B28 — merge an `overlay` object type onto a `base`, `overlay` winning on every
/// conflict. Used to fold interface `extends` bases (left-to-right) and then the
/// interface's own members onto them: an `overlay` property replaces a same-named base
/// property (else appends), and an index / call / construct signature the `overlay`
/// declares shadows the base's (an absent one leaves the inherited one in place).
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

/// Phase 0a — **reserve**. Walk the top-level type declarations and, indexed by
/// the binder's type-space `DeclId`, record each one's lowering plan. Every
/// `interface` gets a fresh nominal object id reserved up front (empty body); each
/// `type` alias records its annotation for lazy resolution.
///
/// Reserving the interface ids *before* any body is resolved is what lets a body
/// reference itself or a sibling: `interface List { tail: List | null }` and the
/// mutual `Ping`/`Pong` lower because `List`/`Pong` already have ids by the time
/// their members are lowered.
///
/// M28: reserve runs **per compilation unit** (the prelude, then the user program):
/// name lookups resolve against the unit's own `scope`, and the decl/`resolved`
/// tables are **appended** through `&mut` (the caller pre-seeds `decls` — with
/// [`TypeDecl::Resolved`] prelude placeholders for the user unit — and sizes
/// `resolved` to the binder's full `type_decl_count`). The binder assigned type
/// `DeclId`s in the same prelude-then-user source order, so appending keeps the decl
/// table index-aligned with the `DeclId`s.
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
                // M28: a top-level **mapped**-type body reserves a mapped template id
                // and seeds `type_resolved`, mirroring the conditional-template
                // machinery, so a self-recursive reference (`DeepPartial<T[K]>` inside
                // the body) resolves to it as a lazy instantiation — never the error
                // type. The placeholder is filled in the fill step.
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
                // B29: a **non-generic** alias whose top body is an object type literal
                // (`type X = { a: X | null }`) reserves an object id and seeds
                // `type_resolved`, so a self-reference through a member resolves to this
                // stable id (legal member recursion) rather than re-entering into the
                // error type. Filled in the fill step. Generic object aliases stay
                // structural templates (instantiated by substitution), so they are not
                // seeded here.
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
            // M11: a named `class` reserves its **instance type** id (an empty object,
            // filled in the fill step with the class's fields + methods) so a field
            // can reference the class's own type or a sibling. The binder declared the
            // class name in the type space in the *same source order*, so pushing here
            // keeps `decls` index-aligned with the type `DeclId`s. An anonymous class
            // declared no type name (the binder skipped it), so it is skipped here too.
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

/// Allocate one fresh [`TypeParamId`] per declared type parameter (M9), in source
/// order, advancing the module-wide counter. Returns an empty vec for a
/// non-generic declaration (`None` type-parameter list). The ids are paired with
/// their source names later, when the body is lowered with a parameter frame in
/// scope ([`with_type_params`]).
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
