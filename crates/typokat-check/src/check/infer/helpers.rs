use super::*;
use crate::types::repr::LiteralValue;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct InferenceMeasure {
    pub object_snapshot_vectors: u64,
    pub object_snapshot_entries: u64,
    pub object_snapshot_name_bytes: u64,
    pub object_target_properties: u64,
    pub object_source_property_comparisons: u64,
}

#[cfg(test)]
thread_local! {
    static INFERENCE_MEASURE: std::cell::RefCell<InferenceMeasure> = std::cell::RefCell::new(InferenceMeasure::default());
}

#[cfg(test)]
pub(super) fn reset_inference_measure() {
    INFERENCE_MEASURE.with(|measure| *measure.borrow_mut() = InferenceMeasure::default());
}

#[cfg(test)]
pub(super) fn inference_measure() -> InferenceMeasure {
    INFERENCE_MEASURE.with(|measure| *measure.borrow())
}

#[cfg(test)]
pub(super) fn measure_inference(update: impl FnOnce(&mut InferenceMeasure)) {
    INFERENCE_MEASURE.with(|measure| update(&mut measure.borrow_mut()));
}

/// The `(key, type)` pairs of an object type, or `None` if `ty` is not an object.
pub(super) fn property_pairs(store: &Store, ty: TypeId) -> Option<Vec<(PropertyKey, TypeId)>> {
    store.object_type(ty).map(|object| {
        #[cfg(test)]
        measure_inference(|measure| measure.object_snapshot_vectors += 1);
        object
            .properties
            .iter()
            .map(|p| {
                #[cfg(test)]
                measure_inference(|measure| {
                    measure.object_snapshot_entries += 1;
                    measure.object_snapshot_name_bytes +=
                        p.key.as_string().map_or(0, str::len) as u64;
                });
                (p.key.clone(), p.ty)
            })
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
            LiteralValue::WellKnownSymbol(_) => return false,
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
    decimal_numeric_value(s).is_some()
}

pub(super) fn decimal_numeric_value(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    let mut seen_dot = false;
    for (i, c) in s.char_indices() {
        if c == '.' {
            if seen_dot || i == 0 || i + 1 == s.len() {
                return None;
            }
            seen_dot = true;
        } else if !c.is_ascii_digit() {
            return None;
        }
    }
    s.parse::<f64>()
        .ok()
        .filter(|number| crate::types::repr::decimal_number_to_string(*number) == s)
}

pub(super) enum TemplateNumericCapture {
    Literal(f64),
    Number,
}

pub(super) enum ConstrainedTemplateCapture {
    String,
    Numeric(TemplateNumericCapture),
    Boolean(bool),
    Constraint,
}

pub(super) fn constrained_template_capture(
    store: &Store,
    constraint: TypeId,
    segment: &str,
) -> Option<ConstrainedTemplateCapture> {
    if store.intrinsic_kind(constraint) == Some(IntrinsicKind::Number) {
        return template_numeric_capture(segment).map(ConstrainedTemplateCapture::Numeric);
    }
    if store.intrinsic_kind(constraint) == Some(IntrinsicKind::Boolean) {
        return match segment {
            "true" => Some(ConstrainedTemplateCapture::Boolean(true)),
            "false" => Some(ConstrainedTemplateCapture::Boolean(false)),
            _ => None,
        };
    }
    if string_literal_satisfies_constraint(store, constraint, segment) {
        return Some(ConstrainedTemplateCapture::String);
    }
    if let Some(TemplateNumericCapture::Literal(number)) = template_numeric_capture(segment) {
        if constraint_accepts_precise_number(store, constraint, number) {
            return Some(ConstrainedTemplateCapture::Numeric(
                TemplateNumericCapture::Literal(number),
            ));
        }
    }
    if let Some(value) = match segment {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    } {
        if constraint_accepts_precise_boolean(store, constraint, value) {
            return Some(ConstrainedTemplateCapture::Boolean(value));
        }
    }
    constraint_matches_template_segment(store, constraint, segment)
        .then_some(ConstrainedTemplateCapture::Constraint)
}

fn constraint_accepts_precise_number(store: &Store, constraint: TypeId, number: f64) -> bool {
    if store.intersection_members(constraint).is_some() {
        return false;
    }
    if store.intrinsic_kind(constraint) == Some(IntrinsicKind::Number) {
        return true;
    }
    if let Some(LiteralValue::Number(expected)) = store.literal_value(constraint) {
        return *expected == number;
    }
    store.union_members(constraint).is_some_and(|members| {
        members
            .iter()
            .any(|member| constraint_accepts_precise_number(store, *member, number))
    })
}

fn constraint_accepts_precise_boolean(store: &Store, constraint: TypeId, value: bool) -> bool {
    if store.intersection_members(constraint).is_some() {
        return false;
    }
    if store.intrinsic_kind(constraint) == Some(IntrinsicKind::Boolean) {
        return true;
    }
    if let Some(LiteralValue::Boolean(expected)) = store.literal_value(constraint) {
        return *expected == value;
    }
    store.union_members(constraint).is_some_and(|members| {
        members
            .iter()
            .any(|member| constraint_accepts_precise_boolean(store, *member, value))
    })
}

fn string_literal_satisfies_constraint(store: &Store, constraint: TypeId, segment: &str) -> bool {
    match store.intrinsic_kind(constraint) {
        Some(IntrinsicKind::Error | IntrinsicKind::Any | IntrinsicKind::Unknown) => return true,
        Some(IntrinsicKind::String) => return true,
        _ => {}
    }
    if let Some(LiteralValue::String(value)) = store.literal_value(constraint) {
        return value == segment;
    }
    if let Some(members) = store.union_members(constraint) {
        return members
            .iter()
            .any(|member| string_literal_satisfies_constraint(store, *member, segment));
    }
    if let Some(members) = store.intersection_members(constraint) {
        return members
            .iter()
            .all(|member| string_literal_satisfies_constraint(store, *member, segment));
    }
    if is_empty_object_constraint(store, constraint) {
        return true;
    }
    false
}

fn constraint_matches_template_segment(store: &Store, constraint: TypeId, segment: &str) -> bool {
    match store.intrinsic_kind(constraint) {
        Some(
            IntrinsicKind::Error
            | IntrinsicKind::Any
            | IntrinsicKind::Unknown
            | IntrinsicKind::String,
        ) => return true,
        Some(IntrinsicKind::Number) => return template_numeric_capture(segment).is_some(),
        Some(IntrinsicKind::Boolean) => return matches!(segment, "true" | "false"),
        Some(IntrinsicKind::Null) => return segment == "null",
        Some(IntrinsicKind::Undefined) => return segment == "undefined",
        _ => {}
    }
    if let Some(literal) = store.literal_value(constraint) {
        let expected = match literal {
            LiteralValue::String(value) => value.clone(),
            LiteralValue::Number(value) => crate::types::repr::number_to_string(*value),
            LiteralValue::Boolean(value) => value.to_string(),
            LiteralValue::WellKnownSymbol(_) => return false,
        };
        return expected == segment;
    }
    if let Some(members) = store.union_members(constraint) {
        return members
            .iter()
            .any(|member| constraint_matches_template_segment(store, *member, segment));
    }
    if let Some(members) = store.intersection_members(constraint) {
        return members
            .iter()
            .all(|member| constraint_matches_template_segment(store, *member, segment));
    }
    if is_empty_object_constraint(store, constraint) {
        return true;
    }
    false
}

fn is_empty_object_constraint(store: &Store, constraint: TypeId) -> bool {
    store.object_type(constraint).is_some_and(|object| {
        object.properties.is_empty()
            && object.string_index.is_none()
            && object.number_index.is_none()
            && object.call_signatures.is_empty()
            && object.construct_signatures.is_empty()
    })
}

/// TypeScript preserves canonical decimal captures as literals and widens other
/// accepted numeric spellings (exponents, radix prefixes, surrounding space) to number.
pub(super) fn template_numeric_capture(s: &str) -> Option<TemplateNumericCapture> {
    if s.is_empty() {
        return None;
    }
    let trimmed = s.trim_matches(is_ecmascript_whitespace);
    if trimmed.is_empty() {
        return Some(TemplateNumericCapture::Number);
    }
    let radix_value = [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ]
    .into_iter()
    .find_map(|(prefix, radix)| {
        trimmed
            .strip_prefix(prefix)
            .filter(|digits| !digits.is_empty())
            .and_then(|digits| {
                digits.chars().try_fold(0.0, |number, digit| {
                    digit
                        .to_digit(radix)
                        .map(|digit| number * f64::from(radix) + f64::from(digit))
                })
            })
    });
    let number = match radix_value.or_else(|| trimmed.parse::<f64>().ok()) {
        Some(number) if number.is_finite() => number,
        _ => return None,
    };
    if s == crate::types::repr::decimal_number_to_string(number) {
        Some(TemplateNumericCapture::Literal(number))
    } else {
        Some(TemplateNumericCapture::Number)
    }
}

fn is_ecmascript_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}' | '\u{000B}' | '\u{000C}' | '\u{0020}' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
                | '\n'
                | '\r'
    )
}

/// Candidate fixing uses the same mode-aware boundary as declarations.
pub(super) fn widen(interner: &mut Interner, ty: TypeId) -> TypeId {
    interner.widen_literal_type(ty)
}
