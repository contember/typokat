//! Type rendering for diagnostic messages (the corpus display format).

use crate::types::repr::TypeTag;
use crate::types::store::{Store, TypeId};

/// The name of the parameter at `index` of a function-typed id, if `id` is a
/// function type with that position. Used to phrase a `Parameter` reason; falls
/// back to `None` (positional phrasing) for anything else.
pub(super) fn parameter_name_at(store: &Store, id: TypeId, index: usize) -> Option<String> {
    let func = store.function_type(id)?;
    func.params.get(index).map(|p| p.name.clone())
}

/// Render a type per the corpus display format. `widen` applies only to the
/// top-level source side of assignability messages; relation logic still uses the
/// literal type. Recursive object expansions are cycle-safe and print `...`.
pub fn render_type(store: &Store, id: TypeId, widen: bool) -> String {
    let mut rendering: Vec<TypeId> = Vec::new();
    render_type_inner(store, id, widen, &mut rendering)
}

/// Cycle-safe core of [`render_type`]. `rendering` tracks object ids on the call
/// stack; re-entry emits `...`, even through union/function members.
fn render_type_inner(
    store: &Store,
    id: TypeId,
    widen: bool,
    rendering: &mut Vec<TypeId>,
) -> String {
    match store.tag(id) {
        TypeTag::Intrinsic => store
            .intrinsic_kind(id)
            .map(|k| k.display_name().to_string())
            // Defensive fallback; an intrinsic always has a kind.
            .unwrap_or_else(|| "unknown".to_string()),
        TypeTag::Literal => {
            let value = store.literal_value(id);
            match value {
                Some(lit) if widen => lit.base_kind().display_name().to_string(),
                Some(lit) => render_literal(lit),
                // Defensive fallback; a literal always has a value.
                None => "unknown".to_string(),
            }
        }
        // Object: `{ a: number; b: string }` — members in stored (canonical)
        // order, `; `-separated (README "Type display format").
        TypeTag::Object => {
            // Break a cycle: a recursive object already being rendered is `...`.
            if rendering.contains(&id) {
                return "...".to_string();
            }
            match store.object_type(id) {
                Some(obj)
                    if obj.properties.is_empty()
                        && obj.string_index.is_none()
                        && obj.number_index.is_none()
                        && obj.call_signatures.is_empty()
                        && obj.construct_signatures.is_empty() =>
                {
                    "{}".to_string()
                }
                Some(obj) => {
                    rendering.push(id);
                    // M19: index signatures render as `[x: string]: T` / `[x: number]: T`,
                    // listed before the named members (a stable, tsc-like form;
                    // object-target messages are asserted code-only in the corpus).
                    let mut members: Vec<String> = Vec::new();
                    if let Some(v) = obj.string_index {
                        members.push(format!(
                            "[x: string]: {}",
                            render_type_inner(store, v, false, rendering)
                        ));
                    }
                    if let Some(v) = obj.number_index {
                        members.push(format!(
                            "[x: number]: {}",
                            render_type_inner(store, v, false, rendering)
                        ));
                    }
                    members.extend(
                        obj.call_signatures
                            .iter()
                            .map(|&signature| render_call_signature(store, signature, rendering)),
                    );
                    members.extend(
                        obj.construct_signatures.iter().map(|&signature| {
                            render_construct_signature(store, signature, rendering)
                        }),
                    );
                    members.extend(obj.properties.iter().map(|p| {
                        format!(
                            "{}: {}",
                            p.name,
                            render_type_inner(store, p.ty, false, rendering)
                        )
                    }));
                    rendering.pop();
                    format!("{{ {} }}", members.join("; "))
                }
                // Defensive fallback; an object always has a side-table entry.
                None => "<unsupported>".to_string(),
            }
        }
        // Function: `(x: number) => string` — parameters as `name: type`,
        // `, `-separated, always parenthesized, then ` => ` and the return type
        // (README "Type display format").
        TypeTag::Function => match store.function_type(id) {
            Some(func) => {
                let (params, ret) = render_function_parts(store, func, rendering);
                let type_params = render_generic_type_params(store, func, rendering);
                format!("{}({}) => {}", type_params, params.join(", "), ret)
            }
            // Defensive fallback; a function always has a side-table entry.
            None => "<unsupported>".to_string(),
        },
        // Union: `number | string` — members in stored (canonical, TypeId-sorted)
        // order, ` | `-separated (README "Type display format"). That order is
        // intern-order dependent, so union-typed targets are asserted code-only.
        TypeTag::Union => match store.union_members(id) {
            Some(members) => {
                let parts: Vec<String> = members
                    .iter()
                    .map(|&m| {
                        let rendered = render_type_inner(store, m, false, rendering);
                        // tsc parenthesizes an intersection element inside a union
                        // (`(A & B) | C`), so the `&`/`|` precedence reads correctly.
                        if store.tag(m) == TypeTag::Intersection {
                            format!("({rendered})")
                        } else {
                            rendered
                        }
                    })
                    .collect();
                parts.join(" | ")
            }
            // Defensive fallback; a union always has a side-table entry.
            None => "<unsupported>".to_string(),
        },
        // Intersection (M31): `A & B` — members in stored (canonical, TypeId-sorted)
        // order, ` & `-separated. That order is intern-order dependent, so
        // intersection-typed targets are asserted code-only. A **union** element is
        // parenthesized (`(A | B) & C`) so `&`/`|` precedence reads correctly.
        TypeTag::Intersection => match store.intersection_members(id) {
            Some(members) => {
                let parts: Vec<String> = members
                    .iter()
                    .map(|&m| {
                        let rendered = render_type_inner(store, m, false, rendering);
                        if matches!(store.tag(m), TypeTag::Union | TypeTag::Function) {
                            format!("({rendered})")
                        } else {
                            rendered
                        }
                    })
                    .collect();
                parts.join(" & ")
            }
            // Defensive fallback; an intersection always has a side-table entry.
            None => "<unsupported>".to_string(),
        },
        // Type parameter (M9): render its source name (`T`). A type parameter only
        // surfaces in a message for an *uninstantiated* generic (out of the M9
        // fixtures' explicit-args path, where every parameter is substituted away);
        // the name is display-only and not part of the type's identity.
        TypeTag::TypeParam => store
            .type_param(id)
            .map(|p| p.name.clone())
            // Defensive fallback; a type parameter always has a side-table entry.
            .unwrap_or_else(|| "unknown".to_string()),
        // Array (M17): `<elem>[]`. Parenthesize union/function elements where bare
        // postfix `[]` would bind ambiguously, matching tsc display.
        TypeTag::Array => match store.array_type(id) {
            Some(array) => {
                let elem = render_type_inner(store, array.element, false, rendering);
                if array_element_needs_parens(store, array.element) {
                    format!("({elem})[]")
                } else {
                    format!("{elem}[]")
                }
            }
            // Defensive fallback; an array always has a side-table entry.
            None => "<unsupported>".to_string(),
        },
        // Tuple (M18): `[A, B]` — source-order elements, `, `-separated. The empty
        // tuple renders as `[]`; brackets already delimit every element.
        TypeTag::Tuple => match store.tuple_type(id) {
            Some(tuple) => {
                let elems = render_tuple_parts(store, tuple, rendering);
                format!("[{}]", elems.join(", "))
            }
            // Defensive fallback; a tuple always has a side-table entry.
            None => "<unsupported>".to_string(),
        },
        TypeTag::Readonly => match store.readonly_operand(id) {
            Some(operand) => {
                let rendered = render_type_inner(store, operand, false, rendering);
                format!("readonly {rendered}")
            }
            None => "<unsupported>".to_string(),
        },
        // Conditional (M25): `C extends E ? T : F`. Conditional-typed targets are
        // asserted code-only, so the exact form only has to be stable.
        TypeTag::Conditional => match store.conditional_type(id) {
            Some(cond) => {
                if rendering.contains(&id) {
                    return "...".to_string();
                }
                rendering.push(id);
                let check = render_type_inner(store, cond.check, false, rendering);
                let extends = render_type_inner(store, cond.extends_ty, false, rendering);
                let true_branch = render_type_inner(store, cond.true_branch, false, rendering);
                let false_branch = render_type_inner(store, cond.false_branch, false, rendering);
                rendering.pop();
                format!("{check} extends {extends} ? {true_branch} : {false_branch}")
            }
            None => "<unsupported>".to_string(),
        },
        // Lazy instantiation (M25): render as the applied base with its arguments. Only
        // ever surfaces for a still-deferred recursive conditional alias; asserted
        // code-only, so a stable form suffices.
        TypeTag::Instantiation => match store.instantiation_type(id) {
            Some(inst) => {
                if rendering.contains(&id) {
                    return "...".to_string();
                }
                rendering.push(id);
                let args: Vec<String> = inst
                    .args
                    .iter()
                    .map(|(_, arg)| render_type_inner(store, *arg, false, rendering))
                    .collect();
                // M28 round 3: a NAMED template base (a reserved conditional/mapped
                // alias row) renders by its alias name — `Extract<K, string>` — never
                // the raw body; intrinsic markers already name themselves via
                // `IntrinsicKind::display_name`.
                let base = match store.template_name(inst.base) {
                    Some(name) => name.to_string(),
                    None => render_type_inner(store, inst.base, false, rendering),
                };
                rendering.pop();
                format!("{base}<{}>", args.join(", "))
            }
            None => "<unsupported>".to_string(),
        },
        // Infer binder (M25): `infer` de Bruijn index. Only surfaces inside a deferred
        // conditional's rendered form; a stable placeholder suffices.
        TypeTag::Infer => match store.infer_index(id) {
            Some(index) => format!("infer#{index}"),
            None => "<unsupported>".to_string(),
        },
        // Mapped type (M26): `{ [K in S]: V }` (with readonly/optional modifiers). Only
        // surfaces for a still-deferred (generic) mapped type; mapped-typed targets are
        // asserted code-only in the corpus, so a stable, sanely-parenthesized form
        // suffices.
        TypeTag::Mapped => match store.mapped_type(id) {
            Some(mapped) => {
                if rendering.contains(&id) {
                    return "...".to_string();
                }
                rendering.push(id);
                let source = render_type_inner(store, mapped.key_source, false, rendering);
                let source = if mapped.homomorphic {
                    format!("keyof {source}")
                } else {
                    source
                };
                let value = render_type_inner(store, mapped.value_template, false, rendering);
                rendering.pop();
                let readonly = render_modifier(mapped.readonly_modifier, "readonly ");
                let optional = render_modifier(mapped.optional_modifier, "?");
                format!("{{ {readonly}[K in {source}]{optional}: {value} }}")
            }
            None => "<unsupported>".to_string(),
        },
        // Mapped-value placeholder (M26): the source property value `T[K]`. Only
        // surfaces inside a deferred mapped type's rendered form; a stable placeholder
        // suffices.
        TypeTag::MappedValue => "T[K]".to_string(),
        // Deferred keyof (M28): `keyof <operand>`. Only surfaces for a still-deferred
        // node (a free type parameter operand); keyof-typed targets are asserted
        // code-only in the corpus, so a stable form suffices.
        TypeTag::Keyof => match store.keyof_operand(id) {
            Some(operand) => {
                if rendering.contains(&id) {
                    return "...".to_string();
                }
                rendering.push(id);
                let rendered = render_type_inner(store, operand, false, rendering);
                rendering.pop();
                format!("keyof {rendered}")
            }
            None => "<unsupported>".to_string(),
        },
        // Template literal type (M27): the backtick form `` `a${T}b` `` — text
        // segments interleaved with `${hole}`. Template-typed targets are asserted
        // code-only, so the exact form only has to be stable.
        TypeTag::Template => match store.template_type(id) {
            Some(template) => {
                if rendering.contains(&id) {
                    return "...".to_string();
                }
                rendering.push(id);
                let mut out = String::from("`");
                for (i, hole) in template.holes.iter().enumerate() {
                    out.push_str(template.texts.get(i).map(String::as_str).unwrap_or(""));
                    out.push_str("${");
                    out.push_str(&render_type_inner(store, *hole, false, rendering));
                    out.push('}');
                }
                out.push_str(
                    template
                        .texts
                        .get(template.holes.len())
                        .map(String::as_str)
                        .unwrap_or(""),
                );
                out.push('`');
                rendering.pop();
                out
            }
            None => "<unsupported>".to_string(),
        },
    }
}

