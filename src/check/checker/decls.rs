//! decls module (extracted from checker/mod.rs).

use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::binder::Binder;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{
    ClassId, ObjectType, PropertyType,
    TypeParamId,
};
use crate::types::store::TypeId;
use crate::types::{substitute, Interner};
use oxc_ast::ast::{
    Program, Statement, TSSignature, TSTypeName, TSTypeParameterDeclaration,
    TSTypeParameterInstantiation,
};
use rustc_hash::FxHashMap;
use super::context::*;

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
        let count = self.type_decls.len();

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
        for index in 0..count {
            let TypeDecl::Interface {
                reserved,
                ref params,
                param_decl,
                members,
            } = self.type_decls[index]
            else {
                continue;
            };
            let params = params.clone();
            let frame = self.build_type_param_frame(param_decl, &params);
            let object = self.with_type_params(frame, |pass| {
                pass.lower_interface_members(scope, members)
            });
            self.interner.fill_object(reserved, object);
        }

        // Resolve every remaining alias (interfaces are now filled, so a generic alias
        // instantiating an interface substitutes over the filled template). Resolution
        // is on-demand and idempotent, so touching every alias resolves the whole DAG.
        for index in 0..count {
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
        for index in 0..count {
            if matches!(self.type_decls[index], TypeDecl::Class { .. }) {
                self.ensure_class_filled(scope, index);
            }
        }
    }

    /// Resolve a single type declaration to its `TypeId`, memoizing the result in
    /// `pass.type_resolved`. Interfaces are already seeded (their reserved id);
    /// aliases are lowered on first request. A recursive alias (an alias whose body,
    /// directly or transitively, references itself) is detected by the `resolving`
    /// flag and broken by yielding the error type — recursive *aliases* are out of the
    /// M5 subset (recursion comes via interfaces), so this never loops.
    fn resolve_type_decl(&mut self, scope: ScopeId, decl_id: DeclId) -> TypeId {
        let error_ty = self.interner.well_known().error;

        // Already resolved (interface reserved id, or a previously-resolved alias).
        if let Some(existing) = self.type_resolved.get(decl_id.index()).copied().flatten() {
            return existing;
        }

        let index = decl_id.index();
        // Capture the alias annotation and its (M9) type-parameter frame inputs before
        // mutating, so the body is lowered with the parameters in scope.
        let (annotation, param_decl, params) = match self.type_decls.get(index) {
            Some(TypeDecl::Alias {
                annotation,
                param_decl,
                params,
                resolving: false,
            }) => (*annotation, *param_decl, params.clone()),
            // A reference re-entered while this alias is mid-resolution: a recursive
            // alias (out of subset). Break the cycle with the error type.
            Some(TypeDecl::Alias { resolving: true, .. }) => return error_ty,
            // An interface with no seeded id, or an out-of-range id: defensive.
            _ => return error_ty,
        };

        // Mark in-progress so a transitive self-reference is caught above.
        if let Some(TypeDecl::Alias { resolving, .. }) = self.type_decls.get_mut(index) {
            *resolving = true;
        }

        // M9: lower the annotation with the alias's type parameters in scope, so a
        // reference to `A`/`B` in `type Pair<A, B> = { … }` resolves to the parameter
        // type. The frame is popped before returning (a parameter does not leak).
        let frame = self.build_type_param_frame(param_decl, &params);
        let target =
            self.with_type_params(frame, |pass| pass.lower_annotation(scope, annotation))
                .unwrap_or(error_ty);

        if let Some(TypeDecl::Alias { resolving, .. }) = self.type_decls.get_mut(index) {
            *resolving = false;
        }
        if let Some(slot) = self.type_resolved.get_mut(index) {
            *slot = Some(target);
        }
        target
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

        // 2. With type arguments: instantiate the generic declaration by substitution.
        if let Some(args) = type_arguments {
            return self.instantiate_type_reference(scope, decl_id, args);
        }

        // 3. Bare named type (M5 behaviour).
        Some(self.resolve_type_decl(scope, decl_id))
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
        // type names / parameters live). A non-lowerable argument aborts.
        let mut lowered_args: Vec<TypeId> = Vec::with_capacity(args.params.len());
        for arg in &args.params {
            lowered_args.push(self.lower_annotation(scope, arg)?);
        }

        // The declaration's template (its body with parameter types embedded) and its
        // ordered parameter ids.
        let template = self.resolve_type_decl(scope, decl_id);
        let params = self.type_decl_params(decl_id);

        // Build the substitution, zipping parameters to arguments up to the shorter
        // list (graceful on an arity mismatch — no panic, no spurious diagnostic).
        let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
        for (&param, &arg) in params.iter().zip(&lowered_args) {
            map.insert(param, arg);
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
            | Some(TypeDecl::Class { params, .. }) => params.clone(),
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
    fn lookup_type_param(&self, name: &str) -> Option<TypeId> {
        self.type_param_scopes
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
    }

    /// Lower an interface body's members to a nominal `ObjectType` (M5). Mirrors
    /// [`lower_object_annotation`] but returns the `ObjectType` (the caller fills the
    /// reserved nominal id rather than interning a fresh structural object). A member
    /// that is not a plain required property signature, or whose type cannot be
    /// lowered, is skipped — the interface keeps the members it can express (a partial
    /// interface is more useful than none, and the unsupported members are out of the
    /// M5 subset).
    fn lower_interface_members(
        &mut self,
        scope: ScopeId,
        members: &[TSSignature<'_>],
    ) -> ObjectType {
        let mut object = ObjectType::default();
        for member in members {
            match member {
                TSSignature::TSPropertySignature(sig) => {
                    let Some(name) = sig.key.static_name() else {
                        continue;
                    };
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
                _ => continue,
            }
        }
        object
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
/// their members are lowered. Returns the per-`DeclId` decl table and a parallel
/// `resolved` table (interfaces pre-seeded with their reserved id; aliases `None`).
pub(in crate::check::checker) fn reserve_type_decls<'ast>(
    interner: &mut Interner,
    binder: &Binder,
    program: &'ast Program<'ast>,
    next_type_param: &mut u32,
    next_class_id: &mut u32,
) -> (Vec<TypeDecl<'ast>>, Vec<Option<TypeId>>) {
    let count = binder.type_decl_count as usize;
    // Placeholders so the tables are indexable by every type `DeclId`; a
    // declaration the binder counted but we don't recognise stays an unresolved
    // alias of a never-resolving annotation (defensive — not expected).
    let mut decls: Vec<TypeDecl<'ast>> = Vec::with_capacity(count);
    let mut resolved: Vec<Option<TypeId>> = vec![None; count];

    // Build by walking declarations in source order; the binder assigned type
    // `DeclId`s in that same order (`bind_type_declarations`), so pushing in order
    // keeps the decl table index-aligned with the `DeclId`s.
    for stmt in &program.body {
        match stmt {
            Statement::TSInterfaceDeclaration(iface) => {
                let reserved = interner.reserve_object();
                if let Some(decl_id) = type_decl_id(binder, binder.module, iface.id.name.as_str()) {
                    if let Some(slot) = resolved.get_mut(decl_id.index()) {
                        *slot = Some(reserved);
                    }
                }
                // M9: allocate one id per declared type parameter (in source order).
                let params = alloc_type_param_ids(iface.type_parameters.as_deref(), next_type_param);
                decls.push(TypeDecl::Interface {
                    reserved,
                    params,
                    param_decl: iface.type_parameters.as_deref(),
                    members: &iface.body.body,
                });
            }
            Statement::TSTypeAliasDeclaration(alias) => {
                let params = alloc_type_param_ids(alias.type_parameters.as_deref(), next_type_param);
                decls.push(TypeDecl::Alias {
                    annotation: &alias.type_annotation,
                    params,
                    param_decl: alias.type_parameters.as_deref(),
                    resolving: false,
                });
            }
            // M11: a named `class` reserves its **instance type** id (an empty object,
            // filled in the fill step with the class's fields + methods) so a field
            // can reference the class's own type or a sibling. The binder declared the
            // class name in the type space in the *same source order*, so pushing here
            // keeps `decls` index-aligned with the type `DeclId`s. An anonymous class
            // declared no type name (the binder skipped it), so it is skipped here too.
            Statement::ClassDeclaration(class) if class.id.is_some() => {
                let reserved = interner.reserve_object();
                // M13: a fresh stable `ClassId` for this declaration (source order),
                // stamped onto its members in `fill_class`.
                let class_id = ClassId(*next_class_id);
                *next_class_id += 1;
                if let Some(id) = &class.id {
                    if let Some(decl_id) = type_decl_id(binder, binder.module, id.name.as_str()) {
                        if let Some(slot) = resolved.get_mut(decl_id.index()) {
                            *slot = Some(reserved);
                        }
                    }
                }
                // M16: allocate one id per declared type parameter (in source order),
                // paired with their names later when the class body is lowered with the
                // parameter frame in scope (`fill_class`) — exactly like an interface.
                let params = alloc_type_param_ids(class.type_parameters.as_deref(), next_type_param);
                decls.push(TypeDecl::Class {
                    reserved,
                    class_id,
                    params,
                    param_decl: class.type_parameters.as_deref(),
                    class,
                });
            }
            _ => {}
        }
    }

    (decls, resolved)
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
pub(in crate::check::checker) fn type_decl_id(binder: &Binder, scope: ScopeId, name: &str) -> Option<DeclId> {
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.ty)
}

/// The **value**-space `DeclId` a name resolves to from `scope` (binder value slot),
/// if any (M11 — the class constructor side). Mirrors [`type_decl_id`] for the value
/// space.
pub(in crate::check::checker) fn value_decl_id(binder: &Binder, scope: ScopeId, name: &str) -> Option<DeclId> {
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}

