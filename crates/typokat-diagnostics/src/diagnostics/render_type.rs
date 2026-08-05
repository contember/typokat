//! Type rendering for diagnostic messages (the corpus display format).

use crate::types::repr::{
    DeclaredRecipeId, DeclaredRecipeNode, LiteralValue, TypeParamId, TypeTag,
};
use crate::types::store::{Store, TypeId};
use rustc_hash::FxHashMap;

/// The name of the parameter at `index` of a function-typed id, if `id` is a
/// function type with that position. Used to phrase a `Parameter` reason; falls
/// back to `None` (positional phrasing) for anything else.
pub(super) fn parameter_name_at(store: &Store, id: TypeId, index: usize) -> Option<String> {
    let func = store.function_type(id)?;
    func.params.get(index).map(|p| p.name.clone())
}

const DISPLAY_CHAR_LIMIT: usize = 320;
const DISPLAY_DEPTH_LIMIT: usize = 64;
const ELLIPSIS: &str = "...";

struct RenderContext {
    output: String,
    rendering: Vec<TypeId>,
    overlays: Vec<FxHashMap<TypeParamId, RenderBinding>>,
    overlay_limit: usize,
    remaining_chars: usize,
    depth: usize,
    truncated: bool,
}

#[derive(Clone, Copy)]
enum RenderBinding {
    Type(TypeId),
    Recipe(DeclaredRecipeId),
}

struct OverlayFrame {
    previous_limit: usize,
    hidden: Vec<FxHashMap<TypeParamId, RenderBinding>>,
}

impl RenderContext {
    fn new() -> Self {
        Self {
            output: String::new(),
            rendering: Vec::new(),
            overlays: Vec::new(),
            overlay_limit: 0,
            remaining_chars: DISPLAY_CHAR_LIMIT,
            depth: 0,
            truncated: false,
        }
    }

    fn append(&mut self, text: &str) {
        if self.truncated {
            return;
        }

        // Inspect at most the remaining budget plus one character: a single huge
        // identifier or literal must not make display work proportional to its size.
        let text_len = text.chars().take(self.remaining_chars + 1).count();
        if text_len <= self.remaining_chars {
            self.output.push_str(text);
            self.remaining_chars -= text_len;
            return;
        }

        let target_prefix_len = DISPLAY_CHAR_LIMIT - ELLIPSIS.len();
        let current_len = DISPLAY_CHAR_LIMIT - self.remaining_chars;
        if current_len > target_prefix_len {
            for _ in target_prefix_len..current_len {
                self.output.pop();
            }
        } else {
            self.output
                .extend(text.chars().take(target_prefix_len - current_len));
        }
        self.output.push_str(ELLIPSIS);
        self.remaining_chars = 0;
        self.truncated = true;
    }

    fn stop_at_depth_limit(&mut self) {
        self.output.clear();
        self.output.push_str(ELLIPSIS);
        self.remaining_chars = DISPLAY_CHAR_LIMIT - ELLIPSIS.len();
        self.truncated = true;
    }
}

/// Render a type per the corpus display format. `widen` applies only to the
/// top-level source side of assignability messages; relation logic still uses the
/// literal type. Displays are cycle-safe and bounded by character and depth limits.
pub fn render_type(store: &Store, id: TypeId, widen: bool) -> String {
    let mut context = RenderContext::new();
    render_type_inner(store, id, widen, &mut context);
    context.output
}

/// Cycle-safe core of [`render_type`]. `context.rendering` tracks expandable ids on
/// the call stack; re-entry emits `...`, even through union/function members.
fn render_type_inner(store: &Store, id: TypeId, widen: bool, context: &mut RenderContext) {
    if context.truncated {
        return;
    }
    if context.depth >= DISPLAY_DEPTH_LIMIT {
        context.stop_at_depth_limit();
        return;
    }

    context.depth += 1;
    render_type_node(store, id, widen, context);
    context.depth -= 1;
}