/// Render a mapped-type modifier for display (M26). A `readonly`/`?` prefix or suffix
/// is emitted for `Add`, an explicit `-` for `Remove`, and nothing for `Keep`. Only
/// surfaces for a deferred mapped type (asserted code-only), so the exact form only has
/// to be stable.
fn render_modifier(op: crate::types::repr::ModifierOp, token: &str) -> String {
    use crate::types::repr::ModifierOp;
    match op {
        ModifierOp::Keep => String::new(),
        ModifierOp::Add => token.to_string(),
        ModifierOp::Remove => format!("-{}", token.trim()),
    }
}

fn render_call_signature(store: &Store, id: TypeId, rendering: &mut Vec<TypeId>) -> String {
    match store.function_type(id) {
        Some(func) => {
            let (params, ret) = render_function_parts(store, func, rendering);
            let type_params = render_generic_type_params(store, func, rendering);
            format!("{}({}): {}", type_params, params.join(", "), ret)
        }
        None => "<unsupported>".to_string(),
    }
}

fn render_construct_signature(store: &Store, id: TypeId, rendering: &mut Vec<TypeId>) -> String {
    match store.function_type(id) {
        Some(func) => {
            let (params, ret) = render_function_parts(store, func, rendering);
            let type_params = render_generic_type_params(store, func, rendering);
            format!("new {}({}): {}", type_params, params.join(", "), ret)
        }
        None => "<unsupported>".to_string(),
    }
}

