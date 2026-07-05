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

### WU2 — Omit composition: demand-evaluate mapped key sources (effort S/M)

In `eval_mapped`/`assemble_mapped`: a key source that is itself evaluable
(Instantiation/Conditional/Mapped/keyof-of-concrete) evaluates through the shared work-stack
first; only then the iterable-object/union checks run (the M26 no-permissive-fallback rules
unchanged). Witness: `builtin_utilities.ts` om1–om3 + the leader probe t10.

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
