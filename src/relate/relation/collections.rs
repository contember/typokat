use super::*;

impl<'a> Relater<'a> {
    /// Array assignability is covariant in the element type (M17). The element
    /// relation uses ordinary [`Relater::relate`] so cache/cycle invariants and
    /// nested-array reasons are preserved.
    pub(super) fn relate_arrays(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        // Both ids are array-tagged here; the side-tables always resolve. The `else`
        // arms are defensive (an array tag without a payload is a store invariant
        // violation, never expected) and produce a leaf rather than panicking.
        let Some(src_arr) = self.store.array_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_arr) = self.store.array_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let (src_elem, tgt_elem) = (src_arr.element, tgt_arr.element);

        // Covariant: source element must be assignable to target element.
        if let Relation::No(child) = self.relate(src_elem, tgt_elem, kind, assumed) {
            return Relation::No(ReasonChain::of(Reason::ArrayElement {
                src,
                tgt,
                because: Box::new(child.head),
            }));
        }
        Relation::Yes
    }

    /// Tuple assignability is same-length and positional (M18). Length mismatch is
    /// terminal; the first failing element wraps its nested cause, and element
    /// relations use ordinary cache/cycle-safe [`Relater::relate`].
    pub(super) fn relate_tuples(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        // Both ids are tuple-tagged here; the side-tables always resolve. The `else`
        // arms are defensive (a tuple tag without a payload is a store invariant
        // violation, never expected) and produce a leaf rather than panicking.
        let Some(src_tuple) = self.store.tuple_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_tuple) = self.store.tuple_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };

        // Length must match exactly — tuples are fixed-length and positional (M18
        // has no optional/rest elements, so a different arity is unsatisfiable).
        if src_tuple.elements.len() != tgt_tuple.elements.len() {
            return Relation::No(ReasonChain::of(Reason::TupleLength { src, tgt }));
        }

        // Snapshot the per-position element ids up front so the immutable borrow of
        // the store does not overlap the recursive `self.relate` calls below (which
        // also borrow it). `zip` pairs equal-length lists element-by-element.
        let element_pairs: Vec<(TypeId, TypeId)> = src_tuple
            .elements
            .iter()
            .zip(&tgt_tuple.elements)
            .map(|(&s, &t)| (s, t))
            .collect();

        // Positional: each source element must be assignable to the target element at
        // the same position. First failing position wins (single nested reason).
        for (index, (src_elem, tgt_elem)) in element_pairs.into_iter().enumerate() {
            if let Relation::No(child) = self.relate(src_elem, tgt_elem, kind, assumed) {
                return Relation::No(ReasonChain::of(Reason::TupleElement {
                    index,
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }
        Relation::Yes
    }

    /// Tuple-to-array assignability requires every tuple element to fit the array
    /// element type. The first failure reuses `ArrayElement`; `[]` trivially
    /// satisfies any `T[]`.
    pub(super) fn relate_tuple_to_array(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        // `src` is tuple-tagged and `tgt` array-tagged here; the side-tables always
        // resolve. The `else` arms are defensive and produce a leaf.
        let Some(src_tuple) = self.store.tuple_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_arr) = self.store.array_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let tgt_elem = tgt_arr.element;
        // Snapshot the element ids before the recursive `self.relate` (borrow note as
        // in `relate_tuples`).
        let elements: Vec<TypeId> = src_tuple.elements.clone();

        // Every tuple element must be assignable to the array's element type.
        for src_elem in elements {
            if let Relation::No(child) = self.relate(src_elem, tgt_elem, kind, assumed) {
                return Relation::No(ReasonChain::of(Reason::ArrayElement {
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }
        Relation::Yes
    }
}
