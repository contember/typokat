use super::*;

impl<'a> ConditionalEvaluator<'a> {
    /// **Construct** a template literal type (M27). When every hole is a string / number
    /// / boolean literal (or a union thereof) the template **collapses**: a single
    /// combination to a string literal, several to the cartesian-product **union**
    /// (canonicalized by `Interner::union`). A `never` hole short-circuits the whole
    /// template to `never`; a `boolean` hole expands to `"false" | "true"` before the
    /// product. A **non-literal** hole (`string`/`number` intrinsic, a free declaration
    /// type parameter, an `infer` binder, or any still-symbolic type) leaves the template
    /// a **symbolic pattern** — returned unchanged. The cartesian product iterates under
    /// the shared per-root step budget, so a combinatorial blow-up trips `TK2589` (via
    /// `exhausted`), never OOM. The result is committed through [`Task::SetMemo`], which
    /// refuses to commit under an exhausted budget — so a hole that resolved to error
    /// only because an unrelated earlier member drained the budget never poisons the
    /// pass-wide memo (backlog 55). A symbolic survivor memoizes to itself, idempotent.
    pub(super) fn eval_template(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let Some(template) = self.interner.store().template_type(ty).cloned() else {
            values.push(ty);
            return;
        };
        // A self-cycle, or a result reached under an already-exhausted budget, is the
        // error type and is NOT memoized (backlog 55 — the template path formerly
        // bypassed this gate and poisoned the shared memo; mirrors the other node
        // kinds and invariants §1).
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

        // M28: a hole may itself be a pending type-level computation (a substituted
        // string-intrinsic instantiation — the `Greet` composition — a conditional, a
        // keyof, …). Evaluate such holes through the shared work-stack FIRST, then
        // finish construction inline ([`Task::FinishTemplate`] never re-schedules, so
        // a hole that stays deferred cannot loop). The enclosing [`Task::SetMemo`]
        // commits the result under `ty` (skipped when exhausted — backlog 55).
        let needs_eval = template.holes.iter().any(|&h| self.arg_needs_pre_eval(h));
        if needs_eval {
            tasks.push(Task::SetMemo(ty));
            tasks.push(Task::FinishTemplate(ty));
            for &hole in template.holes.iter().rev() {
                tasks.push(Task::Eval(hole));
            }
            return;
        }

        tasks.push(Task::SetMemo(ty));
        let holes = template.holes.clone();
        self.finish_template_with_holes(ty, &template, holes, values, error);
    }

    /// Finish a template whose evaluable holes were pre-evaluated (M28): pop one value
    /// per hole (in order) and construct inline.
    pub(super) fn finish_template(&mut self, ty: TypeId, values: &mut Vec<TypeId>, error: TypeId) {
        let Some(template) = self.interner.store().template_type(ty).cloned() else {
            values.push(ty);
            return;
        };
        let start = values.len().saturating_sub(template.holes.len());
        let holes: Vec<TypeId> = values.split_off(start);
        self.finish_template_with_holes(ty, &template, holes, values, error);
    }

    /// The template construction core (M27, factored for M28's hole pre-evaluation):
    /// classify the (possibly re-evaluated) `holes`, then collapse / short-circuit /
    /// stay symbolic exactly as before. Pushes exactly one result value; the enclosing
    /// [`Task::SetMemo`] (scheduled by [`Self::eval_template`]) commits it under the
    /// ORIGINAL node id `ty` — but never under an exhausted budget (backlog 55). A
    /// symbolic survivor whose holes changed re-interns over the resolved holes.
    pub(super) fn finish_template_with_holes(
        &mut self,
        ty: TypeId,
        template: &TemplateType,
        holes: Vec<TypeId>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let wk = self.interner.well_known();

        // M22 discipline: an error-typed hole (an unresolved name upstream) degrades the
        // whole template to the error type so cascades stay suppressed — mirroring
        // `assemble_mapped`'s error/any key-source handling. Commit via the enclosing
        // `Task::SetMemo` (backlog 55 — never a direct insert that ignores exhaustion).
        if holes.contains(&wk.error) {
            values.push(error);
            return;
        }

        // Classify each hole; a `never` hole makes the whole template `never`, a
        // non-literal hole keeps it symbolic, otherwise it is a cartesian factor.
        let mut factors: Vec<Vec<String>> = Vec::with_capacity(holes.len());
        for &hole in &holes {
            match self.hole_parts(hole) {
                HolePart::Never => {
                    values.push(wk.never);
                    return;
                }
                HolePart::NonLiteral => {
                    // A symbolic pattern (string/number intrinsic, free param, a
                    // still-symbolic intrinsic application, …) — keep the node
                    // symbolic, un-memoized (idempotent). Holes that DID evaluate are
                    // baked in (re-interned) so relations see the resolved form.
                    let node = if holes == template.holes {
                        ty
                    } else {
                        self.interner.intern_template(TemplateType {
                            texts: template.texts.clone(),
                            holes,
                        })
                    };
                    values.push(node);
                    return;
                }
                HolePart::Literals(parts) => factors.push(parts),
            }
        }

        // All-literal holes: build the cartesian product of text + hole combinations,
        // metering each combination against the shared step budget.
        let empty = String::new();
        let mut acc: Vec<String> = vec![template.texts.first().cloned().unwrap_or_default()];
        for (i, factor) in factors.iter().enumerate() {
            let sep = template.texts.get(i + 1).unwrap_or(&empty);
            let mut next: Vec<String> = Vec::with_capacity(acc.len().saturating_mul(factor.len()));
            for prefix in &acc {
                for part in factor {
                    self.steps += 1;
                    if self.steps > self.budget {
                        self.exhausted = true;
                        values.push(error);
                        return;
                    }
                    next.push(format!("{prefix}{part}{sep}"));
                }
            }
            acc = next;
        }

        let members: Vec<TypeId> = acc
            .into_iter()
            .map(|s| self.interner.intern_literal(LiteralValue::String(s)))
            .collect();
        let result = self.interner.union(members);
        values.push(result);
    }

