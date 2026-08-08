# Sprint — default module slots (2026-08-08)

**Goal.** Admit local acyclic default declaration exports, default expression exports, and default
imports on the bounded production Bundler route through a structurally distinct default slot.

**Theme.** A module has one default export slot. It is not a named export whose spelling happens to
be `default`. This sprint adds that slot to frontend evidence, project binding, checking, and
reporting. It keeps the existing named value/type map intact and admits no named/default bridge in
this first slice. Success means supported default forms are
checked in the full production default-library route in either root order, while missing defaults,
missing targets, unsupported forms, and every result channel remain exact and deterministic.

## Refs re-verified at HEAD (`ab428fd`, 2026-08-08)

`✔` = confirmed live · `⚠` = drift or nuance caught.

- ✔ `ProjectProgram` carries a list of `ProjectImport` rows whose `imported: String` has no
  structural named/default distinction — `crates/typokat-frontend/src/frontend.rs:135-163`.
- ✔ Production inventory classifies every default import as unsupported before resolution and
  records every default declaration/expression export as `default-export -> unsupported` —
  `crates/typokat-frontend/src/frontend.rs:1217-1248`,
  `crates/typokat-frontend/src/frontend.rs:1522-1531`, and
  `crates/typokat-frontend/src/frontend.rs:2169-2192`.
- ✔ The admitted named path resolves through `oxc_resolver`, retains exact missing-module
  locations, and converts imports into dependency-ordered project rows —
  `crates/typokat-frontend/src/frontend.rs:1312-1402` and
  `crates/typokat-frontend/src/frontend.rs:957-1018`.
- ✔ The checker models one export surface as `BTreeMap<String, ExportedSlots>` and resolves an
  import by looking up `import.imported`; adding the string `"default"` there would conflate the
  two spaces — `crates/typokat-check/src/check/checker/mod.rs:1482-1494` and
  `crates/typokat-check/src/check/checker/mod.rs:3852-3907`.
- ✔ Export collection visits only `ExportNamedDeclaration`. Its declaration helper already proves
  the required ordinary slot shapes: a function has a value slot, while a class has value and type
  slots — `crates/typokat-check/src/check/checker/mod.rs:3922-3967` and
  `crates/typokat-check/src/check/checker/mod.rs:4004-4043`.
- ⚠ Lexical reservation already sees default-export expressions and recognizes default class and
  function declarations, but the statement checker still reports every default export incomplete
  — `crates/typokat-check/src/check/checker/lexical_events.rs:938-942`,
  `crates/typokat-check/src/check/checker/lexical_events.rs:1984-2011`, and
  `crates/typokat-check/src/check/checker/statements.rs:778-789`.
- ⚠ Ordinary binding enters declarations only through direct or named-export statements. Named
  function/class binders allocate storage only when an identifier exists, so anonymous default
  declarations cannot be implemented by inventing a lexical name —
  `crates/typokat-binder/src/binder/bind.rs:2097-2101`,
  `crates/typokat-binder/src/binder/bind.rs:2193-2197`, and
  `crates/typokat-binder/src/binder/bind.rs:2447-2480`.
- ✔ The production Bundler driver alone selects the frontend-certified source-re-export compiler;
  explicit-file input takes the separate frozen route — `crates/typokat-driver/src/driver.rs:310-344`.
- ✔ The production compiler reserves user events, then continues the project binder after the
  packaged library. No replay plan is retained —
  `crates/typokat-check/src/check/checker/library_compiler.rs:6015-6051` and
  `crates/typokat-check/src/check/checker/library_compiler.rs:6172-6196`.
- ✔ The existing black-box B15 contract compares complete JSON/stderr bytes in both root orders and
  freezes the explicit/legacy routes independently —
  `tests/b15_acyclic_source_reexports_cli.rs:253-285` and
  `tests/b15_acyclic_source_reexports_cli.rs:420-428`.
