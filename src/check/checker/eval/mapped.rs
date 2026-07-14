use super::*;

/// Per-call context for [`ConditionalEvaluator::replace_mapped_value`] (M26 `T[K]`
/// substitution). Kept SEPARATE from the evaluator's `in_flight`/`memo` so a provisional
/// rewrite over a cyclic value template can never poison the durable evaluator memo.
struct MappedRewrite {
    /// The source property type that replaces every `T[K]` placeholder.
    value: TypeId,
    /// Ids currently being rewritten — breaks recursion over a cyclic TypeId graph.
    in_progress: FxHashSet<TypeId>,
    /// `input id → rewritten id` for fully-resolved nodes (shared-subgraph short-circuit).
    memo: FxHashMap<TypeId, TypeId>,
}

/// One pending post-order rewrite. Each frame owns the store snapshot needed to
/// rebuild after its child results are available.
enum MappedRewriteFrame {
    Identity {
        ty: TypeId,
        result: TypeId,
    },
    Mapped {
        ty: TypeId,
        mapped: MappedType,
    },
    Object {
        ty: TypeId,
        object: ObjectType,
    },
    Function {
        ty: TypeId,
        function: FunctionType,
    },
    Union {
        ty: TypeId,
        members: Vec<TypeId>,
    },
    Intersection {
        ty: TypeId,
        members: Vec<TypeId>,
    },
    Array {
        ty: TypeId,
        element: TypeId,
    },
    Tuple {
        ty: TypeId,
        tuple: TupleType,
    },
    Readonly {
        ty: TypeId,
        operand: TypeId,
    },
    Conditional {
        ty: TypeId,
        conditional: ConditionalType,
    },
    Instantiation {
        ty: TypeId,
        base: TypeId,
        args: Vec<(TypeParamId, TypeId)>,
    },
    ClassInstance {
        ty: TypeId,
        class: crate::types::ClassId,
        args: Vec<TypeId>,
    },
    Template {
        ty: TypeId,
        template: TemplateType,
    },
    Keyof {
        ty: TypeId,
        operand: TypeId,
    },
    DeferredIndexedAccess {
        ty: TypeId,
        object: TypeId,
        index: TypeId,
    },
}

impl MappedRewriteFrame {
    fn ty(&self) -> TypeId {
        match self {
            MappedRewriteFrame::Identity { ty, .. }
            | MappedRewriteFrame::Mapped { ty, .. }
            | MappedRewriteFrame::Object { ty, .. }
            | MappedRewriteFrame::Function { ty, .. }
            | MappedRewriteFrame::Union { ty, .. }
            | MappedRewriteFrame::Intersection { ty, .. }
            | MappedRewriteFrame::Array { ty, .. }
            | MappedRewriteFrame::Tuple { ty, .. }
            | MappedRewriteFrame::Readonly { ty, .. }
            | MappedRewriteFrame::Conditional { ty, .. }
            | MappedRewriteFrame::Instantiation { ty, .. }
            | MappedRewriteFrame::ClassInstance { ty, .. }
            | MappedRewriteFrame::Template { ty, .. }
            | MappedRewriteFrame::Keyof { ty, .. }
            | MappedRewriteFrame::DeferredIndexedAccess { ty, .. } => *ty,
        }
    }

    fn child_count(&self) -> usize {
        match self {
            MappedRewriteFrame::Identity { .. } => 0,
            MappedRewriteFrame::Mapped { mapped, .. } => {
                1 + usize::from(mapped.modifiers_source.is_some())
            }
            MappedRewriteFrame::Object { object, .. } => {
                object.properties.len()
                    + usize::from(object.string_index.is_some())
                    + usize::from(object.number_index.is_some())
                    + object.call_signatures.len()
                    + object.construct_signatures.len()
            }
            MappedRewriteFrame::Function { function, .. } => {
                function
                    .type_params
                    .iter()
                    .map(|type_param| {
                        usize::from(type_param.constraint.is_some())
                            + usize::from(type_param.default.is_some())
                    })
                    .sum::<usize>()
                    + usize::from(function.receiver.is_some())
                    + function.params.len()
                    + 1
            }
            MappedRewriteFrame::Union { members, .. }
            | MappedRewriteFrame::Intersection { members, .. } => members.len(),
            MappedRewriteFrame::Array { .. }
            | MappedRewriteFrame::Readonly { .. }
            | MappedRewriteFrame::Keyof { .. } => 1,
            MappedRewriteFrame::Tuple { tuple, .. } => {
                tuple.elements.len() + usize::from(tuple.rest.is_some())
            }
            MappedRewriteFrame::Conditional { .. } => 4,
            MappedRewriteFrame::Instantiation { args, .. } => args.len(),
            MappedRewriteFrame::ClassInstance { args, .. } => args.len(),
            MappedRewriteFrame::Template { template, .. } => template.holes.len(),
            MappedRewriteFrame::DeferredIndexedAccess { .. } => 2,
        }
    }

