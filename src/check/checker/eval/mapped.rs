use super::*;

impl<'a> ConditionalEvaluator<'a> {
    /// Schedule mapped-type evaluation. Free key sources defer conservatively;
    /// concrete key sources evaluate as tail steps before [`Task::AssembleMapped`]
    /// derives the output properties.
    pub(super) fn eval_mapped(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
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
    /// only when something changed. A **nested** mapped type descends into its
    /// **key source only** (which is outer-scoped — `Outer<T> = { [K in keyof T]:
    /// Ident<T[K]> }` injects the outer placeholder there, review probe X): its value
    /// template rebinds its OWN placeholder and stays untouched (the cross-binder case
    /// is out of subset, safe over-report).
    pub(super) fn replace_mapped_value(&mut self, ty: TypeId, value: TypeId) -> TypeId {
        match self.interner.store().tag(ty) {
            TypeTag::MappedValue => value,
            TypeTag::Intrinsic | TypeTag::Literal | TypeTag::TypeParam | TypeTag::Infer => ty,
            TypeTag::Mapped => {
                let Some(mapped) = self.interner.store().mapped_type(ty).copied() else {
                    return ty;
                };
                let key_source = self.replace_mapped_value(mapped.key_source, value);
                // M28: the modifiers source is outer-scoped like the key source (the
                // captured `T` of a nested `{ [P in K]: T[P] }` may be the OUTER map's
                // placeholder) — descend into it too.
                let modifiers_source = mapped
                    .modifiers_source
                    .map(|ms| self.replace_mapped_value(ms, value));
                if key_source == mapped.key_source && modifiers_source == mapped.modifiers_source {
                    return ty;
                }
                self.interner.intern_mapped(MappedType {
                    key_source,
                    modifiers_source,
                    ..mapped
                })
            }
            TypeTag::Object => {
                let Some(object) = self.interner.store().object_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let mut new = object.clone();
                for prop in &mut new.properties {
                    let nt = self.replace_mapped_value(prop.ty, value);
                    changed |= nt != prop.ty;
                    prop.ty = nt;
                }
                new.string_index = object.string_index.map(|v| {
                    let nv = self.replace_mapped_value(v, value);
                    changed |= nv != v;
                    nv
                });
                new.number_index = object.number_index.map(|v| {
                    let nv = self.replace_mapped_value(v, value);
                    changed |= nv != v;
                    nv
                });
                new.call_signatures = object
                    .call_signatures
                    .iter()
                    .map(|&s| {
                        let ns = self.replace_mapped_value(s, value);
                        changed |= ns != s;
                        ns
                    })
                    .collect();
                new.construct_signatures = object
                    .construct_signatures
                    .iter()
                    .map(|&s| {
                        let ns = self.replace_mapped_value(s, value);
                        changed |= ns != s;
                        ns
                    })
                    .collect();
                if changed {
                    self.interner.intern_object(new)
                } else {
                    ty
                }
            }
            TypeTag::Function => {
                let Some(function) = self.interner.store().function_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let mut new = function.clone();
                for param in &mut new.params {
                    let nt = self.replace_mapped_value(param.ty, value);
                    changed |= nt != param.ty;
                    param.ty = nt;
                }
                let nr = self.replace_mapped_value(function.ret, value);
                changed |= nr != function.ret;
                new.ret = nr;
                if changed {
                    self.interner.intern_function(new)
                } else {
                    ty
                }
            }
            TypeTag::Union => {
                let Some(members) = self.interner.store().union_members(ty) else {
                    return ty;
                };
                let members: Vec<TypeId> = members.to_vec();
                let mut changed = false;
                let subst: Vec<TypeId> = members
                    .iter()
                    .map(|&m| {
                        let nm = self.replace_mapped_value(m, value);
                        changed |= nm != m;
                        nm
                    })
                    .collect();
                if changed {
                    self.interner.union(subst)
                } else {
                    ty
                }
            }
            // M31: descend into intersection members like a union, re-interning through
            // `Interner::intersection` only when a member changed.
            TypeTag::Intersection => {
                let Some(members) = self.interner.store().intersection_members(ty) else {
                    return ty;
                };
                let members: Vec<TypeId> = members.to_vec();
                let mut changed = false;
                let subst: Vec<TypeId> = members
                    .iter()
                    .map(|&m| {
                        let nm = self.replace_mapped_value(m, value);
                        changed |= nm != m;
                        nm
                    })
                    .collect();
                if changed {
                    self.interner.intersection(subst)
                } else {
                    ty
                }
            }
            TypeTag::Array => {
                let Some(element) = self.interner.store().array_type(ty).map(|a| a.element) else {
                    return ty;
                };
                let ne = self.replace_mapped_value(element, value);
                if ne != element {
                    self.interner.intern_array(ne)
                } else {
                    ty
                }
            }
            TypeTag::Tuple => {
                let Some(tuple) = self.interner.store().tuple_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let elements = tuple
                    .elements
                    .iter()
                    .map(|&e| {
                        let ne = self.replace_mapped_value(e, value);
                        changed |= ne != e;
                        ne
                    })
                    .collect();
                let rest = tuple.rest.map(|rest| {
                    let nt = self.replace_mapped_value(rest.ty, value);
                    changed |= nt != rest.ty;
                    TupleRestType { ty: nt, ..rest }
                });
                if changed {
                    self.interner
                        .intern_tuple_type(TupleType { elements, rest })
                } else {
                    ty
                }
            }
            TypeTag::Readonly => {
                let Some(operand) = self.interner.store().readonly_operand(ty) else {
                    return ty;
                };
                let no = self.replace_mapped_value(operand, value);
                if no != operand {
                    self.interner.intern_readonly(no)
                } else {
                    ty
                }
            }
            TypeTag::Conditional => {
                let Some(cond) = self.interner.store().conditional_type(ty).copied() else {
                    return ty;
                };
                let check = self.replace_mapped_value(cond.check, value);
                let extends_ty = self.replace_mapped_value(cond.extends_ty, value);
                let true_branch = self.replace_mapped_value(cond.true_branch, value);
                let false_branch = self.replace_mapped_value(cond.false_branch, value);
                if check == cond.check
                    && extends_ty == cond.extends_ty
                    && true_branch == cond.true_branch
                    && false_branch == cond.false_branch
                {
                    return ty;
                }
                self.interner.intern_conditional(ConditionalType {
                    check,
                    extends_ty,
                    true_branch,
                    false_branch,
                    infer_count: cond.infer_count,
                    distributive: cond.distributive,
                    poisoned: cond.poisoned,
                })
            }
            TypeTag::Instantiation => {
                let Some(inst) = self.interner.store().instantiation_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let new_args: Vec<(TypeParamId, TypeId)> = inst
                    .args
                    .iter()
                    .map(|&(p, v)| {
                        let nv = self.replace_mapped_value(v, value);
                        changed |= nv != v;
                        (p, nv)
                    })
                    .collect();
                if changed {
                    self.interner.intern_instantiation(inst.base, new_args)
                } else {
                    ty
                }
            }
            // M27: a template's `T[K]` placeholder lives in its holes
            // (`` `x${T[K]}` `` inside a mapped value template) — recurse into them.
            TypeTag::Template => {
                let Some(template) = self.interner.store().template_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let new_holes: Vec<TypeId> = template
                    .holes
                    .iter()
                    .map(|&hole| {
                        let nh = self.replace_mapped_value(hole, value);
                        changed |= nh != hole;
                        nh
                    })
                    .collect();
                if changed {
                    self.interner.intern_template(TemplateType {
                        texts: template.texts,
                        holes: new_holes,
                    })
                } else {
                    ty
                }
            }
            // M28: a `keyof X[K]`-style value template carries the placeholder in the
            // keyof operand — recurse into it.
            TypeTag::Keyof => {
                let Some(operand) = self.interner.store().keyof_operand(ty) else {
                    return ty;
                };
                let no = self.replace_mapped_value(operand, value);
                if no != operand {
                    self.interner.intern_keyof(no)
                } else {
                    ty
                }
            }
        }
    }
}
