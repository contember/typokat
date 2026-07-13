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
    PropertyType, TemplateType, TupleRestType, TupleType, TypeTag,
};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    Expression, FormalParameters, TSCallSignatureDeclaration, TSConditionalType,
    TSConstructSignatureDeclaration, TSConstructorType, TSInferType, TSLiteral, TSMappedType,
    TSMappedTypeModifierOperator, TSMethodSignature, TSMethodSignatureKind, TSSignature,
    TSTemplateLiteralType, TSThisParameter, TSTupleElement, TSType, TSTypeAnnotation, TSTypeName,
    TSTypeOperatorOperator, UnaryOperator,
};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};

/// Host-recursion budget for [`Pass::lower_annotation`] (backlog 63k). Chosen far above
/// any realistic annotation nesting yet far below the stack-overflow threshold on both
/// the CLI (8 MiB) and cargo-test (2 MiB) thread stacks; the HEAD crash needed ~2.5k
/// levels on 8 MiB, so 200 keeps a wide margin in every context.
const MAX_ANNOTATION_DEPTH: u32 = 200;

mod composites;
mod functions;
mod signatures;
mod type_operators;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower an annotation type to its `TypeId`. Type references resolve to stored
    /// declaration ids, never inlined structures, so recursive aliases/interfaces
    /// terminate by pointing at the reserved id (mvp-plan M5, §3, §6.3).
    pub(in crate::check::checker) fn lower_annotation(
        &mut self,
        scope: ScopeId,
        ts_type: &TSType<'_>,
    ) -> Option<TypeId> {
        // Graceful nesting budget (backlog 63k). Lowering is host-recursive, so an
        // absurdly deep type literal would overflow the stack; past the budget we report
        // TK2589 and stop descending. tsc itself RangeError-crashes around ~3k, so there
        // is no oracle — the contract is a bounded diagnostic, never a native crash and
        // never a silent permissive type. The limit sits far below the stack-overflow
        // threshold on both the CLI (8 MiB) and cargo-test (2 MiB) thread stacks.
        self.annotation_depth += 1;
        if self.annotation_depth > MAX_ANNOTATION_DEPTH {
            self.annotation_depth -= 1;
            self.diagnostics
                .push(Diagnostic::excessively_deep(Span::from_oxc(ts_type.span())));
            return Some(self.interner.well_known().error);
        }
        let result = self.lower_annotation_inner(scope, ts_type);
        self.annotation_depth -= 1;
        result
    }

    fn lower_annotation_inner(&mut self, scope: ScopeId, ts_type: &TSType<'_>) -> Option<TypeId> {
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
                return self.lower_generic_function_annotation(
                    scope,
                    func.type_parameters.as_deref(),
                    func.this_param.as_deref(),
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
                // `unique symbol` is the remaining operator (WU5 accounting).
                self.record_incomplete(
                    "annotation-lower/type-operator/unique-operand",
                    Span::from_oxc(op.span),
                    "unique symbol operator not lowered",
                );
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
            // Unmodeled `TSType` variants (WU5 accounting): record the skipped surface
            // before degrading to `None` (→ the error type) so an unsupported annotation
            // can no longer exit clean. Each id is the inventory identity for the variant.
            TSType::TSTypeQuery(query) => {
                self.record_incomplete(
                    "annotation-lower/type-query/typeof",
                    Span::from_oxc(query.span),
                    "typeof type query not lowered",
                );
                return None;
            }
            TSType::TSTypePredicate(predicate) => {
                self.record_incomplete(
                    "annotation-lower/type-predicate/self",
                    Span::from_oxc(predicate.span),
                    "type predicate not lowered",
                );
                return None;
            }
            TSType::TSThisType(this_ty) => {
                self.record_incomplete(
                    "annotation-lower/this-type/self",
                    Span::from_oxc(this_ty.span),
                    "this type annotation not modeled",
                );
                return None;
            }
            TSType::TSImportType(import_ty) => {
                self.record_incomplete(
                    "annotation-lower/import-type/self",
                    Span::from_oxc(import_ty.span),
                    "import type not modeled",
                );
                return None;
            }
            TSType::TSSymbolKeyword(kw) => {
                self.record_incomplete(
                    "annotation-lower/symbol-keyword/self",
                    Span::from_oxc(kw.span),
                    "symbol keyword type not modeled",
                );
                return None;
            }
            TSType::TSBigIntKeyword(kw) => {
                self.record_incomplete(
                    "annotation-lower/bigint-keyword/self",
                    Span::from_oxc(kw.span),
                    "bigint keyword type not modeled",
                );
                return None;
            }
            TSType::TSObjectKeyword(kw) => {
                self.record_incomplete(
                    "annotation-lower/object-keyword/self",
                    Span::from_oxc(kw.span),
                    "object keyword type not modeled",
                );
                return None;
            }
            TSType::TSIntrinsicKeyword(kw) => {
                self.record_incomplete(
                    "annotation-lower/intrinsic-keyword/self",
                    Span::from_oxc(kw.span),
                    "intrinsic keyword type not modeled",
                );
                return None;
            }
            // JSDoc types are design-OOS (no in-scope child to account); a bare named
            // tuple member reaching here is handled at its tuple site.
            _ => return None,
        };
        Some(id)
    }

    /// B29: run `f` one legal-recursion boundary deeper. Balanced even through
    /// early returns; re-entering a resolving alias at greater depth is recursion,
    /// not a surface cycle (see [`Pass::resolve_type_decl`]).
    pub(super) fn with_indirection<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.alias_indirection_depth += 1;
        let result = f(self);
        self.alias_indirection_depth -= 1;
        result
    }
}

/// Map an oxc mapped-type modifier to the type-model [`ModifierOp`] (M26). No modifier
/// is `Keep` (preserve the source flag / default absent); `?`/`+?` and
/// `readonly`/`+readonly` (`True`/`Plus`) are `Add`; `-?`/`-readonly` (`Minus`) is
/// `Remove`.
pub(super) fn modifier_op(op: Option<TSMappedTypeModifierOperator>) -> ModifierOp {
    match op {
        None => ModifierOp::Keep,
        Some(TSMappedTypeModifierOperator::True) | Some(TSMappedTypeModifierOperator::Plus) => {
            ModifierOp::Add
        }
        Some(TSMappedTypeModifierOperator::Minus) => ModifierOp::Remove,
    }
}