    fn push_children_reverse(&self, tasks: &mut Vec<MappedRewriteTask>) {
        let mut push = |ty| tasks.push(MappedRewriteTask::Visit(ty));
        match self {
            MappedRewriteFrame::Identity { .. } => {}
            MappedRewriteFrame::Mapped { mapped, .. } => {
                if let Some(modifiers_source) = mapped.modifiers_source {
                    push(modifiers_source);
                }
                push(mapped.key_source);
            }
            MappedRewriteFrame::Object { object, .. } => {
                for &signature in object.construct_signatures.iter().rev() {
                    push(signature);
                }
                for &signature in object.call_signatures.iter().rev() {
                    push(signature);
                }
                if let Some(number_index) = object.number_index {
                    push(number_index);
                }
                if let Some(string_index) = object.string_index {
                    push(string_index);
                }
                for property in object.properties.iter().rev() {
                    if let Some(write_ty) = property.write_ty {
                        push(write_ty);
                    }
                    push(property.ty);
                }
            }
            MappedRewriteFrame::Function { function, .. } => {
                push(function.ret);
                for parameter in function.params.iter().rev() {
                    push(parameter.ty);
                }
                if let Some(receiver) = function.receiver {
                    push(receiver);
                }
                for type_param in function.type_params.iter().rev() {
                    if let Some(default) = type_param.default {
                        push(default);
                    }
                    if let Some(constraint) = type_param.constraint {
                        push(constraint);
                    }
                }
            }
            MappedRewriteFrame::Union { members, .. }
            | MappedRewriteFrame::Intersection { members, .. } => {
                for &member in members.iter().rev() {
                    push(member);
                }
            }
            MappedRewriteFrame::Array { element, .. } => push(*element),
            MappedRewriteFrame::Tuple { tuple, .. } => {
                if let Some(rest) = tuple.rest {
                    push(rest.ty);
                }
                for &element in tuple.elements.iter().rev() {
                    push(element);
                }
            }
            MappedRewriteFrame::Readonly { operand, .. }
            | MappedRewriteFrame::Keyof { operand, .. } => push(*operand),
            MappedRewriteFrame::Conditional { conditional, .. } => {
                push(conditional.false_branch);
                push(conditional.true_branch);
                push(conditional.extends_ty);
                push(conditional.check);
            }
            MappedRewriteFrame::Instantiation { args, .. } => {
                for &(_, value) in args.iter().rev() {
                    push(value);
                }
            }
            MappedRewriteFrame::ClassInstance { args, .. } => {
                for &arg in args.iter().rev() {
                    push(arg);
                }
            }
            MappedRewriteFrame::Template { template, .. } => {
                for &hole in template.holes.iter().rev() {
                    push(hole);
                }
            }
            MappedRewriteFrame::DeferredIndexedAccess { object, index, .. } => {
                push(*index);
                push(*object);
            }
        }
    }
}

enum MappedRewriteTask {
    Visit(TypeId),
    Finish(usize),
}

struct MappedRewriteChildren<'a> {
    values: &'a [TypeId],
    index: usize,
}

impl<'a> MappedRewriteChildren<'a> {
    fn new(values: &'a [TypeId]) -> Self {
        Self { values, index: 0 }
    }

    fn next(&mut self, original: TypeId) -> TypeId {
        let result = self.values.get(self.index).copied();
        debug_assert!(
            result.is_some(),
            "mapped rewrite frame is missing a child result"
        );
        self.index += 1;
        result.unwrap_or(original)
    }

    fn take(&mut self, count: usize) -> Option<&'a [TypeId]> {
        let start = self.index;
        self.index += count;
        let result = self.values.get(start..self.index);
        debug_assert!(
            result.is_some(),
            "mapped rewrite frame is missing child results"
        );
        result
    }

    fn finish(&self) {
        debug_assert_eq!(
            self.index,
            self.values.len(),
            "mapped rewrite frame consumed the wrong number of child results"
        );
    }
}

