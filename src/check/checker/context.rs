//! Shared checker-pass data types (architecture §5).
//! The `checker` submodules all operate on [`Pass`], so this module owns the
//! obligation, declaration, class, and flow bookkeeping visible within the tree.

use crate::binder::scope::ScopeId;
use crate::binder::symbol::{DeclId, SymbolId};
use crate::binder::Binder;
use crate::check::flow::{FlowNode, FlowNodeId};
use crate::diagnostics::{Diagnostic, IncompleteSurface};
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
    /// A contextually typed fresh literal in a call argument. Most structural
    /// mismatches stay assignment-style, but missing required properties and
    /// tuple length mismatches remain call-argument failures.
    FreshArgument,
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

/// One class-member override-compatibility check (`TK2416`).
/// Collected at fill time, decided in phase 2, and kept separate from
/// [`AssignObligation`] because method overrides use tsc's bivariant-param /
/// covariant-return rule rather than one whole-type relation query.
pub(in crate::check::checker) struct OverrideCheck {
    pub(in crate::check::checker) own_ty: TypeId,
    pub(in crate::check::checker) base_ty: TypeId,
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) derived: String,
    pub(in crate::check::checker) base: String,
    pub(in crate::check::checker) span: Span,
    /// Whether the base member used method syntax.
    /// tsc keys the override variance rule on the base member kind; the derived
    /// member's kind only affects diagnostic positioning.
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

/// A function declaration's callable signature, reserved before statement bodies
/// are checked. The reservation owns the stable generic ids and lowered signature;
/// body checking later fills only an unannotated return type.
pub(in crate::check::checker) struct FunctionSurface {
    pub(in crate::check::checker) params: Vec<crate::types::repr::ParameterType>,
    pub(in crate::check::checker) type_params: Vec<TypeParamId>,
    pub(in crate::check::checker) type_param_frame: FxHashMap<String, TypeId>,
    pub(in crate::check::checker) declared_return: Option<TypeId>,
    pub(in crate::check::checker) function_ty: TypeId,
    /// Records produced by eager signature lowering, replayed at source position.
    pub(in crate::check::checker) diagnostics: Vec<Diagnostic>,
    pub(in crate::check::checker) incomplete: Vec<IncompleteSurface>,
}

/// An explicit `var` annotation lowered before executable checking. The type makes
/// the hoisted binding usable, while records wait for the declaration position.
pub(in crate::check::checker) struct VarAnnotationSurface {
    pub(in crate::check::checker) annotation: Option<TypeId>,
    pub(in crate::check::checker) diagnostics: Vec<Diagnostic>,
    pub(in crate::check::checker) incomplete: Vec<IncompleteSurface>,
}