fn render_function_parts(
    store: &Store,
    func: &crate::types::repr::FunctionType,
    rendering: &mut Vec<TypeId>,
) -> (Vec<String>, String) {
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            let name = render_parameter_name(p);
            format!(
                "{}: {}",
                name,
                render_type_inner(store, p.ty, false, rendering)
            )
        })
        .collect();
    let ret = render_type_inner(store, func.ret, false, rendering);
    (params, ret)
}

fn render_generic_type_params(
    store: &Store,
    func: &crate::types::repr::FunctionType,
    rendering: &mut Vec<TypeId>,
) -> String {
    if func.type_params.is_empty() {
        return String::new();
    }
    let params: Vec<String> = func
        .type_params
        .iter()
        .map(|param| {
            let mut rendered = store.type_param_name(param.id).unwrap_or("T").to_string();
            if let Some(constraint) = param.constraint {
                rendered.push_str(" extends ");
                rendered.push_str(&render_type_inner(store, constraint, false, rendering));
            }
            if let Some(default) = param.default {
                rendered.push_str(" = ");
                rendered.push_str(&render_type_inner(store, default, false, rendering));
            }
            rendered
        })
        .collect();
    format!("<{}>", params.join(", "))
}

fn render_parameter_name(param: &crate::types::repr::ParameterType) -> String {
    if param.rest {
        format!("...{}", param.name)
    } else if param.optional {
        format!("{}?", param.name)
    } else {
        param.name.clone()
    }
}