impl<'a> ConditionalEvaluator<'a> {
    /// Schedule mapped-type evaluation. Free key sources defer conservatively;
    /// concrete key sources evaluate as tail steps before [`Task::AssembleMapped`]
    /// derives the output properties.
    pub(super) fn eval_mapped(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
    ) {
        let Some(mapped) = self.interner.store().mapped_type(ty).copied() else {
            values.push(ty);
            return;
        };
        // Deferred: a free declaration type parameter (in the key source or value
        // template) leaves the whole mapped type an ordinary interned (deferred) node.
        if !self.is_concrete(ty) {
            values.push(ty);
            return;
        }
        if self.in_flight.contains(&ty) {
            self.note_cycle();
            return;
        }
        if self.exhausted {
            return;
        }
        self.steps += 1;
        if self.steps > self.budget {
            self.exhausted = true;
            return;
        }
        self.in_flight.insert(ty);
        // The result IS the assembled object; memoize this id to it once the key source
        // resolves and the properties are built (tail steps). M28: a captured modifiers
        // source evaluates through the same stack (so `Pick<Partial<P>, …>` composes);
        // the tasks pop in push-reverse order, so `assemble_mapped` pops the key source
        // first, then the modifiers source.
        tasks.push(Task::SetMemo(ty));
        tasks.push(Task::AssembleMapped(ty));
        tasks.push(Task::Eval(mapped.key_source));
        if let Some(ms) = mapped.modifiers_source {
            tasks.push(Task::Eval(ms));
        }
    }

    /// Assemble a mapped type's output properties (M26) after its key source has been
    /// evaluated (its result is on top of the value stack). Homomorphic: iterate the
    /// source properties ([`Self::homomorphic_source_props`] — a plain object's members,
    /// or the common-key intersection for the direct-union form), starting each output
    /// property's `?`/`readonly` from the source property (homomorphic preservation) and
    /// applying the node's modifier arithmetic, with `T[K]` resolved to the source
    /// property's type. Non-homomorphic: one property per string-literal key, starting
    /// both flags absent. Schedules a per-property value evaluation (the value template
    /// may itself demand conditional evaluation) and a final [`Task::BuildMappedObject`].
    ///
    /// **No permissive fallback (review F1 root cause):** a key source this subset
    /// cannot iterate — an index-signature source, a primitive, a non-literal key set —
    /// leaves the node **DEFERRED** (its own value, conservative relations), never an
    /// accept-everything `{}`. An error/any key source (an unresolved name upstream)
    /// degrades to the **error type** instead (M22 cascade suppression).
    pub(super) fn assemble_mapped(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let key_source = values.pop().unwrap_or(error);
        let Some(mapped) = self.interner.store().mapped_type(ty).copied() else {
            // Defensive: leave the node as its own (deferred) value; SetMemo pops it.
            // (A hand-built node without a mapped row carries no modifiers source, so
            // there is no second stack value to pop here.)
            values.push(ty);
            return;
        };
        // M28: the (evaluated) modifiers source sits under the key source on the value
        // stack — pop it unconditionally whenever the node carries one, keeping the
        // stack arity balanced on every early return below.
        let modifiers_source = mapped
            .modifiers_source
            .map(|_| values.pop().unwrap_or(error));
        // M22 discipline: an error/any key source (e.g. `keyof Bogus` after TK2304)
        // degrades the whole result to the error type — cascades stay suppressed.
        let wk = self.interner.well_known();
        if key_source == wk.error || key_source == wk.any {
            values.push(error);
            return;
        }

        let mut meta: Vec<MappedProp> = Vec::new();
        let mut value_pre: Vec<TypeId> = Vec::new();
        if mapped.homomorphic {
            let Some(props) = self.homomorphic_source_props(key_source) else {
                // Non-iterable source (index signatures, primitives, …): out of the M26
                // subset — the node stays deferred, never a permissive `{}`.
                values.push(ty);
                return;
            };
            for prop in props {
                let value = self.replace_mapped_value(mapped.value_template, prop.ty);
                meta.push(MappedProp {
                    name: prop.name,
                    optional: mapped.optional_modifier.apply(prop.optional),
                    readonly: mapped.readonly_modifier.apply(prop.readonly),
                    // `-?` over an optional source member strips `undefined` from the
                    // evaluated value (tsc Required semantics; applied in
                    // `build_mapped_object` once the value has resolved).
                    strip_undefined: mapped.optional_modifier == ModifierOp::Remove
                        && prop.optional,
                });
                value_pre.push(value);
            }
        } else {
            // Non-homomorphic: the key set is the string-literal members of the key
            // source. A key set with any non-string-literal member (`K in string`, a
            // numeric key) is out of subset → deferred.
            let Some(names) = self.literal_string_keys(key_source) else {
                values.push(ty);
                return;
            };
            // M28: a captured **modifiers source** (`{ [P in K]: T[P] }` — tsc's
            // modifiersType) resolves each key against the source object: the
            // property's value type replaces the `MappedValue` placeholder and its
            // `?`/`readonly` flags seed the modifier arithmetic — so `Pick` preserves
            // both. Without one (Record's bare `V`), or for a key the source lacks
            // (`Pick<P, "q">` after its TK2344 — tsc still instantiates), the M26
            // behavior is unchanged: placeholder → error type, flags start absent.
            for name in names {
                let source_prop = modifiers_source
                    .and_then(|source| self.modifiers_source_property(source, &name));
                match source_prop {
                    Some(prop) => {
                        let value = self.replace_mapped_value(mapped.value_template, prop.ty);
                        meta.push(MappedProp {
                            name,
                            optional: mapped.optional_modifier.apply(prop.optional),
                            readonly: mapped.readonly_modifier.apply(prop.readonly),
                            // `-?` over an optional source member strips `undefined`,
                            // exactly like the homomorphic path.
                            strip_undefined: mapped.optional_modifier == ModifierOp::Remove
                                && prop.optional,
                        });
                        value_pre.push(value);
                    }
                    None => {
                        let value = self.replace_mapped_value(mapped.value_template, error);
                        meta.push(MappedProp {
                            name,
                            optional: mapped.optional_modifier.apply(false),
                            readonly: mapped.readonly_modifier.apply(false),
                            strip_undefined: false,
                        });
                        value_pre.push(value);
                    }
                }
            }
        }

        tasks.push(Task::BuildMappedObject(meta));
        // Push in reverse so the per-property values pop (and their results land) in
        // order, aligning with the metadata order in `BuildMappedObject`.
        for &v in value_pre.iter().rev() {
            tasks.push(Task::Eval(v));
        }
    }

