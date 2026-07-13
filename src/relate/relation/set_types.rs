use super::*;

impl<'a> Relater<'a> {
    /// Union **source** relation (mvp-plan §6, M4): a union `src` is assignable to
    /// `tgt` iff **every** member is. The first failing member (in canonical
    /// order) is wrapped as a `UnionSourceMember` reason carrying the member's own
    /// nested cause, so M6 can render "…because `<member>` is not assignable…".
    pub(super) fn relate_union_source(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
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
    pub(super) fn relate_union_target(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
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
            let mut member_assumed: AssumedSet = FxHashSet::default();
            if self.relate(src, member, kind, &mut member_assumed).is_yes() {
                assumed.extend(member_assumed);
                return Relation::Yes;
            }
        }
        Relation::No(ReasonChain::of(Reason::NoUnionMember { src, tgt }))
    }

    /// Intersection **target** relation (M31): `src` is assignable to `A & B` iff it is
    /// assignable to **every** member (AND — the structural dual of a union *source*,
    /// mirroring [`Relater::relate_union_source`]). The first failing member's own
    /// reason is returned **directly** (unwrapped, like [`Relater::relate_conditional_source`]),
    /// so a source missing a member's required property surfaces as that member's
    /// `MissingProperty` (→ `TK2741`) and a value mismatch as its `Property` (→ `TK2322`)
    /// — the headline the corpus pins.
    ///
    /// "Every member relates" is an AND, so all members' assume-true dependencies
    /// genuinely contribute to the intersection's `Yes` and flow up through the shared
    /// `assumed` accumulator (the cache-soundness discipline is unchanged — §6.3).
    pub(super) fn relate_intersection_target(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        // Snapshot the member ids so the immutable borrow does not overlap the
        // recursive `self.relate` calls (see `relate_union_source` for the borrow note).
        let Some(members) = self.store.intersection_members(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let members: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|member| {
                self.well_known
                    .this_type_operand(self.store, *member)
                    .is_none()
            })
            .collect();

        for member in members {
            if let Relation::No(child) = self.relate(src, member, kind, assumed) {
                return Relation::No(child);
            }
        }
        Relation::Yes
    }

    /// Intersection **source** relation (M31): whether `A & B & … <: tgt`. Delegates to
    /// the sound merged-source engine [`Relater::relate_source_members_to`] over the
    /// intersection's members.
    ///
    /// **The soundness subtlety** (review finding 1): `A <: T` does **not** imply
    /// `A & C <: T` — a single member being assignable is enough ONLY when the target
    /// cannot reject on a *sibling-contributed present* property. It can whenever the
    /// target penalizes a present property: an **optional** property (`{a?:string}`
    /// rejects a present `a:number`) or an **index signature**. So a naive
    /// "some member assignable ⟹ intersection assignable" shortcut drops errors. The
    /// merged engine below only takes the some-member OR for **non-object** targets
    /// (which have no presence penalty) and sees ALL contributed properties for object
    /// targets.
    pub(super) fn relate_intersection_source(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        let Some(members) = self.store.intersection_members(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let members: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|member| {
                self.well_known
                    .this_type_operand(self.store, *member)
                    .is_none()
            })
            .collect();
        // The merged-source recursion's own assume-true cycle guard, seeded empty and
        // scoped to THIS query — see `relate_source_members_to`.
        let mut in_flight: MergedInFlightSet = FxHashSet::default();
        self.relate_source_members_to(src, &members, tgt, kind, assumed, &mut in_flight)
    }
}
