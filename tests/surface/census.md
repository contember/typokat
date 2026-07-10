# Surface-accounting census (sprint 2026-07-10, WU0)

The bounded census WU0 owes the split gate: every dispatcher **role** and relevant **child
slot** across the checker, the `oxc 0.137.0` enum-variant counts they dispatch on, where the
silent drops live (`file:line`), and the split-gate verdict. Classification is per **role and
child slot**, not per `oxc` node variant — the same construct is covered differently by
different layers, which is the whole reason a variant-only inventory is insufficient.

Counts were read from the vendored `oxc_ast-0.137.0` source for counting only; nothing
committed here depends on a Cargo-registry path (the pinned version lives in `Cargo.toml` and
is mirrored in the manifest `[meta].oxc_version`).

## `oxc 0.137.0` enum-variant counts (the dispatch surface)

| Enum | Variants | Notes |
|---|---|---|
| `Expression` | 43 | 40 explicit + 3 inherited from `MemberExpression` (`@inherit`) |
| `MemberExpression` | 3 | Computed / Static / PrivateField member access |
| `Statement` | 33 | 18 explicit + 9 inherited `Declaration` + 6 inherited `ModuleDeclaration` |
| `Declaration` | 9 | var / function / class / type-alias / interface / enum / module / global / import-equals |
| `ModuleDeclaration` | 6 | import / export-all / export-default / export-named / export-assignment / namespace-export |
| `TSType` | 37 | keywords, references, composites, mapped/conditional/template, query/predicate/import, JSDoc |
| `TSSignature` | 5 | index / property / call / construct / method signatures |
| `ClassElement` | 5 | static-block / method / property / accessor / index-signature |
| `ObjectPropertyKind` | 2 | ObjectProperty / SpreadProperty |
| `TSTupleElement` | 2 (+ inherits `TSType`) | named-member / optional/rest markers over any `TSType` |
| `ForStatementInit` | 1 (+ inherits `Expression`) | VariableDeclaration or any expression |
| `ForStatementLeft` | 1 (+ inherits `AssignmentTarget`) | VariableDeclaration or an assignment target |
| `TSTypeName` | 3 | identifier / qualified-name (`A.B`) / this |
| `TSLiteral` | 6 | boolean / numeric / bigint / string / template / unary-minus |

An `oxc` upgrade that changes any of these counts must invalidate the manifest (the WU1
validator pins `[meta].oxc_version` against `Cargo.toml` and drives exhaustive Rust matches).

## Dispatcher roles and their silent-drop sites

The pipeline (`src/check/checker/mod.rs:60`, `:159`) runs five node-dispatching layers, each
with an independent fallback. A silent drop is a wildcard `_ => {}` / `_ => None` / a `continue`
past a child slot / a `None` degradation to the error type — none of which record a diagnostic.

### Role `bind` — `src/binder/bind.rs`

| Child-slot role | Dispatch fn | Silent drop |
|---|---|---|
| type-predeclaration (statement) | `bind_type_declaration_statement` | `bind.rs:250` `_ => {}` |
| type-predeclaration (in export) | `bind_type_declaration` | `bind.rs:270` `_ => {}` |
| statement | `bind_statement` | `bind.rs:364` `_ => {}` |
| declaration (in export) | `bind_declaration` | `bind.rs:381` `_ => {}` |
| class element | `bind_class` | `bind.rs:527` `_ => {}` (static-block / accessor / index-sig) |
| expression | `bind_expression` | `bind.rs:646` `_ => {}` — **narrower than `infer_expr`** |
| binding pattern | `binding_name` | `bind.rs:746` `_ => None` (destructuring patterns) |

### Role `flow` — `src/check/checker/flowgraph/`

| Child-slot role | Dispatch fn | Silent drop |
|---|---|---|
| statement | `build_flow_stmt` | `flowgraph/mod.rs:107` `_ => {}` — comment still claims `for`/`for-of`/`do` "not walked", but `stmt-check` DOES walk them (documented **cross-layer drift**) |
| declaration (in export) | `build_flow_declaration` | `flowgraph/mod.rs:122` `_ => {}` |
| expression | `build_flow_expr` | `flowgraph/exprs.rs:97`, `:245`, `:298`, `:351` |

### Role `stmt-check` — `src/check/checker/statements.rs`

| Child-slot role | Dispatch fn | Silent drop |
|---|---|---|
| statement | `check_stmt` | `statements.rs:145` `_ => {}` — drops `try`/`with`/`debugger` + all module decls |
| declaration (in export) | `check_declaration` | `statements.rs:162` `_ => {}` |

### Role `expr-infer` — `src/check/checker/expr.rs`

| Child-slot role | Dispatch fn | Silent drop |
|---|---|---|
| expression | `infer_expr` | `expr.rs:162` `_ => None` — no arm for template-literal, tagged-template, optional-chain, non-null, update, await, yield, satisfies, instantiation, import-expr |
| object-literal member kind | `infer_object_literal` | `expr.rs:263` (SpreadProperty skipped) |
| object-literal key | `infer_object_literal` | `expr.rs:265` (computed key skipped via `static_name`) |
| array-literal element | `infer_array_literal` | `expr.rs:290` (spread / elision skipped via `as_expression`) |
| member / element access base | `infer_member_access` / `infer_element_access` | non-object/array/tuple base → error type, no diagnostic |

