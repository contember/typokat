//! Shared data types threaded through the checker pass (architecture §5).
//!
//! The inference walk is split across the `checker` submodules (`decls`,
//! `classes`, `statements`, `narrowing`, `assignment`, `annotations`, `expr`,
//! `calls`), all of which operate on the [`Pass`] struct defined here. The
//! types in this module — the obligation/decl/class bookkeeping and the [`Pass`]
//! working set itself — are the shared vocabulary those submodules' `impl Pass`
//! blocks read and write, so their fields/variants are visible across the
//! `checker` module tree (`pub(in crate::check::checker)`).

use crate::binder::scope::ScopeId;
use crate::binder::symbol::{DeclId, SymbolId};
use crate::binder::Binder;
use crate::check::flow::{FlowNode, FlowNodeId};
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{ClassId, TypeParamId, Visibility};
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_ast::ast::{Class, TSInterfaceHeritage, TSType, TSTypeParameterDeclaration};
use rustc_hash::{FxHashMap, FxHashSet};

/// Which diagnostic an assignability obligation produces on failure. The
/// structural verdict is the same relation query; only the code/message mapping
/// differs (mvp-plan §6 "code mapping").
#[derive(Copy, Clone, PartialEq, Eq)]
pub(in crate::check::checker) enum ObligationKind {
    /// Annotation-vs-initializer, reassignment, or `return`-vs-declared-return.
    /// Maps a missing-property reason to `TK2741`, everything else to `TK2322`.
    Assignment,
    /// A call argument vs its parameter. Context-free argument failures map to
    /// `TK2345`; contextually typed fresh object/tuple literals use assignment-style
    /// diagnostics for their member/element mismatch parity with tsc.
    Argument,
}

/// One assignability obligation: `src` must be assignable to `tgt`, with the
/// resulting diagnostic's primary span at `src_span` and its code determined by
/// `kind`.
pub(in crate::check::checker) struct AssignObligation {
    pub(in crate::check::checker) src: TypeId,
    pub(in crate::check::checker) tgt: TypeId,
    pub(in crate::check::checker) src_span: Span,
    pub(in crate::check::checker) kind: ObligationKind,
}

/// One class-member override-compatibility check (backlog 06, `TK2416`): the
/// derived class's own instance member `own_ty` must be compatible with the direct
/// base's same-named member `base_ty`. Collected at fill time, decided in phase 2 by
/// the shared relation engine (a separate list from [`AssignObligation`] because a
/// **method** override follows tsc's *bivariant-parameter / covariant-return* rule,
/// composed from `is_assignable` over the signature's parts — see
/// [`emit_override_failures`](super::statements::emit_override_failures) — rather than
/// a single whole-type query). `name`/`derived`/`base` phrase the headline; `span` is
/// the derived member's name.
pub(in crate::check::checker) struct OverrideCheck {
    pub(in crate::check::checker) own_ty: TypeId,
    pub(in crate::check::checker) base_ty: TypeId,
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) derived: String,
    pub(in crate::check::checker) base: String,
    pub(in crate::check::checker) span: Span,
    /// Whether the **base** member was declared with method syntax (`m() {}`) — read
    /// from [`Pass::class_member_kinds`]. tsc keys the variance split on the base
    /// (target) member's kind: base method → bivariant parameters; base field /
    /// accessor → strict contravariance. The derived member's own kind is irrelevant
    /// to the verdict (it only positions the diagnostic).
    pub(in crate::check::checker) base_is_method: bool,
}

/// Per-declaration computed types, indexed by `DeclId`. `None` means a
/// declaration whose type could not be computed (out of subset); a reference to
/// it resolves to the error type defensively.
pub(in crate::check::checker) struct DeclTypes {
    types: Vec<Option<TypeId>>,
}

impl DeclTypes {
    pub(in crate::check::checker) fn new(count: u32) -> Self {
        DeclTypes {
            types: vec![None; count as usize],
        }
    }

    pub(in crate::check::checker) fn set(&mut self, id: DeclId, ty: TypeId) {
        if let Some(slot) = self.types.get_mut(id.index()) {
            *slot = Some(ty);
        }
    }

    pub(in crate::check::checker) fn get(&self, id: DeclId) -> Option<TypeId> {
        self.types.get(id.index()).copied().flatten()
    }
}