    pub(super) fn modifiers_source_property(
        &mut self,
        source: TypeId,
        name: &str,
    ) -> Option<PropertyType> {
        if let Some(prop) = self
            .interner
            .store()
            .object_type(source)
            .and_then(|object| Self::named_source_property(object, name))
        {
            return Some(prop);
        }

        if let Some(members) = self
            .interner
            .store()
            .intersection_members(source)
            .map(|m| m.to_vec())
        {
            return self.intersection_source_property(&members, name);
        }

        let members = self.interner.store().union_members(source)?.to_vec();
        let mut tys = Vec::with_capacity(members.len());
        let mut optional = false;
        let mut readonly = false;
        for member in members {
            let prop = self
                .interner
                .store()
                .object_type(member)
                .and_then(|object| Self::named_source_property(object, name))?;
            tys.push(prop.ty);
            optional |= prop.optional;
            readonly |= prop.readonly;
        }

        let mut prop = PropertyType::public(name.to_string(), self.interner.union(tys));
        prop.optional = optional;
        prop.readonly = readonly;
        Some(prop)
    }

    pub(super) fn named_source_property(object: &ObjectType, name: &str) -> Option<PropertyType> {
        if let Some(prop) = object.property(name) {
            return Some(prop.clone());
        }
        object
            .string_index
            .map(|ty| PropertyType::public(name.to_string(), ty))
    }

    pub(super) fn intersection_source_property(
        &mut self,
        members: &[TypeId],
        name: &str,
    ) -> Option<PropertyType> {
        let mut tys = Vec::with_capacity(members.len());
        let mut optional = true;
        let mut readonly = false;
        {
            let store = self.interner.store();
            for &member in members {
                let prop = store
                    .object_type(member)
                    .and_then(|object| Self::named_source_property(object, name))?;
                tys.push(prop.ty);
                optional &= prop.optional;
                readonly |= prop.readonly;
            }
        }

        let mut prop = PropertyType::public(name.to_string(), self.interner.intersection(tys));
        prop.optional = optional;
        prop.readonly = readonly;
        Some(prop)
    }

