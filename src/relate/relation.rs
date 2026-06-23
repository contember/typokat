//! The relation engine: `is_assignable` / `is_subtype` / `is_identical`.
//!
//! Architecture §6, mvp-plan §4.4. This is "probably the biggest single piece"
//! and three of its properties are built correct-from-day-1 because retrofitting
//! them is a rewrite (mvp-plan §1.3):
//!
//!  1. the relation cache keyed on `(u32, u32, RelationKind)` (see `cache.rs`),
//!  2. an assume-true-until-disproven cycle stack consulted on re-entry,
//!  3. a result type that returns a **reason chain on failure**, not `bool`.
//!
//! M0 only exercises the intrinsic lattice + literal widening (no objects, no
//! cycles), but the cache, the cycle stack, and the reason chain are all wired
//! in now so M2–M6 add rules, not infrastructure.

use crate::relate::cache::{RelationCache, RelationKey};
use crate::types::repr::{IntrinsicKind, PropertyType, TypeTag, Visibility};
use crate::types::store::{Store, TypeId};
use crate::types::WellKnown;
use rustc_hash::FxHashSet;

/// The relations we cache. They are *different* relations with different rules
/// and must not share a cache (architecture §6.1). M0 uses `Assignable`;
/// `Identity`/`Subtype` are defined for the M4/M5 work that needs them.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum RelationKind {
    Identity,
    Subtype,
    Assignable,
}

