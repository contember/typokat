//! annotations module (extracted from checker/mod.rs).

use super::calls::parameter_name;
use super::context::*;
use crate::binder::declaration::TypeGroupId;
use crate::binder::scope::ScopeId;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{
    ConditionalType, FunctionType, LiteralValue, MappedType, ModifierOp, ObjectType, ParameterType,
    PropertyKey, PropertyType, TemplateType, TupleRestType, TupleType, TypeTag, WellKnownSymbol,
};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    Expression, FormalParameters, TSCallSignatureDeclaration, TSConditionalType,
    TSConstructSignatureDeclaration, TSConstructorType, TSInferType, TSLiteral, TSMappedType,
    TSMappedTypeModifierOperator, TSMethodSignature, TSMethodSignatureKind, TSSignature,
    TSTemplateLiteralType, TSThisParameter, TSTupleElement, TSType, TSTypeAnnotation, TSTypeName,
    TSTypeOperatorOperator, TSTypeQueryExprName, UnaryOperator,
};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};

/// Host-recursion budget for [`Pass::lower_annotation`] (backlog 63k). Chosen far above
/// any realistic annotation nesting yet far below the stack-overflow threshold on both
/// the CLI (8 MiB) and cargo-test (2 MiB) thread stacks; the HEAD crash needed ~2.5k
/// levels on 8 MiB, so 200 keeps a wide margin in every context.
const MAX_ANNOTATION_DEPTH: u32 = 200;

mod composites;
mod declared;
mod functions;
mod signatures;
mod type_operators;

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
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
            self.emit_diagnostic(Diagnostic::excessively_deep(Span::from_oxc(ts_type.span())));
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
                if self.capture_compact_replay_dependencies {
                    let _ = self.compact_type_decl_id_replay(scope, "Array");
                }
                let element =
                    self.with_indirection(|p| p.lower_annotation(scope, &array.element_type))?;
                return Some(self.interner.intern_array(element));
            }
            // M18: a tuple type `[A, B]`. Each element is lowered recursively (so nested
            // tuples / arrays work) and the ordered list is interned. Named/optional/rest
            // tuple elements are out of the M18 subset and abort the whole annotation
            // (`None`) — see [`lower_tuple_annotation`].
            TSType::TSTupleType(tuple) => {
                if self.capture_compact_replay_dependencies {
                    let _ = self.compact_type_decl_id_replay(scope, "Array");
                }
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
                    Span::from_oxc(reference.span),
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
                    let object = self.lower_annotation(scope, &access.object_type);
                    if let Some(param_ty) = self.bare_type_param_reference(&access.object_type) {
                        if let Some(frame) = self.mapped_frames.last_mut() {
                            frame.captured_source.get_or_insert(param_ty);
                        }
                    }
                    return object.map(|_| self.interner.intern_mapped_value());
                }
                let object = self.lower_annotation(scope, &access.object_type);
                let index = self.lower_annotation(scope, &access.index_type);
                let (Some(object), Some(index)) = (object, index) else {
                    return None;
                };
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
                if query.type_arguments.is_none()
                    && matches!(
                        &query.expr_name,
                        TSTypeQueryExprName::IdentifierReference(identifier)
                            if identifier.name == "globalThis"
                    )
                {
                    return self.global_object_type;
                }
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
            TSType::TSSymbolKeyword(_) => return Some(self.interner.well_known().symbol),
            TSType::TSBigIntKeyword(_) => return Some(self.interner.well_known().bigint),
            TSType::TSObjectKeyword(_) => return Some(self.interner.well_known().object),
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

#[cfg(test)]
mod multi_child_recovery_tests {
    use crate::check::test_support::check_source;
    use crate::diagnostics::DiagnosticCode;

    fn starts(source: &str, needle: &str) -> Vec<u32> {
        source
            .match_indices(needle)
            .map(|(start, _)| u32::try_from(start).expect("test source span fits u32"))
            .collect()
    }