    /// The source properties a homomorphic map iterates (M26), or `None` when the key
    /// source is not iterable in this subset (the node then stays deferred):
    ///
    ///  - a plain **object** (no index signatures) → its properties (`{}` included:
    ///    `Ident<{}>` = `{}`);
    ///  - a **union of plain objects** — only reachable as the DIRECT
    ///    `{ [K in keyof (A | B)]: … }` form, since a substituted naked-param union
    ///    distributes in `substitute` before evaluation — → the **common-key**
    ///    intersection (tsc: `keyof (A | B)` = `keyof A & keyof B`), each common
    ///    property's type the union of the members' types, `?`/`readonly` OR-ed across
    ///    members (matching tsc's union-property synthesis);
    ///  - an **intersection of plain objects** → all member properties, duplicate keys
    ///    intersected;
    ///  - an object **with** index signatures (no `K in string` production), a
    ///    primitive, or any other shape → `None`.
    pub(super) fn homomorphic_source_props(
        &mut self,
        key_source: TypeId,
    ) -> Option<Vec<PropertyType>> {
        if let Some(object) = self.interner.store().object_type(key_source) {
            if object.string_index.is_some() || object.number_index.is_some() {
                return None;
            }
            return Some(object.properties.clone());
        }
        if let Some(members) = self
            .interner
            .store()
            .intersection_members(key_source)
            .map(|m| m.to_vec())
        {
            return self.intersection_source_props(&members);
        }
        let members = self.interner.store().union_members(key_source)?.to_vec();
        let mut member_objects: Vec<Vec<PropertyType>> = Vec::with_capacity(members.len());
        {
            let store = self.interner.store();
            for member in &members {
                let object = store.object_type(*member)?;
                if object.string_index.is_some() || object.number_index.is_some() {
                    return None;
                }
                member_objects.push(object.properties.clone());
            }
        }
        // Intersect: keep the first member's keys present in EVERY member, collecting
        // each member's value type + flags. (A union always has ≥ 2 members.)
        let (first, rest) = member_objects.split_first()?;
        let mut common: Vec<(String, Vec<TypeId>, bool, bool)> = Vec::new();
        for prop in first {
            let mut tys = vec![prop.ty];
            let mut optional = prop.optional;
            let mut readonly = prop.readonly;
            let mut in_all = true;
            for other in rest {
                match other.iter().find(|p| p.name == prop.name) {
                    Some(p) => {
                        tys.push(p.ty);
                        optional |= p.optional;
                        readonly |= p.readonly;
                    }
                    None => {
                        in_all = false;
                        break;
                    }
                }
            }
            if in_all {
                common.push((prop.name.clone(), tys, optional, readonly));
            }
        }
        // Union the per-member value types outside the store borrow.
        let mut out: Vec<PropertyType> = Vec::with_capacity(common.len());
        for (name, tys, optional, readonly) in common {
            let ty = self.interner.union(tys);
            let mut prop = PropertyType::public(name, ty);
            prop.optional = optional;
            prop.readonly = readonly;
            out.push(prop);
        }
        Some(out)
    }

    pub(super) fn intersection_source_props(
        &mut self,
        members: &[TypeId],
    ) -> Option<Vec<PropertyType>> {
        let mut entries: Vec<(String, Vec<TypeId>, bool, bool)> = Vec::new();
        {
            let store = self.interner.store();
            for &member in members {
                let object = store.object_type(member)?;
                if object.string_index.is_some() || object.number_index.is_some() {
                    return None;
                }
                for prop in &object.properties {
                    match entries
                        .iter_mut()
                        .find(|(name, _, _, _)| *name == prop.name)
                    {
                        Some((_, tys, optional, readonly)) => {
                            tys.push(prop.ty);
                            *optional &= prop.optional;
                            *readonly |= prop.readonly;
                        }
                        None => entries.push((
                            prop.name.clone(),
                            vec![prop.ty],
                            prop.optional,
                            prop.readonly,
                        )),
                    }
                }
            }
        }

        let mut props = Vec::with_capacity(entries.len());
        for (name, tys, optional, readonly) in entries {
            let mut prop = PropertyType::public(name, self.interner.intersection(tys));
            prop.optional = optional;
            prop.readonly = readonly;
            props.push(prop);
        }
        Some(props)
    }

    /// Build the mapped result object, preserving metadata by position. `-?` over
    /// optional strips `undefined`; optional outputs bake `| undefined` into the
    /// stored type for the relation engine.
    pub(super) fn build_mapped_object(&mut self, meta: &[MappedProp], values: &[TypeId]) -> TypeId {
        let undefined = self.interner.well_known().undefined;
        let mut object = ObjectType::default();
        for (m, &value) in meta.iter().zip(values) {
            // `-?` Required semantics: strip `undefined` from the EVALUATED value of an
            // optional source member (probed tsc 6.0.3 — see `strip_undefined`).
            let value = if m.strip_undefined {
                self.strip_undefined(value)
            } else {
                value
            };
            // M21: an optional member's stored effective type includes `| undefined`, so
            // the relation engine's optional handling stays consistent.
            let ty = if m.optional {
                self.interner.union(vec![value, undefined])
            } else {
                value
            };
            let mut prop = PropertyType::public(m.name.clone(), ty);
            prop.optional = m.optional;
            prop.readonly = m.readonly;
            object.properties.push(prop);
        }
        self.interner.intern_object(object)
    }

    /// Remove `undefined` from a value type (M26 — the `-?` Required strip, probed
    /// against tsc 6.0.3, leader-arbitrated `m26_arb.ts`): a union containing
    /// `undefined` re-unions its other members (a 1-member remainder collapses via
    /// `Interner::union`); a value that is EXACTLY `undefined` maps to **`never`**
    /// (`Required<{ b?: undefined }>` gives `b: never` — filtering `undefined` by the
    /// not-undefined fact leaves nothing); any other non-union type is untouched.
    pub(super) fn strip_undefined(&mut self, ty: TypeId) -> TypeId {
        let wk = self.interner.well_known();
        if ty == wk.undefined {
            return wk.never;
        }
        let filtered: Vec<TypeId> = match self.interner.store().union_members(ty) {
            Some(members) if members.contains(&wk.undefined) => members
                .iter()
                .copied()
                .filter(|&m| m != wk.undefined)
                .collect(),
            _ => return ty,
        };
        self.interner.union(filtered)
    }

