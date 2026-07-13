use super::*;
use crate::types::repr::LiteralValue;

/// The `(name, type)` pairs of an object type, or `None` if `ty` is not an object.
pub(super) fn property_pairs(store: &Store, ty: TypeId) -> Option<Vec<(String, TypeId)>> {
    store.object_type(ty).map(|object| {
        object
            .properties
            .iter()
            .map(|p| (p.name.clone(), p.ty))
            .collect()
    })
}

/// The (parameter types, return type) of a function type, or `None` if `ty` is not
/// a function. Parameters are returned positionally (the relation/inference order).
pub(super) fn function_shape(store: &Store, ty: TypeId) -> Option<(Vec<TypeId>, TypeId)> {
    store
        .function_type(ty)
        .map(|function| (function.params.iter().map(|p| p.ty).collect(), function.ret))
}

/// Whether a matched segment satisfies a **non-capturing** template hole (M27): a
/// `string` intrinsic accepts anything, a `number` intrinsic a decimal segment, a
/// literal its own string form, a union any matching member. Mirrors the relation
/// engine's hole matcher (kept local so the inference engine has no dependency on it).
pub(super) fn segment_matches_hole(store: &Store, hole: TypeId, seg: &str) -> bool {
    match store.intrinsic_kind(hole) {
        Some(IntrinsicKind::String) => return true,
        Some(IntrinsicKind::Number) => return is_decimal_numeric(seg),
        _ => {}
    }
    if let Some(lit) = store.literal_value(hole) {
        let text = match lit {
            LiteralValue::String(s) => s.clone(),
            LiteralValue::Number(n) => crate::types::repr::number_to_string(*n),
            LiteralValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
        };
        return text == seg;
    }
    if let Some(members) = store.union_members(hole) {
        return members
            .iter()
            .any(|&member| segment_matches_hole(store, member, seg));
    }
    false
}

/// Whether a segment is a decimal numeric string (M27 — the `` `${number}` `` acceptance
/// rule, using the historical Rust `Display` round trip).
fn is_decimal_numeric(s: &str) -> bool {
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
        Ok(n) => crate::types::repr::decimal_number_to_string(n) == s,
        Err(_) => false,
    }
}

/// Widen a candidate: a literal widens to its base intrinsic (`5` → `number`);
/// every other type passes through unchanged. Mirrors the checker's `widen` (kept
/// local so the inference engine has no back-dependency on the checker module).
pub(super) fn widen(interner: &Interner, ty: TypeId) -> TypeId {
    match interner.store().literal_value(ty) {
        Some(lit) => intrinsic_id(interner, lit.base_kind()),
        None => ty,
    }
}

/// Well-known id for an intrinsic kind (the literal-widening targets only need the
/// three primitive bases, but the full mapping keeps the match exhaustive).
fn intrinsic_id(interner: &Interner, kind: crate::types::repr::IntrinsicKind) -> TypeId {
    use crate::types::repr::IntrinsicKind;
    let wk = interner.well_known();
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
        // M28 string-intrinsic markers.
        IntrinsicKind::Uppercase => wk.uppercase,
        IntrinsicKind::Lowercase => wk.lowercase,
        IntrinsicKind::Capitalize => wk.capitalize,
        IntrinsicKind::Uncapitalize => wk.uncapitalize,
        IntrinsicKind::ThisType => wk.this_type,
        IntrinsicKind::OmitThisParameter => wk.omit_this_parameter,
    }
}