/// How a top-level type declaration lowers (M5, extended for M9 generics), indexed
/// by its type-space `DeclId`. Collected up front so every declaration's `TypeId`
/// is reserved before any body is resolved — the reserve-then-fill that makes
/// recursive/mutual references lowerable (mvp-plan M5, §6.3). The `'ast` borrows
/// the AST bodies.
///
/// M9: a declaration may be **generic** — carry an ordered list of type-parameter
/// ids ([`type_params`](TypeDecl::params)). While the body is resolved those
/// parameters are pushed onto [`Pass::type_param_scopes`] so a reference to one
/// (`value: T`) lowers to its interned [`TypeTag::TypeParam`] type; the parameters
/// leave scope when the body is done (a type parameter is in scope **only** within
/// its own declaration). A generic declaration's resolved id is its **template**
/// (a structural type containing the parameter types); a `TSTypeReference` with
/// type arguments instantiates it by substitution ([`instantiate_type_reference`]).
pub(in crate::check::checker) enum TypeDecl<'ast> {
    /// An `interface`. `reserved` is its id (allocated empty via
    /// `Interner::reserve_object`, filled once the members are lowered); `members`
    /// is the interface body, lowered in the fill step. A **non-generic** interface
    /// is **nominal** (its reserved id is filled in place and never re-interned). A
    /// **generic** interface (`params` non-empty) is filled with its parameter types
    /// embedded, becoming a structural **template**; instantiating `Box<number>`
    /// substitutes the template to `{ value: number }` — see the structural-vs-
    /// nominal FLAG on [`instantiate_type_reference`].
    Interface {
        reserved: TypeId,
        params: Vec<TypeParamId>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        members: &'ast [oxc_ast::ast::TSSignature<'ast>],
        /// B28: the interface's `extends` heritage clauses. Their members are folded
        /// into the reserved object at fill time (own members override by name; bases
        /// merged left-to-right), so inherited members are part of the interface's
        /// type for assignability / member access / `keyof` / mapped sources.
        extends: &'ast [TSInterfaceHeritage<'ast>],
    },
    /// A `type` alias — **transparent**. `annotation` is the aliased type, lowered
    /// on demand to the target id; `resolving` guards a recursive alias (out of the
    /// M5 subset — broken by yielding the error type rather than looping). A generic
    /// alias (`params` non-empty) lowers its annotation with the parameters in scope
    /// to a **template**; `Pair<number, string>` substitutes it.
    ///
    /// M25: a top-level **conditional-type** body reserves a conditional template id
    /// ([`conditional_template`](TypeDecl::Alias::conditional_template)) so a
    /// self-recursive reference (`type Unwrap<T> = … Unwrap<U> …`) lowers to a lazy
    /// instantiation of it rather than expanding (which would loop); it is filled in the
    /// fill step. `name`/`name_span` phrase the `TK2456` circular-alias diagnostic.
    Alias {
        annotation: &'ast TSType<'ast>,
        params: Vec<TypeParamId>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        resolving: bool,
        /// The reserved conditional template id when the top body is a conditional
        /// (M25), else `None`. `type_resolved` is seeded with it so a self-reference
        /// resolves here; the fill step lowers the body and fills it.
        conditional_template: Option<TypeId>,
        /// M28: the reserved **mapped** template id when the top body is a mapped type
        /// (`type DeepPartial<T> = { [K in keyof T]?: DeepPartial<T[K]> }`), else
        /// `None`. Mirrors `conditional_template`: `type_resolved` is seeded with it so
        /// a self-recursive reference resolves to a lazy instantiation of the reserved
        /// id (never the error type); the fill step lowers the body and fills it, and
        /// every instantiation of a mapped-template alias is **lazy** (an
        /// [`crate::types::repr::InstantiationType`] the evaluator expands on demand —
        /// faithful to the conditional-template machinery).
        mapped_template: Option<TypeId>,
        /// B29: the reserved object id when the top body is an **object type literal**
        /// (`type X = { a: X | null }`), else `None`. Mirrors `conditional_template`:
        /// `type_resolved` is seeded with it so a self-reference **through a member**
        /// resolves to this stable id (the m5 named-recursive representation) rather than
        /// re-entering into the error type; the fill step lowers the members and fills it.
        /// This is what keeps legal member recursion resolving correctly (and off the
        /// surface-cycle path — a seeded alias never re-enters `resolve_type_decl`).
        object_template: Option<TypeId>,
        /// The alias name (M25) — for the `TK2456` message.
        name: String,
        /// The alias name-declaration span (M25) — the `TK2456` primary span.
        name_span: Span,
    },
    /// A `class` (M11, extended for M13/M16). `reserved` is its **instance type** id: an
    /// object id allocated empty via `Interner::reserve_object`, filled with the
    /// class's fields and methods once they are lowered. Reserving up front lets a
    /// field reference the class's own type (`next: Node | null`) or a sibling,
    /// exactly like an interface. The instance type is a (nominal) object — member
    /// access and the structural relation reuse the M2 object rules; M13 adds member
    /// **visibility** + **declaring class** to those members (a class with a
    /// `private`/`protected` member becomes nominal). The constructor signature is
    /// **not** an instance member; it is built in the fill step and registered under
    /// the class's *value* `DeclId` (see [`Pass::class_ctors`]). `class_id` is this
    /// class's stable [`ClassId`], stamped onto every member it declares (the origin
    /// the access-control + nominal rules key on).
    ///
    /// M16: a **generic** class (`class Box<T>`) carries an ordered list of
    /// type-parameter ids ([`params`](TypeDecl::Class::params)), allocated up front like a
    /// generic interface's. While the class's instance type, static side, constructor,
    /// methods, and accessors are lowered ([`fill_class`]) those parameters are pushed
    /// onto [`Pass::type_param_scopes`] (the M9 frame), so a field/return/parameter
    /// referencing `T` (`value: T`, `get(): T`) lowers to its interned
    /// [`TypeTag::TypeParam`] type. The reserved instance id then holds a structural
    /// **template** (`{ value: T; get: () => T }`); a `TSTypeReference` `Box<number>`
    /// instantiates it by the M9 [`instantiate_type_reference`], and `new Box<number>(…)` /
    /// `new Box(…)` substitute into the constructor + instance ([`infer_new`]). A
    /// non-generic class has an empty list and behaves exactly as in M11.
    Class {
        reserved: TypeId,
        class_id: ClassId,
        params: Vec<TypeParamId>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        class: &'ast Class<'ast>,
    },
    /// M28 — a declaration **already fully resolved in a previous compilation unit**
    /// (the prelude): its `type_resolved` slot is seeded, so nothing is ever lowered
    /// from it again — only its ordered type-parameter ids are needed (for
    /// instantiation by substitution). Lifetime-free by design: the prelude's AST does
    /// not outlive the prelude pass, so the user pass's `TypeDecl<'ast>` table holds
    /// this variant for prelude indices instead of AST borrows.
    Resolved { params: Vec<TypeParamId> },
}

