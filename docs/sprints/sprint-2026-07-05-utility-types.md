<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/.
-->

# Sprint — utility types / M28 (2026-07-05)

**Goal.** Ship backlog [`12`](../backlog/12-utility-types.md): the ten standard utility
aliases become **built-ins via a prelude compilation unit** (each is its ordinary
mapped/conditional definition — no second evaluator), `Omit`-style composition works
(mapped key sources that are alias instantiations evaluate on demand), and the
`Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize` **intrinsics** evaluate natively.
Closes the type-level evaluation phase.

**Theme.** Leader scope-probe (`m28_scope.ts`): 9 of 10 utility shapes already work as
hand-written aliases on the M24–M27 machinery; only the `Omit` composition fails (its mapped
key source is an alias instantiation the evaluator never demands). So M28 is: (1) prelude
infrastructure — the architecturally anticipated "minimal ambient/prelude slice" (backlog
`14`), (2) one evaluator demand fix, (3) four intrinsics.

## Refs re-verified at HEAD (2026-07-05, 9bc1123)

- ✔ **Driver is single-source** — `check_source(&source)` (`src/driver.rs`): parse → bind →
  check one unit. The prelude needs a second unit bound into a ROOT scope the user scope
  chains to; prelude diagnostics must never surface (trusted source — assert clean in a unit
  test instead).
- ✔ **Omit composition gap** — `type MyOmit<T, K> = MyPick<T, MyExclude<keyof T, K>>` leaves
  the mapped node deferred: `eval_mapped` reads the key source directly and a lazy
  `Instantiation` key source never evaluates. Fix: demand-evaluate the (substituted) key
  source through the shared work-stack before key iteration.
- ✔ **`Pick`'s `K extends keyof T` constraint works at concrete instantiation** (M24
  substitutes the constraint before checking — scope probe t8 + fixture pk3's TK2344).
- ✔ **tsc probes**: fixture cross-check green; `Pick<P, "q">` → TS2344 AND still
  instantiates; `Omit<P, "a">` excess-flags via TS2353 (b optional); TS2820 (did-you-mean
  2322 variant) avoided in fixtures; Uppercase distributes over unions.

## Work units

### WU1 — Prelude compilation unit (effort M)

An embedded prelude source (Rust `include_str!` or const) parsed + bound + resolved BEFORE
user code; user's top scope chains to the prelude scope; user declarations SHADOW prelude
names (tsc-like). Prelude content: the ten aliases (`Partial`, `Required`, `Readonly`,
`Record`, `Pick`, `Omit`, `Exclude`, `Extract`, `NonNullable`, `ReturnType`) written as
ordinary TS type aliases, plus the four intrinsics declared `type Uppercase<S extends
string> = intrinsic;`. Prelude diagnostics: assert-clean unit test; never surfaced to users.
Spans from prelude types render by NAME (alias display), never by prelude location.
Touch: `src/driver.rs`, binder entry, a new `src/prelude.ts`-equivalent asset.

### WU2 — Type-model work for utility composition (effort L; expanded 2026-07-05)

Originally "one evaluator demand fix (S/M)"; the impl agent's pre-implementation probes
(run log below) proved the committed fixtures require **three type-model features**, each
an extension of existing machinery (leader-approved):

1. **Deferred `keyof` node** (`TypeTag::Keyof`): `keyof <free param>` currently collapses
   to the error type (permissive — a silent-FN generator). New deferred node survives
   substitution and evaluates once its operand is concrete, routing to the SAME existing
   keyof computation used for concrete operands today (single source of truth, no new
   semantics). Unevaluable → relates conservatively (identical-node only). Touches
   repr/store/hash/intern/substitute/eval/relate/annotations/render.
2. **Modifier/value-preserving Pick** (`MappedType.modifiers_source: Option<TypeId>`,
   identity-bearing): tsc treats `{ [P in K]: T[P] }` as homomorphic-with-key-filter
   (modifiersType = T). Capture `T` from the value's `T[P]` at lowering; in
   `assemble_mapped` resolve each filtered key against T (value type + `?`/`readonly`).
3. **Recursive mapped-alias seeding** (`reserve_mapped` + mapped template on
   `TypeDecl::Alias`), mirroring the conditional-template machinery, so `DeepPartial`'s
   self-reference is a lazy node instead of the error type.