    /// The string-literal keys of a non-homomorphic mapped type's key source (M26): the
    /// members of a literal union, or a single string literal. `None` — deferring the
    /// node — when ANY member is not a string literal (`K in string`, numeric keys, …):
    /// silently dropping such a key would shrink the target (a missed-member false
    /// negative), so the whole map is out of subset instead (review F1 secondaries).
    pub(super) fn literal_string_keys(&self, ty: TypeId) -> Option<Vec<String>> {
        let store = self.interner.store();
        let members: Vec<TypeId> = match store.union_members(ty) {
            Some(members) => members.to_vec(),
            None => vec![ty],
        };
        members
            .into_iter()
            .map(|m| match store.literal_value(m) {
                Some(LiteralValue::String(s)) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    /// Replace every [`TypeTag::MappedValue`] placeholder (`T[K]`) in a mapped type's
    /// value template with `value` — the current key's source property type (M26).
    /// Recurses through structural composites and a conditional's components; re-interns
    /// only when something changed. A **nested** mapped type descends into its key and
    /// modifiers sources (both outer-scoped); its value template rebinds its OWN
    /// placeholder and stays untouched (the cross-binder case is out of subset, safe
    /// over-report).
    pub(super) fn replace_mapped_value(&mut self, ty: TypeId, value: TypeId) -> TypeId {
        let mut ctx = MappedRewrite {
            value,
            in_progress: FxHashSet::default(),
            memo: FxHashMap::default(),
        };
        let mut tasks = vec![MappedRewriteTask::Visit(ty)];
        let mut frames = Vec::new();
        let mut values = Vec::new();

        while let Some(task) = tasks.pop() {
            match task {
                MappedRewriteTask::Visit(ty) => {
                    if let Some(&done) = ctx.memo.get(&ty) {
                        values.push(done);
                        continue;
                    }
                    if !ctx.in_progress.insert(ty) {
                        values.push(ty);
                        continue;
                    }
                    let frame = self.mapped_rewrite_frame(ty, ctx.value);
                    let frame_index = frames.len();
                    frames.push(frame);
                    tasks.push(MappedRewriteTask::Finish(frame_index));
                    frames[frame_index].push_children_reverse(&mut tasks);
                }
                MappedRewriteTask::Finish(frame_index) => {
                    debug_assert_eq!(frame_index + 1, frames.len());
                    let Some(frame) = frames.pop() else {
                        debug_assert!(false, "mapped rewrite task lost its frame");
                        continue;
                    };
                    let ty = frame.ty();
                    let child_count = frame.child_count();
                    let result = if let Some(start) = values.len().checked_sub(child_count) {
                        let result = self.rebuild_mapped_rewrite_frame(frame, &values[start..]);
                        values.truncate(start);
                        result
                    } else {
                        debug_assert!(false, "mapped rewrite frame is missing child results");
                        ty
                    };
                    debug_assert!(ctx.in_progress.remove(&ty));
                    ctx.memo.insert(ty, result);
                    values.push(result);
                }
            }
        }

        debug_assert!(
            frames.is_empty(),
            "mapped rewrite traversal left pending frames"
        );
        debug_assert!(
            ctx.in_progress.is_empty(),
            "mapped rewrite traversal left in-progress nodes"
        );
        debug_assert_eq!(
            values.len(),
            1,
            "mapped rewrite traversal left extra results"
        );
        values.pop().unwrap_or(ty)
    }

    fn mapped_rewrite_frame(&self, ty: TypeId, value: TypeId) -> MappedRewriteFrame {
        let identity = |result| MappedRewriteFrame::Identity { ty, result };
        match self.interner.store().tag(ty) {
            TypeTag::MappedValue => identity(value),
            TypeTag::Intrinsic | TypeTag::Literal | TypeTag::TypeParam | TypeTag::Infer => {
                identity(ty)
            }
            TypeTag::Mapped => {
                let Some(mapped) = self.interner.store().mapped_type(ty).copied() else {
                    return identity(ty);
                };
                MappedRewriteFrame::Mapped { ty, mapped }
            }
            TypeTag::Object => {
                let Some(object) = self.interner.store().object_type(ty).cloned() else {
                    return identity(ty);
                };
                MappedRewriteFrame::Object { ty, object }
            }
            TypeTag::Function => {
                let Some(function) = self.interner.store().function_type(ty).cloned() else {
                    return identity(ty);
                };
                MappedRewriteFrame::Function { ty, function }
            }
            TypeTag::Union => {
                let Some(members) = self
                    .interner
                    .store()
                    .union_members(ty)
                    .map(|members| members.to_vec())
                else {
                    return identity(ty);
                };
                MappedRewriteFrame::Union { ty, members }
            }
            TypeTag::Intersection => {
                let Some(members) = self
                    .interner
                    .store()
                    .intersection_members(ty)
                    .map(|members| members.to_vec())
                else {
                    return identity(ty);
                };
                MappedRewriteFrame::Intersection { ty, members }
            }
            TypeTag::Array => {
                let Some(element) = self
                    .interner
                    .store()
                    .array_type(ty)
                    .map(|array| array.element)
                else {
                    return identity(ty);
                };
                MappedRewriteFrame::Array { ty, element }
            }
            TypeTag::Tuple => {
                let Some(tuple) = self.interner.store().tuple_type(ty).cloned() else {
                    return identity(ty);
                };
                MappedRewriteFrame::Tuple { ty, tuple }
            }
            TypeTag::Readonly => {
                let Some(operand) = self.interner.store().readonly_operand(ty) else {
                    return identity(ty);
                };
                MappedRewriteFrame::Readonly { ty, operand }
            }
            TypeTag::Conditional => {
                let Some(conditional) = self.interner.store().conditional_type(ty).copied() else {
                    return identity(ty);
                };
                MappedRewriteFrame::Conditional { ty, conditional }
            }
            TypeTag::Instantiation => {
                let Some(instantiation) = self.interner.store().instantiation_type(ty).cloned()
                else {
                    return identity(ty);
                };
                MappedRewriteFrame::Instantiation {
                    ty,
                    base: instantiation.base,
                    args: instantiation.args,
                }
            }
            TypeTag::ClassInstance => {
                let Some(instance) = self.interner.store().class_instance_type(ty).cloned() else {
                    return identity(ty);
                };
                MappedRewriteFrame::ClassInstance {
                    ty,
                    class: instance.class,
                    args: instance.args,
                }
            }
            TypeTag::Template => {
                let Some(template) = self.interner.store().template_type(ty).cloned() else {
                    return identity(ty);
                };
                MappedRewriteFrame::Template { ty, template }
            }
            TypeTag::Keyof => {
                let Some(operand) = self.interner.store().keyof_operand(ty) else {
                    return identity(ty);
                };
                MappedRewriteFrame::Keyof { ty, operand }
            }
            TypeTag::DeferredIndexedAccess => {
                let Some(access) = self
                    .interner
                    .store()
                    .deferred_indexed_access_type(ty)
                    .copied()
                else {
                    return identity(ty);
                };
                MappedRewriteFrame::DeferredIndexedAccess {
                    ty,
                    object: access.object,
                    index: access.index,
                }
            }
        }
    }

    fn rebuild_mapped_rewrite_frame(
        &mut self,
        frame: MappedRewriteFrame,
        child_results: &[TypeId],
    ) -> TypeId {
        let mut children = MappedRewriteChildren::new(child_results);
        let result = match frame {
            MappedRewriteFrame::Identity { result, .. } => result,
            MappedRewriteFrame::Mapped { ty, mapped } => {
                let key_source = children.next(mapped.key_source);
                let modifiers_source = mapped.modifiers_source.map(|source| children.next(source));
                if key_source != mapped.key_source || modifiers_source != mapped.modifiers_source {
                    self.interner.intern_mapped(MappedType {
                        key_source,
                        modifiers_source,
                        ..mapped
                    })
                } else {
                    ty
                }
            }
            MappedRewriteFrame::Object { ty, object } => {
                let mut changed = false;
                let mut new = object.clone();
                for (original, rewritten) in object.properties.iter().zip(&mut new.properties) {
                    let value = children.next(original.ty);
                    changed |= value != original.ty;
                    rewritten.ty = value;
                    rewritten.write_ty = original.write_ty.map(|write_ty| {
                        let value = children.next(write_ty);
                        changed |= value != write_ty;
                        value
                    });
                }
                new.string_index = object.string_index.map(|source| children.next(source));
                changed |= new.string_index != object.string_index;
                new.number_index = object.number_index.map(|source| children.next(source));
                changed |= new.number_index != object.number_index;
                new.call_signatures = object
                    .call_signatures
                    .iter()
                    .map(|&source| children.next(source))
                    .collect();
                changed |= new.call_signatures != object.call_signatures;
                new.construct_signatures = object
                    .construct_signatures
                    .iter()
                    .map(|&source| children.next(source))
                    .collect();
                changed |= new.construct_signatures != object.construct_signatures;
                if changed {
                    self.interner.intern_object(new)
                } else {
                    ty
                }
            }
            MappedRewriteFrame::Function { ty, function } => {
                let mut changed = false;
                let mut new = function.clone();
                for (original, rewritten) in function.type_params.iter().zip(&mut new.type_params) {
                    rewritten.constraint = original.constraint.map(|source| children.next(source));
                    rewritten.default = original.default.map(|source| children.next(source));
                    changed |= rewritten.constraint != original.constraint
                        || rewritten.default != original.default;
                }
                new.receiver = function.receiver.map(|source| children.next(source));
                changed |= new.receiver != function.receiver;
                for (original, rewritten) in function.params.iter().zip(&mut new.params) {
                    let value = children.next(original.ty);
                    changed |= value != original.ty;
                    rewritten.ty = value;
                }
                new.ret = children.next(function.ret);
                changed |= new.ret != function.ret;
                if changed {
                    self.interner.intern_function(new)
                } else {
                    ty
                }
            }
            MappedRewriteFrame::Union { ty, members } => match children.take(members.len()) {
                Some(rewritten) if rewritten != members => self.interner.union(rewritten.to_vec()),
                Some(_) | None => ty,
            },
            MappedRewriteFrame::Intersection { ty, members } => {
                match children.take(members.len()) {
                    Some(rewritten) if rewritten != members => {
                        self.interner.intersection(rewritten.to_vec())
                    }
                    Some(_) | None => ty,
                }
            }
            MappedRewriteFrame::Array { ty, element } => {
                let rewritten = children.next(element);
                if rewritten != element {
                    self.interner.intern_array(rewritten)
                } else {
                    ty
                }
            }
            MappedRewriteFrame::Tuple { ty, tuple } => {
                let mut changed = false;
                let elements = tuple
                    .elements
                    .iter()
                    .map(|&element| {
                        let rewritten = children.next(element);
                        changed |= rewritten != element;
                        rewritten
                    })
                    .collect();
                let rest = tuple.rest.map(|rest| {
                    let rewritten = children.next(rest.ty);
                    changed |= rewritten != rest.ty;
                    TupleRestType {
                        ty: rewritten,
                        ..rest
                    }
                });
                if changed {
                    self.interner
                        .intern_tuple_type(TupleType { elements, rest })
                } else {
                    ty
                }
            }
            MappedRewriteFrame::Readonly { ty, operand } => {
                let rewritten = children.next(operand);
                if rewritten != operand {
                    self.interner.intern_readonly(rewritten)
                } else {
                    ty
                }
            }
            MappedRewriteFrame::Conditional { ty, conditional } => {
                let check = children.next(conditional.check);
                let extends_ty = children.next(conditional.extends_ty);
                let true_branch = children.next(conditional.true_branch);
                let false_branch = children.next(conditional.false_branch);
                if check != conditional.check
                    || extends_ty != conditional.extends_ty
                    || true_branch != conditional.true_branch
                    || false_branch != conditional.false_branch
                {
                    self.interner.intern_conditional(ConditionalType {
                        check,
                        extends_ty,
                        true_branch,
                        false_branch,
                        ..conditional
                    })
                } else {
                    ty
                }
            }
            MappedRewriteFrame::Instantiation { ty, base, args } => {
                match children.take(args.len()) {
                    Some(values)
                        if args
                            .iter()
                            .map(|(_, value)| *value)
                            .ne(values.iter().copied()) =>
                    {
                        self.interner.intern_instantiation(
                            base,
                            args.iter()
                                .zip(values)
                                .map(|(&(param, _), &value)| (param, value))
                                .collect(),
                        )
                    }
                    Some(_) | None => ty,
                }
            }
            MappedRewriteFrame::ClassInstance { ty, class, args } => {
                match children.take(args.len()) {
                    Some(values) if values != args => {
                        self.interner.intern_class_instance(class, values.to_vec())
                    }
                    Some(_) | None => ty,
                }
            }
            MappedRewriteFrame::Template { ty, template } => {
                match children.take(template.holes.len()) {
                    Some(holes) if holes != template.holes => {
                        self.interner.intern_template(TemplateType {
                            texts: template.texts,
                            holes: holes.to_vec(),
                        })
                    }
                    Some(_) | None => ty,
                }
            }
            MappedRewriteFrame::Keyof { ty, operand } => {
                let rewritten = children.next(operand);
                if rewritten != operand {
                    self.interner.intern_keyof(rewritten)
                } else {
                    ty
                }
            }
            MappedRewriteFrame::DeferredIndexedAccess { ty, object, index } => {
                let rewritten_object = children.next(object);
                let rewritten_index = children.next(index);
                if rewritten_object != object || rewritten_index != index {
                    self.interner
                        .intern_deferred_indexed_access(rewritten_object, rewritten_index)
                } else {
                    ty
                }
            }
        };
        children.finish();
        result
    }
}
