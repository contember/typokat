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

/// A top-level type declaration's reserve-then-fill plan, indexed by type-space
/// `DeclId`. Generic declarations carry ordered type-parameter ids and resolve to
/// templates instantiated by substitution.
pub(in crate::check::checker) enum TypeDecl<'ast> {
    /// An interface reserves an object id, then fills own and inherited members into it.
    /// Generic interfaces fill a template that type references instantiate later.
    Interface {
        reserved: TypeId,
        params: Vec<TypeParamId>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        members: &'ast [oxc_ast::ast::TSSignature<'ast>],
        /// Heritage clauses are composed into the reserved object during fill.
        extends: &'ast [TSInterfaceHeritage<'ast>],
    },
    /// A transparent alias lowers on demand. Template placeholders keep recursive
    /// conditional, mapped, and object-literal alias shapes from expanding inline.
    Alias {
        annotation: &'ast TSType<'ast>,
        params: Vec<TypeParamId>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        resolving: bool,
        /// Reserved conditional template id, seeded before the body is lowered.
        conditional_template: Option<TypeId>,
        /// Reserved mapped template id, kept lazy for recursive mapped aliases.
        mapped_template: Option<TypeId>,
        /// Reserved object id for legal member recursion in non-generic object aliases.
        object_template: Option<TypeId>,
        /// Alias name for circular-alias diagnostics.
        name: String,
        /// Alias name span for circular-alias diagnostics.
        name_span: Span,
    },
    /// A class reserves its instance object id up front; fill builds the instance,
    /// static side, constructor signature, and class metadata.
    Class {
        reserved: TypeId,
        class_id: ClassId,
        params: Vec<TypeParamId>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        class: &'ast Class<'ast>,
    },
    /// A declaration already resolved by an earlier compilation unit, such as the prelude.
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

/// A class's copyable `new` metadata, keyed by value-space `DeclId`.
#[derive(Copy, Clone)]
pub(in crate::check::checker) struct ClassInfo {
    /// Constructor parameters as a function type; `new` yields `instance`, not this return.
    pub(in crate::check::checker) ctor: TypeId,
    /// Composed instance type.
    pub(in crate::check::checker) instance: TypeId,
    /// Composed static side used when the class name is read as a value.
    pub(in crate::check::checker) static_side: TypeId,
    /// Stable class identity used by access-control and nominal rules.
    pub(in crate::check::checker) class_id: ClassId,
    /// Base constructor signature for `super(args)`, if any.
    pub(in crate::check::checker) super_ctor: Option<TypeId>,
    /// Whether directly constructing this class reports `TK2511`.
    pub(in crate::check::checker) is_abstract: bool,
    /// Constructor visibility for direct `new` accessibility checks.
    pub(in crate::check::checker) ctor_visibility: Visibility,
    /// Class that declares the constructor used for direct `new` accessibility checks.
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
    /// The flow pre-pass's named label stack. Labeled `break` edges exit the matching
    /// label's statement; labeled `continue` uses the matching labeled loop's target.
    pub(in crate::check::checker) label_targets: Vec<FlowLabelFrame>,
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

/// One named label frame for the flow pre-pass.
pub(in crate::check::checker) struct FlowLabelFrame {
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) breaks: Vec<FlowNodeId>,
    pub(in crate::check::checker) continue_target: Option<FlowNodeId>,
    pub(in crate::check::checker) allows_continue: bool,
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