    /// Classify a template hole for construction. Literal string/number/boolean
    /// inputs feed the cartesian product, `never` short-circuits, and non-literal
    /// inputs leave the template symbolic.
    pub(super) fn hole_parts(&self, hole: TypeId) -> HolePart {
        let wk = self.interner.well_known();
        let store = self.interner.store();
        if hole == wk.never {
            return HolePart::Never;
        }
        if hole == wk.boolean {
            return HolePart::Literals(vec!["false".to_string(), "true".to_string()]);
        }
        if let Some(lit) = store.literal_value(hole) {
            return HolePart::Literals(vec![literal_to_string(lit)]);
        }
        if let Some(members) = store.union_members(hole) {
            // Every member must itself be constructible (a `never` member cannot occur —
            // the interner drops it from a union). A non-literal member keeps the whole
            // template symbolic.
            let mut parts: Vec<String> = Vec::with_capacity(members.len());
            for &member in members {
                match self.hole_parts(member) {
                    HolePart::Literals(sub) => parts.extend(sub),
                    HolePart::Never => return HolePart::NonLiteral,
                    HolePart::NonLiteral => return HolePart::NonLiteral,
                }
            }
            return HolePart::Literals(parts);
        }
        // A `string`/`number` intrinsic, a free type parameter, an `infer` binder, or any
        // other symbolic type — not constructible.
        HolePart::NonLiteral
    }

    /// Apply a string intrinsic to its (freshly evaluated) argument (M28/WU3): a string
    /// literal transforms (Rust `to_uppercase`/`to_lowercase`; Capitalize/Uncapitalize
    /// touch the first char only); a union distributes per member; an error/any
    /// argument degrades to the error type (M22); anything else (a template pattern,
    /// the `string` intrinsic, a free parameter) stays a **symbolic** instantiation —
    /// rebuilt over the evaluated argument — relating conservatively (identical-node;
    /// → `string`).
    pub(super) fn apply_string_intrinsic(&mut self, ty: TypeId, values: &mut Vec<TypeId>, error: TypeId) {
        let arg = values.pop().unwrap_or(error);
        let Some(inst) = self.interner.store().instantiation_type(ty).cloned() else {
            values.push(ty);
            return;
        };
        let wk = self.interner.well_known();
        if arg == wk.error || arg == wk.any {
            values.push(error);
            return;
        }
        let Some(&(param, _)) = inst.args.first() else {
            values.push(ty);
            return;
        };
        let members: Vec<TypeId> = match self.interner.store().union_members(arg) {
            Some(members) => members.to_vec(),
            None => vec![arg],
        };
        let mut results: Vec<TypeId> = Vec::with_capacity(members.len());
        for member in members {
            let transformed =
                self.interner
                    .store()
                    .literal_value(member)
                    .and_then(|lit| match lit {
                        LiteralValue::String(s) => Some(transform_string_intrinsic(
                            &self.interner.well_known(),
                            inst.base,
                            s,
                        )),
                        _ => None,
                    });
            match transformed {
                Some(out) => results.push(self.interner.intern_literal(LiteralValue::String(out))),
                // A non-string-literal member stays a symbolic per-member application.
                // Hash-consing makes a rebuild over the unchanged single argument THE
                // original node, so identical-node relations stay total.
                None => results.push(
                    self.interner
                        .intern_instantiation(inst.base, vec![(param, member)]),
                ),
            }
        }
        // A 1-member list collapses through `union` to that member.
        let result = self.interner.union(results);
        values.push(result);
    }
}

/// Apply an M28 string intrinsic to a string literal. Upper/lowercase map the
/// whole string; capitalize/uncapitalize map only the first char.
fn transform_string_intrinsic(wk: &crate::types::WellKnown, base: TypeId, s: &str) -> String {
    if base == wk.uppercase {
        return s.to_uppercase();
    }
    if base == wk.lowercase {
        return s.to_lowercase();
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let mapped: String = if base == wk.capitalize {
                first.to_uppercase().collect()
            } else {
                first.to_lowercase().collect()
            };
            mapped + chars.as_str()
        }
        None => String::new(),
    }
}
