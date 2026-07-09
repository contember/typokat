use super::*;
use crate::types::repr::{PropertyType, Visibility};

impl<'a> Relater<'a> {
    /// Property-wise object relation. Returns the **first** failing target
    /// property (in canonical order) as a precise reason: a `MissingProperty`
    /// when the source lacks a required target property, or a `Property` wrapping
    /// the nested reason when the property is present but its type does not
    /// relate. First-failure ordering keeps the verdict deterministic and matches
    /// how the M2 corpus pairs one cause per failing assignment.
    ///
    /// M19 — **index signatures**: after the named-property obligations, if the
    /// **target** has a string index signature, every **named property of the
    /// source** (any key is a string key) and the source's own string index value
    /// (if any) must be assignable to the target's string index value type; likewise
    /// a target number index signature constrains the source's **numeric-named**
    /// properties and the source's number index value. A single failing value is
    /// reported as one `IndexSignature` reason (`TK2322`). The target's index
    /// signatures are checked here; the source having *extra* index signatures the
    /// target lacks is fine (width).
    pub(super) fn relate_objects(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        // Both ids are object-tagged here; the side-tables always resolve. The
        // `else` arms are defensive (an object tag without a payload is a store
        // invariant violation, never expected) and produce a leaf rather than
        // panicking. These borrows are of `*self.store` (lifetime `'a`), independent
        // of `&mut self`, so they live across the recursive `self.relate` calls.
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
                    // M21: presence is independent of the value type. An *optional* source
                    // property may be ABSENT, so it cannot satisfy a *required* target
                    // property (which must be present) — regardless of whether their value
                    // types relate (e.g. a required target of `string | undefined` still
                    // rejects an optional source, because the source may OMIT it entirely).
                    // required->optional and both-optional are fine; the value relation
                    // below handles the rest. Like the nominal rule above, this is an
                    // object-level structural failure, so it is a plain `Leaf` (not a
                    // value-depth `Property` — the member's *type* may relate fine).
                    if src_prop.optional && !tgt_prop.optional {
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
                // Target property absent in the source.
                None => {
                    // M21: an *optional* target property may be absent — skip it. Its
                    // effective type already includes `undefined` (unioned in at
                    // lowering), so a *present* source value is related normally in the
                    // `Some(..)` arm above; absence is simply allowed. Only a *required*
                    // absent target property is a missing-property failure (`TK2741`).
                    if tgt_prop.optional {
                        continue;
                    }
                    return Relation::No(ReasonChain::of(Reason::MissingProperty {
                        name: tgt_prop.name.clone(),
                        src,
                        tgt,
                    }));
                }
            }
        }

        // F1/WU2 — call-signature obligation. A target object with a call
        // signature requires the source object to have a compatible call
        // signature; an object without one cannot satisfy a callable target. This
        // obligation goes through `self.relate` on the interned FunctionType ids,
        // preserving the relation cache/cycle-stack invariants.
        if let Relation::No(child) = self.relate_object_call_signatures(src, tgt, kind, assumed) {
            return Relation::No(child);
        }

        // F1/WU3 — construct-signature obligation. Mirrors the call-signature
        // obligation above: a target object with `new (...)` requires the source
        // object to have a compatible single construct signature, compared by the
        // existing FunctionType relation through `self.relate`.
        if let Relation::No(child) =
            self.relate_object_construct_signatures(src, tgt, kind, assumed)
        {
            return Relation::No(child);
        }

        // M19 — index-signature obligations. Snapshot the target's index value types
        // and the source's named-property/(index) value types up front so the
        // recursive `self.relate` below does not overlap the read borrows.
        let tgt_string_index = tgt_obj.string_index;
        let tgt_number_index = tgt_obj.number_index;
        if tgt_string_index.is_none() && tgt_number_index.is_none() {
            // No index signature on the target — nothing further to check (the M0–M18
            // structural verdict stands).
            return Relation::Yes;
        }
        // (name, value type) of each source named property, plus the source's own
        // index value types (the source may itself be an index-sig object — its index
        // value must fit the target's, like a dictionary assigned to a dictionary).
        let src_props: Vec<(String, TypeId)> = src_obj
            .properties
            .iter()
            .map(|p| (p.name.clone(), p.ty))
            .collect();
        let src_string_index = src_obj.string_index;
        let src_number_index = src_obj.number_index;

        // String index target: every source named property (any name is a string
        // key) AND the source's own string index value must be assignable to it.
        if let Some(tgt_value) = tgt_string_index {
            for (_, src_value) in &src_props {
                if let Relation::No(child) = self.relate(*src_value, tgt_value, kind, assumed) {
                    return Relation::No(ReasonChain::of(Reason::IndexSignature {
                        src,
                        tgt,
                        because: Box::new(child.head),
                    }));
                }
            }
            if let Some(src_value) = src_string_index {
                if let Relation::No(child) = self.relate(src_value, tgt_value, kind, assumed) {
                    return Relation::No(ReasonChain::of(Reason::IndexSignature {
                        src,
                        tgt,
                        because: Box::new(child.head),
                    }));
                }
            }
        }