Plus the original demand fix: in `eval_mapped`/`assemble_mapped` an evaluable key source
(Instantiation/Conditional/Mapped/deferred-keyof) evaluates through the shared work-stack
first; only then the iterable-object/union checks run (the M26 no-permissive-fallback rules
unchanged). Witness: `builtin_utilities.ts` om1–om3, `intrinsics_and_recursion.ts` dp1–dp3,
leader probe t10.

### WU3 — String intrinsics (effort M)

The evaluator intercepts the four prelude intrinsic aliases by identity: literal argument →
transformed literal (Rust `to_uppercase`/`to_lowercase`; Capitalize/Uncapitalize = first
char only); union → distribute per member; boolean/number literals in holes are already
strings by construction time; anything else (patterns, `string`, free params) stays a
symbolic instantiation relating conservatively (identical-node only; → `string` allowed).
Composes with template construction (`Greet` fixture). Unicode: use Rust char-wise
uppercase; document any multi-char-mapping divergence (ß → SS vs tsc) as it arises.

### WU4 — Independent adversarial review + ratchet (effort M)

Attack: prelude shadowing (user `type Partial<T> = T` wins; no double-diagnostics), prelude
+ TK2344 spans on user code, Omit over derived interfaces/unions, DeepPartial-style
recursion depth/memo, intrinsics on patterns/`string`/empty string/unicode, intrinsic-in-
conditional/mapped composition, `run --check` audit (utility-heavy suite files may enter
scope — there is no harness gate for utilities, so movement is organic). Ratchet.

## Out of scope (explicit)

- `Parameters`/`ConstructorParameters` (rest elements — backlog `24`); `InstanceType`,
  `ThisType`, `Awaited`, `NoInfer`, `ThisParameterType`/`OmitThisParameter`.
- Full lib.d.ts loading (backlog `14`) — the prelude carries ONLY the utility slice.
- `intrinsic` keyword outside the four string intrinsics.
- tsc's TS2820 did-you-mean variant of TK2322.

## Decisions

- **Prelude as a real compilation unit** (not programmatic type synthesis): exercises the
  same lowering/binding paths, scales to backlog `14`, and keeps definitions readable.
- **Intrinsics intercepted by prelude-declared identity** — no new node kind unless forced;
  record the representation choice in the run log.

## Run log

<!-- Append as you work. -->

### 2026-07-05 — impl agent pre-implementation probe: WU2 is materially under-scoped (BLOCKER, needs leader direction)

Verified refs re-read at HEAD; oxc 0.137.0 **does** parse `= intrinsic`
(`TSType::TSIntrinsicKeyword`) — so WU3's representation can stay keyword-based, no
sentinel needed. Then ran the leader's t10-style probe plus targeted probes
(`scratchpad/probe{A..L}.ts`) against the release binary to pin what the committed
fixtures actually require. **Three substantial evaluator/type-model gaps surfaced,
two of them entirely outside the WU descriptions. `keyof`/`Pick`/`DeepPartial` do NOT
work as the sprint assumed.** Evidence:

1. **`keyof T` over a free param collapses to the error type** (not deferred). Probe:
   `type Keys<T> = keyof T; const k: Keys<P> = 123;` → **no error** (error type is
   permissive). Direct `keyof P` (concrete) works. So Omit's key set
   `Exclude<keyof T, K>` bakes `keyof T = error` at template-build time →
   `Exclude<error, K>` → the mapped defers/mis-keys. **Fix needed: a deferred `keyof`
   node** that survives substitution and evaluates once its operand is concrete (the
   "keyof-of-concrete" the WU2 bullet gestures at — but it is a *new type node*, not a
   one-line demand fix). New `TypeTag::Keyof` (operand inline in `payload`, like
   `Infer`) touches repr/store/hash/intern/substitute/eval/relation/annotations/render.