/// A generic value declaration's signature (M9): the ordered type-parameter ids
/// and the **template** function type (its signature with those parameter types
/// embedded). A generic call with explicit type arguments substitutes
/// `params[i] → type_args[i]` into `fn_ty`, then runs the usual arity/argument/
/// return checks against the instantiated signature.
#[derive(Clone)]
pub(in crate::check::checker) struct GenericSig {
    pub(in crate::check::checker) params: Vec<TypeParamId>,
    pub(in crate::check::checker) fn_ty: TypeId,
}

/// A class's `new`-relevant types (M11, extended for M12 inheritance), keyed by the
/// class's **value-space** `DeclId`. `new ClassName(args)` looks the class up via its
/// value slot, checks the arguments against [`ctor`](ClassInfo::ctor) (the
/// constructor signature — a function type whose parameters are the constructor's,
/// defaulting to zero when there is no explicit `constructor`), exactly like an M3
/// call, and yields [`instance`](ClassInfo::instance) (the class's instance type) as
/// the expression type. The constructor's return type is irrelevant to `new`; only
/// its parameters drive the arity/argument checks, so `ctor`'s return is `void` by
/// construction.
///
/// M12: with `extends`, [`instance`](ClassInfo::instance) is the **composed** type
/// (the base's members plus the subclass's own, the subclass overriding on a name
/// conflict — see [`fill_class`]). A derived class with **no own `constructor`**
/// inherits the base's signature, so [`ctor`](ClassInfo::ctor) is then the base's
/// constructor (driving `new Derived(...)`). [`super_ctor`](ClassInfo::super_ctor) is
/// the **base** class's constructor signature, used to check a `super(args)` call
/// inside this class's constructor body; it is `None` for a class with no `extends`.
#[derive(Copy, Clone)]
pub(in crate::check::checker) struct ClassInfo {
    /// The constructor signature as a function type (its parameters are the
    /// constructor's; the return is `void` and unused — `new` yields `instance`). For
    /// a derived class with no own `constructor`, this is the base's signature (M12).
    pub(in crate::check::checker) ctor: TypeId,
    /// The class's instance type (its fields + methods, a nominal object). For a
    /// derived class this is the composed (base + own) type (M12).
    pub(in crate::check::checker) instance: TypeId,
    /// The class's **static side** (M13): an object type holding the `static` fields
    /// and `static` methods (as function-typed properties). This is what the class
    /// **value** (`C` used as a value, the base of `C.staticMember`) resolves to; an
    /// instance member is *not* on it (cross-access → `TK2339`) and a static member
    /// is *not* on the instance type (also `TK2339`). For a derived class this is the
    /// composition of the base's static side with the class's own static members
    /// (own overriding on a name conflict), mirroring the instance side.
    pub(in crate::check::checker) static_side: TypeId,
    /// This class's stable [`ClassId`] (M13) — the origin stamped onto its members
    /// and the key the access-control + nominal rules use.
    pub(in crate::check::checker) class_id: ClassId,
    /// The **base** class's constructor signature (M12), for checking `super(args)`
    /// inside this class's constructor. `None` when the class has no `extends`.
    pub(in crate::check::checker) super_ctor: Option<TypeId>,
    /// Whether this class was declared `abstract` (M15). Read from the AST
    /// (`class.r#abstract`) in [`fill_class`] and consulted by [`infer_new`]:
    /// `new C(...)` on an `abstract` `C` is `TK2511`. **Only the directly-named
    /// class's flag matters** — a concrete subclass of an abstract class is *not*
    /// abstract (its own flag is `false`), so it instantiates fine. The flag is not
    /// part of the instance type's structural identity; it gates `new`, nothing else.
    pub(in crate::check::checker) is_abstract: bool,
    /// The **constructor's visibility** (backlog 20), for gating a direct `new C(...)`
    /// ([`infer_new`]): `private` is constructable only lexically inside the declaring
    /// class, `protected` also inside its subclasses. A derived class with **no own
    /// `constructor`** inherits the base's constructor visibility (mirroring
    /// [`ctor`](ClassInfo::ctor)); a class with an explicit or implicit public
    /// constructor is `Public`. Distinct from the F1/WU3 static-side construct-signature
    /// gating (`has_public_constructor`), which stays on the AST.
    pub(in crate::check::checker) ctor_visibility: Visibility,
    /// The [`ClassId`] that **declares** the constructor gating this class's `new`
    /// (backlog 20). For a derived class inheriting the base's constructor this is the
    /// **base** class (so the `TK2673`/`TK2674` message names the declaring class, and
    /// the `protected` subclass walk keys on it); otherwise this class itself. The
    /// declaring class's *name* is looked up in [`Pass::class_names`] for the message.
    pub(in crate::check::checker) ctor_declaring_class: ClassId,
}