        // Number index target: only the source's **numeric-named** properties are
        // constrained (a number index sig does not govern arbitrary string keys),
        // plus the source's own number index value.
        if let Some(tgt_value) = tgt_number_index {
            for (name, src_value) in &src_props {
                if !is_numeric_property_name(name) {
                    continue;
                }
                if let Relation::No(child) = self.relate(*src_value, tgt_value, kind, assumed) {
                    return Relation::No(ReasonChain::of(Reason::IndexSignature {
                        src,
                        tgt,
                        because: Box::new(child.head),
                    }));
                }
            }
            if let Some(src_value) = src_number_index {
                if let Relation::No(child) = self.relate(src_value, tgt_value, kind, assumed) {
                    return Relation::No(ReasonChain::of(Reason::IndexSignature {
                        src,
                        tgt,
                        because: Box::new(child.head),
                    }));
                }
            }
        }

        Relation::Yes
    }

    fn relate_object_call_signatures(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if tgt_obj.call_signatures.is_empty() {
            return Relation::Yes;
        }
        let Some(src_obj) = self.store.object_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let ([src_sig], [tgt_sig]) = (
            src_obj.call_signatures.as_slice(),
            tgt_obj.call_signatures.as_slice(),
        ) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        self.relate(*src_sig, *tgt_sig, kind, assumed)
    }

    fn relate_object_construct_signatures(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if tgt_obj.construct_signatures.is_empty() {
            return Relation::Yes;
        }
        let Some(src_obj) = self.store.object_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let ([src_sig], [tgt_sig]) = (
            src_obj.construct_signatures.as_slice(),
            tgt_obj.construct_signatures.as_slice(),
        ) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        self.relate(*src_sig, *tgt_sig, kind, assumed)
    }

    pub(super) fn relate_object_to_function(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        let Some(src_obj) = self.store.object_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let [src_sig] = src_obj.call_signatures.as_slice() else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        self.relate(*src_sig, tgt, kind, assumed)
    }

    pub(super) fn relate_function_to_object(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let [tgt_sig] = tgt_obj.call_signatures.as_slice() else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };

        // A plain function value has no represented named members. It can satisfy
        // optional target properties by omission, but any required target member is
        // missing and remains a `TK2741`-shaped relation failure.
        for tgt_prop in &tgt_obj.properties {
            if !tgt_prop.optional {
                return Relation::No(ReasonChain::of(Reason::MissingProperty {
                    name: tgt_prop.name.clone(),
                    src,
                    tgt,
                }));
            }
        }

        if !tgt_obj.construct_signatures.is_empty() {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }

        self.relate(src, *tgt_sig, kind, assumed)
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
    pub(super) fn relate_functions(
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

    /// Whether the **merged source** — the intersection of `cands` — relates to `tgt`
    /// (M31, the sound core of the source-intersection rule; no interning, as the
    /// relation engine holds the store read-only). Recursive, so the merge is decided at
    /// every depth:
    ///
    ///  - **object `tgt`** (no index / call / construct signature): the merged source
    ///    must satisfy **every** target property — an AND that sees ALL contributed
    ///    properties, never a some-member shortcut. For a property named by exactly ONE
    ///    member the merged value IS that member's type, so it delegates to the
    ///    cycle-guarded main engine (`self.relate`, exact); a property named by SEVERAL
    ///    members has a genuine merged (intersection) value, decided by **recursing** with
    ///    those `subcands` — NOT a some-candidate shortcut (which would reintroduce the
    ///    finding-1 hole one level down). An uncovered required property is a
    ///    `MissingProperty` (`TK2741`); a covered-but-incompatible one a `Property`
    ///    (`TK2322`). This threads the shared `assumed` (an AND — all contributions
    ///    matter);
    ///  - an object `tgt` carrying an **index / call / construct signature** is out of
    ///    the merged subset → conservative `No` (a documented safe over-report; it is
    ///    also a genuine rejection when a member's value violates the target's index);
    ///  - **non-object `tgt`** (primitive / function / tuple / array / …): no presence
    ///    penalty, so a **single** candidate assignable to `tgt` suffices (sound). An OR,
    ///    so — like [`Relater::relate_union_target`] — each attempt uses its own local
    ///    accumulator and only the winner's assumptions merge up (a union target was
    ///    already decomposed by the earlier target-union dispatch, so `tgt` here is never
    ///    a union).
    ///
    /// **Termination (§6.3-sound).** The multi-contributor per-property recursion can
    /// re-enter the same `(merged source, target)` on a recursive object property
    /// (`interface P { p: P }`; `P & Q <: P`). `in_flight` is an **assume-true** cycle
    /// guard, exactly the coinductive rule the main cycle stack uses
    /// (`relation.rs` §"Cycle stack FIRST"): re-entering a `(sorted cands, tgt, kind)`
    /// already in flight returns `Yes`. The cycle is self-contained within one
    /// `relate_intersection_source` query and discharged at its root (each object-target
    /// entry removes its key on exit); this method **never** touches the durable cache,
    /// and single-contributor properties flow through `self.relate` (so genuine
    /// cross-relation cycles still bubble through `assumed`) — so no provisional `Yes` is
    /// ever durably cached.
    pub(super) fn relate_source_members_to(
        &mut self,
        src: TypeId,
        cands: &[TypeId],
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
        in_flight: &mut MergedInFlightSet,
    ) -> Relation {
        if self.store.tag(tgt) == TypeTag::Object {
            // Assume-true cycle guard for the merged-source recursion (see the doc above).
            // The key canonicalizes the candidate set (sorted) so `[P, Q]` and `[Q, P]`
            // collide.
            let mut key_cands = cands.to_vec();
            key_cands.sort_unstable();
            let key = (key_cands, tgt, kind);
            if in_flight.contains(&key) {
                return Relation::Yes;
            }
            in_flight.insert(key.clone());
            let result = self.relate_merged_object_properties(src, cands, tgt, kind, assumed, in_flight);
            in_flight.remove(&key);
            return result;
        }

        // Non-object target: no presence penalty, so a single assignable candidate
        // suffices (OR — per-attempt accumulator, merge only the winner).
        let mut last_child: Option<ReasonChain> = None;
        for &cand in cands {
            let mut cand_assumed: FxHashSet<RelationKey> = FxHashSet::default();
            match self.relate(cand, tgt, kind, &mut cand_assumed) {
                Relation::Yes => {
                    assumed.extend(cand_assumed);
                    return Relation::Yes;
                }
                Relation::No(child) => last_child = Some(child),
            }
        }
        Relation::No(last_child.unwrap_or_else(|| ReasonChain::leaf(src, tgt)))
    }

    /// The per-property body of the object-target branch of [`Relater::relate_source_members_to`]
    /// (extracted so the caller can bracket it with the `in_flight` insert/remove). The
    /// merged source (`cands`) must satisfy **every** target property.
    fn relate_merged_object_properties(
        &mut self,
        src: TypeId,
        cands: &[TypeId],
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
        in_flight: &mut MergedInFlightSet,
    ) -> Relation {
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if tgt_obj.string_index.is_some()
            || tgt_obj.number_index.is_some()
            || !tgt_obj.call_signatures.is_empty()
            || !tgt_obj.construct_signatures.is_empty()
        {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }

        // Snapshot (name, target type, optional, contributing source types) per target
        // property BEFORE any recursive relate, so no store borrow is held across it. The
        // contributor list is sorted + deduped, so a property named by one DISTINCT type
        // takes the single-contributor path (exact `self.relate`) even if two members
        // agree on it.
        struct Obligation {
            name: String,
            tgt_ty: TypeId,
            optional: bool,
            subcands: Vec<TypeId>,
        }
        let obligations: Vec<Obligation> = tgt_obj
            .properties
            .iter()
            .map(|tgt_prop| {
                let mut subcands: Vec<TypeId> = cands
                    .iter()
                    .filter_map(|&m| {
                        self.store
                            .object_type(m)
                            .and_then(|o| o.property(&tgt_prop.name))
                            .map(|p| p.ty)
                    })
                    .collect();
                subcands.sort_unstable();
                subcands.dedup();
                Obligation {
                    name: tgt_prop.name.clone(),
                    tgt_ty: tgt_prop.ty,
                    optional: tgt_prop.optional,
                    subcands,
                }
            })
            .collect();

        for ob in obligations {
            if ob.subcands.is_empty() {
                // An optional target property may be absent from the merged source;
                // a required one is genuinely missing.
                if ob.optional {
                    continue;
                }
                return Relation::No(ReasonChain::of(Reason::MissingProperty {
                    name: ob.name,
                    src,
                    tgt,
                }));
            }
            // Part A — a single DISTINCT contributor: the merged value IS that type, so
            // delegate to the cycle-guarded main engine (exact `(c) <: tgt_ty` ≡
            // `relate(c, tgt_ty)`; reuses the cache + cycle stack, threads `assumed`).
            // Part B — several contributors: a genuine merged value, recursed through the
            // in-flight-guarded merged engine. Both thread the shared `assumed` (an AND).
            let child = if let [only] = ob.subcands.as_slice() {
                self.relate(*only, ob.tgt_ty, kind, assumed)
            } else {
                self.relate_source_members_to(src, &ob.subcands, ob.tgt_ty, kind, assumed, in_flight)
            };
            if let Relation::No(child) = child {
                return Relation::No(ReasonChain::of(Reason::Property {
                    name: ob.name,
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }
        Relation::Yes
    }
}

/// Whether a property name is numeric-keyed and thus governed by a number index
/// signature. The name must parse as a finite `f64`; ordinary identifiers remain
/// pure string keys.
fn is_numeric_property_name(name: &str) -> bool {
    name.parse::<f64>().map(|n| n.is_finite()).unwrap_or(false)
}

/// Whether the source member satisfies the target member's nominal-origin
/// requirement (M13). Non-public target members require the same visibility and
/// declaring class; a failed check must make the relation fail.
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
