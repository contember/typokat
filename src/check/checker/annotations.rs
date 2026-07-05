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
    FormalParameters, TSCallSignatureDeclaration, TSConditionalType, TSConstructSignatureDeclaration,
    TSConstructorType, TSInferType, TSLiteral, TSMappedType, TSMappedTypeModifierOperator,
    TSMethodSignature, TSMethodSignatureKind, TSSignature, TSTemplateLiteralType, TSTupleElement,
    TSType, TSTypeAnnotation, TSTypeName, TSTypeOperatorOperator,
};
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower an annotation type to its `TypeId`. Supports intrinsic keywords, object
    /// type literals, function types (`(x: number) => string`), unions, and (M5) type
    /// **references** (`Point`, `Num`, `List`) resolved through the binder's type slot.
    ///
    /// Reference resolution returns a **stored id** for the referenced declaration
    /// (the interface's reserved nominal id, or the alias's resolved target) — never
    /// an inlined copy of the referenced structure. That is what makes lowering a
    /// recursive type terminate: `tail: List | null` stores the union of `List`'s id
    /// and `null`, not an expansion of `List` (mvp-plan M5, §3, §6.3).
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
            TSType::TSParenthesizedType(paren) => {
                return self.lower_annotation(scope, &paren.type_annotation);
            }
            // M17: an array type `T[]`. The element is lowered recursively (so `T[][]`
            // nests), then interned as an array type. A non-lowerable element (out of
            // subset) aborts the whole annotation (`None`), matching the other lowerings.
            TSType::TSArrayType(array) => {
                let element = self.lower_annotation(scope, &array.element_type)?;
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
            // M20: `keyof T`. Only the `keyof` operator is in scope (`unique`/`readonly`
            // operators are not); it is computed eagerly on a concrete object type
            // ([`keyof_type`]). A generic/deferred `keyof` (over a type parameter) is a
            // VM-phase feature → error type (no crash).
            TSType::TSTypeOperatorType(op) => {
                if op.operator != TSTypeOperatorOperator::Keyof {
                    return None;
                }
                let operand = self.lower_annotation(scope, &op.type_annotation)?;
                return Some(self.keyof_type(operand));
            }
            // M20: an indexed-access type `T[K]`. Both sides are lowered, then the
            // member type(s) named by `K` are looked up on `T` eagerly
            // ([`indexed_access_type`]). Out-of-scope combinations (a non-object `T`, a
            // missing key, a generic key) yield the error type (no crash).
            //
            // M26: inside a mapped type's value template, `X[K]` whose index names the
            // innermost mapped key binder is the source property value — it lowers to the
            // node-scoped [`TypeTag::MappedValue`] placeholder (resolved per key at
            // evaluation), NOT the eager M20 lookup (which would resolve to the error
            // type against the still-abstract source).
            TSType::TSIndexedAccessType(access) => {
                if self.index_is_active_mapped_key(&access.index_type) {
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

    /// Lower an `infer U` binder to its node-scoped de Bruijn [`TypeTag::Infer`] index
    /// (M25), declared into the **innermost active** frame. A new name takes the next
    /// index (`binders.len()`); a repeated name reuses its index (so
    /// `{ a: infer U; b: infer U }` binds one index). Outside any conditional's
    /// `extends`/true position (no active frame) it is out of subset → `None`.
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
    /// `tests/cases/README.md`).
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

        // TK2456 (review): a key source that surface-references the alias currently
        // being resolved (`type M = { [K in keyof M]: number }`) is a circular alias —
        // the re-entry error type would otherwise silently feed the map. The alias
        // degrades to the error type (silent downstream, M22 discipline). tsc also adds
        // a TS2313 for K's circular constraint — secondary code omitted (documented).
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
        });
        let value_template = match &mapped.type_annotation {
            Some(annotation) => self.lower_annotation(scope, annotation),
            // A mapped type with no value (`{ [K in S] }`) is out of subset.
            None => None,
        };
        self.mapped_frames.pop();
        let value_template = value_template?;

        let id = self.interner.intern_mapped(MappedType {
            homomorphic,
            key_source,
            value_template,
            optional_modifier: modifier_op(mapped.optional),
            readonly_modifier: modifier_op(mapped.readonly),
        });
        let span = Span::from_oxc(mapped.span);
        Some(self.maybe_evaluate(id, span))
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

    /// Lower a union type annotation `A | B | …` to a canonical interned `TypeId`
    /// (M4). Each member is lowered recursively, then `Interner::union` flattens,
    /// sorts, dedups, drops `never`, and collapses degenerate unions (mvp-plan
    /// §3.3). A member whose type cannot be lowered (out of subset) aborts the whole
    /// annotation (`None`), matching the object/function lowering — dropping a member
    /// silently would mis-state the union.
    fn lower_union_annotation(&mut self, scope: ScopeId, members: &[TSType<'_>]) -> Option<TypeId> {
        let mut lowered: Vec<TypeId> = Vec::with_capacity(members.len());
        for member in members {
            lowered.push(self.lower_annotation(scope, member)?);
        }
        Some(self.interner.union(lowered))
    }

    /// Lower a tuple type annotation `[A, B, …]` to a canonical interned `TypeId`
    /// (M18). Each element is lowered recursively (preserving order — `intern_tuple`
    /// never sorts), so nested tuples (`[[number], string]`) and array elements
    /// (`[number[], string]`) work. The **empty** tuple `[]` lowers to the interned
    /// empty tuple.
    ///
    /// Out of the M18 subset, aborting the whole annotation (`None`, matching the
    /// object/union/array lowerings — silently dropping or mis-shaping an element would
    /// mis-state the tuple):
    ///
    ///  - **optional** (`[number?]`) and **rest** (`[number, ...string[]]`) elements:
    ///    [`TSTupleElement::as_ts_type`] returns `None` for these, so the lowering
    ///    aborts;
    ///  - **named** tuple members (`[x: number, y: string]`): the element *is* a
    ///    `TSType::TSNamedTupleMember`, which `lower_annotation` does not handle → `None`.
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
            lowered.push(self.lower_annotation(scope, ts_type)?);
        }
        Some(self.interner.intern_tuple(lowered))
    }

    /// Lower a **literal type** (`TSLiteralType`'s literal) to its interned literal
    /// `TypeId` (M8). A string/number/boolean literal interns to the hash-consed
    /// literal id (so it shares identity with the same literal anywhere, which is what
    /// makes literal↔literal assignability and discriminant matching reduce to id
    /// equality). A `bigint`/template/`-1`-style (unary) literal type is out of the M8
    /// subset → `None` (the caller aborts the enclosing annotation, matching the other
    /// lowerings — silently dropping it would mis-state the type).
    fn lower_literal_type(&mut self, literal: &TSLiteral<'_>) -> Option<TypeId> {
        let value = match literal {
            TSLiteral::StringLiteral(s) => LiteralValue::String(s.value.to_string()),
            TSLiteral::NumericLiteral(n) => LiteralValue::Number(n.value),
            TSLiteral::BooleanLiteral(b) => LiteralValue::Boolean(b.value),
            // `bigint`, template-literal types, and unary (`-1`) literal types are out
            // of the M8 subset.
            TSLiteral::BigIntLiteral(_)
            | TSLiteral::TemplateLiteral(_)
            | TSLiteral::UnaryExpression(_) => return None,
        };
        Some(self.interner.intern_literal(value))
    }

    /// Lower an object type literal's members to an interned (structural) object
    /// `TypeId`. Object **type literals** stay structurally hash-consed (only nominal
    /// interfaces bypass interning).
    ///
    /// M19: a **string** index signature (`[k: string]: T`) sets `string_index = T`
    /// and a **number** index signature (`[i: number]: T`) sets `number_index = T`;
    /// both coexist with named properties.
    ///
    /// M21: an **optional** property (`b?: T`) is a real member with `optional: true`
    /// and an effective type of `T | undefined` (the `| undefined` is interned here).
    ///
    /// F1/WU1: a non-generic static-name method signature lowers to a
    /// function-typed property (`f(x: number): string` → `f: (x: number) => string`).
    ///
    /// A member whose type cannot be lowered, or any unsupported signature
    /// (call/construct/generic method/accessor method, or an index sig with an
    /// unsupported key) aborts the lowering (`None`).
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
                    let ty = self.lower_annotation(scope, &annotation.type_annotation)?;
                    // M21: an optional member (`b?: T`) lowers like a normal property whose
                    // effective type is `T | undefined` (see [`lower_interface_members`] for
                    // the rationale — the `| undefined` must be interned here, not in the
                    // relation engine). Previously any optional member aborted the whole
                    // annotation (degrading it to the error type); now it is a real member.
                    let ty = if sig.optional {
                        let undefined = self.interner.well_known().undefined;
                        self.interner.union(vec![ty, undefined])
                    } else {
                        ty
                    };
                    let mut prop = PropertyType::public(name.into_owned(), ty);
                    prop.optional = sig.optional;
                    // F5/backlog-03: carry the `readonly` modifier onto object-type-literal
                    // members (`{ readonly k: T }`). Previously dropped, so assigning to such a
                    // member was silently allowed. Part of the property's structural identity
                    // (hashed by the interner) but ignored by the relation engine for
                    // assignability; it gates the assignment target only (`TK2540`).
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

    /// Lower a `TSMethodSignature` member to a public function-typed property.
    ///
    /// This intentionally accepts only the WU1 subset: required, non-generic,
    /// non-`this`, non-accessor method signatures with static names. Optional
    /// method signatures are out of subset for WU1 and are dropped rather than
    /// represented incompletely.
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

    /// Lower a single object/interface call signature to an interned function type.
    ///
    /// WU2 accepts only a required, non-generic, non-`this`, non-rest signature.
    /// Optional/rest/default params and generic/`this` call signatures are out of
    /// subset and do not create represented callability.
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

    /// Lower a single object/interface construct signature to an interned function type.
    ///
    /// WU3 accepts only a required, non-generic, non-rest signature whose declared
    /// instance/return type is representable. Optional/rest/default params and generic
    /// construct signatures are out of subset and do not create represented
    /// constructability.
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
            let ty = self.lower_annotation(scope, &annotation.type_annotation)?;
            lowered.push(ParameterType {
                name,
                ty,
                optional: false,
            });
        }
        let ret = self.lower_annotation(scope, &return_type.type_annotation)?;
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

    /// Lower an object/interface method or call signature only when every parameter
    /// and the declared return type are faithfully representable in the current
    /// subset. Unlike class/free-function lowering, this never substitutes the error
    /// type for a bad parameter, because that would make an unsupported signature
    /// participate in calls/relations as if it were real.
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

    /// Lower a function-like signature into an interned [`FunctionType`].
    ///
    /// Parameters come from annotations; missing/unlowerable parameter annotations use
    /// the error type to suppress cascades. A missing return annotation becomes `void`,
    /// matching the class-method lowering this helper replaces.
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

    /// Lower a single index signature (M19) into the appropriate slot of `object`.
    /// `[k: string]: T` sets `string_index`; `[i: number]: T` sets `number_index`. The
    /// **value** type `T` is lowered; the key type itself is fixed (`string`/`number`)
    /// and not stored. Returns `None` — aborting the enclosing annotation, matching the
    /// other lowerings — for anything out of the M19 subset: a non-`string`/`number`
    /// key (symbol/template-literal index sigs are deferred), a value type that cannot
    /// be lowered, or a malformed signature (no/multiple key parameters). `readonly`
    /// index signatures are deferred — the `readonly` flag is ignored here.
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
        let value = self.lower_annotation(scope, &sig.type_annotation.type_annotation)?;
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

    /// Compute `keyof T` **eagerly** (M20). `T` is the already-lowered operand type.
    ///
    /// For a concrete **object** type the result is the `union(...)` of its property
    /// **names** as **string-literal** types (`keyof { x; y }` → `"x" | "y"`), plus
    /// `string` if the object has a string index signature and `number` if it has a
    /// number index signature (`keyof { [k: string]: V }` → `string`). An object with
    /// no members and no index signature yields `never` (an empty union collapses to
    /// `never` via `Interner::union`).
    ///
    /// Anything else — a primitive, a union/intersection, a type parameter, an array,
    /// a tuple — is **out of the M20 scope** (generic `keyof` is a type-level-VM
    /// feature) and yields the **error type**: no crash, and the error type suppresses
    /// any cascade where the result is used. The error/`any` operand also yields the
    /// error type (it carries no enumerable keys here).
    fn keyof_type(&mut self, operand: TypeId) -> TypeId {
        let wk = self.interner.well_known();
        let store = self.interner.store();
        let Some(object) = store.object_type(operand) else {
            // Non-object (primitive / union / type parameter / array / tuple) → out of
            // M20 scope. No crash; the error type suppresses cascade.
            return wk.error;
        };

        // Snapshot the key components before the mutable interning borrow: property
        // names become string-literal types, and each index signature contributes its
        // fixed key intrinsic (`string` / `number`).
        let names: Vec<String> = object.properties.iter().map(|p| p.name.clone()).collect();
        let has_string_index = object.string_index.is_some();
        let has_number_index = object.number_index.is_some();

        let mut members: Vec<TypeId> = Vec::with_capacity(names.len() + 2);
        for name in names {
            members.push(self.interner.intern_literal(LiteralValue::String(name)));
        }
        if has_string_index {
            members.push(wk.string);
        }
        if has_number_index {
            members.push(wk.number);
        }
        // `union(...)` canonicalizes/dedups (so `keyof {}` → `never`, and a repeated
        // key collapses).
        self.interner.union(members)
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
            if let Some(value) = store.object_type(object).and_then(|o| o.number_index) {
                return value;
            }
            return wk.error;
        }

        // The bare `number` intrinsic key → the number index value type (or error).
        if index == wk.number {
            if let Some(value) = store.object_type(object).and_then(|o| o.number_index) {
                return value;
            }
            return wk.error;
        }

        // Any other key (a non-literal `string`, a type parameter, …) is out of the M20
        // scope → error type (no crash).
        wk.error
    }

    /// Lower a function type annotation's parameters and return type to an interned
    /// function `TypeId`. Parameters are kept **positional** (never sorted). A
    /// parameter without a type annotation, or one whose type cannot be lowered, or
    /// any optional/rest parameter, aborts the lowering (`None`) — these are out of
    /// the M3 subset and dropping them silently would mis-state the type.
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
            let ty = self.lower_annotation(scope, &annotation.type_annotation)?;
            lowered.push(ParameterType {
                name,
                ty,
                optional: false,
            });
        }
        let ret = self.lower_annotation(scope, return_type)?;
        Some(self.interner.intern_function(FunctionType {
            params: lowered,
            ret,
        }))
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
