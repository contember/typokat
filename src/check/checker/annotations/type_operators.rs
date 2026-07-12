use super::*;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower a conditional type `check extends extends_ty ? true : false` (M25, WU1).
    ///
    /// The node's [`CondFrame`] is pushed for the WHOLE call (check/extends/true/false
    /// positions) and its binders are in scope (`active`) only while the `extends` type
    /// and the **true** branch are lowered — so `infer U` binds a node-scoped de Bruijn
    /// index there, a reference to a name declared by THIS node from its false branch
    /// finds no active frame → `TK2304`, and a reference to an OUTER node's still-active
    /// binder from any nested position resolves as **cross-binder** — no `TK2304`, but it
    /// POISONS every node from the reference up to the binder's owner
    /// ([`Pass::resolve_infer_reference`], backlog 26 stopgap; a poisoned node never
    /// evaluates). The check type is lowered without this node's binders; `distributive`
    /// records whether it was a **naked** declaration type parameter. A check that
    /// surface-references the enclosing conditional alias is `TK2456` (a circular
    /// alias). At a value-position demand site the built node is evaluated
    /// ([`Pass::maybe_evaluate`]); inside a template body it is left as the interned
    /// node.
    pub(super) fn lower_conditional_type(
        &mut self,
        scope: ScopeId,
        cond: &TSConditionalType<'_>,
    ) -> Option<TypeId> {
        let error_ty = self.interner.well_known().error;

        // TK2456: the check surface-references the conditional alias currently being
        // resolved (`type Self = Self extends string ? 1 : 2`). Scoped to the check
        // surface so m5 recursion through object members stays legal.
        if let Some((decl_id, alias_span, name)) = self.resolving_conditional_alias.clone() {
            if self.check_surface_references(scope, &cond.check_type, decl_id) {
                self.diagnostics
                    .push(Diagnostic::circular_type_alias(alias_span, &name));
                return Some(error_ty);
            }
        }

        // This node's lowering context — see the doc above and `Pass::cond_frames`.
        self.cond_frames.push(CondFrame::default());

        // Check type — this node's own binders are NOT in scope (frame inactive). A
        // naked declaration type parameter check drives distribution.
        let check = self.lower_annotation(scope, &cond.check_type);
        let distributive =
            check.is_some_and(|c| self.interner.store().tag(c) == TypeTag::TypeParam);
        // An out-of-subset check aborts the whole annotation (pre-poison behavior kept);
        // the context must still unwind.
        let Some(check) = check else {
            self.cond_frames.pop();
            return None;
        };

        // The `infer` binders declared in the extends type are in scope for the extends
        // type itself and the true branch only.
        if let Some(frame) = self.cond_frames.last_mut() {
            frame.active = true;
        }
        let extends_ty = self.lower_annotation(scope, &cond.extends_type);
        let true_branch = self.lower_annotation(scope, &cond.true_type);
        if let Some(frame) = self.cond_frames.last_mut() {
            frame.active = false;
        }
        // False branch — this node's own infer names are out of scope here (→ TK2304);
        // an outer node's binder still resolves (and poisons — cross-binder).
        let false_branch = self.lower_annotation(scope, &cond.false_type);

        // Unwind the context: the binder count and the poison verdict.
        let frame = self.cond_frames.pop().unwrap_or_default();
        let infer_count = frame.binders.len() as u32;
        let poisoned = frame.poisoned;

        let (extends_ty, true_branch, false_branch) = match (extends_ty, true_branch, false_branch)
        {
            (Some(e), Some(t), Some(f)) => (e, t, f),
            // An out-of-subset component degrades the whole conditional to the error
            // type (the M22 discipline — the diagnostics for the component are
            // already emitted; the error type suppresses cascade).
            _ => return Some(error_ty),
        };

        let id = self.interner.intern_conditional(ConditionalType {
            check,
            extends_ty,
            true_branch,
            false_branch,
            infer_count,
            distributive,
            poisoned,
        });
        let span = Span::from_oxc(cond.span);
        Some(self.maybe_evaluate(id, span))
    }

    /// Lower `infer U` into the innermost active conditional frame. Repeated names
    /// reuse their de Bruijn index; outside `extends`/true positions it is out of
    /// subset (`None`).
    pub(super) fn lower_infer_type(&mut self, infer: &TSInferType<'_>) -> Option<TypeId> {
        let name = infer.type_parameter.name.name.as_str();
        let frame = self.cond_frames.iter_mut().rev().find(|f| f.active)?;
        let index = match frame.binders.get(name) {
            Some(&i) => i,
            None => {
                let i = frame.binders.len() as u32;
                frame.binders.insert(name.to_string(), i);
                i
            }
        };
        Some(self.interner.intern_infer(index))
    }

    /// Lower a template literal type `` `a${T}b${U}c` `` to its interned node (M27, WU1).
    ///
    /// The `quasis` become the ordered text segments and each interpolated `types[i]`
    /// becomes a hole (lowered recursively — a hole may be a literal, a union, a
    /// `string`/`number` intrinsic, an in-scope type parameter, or, inside a conditional's
    /// extends position, an `infer` binder). A hole that cannot be lowered (out of subset),
    /// or a quasi with no cooked value (an invalid escape), aborts the whole annotation
    /// (`None`), matching the object / union lowering.
    ///
    /// **Adjacent-`infer` poison (WU3):** two holes with no literal separator between them
    /// (an empty interior text) where either is an `infer` binder are out of the M27 subset
    /// (tsc's one-char-first resolution is not modelled). The enclosing conditional is
    /// **poisoned** via the M25 mechanism (the innermost active `infer` frame), so it never
    /// evaluates and relates conservatively (documented over-report divergence,
    /// `docs/reference/divergences.md`).
    ///
    /// At a value-position demand site the built node is **constructed**
    /// ([`Pass::maybe_evaluate`]); inside a template body it stays the interned node.
    pub(super) fn lower_template_type(
        &mut self,
        scope: ScopeId,
        template: &TSTemplateLiteralType<'_>,
    ) -> Option<TypeId> {
        // Text segments — the cooked quasi values (`texts.len() == holes.len() + 1`). An
        // invalid escape (`cooked == None`) is out of subset.
        let mut texts: Vec<String> = Vec::with_capacity(template.quasis.len());
        for quasi in &template.quasis {
            texts.push(quasi.value.cooked.as_ref()?.to_string());
        }

        // Holes — the interpolated types, lowered in order (an `infer` hole registers into
        // the active conditional frame, exactly like a bare `infer U`).
        let mut holes: Vec<TypeId> = Vec::with_capacity(template.types.len());
        for hole in &template.types {
            holes.push(self.lower_annotation(scope, hole)?);
        }

        // Adjacent-`infer` poison: an empty interior separator between two holes where
        // either is an `infer` node poisons the innermost active conditional frame.
        for i in 1..holes.len() {
            let separator_empty = texts.get(i).is_some_and(|t| t.is_empty());
            let adjacent_infer = self.interner.store().tag(holes[i - 1]) == TypeTag::Infer
                || self.interner.store().tag(holes[i]) == TypeTag::Infer;
            if separator_empty && adjacent_infer {
                if let Some(frame) = self.cond_frames.iter_mut().rev().find(|f| f.active) {
                    frame.poisoned = true;
                }
            }
        }

        let id = self.interner.intern_template(TemplateType { texts, holes });
        let span = Span::from_oxc(template.span);
        Some(self.maybe_evaluate(id, span))
    }

    /// Whether the conditional check type `check` surface-references the alias `decl_id`
    /// (a bare `TSTypeReference` whose name resolves to that declaration) — the `TK2456`
    /// circular-alias case (M25). Deliberately only the surface form (not through an
    /// object member), so recursion through structural members stays legal.
    fn check_surface_references(
        &self,
        scope: ScopeId,
        check: &TSType<'_>,
        decl_id: DeclId,
    ) -> bool {
        let TSType::TSTypeReference(reference) = check else {
            return false;
        };
        let TSTypeName::IdentifierReference(ident) = &reference.type_name else {
            return false;
        };
        type_decl_id(self.binder, scope, ident.name.as_str()) == Some(decl_id)
    }

    /// Resolve a type-name reference against the in-scope `infer` binders (M25),
    /// searching the ACTIVE conditional frames innermost-first. A hit on the frame of
    /// the node currently being built (the innermost context) resolves normally; a hit
    /// on an OUTER node's frame is a **cross-binder** reference (backlog 26 stopgap): it
    /// still resolves — no spurious `TK2304` — but POISONS every node from the
    /// referencing one up to and including the binder-owning one (a poisoned node never
    /// evaluates; conservative relations apply). A miss falls through to the ordinary
    /// type-reference resolution.
    pub(in crate::check::checker) fn resolve_infer_reference(
        &mut self,
        name: &str,
    ) -> Option<TypeId> {
        let innermost = self.cond_frames.len().checked_sub(1)?;
        let (owner, index) = self
            .cond_frames
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, frame)| frame.active)
            .find_map(|(pos, frame)| frame.binders.get(name).map(|&i| (pos, i)))?;
        if owner != innermost {
            for frame in &mut self.cond_frames[owner..] {
                frame.poisoned = true;
            }
        }
        Some(self.interner.intern_infer(index))
    }

    /// Evaluate `ty` at a demand site unless a **template** body is being lowered (M25).
    /// A template's conditional must survive as its interned node until instantiated;
    /// a value-position type is resolved eagerly.
    pub(in crate::check::checker) fn maybe_evaluate(&mut self, ty: TypeId, span: Span) -> TypeId {
        if self.building_template {
            ty
        } else {
            self.evaluate_type(ty, span)
        }
    }

    /// Lower a mapped type `{ [K in S]: V }` (M26, WU1).
    ///
    /// The `in` clause `S` is classified: `keyof <source>` is **homomorphic** (the key
    /// source is `<source>`, whose per-property `?`/`readonly` flags are preserved), any
    /// other constraint is non-homomorphic (the key source is the constraint type — a
    /// literal-union key set). The value template `V` is lowered with a
    /// [`MappedFrame`] pushed, so an indexed access on the key binder (`T[K]`) lowers to
    /// the node-scoped [`TypeTag::MappedValue`] placeholder. The `?`/`readonly` modifier
    /// operators are recorded. At a value-position demand site the built node is
    /// evaluated ([`Pass::maybe_evaluate`]); inside a template body it stays the interned
    /// node.
    ///
    /// Out of the M26 subset (aborting the annotation, degrading to the error type): an
    /// `as` key remapping (`name_type`), a missing value template, or an un-lowerable key
    /// source / value template.
    pub(super) fn lower_mapped_type(
        &mut self,
        scope: ScopeId,
        mapped: &TSMappedType<'_>,
    ) -> Option<TypeId> {
        // `as` key remapping is out of the M26 subset (backlog 11).
        if mapped.name_type.is_some() {
            return None;
        }

        // The key-source surface: the `keyof` operand for a homomorphic map, else the
        // constraint itself.
        let key_surface: &TSType<'_> = match &mapped.constraint {
            TSType::TSTypeOperatorType(op) if op.operator == TSTypeOperatorOperator::Keyof => {
                &op.type_annotation
            }
            other => other,
        };

        // TK2456: a key source that surface-references the alias being resolved
        // is circular; otherwise the re-entry error type would silently feed the
        // map. The alias degrades to the error type (M22); tsc's extra TS2313 is
        // documented but omitted.
        if let Some((decl_id, alias_span, name)) = self.resolving_alias.clone() {
            if self.check_surface_references(scope, key_surface, decl_id) {
                self.diagnostics
                    .push(Diagnostic::circular_type_alias(alias_span, &name));
                return Some(self.interner.well_known().error);
            }
        }

        // Classify the `in` clause: `keyof <source>` is homomorphic (preserves the
        // source's `?`/`readonly`); anything else is a non-homomorphic key set.
        let (homomorphic, key_source) = match &mapped.constraint {
            TSType::TSTypeOperatorType(op) if op.operator == TSTypeOperatorOperator::Keyof => {
                (true, self.lower_annotation(scope, &op.type_annotation)?)
            }
            other => (false, self.lower_annotation(scope, other)?),
        };

        // Lower the value template with this node's key binder in scope, so `X[K]`
        // becomes the source-value placeholder.
        self.mapped_frames.push(MappedFrame {
            key_name: mapped.key.name.to_string(),
            captured_source: None,
        });
        // B29: a mapped VALUE template is a legal-recursion boundary (`type MapRec =
        // string | { [K in "a" | "b"]: MapRec }`), so it lowers one indirection deeper.
        // The key source stays at surface depth (a self-referencing key set is the
        // TK2456 case, caught above / by surface re-entry).
        let value_template = match &mapped.type_annotation {
            Some(annotation) => self.with_indirection(|p| p.lower_annotation(scope, annotation)),
            // A mapped type with no value (`{ [K in S] }`) is out of subset.
            None => None,
        };
        let captured_source = self
            .mapped_frames
            .pop()
            .and_then(|frame| frame.captured_source);
        let value_template = value_template?;

        let id = self.interner.intern_mapped(MappedType {
            homomorphic,
            key_source,
            value_template,
            // M28: only a NON-homomorphic map carries a modifiers source (a
            // homomorphic map's `key_source` already IS its source object) — see
            // [`crate::types::repr::MappedType::modifiers_source`].
            modifiers_source: if homomorphic { None } else { captured_source },
            optional_modifier: modifier_op(mapped.optional),
            readonly_modifier: modifier_op(mapped.readonly),
        });
        let span = Span::from_oxc(mapped.span);
        Some(self.maybe_evaluate(id, span))
    }

    /// The interned type-parameter id a bare `TSTypeReference` (no type arguments)
    /// resolves to through the in-scope parameter frames, or `None` for any other
    /// shape (M28 — the `T[P]` modifiers-source capture; deliberately narrow so the
    /// capture never triggers extra lowering or diagnostics).
    pub(super) fn bare_type_param_reference(&self, ty: &TSType<'_>) -> Option<TypeId> {
        let TSType::TSTypeReference(reference) = ty else {
            return None;
        };
        if reference.type_arguments.is_some() {
            return None;
        }
        let TSTypeName::IdentifierReference(ident) = &reference.type_name else {
            return None;
        };
        self.lookup_type_param(ident.name.as_str())
    }

    /// Whether `index` names the **innermost active mapped key** binder (M26) — a bare
    /// `TSTypeReference` (no type arguments) whose name equals the current mapped
    /// frame's key. Used to recognize `T[K]` as the source-value placeholder inside a
    /// mapped type's value template.
    pub(super) fn index_is_active_mapped_key(&self, index: &TSType<'_>) -> bool {
        let Some(frame) = self.mapped_frames.last() else {
            return false;
        };
        let TSType::TSTypeReference(reference) = index else {
            return false;
        };
        let TSTypeName::IdentifierReference(ident) = &reference.type_name else {
            return false;
        };
        reference.type_arguments.is_none() && ident.name.as_str() == frame.key_name
    }

    /// Compute `keyof T` (M20, extended for M28). `T` is the already-lowered operand.
    ///
    /// A concrete **object** operand, or a union of object operands, keys **eagerly**
    /// through the shared [`keyof_of_type`] computation (the SAME one the evaluator's
    /// deferred path uses — single source of truth): object keys are property names
    /// plus index signatures, while union keys are the keys common to every member.
    ///
    /// M28: an operand that is a **pending type-level computation** — a free type
    /// parameter, a deferred conditional / mapped / instantiation / template / keyof,
    /// an `infer` binder, or the mapped-value placeholder — lowers to a **deferred**
    /// [`TypeTag::Keyof`] node: substitution rewrites its operand and the evaluator
    /// resolves it at a value-position demand (previously these collapsed to the
    /// permissive error type — a silent-false-negative generator).
    ///
    /// Anything else — a primitive, a union, an array, a tuple — stays the **M20
    /// out-of-scope error type** (no crash; the error type suppresses cascade), as
    /// does an error/`any` operand.
    pub(super) fn keyof_type(&mut self, operand: TypeId) -> TypeId {
        if let Some(keys) = super::super::eval::keyof_of_type(self.interner, operand) {
            return keys;
        }
        let wk = self.interner.well_known();
        match self.interner.store().tag(operand) {
            TypeTag::TypeParam
            | TypeTag::Conditional
            | TypeTag::Instantiation
            | TypeTag::Mapped
            | TypeTag::Template
            | TypeTag::Keyof
            | TypeTag::Infer
            | TypeTag::MappedValue => self.interner.intern_keyof(operand),
            // Primitives / unions / arrays / tuples: out of the M20 subset — the
            // error type (unchanged behaviour), never a deferred node that could not
            // possibly resolve.
            _ => wk.error,
        }
    }

    /// Compute an indexed-access type `T[K]` **eagerly** (M20). `object` and `index`
    /// are the already-lowered `T` and `K`.
    ///
    /// `K` is resolved by shape:
    ///
    ///  - a **union** key (`T["a" | "b"]`, or the result of `keyof T`) → the
    ///    `union(...)` of `T[member]` over each union member (so `T[keyof T]` yields the
    ///    union of all value types; `union` dedups, so `number | number` → `number`);
    ///  - a **string-literal** key → the named property's type, else the string index
    ///    value type, else the error type;
    ///  - a **number-literal** key → the matching tuple **element** (positional) when
    ///    `T` is a tuple, else the number index value type, else the error type;
    ///  - the `number` intrinsic key → the number index value type, else the error type;
    ///  - anything else (a generic key, a non-literal `string`, …) → the error type.
    ///
    /// Every out-of-scope / missing-key path returns the **error type** (no crash, no
    /// diagnostic — matching the M19 element-access leniency). An error/`any` object or
    /// key likewise yields the error type.
    pub(super) fn indexed_access_type(&mut self, object: TypeId, index: TypeId) -> TypeId {
        let wk = self.interner.well_known();

        if object == wk.error || object == wk.any || index == wk.error || index == wk.any {
            return wk.error;
        }
        let object = self
            .interner
            .store()
            .readonly_operand(object)
            .unwrap_or(object);

        let store = self.interner.store();

        // A union key distributes over its members: `T[A | B]` = `T[A] | T[B]`. This is
        // also how `T[keyof T]` reduces (the key is the union of the property-name
        // literals).
        if let Some(union_members) = store.union_members(index) {
            let members: Vec<TypeId> = union_members.to_vec();
            let resolved: Vec<TypeId> = members
                .into_iter()
                .map(|member| self.indexed_access_type(object, member))
                .collect();
            return self.interner.union(resolved);
        }

        self.indexed_access_single(object, index)
    }

    /// Resolve `T[K]` for a **non-union** key `K` (M20). Factored out of
    /// [`indexed_access_type`] so the union case can recurse per member. Returns the
    /// looked-up value type, or the error type for any missing-key / out-of-scope case.
    fn indexed_access_single(&mut self, object: TypeId, index: TypeId) -> TypeId {
        let wk = self.interner.well_known();
        let store = self.interner.store();

        // A string-literal key names a property (or selects the string index value).
        if let Some(LiteralValue::String(name)) = store.literal_value(index) {
            let name = name.clone();
            let store = self.interner.store();
            if let Some(obj) = store.object_type(object) {
                if let Some(prop) = obj.property(&name) {
                    return prop.ty;
                }
                if let Some(value) = obj.string_index {
                    return value;
                }
            }
            return wk.error;
        }

        // A number-literal key: a tuple's positional element, else the number index value.
        if let Some(LiteralValue::Number(n)) = store.literal_value(index) {
            // A tuple element, addressed positionally — reuse the same non-negative
            // whole-number-in-range check the M18 element access uses.
            if store.tag(object) == TypeTag::Tuple {
                if let Some(i) = whole_index(*n) {
                    if let Some(&element) = store.tuple_type(object).and_then(|t| t.elements.get(i))
                    {
                        return element;
                    }
                }
                return wk.error;
            }
            if let Some(array) = store.array_type(object) {
                return array.element;
            }
            if let Some(value) = store.object_type(object).and_then(|o| o.number_index) {
                return value;
            }
            return wk.error;
        }

        // The bare `number` intrinsic key → the number index value type (or error).
        if index == wk.number {
            if let Some(array) = store.array_type(object) {
                return array.element;
            }
            if let Some(value) = store.object_type(object).and_then(|o| o.number_index) {
                return value;
            }
            return wk.error;
        }

        // A singleton constrained key keeps `Map[K]` usable; multi-key correlation is
        // deferred to backlog 75 rather than guessed from the value union.
        if let Some(parameter) = store.type_param(index) {
            if let Some(constraint) = store.type_param_constraint(parameter.id) {
                if constraint != index {
                    return self.indexed_access_type(object, constraint);
                }
            }
        }

        // Any other key (a non-literal `string`, an unconstrained type parameter, …) is
        // out of the M20 scope → error type (no crash).
        wk.error
    }
}
