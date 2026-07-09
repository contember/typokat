//! annotations module (extracted from checker/mod.rs).

use super::calls::parameter_name;
use super::context::*;
use super::decls::type_decl_id;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{
    ConditionalType, FunctionType, LiteralValue, MappedType, ModifierOp, ObjectType, ParameterType,
    PropertyType, TemplateType, TypeTag,
};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    Expression, FormalParameters, TSCallSignatureDeclaration, TSConditionalType,
    TSConstructSignatureDeclaration, TSConstructorType, TSInferType, TSLiteral, TSMappedType,
    TSMappedTypeModifierOperator, TSMethodSignature, TSMethodSignatureKind, TSSignature,
    TSTemplateLiteralType, TSTupleElement, TSType, TSTypeAnnotation, TSTypeName,
    TSTypeOperatorOperator, UnaryOperator,
};
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower an annotation type to its `TypeId`. Type references resolve to stored
    /// declaration ids, never inlined structures, so recursive aliases/interfaces
    /// terminate by pointing at the reserved id (mvp-plan M5, §3, §6.3).
    pub(in crate::check::checker) fn lower_annotation(
        &mut self,
        scope: ScopeId,
        ts_type: &TSType<'_>,
    ) -> Option<TypeId> {
        let wk = self.interner.well_known();
        let id = match ts_type {
            TSType::TSAnyKeyword(_) => wk.any,
            TSType::TSUnknownKeyword(_) => wk.unknown,
            TSType::TSNeverKeyword(_) => wk.never,
            TSType::TSVoidKeyword(_) => wk.void,
            TSType::TSNullKeyword(_) => wk.null,
            TSType::TSUndefinedKeyword(_) => wk.undefined,
            TSType::TSBooleanKeyword(_) => wk.boolean,
            TSType::TSNumberKeyword(_) => wk.number,
            TSType::TSStringKeyword(_) => wk.string,
            // M8: a **literal type** (`"hello"`, `42`, `true`) lowers to its interned
            // literal id. This is what makes union-of-literals (`"a" | "b"`) and the
            // discriminant property (`kind: "circle"`) carry literal types.
            TSType::TSLiteralType(lit) => return self.lower_literal_type(&lit.literal),
            TSType::TSTypeLiteral(lit) => return self.lower_object_annotation(scope, &lit.members),
            TSType::TSFunctionType(func) => {
                return self.lower_function_annotation(
                    scope,
                    &func.params,
                    &func.return_type.type_annotation,
                );
            }
            TSType::TSConstructorType(ctor) => return self.lower_constructor_type(scope, ctor),
            TSType::TSUnionType(union) => return self.lower_union_annotation(scope, &union.types),
            // M31: an intersection type `A & B`. Each member is lowered recursively, then
            // `Interner::intersection` canonicalizes (flatten, absorb, drop `unknown`,
            // sort+dedup, collapse). Mirrors the union lowering — no `with_indirection`.
            TSType::TSIntersectionType(intersection) => {
                return self.lower_intersection_annotation(scope, &intersection.types);
            }
            TSType::TSParenthesizedType(paren) => {
                return self.lower_annotation(scope, &paren.type_annotation);
            }
            // M17: an array type `T[]`. The element is lowered recursively (so `T[][]`
            // nests), then interned as an array type. A non-lowerable element (out of
            // subset) aborts the whole annotation (`None`), matching the other lowerings.
            TSType::TSArrayType(array) => {
                let element =
                    self.with_indirection(|p| p.lower_annotation(scope, &array.element_type))?;
                return Some(self.interner.intern_array(element));
            }
            // M18: a tuple type `[A, B]`. Each element is lowered recursively (so nested
            // tuples / arrays work) and the ordered list is interned. Named/optional/rest
            // tuple elements are out of the M18 subset and abort the whole annotation
            // (`None`) — see [`lower_tuple_annotation`].
            TSType::TSTupleType(tuple) => {
                return self.lower_tuple_annotation(scope, &tuple.element_types);
            }
            // M5/M9: a type reference (`Point`, `Num`, `List`, `Box<number>`, an
            // in-scope type parameter `T`) resolves through the type-parameter scope,
            // the binder's type slot, and (with arguments) generic instantiation.
            // Qualified names (`A.B`) are out of subset.
            TSType::TSTypeReference(reference) => {
                return self.resolve_type_reference(
                    scope,
                    &reference.type_name,
                    reference.type_arguments.as_deref(),
                );
            }
            // M20: `keyof T` is computed eagerly on a concrete object type
            // ([`keyof_type`]). b64: `readonly` over array/tuple syntax is lowered to a
            // readonly-only wrapper so conditional `infer` binders under that syntax are
            // traversed without making readonly sources behave like mutable arrays.
            TSType::TSTypeOperatorType(op) => {
                if op.operator == TSTypeOperatorOperator::Keyof {
                    let operand = self.lower_annotation(scope, &op.type_annotation)?;
                    return Some(self.keyof_type(operand));
                }
                if op.operator == TSTypeOperatorOperator::Readonly {
                    return self.lower_readonly_array_or_tuple(scope, &op.type_annotation);
                }
                return None;
            }
            // M20 eager `T[K]`, except inside an M26 mapped value template where
            // `X[K]` on the active key lowers to [`TypeTag::MappedValue`]. That
            // placeholder is resolved per key at evaluation; eager lookup would see
            // the still-abstract source and collapse to the error type.
            TSType::TSIndexedAccessType(access) => {
                if self.index_is_active_mapped_key(&access.index_type) {
                    // M28: capture the `T` of `T[P]` for homomorphic modifiers only
                    // in the bare type-parameter (`Pick`) shape. First capture wins.
                    if let Some(param_ty) = self.bare_type_param_reference(&access.object_type) {
                        if let Some(frame) = self.mapped_frames.last_mut() {
                            frame.captured_source.get_or_insert(param_ty);
                        }
                    }
                    return Some(self.interner.intern_mapped_value());
                }
                let object = self.lower_annotation(scope, &access.object_type)?;
                let index = self.lower_annotation(scope, &access.index_type)?;
                return Some(self.indexed_access_type(object, index));
            }
            // M26: a mapped type `{ [K in S]: V }`. Lowered to an interned node (WU1)
            // and, at a value-position demand site, evaluated (WU2) — see
            // [`lower_mapped_type`].
            TSType::TSMappedType(mapped) => return self.lower_mapped_type(scope, mapped),
            // M25: a conditional type `C extends E ? T : F`. Lowered to an interned node
            // (WU1) and, at a value-position demand site, evaluated (WU2) — see
            // [`lower_conditional_type`].
            TSType::TSConditionalType(cond) => return self.lower_conditional_type(scope, cond),
            // M25: an `infer U` binder — only meaningful inside a conditional's `extends`
            // position (where an infer frame is active). Elsewhere it is out of subset.
            TSType::TSInferType(infer) => return self.lower_infer_type(infer),
            // M27: a template literal type `` `a${T}b` ``. Lowered to an interned node
            // (WU1) and, at a value-position demand site, constructed (collapse / cartesian
            // union) — see [`lower_template_type`].
            TSType::TSTemplateLiteralType(template) => {
                return self.lower_template_type(scope, template);
            }
            _ => return None,
        };
        Some(id)
    }

    /// B29: run `f` one legal-recursion boundary deeper. Balanced even through
    /// early returns; re-entering a resolving alias at greater depth is recursion,
    /// not a surface cycle (see [`Pass::resolve_type_decl`]).
    fn with_indirection<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.alias_indirection_depth += 1;
        let result = f(self);
        self.alias_indirection_depth -= 1;
        result
    }

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
    fn lower_conditional_type(
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

        let (extends_ty, true_branch, false_branch) =
            match (extends_ty, true_branch, false_branch) {
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
    fn lower_infer_type(&mut self, infer: &TSInferType<'_>) -> Option<TypeId> {
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
    fn lower_template_type(
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
    fn check_surface_references(&self, scope: ScopeId, check: &TSType<'_>, decl_id: DeclId) -> bool {
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
    pub(in crate::check::checker) fn resolve_infer_reference(&mut self, name: &str) -> Option<TypeId> {
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
    fn lower_mapped_type(&mut self, scope: ScopeId, mapped: &TSMappedType<'_>) -> Option<TypeId> {
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
    fn bare_type_param_reference(&self, ty: &TSType<'_>) -> Option<TypeId> {
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
    fn index_is_active_mapped_key(&self, index: &TSType<'_>) -> bool {
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

    /// Lower `A | B | …` to a canonical interned union. Any unlowerable member
    /// aborts the whole annotation; dropping it would mis-state the union.
    fn lower_union_annotation(&mut self, scope: ScopeId, members: &[TSType<'_>]) -> Option<TypeId> {
        let mut lowered: Vec<TypeId> = Vec::with_capacity(members.len());
        for member in members {
            lowered.push(self.lower_annotation(scope, member)?);
        }
        Some(self.interner.union(lowered))
    }

    /// Lower `A & B & …` to a canonical interned intersection. Any unlowerable
    /// member aborts the whole annotation; dropping it would mis-state the type.
    fn lower_intersection_annotation(
        &mut self,
        scope: ScopeId,
        members: &[TSType<'_>],
    ) -> Option<TypeId> {
        let mut lowered: Vec<TypeId> = Vec::with_capacity(members.len());
        for member in members {
            lowered.push(self.lower_annotation(scope, member)?);
        }
        Some(self.interner.intersection(lowered))
    }

    /// Lower `[A, B, …]` to an ordered interned tuple; `[]` is the empty tuple.
    /// Optional, rest, and named tuple members are out of subset and abort the
    /// annotation rather than silently mis-shaping the tuple.
    fn lower_tuple_annotation(
        &mut self,
        scope: ScopeId,
        elements: &[TSTupleElement<'_>],
    ) -> Option<TypeId> {
        let mut lowered: Vec<TypeId> = Vec::with_capacity(elements.len());
        for element in elements {
            // A plain positional element exposes its underlying `TSType`; an optional /
            // rest element does not (`as_ts_type` → `None`) and is out of subset.
            let ts_type = element.as_ts_type()?;
            lowered.push(self.with_indirection(|p| p.lower_annotation(scope, ts_type))?);
        }
        Some(self.interner.intern_tuple(lowered))
    }

    /// Lower a literal type to its hash-consed literal id, including unary-minus
    /// numeric literals. Bigint/template/other unary literals are out of subset and
    /// abort the enclosing annotation.
    fn lower_literal_type(&mut self, literal: &TSLiteral<'_>) -> Option<TypeId> {
        let value = match literal {
            TSLiteral::StringLiteral(s) => LiteralValue::String(s.value.to_string()),
            TSLiteral::NumericLiteral(n) => LiteralValue::Number(n.value),
            TSLiteral::BooleanLiteral(b) => LiteralValue::Boolean(b.value),
            // `-<numeric literal>` is a negative number literal type. tsc interns `-0`
            // and `0` to the same literal (SameValueZero), so collapse `-0.0` to `0.0`.
            TSLiteral::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation =>
            {
                let Expression::NumericLiteral(n) = &unary.argument else {
                    return None;
                };
                let negated = -n.value;
                LiteralValue::Number(if negated == 0.0 { 0.0 } else { negated })
            }
            // `bigint`, template-literal types, and other unary literal types (`+1`,
            // `-1n`, `~1`) are out of the M8 subset.
            TSLiteral::BigIntLiteral(_)
            | TSLiteral::TemplateLiteral(_)
            | TSLiteral::UnaryExpression(_) => return None,
        };
        Some(self.interner.intern_literal(value))
    }

    /// Lower object type literal members to a structural object. Optional members
    /// intern `T | undefined` here; string/number indexes coexist with named
    /// properties; unsupported or unlowerable members abort the whole object.
    fn lower_object_annotation(
        &mut self,
        scope: ScopeId,
        members: &[TSSignature<'_>],
    ) -> Option<TypeId> {
        let mut object = ObjectType::default();
        let overloaded_method_names = self.overloaded_method_names(members);
        let call_signatures_overloaded = self.call_signatures_overloaded(members);
        let construct_signatures_overloaded = self.construct_signatures_overloaded(members);
        for member in members {
            match member {
                TSSignature::TSPropertySignature(sig) => {
                    let name = sig.key.static_name()?;
                    if overloaded_method_names.contains(name.as_ref()) {
                        continue;
                    }
                    let annotation = sig.type_annotation.as_ref()?;
                    // B29: an object member is a legal-recursion boundary (`type W = { a: W
                    // } | null`), so lower it at a deeper indirection level.
                    let ty = self
                        .with_indirection(|p| p.lower_annotation(scope, &annotation.type_annotation))?;
                    // M21: optional properties intern `T | undefined` here, matching
                    // interface members and keeping this out of the relation engine.
                    let ty = if sig.optional {
                        let undefined = self.interner.well_known().undefined;
                        self.interner.union(vec![ty, undefined])
                    } else {
                        ty
                    };
                    let mut prop = PropertyType::public(name.into_owned(), ty);
                    prop.optional = sig.optional;
                    // F5/backlog-03: `readonly` is structural identity and gates
                    // assignment targets (`TK2540`), but does not affect assignability.
                    prop.readonly = sig.readonly;
                    object.properties.push(prop);
                }
                // M19: an index signature `[k: string]: T` / `[i: number]: T`.
                TSSignature::TSIndexSignature(sig) => {
                    self.lower_index_signature(scope, sig, &mut object)?;
                }
                TSSignature::TSMethodSignature(sig) => {
                    let name = sig.key.static_name()?;
                    if overloaded_method_names.contains(name.as_ref()) {
                        return None;
                    }
                    let prop = self.lower_method_signature_property(scope, sig)?;
                    object.properties.push(prop);
                }
                TSSignature::TSCallSignatureDeclaration(sig) => {
                    if call_signatures_overloaded {
                        return None;
                    }
                    let signature = self.lower_call_signature(scope, sig)?;
                    object.call_signatures.push(signature);
                }
                TSSignature::TSConstructSignatureDeclaration(sig) => {
                    if construct_signatures_overloaded {
                        return None;
                    }
                    let signature = self.lower_construct_signature(scope, sig)?;
                    object.construct_signatures.push(signature);
                }
            }
        }
        Some(self.interner.intern_object(object))
    }

    /// Lower a WU1 method signature: required, non-generic, non-`this`,
    /// non-accessor, static name. Optional methods stay out of subset.
    pub(in crate::check::checker) fn lower_method_signature_property(
        &mut self,
        scope: ScopeId,
        sig: &TSMethodSignature<'_>,
    ) -> Option<PropertyType> {
        if sig.kind != TSMethodSignatureKind::Method
            || sig.type_parameters.is_some()
            || sig.this_param.is_some()
            || sig.optional
        {
            return None;
        }

        let name = sig.key.static_name()?;
        let ty = self.lower_strict_signature_function_type(
            scope,
            &sig.params,
            sig.return_type.as_deref(),
        )?;
        Some(PropertyType::public(name.into_owned(), ty))
    }

    /// Lower a WU2 call signature: required, non-generic, non-`this`, non-rest.
    /// Other signatures stay out of subset and do not create callability.
    pub(in crate::check::checker) fn lower_call_signature(
        &mut self,
        scope: ScopeId,
        sig: &TSCallSignatureDeclaration<'_>,
    ) -> Option<TypeId> {
        if sig.type_parameters.is_some() || sig.this_param.is_some() {
            return None;
        }
        self.lower_strict_signature_function_type(scope, &sig.params, sig.return_type.as_deref())
    }

    /// Lower a WU3 construct signature: required, non-generic, non-rest, with a
    /// representable instance type. Other signatures do not create constructability.
    pub(in crate::check::checker) fn lower_construct_signature(
        &mut self,
        scope: ScopeId,
        sig: &TSConstructSignatureDeclaration<'_>,
    ) -> Option<TypeId> {
        if sig.type_parameters.is_some() {
            return None;
        }
        let return_type = sig.return_type.as_deref()?;
        if !self.signature_annotations_are_locally_resolvable(scope, &sig.params, return_type) {
            return None;
        }
        self.lower_strict_construct_function_type(scope, &sig.params, return_type)
    }

    /// Lower a constructor-type annotation (`new (x: T) => U`) to an object carrying
    /// a single construct signature, making it equivalent to `{ new (x: T): U }` in
    /// relation while preserving named object members for the object-literal form.
    fn lower_constructor_type(
        &mut self,
        scope: ScopeId,
        ctor: &TSConstructorType<'_>,
    ) -> Option<TypeId> {
        if ctor.r#abstract || ctor.type_parameters.is_some() {
            return None;
        }
        if !self.signature_annotations_are_locally_resolvable(scope, &ctor.params, &ctor.return_type)
        {
            return None;
        }
        let signature =
            self.lower_strict_construct_function_type(scope, &ctor.params, &ctor.return_type)?;
        Some(self.interner.intern_object(ObjectType {
            construct_signatures: vec![signature],
            ..Default::default()
        }))
    }

    fn lower_strict_construct_function_type(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: &TSTypeAnnotation<'_>,
    ) -> Option<TypeId> {
        self.signature_params_in_subset(params)?;
        let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
        for param in &params.items {
            let name = parameter_name(&param.pattern)?;
            let annotation = param.type_annotation.as_ref()?;
            // B29: a parameter/return is a legal-recursion boundary.
            let ty = self
                .with_indirection(|p| p.lower_annotation(scope, &annotation.type_annotation))?;
            lowered.push(ParameterType {
                name,
                ty,
                optional: false,
            });
        }
        let ret =
            self.with_indirection(|p| p.lower_annotation(scope, &return_type.type_annotation))?;
        Some(self.interner.intern_function(FunctionType {
            params: lowered,
            ret,
        }))
    }

    fn signature_annotations_are_locally_resolvable(
        &self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: &TSTypeAnnotation<'_>,
    ) -> bool {
        let params_ok = params.items.iter().all(|param| {
            param
                .type_annotation
                .as_ref()
                .is_some_and(|ann| {
                    self.annotation_type_refs_are_locally_resolvable(scope, &ann.type_annotation)
                })
        });
        params_ok
            && self.annotation_type_refs_are_locally_resolvable(scope, &return_type.type_annotation)
    }

    fn annotation_type_refs_are_locally_resolvable(&self, scope: ScopeId, ty: &TSType<'_>) -> bool {
        match ty {
            TSType::TSAnyKeyword(_)
            | TSType::TSUnknownKeyword(_)
            | TSType::TSNeverKeyword(_)
            | TSType::TSVoidKeyword(_)
            | TSType::TSNullKeyword(_)
            | TSType::TSUndefinedKeyword(_)
            | TSType::TSBooleanKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSStringKeyword(_)
            | TSType::TSLiteralType(_) => true,
            TSType::TSTypeReference(reference) => {
                let TSTypeName::IdentifierReference(ident) = &reference.type_name else {
                    return false;
                };
                let name = ident.name.as_str();
                if name == "Array" {
                    let Some(args) = reference.type_arguments.as_deref() else {
                        return false;
                    };
                    let [arg] = args.params.as_slice() else {
                        return false;
                    };
                    return self.annotation_type_refs_are_locally_resolvable(scope, arg);
                }
                let found = self.lookup_type_param(name).is_some()
                    || type_decl_id(self.binder, scope, name).is_some();
                found
                    && reference.type_arguments.as_ref().is_none_or(|args| {
                        args.params
                            .iter()
                            .all(|arg| self.annotation_type_refs_are_locally_resolvable(scope, arg))
                    })
            }
            TSType::TSTypeLiteral(lit) => lit.members.iter().all(|member| match member {
                TSSignature::TSPropertySignature(sig) => sig
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| {
                        self.annotation_type_refs_are_locally_resolvable(
                            scope,
                            &ann.type_annotation,
                        )
                    }),
                TSSignature::TSIndexSignature(sig) => {
                    sig.parameters.iter().all(|param| {
                        self.annotation_type_refs_are_locally_resolvable(
                            scope,
                            &param.type_annotation.type_annotation,
                        )
                    }) && self.annotation_type_refs_are_locally_resolvable(
                        scope,
                        &sig.type_annotation.type_annotation,
                    )
                }
                TSSignature::TSMethodSignature(sig) => sig.return_type.as_ref().is_some_and(|ret| {
                    self.signature_annotations_are_locally_resolvable(scope, &sig.params, ret)
                }),
                TSSignature::TSCallSignatureDeclaration(sig) => {
                    sig.return_type.as_ref().is_some_and(|ret| {
                        self.signature_annotations_are_locally_resolvable(scope, &sig.params, ret)
                    })
                }
                TSSignature::TSConstructSignatureDeclaration(sig) => {
                    sig.return_type.as_ref().is_some_and(|ret| {
                        self.signature_annotations_are_locally_resolvable(scope, &sig.params, ret)
                    })
                }
            }),
            TSType::TSFunctionType(func) => {
                self.signature_annotations_are_locally_resolvable(
                    scope,
                    &func.params,
                    &func.return_type,
                )
            }
            TSType::TSConstructorType(ctor) => {
                !ctor.r#abstract
                    && ctor.type_parameters.is_none()
                    && self.signature_annotations_are_locally_resolvable(
                        scope,
                        &ctor.params,
                        &ctor.return_type,
                    )
            }
            TSType::TSUnionType(union) => union
                .types
                .iter()
                .all(|member| self.annotation_type_refs_are_locally_resolvable(scope, member)),
            // M31: an intersection is locally resolvable iff every member is (mirrors union).
            TSType::TSIntersectionType(intersection) => intersection
                .types
                .iter()
                .all(|member| self.annotation_type_refs_are_locally_resolvable(scope, member)),
            TSType::TSParenthesizedType(paren) => {
                self.annotation_type_refs_are_locally_resolvable(scope, &paren.type_annotation)
            }
            TSType::TSArrayType(array) => {
                self.annotation_type_refs_are_locally_resolvable(scope, &array.element_type)
            }
            TSType::TSTupleType(tuple) => tuple.element_types.iter().all(|element| {
                element
                    .as_ts_type()
                    .is_some_and(|ty| self.annotation_type_refs_are_locally_resolvable(scope, ty))
            }),
            TSType::TSTypeOperatorType(op) => {
                op.operator == TSTypeOperatorOperator::Keyof
                    && self.annotation_type_refs_are_locally_resolvable(scope, &op.type_annotation)
            }
            TSType::TSIndexedAccessType(access) => {
                self.annotation_type_refs_are_locally_resolvable(scope, &access.object_type)
                    && self.annotation_type_refs_are_locally_resolvable(scope, &access.index_type)
            }
            _ => false,
        }
    }

    /// Lower object/interface signatures only when every parameter and return type
    /// is representable. Unlike class/free-function lowering, bad parameters do
    /// not become the error type because that would create fake callability.
    fn lower_strict_signature_function_type(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: Option<&TSTypeAnnotation<'_>>,
    ) -> Option<TypeId> {
        self.signature_params_in_subset(params)?;
        let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
        for param in &params.items {
            let name = parameter_name(&param.pattern)?;
            let annotation = param.type_annotation.as_ref()?;
            let ty = self.lower_annotation(scope, &annotation.type_annotation)?;
            lowered.push(ParameterType {
                name,
                ty,
                optional: false,
            });
        }
        let ret = match return_type {
            Some(ann) => self.lower_annotation(scope, &ann.type_annotation)?,
            None => self.interner.well_known().void,
        };
        Some(self.interner.intern_function(FunctionType {
            params: lowered,
            ret,
        }))
    }

    /// Lower a function-like signature. Bad parameter annotations become the error
    /// type to suppress cascades; missing returns become `void`.
    pub(in crate::check::checker) fn lower_signature_function_type(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: Option<&TSTypeAnnotation<'_>>,
    ) -> Option<TypeId> {
        let void_ty = self.interner.well_known().void;
        let params = self.lower_signature_parameters(scope, params);
        let ret = match return_type {
            Some(ann) => self.lower_annotation(scope, &ann.type_annotation)?,
            None => void_ty,
        };
        Some(self.interner.intern_function(FunctionType { params, ret }))
    }

    /// Lower positional signature parameters for function-typed properties and class
    /// method/constructor signatures.
    pub(in crate::check::checker) fn lower_signature_parameters(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
    ) -> Vec<ParameterType> {
        let error_ty = self.interner.well_known().error;
        let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
        for param in &params.items {
            let name = parameter_name(&param.pattern).unwrap_or_default();
            let ty = match param.type_annotation.as_ref() {
                Some(ann) => self
                    .lower_annotation(scope, &ann.type_annotation)
                    .unwrap_or(error_ty),
                None => error_ty,
            };
            lowered.push(ParameterType {
                name,
                ty,
                optional: false,
            });
        }
        lowered
    }

    /// Required-only parameter subset shared by WU2 call signatures and the
    /// function-type annotation lowerer. Rest and optional parameters need
    /// different arity/relation rules, so accepting them as required would
    /// mis-state the type.
    fn signature_params_in_subset(&self, params: &FormalParameters<'_>) -> Option<()> {
        if params.rest.is_some() {
            return None;
        }
        for param in &params.items {
            if param.optional || param.initializer.is_some() {
                return None;
            }
        }
        Some(())
    }

    pub(in crate::check::checker) fn overloaded_method_names(
        &self,
        members: &[TSSignature<'_>],
    ) -> FxHashSet<String> {
        let mut counts: FxHashMap<String, (usize, bool)> = FxHashMap::default();
        for member in members {
            let (name, is_method) = match member {
                TSSignature::TSPropertySignature(sig) => (sig.key.static_name(), false),
                TSSignature::TSMethodSignature(sig) => (sig.key.static_name(), true),
                _ => (None, false),
            };
            let Some(name) = name else {
                continue;
            };
            let entry = counts.entry(name.into_owned()).or_insert((0, false));
            entry.0 += 1;
            entry.1 |= is_method;
        }
        counts
            .into_iter()
            .filter_map(|(name, (count, has_method))| {
                if count > 1 && has_method {
                    Some(name)
                } else {
                    None
                }
            })
            .collect()
    }

    pub(in crate::check::checker) fn call_signatures_overloaded(
        &self,
        members: &[TSSignature<'_>],
    ) -> bool {
        members
            .iter()
            .filter(|member| matches!(member, TSSignature::TSCallSignatureDeclaration(_)))
            .count()
            > 1
    }

    pub(in crate::check::checker) fn construct_signatures_overloaded(
        &self,
        members: &[TSSignature<'_>],
    ) -> bool {
        members
            .iter()
            .filter(|member| matches!(member, TSSignature::TSConstructSignatureDeclaration(_)))
            .count()
            > 1
    }

    /// Lower an M19 index signature into `object`. Only `[k: string]: T` and
    /// `[i: number]: T` are represented; malformed, unsupported-key, or unlowerable
    /// signatures abort the enclosing annotation. `readonly` indexes are deferred.
    pub(in crate::check::checker) fn lower_index_signature(
        &mut self,
        scope: ScopeId,
        sig: &oxc_ast::ast::TSIndexSignature<'_>,
        object: &mut ObjectType,
    ) -> Option<()> {
        // Exactly one key parameter (`[k: string]`); anything else is malformed.
        let [param] = sig.parameters.as_slice() else {
            return None;
        };
        let key = self.lower_annotation(scope, &param.type_annotation.type_annotation)?;
        // B29: an index-signature VALUE is a legal-recursion boundary (the canonical
        // `type Json = … | { [k: string]: Json }`), so it lowers one indirection deeper.
        // The key stays at surface depth (recursion through a key is never legal).
        let value = self
            .with_indirection(|p| p.lower_annotation(scope, &sig.type_annotation.type_annotation))?;
        let wk = self.interner.well_known();
        if key == wk.string {
            object.string_index = Some(value);
            Some(())
        } else if key == wk.number {
            object.number_index = Some(value);
            Some(())
        } else {
            // A symbol / template-literal / other index-key type is out of the M19
            // subset → abort the enclosing annotation.
            None
        }
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
    fn keyof_type(&mut self, operand: TypeId) -> TypeId {
        if let Some(keys) = super::eval::keyof_of_type(self.interner, operand) {
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
    fn indexed_access_type(&mut self, object: TypeId, index: TypeId) -> TypeId {
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

        // Any other key (a non-literal `string`, a type parameter, …) is out of the M20
        // scope → error type (no crash).
        wk.error
    }

    /// Lower a function type annotation to an interned function. Parameters stay
    /// positional; missing/unlowerable/optional/rest parameters abort rather than
    /// silently mis-stating the signature.
    fn lower_function_annotation(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        return_type: &TSType<'_>,
    ) -> Option<TypeId> {
        self.signature_params_in_subset(params)?;
        let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
        for param in &params.items {
            let name = parameter_name(&param.pattern)?;
            let annotation = param.type_annotation.as_ref()?;
            // B29: a parameter/return is a legal-recursion boundary (`type Fn = () => Fn`).
            let ty = self
                .with_indirection(|p| p.lower_annotation(scope, &annotation.type_annotation))?;
            lowered.push(ParameterType {
                name,
                ty,
                optional: false,
            });
        }
        let ret = self.with_indirection(|p| p.lower_annotation(scope, return_type))?;
        Some(self.interner.intern_function(FunctionType {
            params: lowered,
            ret,
        }))
    }

    fn lower_readonly_array_or_tuple(
        &mut self,
        scope: ScopeId,
        operand: &TSType<'_>,
    ) -> Option<TypeId> {
        match operand {
            TSType::TSArrayType(array) => {
                let element =
                    self.with_indirection(|p| p.lower_annotation(scope, &array.element_type))?;
                Some(self.intern_readonly_array(element))
            }
            TSType::TSTupleType(tuple) => {
                let mut elements = Vec::with_capacity(tuple.element_types.len());
                for element in &tuple.element_types {
                    let ts_type = element.as_ts_type()?;
                    elements.push(self.with_indirection(|p| p.lower_annotation(scope, ts_type))?);
                }
                Some(self.intern_readonly_tuple(elements))
            }
            _ => None,
        }
    }

    fn intern_readonly_array(&mut self, element: TypeId) -> TypeId {
        let array = self.interner.intern_array(element);
        self.interner.intern_readonly(array)
    }

    fn intern_readonly_tuple(&mut self, elements: Vec<TypeId>) -> TypeId {
        let tuple = self.interner.intern_tuple(elements);
        self.interner.intern_readonly(tuple)
    }
}

/// Map an oxc mapped-type modifier to the type-model [`ModifierOp`] (M26). No modifier
/// is `Keep` (preserve the source flag / default absent); `?`/`+?` and
/// `readonly`/`+readonly` (`True`/`Plus`) are `Add`; `-?`/`-readonly` (`Minus`) is
/// `Remove`.
fn modifier_op(op: Option<TSMappedTypeModifierOperator>) -> ModifierOp {
    match op {
        None => ModifierOp::Keep,
        Some(TSMappedTypeModifierOperator::True) | Some(TSMappedTypeModifierOperator::Plus) => {
            ModifierOp::Add
        }
        Some(TSMappedTypeModifierOperator::Minus) => ModifierOp::Remove,
    }
}

/// Map an `f64` literal value to a non-negative `usize` index, or `None` for a
/// fractional / negative / non-finite / out-of-`usize` value (M20 tuple indexed
/// access). The literal-type counterpart of [`literal_index`], which reads the
/// index off an AST expression.
fn whole_index(value: f64) -> Option<usize> {
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 || value > usize::MAX as f64 {
        return None;
    }
    Some(value as usize)
}
