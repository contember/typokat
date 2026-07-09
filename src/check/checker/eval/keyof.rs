use super::*;

impl<'a> ConditionalEvaluator<'a> {
    /// Schedule the evaluation of a deferred `keyof` (M28). A node whose operand still
    /// contains a free declaration type parameter stays deferred (its own value,
    /// conservative relations); a concrete one first evaluates its operand through the
    /// shared work-stack (the operand may itself be an instantiation / mapped /
    /// conditional — `keyof Omit<P, "a">`), then [`Task::BuildKeyof`] resolves it
    /// through the SAME keyof computation the eager path uses ([`keyof_of_object`] —
    /// single source of truth).
    pub(super) fn eval_keyof(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let Some(operand) = self.interner.store().keyof_operand(ty) else {
            values.push(ty);
            return;
        };
        // Deferred: a free declaration type parameter in the operand leaves the node
        // an ordinary interned (deferred) type.
        if !self.is_concrete(ty) {
            values.push(ty);
            return;
        }
        if self.in_flight.contains(&ty) || self.exhausted {
            values.push(error);
            return;
        }
        self.steps += 1;
        if self.steps > self.budget {
            self.exhausted = true;
            values.push(error);
            return;
        }
        self.in_flight.insert(ty);
        tasks.push(Task::SetMemo(ty));
        tasks.push(Task::BuildKeyof(ty));
        tasks.push(Task::Eval(operand));
    }

    /// Resolve a deferred `keyof` whose operand has been evaluated (M28 — the operand
    /// result is on top of the value stack). A concrete object or union-of-objects
    /// operand keys through the shared [`keyof_of_type`] computation; an error/any
    /// operand degrades to the error type (M22 cascade suppression); any other shape
    /// stays a deferred value (rebuilt over the evaluated operand), never a permissive
    /// fallback.
    pub(super) fn build_keyof(&mut self, ty: TypeId, values: &mut Vec<TypeId>, error: TypeId) {
        let operand = values.pop().unwrap_or(error);
        let wk = self.interner.well_known();
        if operand == wk.error || operand == wk.any {
            values.push(error);
            return;
        }
        if let Some(keys) = keyof_of_type(self.interner, operand) {
            values.push(keys);
            return;
        }
        let node = if self.interner.store().keyof_operand(ty) == Some(operand) {
            ty
        } else {
            self.interner.intern_keyof(operand)
        };
        values.push(node);
    }
}

/// Compute `keyof` over a concrete object or union of objects. Object operands use
/// [`keyof_of_object`]; union operands intersect the members' known keys (`keyof (A | B)`
/// is the keys common to every member). `None` means the operand shape is outside this
/// subset and the caller decides the fallback.
pub(in crate::check::checker) fn keyof_of_type(
    interner: &mut Interner,
    operand: TypeId,
) -> Option<TypeId> {
    if interner.store().object_type(operand).is_some() {
        return keyof_of_object(interner, operand);
    }
    if let Some(members) = interner
        .store()
        .intersection_members(operand)
        .map(|m| m.to_vec())
    {
        let mut keys = Vec::with_capacity(members.len());
        for member in members {
            keys.push(keyof_of_type(interner, member)?);
        }
        return Some(interner.union(keys));
    }
    let members = interner.store().union_members(operand)?.to_vec();
    keyof_of_union(interner, &members)
}

/// Compute `keyof` over an **object** operand — shared by eager lowering and deferred
/// evaluation. The result is the `union(...)` of property names as string-literal
/// types, plus `string`/`number` for the respective index signatures (an empty object
/// yields `never` via the union collapse). `None` when the operand is not an object.
pub(in crate::check::checker) fn keyof_of_object(
    interner: &mut Interner,
    operand: TypeId,
) -> Option<TypeId> {
    let store = interner.store();
    let object = store.object_type(operand)?;

    // Snapshot the key components before the mutable interning borrow.
    let names: Vec<String> = object.properties.iter().map(|p| p.name.clone()).collect();
    let has_string_index = object.string_index.is_some();
    let has_number_index = object.number_index.is_some();

    let wk = interner.well_known();
    let mut members: Vec<TypeId> = Vec::with_capacity(names.len() + 2);
    for name in names {
        members.push(interner.intern_literal(LiteralValue::String(name)));
    }
    if has_string_index {
        members.push(wk.string);
    }
    if has_number_index {
        members.push(wk.number);
    }
    Some(interner.union(members))
}

struct UnionKeyInfo {
    names: Vec<String>,
    name_set: FxHashSet<String>,
    has_string_index: bool,
    has_number_index: bool,
}

fn keyof_of_union(interner: &mut Interner, members: &[TypeId]) -> Option<TypeId> {
    let infos: Vec<UnionKeyInfo> = {
        let store = interner.store();
        let mut infos = Vec::with_capacity(members.len());
        for &member in members {
            let object = store.object_type(member)?;
            let names: Vec<String> = object.properties.iter().map(|p| p.name.clone()).collect();
            let name_set = names.iter().cloned().collect();
            infos.push(UnionKeyInfo {
                names,
                name_set,
                has_string_index: object.string_index.is_some(),
                has_number_index: object.number_index.is_some(),
            });
        }
        infos
    };

    let wk = interner.well_known();
    let mut seen = FxHashSet::default();
    let mut all_names = Vec::new();
    for info in &infos {
        for name in &info.names {
            if seen.insert(name.clone()) {
                all_names.push(name.clone());
            }
        }
    }

    let mut keys = Vec::new();
    for name in all_names {
        if infos
            .iter()
            .all(|info| info.name_set.contains(&name) || info.has_string_index)
        {
            keys.push(interner.intern_literal(LiteralValue::String(name)));
        }
    }
    if infos.iter().all(|info| info.has_string_index) {
        keys.push(wk.string);
    }
    if infos.iter().all(|info| info.has_number_index) {
        keys.push(wk.number);
    }
    Some(interner.union(keys))
}

