use super::*;
use crate::types::repr::TupleType;

#[derive(Clone)]
struct TupleShape {
    prefix: Vec<TypeId>,
    variadic: Option<TypeId>,
    suffix: Vec<TypeId>,
}

impl TupleShape {
    fn fixed(elements: Vec<TypeId>) -> Self {
        TupleShape {
            prefix: elements,
            variadic: None,
            suffix: Vec::new(),
        }
    }

    fn min_len(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }

    fn fixed_len(&self) -> Option<usize> {
        if self.variadic.is_some() {
            None
        } else {
            Some(self.prefix.len() + self.suffix.len())
        }
    }

    fn accepts_len(&self, len: usize) -> bool {
        if len < self.min_len() {
            return false;
        }
        self.variadic.is_some() || len == self.min_len()
    }

    fn element_at(&self, index: usize, len: usize) -> Option<TypeId> {
        if !self.accepts_len(len) || index >= len {
            return None;
        }
        if index < self.prefix.len() {
            return self.prefix.get(index).copied();
        }
        let suffix_start = len.saturating_sub(self.suffix.len());
        if index >= suffix_start {
            return self.suffix.get(index - suffix_start).copied();
        }
        self.variadic
    }
}

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

        let Some(src_shape) = self.tuple_shape(src_tuple) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_shape) = self.tuple_shape(tgt_tuple) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };

        let Some(src_len) = src_shape.fixed_len() else {
            return self
                .relate_variadic_tuple_to_tuple(src, tgt, src_shape, tgt_shape, kind, assumed);
        };
        if !tgt_shape.accepts_len(src_len) {
            return Relation::No(ReasonChain::of(Reason::TupleLength { src, tgt }));
        }

        for index in 0..src_len {
            let Some(src_elem) = src_shape.element_at(index, src_len) else {
                return Relation::No(ReasonChain::of(Reason::TupleLength { src, tgt }));
            };
            let Some(tgt_elem) = tgt_shape.element_at(index, src_len) else {
                return Relation::No(ReasonChain::of(Reason::TupleLength { src, tgt }));
            };
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
        let Some(src_shape) = self.tuple_shape(src_tuple) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };

        for src_elem in src_shape
            .prefix
            .iter()
            .chain(src_shape.suffix.iter())
            .copied()
        {
            if let Relation::No(child) = self.relate(src_elem, tgt_elem, kind, assumed) {
                return Relation::No(ReasonChain::of(Reason::ArrayElement {
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }
        if let Some(src_elem) = src_shape.variadic {
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

    fn relate_variadic_tuple_to_tuple(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        src_shape: TupleShape,
        tgt_shape: TupleShape,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        if tgt_shape.variadic.is_none()
            || src_shape.min_len() < tgt_shape.min_len()
            || src_shape.prefix.len() != tgt_shape.prefix.len()
            || src_shape.suffix.len() != tgt_shape.suffix.len()
        {
            return Relation::No(ReasonChain::of(Reason::TupleLength { src, tgt }));
        }

        for (index, (&src_elem, &tgt_elem)) in
            src_shape.prefix.iter().zip(&tgt_shape.prefix).enumerate()
        {
            if let Relation::No(child) = self.relate(src_elem, tgt_elem, kind, assumed) {
                return Relation::No(ReasonChain::of(Reason::TupleElement {
                    index,
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }
        let suffix_offset = src_shape.prefix.len();
        for (offset, (&src_elem, &tgt_elem)) in
            src_shape.suffix.iter().zip(&tgt_shape.suffix).enumerate()
        {
            if let Relation::No(child) = self.relate(src_elem, tgt_elem, kind, assumed) {
                return Relation::No(ReasonChain::of(Reason::TupleElement {
                    index: suffix_offset + offset,
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }
        let Some(src_elem) = src_shape.variadic else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_elem) = tgt_shape.variadic else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if let Relation::No(child) = self.relate(src_elem, tgt_elem, kind, assumed) {
            return Relation::No(ReasonChain::of(Reason::TupleElement {
                index: src_shape.prefix.len(),
                src,
                tgt,
                because: Box::new(child.head),
            }));
        }
        Relation::Yes
    }

    fn tuple_shape(&self, tuple: &TupleType) -> Option<TupleShape> {
        let Some(rest) = tuple.rest else {
            return Some(TupleShape::fixed(tuple.elements.clone()));
        };
        if rest.position > tuple.elements.len() {
            return None;
        }
        let mut prefix = tuple.elements[..rest.position].to_vec();
        let suffix = tuple.elements[rest.position..].to_vec();
        let rest_shape = self.rest_tuple_shape(rest.ty)?;
        prefix.extend(rest_shape.prefix);
        let mut combined_suffix = rest_shape.suffix;
        combined_suffix.extend(suffix);
        Some(TupleShape {
            prefix,
            variadic: rest_shape.variadic,
            suffix: combined_suffix,
        })
    }

    fn rest_tuple_shape(&self, ty: TypeId) -> Option<TupleShape> {
        let ty = self.store.readonly_operand(ty).unwrap_or(ty);
        if let Some(array) = self.store.array_type(ty) {
            return Some(TupleShape {
                prefix: Vec::new(),
                variadic: Some(array.element),
                suffix: Vec::new(),
            });
        }
        if let Some(tuple) = self.store.tuple_type(ty) {
            return self.tuple_shape(tuple);
        }
        None
    }
}
