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

        // Cache: a previously-decided durable relation. A cached success is
        // returned directly; a cached *failure* recomputes the reason chain on
        // the (rare) repeated-failure path so the checker still sees the precise
        // missing-vs-mismatch cause — the cache stores only the bool verdict
        // (architecture §6.1), not the reason.
        if let Some(verdict) = self.cache.get(key) {
            if verdict {
                return Relation::Yes;
            }
            return self.relate_uncached(src, tgt, kind);
        }

        // Cycle stack: re-entry on an in-flight relation is assumed true
        // (architecture §6.3). Resolved at the end of the outermost call.
        if self.stack.contains(&key) {
            return Relation::Yes;
        }
        self.stack.insert(key);

        let result = self.relate_uncached(src, tgt, kind);

        self.stack.remove(&key);
        // Cache the boolean verdict only; the reason chain is rebuilt cheaply on
        // the rare repeated-failure path (see the cache-hit branch above).
        self.cache.insert(key, result.is_yes());
        result
    }

    /// The structural rules, run when the cache and cycle stack don't decide it.
    /// M0 scope: the intrinsic lattice and literal → base widening. M2 adds the
    /// object property-wise rule (width + depth).
    fn relate_uncached(&mut self, src: TypeId, tgt: TypeId, kind: RelationKind) -> Relation {
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

        // Object structural rule (mvp-plan §6/§9, M2): `src` is assignable to
        // `tgt` iff every property of `tgt` is present in `src` with the src
        // property type assignable to the tgt property type. Width is allowed
        // (extra `src` props are fine); depth recurses. This is the only rule
        // that can fail with a *structured* (non-leaf) reason.
        if self.store.tag(src) == TypeTag::Object && self.store.tag(tgt) == TypeTag::Object {
            return self.relate_objects(src, tgt, kind);
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
    fn relate_objects(&mut self, src: TypeId, tgt: TypeId, kind: RelationKind) -> Relation {
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
                    if let Relation::No(child) = self.relate(src_prop.ty, tgt_prop.ty, kind) {
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
    use crate::types::repr::{LiteralValue, ObjectType, PropertyType};
    use crate::types::Interner;

    fn prop(name: &str, ty: TypeId) -> PropertyType {
        PropertyType {
            name: name.to_string(),
            ty,
            optional: false,
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