/// A class's fill progress (M12), tracked per [`TypeDecl`] index so a derived class
/// can fill its base **on demand and exactly once**, in any declaration order, while
/// an `extends` cycle terminates.
///
///  - `Pending` — not filled yet (the initial state for every class index).
///  - `Filling` — fill is in progress. Re-entering a `Filling` class (only possible
///    via an `extends` **cycle**, `class A extends B {}` / `class B extends A {}`)
///    does **not** recurse: the cycle is broken by treating the base's contribution
///    as empty/absent, so lowering terminates without a panic or infinite loop.
///  - `Done` — the class's instance type + constructor are built and registered.
///
/// A non-class index is `Done` from the start (nothing to fill).
#[derive(Copy, Clone, PartialEq, Eq)]
pub(in crate::check::checker) enum ClassFillState {
    Pending,
    Filling,
    Done,
}

/// The phase-1 working set threaded through the walk: everything the inference
/// pass writes to. Bundled into one struct so the many recursive `infer_*`/
/// `lower_*` helpers take a single `&mut` rather than a long, churn-prone argument
/// list.
pub(in crate::check::checker) struct Pass<'a, 'ast> {
    pub(in crate::check::checker) interner: &'a mut Interner,
    pub(in crate::check::checker) binder: &'a Binder,
    /// The module scope currently being checked — the disambiguating half of the
    /// binder's `(module scope, span start)` scope-map keys and of
    /// [`reference_flow`](Pass::reference_flow) (backlog 58). Set before each
    /// module's fill/flow/check phase; single-file checking keeps one module scope.
    /// Correct only while bodies are walked strictly per-module — lazy cross-module
    /// body/return inference would desynchronize insert vs lookup keys.
    pub(in crate::check::checker) current_module: ScopeId,
    /// Named-type declarations (M5), indexed by type-space `DeclId`. Reserve-then-
    /// fill populates this in phase 0; a `TSTypeReference` resolves through the
    /// binder's type slot to a `DeclId`, then to a `TypeId` via `type_resolved`.
    pub(in crate::check::checker) type_decls: Vec<TypeDecl<'ast>>,
    /// Resolved named types (M5), indexed by type-space `DeclId`. An interface's
    /// entry is its reserved id; an alias's is filled on first resolution. `None`
    /// means unresolved (out of subset / recursive alias → error type). A
    /// `TSTypeReference` reads a stored id here — **never** an inlined copy of the
    /// referenced structure, which is what keeps lowering terminating. For a
    /// **generic** declaration (M9) this is the **template** (its body with the
    /// parameter types embedded); a `TSTypeReference` with arguments substitutes it.
    pub(in crate::check::checker) type_resolved: Vec<Option<TypeId>>,
    /// **Type-parameter scope stack** (M9): each frame maps an in-scope type
    /// parameter's source name to its interned [`TypeTag::TypeParam`] id. A frame is
    /// pushed while a generic declaration's body/signature is lowered and popped
    /// after, so a type parameter is in scope **only** within its own declaration
    /// (no leak). [`resolve_type_reference`] consults the **innermost** frame first,
    /// before the binder's type slot, so `T` shadows a same-named type only inside
    /// the generic.
    pub(in crate::check::checker) type_param_scopes: Vec<FxHashMap<String, TypeId>>,
    /// Running counter allocating a unique [`TypeParamId`] per declared type
    /// parameter across the whole module (the named-unique-id representation — see
    /// [`crate::types::repr::TypeParamId`]).
    pub(in crate::check::checker) next_type_param: u32,
    /// The **parent class** of each class (M13), keyed by [`ClassId`] → its base
    /// class's `ClassId`. Built in [`fill_class`] from a resolvable `extends` clause;
    /// a class with no (resolvable) base has no entry. Walked by
    /// [`is_class_or_subclass`] to decide `protected` access (the accessing context's
    /// class must be the declaring class **or** a subclass of it). Keying on the
    /// stable `ClassId` keeps the chain independent of interned-type identity.
    pub(in crate::check::checker) class_parents: FxHashMap<ClassId, ClassId>,
    /// Generic value signatures (M9), keyed by the value-space `DeclId` of a generic
    /// `function` declaration. A call `identity<number>(5)` looks the callee's
    /// `DeclId` up here, substitutes the type arguments into the template signature,
    /// and checks the call against the instantiated parameter/return types.
    pub(in crate::check::checker) generic_fns: FxHashMap<DeclId, GenericSig>,
    /// Class `new`-info (M11), keyed by a class's **value-space** `DeclId`. Filled in
    /// phase 0 (fill) once each class's instance type and constructor signature are
    /// built; read by `new ClassName(args)` ([`infer_new`]) to check the arguments
    /// and yield the instance type.
    pub(in crate::check::checker) class_ctors: FxHashMap<DeclId, ClassInfo>,
    /// A **generic** class's ordered type-parameter ids (M16), keyed by the same
    /// value-space `DeclId` as [`class_ctors`](Pass::class_ctors). Present only for a
    /// class with type parameters; absent for a non-generic class. Read by `new` to
    /// instantiate the class's constructor + instance: with explicit type arguments
    /// (`new Box<number>(…)`) the parameters substitute to the given args; without
    /// (`new Box(…)`) they are inferred from the constructor arguments via the M10
    /// engine, then substituted. It is a sibling map rather than a field on the `Copy`
    /// [`ClassInfo`] so that struct keeps its cheap copy semantics.
    pub(in crate::check::checker) class_type_params: FxHashMap<DeclId, Vec<TypeParamId>>,
    /// A class's **pending (unimplemented) abstract member names**, in declaration
    /// order (backlog 06), keyed by the class's value-space `DeclId`. Composed down
    /// the `extends` chain in [`fill_class`]: a class's own abstract members first,
    /// then its direct base's pending members that this class does not implement with
    /// an own concrete member. A **non-abstract** class with a non-empty list reports
    /// `TK2515` (one member) / `TK2654` (aggregated). Stored (even for abstract
    /// classes) so a subclass inherits it; a sibling map — like
    /// [`class_type_params`](Pass::class_type_params) — so [`ClassInfo`] stays `Copy`.
    pub(in crate::check::checker) class_pending_abstract: FxHashMap<DeclId, Vec<String>>,
    /// A class's instance members' **declaration kinds** (backlog 06): member name →
    /// `true` when the member was (last) declared with **method syntax** (`m() {}`),
    /// `false` for a field / accessor / parameter property. Keyed by the class's
    /// value-space `DeclId`; composed down the `extends` chain in [`fill_class`]
    /// (clone of the direct base's map overlaid with this class's own members — own
    /// wins), so an inherited member keeps the kind of wherever it was last declared.
    /// Read by [`collect_override_checks`] to key tsc's method-bivariance rule on the
    /// **base** member's kind ([`OverrideCheck::base_is_method`]). A sibling map —
    /// like [`class_pending_abstract`](Pass::class_pending_abstract) — so
    /// [`ClassInfo`] stays `Copy`.
    pub(in crate::check::checker) class_member_kinds: FxHashMap<DeclId, FxHashMap<String, bool>>,
    /// A class's **display name** keyed by its stable [`ClassId`] (backlog 20). Built in
    /// [`fill_class`] for every named class, so [`infer_new`] can name the constructor's
    /// **declaring** class in a `TK2673`/`TK2674` message — the declaring class may be a
    /// base ([`ClassInfo::ctor_declaring_class`]), not the class being constructed. A
    /// sibling map — like [`class_member_kinds`](Pass::class_member_kinds) — so
    /// [`ClassInfo`] stays `Copy` (a `ClassId` is `Copy`, a name is not).
    pub(in crate::check::checker) class_names: FxHashMap<ClassId, String>,
    /// Per-class fill state (M12), indexed by `TypeDecl` index (parallel to
    /// [`type_decls`](Pass::type_decls)). A class entry tracks whether its instance
    /// type / constructor have been built yet, so a derived class can fill its **base
    /// first on demand** ([`ensure_class_filled`]) regardless of declaration order,
    /// and an `extends` **cycle** is broken (a class re-entered while `Filling`
    /// composes against the base's members built so far rather than looping). A
    /// non-class index stays `Done` (nothing to fill).
    pub(in crate::check::checker) class_fill: Vec<ClassFillState>,
    /// B28/B29 — per-**template** fill state, indexed by `TypeDecl` index (parallel to
    /// [`type_decls`](Pass::type_decls)). Mirrors [`class_fill`](Pass::class_fill) and
    /// covers both reserved-object template kinds: **interfaces**
    /// ([`ensure_interface_filled`]) and **seeded object-literal aliases**
    /// ([`ensure_object_alias_filled`]). Each index is only ever advanced by the ensure
    /// fn matching its decl kind. Fill is on demand and base-first in any declaration
    /// order (interface heritage force-fills its interface / class / alias base —
    /// [`ensure_heritage_base_filled`]), and an `extends` cycle is broken by the
    /// `Filling` guard (out-of-scope TS2310 — no diagnostic, terminates). Any other
    /// index stays `Done`.
    pub(in crate::check::checker) template_fill: Vec<ClassFillState>,
    pub(in crate::check::checker) decl_types: DeclTypes,
    pub(in crate::check::checker) obligations: Vec<AssignObligation>,
    /// Backlog 06 — pending class-member override-compatibility checks (`TK2416`),
    /// collected in [`fill_class`] and decided in phase 2 (see [`OverrideCheck`]).
    pub(in crate::check::checker) override_checks: Vec<OverrideCheck>,
    pub(in crate::check::checker) diagnostics: Vec<Diagnostic>,
    /// The **current `this` type** (M11): the instance type of the class whose
    /// member body is being checked, or `None` outside any class member. Set (via
    /// save/restore) while [`check_class`] walks a method/constructor body, so a
    /// `ThisExpression` inside resolves to the instance type (`this.field`,
    /// `this.method()`). It must NOT leak: a `this` anywhere else resolves to the
    /// error type (no narrowing, no crash). A nested function/arrow inside a method
    /// keeps the same `this` (lexical `this`); the field is saved/restored only at
    /// class-member boundaries, so a plain function body does not reset it (matching
    /// arrow/lexical-`this` semantics — and the fixtures never rely on a function's
    /// own `this`).
    pub(in crate::check::checker) current_this: Option<TypeId>,
    /// The **current class context** (M13): the [`ClassId`] of the class whose member
    /// body is being checked, or `None` outside any class member. Set via
    /// save/restore by [`check_class`] alongside [`current_this`](Pass::current_this),
    /// it is the *accessing context* the access-control checks key on at a member
    /// access `obj.m`: a `private` member is reachable only when `current_class` **is**
    /// the member's declaring class; a `protected` member when `current_class` is the
    /// declaring class **or a subclass** of it (walking [`class_parents`]). It must
    /// NOT leak — an access outside any class member has `current_class == None`, so a
    /// non-public member is correctly rejected there. Like `current_this`, a nested
    /// function/arrow keeps the enclosing class (restored only at class-member
    /// boundaries — lexical, matching `this`).
    pub(in crate::check::checker) current_class: Option<ClassId>,
    /// The **current base-constructor signature** (M12): the constructor signature of
    /// the *base* class of the class whose member body is being checked, or `None`
    /// when that class has no `extends` (or outside any class member). Set via
    /// save/restore by [`check_class`] alongside [`current_this`](Pass::current_this),
    /// so a `super(args)` call inside the derived constructor ([`infer_call`]) is
    /// checked against the base constructor's arity (`TK2554`) and argument types
    /// (`TK2345`). It must NOT leak: a `super(...)` outside a derived class member has
    /// no signature and is ignored (no crash). Like `current_this`, a nested
    /// function/arrow keeps the enclosing value (it is restored only at class-member
    /// boundaries).
    pub(in crate::check::checker) current_super_ctor: Option<TypeId>,
    /// Whether the body currently being checked is the **declaring class's
    /// constructor** (M14). Set to `true` (via save/restore) only while
    /// [`check_class`] walks the `constructor` body of the class whose context is in
    /// [`current_class`](Pass::current_class), and `false` for every other member
    /// body (other methods, field initializers) and outside any class. It is the
    /// **one place** a `readonly` member may be assigned: an assignment target
    /// `this.prop` where `prop` is `readonly` is allowed iff `current_in_ctor` is
    /// `true` **and** `current_class` is `prop`'s declaring class; anywhere else a
    /// `readonly` target is `TK2540` ([`check_member_assignment`]). Like the other
    /// member-context fields it never leaks (restored at the constructor's boundary).
    pub(in crate::check::checker) current_in_ctor: bool,
    /// **Flow-node arena** (M23, architecture §5) — the single narrowing model. A
    /// pre-pass ([`build_flow_graph`](super::Pass::build_flow_graph)) lowers each
    /// function body / the module top level into these nodes; slots 0/1 are the
    /// [`FlowNodeId::UNREACHABLE`]/[`FlowNodeId::START`] sentinels. A reference's
    /// narrowed type is a memoized backward walk from its flow node
    /// ([`resolve_narrowed_type`](super::Pass::resolve_narrowed_type)).
    pub(in crate::check::checker) flow_nodes: Vec<FlowNode>,
    /// The flow pre-pass's working cursor: the flow node currently in effect as the
    /// builder walks. Meaningless during the check walk (which resolves via
    /// [`reference_flow`](Pass::reference_flow)).
    pub(in crate::check::checker) flow_cursor: FlowNodeId,
    /// The flow pre-pass's enclosing-loop stack, so a `continue` can find its target
    /// loop label (the back-edge target). `break` uses [`break_targets`] instead — a
    /// `break` exits the nearest loop **or** `switch`, `continue` only a loop.
    pub(in crate::check::checker) flow_loops: Vec<FlowLoopFrame>,
    /// The flow pre-pass's enclosing-**breakable** stack (loops + switches): each
    /// entry collects the `break` edges of one construct, joined into its exit flow.
    /// Separate from [`flow_loops`] because a `break` targets the nearest loop or
    /// `switch`, while a `continue` skips any intervening `switch` to the loop label.
    pub(in crate::check::checker) break_targets: Vec<Vec<FlowNodeId>>,
    /// **Reference → flow node** map (M23), keyed by `(module scope, reference span
    /// start)`. Populated by the pre-pass; read by the check walk's
    /// [`resolve_identifier_type`] to resolve a reference against the flow node in
    /// effect at its position. A miss defaults to [`FlowNodeId::START`] (the
    /// declared type — the sound over-report), so partial pre-pass coverage never
    /// under-reports. Keying on `SymbolId` in the resolver keeps narrowing `x` from
    /// touching another symbol / a property access / a shadowed binding. The module
    /// scope disambiguates offset-aligned references across a project's files (it is
    /// also rebuilt per module immediately before that module's check — backlog 58).
    pub(in crate::check::checker) reference_flow: FxHashMap<(ScopeId, u32), FlowNodeId>,
    /// Resolver memo, keyed `(flow node, symbol) → narrowed type`. Durable across
    /// the whole check pass (ids are globally unique). A value that depended on an
    /// in-progress loop back edge is **never** written here (gated on
    /// [`flow_loop_depth`](Pass::flow_loop_depth) — invariants §1).
    pub(in crate::check::checker) flow_memo: FxHashMap<(FlowNodeId, SymbolId), TypeId>,
    /// Provisional loop-label seeds during a fixpoint resolution: a re-entrant walk
    /// of a loop label returns its seed here instead of looping. Cleared per label
    /// once its fixpoint resolves; never promoted to [`flow_memo`](Pass::flow_memo).
    pub(in crate::check::checker) flow_provisional: FxHashMap<(FlowNodeId, SymbolId), TypeId>,
    /// Depth of in-progress loop-label fixpoints. `> 0` suppresses durable memo
    /// writes (the resolved value may depend on a provisional seed), which is what
    /// keeps a stale pre-loop narrow state from being cached across a back edge.
    pub(in crate::check::checker) flow_loop_depth: u32,
    /// **Conditional-type evaluation memo** (M25): `substituted conditional /
    /// instantiation id → result`. Durable across the whole pass (ids are globally
    /// unique and hash-consing makes the key total). A result reached under budget
    /// exhaustion or through an in-flight cycle is never written here (the evaluator's
    /// provisional discipline — invariants §1).
    pub(in crate::check::checker) cond_memo: FxHashMap<TypeId, TypeId>,
    /// **Conditional-type lowering contexts** (M25): one [`CondFrame`] per
    /// `lower_conditional_type` call currently on the stack — pushed for the WHOLE node
    /// (check/extends/true/false positions), with the node's `infer` binders in scope
    /// (`active`) only while its `extends` type + true branch are lowered. A reference
    /// resolving to a NON-innermost frame is a **cross-binder** nested-`infer` reference
    /// (backlog 26 stopgap): it resolves without `TK2304` but poisons every node from
    /// the referencing one up to and including the binder-owning one. A name found in no
    /// active frame falls through to ordinary resolution (`TK2304` — e.g. an own-binder
    /// reference in this node's false branch).
    pub(in crate::check::checker) cond_frames: Vec<CondFrame>,
    /// Whether a **type-declaration template** is currently being lowered (M25): an
    /// alias / interface / class body. While true, a concrete conditional is left as its
    /// interned node rather than evaluated eagerly — evaluation is a value-position
    /// demand only (a generic template's conditional must stay a template until
    /// instantiated). Set via save/restore around each template body's lowering.
    pub(in crate::check::checker) building_template: bool,
    /// The **conditional-alias declaration currently being resolved** (M25): its type
    /// `DeclId`, name-declaration span, and name. Set while a `type A = C extends E ? …`
    /// body is lowered, so a check type that surface-references `A` itself is caught as
    /// `TK2456` at the alias declaration. `None` outside such a body.
    pub(in crate::check::checker) resolving_conditional_alias: Option<(DeclId, Span, String)>,
    /// The **plain alias declaration currently being resolved** (M26): its type
    /// `DeclId`, name-declaration span, and name, save/restored per nested
    /// `resolve_type_decl` call. Consumed ONLY by `lower_mapped_type`, so a mapped key
    /// source that surface-references the alias itself
    /// (`type M = { [K in keyof M]: number }`) is `TK2456` at the alias declaration —
    /// the silent re-entry error type would otherwise feed the mapped evaluation a
    /// bogus source. Kept separate from
    /// [`resolving_conditional_alias`](Pass::resolving_conditional_alias) (which the
    /// conditional fill loop scopes to top-level conditional bodies only) so nested
    /// conditionals inside plain alias bodies keep their M25 behavior.
    pub(in crate::check::checker) resolving_alias: Option<(DeclId, Span, String)>,
    /// B29 — the **stack** of aliases currently being resolved (pushed/popped per
    /// `resolve_type_decl`), so a detected surface cycle can report `TK2456` at **every**
    /// alias in the cycle, not just the one re-entered. A re-entry into a stack member
    /// (`type Mut1 = Mut2 | null; type Mut2 = Mut1`) reports the whole slice from that
    /// member to the top. Distinct from [`resolving_alias`](Pass::resolving_alias) (the
    /// single innermost, for M26 mapped detection). The trailing `u32` is the
    /// [`alias_indirection_depth`](Pass::alias_indirection_depth) at which the alias
    /// started resolving: a re-entry at the **same** depth is a surface cycle (`TK2456`);
    /// a re-entry at a **greater** depth came through a type constructor (array / tuple /
    /// function / object member) — legal recursion, silently error-typed (no `TK2456`).
    pub(in crate::check::checker) resolving_alias_stack: Vec<(DeclId, Span, String, u32)>,
    /// B29 — how many **type-constructor** boundaries (array element, tuple element,
    /// object-literal member, function/constructor parameter or return) the current
    /// lowering has descended through. Bracketed by
    /// [`with_indirection`](Pass::with_indirection). Only constructors that make
    /// recursion **legal** increment it; unions / intersections / `keyof` stay at the
    /// surface (a recursive alias through them is a genuine cycle). Conservative by
    /// design: a missed increment over-reports `TK2456` (safe), never under-reports.
    pub(in crate::check::checker) alias_indirection_depth: u32,
    /// B29 — aliases confirmed to be part of a **surface cycle** (`TK2456` reported).
    /// Their resolution is forced to the error type (final, not provisional — a detected
    /// cycle is a settled verdict), so the M22 silent-downstream discipline holds.
    pub(in crate::check::checker) circular_aliases: FxHashSet<usize>,
    /// **Mapped-type lowering contexts** (M26): one [`MappedFrame`] per
    /// `lower_mapped_type` call currently on the stack, each recording the node's key
    /// binder name. While a frame is active, an indexed access `X[K]` whose index names
    /// the innermost mapped key lowers to the node-scoped [`crate::types::repr::TypeTag::MappedValue`]
    /// placeholder (the source property value `T[K]`) instead of eagerly resolving.
    pub(in crate::check::checker) mapped_frames: Vec<MappedFrame>,
}