    #[test]
    fn test_support_without_library_does_not_certify_symbol_spelling() {
        let source = r#"
            declare const Symbol: { iterator: "test-support-local" };
            interface NoLibrarySymbolControl {
                [Symbol.iterator](): void;
            }
        "#;
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.incomplete.len(), 1, "{:?}", output.incomplete);
        assert_eq!(
            output.incomplete[0].id,
            "signature/method-signature/computed-key"
        );
        assert_eq!(
            output.incomplete[0].span.start,
            starts(source, "Symbol.iterator")[0]
        );
    }

    #[test]
    fn interleaved_overload_diagnostics_keep_global_source_order() {
        let source = r#"
            type Interleaved = {
                method(value: MissingOverload.First): void;
                middle: MissingOverload.Middle;
                method(value: MissingOverload.Last): void;
            };
        "#;
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.span.start))
                .collect::<Vec<_>>(),
            starts(source, "MissingOverload")
                .into_iter()
                .map(|start| (DiagnosticCode::TK2503, start))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn unavailable_children_do_not_hide_later_or_reversed_topology_errors() {
        let source = r#"
            namespace Available { export interface Good {} }
            enum DeferredEnum { A }
            type UnionForward = Available.Good | MissingForward.Bad;
            type UnionReverse = MissingReverse.Bad | Available.Good;
            type FunctionForward = (value: Available.Good) => MissingReturn.Bad;
            type FunctionReverse = (value: MissingParameter.Bad) => Available.Good;
            type EnumForward = DeferredEnum.A | MissingEnumForward.Bad;
            type EnumReverse = MissingEnumReverse.Bad | DeferredEnum.A;
        "#;
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.span.start))
                .collect::<Vec<_>>(),
            [
                "MissingForward",
                "MissingReverse",
                "MissingReturn",
                "MissingParameter",
                "MissingEnumForward",
                "MissingEnumReverse",
            ]
            .into_iter()
            .map(|name| (DiagnosticCode::TK2503, starts(source, name)[0]))
            .collect::<Vec<_>>()
        );
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != DiagnosticCode::TK2322),
            "unavailable parents must not publish a partial semantic type"
        );
    }

    #[test]
    fn unavailable_child_withholds_the_whole_annotation_callable() {
        let source = r#"
            enum DeferredCallable { Parameter }
            declare const unavailable: (value: DeferredCallable.Parameter) => string;
            const noPartialReturn: never = unavailable({});
        "#;
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert!(
            output.diagnostics.is_empty(),
            "a TK2322 from the call result would expose a partial callable: {:?}",
            output.diagnostics
        );
        assert_eq!(
            output
                .incomplete
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            [
                "decl/enum-declaration/self",
                "annotation-lower/type-name/qualified-enum",
            ]
        );
    }

    #[test]
    fn unavailable_overload_row_keeps_ready_compatibility_without_publication() {
        let source = r#"
            enum DeferredOverload { A }
            function mixed(value: DeferredOverload.A): number;
            function mixed(value: string): string;
            function mixed(value: number): number {
                const bodyStillChecked: string = 1;
                return value;
            }
            const noPartialOverload: never = mixed("ready");
        "#;
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [DiagnosticCode::TK2394, DiagnosticCode::TK2322],
            "ready compatibility and bodies run, but the unavailable group publishes no callable"
        );
    }

    #[test]
    fn wrong_arity_array_visits_every_argument_once_in_source_order() {
        let source = "type Probe = Array<MissingArray.First, AlsoMissingArray.Second>;";
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.span.start))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticCode::TK2503, starts(source, "MissingArray")[0]),
                (
                    DiagnosticCode::TK2503,
                    starts(source, "AlsoMissingArray")[0],
                ),
                (DiagnosticCode::TK2314, starts(source, "Array")[0]),
            ]
        );
    }
}
