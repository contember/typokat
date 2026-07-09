//! Reason-chain rendering — the nested "…are incompatible." cascade below a headline.

use super::render_type::{parameter_name_at, render_type};
use crate::relate::Reason;
use crate::types::store::Store;

/// Indentation step for one level of reason-chain nesting (two spaces, tsc-style).
const REASON_INDENT: &str = "  ";

/// Render a relation-failure reason chain into the nested lines below the
/// diagnostic headline. Terminals stay headline-only, union-source heads descend
/// to the offending member, and wrappers render their incompatibility line before
/// the nested cause.
pub fn render_reason_chain(store: &Store, head: &Reason) -> Vec<String> {
    match head {
        // The headline already states this mismatch in full.
        Reason::Leaf { .. }
        | Reason::MissingProperty { .. }
        | Reason::NoUnionMember { .. }
        | Reason::ParameterCount { .. }
        // M18: a tuple length mismatch is terminal — the headline states the two
        // tuple types and the cause is just "their lengths differ", so no extra line.
        | Reason::TupleLength { .. } => Vec::new(),
        // The headline names the offending union member (the checker's
        // `headline_src`), so the elaboration is *that member's* reason — exactly
        // as if the member's reason were itself the head.
        Reason::UnionSourceMember { because, .. } => render_reason_chain(store, because),
        // Structural wrappers: emit the "…are incompatible." line plus the nested
        // cause, starting one indent level in (the headline sits at level 0).
        Reason::Property { .. } | Reason::Parameter { .. } | Reason::ReturnType { .. } => {
            reason_lines(store, head, 1)
        }
        // The headline already states the `S[]`/`T[]` mismatch, so elaborate the
        // element's own cause one level in; union elements skip redundant wrappers.
        Reason::ArrayElement { because, .. } => element_reason_lines(store, because, 1),
        // M18: a tuple-element mismatch — the headline already states the `[…]`/`[…]`
        // mismatch, so the elaboration is the offending **element's** own cause, one
        // indent level in (a union element descends straight into its member, like the
        // array case). The position is implicit in the headline tuples.
        Reason::TupleElement { because, .. } => element_reason_lines(store, because, 1),
        // M19: an index-signature mismatch — the headline already states the
        // object/object mismatch, so the elaboration is the offending **value's** own
        // cause, one indent level in (a union value descends straight into its member,
        // like the array/tuple cases).
        Reason::IndexSignature { because, .. } => element_reason_lines(store, because, 1),
    }
}

/// Render an array element's own cause. Union-member causes descend straight into
/// the offending member to avoid a redundant wrapper; nested arrays and structural
/// causes use the normal reason renderer.
fn element_reason_lines(store: &Store, cause: &Reason, depth: usize) -> Vec<String> {
    match cause {
        // A union element: the offending member is the cause; descend into it so a
        // scalar member renders one line (its own), not two identical ones.
        Reason::UnionSourceMember { because, .. } => element_reason_lines(store, because, depth),
        // Everything else renders normally (a nested `ArrayElement` prints its array
        // line and recurses; a leaf prints the leaf line).
        other => reason_lines(store, other, depth),
    }
}