/// One link in a failure explanation. M0 only ever produces a single
/// `Leaf { src, tgt }`; M2 adds the two object-structural causes. The structure
/// is recursive so the depth case nests "...because property `p`: <child reason>"
/// (architecture §6.4) — the chain M6 renders.
#[derive(Clone, Debug)]
pub enum Reason {
    /// The base mismatch: `src` is not assignable to `tgt`.
    Leaf { src: TypeId, tgt: TypeId },
    /// A **required target property is absent in the source** object. `src`/`tgt`
    /// are the two object types; `name` is the missing property. The checker maps
    /// this to `TK2741`.
    MissingProperty {
        name: String,
        src: TypeId,
        tgt: TypeId,
    },
    /// A property is **present but its type is incompatible**, wrapping the inner
    /// reason for that property's types. `src`/`tgt` are the two object types. The
    /// checker maps this to `TK2322`.
    Property {
        name: String,
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// Two function types have **different arity** (M3 has no optional/rest
    /// params, so arity must match exactly). `src`/`tgt` are the two function
    /// types. The checker maps this to `TK2322`.
    ParameterCount { src: TypeId, tgt: TypeId },
    /// A parameter is **contravariantly incompatible**: the target's parameter is
    /// not assignable to the source's parameter at position `index`, wrapping the
    /// inner reason (built in the contravariant `tgt_param → src_param`
    /// direction). `src`/`tgt` are the two function types. The checker maps this
    /// to `TK2322`.
    Parameter {
        index: usize,
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// The **return types are covariantly incompatible**: the source return type
    /// is not assignable to the target return type, wrapping the inner reason.
    /// `src`/`tgt` are the two function types. The checker maps this to `TK2322`.
    ReturnType {
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// A **union source has a member that is not assignable to the target** (a
    /// union `src <: tgt` requires *every* member to be assignable). `src` is the
    /// union, `tgt` the target; `member` is the first offending member, wrapping
    /// the inner reason for `member <: tgt`. The checker maps this to `TK2322`.
    UnionSourceMember {
        member: TypeId,
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// A **source is not assignable to any member of a union target** (a `src <:`
    /// union requires *some* member to accept it). `src` is the source, `tgt` the
    /// union target. No single member is "the cause", so this is a flat leaf-like
    /// reason over the whole union. The checker maps this to `TK2322`.
    NoUnionMember { src: TypeId, tgt: TypeId },
}

/// A non-empty chain of reasons explaining a relation failure, outermost first.
/// Built **only on the failing path** (architecture §6.4) so the success path
/// stays allocation-free.
#[derive(Clone, Debug)]
pub struct ReasonChain {
    pub head: Reason,
}

impl ReasonChain {
    fn leaf(src: TypeId, tgt: TypeId) -> ReasonChain {
        ReasonChain {
            head: Reason::Leaf { src, tgt },
        }
    }

    /// Wrap an arbitrary head reason (object-structural failures).
    fn of(head: Reason) -> ReasonChain {
        ReasonChain { head }
    }

    /// The outermost reason — the checker inspects its kind to pick the
    /// diagnostic code (missing property → `TK2741`, otherwise `TK2322`).
    pub fn head(&self) -> &Reason {
        &self.head
    }

    /// The (src, tgt) at the root of the failure — what the checker reports as
    /// the primary mismatch.
    pub fn root(&self) -> (TypeId, TypeId) {
        match &self.head {
            Reason::Leaf { src, tgt } => (*src, *tgt),
            Reason::MissingProperty { src, tgt, .. } => (*src, *tgt),
            Reason::Property { src, tgt, .. } => (*src, *tgt),
            Reason::ParameterCount { src, tgt } => (*src, *tgt),
            Reason::Parameter { src, tgt, .. } => (*src, *tgt),
            Reason::ReturnType { src, tgt, .. } => (*src, *tgt),
            Reason::UnionSourceMember { src, tgt, .. } => (*src, *tgt),
            Reason::NoUnionMember { src, tgt } => (*src, *tgt),
        }
    }
}

/// The result of a relation query. Never a bare `bool`: a failure carries its
/// cause so reporting mode (M6) can render nested "because…" messages.
#[derive(Clone, Debug)]
pub enum Relation {
    Yes,
    No(ReasonChain),
}

impl Relation {
    pub fn is_yes(&self) -> bool {
        matches!(self, Relation::Yes)
    }
}

/// The relation engine. Borrows the store immutably (relation checking never
/// mutates the arena) and owns the cache + cycle stack.
pub struct Relater<'a> {
    store: &'a Store,
    well_known: WellKnown,
    cache: RelationCache,
    /// Assume-true-until-disproven stack (architecture §6.3): when a query
    /// re-enters a relation already in flight, we assume it holds and continue,
    /// resolving the fixpoint as the stack unwinds. It fires as of M5, where
    /// recursive/mutually-recursive types (`interface List { tail: List | null }`)
    /// re-enter an in-flight key and rely on this to terminate.
    stack: FxHashSet<RelationKey>,
}

impl<'a> Relater<'a> {
    pub fn new(store: &'a Store, well_known: WellKnown) -> Self {
        Relater {
            store,
            well_known,
            cache: RelationCache::new(),
            stack: FxHashSet::default(),
        }
    }

    /// Is `src` assignable to `tgt`? Entry point used by the checker for
    /// annotation-vs-initializer checks (`TK2322`).
    pub fn is_assignable(&mut self, src: TypeId, tgt: TypeId) -> Relation {
        // The outermost frame has no enclosing assumptions; `assumed` collects any
        // assume-true dependencies its subtree consumes (see `relate`). Whatever
        // survives here would be an assumption about a key with no enclosing
        // frame — impossible by construction, so it is simply dropped.
        let mut assumed = FxHashSet::default();
        self.relate(src, tgt, RelationKind::Assignable, &mut assumed)
    }

    /// Core relation driver: cache + cycle stack around the structural rules.
    ///
    /// `assumed` is the **provisional-assumption channel** (architecture §6.3): it
    /// accumulates the in-flight keys this computation depended on via the
    /// assume-true short-circuit. A `Yes` that rests on an assumption about an
    /// **ancestor** key (one still on the stack above this frame) is *provisional* —
    /// sound only under that assumption — and must NOT be committed to the durable
    /// cache, or a later INDEPENDENT query would read a spurious `true` and drop a
    /// real error. Each frame discharges the assumption about its **own** key (the
    /// fixpoint resolves at the cycle root, so a verdict that depended only on
    /// re-entry to its own key is genuine) and propagates any remaining ancestor
    /// assumptions to its caller.
    fn relate(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        // Identity fast path: `T` relates to `T` under every relation.
        if src == tgt {
            return Relation::Yes;
        }

        let key = RelationKey::new(src, tgt, kind);

        // Cycle stack FIRST (architecture §6.3): re-entry on an in-flight relation
        // is assumed true and continues, resolving the fixpoint as the stack
        // unwinds. Checking the stack *before* the cache is what makes recursive
        // types terminate even on the rebuild path below: a relation cached as a
        // failure is recomputed (to rebuild its reason chain) **under** a stack
        // push, so a self-referential failure re-enters the same key, finds it in
        // flight, and terminates rather than recomputing forever (M5 — §6.3). The
        // assumed key is recorded so the caller's verdict is treated as provisional
        // until that key is discharged at its own root.
        if self.stack.contains(&key) {
            assumed.insert(key);
            return Relation::Yes;
        }

        // Cache: a previously-decided durable relation. Only **sound** verdicts are
        // ever stored (a genuine `false`, or a `true` that rested on no outstanding
        // assumption — see the commit below), so a cached hit is ground truth. A
        // cached success returns directly; a cached *failure* falls through to a
        // stack-guarded recompute so the checker still sees the precise
        // missing-vs-mismatch reason (the cache stores only the bool verdict —
        // architecture §6.1 — not the reason).
        let cached = self.cache.get(key);
        if cached == Some(true) {
            return Relation::Yes;
        }

        self.stack.insert(key);
        // This frame's own assumption accumulator. Children record the ancestor
        // keys (including, possibly, this frame's own key) they assumed true.
        let mut frame_assumed: FxHashSet<RelationKey> = FxHashSet::default();
        let result = self.relate_uncached(src, tgt, kind, &mut frame_assumed);
        self.stack.remove(&key);

        // Discharge the assumption about our OWN key: the fixpoint is resolved at
        // this root, so a dependency that was only on re-entry to `key` is genuine.
        frame_assumed.remove(&key);
        // Anything left is an assumption about a key still in flight ABOVE us — this
        // verdict is provisional. Surface those to the caller so its cacheability
        // accounts for them too.
        let provisional = !frame_assumed.is_empty();
        assumed.extend(frame_assumed.iter().copied());

        // Commit only sound verdicts on first decision:
        //   * a `false` is ALWAYS genuine — the assume-true rule only ever
        //     manufactures a spurious `true`, never a spurious `false`, so a `No`
        //     never depends on an assumption and is always cacheable;
        //   * a `true` is cacheable only when it rested on no outstanding ancestor
        //     assumption (otherwise it is provisional and would poison the cache).
        // A recompute of an already-cached failure must not re-insert.
        if cached.is_none() {
            if !result.is_yes() {
                self.cache.insert(key, false);
            } else if !provisional {
                self.cache.insert(key, true);
            }
        }
        result
    }

    /// The structural rules, run when the cache and cycle stack don't decide it.
    /// M0 scope: the intrinsic lattice and literal → base widening. M2 adds the
    /// object property-wise rule (width + depth). `assumed` is threaded through
    /// every recursive `relate` so provisional (assume-true) dependencies bubble up
    /// to the cache-commit site in [`Relater::relate`].
    fn relate_uncached(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        let wk = self.well_known;

        // `any` relates to everything in both directions (architecture §6,
        // mvp-plan §4.4). The error type behaves the same so cascades are
        // suppressed.
        if self.is_any_like(src) || self.is_any_like(tgt) {
            return Relation::Yes;
        }

        // Union rules (mvp-plan §6, M4) run BEFORE the intrinsic/object/function
        // rules. They are checked source-first, then target-first:
        //
        //  - if `src` is a union, `src <: tgt` iff **every** member is assignable
        //    to `tgt` (the union is at least as wide as any one member), and
        //  - otherwise, if `tgt` is a union, `src <: tgt` iff `src` is assignable
        //    to **some** member (it lands in one of the alternatives).
        //
        // Both fire only when the relevant side is a `Union` node; a union always
        // has ≥ 2 members (the interner collapses the degenerate cases), so these
        // never spuriously match an intrinsic. The `src`-union case is tried first
        // so a union-to-union relation decomposes member-by-member on the source.
        if self.store.tag(src) == TypeTag::Union {
            return self.relate_union_source(src, tgt, kind, assumed);
        }
        if self.store.tag(tgt) == TypeTag::Union {
            return self.relate_union_target(src, tgt, kind, assumed);
        }

        // `unknown` is the top type: everything is assignable TO it.
        if tgt == wk.unknown {
            return Relation::Yes;
        }

        // `never` is the bottom type: it is assignable to everything. (Nothing
        // is assignable *to* `never` except `never`, which the `src == tgt` fast
        // path already accepted.)
        if src == wk.never {
            return Relation::Yes;
        }

        // `void` accepts `undefined` (and `void` itself, via `src == tgt`).
        if tgt == wk.void && src == wk.undefined {
            return Relation::Yes;
        }

        // Literal → base widening: a literal is assignable to its base intrinsic
        // (`"x"` <: `string`, `1` <: `number`, `true` <: `boolean`). The literal
        // *type* is used for the decision; the message widens it (see the
        // renderer). M0 has no literal *targets*, so only this direction is
        // needed; M1 adds literal-to-literal once inference produces them.
        if let Some(lit) = self.store.literal_value(src) {
            let base = self.intrinsic_id(lit.base_kind());
            if base == tgt {
                return Relation::Yes;
            }
        }

        // Object structural rule (mvp-plan §6/§9, M2): `src` is assignable to
        // `tgt` iff every property of `tgt` is present in `src` with the src
        // property type assignable to the tgt property type. Width is allowed
        // (extra `src` props are fine); depth recurses. This is the only rule
        // that can fail with a *structured* (non-leaf) reason.
        if self.store.tag(src) == TypeTag::Object && self.store.tag(tgt) == TypeTag::Object {
            return self.relate_objects(src, tgt, kind, assumed);
        }

        // Function structural rule (mvp-plan §6.5, M3): parameters are
        // contravariant, the return is covariant, with matching arity.
        if self.store.tag(src) == TypeTag::Function && self.store.tag(tgt) == TypeTag::Function {
            return self.relate_functions(src, tgt, kind, assumed);
        }

        // Otherwise: not assignable. Build the leaf reason on this failing path.
        Relation::No(ReasonChain::leaf(src, tgt))
    }

    /// Property-wise object relation. Returns the **first** failing target
    /// property (in canonical order) as a precise reason: a `MissingProperty`
    /// when the source lacks a required target property, or a `Property` wrapping
    /// the nested reason when the property is present but its type does not
    /// relate. First-failure ordering keeps the verdict deterministic and matches
    /// how the M2 corpus pairs one cause per failing assignment.
    fn relate_objects(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        // Both ids are object-tagged here; the side-tables always resolve. The
        // `else` arms are defensive (an object tag without a payload is a store
        // invariant violation, never expected) and produce a leaf rather than
        // panicking.
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(src_obj) = self.store.object_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };

        for tgt_prop in &tgt_obj.properties {
            match src_obj.property(&tgt_prop.name) {
                // Width + depth: present in source — its type must relate to the
                // target property's type (recurse, so nested mismatches nest).
                Some(src_prop) => {
                    // M13 — nominal rule: a `private`/`protected` **target** member
                    // requires the source's same-named member to share its *origin*
                    // (same visibility AND same declaring class). This is what makes
                    // a class with a non-public member nominal: a structurally equal
                    // object literal (public, no origin) or another class's
                    // same-named non-public member does NOT satisfy it, so the
                    // relation FAILS here rather than reporting a depth mismatch.
                    // Soundness-critical: a non-matching origin must make the
                    // relation fail (no false negative). A public target member
                    // imposes no origin requirement (pure structural), so M0–M12
                    // objects are unaffected.
                    //
                    // The failure is a plain object-level `Leaf` (the member's *type*
                    // is fine — only its visibility/origin differs), so it maps to
                    // `TK2322` with no misleading "number is not assignable to
                    // number" depth elaboration. Object-target messages are asserted
                    // code-only in the corpus, so the headline form is unconstrained.
                    if !nominal_origin_ok(tgt_prop, src_prop) {
                        return Relation::No(ReasonChain::leaf(src, tgt));
                    }
                    if let Relation::No(child) =
                        self.relate(src_prop.ty, tgt_prop.ty, kind, assumed)
                    {
                        return Relation::No(ReasonChain::of(Reason::Property {
                            name: tgt_prop.name.clone(),
                            src,
                            tgt,
                            because: Box::new(child.head),
                        }));
                    }
                }
                // Required target property absent in the source.
                None => {
                    return Relation::No(ReasonChain::of(Reason::MissingProperty {
                        name: tgt_prop.name.clone(),
                        src,
                        tgt,
                    }));
                }
            }
        }

        Relation::Yes
    }

    /// Function assignability (mvp-plan §6.5, architecture §6.5). `src` is
    /// assignable to `tgt` iff:
    ///
    ///  - **arity is satisfiable** — the source takes **no more** parameters than
    ///    the target (a source with fewer parameters just ignores the extra
    ///    arguments; M3 has no optional/rest params, so a *surplus source*
    ///    parameter is the only arity failure), and
    ///  - each **parameter is contravariant** over the common positions
    ///    `0..src.len()` — the *target's* parameter type is assignable to the
    ///    *source's* parameter type (`tgt_param → src_param`); extra target
    ///    parameters are ignored, and
    ///  - the **return type is covariant** — the *source's* return type is
    ///    assignable to the *target's* return type (`src_ret → tgt_ret`), **except**
    ///    that a `void` target return accepts any source return type (a
    ///    value-returning function is assignable to a void-returning function
    ///    type; the value is discarded).
    ///
    /// typokat is contravariant on parameters everywhere (it picks soundness over
    /// tsc's method bivariance — methods are not in the MVP subset), matching tsc
    /// under `strictFunctionTypes` for function-typed values. The first failing
    /// obligation (arity, then parameters left-to-right, then return) is returned
    /// as a precise, nestable reason for M6.
    fn relate_functions(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        // Both ids are function-tagged here; the side-tables always resolve. The
        // `else` arms are defensive (a function tag without a payload is a store
        // invariant violation, never expected) and produce a leaf rather than
        // panicking.
        let Some(src_fn) = self.store.function_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_fn) = self.store.function_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };

        // Arity: a source with FEWER parameters than the target is fine — the
        // target's callers may pass extra arguments the source simply ignores
        // (`() => void` is assignable to `(x: number) => void`). It is a failure
        // only when the source needs MORE parameters than the target supplies
        // (M3 has no optional/rest params, so a surplus source parameter is
        // genuinely unsatisfiable). The contravariant check below runs over the
        // common positions `0..src.len()`; any extra *target* parameters are
        // ignored.
        if src_fn.params.len() > tgt_fn.params.len() {
            return Relation::No(ReasonChain::of(Reason::ParameterCount { src, tgt }));
        }

        // Collect the per-position parameter type ids and the return ids up front
        // so the immutable borrow of the store does not overlap the recursive
        // `self.relate` calls below (which also borrow the store). `zip` truncates
        // to the shorter (source) list, i.e. the common positions `0..src.len()`.
        let param_pairs: Vec<(TypeId, TypeId)> = src_fn
            .params
            .iter()
            .zip(&tgt_fn.params)
            .map(|(s, t)| (s.ty, t.ty))
            .collect();
        let (src_ret, tgt_ret) = (src_fn.ret, tgt_fn.ret);

        // Parameters: CONTRAVARIANT — the target parameter must be assignable to
        // the source parameter (`tgt_param → src_param`).
        for (index, (src_param, tgt_param)) in param_pairs.into_iter().enumerate() {
            if let Relation::No(child) = self.relate(tgt_param, src_param, kind, assumed) {
                return Relation::No(ReasonChain::of(Reason::Parameter {
                    index,
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }

        // Return: COVARIANT — the source return must be assignable to the target
        // return (`src_ret → tgt_ret`). Exception: a target return type of `void`
        // accepts **any** source return type — a value-returning function is
        // assignable to a void-returning function type, since the extra value is
        // simply discarded by the caller (`() => 1` is assignable to
        // `() => void`). This is the standard TS rule for void-returning
        // function-typed values.
        if tgt_ret != self.well_known.void {
            if let Relation::No(child) = self.relate(src_ret, tgt_ret, kind, assumed) {
                return Relation::No(ReasonChain::of(Reason::ReturnType {
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }

        Relation::Yes
    }

    /// Union **source** relation (mvp-plan §6, M4): a union `src` is assignable to
    /// `tgt` iff **every** member is. The first failing member (in canonical
    /// order) is wrapped as a `UnionSourceMember` reason carrying the member's own
    /// nested cause, so M6 can render "…because `<member>` is not assignable…".
    fn relate_union_source(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        // Snapshot the member ids so the immutable borrow of the store does not
        // overlap the recursive `self.relate` calls below (which also borrow it).
        // An ill-formed union tag without a side-table entry is a store invariant
        // violation; treat it defensively as a leaf rather than panicking.
        let Some(members) = self.store.union_members(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let members: Vec<TypeId> = members.to_vec();

        // "Every member relates" (AND): each member's assumptions genuinely
        // contribute to the union's Yes, so they all flow up through `assumed`.
        for member in members {
            if let Relation::No(child) = self.relate(member, tgt, kind, assumed) {
                return Relation::No(ReasonChain::of(Reason::UnionSourceMember {
                    member,
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }
        Relation::Yes
    }

    /// Union **target** relation (mvp-plan §6, M4): `src` is assignable to a union
    /// `tgt` iff it is assignable to **some** member. On failure no single member
    /// is "the cause", so a flat `NoUnionMember` reason over the whole union is
    /// returned (the per-member sub-failures are intentionally not retained).
    fn relate_union_target(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        // Snapshot the member ids (see `relate_union_source` for the borrow note).
        let Some(members) = self.store.union_members(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let members: Vec<TypeId> = members.to_vec();

        // "Some member relates" (OR): only the **accepted** member's assumptions
        // contribute to the union's Yes. A rejected member may have recorded
        // assume-true dependencies in its subtree that are irrelevant to the
        // verdict (we did not use that member), so each attempt gets its own local
        // accumulator and only the winning one is merged up. On overall failure no
        // assumptions are merged (a `No` is genuine — it never rests on one).
        for member in members {
            let mut member_assumed: FxHashSet<RelationKey> = FxHashSet::default();
            if self.relate(src, member, kind, &mut member_assumed).is_yes() {
                assumed.extend(member_assumed);
                return Relation::Yes;
            }
        }
        Relation::No(ReasonChain::of(Reason::NoUnionMember { src, tgt }))
    }

    /// `any` or the error type — both relate to everything.
    fn is_any_like(&self, id: TypeId) -> bool {
        if id == self.well_known.any || id == self.well_known.error {
            return true;
        }
        // Defensive: any type explicitly flagged as containing the error type.
        self.store.tag(id) == TypeTag::Intrinsic
            && matches!(
                self.store.intrinsic_kind(id),
                Some(IntrinsicKind::Any) | Some(IntrinsicKind::Error)
            )
    }

    /// The well-known id for an intrinsic kind.
    fn intrinsic_id(&self, kind: IntrinsicKind) -> TypeId {
        let wk = self.well_known;
        match kind {
            IntrinsicKind::Error => wk.error,
            IntrinsicKind::Any => wk.any,
            IntrinsicKind::Unknown => wk.unknown,
            IntrinsicKind::Never => wk.never,
            IntrinsicKind::Void => wk.void,
            IntrinsicKind::Null => wk.null,
            IntrinsicKind::Undefined => wk.undefined,
            IntrinsicKind::Boolean => wk.boolean,
            IntrinsicKind::Number => wk.number,
            IntrinsicKind::String => wk.string,
        }
    }
}

/// Whether the source member satisfies the target member's **nominal origin**
/// requirement (M13). A `public` target member imposes none (pure structural
/// width/depth). A `private`/`protected` target member requires the source's
/// same-named member to have the **same visibility AND the same declaring class**
/// — i.e. to be the *very same* declared member. This is the soundness-critical
/// gate: when it returns `false`, the relation must fail (a structurally-identical
/// object literal, or another class's same-named non-public member, is not
/// assignable to a class with a `private`/`protected` member).
fn nominal_origin_ok(tgt_prop: &PropertyType, src_prop: &PropertyType) -> bool {
    match tgt_prop.visibility {
        // Public target member: structural only — no origin constraint.
        Visibility::Public => true,
        // Non-public target member: the source member must share its exact origin
        // (same visibility and same declaring class). A class's own instances pass
        // trivially (the identity fast path in `relate` short-circuits same-id
        // types before this is ever reached); everything else must match here.
        Visibility::Private | Visibility::Protected => {
            src_prop.visibility == tgt_prop.visibility
                && src_prop.declaring_class == tgt_prop.declaring_class
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{ClassId, LiteralValue, ObjectType, PropertyType};
    use crate::types::Interner;

    fn prop(name: &str, ty: TypeId) -> PropertyType {
        PropertyType::public(name, ty)
    }

    /// Build a member `name: ty` with an explicit visibility + declaring class
    /// (M13), for the nominal-relation tests.
    fn nominal_prop(
        name: &str,
        ty: TypeId,
        visibility: Visibility,
        declaring_class: Option<ClassId>,
    ) -> PropertyType {
        PropertyType {
            name: name.to_string(),
            ty,
            optional: false,
            visibility,
            declaring_class,
            readonly: false,
            is_accessor: false,
        }
    }

    /// The M2 object structural rule: width (extra src props ok), depth (prop
    /// types checked, recursing), and the precise missing-vs-mismatch reason.
    #[test]
    fn object_width_depth_and_reason_kinds() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // { a: number; b: string }
        let ab = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number), prop("b", wk.string)],
        });
        // { a: number } — a width-narrower target.
        let a_only = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number)],
        });
        // { a: string } — same key, incompatible type.
        let a_str = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.string)],
        });

        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        // Width: { a; b } is assignable to { a } (extra `b` ignored).
        assert!(rel.is_assignable(ab, a_only).is_yes());
        // Exact identity short-circuits.
        assert!(rel.is_assignable(ab, ab).is_yes());

        // Missing required property: { a } is NOT assignable to { a; b }.
        match rel.is_assignable(a_only, ab) {
            Relation::No(chain) => match chain.head() {
                Reason::MissingProperty { name, .. } => assert_eq!(name, "b"),
                other => panic!("expected MissingProperty, got {other:?}"),
            },
            Relation::Yes => panic!("expected a missing-property failure"),
        }

        // Depth mismatch: { a: number } is NOT assignable to { a: string }.
        match rel.is_assignable(a_only, a_str) {
            Relation::No(chain) => match chain.head() {
                Reason::Property { name, because, .. } => {
                    assert_eq!(name, "a");
                    assert!(matches!(**because, Reason::Leaf { .. }));
                }
                other => panic!("expected Property, got {other:?}"),
            },
            Relation::Yes => panic!("expected a depth mismatch failure"),
        }
    }

    /// M13 — nominal class typing via a `private`/`protected` member. A
    /// `private`/`protected` **target** member breaks pure structural
    /// assignability: a structurally-identical public object is NOT assignable, and
    /// a same-named non-public member from a *different* declaring class is NOT
    /// assignable; only the class's own instances (same interned id) are. This pins
    /// the soundness-critical rule (a non-matching origin must FAIL the relation).
    #[test]
    fn nominal_private_member_breaks_structural_assignability() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let secret = ClassId(0);
        let other = ClassId(1);

        // `class Secret { private x: number }` (its instance type, nominal object).
        let secret_ty = interner.intern_object(ObjectType {
            properties: vec![nominal_prop(
                "x",
                wk.number,
                Visibility::Private,
                Some(secret),
            )],
        });
        // A structurally identical *public* object literal `{ x: number }`.
        let public_obj = interner.intern_object(ObjectType {
            properties: vec![prop("x", wk.number)],
        });
        // `class Other { private x: number }` — same shape, DIFFERENT origin.
        let other_ty = interner.intern_object(ObjectType {
            properties: vec![nominal_prop(
                "x",
                wk.number,
                Visibility::Private,
                Some(other),
            )],
        });

        // The three are distinct interned ids (origin/visibility are part of
        // identity), so the relation cache keys them apart.
        assert_ne!(secret_ty, public_obj, "private member ⇒ distinct from public");
        assert_ne!(secret_ty, other_ty, "different declaring class ⇒ distinct");

        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        // Same class (same id): assignable via the identity fast path.
        assert!(
            rel.is_assignable(secret_ty, secret_ty).is_yes(),
            "a class's own instance type is assignable to itself"
        );

        // Public `{ x: number }` is NOT assignable to `Secret` (the private member
        // has no public counterpart) — an object-level `Leaf` failure (the member
        // type is fine; only the visibility/origin differs).
        match rel.is_assignable(public_obj, secret_ty) {
            Relation::No(chain) => {
                assert!(
                    matches!(chain.head(), Reason::Leaf { .. }),
                    "expected an object-level Leaf failure, got {:?}",
                    chain.head()
                );
                // The root pins the two object types.
                assert_eq!(chain.root(), (public_obj, secret_ty));
            }
            Relation::Yes => panic!("a public object must NOT be assignable to a private-member class"),
        }

        // `Other` (different origin) is NOT assignable to `Secret`.
        assert!(
            !rel.is_assignable(other_ty, secret_ty).is_yes(),
            "a different class's private member must not satisfy Secret's"
        );

        // Symmetry: `Secret` is also not assignable to `Other` (origin differs).
        assert!(
            !rel.is_assignable(secret_ty, other_ty).is_yes(),
            "Secret's private member must not satisfy Other's"
        );
    }

    /// M13 — a `protected` target member is nominal exactly like `private`: a
    /// structurally-identical public object is not assignable, while the class's own
    /// instance is. Separated from the `private` test so each uses a clean interner.
    #[test]
    fn nominal_protected_member_breaks_structural_assignability() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let owner = ClassId(0);

        let prot_ty = interner.intern_object(ObjectType {
            properties: vec![nominal_prop(
                "owner",
                wk.string,
                Visibility::Protected,
                Some(owner),
            )],
        });
        let public_obj = interner.intern_object(ObjectType {
            properties: vec![prop("owner", wk.string)],
        });

        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        assert!(rel.is_assignable(prot_ty, prot_ty).is_yes(), "own instance ok");
        assert!(
            !rel.is_assignable(public_obj, prot_ty).is_yes(),
            "a public object must NOT be assignable to a protected-member class"
        );
    }

    /// M14 — `readonly` is part of a member's structural identity (a `{ readonly x }`
    /// interns to a *distinct* id from `{ x }`), but it must **NOT** affect
    /// assignability: a readonly-bearing object and a mutable one relate **both ways**.
    /// The relation engine deliberately ignores the flag (it gates assignment targets
    /// only); this pins that it is neither added to the nominal-origin gate nor the
    /// structural depth check.
    #[test]
    fn readonly_does_not_affect_assignability() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // `{ readonly x: number }` and `{ x: number }`.
        let readonly_obj = interner.intern_object(ObjectType {
            properties: vec![PropertyType {
                name: "x".to_string(),
                ty: wk.number,
                optional: false,
                visibility: Visibility::Public,
                declaring_class: None,
                readonly: true,
                is_accessor: false,
            }],
        });
        let mutable_obj = interner.intern_object(ObjectType {
            properties: vec![prop("x", wk.number)],
        });

        // The flag is part of identity, so the two ids differ...
        assert_ne!(
            readonly_obj, mutable_obj,
            "`readonly` is part of structural identity ⇒ distinct interned ids"
        );

        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        // ...yet they relate freely in BOTH directions (readonly ignored for relation).
        assert!(
            rel.is_assignable(readonly_obj, mutable_obj).is_yes(),
            "{{ readonly x }} must be assignable to {{ x }}"
        );
        assert!(
            rel.is_assignable(mutable_obj, readonly_obj).is_yes(),
            "{{ x }} must be assignable to {{ readonly x }}"
        );
    }