fn render_type_node(store: &Store, id: TypeId, widen: bool, context: &mut RenderContext) {
    if render_overlay_binding(store, id, context) {
        return;
    }
    match store.tag(id) {
        TypeTag::Intrinsic => context.append(
            store
                .intrinsic_kind(id)
                .map(|k| k.display_name())
                // Defensive fallback; an intrinsic always has a kind.
                .unwrap_or("unknown"),
        ),
        TypeTag::Literal => {
            let value = store.literal_value(id);
            match value {
                Some(lit) if widen => context.append(lit.base_kind().display_name()),
                Some(lit) => render_literal(lit, context),
                // Defensive fallback; a literal always has a value.
                None => context.append("unknown"),
            }
        }
        // Object: `{ a: number; b: string }` — members in stored (canonical)
        // order, `; `-separated (README "Type display format").
        TypeTag::Object => {
            // Break a cycle: a recursive object already being rendered is `...`.
            if context.rendering.contains(&id) {
                context.append(ELLIPSIS);
                return;
            }
            match store.object_type(id) {
                Some(obj)
                    if obj.properties.is_empty()
                        && obj.string_index.is_none()
                        && obj.number_index.is_none()
                        && obj.call_signatures.is_empty()
                        && obj.construct_signatures.is_empty() =>
                {
                    context.append("{}")
                }
                Some(obj) => {
                    context.rendering.push(id);
                    context.append("{ ");
                    // M19: index signatures render as `[x: string]: T` / `[x: number]: T`,
                    // listed before the named members (a stable, tsc-like form;
                    // object-target messages are asserted code-only in the corpus).
                    let mut needs_separator = false;
                    if let Some(v) = obj.string_index {
                        context.append("[x: string]: ");
                        render_type_inner(store, v, false, context);
                        needs_separator = true;
                    }
                    if !context.truncated {
                        if let Some(v) = obj.number_index {
                            if needs_separator {
                                context.append("; ");
                            }
                            context.append("[x: number]: ");
                            render_type_inner(store, v, false, context);
                            needs_separator = true;
                        }
                    }
                    for &signature in &obj.call_signatures {
                        if context.truncated {
                            break;
                        }
                        if needs_separator {
                            context.append("; ");
                        }
                        render_call_signature(store, signature, context);
                        needs_separator = true;
                    }
                    for &signature in &obj.construct_signatures {
                        if context.truncated {
                            break;
                        }
                        if needs_separator {
                            context.append("; ");
                        }
                        render_construct_signature(store, signature, context);
                        needs_separator = true;
                    }
                    for property in &obj.properties {
                        if context.truncated {
                            break;
                        }
                        if needs_separator {
                            context.append("; ");
                        }
                        context.append(&property.key.to_string());
                        context.append(": ");
                        render_type_inner(store, property.ty, false, context);
                        needs_separator = true;
                    }
                    context.rendering.pop();
                    context.append(" }");
                }
                // Defensive fallback; an object always has a side-table entry.
                None => context.append("<unsupported>"),
            }
        }
        // Function: `(x: number) => string` — parameters as `name: type`,
        // `, `-separated, always parenthesized, then ` => ` and the return type
        // (README "Type display format").
        TypeTag::Function => match store.function_type(id) {
            Some(func) => {
                render_generic_type_params(store, func, context);
                context.append("(");
                render_function_params(store, func, context);
                context.append(") => ");
                render_type_inner(store, func.ret, false, context);
            }
            // Defensive fallback; a function always has a side-table entry.
            None => context.append("<unsupported>"),
        },
        // Union: `number | string` — members in stored (canonical, TypeId-sorted)
        // order, except string-literal-only unions use payload order for stable display.
        TypeTag::Union => match store.union_members(id) {
            Some(members) => {
                let string_members = members
                    .iter()
                    .copied()
                    .map(|member| match store.literal_value(member) {
                        Some(LiteralValue::String(value)) => Some((value.as_str(), member)),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(mut string_members) = string_members {
                    string_members.sort_by(|left, right| left.0.cmp(right.0));
                    render_union_members(
                        store,
                        string_members.into_iter().map(|(_, member)| member),
                        context,
                    );
                } else {
                    render_union_members(store, members.iter().copied(), context);
                }
            }
            // Defensive fallback; a union always has a side-table entry.
            None => context.append("<unsupported>"),
        },
        // Intersection (M31): `A & B` — members in stored (canonical, TypeId-sorted)
        // order, ` & `-separated. That order is intern-order dependent, so
        // intersection-typed targets are asserted code-only. A **union** element is
        // parenthesized (`(A | B) & C`) so `&`/`|` precedence reads correctly.
        TypeTag::Intersection => match store.intersection_members(id) {
            Some(members) => {
                for (index, &member) in members.iter().enumerate() {
                    if context.truncated {
                        break;
                    }
                    if index > 0 {
                        context.append(" & ");
                    }
                    let parenthesized = matches!(
                        render_tag(store, member, context),
                        TypeTag::Union | TypeTag::Function
                    );
                    if parenthesized {
                        context.append("(");
                    }
                    render_type_inner(store, member, false, context);
                    if parenthesized {
                        context.append(")");
                    }
                }
            }
            // Defensive fallback; an intersection always has a side-table entry.
            None => context.append("<unsupported>"),
        },
        // Type parameter (M9): render its source name (`T`). A type parameter only
        // surfaces in a message for an *uninstantiated* generic (out of the M9
        // fixtures' explicit-args path, where every parameter is substituted away);
        // the name is display-only and not part of the type's identity.
        TypeTag::TypeParam => context.append(
            store
                .type_param(id)
                .map(|p| p.name.as_str())
                // Defensive fallback; a type parameter always has a side-table entry.
                .unwrap_or("unknown"),
        ),
        // Array (M17): `<elem>[]`. Parenthesize union/function elements where bare
        // postfix `[]` would bind ambiguously, matching tsc display.
        TypeTag::Array => match store.array_type(id) {
            Some(array) => {
                let parenthesized = array_element_needs_parens(store, array.element, context);
                if parenthesized {
                    context.append("(");
                }
                render_type_inner(store, array.element, false, context);
                if parenthesized {
                    context.append(")");
                }
                context.append("[]");
            }
            // Defensive fallback; an array always has a side-table entry.
            None => context.append("<unsupported>"),
        },
        // Tuple (M18): `[A, B]` — source-order elements, `, `-separated. The empty
        // tuple renders as `[]`; brackets already delimit every element.
        TypeTag::Tuple => match store.tuple_type(id) {
            Some(tuple) => {
                context.append("[");
                render_tuple_parts(store, tuple, context);
                context.append("]");
            }
            // Defensive fallback; a tuple always has a side-table entry.
            None => context.append("<unsupported>"),
        },
        TypeTag::Readonly => match store.readonly_operand(id) {
            Some(operand) => {
                context.append("readonly ");
                render_type_inner(store, operand, false, context);
            }
            None => context.append("<unsupported>"),
        },
        // Conditional (M25): `C extends E ? T : F`. Conditional-typed targets are
        // asserted code-only, so the exact form only has to be stable.
        TypeTag::Conditional => match store.conditional_type(id) {
            Some(cond) => {
                if context.rendering.contains(&id) {
                    context.append(ELLIPSIS);
                    return;
                }
                context.rendering.push(id);
                render_type_inner(store, cond.check, false, context);
                context.append(" extends ");
                render_type_inner(store, cond.extends_ty, false, context);
                context.append(" ? ");
                render_type_inner(store, cond.true_branch, false, context);
                context.append(" : ");
                render_type_inner(store, cond.false_branch, false, context);
                context.rendering.pop();
            }
            None => context.append("<unsupported>"),
        },
        // Lazy instantiation (M25): render as the applied base with its arguments. Only
        // ever surfaces for a still-deferred recursive conditional alias; asserted
        // code-only, so a stable form suffices.
        TypeTag::Instantiation => match store.instantiation_type(id) {
            Some(inst) => {
                if context.rendering.contains(&id) {
                    context.append(ELLIPSIS);
                    return;
                }
                context.rendering.push(id);
                // M28 round 3: a NAMED template base (a reserved conditional/mapped
                // alias row) renders by its alias name — `Extract<K, string>` — never
                // the raw body; intrinsic markers already name themselves via
                // `IntrinsicKind::display_name`.
                match store.template_name(inst.base) {
                    Some(name) => context.append(name),
                    None => render_type_inner(store, inst.base, false, context),
                }
                context.append("<");
                for (index, (_, arg)) in inst.args.iter().enumerate() {
                    if context.truncated {
                        break;
                    }
                    if index > 0 {
                        context.append(", ");
                    }
                    render_type_inner(store, *arg, false, context);
                }
                context.rendering.pop();
                context.append(">");
            }
            None => context.append("<unsupported>"),
        },
        TypeTag::ClassInstance => match store.class_instance_type(id) {
            Some(instance) => {
                if context.rendering.contains(&id) {
                    context.append(ELLIPSIS);
                    return;
                }
                context.rendering.push(id);
                context.append(&format!("class#{}", instance.class.0));
                if !instance.args.is_empty() {
                    context.append("<");
                    for (index, arg) in instance.args.iter().enumerate() {
                        if context.truncated {
                            break;
                        }
                        if index > 0 {
                            context.append(", ");
                        }
                        render_type_inner(store, *arg, false, context);
                    }
                    context.append(">");
                }
                context.rendering.pop();
            }
            None => context.append("<unsupported>"),
        },
        // Infer binder (M25): `infer` de Bruijn index. Only surfaces inside a deferred
        // conditional's rendered form; a stable placeholder suffices.
        TypeTag::Infer => match store.infer_index(id) {
            Some(index) => context.append(&format!("infer#{index}")),
            None => context.append("<unsupported>"),
        },
        // Mapped type (M26): `{ [K in S]: V }` (with readonly/optional modifiers). Only
        // surfaces for a still-deferred (generic) mapped type; mapped-typed targets are
        // asserted code-only in the corpus, so a stable, sanely-parenthesized form
        // suffices.
        TypeTag::Mapped => match store.mapped_type(id) {
            Some(mapped) => {
                if context.rendering.contains(&id) {
                    context.append(ELLIPSIS);
                    return;
                }
                context.rendering.push(id);
                let readonly = render_modifier(mapped.readonly_modifier, "readonly ");
                let optional = render_modifier(mapped.optional_modifier, "?");
                context.append("{ ");
                context.append(&readonly);
                context.append("[K in ");
                if mapped.homomorphic {
                    context.append("keyof ");
                }
                render_type_inner(store, mapped.key_source, false, context);
                context.append("]");
                context.append(&optional);
                context.append(": ");
                render_type_inner(store, mapped.value_template, false, context);
                context.rendering.pop();
                context.append(" }");
            }
            None => context.append("<unsupported>"),
        },
        // Mapped-value placeholder (M26): the source property value `T[K]`. Only
        // surfaces inside a deferred mapped type's rendered form; a stable placeholder
        // suffices.
        TypeTag::MappedValue => context.append("T[K]"),
        // Deferred keyof (M28): `keyof <operand>`. Only surfaces for a still-deferred
        // node (a free type parameter operand); keyof-typed targets are asserted
        // code-only in the corpus, so a stable form suffices.
        TypeTag::Keyof => match store.keyof_operand(id) {
            Some(operand) => {
                if context.rendering.contains(&id) {
                    context.append(ELLIPSIS);
                    return;
                }
                context.rendering.push(id);
                context.append("keyof ");
                render_type_inner(store, operand, false, context);
                context.rendering.pop();
            }
            None => context.append("<unsupported>"),
        },
        TypeTag::DeferredIndexedAccess => match store.deferred_indexed_access_type(id) {
            Some(access) => {
                if context.rendering.contains(&id) {
                    context.append(ELLIPSIS);
                    return;
                }
                context.rendering.push(id);
                let parenthesized =
                    indexed_access_object_needs_parens(store, access.object, context);
                if parenthesized {
                    context.append("(");
                }
                render_type_inner(store, access.object, false, context);
                if parenthesized {
                    context.append(")");
                }
                context.append("[");
                render_type_inner(store, access.index, false, context);
                context.rendering.pop();
                context.append("]");
            }
            None => context.append("<unsupported>"),
        },
        // Template literal type (M27): the backtick form `` `a${T}b` `` — text
        // segments interleaved with `${hole}`. Template-typed targets are asserted
        // code-only, so the exact form only has to be stable.
        TypeTag::Template => match store.template_type(id) {
            Some(template) => {
                if context.rendering.contains(&id) {
                    context.append(ELLIPSIS);
                    return;
                }
                context.rendering.push(id);
                context.append("`");
                for (i, hole) in template.holes.iter().enumerate() {
                    if context.truncated {
                        break;
                    }
                    context.append(template.texts.get(i).map(String::as_str).unwrap_or(""));
                    context.append("${");
                    render_type_inner(store, *hole, false, context);
                    context.append("}");
                }
                context.append(
                    template
                        .texts
                        .get(template.holes.len())
                        .map(String::as_str)
                        .unwrap_or(""),
                );
                context.append("`");
                context.rendering.pop();
            }
            None => context.append("<unsupported>"),
        },
        TypeTag::Declared => match store.declared_type(id) {
            Some(declared) => {
                if context.rendering.contains(&id) {
                    context.append(ELLIPSIS);
                    return;
                }
                context.rendering.push(id);
                let overlay = declared
                    .mapper
                    .iter()
                    .map(|(parameter, ty)| (*parameter, RenderBinding::Type(*ty)))
                    .collect();
                let frame = push_overlay(context, overlay);
                render_declared_recipe(store, declared.recipe, context);
                pop_overlay(context, frame);
                context.rendering.pop();
            }
            None => context.append("<unsupported>"),
        },
    }
}

fn render_union_members(
    store: &Store,
    members: impl IntoIterator<Item = TypeId>,
    context: &mut RenderContext,
) {
    for (index, member) in members.into_iter().enumerate() {
        if context.truncated {
            break;
        }
        if index > 0 {
            context.append(" | ");
        }
        // tsc parenthesizes an intersection element inside a union (`(A & B) | C`).
        let parenthesized = render_tag(store, member, context) == TypeTag::Intersection;
        if parenthesized {
            context.append("(");
        }
        render_type_inner(store, member, false, context);
        if parenthesized {
            context.append(")");
        }
    }
}

fn render_overlay_binding(store: &Store, id: TypeId, context: &mut RenderContext) -> bool {
    let Some(parameter) = store.type_param(id) else {
        return false;
    };
    let Some((layer, binding)) = overlay_binding(context, parameter.id, context.overlay_limit)
    else {
        return false;
    };
    let previous_limit = context.overlay_limit;
    context.overlay_limit = layer;
    match binding {
        RenderBinding::Type(ty) => render_type_inner(store, ty, false, context),
        RenderBinding::Recipe(recipe) => render_declared_recipe(store, recipe, context),
    }
    context.overlay_limit = previous_limit;
    true
}

fn overlay_binding(
    context: &RenderContext,
    parameter: TypeParamId,
    limit: usize,
) -> Option<(usize, RenderBinding)> {
    context.overlays[..limit]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(layer, overlay)| {
            overlay
                .get(&parameter)
                .copied()
                .map(|binding| (layer, binding))
        })
}

fn push_overlay(
    context: &mut RenderContext,
    overlay: FxHashMap<TypeParamId, RenderBinding>,
) -> OverlayFrame {
    let previous_limit = context.overlay_limit;
    let hidden = context.overlays.split_off(previous_limit);
    context.overlays.push(overlay);
    context.overlay_limit = context.overlays.len();
    OverlayFrame {
        previous_limit,
        hidden,
    }
}

fn pop_overlay(context: &mut RenderContext, frame: OverlayFrame) {
    context.overlays.pop();
    context.overlays.extend(frame.hidden);
    context.overlay_limit = frame.previous_limit;
}

/// Cycle- and depth-safe wrapper for [`render_declared_recipe_node`]. The `Array` /
/// `Tuple` / `Readonly` arms recurse into a *recipe* rather than a type, so they never
/// reach [`render_type_inner`]'s guard — the budget has to be charged here instead
/// (backlog 87).
fn render_declared_recipe(store: &Store, recipe: DeclaredRecipeId, context: &mut RenderContext) {
    if context.truncated {
        return;
    }
    if context.depth >= DISPLAY_DEPTH_LIMIT {
        context.stop_at_depth_limit();
        return;
    }
    context.depth += 1;
    render_declared_recipe_node(store, recipe, context);
    context.depth -= 1;
}

fn render_declared_recipe_node(
    store: &Store,
    recipe: DeclaredRecipeId,
    context: &mut RenderContext,
) {
    let Some(recipe) = store.declared_recipe(recipe) else {
        context.append("<unsupported>");
        return;
    };
    match &recipe.node {
        DeclaredRecipeNode::Type(ty) => render_type_inner(store, *ty, false, context),
        DeclaredRecipeNode::Array(element) => {
            let parenthesized = declared_recipe_needs_array_parens(store, *element, context);
            if parenthesized {
                context.append("(");
            }
            render_declared_recipe(store, *element, context);
            if parenthesized {
                context.append(")");
            }
            context.append("[]");
        }
        DeclaredRecipeNode::Tuple { elements, rest } => {
            context.append("[");
            for (index, element) in elements.iter().enumerate() {
                if index > 0 {
                    context.append(", ");
                }
                if rest.is_some_and(|(position, _)| position == index) {
                    context.append("...");
                    render_declared_recipe(store, rest.expect("checked rest").1, context);
                    context.append(", ");
                }
                render_declared_recipe(store, *element, context);
            }
            if let Some((position, rest)) = rest {
                if *position == elements.len() {
                    if !elements.is_empty() {
                        context.append(", ");
                    }
                    context.append("...");
                    render_declared_recipe(store, *rest, context);
                }
            }
            context.append("]");
        }
        DeclaredRecipeNode::Readonly(operand) => {
            context.append("readonly ");
            render_declared_recipe(store, *operand, context);
        }
        DeclaredRecipeNode::Application {
            template,
            parameters,
            arguments,
        } => {
            let overlay = parameters
                .iter()
                .copied()
                .zip(arguments.iter().copied())
                .map(|(parameter, recipe)| (parameter, RenderBinding::Recipe(recipe)))
                .collect();
            let frame = push_overlay(context, overlay);
            render_type_inner(store, *template, false, context);
            pop_overlay(context, frame);
        }
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

fn render_call_signature(store: &Store, id: TypeId, context: &mut RenderContext) {
    match store.function_type(id) {
        Some(func) => {
            render_generic_type_params(store, func, context);
            context.append("(");
            render_function_params(store, func, context);
            context.append("): ");
            render_type_inner(store, func.ret, false, context);
        }
        None => context.append("<unsupported>"),
    }
}

fn render_construct_signature(store: &Store, id: TypeId, context: &mut RenderContext) {
    match store.function_type(id) {
        Some(func) => {
            context.append("new ");
            render_generic_type_params(store, func, context);
            context.append("(");
            render_function_params(store, func, context);
            context.append("): ");
            render_type_inner(store, func.ret, false, context);
        }
        None => context.append("<unsupported>"),
    }
}

fn render_function_params(
    store: &Store,
    func: &crate::types::repr::FunctionType,
    context: &mut RenderContext,
) {
    let mut needs_separator = false;
    if let Some(receiver) = func.receiver {
        context.append("this: ");
        render_type_inner(store, receiver, false, context);
        needs_separator = true;
    }
    for param in &func.params {
        if context.truncated {
            break;
        }
        if needs_separator {
            context.append(", ");
        }
        render_parameter_name(param, context);
        context.append(": ");
        render_type_inner(store, param.ty, false, context);
        needs_separator = true;
    }
}

fn render_generic_type_params(
    store: &Store,
    func: &crate::types::repr::FunctionType,
    context: &mut RenderContext,
) {
    if func.type_params.is_empty() {
        return;
    }
    context.append("<");
    for (index, param) in func.type_params.iter().enumerate() {
        if context.truncated {
            break;
        }
        if index > 0 {
            context.append(", ");
        }
        context.append(store.type_param_name(param.id).unwrap_or("T"));
        if let Some(constraint) = param.constraint {
            context.append(" extends ");
            render_type_inner(store, constraint, false, context);
        }
        if let Some(default) = param.default {
            context.append(" = ");
            render_type_inner(store, default, false, context);
        }
    }
    context.append(">");
}

fn render_parameter_name(param: &crate::types::repr::ParameterType, context: &mut RenderContext) {
    if param.rest {
        context.append("...");
    }
    context.append(&param.name);
    if param.optional && !param.rest {
        context.append("?");
    }
}

fn render_tuple_parts(
    store: &Store,
    tuple: &crate::types::repr::TupleType,
    context: &mut RenderContext,
) {
    let mut needs_separator = false;
    for (index, &element) in tuple.elements.iter().enumerate() {
        if context.truncated {
            break;
        }
        if let Some(rest) = tuple.rest {
            if rest.position == index {
                if needs_separator {
                    context.append(", ");
                }
                render_tuple_rest(store, rest, context);
                needs_separator = true;
            }
        }
        if needs_separator {
            context.append(", ");
        }
        render_type_inner(store, element, false, context);
        needs_separator = true;
    }
    if !context.truncated {
        if let Some(rest) = tuple.rest {
            if rest.position >= tuple.elements.len() {
                if needs_separator {
                    context.append(", ");
                }
                render_tuple_rest(store, rest, context);
            }
        }
    }
}

fn render_tuple_rest(
    store: &Store,
    rest: crate::types::repr::TupleRestType,
    context: &mut RenderContext,
) {
    context.append("...");
    render_type_inner(store, rest.ty, false, context);
}

/// Whether an array element type must be parenthesized before postfix `[]`.
/// Unions and functions would bind ambiguously; other element types render bare.
fn array_element_needs_parens(store: &Store, element: TypeId, context: &RenderContext) -> bool {
    matches!(
        render_tag(store, element, context),
        TypeTag::Union | TypeTag::Function | TypeTag::Intersection | TypeTag::Readonly
    )
}

fn declared_recipe_needs_array_parens(
    store: &Store,
    recipe: DeclaredRecipeId,
    context: &RenderContext,
) -> bool {
    matches!(
        declared_recipe_tag(store, recipe, context, context.overlay_limit, context.depth),
        TypeTag::Union | TypeTag::Function | TypeTag::Intersection | TypeTag::Readonly
    )
}

/// Postfix indexed access must delimit lower-precedence object forms.
fn indexed_access_object_needs_parens(
    store: &Store,
    object: TypeId,
    context: &RenderContext,
) -> bool {
    matches!(
        render_tag(store, object, context),
        TypeTag::Union
            | TypeTag::Intersection
            | TypeTag::Function
            | TypeTag::Conditional
            | TypeTag::Readonly
            | TypeTag::Keyof
    )
}

fn render_tag(store: &Store, id: TypeId, context: &RenderContext) -> TypeTag {
    render_tag_with_limit(store, id, context, context.overlay_limit, context.depth)
}

/// Resolve the tag a type *displays* as, following overlay bindings and declared
/// recipes. This walk produces no output, so it never reaches `RenderContext::append`
/// or [`render_type_inner`]; `depth` therefore carries the display budget by hand and
/// the walk stops at [`DISPLAY_DEPTH_LIMIT`] with the type's own shallow tag
/// (backlog 87). Only parenthesization consults it, and a display that deep has
/// already collapsed to `...`, so the fallback is not observable today.
fn render_tag_with_limit(
    store: &Store,
    id: TypeId,
    context: &RenderContext,
    limit: usize,
    depth: usize,
) -> TypeTag {
    if depth >= DISPLAY_DEPTH_LIMIT {
        return store.tag(id);
    }
    if let Some(parameter) = store.type_param(id) {
        if let Some((layer, binding)) = overlay_binding(context, parameter.id, limit) {
            return match binding {
                RenderBinding::Type(ty) => {
                    render_tag_with_limit(store, ty, context, layer, depth + 1)
                }
                RenderBinding::Recipe(recipe) => {
                    declared_recipe_tag(store, recipe, context, layer, depth + 1)
                }
            };
        }
    }
    if let Some(declared) = store.declared_type(id) {
        return declared_recipe_tag_with_mapper(
            store,
            declared.recipe,
            &declared.mapper,
            context,
            limit,
            depth + 1,
        );
    }
    store.tag(id)
}

fn declared_recipe_tag(
    store: &Store,
    recipe: DeclaredRecipeId,
    context: &RenderContext,
    limit: usize,
    depth: usize,
) -> TypeTag {
    declared_recipe_tag_with_mapper(store, recipe, &[], context, limit, depth)
}

fn declared_recipe_tag_with_mapper(
    store: &Store,
    recipe: DeclaredRecipeId,
    mapper: &[(TypeParamId, TypeId)],
    context: &RenderContext,
    limit: usize,
    depth: usize,
) -> TypeTag {
    let Some(recipe) = store.declared_recipe(recipe) else {
        return TypeTag::Intrinsic;
    };
    match &recipe.node {
        DeclaredRecipeNode::Type(ty) => {
            if let Some(parameter) = store.type_param(*ty) {
                if let Some((_, mapped)) = mapper
                    .iter()
                    .find(|(candidate, _)| *candidate == parameter.id)
                {
                    return render_tag_with_limit(store, *mapped, context, limit, depth + 1);
                }
            }
            render_tag_with_limit(store, *ty, context, limit, depth + 1)
        }
        DeclaredRecipeNode::Array(_) => TypeTag::Array,
        DeclaredRecipeNode::Tuple { .. } => TypeTag::Tuple,
        DeclaredRecipeNode::Readonly(_) => TypeTag::Readonly,
        DeclaredRecipeNode::Application { template, .. } => {
            render_tag_with_limit(store, *template, context, limit, depth + 1)
        }
    }
}

fn render_literal(lit: &crate::types::repr::LiteralValue, context: &mut RenderContext) {
    use crate::types::repr::LiteralValue;
    match lit {
        LiteralValue::String(s) => {
            context.append("\"");
            for c in s.chars() {
                if context.truncated {
                    break;
                }
                match c {
                    '\\' => context.append("\\\\"),
                    '"' => context.append("\\\""),
                    '\n' => context.append("\\n"),
                    '\r' => context.append("\\r"),
                    '\t' => context.append("\\t"),
                    c if (c as u32) < 0x20 => context.append(&format!("\\u{{{:x}}}", c as u32)),
                    c => context.append(c.encode_utf8(&mut [0; 4])),
                }
            }
            context.append("\"");
        }
        LiteralValue::Boolean(b) => context.append(if *b { "true" } else { "false" }),
        LiteralValue::Number(n) => {
            // Render integers without a trailing `.0` to match `tsc`'s literal
            // display (`1`, not `1.0`).
            if n.fract() == 0.0 && n.is_finite() {
                context.append(&format!("{}", *n as i64));
            } else {
                context.append(&format!("{n}"));
            }
        }
        LiteralValue::WellKnownSymbol(symbol) => {
            context.append("typeof Symbol.");
            context.append(symbol.name());
        }
    }
}

/// Escape control characters (and `\`/`"`) inside a string-literal type body, as tsc
/// renders them — a raw newline would otherwise split a diagnostic across lines and,
/// after WU3, could inject a phantom `incomplete[…]` line into an exit-3 identity list.
#[cfg(test)]
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

#[cfg(test)]
mod stable_union_tests {
    use super::render_type;
    use crate::types::intern::Interner;
    use crate::types::repr::LiteralValue;

    fn render_string_union(first: &str, second: &str) -> String {
        let mut interner = Interner::with_intrinsics();
        let first = interner.intern_literal(LiteralValue::String(first.to_owned()));
        let second = interner.intern_literal(LiteralValue::String(second.to_owned()));
        let union = interner.union(vec![first, second]);
        render_type(interner.store(), union, false)
    }

    #[test]
    fn string_literal_union_render_is_independent_of_intern_history() {
        let a_then_b = render_string_union("a", "b");
        let b_then_a = render_string_union("b", "a");

        assert_eq!(a_then_b, "\"a\" | \"b\"");
        assert_eq!(b_then_a, "\"a\" | \"b\"");
        assert_eq!(a_then_b.as_bytes(), b_then_a.as_bytes());
    }
}