/// Whether `ty` transitively contains a genuinely deferred [`TypeTag::Keyof`] node
/// (M28) — the constraint-side gate for a `keyof` whose operand still contains a free
/// declaration type parameter. Concrete-but-unsupported `keyof` nodes must not be
/// skipped; they relate conservatively instead of accepting bad arguments.
pub(in crate::check) fn contains_deferred_keyof(
    store: &crate::types::store::Store,
    ty: TypeId,
) -> bool {
    contains_deferred_keyof_node(store, ty)
}

fn contains_deferred_keyof_node(store: &crate::types::store::Store, ty: TypeId) -> bool {
    let mut stack = vec![ty];
    let mut visited: FxHashSet<TypeId> = FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !visited.insert(t) {
            continue;
        }
        if store.tag(t) == TypeTag::Keyof {
            if let Some(operand) = store.keyof_operand(t) {
                if contains_free_keyof_operand(store, operand) {
                    return true;
                }
                stack.push(operand);
            }
            continue;
        }
        push_node_children(store, t, &mut stack, false);
    }
    false
}

fn contains_free_keyof_operand(store: &crate::types::store::Store, ty: TypeId) -> bool {
    let mut stack = vec![ty];
    let mut visited: FxHashSet<TypeId> = FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !visited.insert(t) {
            continue;
        }
        match store.tag(t) {
            TypeTag::TypeParam | TypeTag::Infer | TypeTag::MappedValue => return true,
            _ => push_node_children(store, t, &mut stack, false),
        }
    }
    false
}

fn push_node_children(
    store: &crate::types::store::Store,
    ty: TypeId,
    stack: &mut Vec<TypeId>,
    include_instantiation_base: bool,
) {
    match store.tag(ty) {
        TypeTag::Keyof => {
            if let Some(operand) = store.keyof_operand(ty) {
                stack.push(operand);
            }
        }
        TypeTag::Object => {
            if let Some(object) = store.object_type(ty) {
                stack.extend(object.properties.iter().map(|p| p.ty));
                stack.extend(object.string_index);
                stack.extend(object.number_index);
                stack.extend(object.call_signatures.iter().copied());
                stack.extend(object.construct_signatures.iter().copied());
            }
        }
        TypeTag::Function => {
            if let Some(f) = store.function_type(ty) {
                stack.extend(f.params.iter().map(|p| p.ty));
                stack.push(f.ret);
            }
        }
        TypeTag::Union => {
            if let Some(members) = store.union_members(ty) {
                stack.extend(members.iter().copied());
            }
        }
        TypeTag::Intersection => {
            if let Some(members) = store.intersection_members(ty) {
                stack.extend(members.iter().copied());
            }
        }
        TypeTag::Array => {
            if let Some(a) = store.array_type(ty) {
                stack.push(a.element);
            }
        }
        TypeTag::Tuple => {
            if let Some(tup) = store.tuple_type(ty) {
                stack.extend(tup.elements.iter().copied());
            }
        }
        TypeTag::Readonly => {
            if let Some(operand) = store.readonly_operand(ty) {
                stack.push(operand);
            }
        }
        TypeTag::Conditional => {
            if let Some(c) = store.conditional_type(ty) {
                stack.extend([c.check, c.extends_ty, c.true_branch, c.false_branch]);
            }
        }
        TypeTag::Instantiation => {
            if let Some(inst) = store.instantiation_type(ty) {
                if include_instantiation_base {
                    stack.push(inst.base);
                }
                stack.extend(inst.args.iter().map(|(_, v)| *v));
            }
        }
        TypeTag::Mapped => {
            if let Some(m) = store.mapped_type(ty) {
                stack.push(m.key_source);
                stack.push(m.value_template);
                stack.extend(m.modifiers_source);
            }
        }
        TypeTag::Template => {
            if let Some(template) = store.template_type(ty) {
                stack.extend(template.holes.iter().copied());
            }
        }
        TypeTag::Intrinsic
        | TypeTag::Literal
        | TypeTag::TypeParam
        | TypeTag::Infer
        | TypeTag::MappedValue => {}
    }
}

/// Whether `ty` transitively contains a **deferred type-level node** — a deferred
/// keyof, conditional, or lazy instantiation. Since M28 round 4 this is a
/// **message-form chooser only** (the round-3 TK2344 argument-side GATE it powered
/// was probe-disproven and removed — arguments now always evaluate and always
/// check): a failing argument whose evaluated form still trips this walk renders by
/// its WRITTEN form (which carries the alias name — `Extract<K, string>`) instead of
/// the raw substituted body.
pub(in crate::check) fn contains_deferred_argument(
    store: &crate::types::store::Store,
    ty: TypeId,
) -> bool {
    contains_nodes(store, ty, |tag| {
        matches!(
            tag,
            TypeTag::Keyof | TypeTag::Conditional | TypeTag::Instantiation
        )
    })
}

/// The shared deep walk behind the M28 undecidability gates: whether any node of a
/// target tag kind occurs anywhere inside `ty` (descending through every composite,
/// including conditional components, instantiation bases + argument values, mapped
/// components, and template holes). Iterative with a visited set so recursive
/// interned types terminate.
fn contains_nodes(
    store: &crate::types::store::Store,
    ty: TypeId,
    is_target: impl Fn(TypeTag) -> bool,
) -> bool {
    let mut stack = vec![ty];
    let mut visited: FxHashSet<TypeId> = FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !visited.insert(t) {
            continue;
        }
        if is_target(store.tag(t)) {
            return true;
        }
        push_node_children(store, t, &mut stack, true);
    }
    false
}