    /// M15 — `is_accessor` mirrors `readonly`: it is part of a member's structural
    /// identity (so a get-only-accessor property and a same-typed `readonly` data field
    /// are distinct interned ids) but is **ignored** by the relation engine — an accessor
    /// property and a plain field relate freely, both ways. This pins that the assignment
    /// distinction (accessor read-only everywhere vs. field assignable in its ctor) lives
    /// purely in the checker, never leaking into assignability.
    #[test]
    fn is_accessor_does_not_affect_assignability() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // A get-only accessor models as `readonly: true, is_accessor: true`.
        let accessor_obj = interner.intern_object(ObjectType {
            properties: vec![PropertyType {
                name: "x".to_string(),
                ty: wk.number,
                optional: false,
                visibility: Visibility::Public,
                declaring_class: None,
                readonly: true,
                is_accessor: true,
            }],
        });
        // A `readonly` data field: same shape but `is_accessor: false`.
        let readonly_field_obj = interner.intern_object(ObjectType {
            properties: vec![PropertyType {
                name: "x".to_string(),
                ty: wk.number,
                optional: false,
                visibility: Visibility::Public,
                declaring_class: None,
                readonly: true,
                is_accessor: false,
            }],
        });
        let mutable_obj = interner.intern_object(ObjectType {
            properties: vec![prop("x", wk.number)],
        });

