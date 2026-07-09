use super::*;
use crate::types::repr::{LiteralValue, TemplateType};

impl<'a> Relater<'a> {
    /// Deferred conditional as **source** (M25): assignable to `tgt` iff **both**
    /// branches are, but only when both branches are **closed** w.r.t. this node's own
    /// `infer` binders (a branch still containing an `infer` node is not a meaningful
    /// type outside the match, so the whole relation is a conservative `No` — the sound
    /// over-report). Each branch relation runs through the ordinary [`Relater::relate`],
    /// so the cache / cycle-stack invariants are unchanged.
    pub(super) fn relate_conditional_source(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut FxHashSet<RelationKey>,
    ) -> Relation {
        let Some(cond) = self.store.conditional_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let (true_branch, false_branch) = (cond.true_branch, cond.false_branch);
        if self.contains_infer(true_branch) || self.contains_infer(false_branch) {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        if let Relation::No(child) = self.relate(true_branch, tgt, kind, assumed) {
            return Relation::No(child);
        }
        self.relate(false_branch, tgt, kind, assumed)
    }

    /// Whether `ty` contains an unbound `infer` binder ([`TypeTag::Infer`]) — used to
    /// decide a deferred conditional's branch is *open* (M25). Iterative with a visited
    /// set so a recursive interned type cannot loop; does not descend into a nested
    /// conditional (which rebinds its own indices).
    fn contains_infer(&self, ty: TypeId) -> bool {
        let mut stack = vec![ty];
        let mut visited: FxHashSet<TypeId> = FxHashSet::default();
        while let Some(t) = stack.pop() {
            if !visited.insert(t) {
                continue;
            }
            match self.store.tag(t) {
                TypeTag::Infer => return true,
                TypeTag::Object => {
                    if let Some(object) = self.store.object_type(t) {
                        stack.extend(object.properties.iter().map(|p| p.ty));
                        stack.extend(object.string_index);
                        stack.extend(object.number_index);
                        stack.extend(object.call_signatures.iter().copied());
                        stack.extend(object.construct_signatures.iter().copied());
                    }
                }
                TypeTag::Function => {
                    if let Some(f) = self.store.function_type(t) {
                        stack.extend(f.params.iter().map(|p| p.ty));
                        stack.push(f.ret);
                    }
                }
                TypeTag::Union => {
                    if let Some(members) = self.store.union_members(t) {
                        stack.extend(members.iter().copied());
                    }
                }
                // M31: an intersection carries its `infer` binders in its members
                // (`(infer U)[] & X` in an extends position) — descend into them.
                TypeTag::Intersection => {
                    if let Some(members) = self.store.intersection_members(t) {
                        stack.extend(members.iter().copied());
                    }
                }
                TypeTag::Array => {
                    if let Some(a) = self.store.array_type(t) {
                        stack.push(a.element);
                    }
                }
                TypeTag::Tuple => {
                    if let Some(tup) = self.store.tuple_type(t) {
                        stack.extend(tup.elements.iter().copied());
                    }
                }
                TypeTag::Readonly => {
                    if let Some(operand) = self.store.readonly_operand(t) {
                        stack.push(operand);
                    }
                }
                TypeTag::Instantiation => {
                    if let Some(inst) = self.store.instantiation_type(t) {
                        stack.extend(inst.args.iter().map(|(_, v)| *v));
                    }
                }
                // M27: a template literal type carries its `infer` binders in its holes
                // (`` `a${infer R}` `` in an extends position) — descend into them.
                TypeTag::Template => {
                    if let Some(template) = self.store.template_type(t) {
                        stack.extend(template.holes.iter().copied());
                    }
                }
                // M28: a deferred keyof's operand may carry an infer binder
                // (`keyof (infer U)` inside an extends position) — descend into it.
                TypeTag::Keyof => {
                    if let Some(operand) = self.store.keyof_operand(t) {
                        stack.push(operand);
                    }
                }
                // A nested conditional rebinds its own infer indices — do not descend.
                // A mapped type (M26) is related as a deferred node (never traversed for
                // its own infer binders), and a mapped-value placeholder is not an infer.
                TypeTag::Intrinsic
                | TypeTag::Literal
                | TypeTag::TypeParam
                | TypeTag::Conditional
                | TypeTag::Mapped
                | TypeTag::MappedValue => {}
            }
        }
        false
    }

    /// Template literal type as **source** (M27): every template is a string subtype, so
    /// it flows into `string`; into another template it flows by **subsumption** (anchor
    /// containment — see [`template_subsumes`]). A deferred template (free-parameter hole)
    /// relates only to `string` and to an identical node (already accepted by the
    /// `src == tgt` fast path); everything else is a conservative `No`.
    pub(super) fn relate_template_source(&self, src: TypeId, tgt: TypeId) -> Relation {
        // Every template literal type is a subtype of `string`.
        if tgt == self.well_known.string {
            return Relation::Yes;
        }
        if self.store.tag(tgt) == TypeTag::Template && template_subsumes(self.store, src, tgt) {
            return Relation::Yes;
        }
        Relation::No(ReasonChain::leaf(src, tgt))
    }

    /// Template literal type as **target** (M27): a string-literal source is matched
    /// against the pattern by **anchored segment scanning** ([`match_literal_against_pattern`]);
    /// the `string` intrinsic matches only the bare `` `${string}` `` hole (a pattern with
    /// literal text requires more than `string` guarantees); everything else (a
    /// number/boolean literal, a free parameter, an object, …) is a conservative `No`.
    pub(super) fn relate_template_target(&self, src: TypeId, tgt: TypeId) -> Relation {
        let Some(template) = self.store.template_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if let Some(LiteralValue::String(s)) = self.store.literal_value(src) {
            if match_literal_against_pattern(self.store, s, template) {
                return Relation::Yes;
            }
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        if src == self.well_known.string && is_bare_string_hole(self.store, template) {
            return Relation::Yes;
        }
        Relation::No(ReasonChain::leaf(src, tgt))
    }
}

/// Match a string literal `s` against a template **pattern** (M27): the literal anchors
/// must appear IN ORDER — the leading text is a prefix, the trailing text a suffix, and
/// each interior text a left-to-right **non-greedy** separator — with the holes filling
/// the gaps: `${string}` matches any (possibly empty) segment, `${number}` a decimal
/// numeric segment, a literal hole its own value. An empty interior separator (adjacent
/// holes) is out of the relation subset — a conservative `false` (over-report).
fn match_literal_against_pattern(store: &Store, s: &str, template: &TemplateType) -> bool {
    let texts = &template.texts;
    let holes = &template.holes;
    if holes.is_empty() {
        // A hole-less template does not survive as a node; match only its single text.
        return texts.first().map(String::as_str) == Some(s);
    }
    let prefix = texts.first().map(String::as_str).unwrap_or("");
    let Some(mut rest) = s.strip_prefix(prefix) else {
        return false;
    };
    let n = holes.len();
    for (i, &hole) in holes.iter().enumerate() {
        let sep = texts.get(i + 1).map(String::as_str).unwrap_or("");
        if i == n - 1 {
            // Last hole spans the remainder up to the trailing suffix.
            let Some(seg) = rest.strip_suffix(sep) else {
                return false;
            };
            if !hole_matches_segment(store, hole, seg) {
                return false;
            }
            rest = "";
        } else {
            // Non-last hole ends at the first occurrence of the separator (non-greedy);
            // an empty separator (adjacent holes) is out of subset.
            if sep.is_empty() {
                return false;
            }
            let Some(idx) = rest.find(sep) else {
                return false;
            };
            if !hole_matches_segment(store, hole, &rest[..idx]) {
                return false;
            }
            rest = &rest[idx + sep.len()..];
        }
    }
    rest.is_empty()
}

/// Whether a matched segment satisfies a template hole (M27): a `string` intrinsic
/// accepts anything, a `number` intrinsic a decimal numeric segment, a literal its own
/// string form, a union any member that matches. Any other hole (a free parameter,
/// `infer`, `unknown`, …) matches nothing — a string literal is never assignable to a
/// pattern with an unresolved hole.
fn hole_matches_segment(store: &Store, hole: TypeId, seg: &str) -> bool {
    match store.intrinsic_kind(hole) {
        Some(IntrinsicKind::String) => return true,
        Some(IntrinsicKind::Number) => return is_numeric_segment(seg),
        _ => {}
    }
    if let Some(lit) = store.literal_value(hole) {
        return literal_segment(lit) == seg;
    }
    if let Some(members) = store.union_members(hole) {
        return members
            .iter()
            .any(|&member| hole_matches_segment(store, member, seg));
    }
    false
}

/// The string form of a literal for template-segment matching (M27).
fn literal_segment(lit: &LiteralValue) -> String {
    match lit {
        LiteralValue::String(s) => s.clone(),
        LiteralValue::Number(n) => crate::types::repr::number_to_string(*n),
        LiteralValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
    }
}

/// Whether a segment is a **decimal** numeric string a `` `${number}` `` hole accepts
/// (M27): digits with at most one interior decimal point, and — to stay sound against
/// tsc's `String(Number(s)) === s` rule — no redundant leading/trailing zeros (checked by
/// re-stringifying). Scientific / signed / `Infinity` / `NaN` forms are conservatively
/// rejected (over-report, documented).
fn is_numeric_segment(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut seen_dot = false;
    for (i, c) in s.char_indices() {
        if c == '.' {
            if seen_dot || i == 0 || i + 1 == s.len() {
                return false;
            }
            seen_dot = true;
        } else if !c.is_ascii_digit() {
            return false;
        }
    }
    match s.parse::<f64>() {
        Ok(n) => crate::types::repr::number_to_string(n) == s,
        Err(_) => false,
    }
}

/// Whether a template is exactly the bare `` `${string}` `` hole (M27): one `string`
/// intrinsic hole and no literal text. This is the one pattern the `string` intrinsic is
/// assignable to (it is mutually assignable with `string`).
fn is_bare_string_hole(store: &Store, template: &TemplateType) -> bool {
    template.holes.len() == 1
        && store.intrinsic_kind(template.holes[0]) == Some(IntrinsicKind::String)
        && template.texts.iter().all(|t| t.is_empty())
}

/// Whether every hole of a template is a `string`/`number` intrinsic (M27) — a
/// **concrete pattern** whose matched string set is decidable. A template with a
/// free-parameter (or other symbolic) hole is a **deferred** node, which relates only
/// identically.
fn is_concrete_pattern(store: &Store, template: &TemplateType) -> bool {
    template.holes.iter().all(|&hole| {
        matches!(
            store.intrinsic_kind(hole),
            Some(IntrinsicKind::String) | Some(IntrinsicKind::Number)
        )
    })
}

/// Whether template pattern `src` is subsumed by template pattern `tgt` (M27): every
/// string matched by `src` is matched by `tgt`. `src != tgt` here (identity is the fast
/// path). Only **concrete patterns** subsume — a deferred template relates identically
/// only. The bare `` `${string}` `` target accepts everything; otherwise only a single
/// `${string}`-hole target `` `PREFIX${string}SUFFIX` `` is decided (its lone wildcard
/// absorbs everything between the guaranteed anchors), by checking its prefix is a prefix
/// of `src`'s leading text and its suffix a suffix of `src`'s trailing text. A multi-hole
/// or `${number}` target is a conservative `false` (over-report).
fn template_subsumes(store: &Store, src: TypeId, tgt: TypeId) -> bool {
    let (Some(src_t), Some(tgt_t)) = (store.template_type(src), store.template_type(tgt)) else {
        return false;
    };
    if !is_concrete_pattern(store, src_t) || !is_concrete_pattern(store, tgt_t) {
        return false;
    }
    if is_bare_string_hole(store, tgt_t) {
        return true;
    }
    if tgt_t.holes.len() == 1
        && store.intrinsic_kind(tgt_t.holes[0]) == Some(IntrinsicKind::String)
    {
        let tgt_prefix = tgt_t.texts.first().map(String::as_str).unwrap_or("");
        let tgt_suffix = tgt_t.texts.get(1).map(String::as_str).unwrap_or("");
        let src_prefix = src_t.texts.first().map(String::as_str).unwrap_or("");
        let src_suffix = src_t.texts.last().map(String::as_str).unwrap_or("");
        return src_prefix.starts_with(tgt_prefix) && src_suffix.ends_with(tgt_suffix);
    }
    false
}