- ✔ Fresh probes against
  `/run/user/1000/fnm_multishells/1002937_1784884227968/bin/tsc` report `Version 6.0.3`.
  Named and anonymous default classes import in value and type space; default functions and
  expressions normally import only in value space. `export default Identifier` preserves the
  referenced symbol's value/type spaces, so an unmerged class identifier retains both. `import type
  D from` is valid, and direct default import from a module without a default reports `TS1192`.
  The alternate named spellings have different missing-member behavior and remain deferred. Normal
  and reversed `files` lists produced the same oracle diagnostics.

## Fixed semantic contract

The default surface is structurally separate:

```text
ModuleExportSurface {
    default: Option<ExportedSlots>,
    named: BTreeMap<String, ExportedSlots>,
}
```

This is a semantic shape, not a required public type name. A default import reads only `default`.
A named import reads only `named`. The implementation must not encode the default slot by inserting
the key `"default"` into the named map, synthesizing a local identifier, or falling back from one
slot to the other.

The admitted forms are:

- named and anonymous `export default class` with value and type slots;
- named and anonymous `export default function` with a value slot only;
- `export default <expression>` with full expression diagnostics: ordinary expressions publish the
  inferred value, while a bare local identifier preserves its exact value/type slots only after
  namespace provenance is proven absent;
- local default imports, including `import type D from`, through the existing local-relative
  extensionless and `.js` to `.ts` Bundler resolver policy. The declaration must contain only that
  default specifier; mixed default+named and default+namespace imports remain explicit non-clean
  forms, with every specifier retained in accounting.

A named default declaration retains its lexical name inside the exporting module but does not
create a named export. An anonymous declaration creates no lexical name. Pinned `tsc 6.0.3`
reports `TS2652` when a direct named default class or function participates in a same-name namespace
merge, so those forms remain deterministic explicit non-clean input; exact `TK2652` parity is not
part of this slice. Missing physical targets remain `TK2307`. A direct default import from a
resolved module without a default reports exact `TK1192` at the local default binding provenance.
Every named-symbol-backed default producer requires proven-absent namespace provenance. In
particular, the otherwise legal `class/function + namespace + export default Identifier` form
fails closed instead of projecting only part of its value/type/namespace surface. No namespace
slot is projected. Existing type-only value barriers retain their documented `TK2304` stand-in for
`TS1361`/`TS1362`.

## Work units

### WU1 — commit the oracle and RED contract (effort M)

- **Problem.** The current B15 corpus freezes default import/export as unsupported but does not
  specify their semantics — `tests/cases/b15_acyclic_source_reexports/contract.json` and
  `tests/cases/README.md:260-285`.
- **Verify first.** Run every proposed source under pinned `tsc 6.0.3 --strict --noEmit --module
  esnext --moduleResolution bundler` with normal and reversed `files` lists. Record exact ordered
  diagnostics and confirm byte identity after path normalization. Separately run the current
  production CLI and prove the supported rows are RED only because default forms are explicit
  exit-3 notices.
- **Scope.** Add a permanently disabled raw corpus under
  `tests/cases/b15_default_module_slots/`, a machine-readable oracle contract, and a dedicated
  ignored black-box integration contract. Cover named/anonymous classes and functions, expression
  exports, `export default Identifier`, direct and type-only default imports, same-file lexical use
  of named declarations, and both root orders. Add exact missing target/default and wrong-space
  controls. Pin direct named-default class/function namespace merges as `TS2652` oracle rows with a
  deterministic explicit non-clean typokat disposition. Separately pin the legal namespace-only,
  class+namespace, and function+namespace `export default Identifier` forms as explicit non-clean
  because this slice cannot project their namespace slots. Local `export { Local as default }`,
  named-default import, source-default re-export, mixed import, and duplicate-default rows are also
  explicit non-clean controls. Any module with more than one default producer is non-clean before
  publication, independent of producer kinds; it never overwrites the first slot. Update only the
  fixture index and diagnostic inventory needed to describe the spec.
- **Acceptance / witness.** The oracle contract is machine-complete; every fixture text and both
  config orders match it; all intended post-cutover summaries are recorded; the pre-change
  production binary fails every supported row through the frozen default notice; and star,
  namespace, cycle, package, CommonJS, all mixed-import, and all duplicate-default controls retain
  deterministic non-clean identities without dropping a specifier.
- **Commit boundary.** Commit WU1 alone. The fixture directory remains disabled and the black-box
  acceptance remains ignored, so the spec commit changes no production behavior.
- **Touch points.** `tests/cases/b15_default_module_slots/**`,
  `tests/b15_default_module_slots_cli.rs`, `tests/cases/README.md`.

### WU2 — retain typed frontend evidence without cutting over (effort M)

- **Problem.** Default imports are discarded before resolver/dependency accounting, and no typed
  default-export evidence reaches the checker.
- **Verify first.** Trace OXC's exact AST variants for all WU1 forms. Prove the local default binding
  span that owns direct `TS1192`. Confirm admitted default-import edges enter the same acyclic
  dependency order as named-import edges.
- **Scope.** Add explicit named/default import identity and opaque frontend-certified default-export
  evidence. Resolve admitted local default edges through the existing resolver only. Preserve
  declaration kind, anonymity, expression/declaration span, type-only provenance, all
  named-symbol namespace provenance, and exact producer count. Extend dependency and cycle
  evidence for admitted default imports without teaching the checker to infer facts from raw AST
  or strings. Preserve every specifier in mixed-import evidence. This is an unreachable candidate
  product: the public route and its exact bytes remain frozen through WU5, including the currently
  accidental named-map handling of deferred named-default spellings. Production accounting changes
  only in WU6.
- **Acceptance / witness.** Focused frontend tests prove exact evidence and target identities in
  both root orders. Fault controls reject mismatched AST evidence, invalid target indexes,
  dependency-order drift, duplicate producers, dropped mixed-import specifiers, and attempts to
  represent the default as a named-map key. Candidate classification records local
  `export { Local as default }`, named default-import syntax, source re-exports involving a target
  default, direct named-default namespace merges, and namespace-bearing/unknown identifier
  provenance as unsupported, but none of those candidate dispositions is public before WU6.
  Existing named source re-export evidence and all public bytes remain byte-for-byte stable.
- **Touch points.** `crates/typokat-frontend/src/frontend.rs` and focused frontend tests only.

### WU3 — independently review and commit frontend evidence (effort S)

- A different subagent reviews only the WU2 diff and returns PASS/FAIL. It corrupts producer count,
  target index, import kind, mixed-specifier inventory, identifier provenance, and dependency order;
  every control must fire.
- The leader runs focused frontend gates and existing B15/B72 accounting guards, verifies staged
  paths, and commits this green cluster before checker work starts. No WU4 edit may be present in
  the commit.

### WU4 — bind, publish, and check the distinct default slot (effort L)

- **Problem.** Project export collection has only a named map; named default declarations are not
  walked by ordinary binding/checking, anonymous declarations have no declaration storage, and
  expression defaults have no publication storage.
- **Verify first.** Before editing, map one named class, anonymous class, named function, anonymous
  function, and expression from lexical reservation through binding, type completion, publication,
  import placeholder fill, and statement checking. Measure only if a profile is needed; this sprint
  has no performance target.
- **Scope.** Introduce the separate default slot in project export surfaces and import lookup.
  Reuse ordinary class/function construction for named declarations. Give anonymous declarations
  and expression exports declaration-site-owned storage without publishing a fake lexical name.
  Check default bodies/expressions exactly once under their existing lexical event owner. Publish a
  class's value/type pair atomically and a function/ordinary-expression value only. Every
  named-symbol-backed producer projects exact value/type slots only when namespace provenance is
  proven absent. Direct named-default namespace merges, namespace-only symbols, class+namespace,
  function+namespace, and unknown provenance fail closed.
  Preserve type-only barriers. Implement exact `TK1192` for direct missing-default imports and
  retain `TK2307` for missing modules. More than one default producer remains explicit non-clean
  input and cannot reach slot insertion or overwrite.
- **Acceptance / witness.** Focused semantic tests pass for every WU1 supported row in both orders.
  Controls prove: a named default declaration is not a same-name named export; an anonymous default
  creates no local binding; function/expression defaults do not acquire a type slot; a class keeps
  both slots; `export default ClassIdentifier` keeps value and type without its named export;
  namespace-bearing/unknown identifiers do not project; a type-only default import cannot be used
  as a value; and no missing or unavailable slot falls through an error type as a clean result.
- **Boundaries.** Default publication belongs to user module scopes in the mutable project delta.
  It must not write the frozen library prefix, change collision preflight/private rebuild routing,
  add replay-plan retention, or bypass replay-aware resolution. Run the replay raw-access audit and
  the B103 library-merge controls after the focused semantic gate.
- **Touch points.** `crates/typokat-binder/src/binder/`,
  `crates/typokat-check/src/check/checker/mod.rs`,
  `crates/typokat-check/src/check/checker/statements.rs`, lexical ownership only where the existing
  reservation is incomplete, `crates/typokat-diagnostics/src/diagnostics/mod.rs`, and focused unit
  tests.

### WU5 — independently review and commit checker semantics (effort M)

- A different subagent reviews only the WU4 diff against fresh `tsc 6.0.3` probes in both source
  orders. It hunts anonymous declaration loss, default/named fallback, value/type confusion,
  namespace truncation, duplicate overwrite, skipped expression checking, and error-type silence.
- The reviewer must break class type publication, function type absence, bare-identifier
  provenance, namespace refusal, duplicate refusal, and `TK1192` binding ownership. Any HIGH or
  MEDIUM finding returns to WU4 and requires a fresh independent review.
- The leader runs focused semantic, replay-access, and B103 gates, verifies staged paths, and commits
  this green cluster before production accounting changes. No WU6 edit may be present.

### WU6 — atomic production Bundler cutover (effort M)

- **Problem.** Removing the default notices before semantics are live would turn skipped files into
  a false-clean result.
- **Verify first.** Force the WU1 production acceptance RED against the current route, then force
  each evidence-corruption control against the reviewed WU4 semantics. Do not change expected JSON
  to match an implementation result.
- **Scope.** Route the production Bundler frontend/compiler through the certified default product
  and remove supported default unsupported notices only in the same change. Atomically activate the
  candidate refusals for local export-list aliases, named default-import syntax, source re-exports
  involving a target default, mixed imports, duplicate producers, and namespace-bearing producers;
  none may continue through the named map or become false-clean. Count a default import as resolved
  only after the checker consumes the target slot. Keep explicit-file and legacy routes frozen.
  Preserve every checked/skipped root, resolution target, notice, parse, incomplete, and diagnostic
  identity in the deterministic summary.
- **Acceptance / witness.** Enable the dedicated black-box contract. All supported rows reach
  semantic checking, both root orders are byte-identical, directory and explicit-config invocation
  agree, and missing module/default results carry their exact code, span, owner, target, and stderr.
  Negative controls fire for the pre-change accidental named-map admission, default/named
  substitution, dropped root, changed target, reversed order, premature accounting, unknown stderr,
  and unresolved-target suppression. Deferred forms retain their exact exit and identity. Existing
  B15 and B72 integration contracts do not move except for the explicitly owned default rows.
- **Touch points.** `crates/typokat-driver/src/driver.rs`,
  `crates/typokat-check/src/check/checker/library_compiler.rs`, production frontend mode,
  `tests/b15_default_module_slots_cli.rs`, and exact owned baselines.

### WU7 — independent public-route review and cutover commit (effort M)

- A different subagent reviews only the WU6 diff without relying on prior reviewers. It runs fresh
  `tsc 6.0.3` probes for all admitted declaration/expression/import forms in both orders and returns
  PASS/FAIL with concrete repros.
- Hunt false negatives from default/named fallback, anonymous declaration loss, value/type slot
  confusion, duplicate or skipped expression checking, order-dependent publication, missing-target
  error types, statement-owner drift, premature summary cleanup, and writes across the frozen
  library boundary.
- The reviewer must deliberately break default-vs-named isolation, class type publication,
  function type absence, namespace refusal, duplicate refusal, mixed-import completeness, root
  order, resolver target, and accounting. A negative control that does not fire invalidates the
  corresponding gate. Any HIGH or MEDIUM finding returns to WU6 and requires a fresh independent
  review.
- The leader runs the complete black-box gate, verifies staged paths, and commits this green public
  cutover before documentation closure.

### WU8 — leader closure gates and documentation (effort S)

- The leader verifies the exact diff and staged paths, then runs the focused oracle/black-box
  contracts, replay access audit, B103 controls, full workspace tests, full conformance, formatting,
  `cargo clippy --all-targets -- -D warnings`, official-suite ratchet, and docs lint.
- If any implementation touches inference, contextual typing, argument walking, overload
  resolution, or the relation engine, the randomized differential gate is mandatory: compare a
  scratch pre-change release binary with the candidate for at least the committed repros and 400
  cases each at seeds 1, 2, and 3. Also run the negative control against a known broken binary and
  prove the gate fires. Zero divergence without that falsifier is not evidence.
- Update `CLAUDE.md`, `README.md`, architecture/divergence references, backlog `15`, and docs indexes
  to the exact shipped boundary. Explicitly update `docs/reference/scope.md` so `TK1192` is recorded
  as the narrow semantic-module exception to its broad `1xxx` parse/grammar rule. Record the oracle,
  test counts, three independent review results, and commit map in the closure header, then archive
  this sprint. Backlog `15` stays open for the deferred module breadth; backlog `72` stays open
  unless its own complete gate later passes.

## Out of scope (explicit)

- Local `export { Local as default }`; named default-import syntax; source re-exports involving a
  target default; mixed default+named or default+namespace imports; namespace imports; star or
  namespace re-exports; namespace-bearing source targets; cycles.
- Packages, `node_modules`, `@types`, `.d.ts` loading, config/root breadth, paths, references, and
  tsconfig inheritance.
- CommonJS, `export =`, import-equals, NodeNext/Node16, side-effect imports, import attributes, and
  alternate host profiles.
- New checker-model fixes discovered in real projects, any library-base/collision redesign,
  parallel cross-file identity, incrementality, and public witness/CI work.
- Duplicate-default diagnostic parity (`TS2528`, `TS2323`, and `TS2393`); every module with more
  than one default producer instead preserves one deterministic explicit non-clean outcome.
- Candidate-specific branches, shims, allowlists, threshold changes, and performance claims.

## Decisions

- The default slot is structurally distinct. A `"default"` key in the named map is forbidden.
- `export default Identifier` is in scope because the exact oracle preserves its value/type slots,
  but every named-symbol-backed producer requires proven-absent namespace provenance. Direct named
  default declarations merged with namespaces are oracle-invalid `TS2652` controls; legal
  namespace-bearing identifier defaults fail closed because this sprint does not project
  namespaces. Local export-list aliases, named import syntax, and source re-exports involving a
  target default stay explicit follow-up work.
- Direct default-import absence uses exact `TK1192`. This is a narrow semantic-module exception to
  the scope map's broad `1xxx` parse/grammar bucket and must be documented at cutover.
- Any second default producer is deterministic explicit non-clean input. Do not overwrite the slot
  or claim shape-dependent `tsc` diagnostic parity in this slice.
- Anonymous default declarations use declaration-site identity. They do not receive generated
  source names or hidden lexical bindings.
- Project accounting changes only in WU6, after separately reviewed and committed frontend and
  checker clusters are green.

## Ownership, sequence, and machine rules

The sequence is strictly WU1 → WU2 → WU3/commit → WU4 → WU5/commit → WU6 → WU7/commit → WU8.
Keep one active RED/root-cause cluster. Do not start another semantic front while one is unresolved.

- The leader owns WU1, verification, commits, public docs, indexes, and archival. WU1 is a separate
  spec commit.
- Implementation subagents own one cluster at a time: WU2 frontend, WU4 checker, then WU6 public
  cutover. They do not edit docs, unrelated tests, baselines, or supervisor files. A build error
  outside ownership is reported as another worker's change, not fixed.
- WU3, WU5, and WU7 each use a different independent review subagent. Reviews are read-only except
  for `/tmp` probes and never edit implementation or expectations. A reviewer for one cluster may
  not review another cluster.
- No production implementation is written by the leader. No implementation commit lands before
  independent review and leader verification.
- Serialize every Cargo command through
  `flock -w 3600 /tmp/typokat-perf.lock -c '...'`. Run CPU-heavy work through
  `cpu-lease run -n 2 -- ...`. This sprint has no benchmark; if profiling becomes necessary, use a
  lease with `--no-smt` and record that it is diagnostic evidence, not an acceptance number.
- Commit explicit paths atomically. Print `git diff --cached --name-only` before each commit and
  verify it matches the intended ownership exactly.

## Run log

<!-- Append discoveries and deviations here. Graduate durable work to backlog/decision/reference. -->

## Post-closure action — backlog `72` placetext re-screen

After this sprint is archived, run a new read-only backlog-`72` screen of only the immutable
`lokicik/placetext` commit `faf233107146ceca63bf8a6fec8f07ad43ab17e2` using the archived identity
digests, pinned `tsc 6.0.3`, a fresh current-commit release binary, and independent empty caches.
Preserve the native-versus-overlay distinction and re-establish target/library equivalence instead
of assuming it. Record the new first blocker and every non-clean channel. This is a placetext
re-screen, not a qualification promise: do not create the witness descriptor, mutation pack, or CI
gate unless the separate backlog-`72` zero-clean and meaning-equivalence gates genuinely pass.