### Role `annotation-lower` — `src/check/checker/annotations/`

| Child-slot role | Dispatch fn | Silent drop |
|---|---|---|
| type node | `lower_annotation_inner` | `annotations/mod.rs:174` `_ => return None` — no arm for `TSTypeQuery` (`typeof X`), `TSTypePredicate`, `TSThisType`, `TSImportType`, object/symbol/bigint keywords |
| type-operator | `lower_annotation_inner` | `annotations/mod.rs:136` `return None` (operators other than keyof / readonly) |
| composite member | `composites.rs` | `composites.rs:200` `_ => None` |

### Role `call` / `decl` / `class` / `signature`

| Role | Dispatch fn | Silent drop |
|---|---|---|
| call | argument collection | `calls.rs:736`, `:1086`, `:1307` `_ => None` (spread args, non-expression args) |
| decl (reserve/fill) | `decls/mod.rs` | `:63`, `:113` `continue`; `:285`, `:309`, `:468` `_ => {}`; `:499`/`:501` `_ => None` |
| class (member collection) | `classes/mod.rs` | `:561`, `:847`, `:931` `_ => {}`; computed keys skipped at `:365`, `:502`, `:553`, `:892`, `:920` |
| signature (interface members) | `decls/interface.rs` | `:67`, `:111` computed keys skipped via `static_name` |

## How outputs reach `CheckOutput`

`CheckOutput` (`src/driver.rs:22`) carries only `diagnostics` and `parse_errors`. Binder
outputs and flow metadata never surface: the binder feeds the checker's reserve/fill and
scope resolution, and the flow graph is consumed internally by narrowed reads — neither adds a
channel to `CheckOutput`. So an empty `(diagnostics, parse_errors)` pair is **indistinguishable
from a genuinely clean check**. That is the false-clean hole: every silent drop above lands in
this same empty pair. WU2 adds the third `IncompleteSurface` channel so a skipped in-scope
position is representable end to end.

## Official-suite behavior for unexpected exits (read-only confirmation)

`tooling/official-suite/tsofficial.py` accepts only exit `0`/`1` (`OK_EXIT_CODES`, `:180`); any
other code raises `HarnessFailure` (`:236`) rather than scoring a silent zero, and it also
rejects exit-0-with-output and exit-1-without-output as inconsistent (`:241`, `:246`). So a new
exit `3` is currently a hard failure — WU2 must teach the harness the incomplete outcome
without weakening this crash detection.

## Split-gate verdict

### (a) Newly ownerless in-scope semantic families: **more than eight**

Families that currently drop themselves or an in-scope child AND that lack a dedicated backlog
owner today (design-OOS JS-runtime/emit constructs excluded). Per the gate, these are kept
**classified and owned** but are **not implemented** in this sprint; each graduates to a backlog
owner before any WU3–WU5 implementation commit that touches it.

1. `stmt-check/try-statement/*` — try / catch / finally traversal (no owner).
2. `expr-infer/object-literal/spread-property` — `{ ...x }` (distinct from array spread).
3. `expr-infer/array-literal/elision` — array holes `[, x]`.
4. `annotation-lower/type-query/typeof` — `typeof X` type queries.
5. `expr-infer/tagged-template/self` — tagged template expressions.
6. `expr-infer/optional-chain/self` — `a?.b` (`ChainExpression`).
7. `expr-infer/non-null-assertion/self` — `x!` (`TSNonNullExpression`).
8. `expr-infer/update-expression/operand` — `x++` operand reference resolution.
9. `annotation-lower/type-predicate/self` — `x is T` predicate annotations.
10. `call/spread-argument/self` — spread call/`new` arguments.

Families already owned (classified, not new): template interpolation + array spread + iteration
→ `71`; computed/symbol member keys, decorators, delete, reachability, indexed-access compat →
`75`; enums → `42`; namespaces / module declarations → `43`; `this` type/params → `70`; module
breadth → `15`/`29`. The **for/do flow-comment drift** (flow claims "not walked" while
`stmt-check` walks them) is an accounting bug owned by `73`, not a semantic family.

Consequence of >8: this is **volume, which the gate says does NOT trigger the architecture
split** ("Difficulty or volume alone does not trigger the split"). It only mandates classify +
own without implementing — which is already WU3–WU5's declared discipline (traverse or mark
incomplete; do not claim the feature complete).

### (b) Does honest accounting require a second traversal architecture or a new cross-layer ownership boundary? **NO (no architecture stop).**

The five existing layers already walk the whole AST. Honest accounting is **additive to the
existing walkers**, not a new traversal: (1) at each existing wildcard, look up the node's
disposition in the surface manifest and record an `incomplete` identity instead of dropping
silently; (2) for a `supported` wrapper, extend the *existing* walker to descend into each
declared child slot (template hole, computed key, spread element) — or record `incomplete` for
that slot. Both reuse the existing `infer_expr` / statement walkers and the single CFG; no
generic second checker and no second flow model is introduced (and the sprint forbids one). The
one genuine cross-layer subtlety — the same node covered differently across `bind` / `flow` /
`stmt-check` — is resolved by the manifest's `role` field assigning an owning layer per
identity, which is a **data classification**, not a new traversal architecture. Therefore the
sprint may proceed through WU3–WU5 as accounting-only wiring; it does **not** hit the
architecture-stop that would confine it to WU1 / WU2 / WU6.