2. **Non-homomorphic `Pick`/`Omit` drop value types AND modifiers.** `{ [P in K]: T[P] }`
   is classified non-homomorphic (constraint `K`, not `keyof <src>`), so `T[P]` →
   `MappedValue` → `assemble_mapped` substitutes the **error type** for it and emits
   **required** members. Probe: `MyPick<P,"b"> = {}` → `TK2741 { b: any }` (b is
   *required any*, tsc gives `{ b?: string }`); `MyPick<P,"a"> = { a: "wrong" }` → no
   error (value is `any`, not `number`). Consequence: **fixture `om3`
   (`Omit<P,"a"> = { a: 1 }`, expects only `TK2353`) cannot pass** — typokat yields
   `{ b: any }` (required), so it emits `TK2353` (excess `a`) **plus** an extra
   `TK2741` (missing `b`) [verified: excess + missing both fire on one literal,
   probeC x1]. **Fix needed: modifier/value-preserving Pick** — tsc treats
   `{ [P in K]: T[P] }` with `K extends keyof T` as *homomorphic with an explicit key
   filter* (modifiersType = T). Concretely: add `modifiers_source: Option<TypeId>` to
   `MappedType` (identity-bearing), capture the object `T` from the value's `T[P]` at
   lowering, and in `assemble_mapped` resolve each filtered key against T (its type +
   `?`/`readonly`). Surgical to the non-homomorphic path, but a `MappedType`
   representation change (hash/eq/substitute/child_types).

3. **Recursive mapped aliases don't self-seed → `DeepPartial` is inert.** Recursive
   *conditional* aliases get `reserve_conditional`/`conditional_template`; recursive
   *mapped* aliases get nothing, so the self-reference `DeepPartial<T[K]>` inside the
   body resolves via the `resolving:true` guard to the **error type**. Probe: a
   self-recursive `DP<T> = { [K in keyof T]?: DP<T[K]> }` → `DP<Two> = { a: { d: "s" } }`
   **clean** (should be `TK2322`); the identical *non-recursive* two-alias nesting
   catches it. **Fixture `dp3` cannot pass** without this. **Fix needed: seed a reserved
   mapped-alias template** (`reserve_mapped` + a `mapped_template` on `TypeDecl::Alias`),
   mirroring the existing conditional-template machinery, so the self-reference is a
   lazy node. Confirmed a deferred mapped **target** already rejects a literal
   (probeL: `"s"` vs `{ [K in keyof number]: … }` → `TK2322`), so seeding **alone**
   fixes `dp3` — no `keyof <primitive>`/lib.d.ts modeling required.

**Net:** WU1 (prelude, decided/foundational) and WU3 (4 intrinsics via new
`IntrinsicKind` markers + `eval_instantiation` interception + evaluating template
holes for the `Greet` fixture) are as-described. **WU2 is really three type-model
features** (deferred `keyof` node + modifier-preserving Pick + recursive-mapped-alias
seeding), each an extension of the shared mapped/conditional evaluator (in the spirit
of backlog 12 "no second evaluator path"), but well beyond the "one evaluator demand
fix / effort S/M" the plan states, and each touches the interned type model. Per the
binding rules (ask before type-model/architecture changes; don't invent scope),
**paused for leader direction** before implementing 1–3. No source touched; only
scratchpad probes + this log entry. Recommended: keep the WU1/WU3 plan; expand WU2 to
the three items above (est. L, not S/M), sequence deferred-keyof → modifier-preserving
Pick → mapped-alias seeding, and re-run an adversarial pass on the deferred-`keyof` /
`modifiers_source` relation surface (regression risk: `keyof <free param>` currently
error-lenient → deferred is stricter; audit the official suite for movement).

### 2026-07-05 — leader decision: WU2 expansion APPROVED

All three items are extensions of established patterns, not architecture deviations:
deferred `keyof` mirrors the existing deferred conditional/mapped/instantiation nodes
(and removes an error-type permissive hole — the sound direction); `modifiers_source`
follows the "identity-bearing metadata folded into the hash" invariant and tsc's own
modifiersType model; `reserve_mapped` is symmetric to `reserve_conditional`. WU2 section
rewritten above (effort L). Sequencing confirmed: WU1 → WU2 (keyof → modifiers-source →
seeding) → WU3. Extra review attention ordered on the deferred-keyof/`modifiers_source`
relation surface in WU4; official-suite movement from stricter `keyof <free param>` is
expected to be sound over-reporting — audit, don't gate-lift.