fn render_tuple_parts(
    store: &Store,
    tuple: &crate::types::repr::TupleType,
    rendering: &mut Vec<TypeId>,
) -> Vec<String> {
    let mut parts = Vec::with_capacity(tuple.elements.len() + usize::from(tuple.rest.is_some()));
    for (index, &element) in tuple.elements.iter().enumerate() {
        if let Some(rest) = tuple.rest {
            if rest.position == index {
                parts.push(render_tuple_rest(store, rest, rendering));
            }
        }
        parts.push(render_type_inner(store, element, false, rendering));
    }
    if let Some(rest) = tuple.rest {
        if rest.position >= tuple.elements.len() {
            parts.push(render_tuple_rest(store, rest, rendering));
        }
    }
    parts
}

fn render_tuple_rest(
    store: &Store,
    rest: crate::types::repr::TupleRestType,
    rendering: &mut Vec<TypeId>,
) -> String {
    format!("...{}", render_type_inner(store, rest.ty, false, rendering))
}

/// Whether an array element type must be parenthesized before postfix `[]`.
/// Unions and functions would bind ambiguously; other element types render bare.
fn array_element_needs_parens(store: &Store, element: TypeId) -> bool {
    matches!(
        store.tag(element),
        TypeTag::Union | TypeTag::Function | TypeTag::Intersection | TypeTag::Readonly
    )
}

fn render_literal(lit: &crate::types::repr::LiteralValue) -> String {
    use crate::types::repr::LiteralValue;
    match lit {
        LiteralValue::String(s) => format!("\"{}\"", escape_string_literal(s)),
        LiteralValue::Boolean(b) => b.to_string(),
        LiteralValue::Number(n) => {
            // Render integers without a trailing `.0` to match `tsc`'s literal
            // display (`1`, not `1.0`).
            if n.fract() == 0.0 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
    }
}

/// Escape control characters (and `\`/`"`) inside a string-literal type body, as tsc
/// renders them — a raw newline would otherwise split a diagnostic across lines and,
/// after WU3, could inject a phantom `incomplete[…]` line into an exit-3 identity list.
fn escape_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod escape_tests {
    use super::escape_string_literal;

    /// A hostile string-literal type carrying a raw newline (and a fabricated
    /// `incomplete[…]` payload) renders on ONE line with the newline escaped, so it
    /// can never pollute the incomplete-identity list of an exit-3 official-suite run.
    #[test]
    fn hostile_literal_renders_on_one_line() {
        let rendered = format!("\"{}\"", escape_string_literal("x\nincomplete[evil/id]"));
        assert!(!rendered.contains('\n'), "no raw newline: {rendered:?}");
        assert_eq!(rendered, "\"x\\nincomplete[evil/id]\"");
    }

    /// Backslash, quote, and other control chars escape too.
    #[test]
    fn escapes_backslash_quote_and_controls() {
        assert_eq!(escape_string_literal("a\\b\"c\t\r"), "a\\\\b\\\"c\\t\\r");
        assert_eq!(escape_string_literal("\u{7}"), "\\u{7}");
    }
}
