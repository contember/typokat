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
use crate::types::repr::{IntrinsicKind, TypeTag};
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
/// `Leaf { src, tgt }`, but the structure is recursive so M2+ can nest
/// "...because property `p`: <child reason>" (architecture §6.4).
#[derive(Clone, Debug)]
pub enum Reason {
    /// The base mismatch: `src` is not assignable to `tgt`.
    Leaf { src: TypeId, tgt: TypeId },
    /// A mismatch located at a named property, wrapping the inner reason.
    /// TODO(M2/M6): produced when an object property fails to relate.
    #[allow(dead_code)]
    Property {
        name: String,
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
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

    /// The (src, tgt) at the root of the failure — what the checker reports as
    /// the primary mismatch.
    pub fn root(&self) -> (TypeId, TypeId) {
        match &self.head {
            Reason::Leaf { src, tgt } => (*src, *tgt),
            Reason::Property { src, tgt, .. } => (*src, *tgt),
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
    /// resolving the fixpoint as the stack unwinds. M0 has no recursive types so
    /// this never fires, but it is wired in from day 1.
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
        self.relate(src, tgt, RelationKind::Assignable)
    }

    /// Core relation driver: cache + cycle stack around the structural rules.
    fn relate(&mut self, src: TypeId, tgt: TypeId, kind: RelationKind) -> Relation {
        // Identity fast path: `T` relates to `T` under every relation.
        if src == tgt {
            return Relation::Yes;
        }

        let key = RelationKey::new(src, tgt, kind);

        // Cache: a previously-decided durable relation.
        if let Some(result) = self.cache.get(key) {
            return self.materialize(result, src, tgt);
        }

        // Cycle stack: re-entry on an in-flight relation is assumed true
        // (architecture §6.3). Resolved at the end of the outermost call.
        if self.stack.contains(&key) {
            return Relation::Yes;
        }
        self.stack.insert(key);

        let result = self.relate_uncached(src, tgt, kind);

        self.stack.remove(&key);
        // Cache the boolean verdict; the reason chain is recomputed cheaply on
        // the rare repeated-failure path via `materialize`.
        self.cache.insert(key, result.is_yes());
        result
    }

    /// Reconstruct a `Relation` from a cached boolean verdict. A cached failure
    /// rebuilds a leaf reason for `(src, tgt)` (M0 reasons are leaves; M2+ will
    /// extend caching to carry richer reasons if profiling warrants).
    fn materialize(&self, verdict: bool, src: TypeId, tgt: TypeId) -> Relation {
        if verdict {
            Relation::Yes
        } else {
            Relation::No(ReasonChain::leaf(src, tgt))
        }
    }

    /// The structural rules, run when the cache and cycle stack don't decide it.
    /// M0 scope: the intrinsic lattice and literal → base widening.
    fn relate_uncached(&mut self, src: TypeId, tgt: TypeId, _kind: RelationKind) -> Relation {
        let wk = self.well_known;

        // `any` relates to everything in both directions (architecture §6,
        // mvp-plan §4.4). The error type behaves the same so cascades are
        // suppressed.
        if self.is_any_like(src) || self.is_any_like(tgt) {
            return Relation::Yes;
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

        // Otherwise: not assignable. Build the leaf reason on this failing path.
        Relation::No(ReasonChain::leaf(src, tgt))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::LiteralValue;
    use crate::types::Interner;

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
}