        // `is_accessor` is part of identity, so the accessor object differs from the
        // same-shape `readonly` field object (and from a plain field object).
        assert_ne!(
            accessor_obj, readonly_field_obj,
            "`is_accessor` is part of structural identity ⇒ distinct interned ids"
        );

        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        // ...yet the accessor relates freely with a plain field, both directions.
        assert!(
            rel.is_assignable(accessor_obj, mutable_obj).is_yes(),
            "accessor `{{ x }}` must be assignable to field `{{ x }}`"
        );
        assert!(
            rel.is_assignable(mutable_obj, accessor_obj).is_yes(),
            "field `{{ x }}` must be assignable to accessor `{{ x }}`"
        );
    }

    /// Nested depth: a mismatch one level deep nests under the outer property
    /// (the chain M6 renders).
    #[test]
    fn object_nested_depth_reason_nests() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // inner targets: { b: number } vs source { b: string }
        let inner_num = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.number)],
        });
        let inner_str = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.string)],
        });
        let outer_src = interner.intern_object(ObjectType {
            properties: vec![prop("a", inner_str)],
        });
        let outer_tgt = interner.intern_object(ObjectType {
            properties: vec![prop("a", inner_num)],
        });

        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        match rel.is_assignable(outer_src, outer_tgt) {
            Relation::No(chain) => match chain.head() {
                Reason::Property { name, because, .. } => {
                    assert_eq!(name, "a");
                    // Inner reason is the property `b` mismatch.
                    match &**because {
                        Reason::Property { name, .. } => assert_eq!(name, "b"),
                        other => panic!("expected nested Property, got {other:?}"),
                    }
                }
                other => panic!("expected outer Property, got {other:?}"),
            },
            Relation::Yes => panic!("expected nested depth failure"),
        }
    }

    /// M3 function assignability: parameters CONTRAVARIANT (over the common
    /// positions), return COVARIANT, fewer-source-params allowed, surplus-source-
    /// params rejected, and a `void` target return accepting any source return
    /// (mvp-plan §6.5). Exercised independently of the parser so a
    /// variance/arity/void regression is caught here.
    #[test]
    fn function_variance_arity_and_void_return() {
        use crate::types::repr::{FunctionType, ParameterType};

        fn param(name: &str, ty: TypeId) -> ParameterType {
            ParameterType {
                name: name.to_string(),
                ty,
                optional: false,
            }
        }

        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // Reference: `(x: number) => number`.
        let num_to_num = interner.intern_function(FunctionType {
            params: vec![param("x", wk.number)],
            ret: wk.number,
        });
        // `(x: unknown) => number`.
        let unknown_to_num = interner.intern_function(FunctionType {
            params: vec![param("x", wk.unknown)],
            ret: wk.number,
        });
        // `(x: string) => number`.
        let str_to_num = interner.intern_function(FunctionType {
            params: vec![param("x", wk.string)],
            ret: wk.number,
        });
        // `() => number` (FEWER params than `num_to_num`).
        let nullary_to_num = interner.intern_function(FunctionType {
            params: vec![],
            ret: wk.number,
        });
        // `(x: number) => string` (incompatible return).
        let num_to_str = interner.intern_function(FunctionType {
            params: vec![param("x", wk.number)],
            ret: wk.string,
        });
        // `(x: number, y: number) => number` (MORE params than `num_to_num`).
        let two_to_num = interner.intern_function(FunctionType {
            params: vec![param("x", wk.number), param("y", wk.number)],
            ret: wk.number,
        });
        // `() => void` and `() => number` for the void-return rule.
        let nullary_to_void = interner.intern_function(FunctionType {
            params: vec![],
            ret: wk.void,
        });
        let nullary_to_num_only = interner.intern_function(FunctionType {
            params: vec![],
            ret: wk.number,
        });

        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        // CONTRAVARIANT params: `(x: unknown) => number` IS assignable to
        // `(x: number) => number`, because the target param `number` is
        // assignable to the source param `unknown` (tgt → src).
        assert!(
            rel.is_assignable(unknown_to_num, num_to_num).is_yes(),
            "contravariant: wider param (unknown) accepts a narrower target param (number)"
        );

        // `(x: string) => number` is NOT assignable to `(x: number) => number`:
        // the target param `number` is not assignable to the source param
        // `string`.
        match rel.is_assignable(str_to_num, num_to_num) {
            Relation::No(chain) => match chain.head() {
                Reason::Parameter { index, because, .. } => {
                    assert_eq!(*index, 0);
                    // The contravariant child compares `number` (tgt) → `string`
                    // (src) and fails as a leaf.
                    assert!(matches!(**because, Reason::Leaf { .. }));
                }
                other => panic!("expected a Parameter reason, got {other:?}"),
            },
            Relation::Yes => panic!("expected a contravariant parameter failure"),
        }

        // COVARIANT return: `(x: number) => string` is NOT assignable to
        // `(x: number) => number` — the source return `string` is not assignable
        // to the target return `number`.
        match rel.is_assignable(num_to_str, num_to_num) {
            Relation::No(chain) => match chain.head() {
                Reason::ReturnType { because, .. } => {
                    assert!(matches!(**because, Reason::Leaf { .. }));
                }
                other => panic!("expected a ReturnType reason, got {other:?}"),
            },
            Relation::Yes => panic!("expected a covariant return failure"),
        }

        // FEWER source params: `() => number` IS assignable to
        // `(x: number) => number` — the source ignores the extra argument.
        assert!(
            rel.is_assignable(nullary_to_num, num_to_num).is_yes(),
            "a source with fewer parameters is assignable (extra args ignored)"
        );

        // MORE source params: `(x: number, y: number) => number` is NOT assignable
        // to `(x: number) => number` — the target cannot supply the surplus
        // parameter.
        match rel.is_assignable(two_to_num, num_to_num) {
            Relation::No(chain) => {
                assert!(matches!(chain.head(), Reason::ParameterCount { .. }));
            }
            Relation::Yes => panic!("expected a surplus-source-parameter arity failure"),
        }

        // VOID target return: `() => number` IS assignable to `() => void` — the
        // returned value is discarded.
        assert!(
            rel.is_assignable(nullary_to_num_only, nullary_to_void).is_yes(),
            "a value-returning function is assignable to a void-returning function type"
        );
        // `() => void` is assignable to itself (identity), and a void source is
        // fine for a void target.
        assert!(rel.is_assignable(nullary_to_void, nullary_to_void).is_yes());

        // Identity short-circuits.
        assert!(rel.is_assignable(num_to_num, num_to_num).is_yes());
    }

    /// Exhaustively check the M0 intrinsic-lattice + literal-widening rules so a
    /// regression in the relation engine is caught independent of the parser and
    /// the fixtures.
    #[test]
    fn intrinsic_lattice_and_widening() {
        let mut interner = Interner::with_intrinsics();
        // Literal sources used by the M0 fixtures.
        let lit_num = interner.intern_literal(LiteralValue::Number(1.0));
        let lit_str = interner.intern_literal(LiteralValue::String("x".to_string()));
        let lit_bool = interner.intern_literal(LiteralValue::Boolean(true));

        let wk = interner.well_known();
        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        // Literal -> base widening (assignability uses the literal type).
        assert!(rel.is_assignable(lit_num, wk.number).is_yes());
        assert!(rel.is_assignable(lit_str, wk.string).is_yes());
        assert!(rel.is_assignable(lit_bool, wk.boolean).is_yes());
        // Cross-base widening fails.
        assert!(!rel.is_assignable(lit_str, wk.number).is_yes());
        assert!(!rel.is_assignable(lit_num, wk.string).is_yes());
        assert!(!rel.is_assignable(lit_num, wk.boolean).is_yes());
        assert!(!rel.is_assignable(lit_bool, wk.number).is_yes());

        // any: assignable both directions.
        assert!(rel.is_assignable(lit_num, wk.any).is_yes());
        assert!(rel.is_assignable(wk.any, wk.number).is_yes());

        // unknown: top type. Everything -> unknown; unknown -> only unknown/any.
        assert!(rel.is_assignable(lit_num, wk.unknown).is_yes());
        assert!(rel.is_assignable(wk.unknown, wk.unknown).is_yes());
        assert!(rel.is_assignable(wk.unknown, wk.any).is_yes());
        assert!(!rel.is_assignable(wk.unknown, wk.number).is_yes());

        // never: bottom type. never -> anything; nothing -> never except never.
        assert!(rel.is_assignable(wk.never, wk.number).is_yes());
        assert!(rel.is_assignable(wk.never, wk.never).is_yes());
        assert!(!rel.is_assignable(lit_num, wk.never).is_yes());

        // void: accepts undefined and itself.
        assert!(rel.is_assignable(wk.undefined, wk.void).is_yes());
        assert!(rel.is_assignable(wk.void, wk.void).is_yes());
        assert!(!rel.is_assignable(lit_num, wk.void).is_yes());

        // strictNullChecks: null/undefined distinct, each only to self/any/unknown
        // (undefined also to void).
        assert!(rel.is_assignable(wk.null, wk.null).is_yes());
        assert!(rel.is_assignable(wk.undefined, wk.undefined).is_yes());
        assert!(!rel.is_assignable(wk.null, wk.number).is_yes());
        assert!(!rel.is_assignable(wk.undefined, wk.string).is_yes());
        assert!(!rel.is_assignable(wk.undefined, wk.null).is_yes());
        assert!(!rel.is_assignable(wk.null, wk.undefined).is_yes());
    }

    /// A failure returns a reason chain whose root is the (src, tgt) pair — the
    /// hook M6 grows into nested messages, and the data M0's renderer consumes.
    #[test]
    fn failure_carries_reason_root() {
        let mut interner = Interner::with_intrinsics();
        let lit_str = interner.intern_literal(LiteralValue::String("x".to_string()));
        let wk = interner.well_known();
        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        match rel.is_assignable(lit_str, wk.number) {
            Relation::No(chain) => {
                assert_eq!(chain.root(), (lit_str, wk.number));
            }
            Relation::Yes => panic!("expected a relation failure"),
        }
    }

    /// The cache returns a stable verdict on repeat queries (smoke test of the
    /// 3-`u32` cache path; the cycle stack never fires for non-recursive types).
    #[test]
    fn repeated_query_is_stable() {
        let mut interner = Interner::with_intrinsics();
        let lit_num = interner.intern_literal(LiteralValue::Number(1.0));
        let wk = interner.well_known();
        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        let first = rel.is_assignable(lit_num, wk.string).is_yes();
        let second = rel.is_assignable(lit_num, wk.string).is_yes();
        assert_eq!(first, second);
        assert!(!first);
    }

    /// M5 — relating recursive types **terminates** via the assume-true cycle
    /// stack (§6.3). This guards the cycle fixpoint: a stack overflow here is the
    /// failure mode the recursive-type fixture must never hit. It covers the three
    /// paths that each loop forever without the fix:
    ///
    ///  1. a recursive interface relating to **itself**,
    ///  2. a recursive interface relating to a **structural copy** of itself,
    ///  3. a **failing** recursive relation queried **twice** — the second query
    ///     hits the cached-false rebuild path, which must recompute under stack
    ///     protection rather than recurse on the cached failure forever.
    #[test]
    fn recursive_relation_terminates() {
        use crate::types::repr::ObjectType;

        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // `interface List { head: number; tail: List | null }` — a nominal,
        // self-referential object built reserve-then-fill (its `tail` references
        // its own id, never an inlined expansion).
        let list = interner.reserve_object();
        let list_or_null = interner.union(vec![list, wk.null]);
        interner.fill_object(
            list,
            ObjectType {
                properties: vec![prop("head", wk.number), prop("tail", list_or_null)],
            },
        );

        // A *structural* copy with the same shape but its own id (built the same
        // way so it too is self-referential).
        let copy = interner.reserve_object();
        let copy_or_null = interner.union(vec![copy, wk.null]);
        interner.fill_object(
            copy,
            ObjectType {
                properties: vec![prop("head", wk.number), prop("tail", copy_or_null)],
            },
        );

        // Two mutually-shaped recursive interfaces that DISAGREE at a leaf:
        // `A { self: A; tag: number }` vs `B { self: B; tag: string }`. Relating
        // `A <: B` re-enters `(A, B)` via `self` (assumed true) but fails on `tag`.
        let a = interner.reserve_object();
        let b = interner.reserve_object();
        interner.fill_object(
            a,
            ObjectType {
                properties: vec![prop("self", a), prop("tag", wk.number)],
            },
        );
        interner.fill_object(
            b,
            ObjectType {
                properties: vec![prop("self", b), prop("tag", wk.string)],
            },
        );

        let store = interner.store();
        let mut rel = Relater::new(store, wk);

        // 1. Recursive interface relates to itself (identity short-circuit, but the
        //    union member `List | null` still gets relate'd through the cycle for
        //    `List <: List | null`).
        assert!(rel.is_assignable(list, list).is_yes(), "List <: List");
        assert!(
            rel.is_assignable(list_or_null, list_or_null).is_yes(),
            "List | null <: List | null"
        );

        // 2. Recursive interface relates to a structural copy — must terminate and
        //    succeed (each side's `tail` re-enters the in-flight `(List, copy)`).
        assert!(
            rel.is_assignable(list, copy).is_yes(),
            "List <: structural copy must terminate as success"
        );
        assert!(
            rel.is_assignable(copy, list).is_yes(),
            "structural copy <: List must terminate as success"
        );

        // 3. The failing recursive relation, queried TWICE. The first decides
        //    false (and caches the bool); the second recomputes the reason on the
        //    cached-false path — which must run under stack protection and
        //    terminate, not loop. The leaf cause is the `tag` mismatch.
        let first = rel.is_assignable(a, b);
        assert!(!first.is_yes(), "A is not assignable to B (tag mismatch)");
        let second = rel.is_assignable(a, b);
        assert!(
            !second.is_yes(),
            "repeat query of a failing recursive relation must terminate with the same verdict"
        );
        // The rebuilt reason still points at the offending `tag` property.
        match second {
            Relation::No(chain) => match chain.head() {
                Reason::Property { name, .. } => assert_eq!(name, "tag"),
                other => panic!("expected the `tag` Property failure, got {other:?}"),
            },
            Relation::Yes => unreachable!(),
        }
    }

    /// M5 soundness — a recursive **false** verdict must be **order-independent**:
    /// it must not depend on whether an enclosing assume-true query ran first
    /// (architecture §6.3). This is the cache-poisoning hazard: relating `CA <: AA`
    /// decides the nested `CB <: AB` *provisionally* true under the in-flight
    /// `(CA, AA)` assumption; if that provisional `true` were committed to the
    /// durable cache, a later INDEPENDENT `CB <: AB` query would read it as ground
    /// truth and drop a real error. The fix never caches a verdict that rested on an
    /// assumption about an ancestor key, so the standalone verdict is identical
    /// either way.
    #[test]
    fn recursive_false_verdict_is_order_independent() {
        use crate::types::repr::ObjectType;

        // Build the four mutually-recursive interfaces once; reused for both orders.
        // AA { peer: AB; tag: number }   AB { back: AA; leaf: number }
        // CA { peer: CB; tag: string }   CB { back: CA; leaf: number }  (tag differs)
        fn build(interner: &mut Interner) -> (TypeId, TypeId, TypeId, TypeId) {
            let wk = interner.well_known();
            let (aa, ab, ca, cb) = (
                interner.reserve_object(),
                interner.reserve_object(),
                interner.reserve_object(),
                interner.reserve_object(),
            );
            interner.fill_object(
                aa,
                ObjectType {
                    properties: vec![prop("peer", ab), prop("tag", wk.number)],
                },
            );
            interner.fill_object(
                ab,
                ObjectType {
                    properties: vec![prop("back", aa), prop("leaf", wk.number)],
                },
            );
            interner.fill_object(
                ca,
                ObjectType {
                    properties: vec![prop("peer", cb), prop("tag", wk.string)],
                },
            );
            interner.fill_object(
                cb,
                ObjectType {
                    properties: vec![prop("back", ca), prop("leaf", wk.number)],
                },
            );
            (aa, ab, ca, cb)
        }

        // Order A: query `CA <: AA` FIRST (which provisionally relates `CB <: AB`
        // under the `(CA, AA)` assumption), THEN the standalone `CB <: AB`.
        let order_a_cb_ab = {
            let mut interner = Interner::with_intrinsics();
            let wk = interner.well_known();
            let (aa, ab, ca, cb) = build(&mut interner);
            let store = interner.store();
            let mut rel = Relater::new(store, wk);
            // The enclosing query: genuinely false (top-level `tag` mismatch).
            assert!(!rel.is_assignable(ca, aa).is_yes(), "CA <: AA is false (tag)");
            // The standalone nested query MUST still be false.
            rel.is_assignable(cb, ab).is_yes()
        };

        // Order B: the standalone `CB <: AB` query alone, no enclosing query.
        let order_b_cb_ab = {
            let mut interner = Interner::with_intrinsics();
            let wk = interner.well_known();
            let (_aa, ab, _ca, cb) = build(&mut interner);
            let store = interner.store();
            let mut rel = Relater::new(store, wk);
            rel.is_assignable(cb, ab).is_yes()
        };

        // The verdict is the same either way, and it is FALSE: `CB <: AB` requires
        // `CB.back (CA) <: AB.back (AA)`, which fails on the `tag` leaf.
        assert_eq!(
            order_a_cb_ab, order_b_cb_ab,
            "a recursive false verdict must not depend on an enclosing assume-true query"
        );
        assert!(
            !order_b_cb_ab,
            "CB is not assignable to AB (the recursive `tag` mismatch must be reported)"
        );
    }
}