/// Whether a shared `var` declaration type is only a forward annotation, comes
/// from its first source declarator, or belongs to an earlier non-`var` binding.
pub(in crate::check::checker) enum VarValueTypeState {
    Provisional,
    Source,
    Existing,
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

/// A class's fill progress, tracked per [`TypeDecl`] index.
/// `Filling` breaks `extends` cycles by treating the re-entered base as absent, so
/// lowering terminates; non-class indices start as `Done`.
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
    /// The module scope currently being checked.
    /// Disambiguates span-start keyed lookups for scopes and flow. Correct only
    /// while bodies are walked per module; lazy cross-module inference would
    /// desynchronize insert and lookup keys.
    pub(in crate::check::checker) current_module: ScopeId,
    /// Named-type declarations (M5), indexed by type-space `DeclId`. Reserve-then-
    /// fill populates this in phase 0; a `TSTypeReference` resolves through the
    /// binder's type slot to a `DeclId`, then to a `TypeId` via `type_resolved`.
    pub(in crate::check::checker) type_decls: Vec<TypeDecl<'ast>>,
    /// Resolved named types, indexed by type-space `DeclId`.
    /// References read stored ids, never inline copies, which keeps lowering
    /// terminating. Generic entries store templates to instantiate by substitution.
    pub(in crate::check::checker) type_resolved: Vec<Option<TypeId>>,
    /// Type-parameter scope stack.
    /// Frames are pushed only around their generic declaration, and innermost
    /// frames shadow binder type slots so `T` does not leak.
    pub(in crate::check::checker) type_param_scopes: Vec<FxHashMap<String, TypeId>>,
    /// Running counter allocating a unique [`TypeParamId`] per declared type
    /// parameter across the whole module (the named-unique-id representation — see
    /// [`crate::types::repr::TypeParamId`]).
    pub(in crate::check::checker) next_type_param: u32,
    /// Parent class by stable [`ClassId`], built from resolvable `extends`.
    /// Used by `protected` access and independent of interned type identity.
    pub(in crate::check::checker) class_parents: FxHashMap<ClassId, ClassId>,
    /// Generic signature metadata keyed by the template function type itself.
    /// Overload resolution walks signature `TypeId`s, so generic overloads need
    /// the candidate-local type parameters here instead of a global callee entry.
    pub(in crate::check::checker) generic_sig_params: FxHashMap<TypeId, Vec<TypeParamId>>,
    /// Class `new`-info (M11), keyed by a class's **value-space** `DeclId`. Filled in
    /// phase 0 (fill) once each class's instance type and constructor signature are
    /// built; read by `new ClassName(args)` ([`infer_new`]) to check the arguments
    /// and yield the instance type.
    pub(in crate::check::checker) class_ctors: FxHashMap<DeclId, ClassInfo>,
    /// Direct constructor overload signatures for class values, keyed like
    /// [`class_ctors`](Pass::class_ctors). Empty/absent means the implementation
    /// constructor signature is the externally callable one.
    pub(in crate::check::checker) class_ctor_overloads: FxHashMap<DeclId, Vec<TypeId>>,
    /// Generic class type parameters, keyed like [`class_ctors`](Pass::class_ctors).
    /// `new` uses these for explicit substitution or constructor-argument inference.
    /// Kept out of [`ClassInfo`] so that struct stays `Copy`.
    pub(in crate::check::checker) class_type_params: FxHashMap<DeclId, Vec<TypeParamId>>,
    /// Pending abstract member names per class, in declaration order.
    /// Composed down `extends`; non-abstract classes report `TK2515`/`TK2654`.
    /// Stored outside [`ClassInfo`] so subclasses inherit it without losing `Copy`.
    pub(in crate::check::checker) class_pending_abstract: FxHashMap<DeclId, Vec<String>>,
    /// Instance member declaration kinds per class (`true` = method syntax).
    /// Composed down `extends` with own members winning, so `TK2416` can key
    /// method bivariance on the base member's last declaration kind.
    pub(in crate::check::checker) class_member_kinds: FxHashMap<DeclId, FxHashMap<String, bool>>,
    /// Display name by stable [`ClassId`].
    /// Lets constructor-access diagnostics name the declaring class, which may be
    /// an inherited base, while keeping [`ClassInfo`] `Copy`.
    pub(in crate::check::checker) class_names: FxHashMap<ClassId, String>,
    /// Per-class fill state, parallel to [`type_decls`](Pass::type_decls).
    /// Enables base-first on-demand fill in any declaration order and breaks
    /// `extends` cycles via the `Filling` guard.
    pub(in crate::check::checker) class_fill: Vec<ClassFillState>,
    /// Per-template fill state for interfaces and seeded object-literal aliases.
    /// Mirrors class fill: on-demand, base-first, decl-kind-specific, and guarded
    /// by `Filling` so out-of-scope `extends` cycles terminate.
    pub(in crate::check::checker) template_fill: Vec<ClassFillState>,
    pub(in crate::check::checker) decl_types: DeclTypes,
    /// Explicit `var` annotations reserved across one function/module hoist
    /// container, keyed by their own `(module, declarator span)` source site.
    pub(in crate::check::checker) var_annotation_surfaces:
        FxHashMap<(ScopeId, u32), VarAnnotationSurface>,
    /// Publication state for each shared `var` value declaration. This keeps a
    /// forward annotation provisional without overwriting a parameter type.
    pub(in crate::check::checker) var_value_type_states: FxHashMap<DeclId, VarValueTypeState>,
    pub(in crate::check::checker) obligations: Vec<AssignObligation>,
    /// Backlog 06 — pending class-member override-compatibility checks (`TK2416`),
    /// collected in [`fill_class`] and decided in phase 2 (see [`OverrideCheck`]).
    pub(in crate::check::checker) override_checks: Vec<OverrideCheck>,
    pub(in crate::check::checker) diagnostics: Vec<Diagnostic>,
    /// The third structured channel (sprint 2026-07-10, WU2): in-scope AST positions
    /// the walk skipped. Populated via `record_incomplete`; drained into `CheckResult`
    /// alongside `diagnostics`. Nothing emits into it yet (WU3–5 wire the emissions).
    pub(in crate::check::checker) incomplete: Vec<IncompleteSurface>,
    /// Current `this` type while checking class members.
    /// Save/restored at member boundaries so it never leaks; nested functions keep
    /// the enclosing value in this subset.
    pub(in crate::check::checker) current_this: Option<TypeId>,
    /// Current class context for access-control checks.
    /// Save/restored with `current_this`; `private` requires the declaring class and
    /// `protected` allows subclasses. Outside class members it is `None`.
    pub(in crate::check::checker) current_class: Option<ClassId>,
    /// Current base-constructor signature for checking `super(args)`.
    /// Save/restored at class-member boundaries so it never leaks; outside a
    /// derived class member, `super(...)` has no signature and is ignored.
    pub(in crate::check::checker) current_super_ctor: Option<TypeId>,
    /// Whether the current body is the declaring class's constructor.
    /// This gates the only allowed write to `readonly this.prop`: the current class
    /// must match the property's declaring class. Restored at constructor boundary.
    pub(in crate::check::checker) current_in_ctor: bool,
    /// Flow-node arena: the single narrowing model.
    /// A pre-pass lowers module/function bodies here; narrowed reference types are
    /// memoized backward walks from their flow node.
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
    /// Reference-to-flow-node map, keyed by `(module scope, reference span start)`.
    /// Misses default to START, the sound over-report. Resolver-side `SymbolId`
    /// checks keep narrowing from crossing symbols, properties, or shadowed bindings.
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
    /// Conditional-type evaluation memo.
    /// Durable pass-wide, but results reached under budget exhaustion or in-flight
    /// cycles are never stored (the provisional discipline from invariants §1).
    pub(in crate::check::checker) cond_memo: FxHashMap<TypeId, TypeId>,
    /// Conditional-type lowering contexts.
    /// Frames cover the whole node, but `infer` binders are active only in extends/true.
    /// Cross-binder nested-`infer` references poison the intervening nodes; names in no
    /// active frame fall through to ordinary resolution.
    pub(in crate::check::checker) cond_frames: Vec<CondFrame>,
    /// Whether a type-declaration template is being lowered.
    /// While true, concrete conditionals stay interned templates until a value-position
    /// demand evaluates them.
    pub(in crate::check::checker) building_template: bool,
    /// The **conditional-alias declaration currently being resolved** (M25): its type
    /// `DeclId`, name-declaration span, and name. Set while a `type A = C extends E ? …`
    /// body is lowered, so a check type that surface-references `A` itself is caught as
    /// `TK2456` at the alias declaration. `None` outside such a body.
    pub(in crate::check::checker) resolving_conditional_alias: Option<(DeclId, Span, String)>,
    /// Plain alias currently being resolved, for mapped self-reference diagnostics.
    /// `lower_mapped_type` uses this to report `TK2456` at the alias declaration
    /// instead of feeding a silent re-entry error type into mapped evaluation.
    /// Separate from conditional alias tracking so nested conditionals keep M25 behavior.
    pub(in crate::check::checker) resolving_alias: Option<(DeclId, Span, String)>,
    /// Stack of aliases currently resolving, used to report `TK2456` on every alias
    /// in a surface cycle. Each entry records its starting indirection depth: same-depth
    /// re-entry is circular; deeper re-entry came through a type constructor and is
    /// legal recursion, silently error-typed.
    pub(in crate::check::checker) resolving_alias_stack: Vec<(DeclId, Span, String, u32)>,
    /// Current legal-recursion indirection depth.
    /// Incremented only across type constructors; unions/intersections/`keyof` stay
    /// surface cycles. Missed increments over-report `TK2456`, the safe direction.
    pub(in crate::check::checker) alias_indirection_depth: u32,
    /// Current syntactic nesting depth of the annotation being lowered (backlog 63k).
    /// Bounds host recursion in `lower_annotation` so a pathologically deep type literal
    /// reports `TK2589` instead of overflowing the stack. Balanced through every return.
    pub(in crate::check::checker) annotation_depth: u32,
    /// B29 — aliases confirmed to be part of a **surface cycle** (`TK2456` reported).
    /// Their resolution is forced to the error type (final, not provisional — a detected
    /// cycle is a settled verdict), so the M22 silent-downstream discipline holds.
    pub(in crate::check::checker) circular_aliases: FxHashSet<usize>,
    /// Mapped-type lowering contexts.
    /// `X[K]` using the innermost mapped key lowers to the node-scoped
    /// [`crate::types::repr::TypeTag::MappedValue`] placeholder instead of eager resolution.
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
    /// Captured modifiers source for the `Pick` shape (`T[P]` on this frame's key).
    /// First capture wins; non-homomorphic mapped lowering consumes it.
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