/// One enclosing-loop frame for the flow pre-pass: the loop's label (the
/// `continue`/back-edge target). `break` edges live on [`Pass::break_targets`]
/// (shared with `switch`), not here.
pub(in crate::check::checker) struct FlowLoopFrame {
    pub(in crate::check::checker) label: FlowNodeId,
}

/// One mapped-type lowering context (M26): the node's key binder name. Lives on
/// [`Pass::mapped_frames`] while the mapped type's value template is lowered, so an
/// indexed access on the key (`T[K]`) is recognized as the source-value placeholder.
pub(in crate::check::checker) struct MappedFrame {
    /// The key binder name (`K` in `{ [K in S]: V }`).
    pub(in crate::check::checker) key_name: String,
    /// M28 — the **captured modifiers source**: the lowered `T` of a `T[P]` indexed
    /// access on this frame's key whose object side is a bare in-scope type parameter
    /// (the `Pick` shape; nothing else is captured, so no new diagnostics can fire).
    /// First capture wins; consumed by `lower_mapped_type` into
    /// [`crate::types::repr::MappedType::modifiers_source`] for a NON-homomorphic node.
    pub(in crate::check::checker) captured_source: Option<TypeId>,
}

/// One conditional-type lowering context (M25): the node's `infer` binder frame plus the
/// cross-binder poison flag (backlog 26 stopgap). Lives on [`Pass::cond_frames`] for the
/// whole `lower_conditional_type` call.
#[derive(Default)]
pub(in crate::check::checker) struct CondFrame {
    /// This node's `infer` name → de Bruijn index map. A new name takes index
    /// `binders.len()`; a repeated name reuses its index (`infer_count` is the final
    /// `binders.len()`).
    pub(in crate::check::checker) binders: FxHashMap<String, u32>,
    /// Whether this node's binders are in scope — `true` only while its `extends` type
    /// and true branch are lowered.
    pub(in crate::check::checker) active: bool,
    /// Set when a cross-binder reference poisons this node (see
    /// [`crate::types::repr::ConditionalType::poisoned`]).
    pub(in crate::check::checker) poisoned: bool,
}