/// Render `reason` and its nested causes as indented lines, leaf last. Every
/// variant is handled exhaustively.
fn reason_lines(store: &Store, reason: &Reason, depth: usize) -> Vec<String> {
    let indent = REASON_INDENT.repeat(depth);
    match reason {
        // The base mismatch. Source widened (literal → base) to match the headline
        // rendering convention; target as-is.
        Reason::Leaf { src, tgt } => {
            let src = render_type(store, *src, /* widen */ true);
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )]
        }
        // A present-but-incompatible property: announce it, then nest its cause.
        Reason::Property { name, because, .. } => {
            let mut lines = vec![format!(
                "{indent}Types of property '{name}' are incompatible."
            )];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // A required target property the source lacks (terminal).
        Reason::MissingProperty { name, tgt, .. } => {
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!(
                "{indent}Property '{name}' is missing in type '{tgt}'."
            )]
        }
        // A contravariantly-incompatible parameter: name it (from the source
        // signature when available, else by position), then nest its cause.
        Reason::Parameter {
            index,
            src,
            tgt,
            because,
        } => {
            let src_name = parameter_name_at(store, *src, *index);
            let tgt_name = parameter_name_at(store, *tgt, *index);
            let header = match (src_name, tgt_name) {
                (Some(s), Some(t)) if s == t => {
                    format!("{indent}Types of parameters '{s}' are incompatible.")
                }
                (Some(s), Some(t)) => {
                    format!("{indent}Types of parameters '{s}' and '{t}' are incompatible.")
                }
                _ => format!(
                    "{indent}Types of parameters at position {} are incompatible.",
                    index + 1
                ),
            };
            let mut lines = vec![header];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // Covariantly-incompatible return types: announce, then nest the cause.
        Reason::ReturnType { because, .. } => {
            let mut lines = vec![format!(
                "{indent}Call signature return types are incompatible."
            )];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // A union source whose member fails: announce the member, then nest its
        // cause. (At the chain head this arm is bypassed by `render_reason_chain`,
        // which descends straight into `because`; this arm renders a union nested
        // *inside* another reason.)
        Reason::UnionSourceMember {
            member,
            tgt,
            because,
            ..
        } => {
            let member = render_type(store, *member, /* widen */ true);
            let tgt = render_type(store, *tgt, /* widen */ false);
            let mut lines = vec![format!(
                "{indent}Type '{member}' is not assignable to type '{tgt}'."
            )];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // A source assignable to no member of a union target (terminal).
        Reason::NoUnionMember { src, tgt } => {
            let src = render_type(store, *src, /* widen */ true);
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )]
        }
        // Mismatched arity (terminal). M3 has no optional/rest params, so a
        // surplus source parameter is genuinely unsatisfiable.
        Reason::ParameterCount { src, tgt } => {
            let src = render_type(store, *src, /* widen */ false);
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!(
                "{indent}Type '{src}' provides more parameters than type '{tgt}' expects."
            )]
        }
        // An array whose element fails (M17): announce the two array types, then
        // nest the element's cause. (At the chain head this arm is bypassed by
        // `render_reason_chain`, which descends straight into `because`; this arm
        // renders an array mismatch nested *inside* another reason.)
        Reason::ArrayElement { src, tgt, because } => {
            let src = render_type(store, *src, /* widen */ false);
            let tgt = render_type(store, *tgt, /* widen */ false);
            let mut lines = vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // A tuple length mismatch (M18, terminal). The two tuple types render in full;
        // the cause is that their lengths differ (`[A, B]` vs `[A]`).
        Reason::TupleLength { src, tgt } => {
            let src = render_type(store, *src, /* widen */ false);
            let tgt = render_type(store, *tgt, /* widen */ false);
            vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )]
        }
        // A tuple whose element at a position fails (M18): announce the two tuple
        // types, then nest the element's cause. (At the chain head this arm is
        // bypassed by `render_reason_chain`, which descends straight into `because`;
        // this arm renders a tuple mismatch nested *inside* another reason.)
        Reason::TupleElement {
            src, tgt, because, ..
        } => {
            let src = render_type(store, *src, /* widen */ false);
            let tgt = render_type(store, *tgt, /* widen */ false);
            let mut lines = vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
        // Nested index-signature mismatch: announce the object pair, then nest the
        // value cause. Chain heads descend straight into `because`.
        Reason::IndexSignature { src, tgt, because } => {
            let src = render_type(store, *src, /* widen */ false);
            let tgt = render_type(store, *tgt, /* widen */ false);
            let mut lines = vec![format!(
                "{indent}Type '{src}' is not assignable to type '{tgt}'."
            )];
            lines.extend(reason_lines(store, because, depth + 1));
            lines
        }
    }
}
