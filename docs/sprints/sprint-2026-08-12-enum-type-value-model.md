# Sprint — enum type/value model (2026-08-12)

**Goal.** Model TypeScript enums as one nominal member family with coherent type and value sides,
publish that family through the shipped namespace and Bundler module routes, and re-screen the
immutable `placetext` project to establish the next general blocker.

**Theme.** Backlog [`42`](../backlog/42-enums-type-side.md) is the first remaining general blocker
in the immutable `placetext` screen and one of three open Track-A model-completeness items. This
sprint closes the enum model itself. It does not special-case `Corpus`, weaken backlog `72`'s
zero-clean gate, or implement JavaScript emit behavior.

## Refs re-verified at HEAD (2026-08-12)

Planning HEAD is `33c7e5e4ae8e9e5e91bb3080f4b90a4fb792e5c1`.
`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ The binder already reserves one declaration and both value/type candidates for a direct enum;
  the missing work starts after symbol-space allocation —
  `crates/typokat-binder/src/binder/declaration.rs:620-631`.
- ⚠ The type arena has no enum or enum-member tag, and published type groups expose only
  `Template(TypeId)` or `Class(ClassId)`. A checker-only object-literal patch would collapse
  nominal member identity and make cross-enum assignments unsound —
  `crates/typokat-types/src/types/repr.rs:11-130`,
  `crates/typokat-check/src/check/checker/type_groups.rs:15-29`.
- ✔ Both ordinary and exported enum declarations currently stop at the same explicit
  `decl/enum-declaration/self` record —
  `crates/typokat-check/src/check/checker/statements.rs:786-797`,
  `crates/typokat-check/src/check/checker/statements.rs:958-966`.
- ✔ Qualified enum endpoints are deliberately deferred before annotation lowering, both at a root
  merge group and inside a namespace —
  `crates/typokat-binder/src/binder/namespace.rs:4289-4304`,
  `crates/typokat-binder/src/binder/namespace.rs:4350-4359`.
- ✔ Every composition containing an enum is currently classified
  `DeferredBacklog42`, and standalone namespace values reject an exported enum payload —
  `crates/typokat-binder/src/binder/namespace.rs:4640-4649`,
  `crates/typokat-check/src/check/checker/namespace_values.rs:1231-1244`.
- ⚠ The module frontend already proves a bare named enum declaration has no namespace slot, while
  identifier-default classification conservatively treats an enum as namespace-bearing or unknown.
  The sprint must preserve that fail-closed distinction for actual enum+namespace merges rather
  than globally relabel enums as namespace-free —
  `crates/typokat-frontend/src/frontend.rs:4137-4142`,
  `crates/typokat-frontend/src/frontend.rs:4158-4167`.
- ✔ Surface accounting has three backlog-42 records to graduate:
  `type-fill/enum-declaration/self`, `annotation-lower/type-name/qualified-enum`, and
  `decl/enum-declaration/self` — `tests/surface/inventory.toml:97-105`,
  `tests/surface/inventory.toml:1196-1204`, `tests/surface/inventory.toml:1324-1332`.
- ✔ The official-suite enum syntax gate currently hides 34 files. It is one regex gate, so removal
  requires an audited disposition of every revealed file, not only a headline score increase —
  `tooling/official-suite/tsofficial.py:153-160`, `tooling/official-suite/scoreboard.txt`.
- ✔ The existing source-re-export contract already freezes a cross-file enum failure in both root
  orders (`TK2305`, `TK2304`, and the declaration incomplete). It is the ready project witness,
  not a new route to invent — `tests/cases/b15_acyclic_source_reexports/README.md:18-26`.
- ✔ The 2026-08-11 immutable screen is 9 roots, 13 successful resolutions, 7 incomplete records,
  and 5 diagnostics. Exactly one incomplete plus one `TK2304` and two `TK2305` identities belong to
  `Corpus`; the five computed-key records, one template-interpolation record, and two predicate-array
  `TK2339` records have other owners —
  `docs/archive/real-project-rescreen-2026-08-11/README.md:41-60`.
- ✔ A fresh pinned `tsc 6.0.3 --strict --noEmit` probe confirms the load-bearing nominal rules:
  a known numeric literal and a broad `number` are assignable to their numeric enum; an unknown
  numeric literal is not; raw string literals are not assignable to string enum members; and members
  from distinct numeric enums are not mutually assignable. Two enum declarations and enum+namespace
  merge; enum+function reports `TS2567` in either order. WU0 expands this probe into the committed
  acceptance matrix before implementation.

## Work units

### WU0 — commit the disabled RED enum corpus (effort M)

- **Problem.** Existing fixtures prove only that enum surfaces are unavailable. They do not specify
  the identity, initializer, relation, merge, recovery, flow, or project semantics the replacement
  must preserve.
- **Verify first.** Cross-check every row with the installed pinned
  `tsc 6.0.3 --strict --noEmit --pretty false`. Record exact pre-change typokat diagnostics and
  incomplete identities. Where TypeScript behavior differs by enum kind, keep separate numeric,
  string, heterogeneous, ambient, and `const enum` fixtures rather than infer a shared rule.
- **Scope.** Add disabled `tests/cases/b42_enum_type_value_model/` fixtures for:
  declaration/type/value/member use; identifier, quoted, and statically computed member names;
  implicit numbering, negative values, string values, prior-member references, constant
  expressions, opaque computed numeric members, and invalid auto-increment/initializer rows;
  whole-enum and member assignability, widening, unions, intersections, generics, arrays/objects,
  distinct enums, and the exact numeric-versus-string asymmetry; equality and `switch` narrowing;
  `declare enum` and checker-only `const enum`; repeated enum declarations; legal enum+namespace
  orders; illegal enum+function and enum+function+namespace orders with exact `TK2567`/`TK2434`
  ownership and typed recovery. Add project fixtures for direct named exports/imports, type-only
  imports, source re-exports, namespace payloads, and supported default-slot forms in both root
  orders. Extend the existing `b15` enum rows rather than duplicate their route contract.
- **Acceptance / witness.** The commit changes no enabled checker behavior. A forced isolated run is
  RED on every intended acceptance family and already green on invalid controls. The tsc matrix
  records exact codes/lines and proves the raw-number, raw-string, cross-enum, merge-order, and
  recovery boundaries. Any syntax that tsc accepts but the proposed model cannot represent stops
  the sprint before WU1; it is not silently converted to `any` or a primitive literal.
- **Touch points.** `tests/cases/b42_enum_type_value_model/`, `tests/conformance.rs`,
  `tests/cases/README.md`, existing `tests/cases/b15_*` contracts, and focused tsc probe evidence
  beside the corpus if the fixture comments cannot preserve the full matrix clearly.

### WU1 — add nominal enum-member identity to the type core (effort L)

- **Problem.** Reusing ordinary number/string literal types would accept raw string literals and
  cross-enum members that tsc rejects. Reusing object identity would not give annotations and flow
  one canonical enum-member type.
- **Verify first.** Unit-test the exact WU0 relation matrix directly against interned types before
  connecting any AST path. Include relation-cache order permutations so a cached primitive/enum
  result cannot erase the enum identity.
- **Scope.** Add a stable `EnumId`, an identity-bearing whole-enum type, and an enum-member type keyed
  by enum identity and member identity, carrying only the proved primitive/literal facts needed by
  relation, rendering, and narrowing. The whole-enum node records its canonical member family plus
  the numeric openness proved by WU0; a plain member union is insufficient for empty enums and
  opaque/ambient numeric members. Equal-valued members of one enum retain distinct declaration
  identities but are relation-compatible. Keep ordinary `LiteralValue`
  unchanged. Extend store/interner hashing and equality, substitution, evaluator/free-parameter
  walkers, type rendering, relation dispatch, apparent/base-type demand, and serialization-free
  frozen-layer validation as required by the new tag. Preserve `TypeId` hash-consing and the
  three-`u32` relation cache key.
- **Acceptance / witness.** Same enum member identity interns once; distinct enums and distinct
  members stay distinct; whole-enum member families are deterministic; numeric and string assignment rules
  match WU0 in both source/target directions; rendering distinguishes `E` and `E.Member`; no
  exhaustive type walker falls through to error/recovery. Existing literal, union, substitution,
  evaluator, and cache-soundness tests remain byte-identical.
- **Touch points.** `crates/typokat-types/src/types/` (repr/store/interner/hash/substitution),
  `crates/typokat-relate/src/relate/`, `crates/typokat-diagnostics/src/diagnostics/render_type.rs`,
  and exhaustive type walkers selected by compiler errors/tests. No binder/checker AST behavior in
  this WU.

### WU1B — freeze canonical enum groups and merge legality (effort M)

- **Problem.** Descriptor construction needs one canonical declaration group and a settled legality
  result before it can allocate identity. Leaving that decision to the later namespace integration
  would create a cycle: publication would depend on merge legality while merge recovery would depend
  on publication.
- **Verify first.** Turn WU0's repeated-enum, enum+namespace, enum+function, and three-way order
  matrix into binder-only table tests. Assert the canonical group key, ordered enum participants,
  attached namespace identity, classification, placement issues, and exact diagnostic-owner sites
  without constructing any types.
- **Scope.** Expose one binder-owned canonical enum-group view keyed by the existing `TypeGroupId` and
  containing the ordered enum declaration participants plus any admitted namespace composition.
  Replace the blanket `DeferredBacklog42` classification with the exact WU0 legality matrix and
  preserve independent placement issues. This seam states what may be built; it does not allocate
  `EnumId`, lower members, publish values, or emit checker diagnostics.
- **Acceptance / witness.** Named binder tests cover every WU0 merge-order row. Root/module insertion
  permutations that preserve source order produce the same normalized group; changing source order
  changes only the ordered participant list and the tsc-prescribed owner sites. Illegal
  enum+function groups never appear buildable, while legal repeated-enum and enum+namespace groups
  expose one canonical build input.
- **Touch points.** `crates/typokat-binder/src/binder/namespace.rs` and binder tests only.

### WU2 — build and publish one enum descriptor (effort L)

- **Problem.** The binder has value/type slots, but the checker publishes neither a canonical enum
  type endpoint nor the object read by `E.Member`. Initializers also need one ordered analysis; using
  unrelated type- and value-side builders would let the two surfaces disagree.
- **Verify first.** Construct one descriptor in a checker unit test from numeric, string, mixed,
  ambient, and merged enum declarations. Assert that its whole type, member types, value-object
  properties, declaration order, and diagnostic owners agree before exposing it to lookup.
- **Scope.** Consume WU1B's frozen canonical group and reserve one enum identity per legal group.
  Analyze members in source
  order across merged declarations. Fold the WU0-approved constant forms; type-check every computed
  initializer exactly once; retain an opaque nominal numeric member where tsc does; and emit the
  exact initializer/auto-increment diagnostics without publishing a permissive partial member.
  Publish one enum descriptor through a new enum type-group surface: whole type, ordered static
  member map, and value-object type with readonly member properties. Publish the value object to the
  existing `ValueStorageId` and the enum endpoint to the existing `TypeGroupId` atomically. Treat
  `declare` and `const enum` as checker-visible type/value declarations; no emit decision enters the
  semantic identity.
- **Acceptance / witness.** Named checker tests instrument reservation/fill/publication phases:
  every descriptor has one reservation, every initializer span has one typed walk, and no lookup can
  observe the descriptor before its single terminal publication. `E`, `E.Member`, and `E.Member` as
  a value all refer to the same member identity; forward annotations and project dependency ordering
  do not observe a half-filled descriptor; invalid members keep independent child diagnostics and
  suppress only their own cascade. Query-order and project-root-order permutations preserve type ids
  and diagnostics; source order remains semantic and is not permuted. The three enum incomplete
  records disappear only when the corresponding surface is genuinely published.
- **Touch points.** `crates/typokat-check/src/check/checker/decls/`,
  `crates/typokat-check/src/check/checker/type_groups.rs`,
  `crates/typokat-check/src/check/checker/statements.rs`, checker context/replay ownership, and
  diagnostics for tsc-compatible enum declaration errors. The frozen binder seam changes only via a
  leader-owned replan.

### WU3 — consume enum members in annotations, expressions, and flow (effort M)

- **Problem.** Publication alone leaves qualified type names deferred, value member access routed
  through generic object lookup, and `switch` case labels limited to primitive literal AST nodes.
- **Verify first.** Trace one string member and one opaque numeric member through annotation
  resolution, expression inference, equality narrowing, `switch` case/default flow, assignment, and
  diagnostic rendering. Confirm no path re-infers an initializer or enum member expression for
  effects.
- **Scope.** Resolve bare `E` to the whole enum and bare `E.Member` to the exact member endpoint.
  Resolve static and statically keyed computed
  value access through the descriptor while preserving ordinary missing-member diagnostics. Extend
  equality and `switch` label extraction to exact enum member expressions and overlapping raw
  primitive values with a side-effect-free lookup; assignment compatibility is deliberately not
  reused because raw strings compare and narrow but do not assign to string enums. Reuse the existing
  union/discriminant narrowing machinery. Wire numeric-enum operand
  classification only where the already implemented operator checks ask whether an operand is
  number-like; general operator semantics remain backlog `45`.
- **Acceptance / witness.** Enable the single-file portion of WU0. Clean enum uses lose all three
  backlog-42 incompletes; invalid raw/cross-enum assignments retain exact `TK2322`; missing members
  retain `TK2339`; enum case branches narrow to the member and `default` to the complement without
  leaking after the switch; child expressions and diagnostics occur once.
- **Touch points.** `crates/typokat-check/src/check/checker/annotations/`,
  `crates/typokat-check/src/check/checker/decls/resolve.rs`,
  `crates/typokat-check/src/check/checker/expr.rs`,
  `crates/typokat-check/src/check/checker/narrowing.rs`,
  `crates/typokat-check/src/check/checker/flowgraph/`, and focused diagnostics.

### WU4 — integrate namespaces and Bundler slot publication (effort L)

- **Problem.** Every enum composition is currently deferred, namespace value assembly rejects enum
  payloads, and the project route therefore cannot carry an enum's paired value/type slots through
  imports, source re-exports, or default-slot classification.
- **Verify first.** Run WU0's complete merge-order and project root-order matrices against the
  published descriptor. Compare exact slot availability and diagnostic identities, not only whether
  a file exits clean.
- **Scope.** Consume WU1B's frozen legality: repeated enums and legal enum+namespace groups share one
  descriptor; enum+function combinations emit `TK2567` at the tsc sites; namespace placement
  diagnostics and typed recovery remain independent. Resolve enum members inside legal namespace
  compositions through the descriptor. Allow enum value
  dependencies in standalone namespace assembly without synthesizing a namespace for bare enums.
  Publish direct named enum exports/imports and acyclic named source re-exports as paired slots,
  with type-only syntax erasing only the value slot. Admit only enum default forms whose namespace
  absence the existing frontend/checker evidence proves; actual enum+namespace defaults remain
  fail-closed under backlog `15`'s current default/namespace boundary. Preserve project dependency
  order, source identity, and byte-identical summaries across root order.
- **Acceptance / witness.** Enable the namespace/project/merge portion of WU0 and update the
  existing `b15` enum contracts. No legal enum path reports `TK2304`, `TK2305`, or an enum
  incomplete. Illegal groups retain exact `TK2567`/`TK2434` plus the usable recovery surface tsc
  proves. Named, type-only,
  source-re-export, namespace-payload, and admitted default cases publish exactly the intended slots;
  no unsupported namespace/default bridge becomes clean.
- **Touch points.** `crates/typokat-binder/src/binder/namespace.rs`,
  `crates/typokat-check/src/check/checker/namespace_values.rs`, project export-slot code in
  `crates/typokat-check/src/check/checker/mod.rs`,
  `crates/typokat-frontend/src/frontend.rs`, and existing `b15` project contracts.

### WU5 — independent adversarial review and semantic gates (effort M)

- **Problem.** Enum identity crosses the arena, relation cache, declaration publication, namespace
  recovery, flow, and project slots. A green happy-path corpus can still hide a cross-enum false
  negative or a partially published merge.
- **Verify first.** A subagent that did not implement WU1-WU4 reviews the full diff and independently
  reproduces the tsc matrix. The reviewer starts from the false-negative question: “Which value can
  now enter an enum/member target that tsc rejects?”
- **Scope.** Hunt raw-string admission, arbitrary numeric-literal admission, cross-enum compatibility,
  mixed/opaque member base mistakes, cache-order dependence, duplicate initializer walks, invalid
  merge recovery, enum+namespace partial publication, incorrect type-only/default slots, source-order
  identity drift, frozen-base/replay leakage, and missing enum members hidden as `any`/error. Audit
  every exhaustive `TypeTag` consumer. Remove the official `syntax:enum` gate only after triaging all
  34 revealed files; retain independent OOS/incomplete owners where another backlog genuinely owns
  the remaining syntax or semantic family. Update the surface inventory, completion manifest,
  architecture/current limitations, and divergence ledger to the shipped truth. Compare WU0's
  committed fixture-family checklist against enabled cases so no planned family disappears between
  specification and closure.
- **Acceptance / witness.** Independent review returns no HIGH or MEDIUM finding and lists the
  concrete probes it ran. A source-level exhaustiveness guard enumerates every production
  `TypeTag` match site and fails if the new enum-member tag reaches an unadjudicated wildcard or
  error fallback. Run focused tests,
  `cargo fmt --check`, `cpu-lease run -n 2 -- cargo test`, and
  `cpu-lease run -n 2 -- cargo clippy --all-targets -- -D warnings`. Build a current release binary
  under a CPU lease and run the full official-suite identity ratchet; inspect every enum-gate status
  transition and accept the new scoreboard only after zero regression. If implementation touches
  generic inference or contextual typing, the randomized differential unit/repro and seeds
  1/2/3 × 400 gates become mandatory; otherwise a focused enum tsc matrix is sufficient.
- **Touch points.** Review and gates; `tooling/official-suite/tsofficial.py`,
  `tooling/official-suite/scoreboard.txt`, `tests/surface/inventory.toml`,
  `docs/backlog/completion-1.0.toml`, `docs/reference/{architecture,divergences}.md`, public
  `README.md`, and `tooling/differential/` only if the changed seam requires it.

### WU6 — immutable `placetext` re-screen and close (effort S)

- **Problem.** Removing the enum recovery type may reveal later diagnostics, and backlog `72` still
  requires independently reviewed immutable evidence before any mutation or CI witness starts.
- **Verify first.** Build a fresh release binary from reviewed HEAD and record its SHA-256 plus the
  production library route attestation.
- **Scope.** Repeat the exact detached `faf233107146ceca63bf8a6fec8f07ad43ab17e2` checkout,
  license/lock/config/source hashes, native and overlay pinned-tsc runs, and two byte-identical
  typokat summaries. Compare identities with the 2026-08-11 evidence. The expected direct delta is
  removal of `repo/src/core/types.ts:53:8 decl/enum-declaration/self`,
  `repo/src/core/types.ts:68:12 TK2304`, and the two `Corpus` `TK2305` records in `repo/src/index.ts`;
  treat any additional disappearance or new diagnostic as a finding to explain, not assumed progress.
- **Acceptance / witness.** Roots remain 9, checked/skipped/excluded remain 9/0/0, resolutions remain
  13, and notices/parse errors remain 0/0. All four enum-owned identities disappear with no new enum
  or module identity. If no recovery cascade is revealed, the screen becomes 6 incomplete records
  and 2 diagnostics: five computed object keys, one template interpolation, and the two existing
  predicate-array `TK2339` records. A different reviewer reproduces the evidence and returns PASS.
  Backlog `72` remains open unless every zero-clean and meaning-equivalence gate genuinely passes;
  no mutation/witness/CI work starts otherwise.
- **Touch points.** New compact evidence under `docs/archive/`, backlog `72`, removal of backlog `42`
  on ship, README/root/index updates, and this sprint's closure record plus archive move.

## Out of scope (explicit)

- JavaScript emit and runtime semantics: numeric reverse maps (`E[0]`), IIFE generation,
  `const enum` inlining, tree-shaking, and runtime evaluation. typokat remains a checker.
- Compiler-option profiles such as `preserveConstEnums`, `isolatedModules`, and alternate targets;
  the public profile remains the pinned strict Bundler checker profile.
- General arithmetic/comparison result typing (`45`). This sprint supplies the correct enum operand
  facts only to existing checks.
- Exhaustiveness/return-path diagnostics (`46`), predicate/assertion call narrowing (`50`), member-
  path invalidation and the wider narrowing tail (`51`), and generic scope-disposition work (`75`).
  Enum equality and switch member narrowing are in scope because they preserve the enum type model.
- General type-query/indexed-access support such as `keyof typeof E` when the syntax is independently
  deferred, package/namespace/star/default-bridge module breadth (`15`), and project-specific shims.
- The five computed object-key incompletes, template interpolation, and predicate-bearing array
  diagnostics visible in `placetext`; they retain owners `75`, `71`, and `50`/`107`.
- Backlogs `110` and `111`; the sprint must not broaden contextual or structural inference merely
  because enum members can appear as candidates.

## Decisions

- **Nominal family and members.** Each whole enum and member has declaration identity plus its proved
  primitive/literal facts. The whole node owns the canonical member family and numeric-openness
  state; it is not reducible to a member union because empty enums and opaque/ambient numeric members
  admit numeric sources that a closed union cannot express. Equal-valued members of one family are
  compatible without collapsing their identities. Ordinary literals do not acquire enum metadata.
- **One descriptor publishes both slots.** Type and value surfaces are derived from one ordered
  member table and become visible atomically. There is no duplicate enum builder and no partial
  value/type publication.
- **A bare enum is not a namespace.** Legal enum+namespace merges attach the existing namespace to
  the enum value. They do not justify synthesizing a namespace slot for every enum or relaxing
  default-module provenance globally.
- **Initializer analysis is type checking, not runtime execution.** Constant forms are folded only
  to establish type facts required by tsc parity. Other legal computed numeric members remain opaque
  nominal members after their initializer is checked once. Emit behavior stays absent.
- **Fail closed on an unrepresentable accepted form.** WU0 is the stop gate. Do not approximate an
  accepted enum member as `any`, `number`, `string`, or an ordinary literal merely to enable the
  corpus or `placetext`.
- **The real-project gate remains exactly zero.** Removing the enum family records progress and the
  next owner; it does not qualify backlog `72` or authorize witness mutations.

## Sequencing

WU0 → (WU1 + WU1B in parallel) → WU2 → WU3 → WU4 → WU5 → WU6.

Implementation follows the mandatory spec-first method. WU0 is committed alone and includes a
fixture-family checklist. WU1 owns `crates/typokat-types/src/types/`,
`crates/typokat-relate/src/relate/`, and enum rendering; WU1B owns
`crates/typokat-binder/src/binder/namespace.rs`. Their territories are disjoint and their contracts
meet only at the existing `TypeGroupId`; both seams freeze before WU2. WU2 then owns enum
construction/publication under `checker/decls/`, `type_groups.rs`, and `statements.rs`; WU3 owns
bare annotation/expression/flow consumers and runs only after WU2; WU4 owns namespace/project
integration and runs only after WU3. WU1-WU4 each land as independently reviewable implementation
commits with focused witnesses. Implementation is performed through subagents under leader
supervision. WU5 uses a fresh reviewer and may route a discovered corpus omission back through a
new spec commit before any production fix; it may not weaken an existing expectation. Only
read-only tsc probes and official-suite triage may run in parallel with implementation; publication,
project contracts, review, and immutable evidence remain ordered because each consumes the prior
semantic identity.

## Run log

- **WU0 oracle refinement (2026-08-12).** The complete TypeScript 6.0.3 matrix disproved the initial
  plain-union whole-enum model: empty enums and opaque/ambient numeric members accept arbitrary
  numeric sources; broad `number` also assigns to a closed numeric member. It also proved that
  equal-valued members of one enum are compatible, while raw string values compare and narrow
  against string members but remain non-assignable. WU1/WU3 were tightened before implementation.
  Oxc reports dynamic and numeric enum member names as parse errors, so their invalid-syntax
  diagnostics are not part of the checker corpus; accepted identifier, quoted, and static-computed
  names remain required.
- **WU0 RED gate (2026-08-12).** Every flat marker matched the exact TypeScript 6.0.3 code and source
  line. Normal/reverse Bundler configs were byte-identical at the oracle boundary. A temporary
  isolated run of both disabled corpora failed with 239 conformance differences across 17 files and
  no unexpected parse error; the committed registration remains disabled and behavior-neutral.
