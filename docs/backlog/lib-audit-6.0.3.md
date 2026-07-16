# TS 6.0.3 `lib.d.ts` surface audit (es5 core)

The pinned-standard-library construct audit required by the 1.0 manifest
([`completion-1.0.toml`](completion-1.0.toml), `[meta].lib_audit_artifact`). It records,
reproducibly, **which TypeScript constructs the standard-library surface uses** and
classifies each against typokat's shipped model and the remaining owners. It is an **audit and
explicit-input readiness proof**, not `lib.d.ts` loading (backlog `14`). The machine-enforced result
is [`readiness.toml`](../../tests/fixtures/lib-es5-6.0.3/readiness.toml).

## Pin (reproducible inputs)

| Input | Value |
|---|---|
| TypeScript version | **6.0.3** (`tsc --version`) |
| Official-suite `PINNED_SHA` | `050880ce59e30b356b686bd3144efe24f875ebc8` |
| Authoritative upstream path | `lib/lib.es5.d.ts` (npm package output; not `src/lib/es5.d.ts`) |
| Upstream Git blob | `496166ca309c28ab7e07ea0154a406f26b6cf26a` |
| SHA-256 | `bcd24271a113971ba9eb71ff8cb01bc6b0f872a85c23fdbe5d93065b375933cd` |
| Size | 218,972 bytes; 4,599 LF-terminated lines; 80 interfaces |
| Committed artifact | `tests/fixtures/lib-es5-6.0.3/lib.es5.d.ts` |
| Proof commits | `b424e74`, `5951968`, `3f641ea` |

es5 core is the **minimum** surface: `lib.d.ts` itself is a reference file that pulls in
`lib.es5.d.ts` + `lib.dom.d.ts` + …; es5 is the part every non-`--lib` program loads and
the part whose own source text exercises nearly the whole type model. The DOM and
`es2015.*` libs add more (symbol/computed keys, `[Symbol.iterator]`, more namespaces) but
do not remove any es5 requirement, so es5 is the sound floor for "what blocks `14`".

## Classification table

`✓ shipped` = typokat models it today; `✗ blocks start of 14` = the remaining declared
architecture dependency; `1.0 owner` and `parity owner` remain explicit without blocking the start
of loader work.

| Construct family | es5 evidence (probe) | Count | typokat status |
|---|---|---:|---|
| `interface` declarations | `^interface ` | 80 | ✓ shipped (M2 / F1) |
| Index signatures `[k: string\|number]` | `\[[a-z]+: (string\|number)\]` | 18 | ✓ shipped (M19) |
| `keyof` | `keyof ` | 12 | ✓ shipped (M20; `K extends keyof T` via M24/M28) |
| Conditional types (`T extends U ? … : …`) | `extends .* \? ` | 13 | ✓ shipped (M25) — e.g. `Exclude`, `Extract`, `Awaited`, `ThisParameterType` |
| Mapped types (`{ [K in …]: … }`) | `\[[A-Za-z]+ in ` | 5 | ✓ shipped (M26; `Partial`/`Required`/`Readonly`/`Pick`/`Record` are M28 prelude built-ins) |
| Generic type aliases | `^type [A-Za-z]+<` | 10+ | ✓ shipped (M9 / M28) |
| `readonly` members / arrays | `readonly ` | 130 | ✓ shipped (M14 / b64) |
| Optional params & members (`?:`) | `\?\s*:` | 349 | ✓ shipped (M21 members, M32 params) |
| Method / function overloads (non-generic) | `concat`×2, `reduce`×2/3, `replace`… | many | ✓ shipped (M33) |
| **Generic methods (method-level `<T>`/`<U>`)** | `^\s+\w+<[A-Z][^>]*>\(` | pervasive | **✓ shipped (B41)** — persistent generic method/call/construct signatures; member projection and lib loading remain `14` |
| **Declaration merging** (interface+`var` same name; repeated `interface` blocks) | committed semantic witnesses | 28 pairs + `Date`/`Number`/`String` | **✓ shipped** — pair type/value witnesses and repeated-interface deep members reject the wrong types |
| **`namespace` type side** | `declare namespace Intl` | 1 | **✓ shipped** — `Intl.CollatorOptions` resolves and checks |
| **Standalone namespace value** | `Intl.Collator()` | 1 | **✗ blocks start of 14 → `43`** — no standalone namespace value metadata/qualified receiver; requires a superseding architecture decision |
| **Type predicates** | annotation lowering | 8 | **1.0 owner → `50`**; explicit incompletes, independent of loader start |
| **Polymorphic `this` / object / intrinsic / symbol / bigint annotations** | annotation lowering | 179 | **1.0 owner → `75`** (164/6/5/3/1); explicit incompletes, independent of loader start |
| **Callable heritage compatibility** | `CallableFunction`/`NewableFunction extends Function` | 2 canonical + 2 surplus `TK2430` | canonical compatibility → `14`; surplus cardinality → parity-only `63` |
| **`this`-parameter typing + `ThisType<T>`** | `\(this:`; `ThisType\|ThisParameterType\|OmitThisParameter` | 16 + 7 | **✓ shipped (B70)** — explicit receiver slots, `ThisType<T>`, and `ThisParameterType`/`OmitThisParameter`; member projection/loading remain `14` |
| `enum` | `\benum\b` | **0** | ✓ not used by es5 core (needed for full model completeness → `42`, not for `14`) |
| `satisfies` / `as const` | `\bsatisfies\b`, `as const` | **0** | ✓ not used by es5 core (full model completeness → `44`, not for `14`) |
| Symbol / computed keys (`[Symbol.x]`) | `\[Symbol\.` | **0** in es5 | out of es5 (arrives with `es2015.iterable`; Tier B) |

## Headline — current NO-GO boundary

The type-side namespace and declaration-merging work is real: all 28 constructor pairs,
`Date`/`Number`/`String` reopenings, `Intl` type access, and local `Array<T>` heritage are proven.
The raw pinned artifact produces exactly 4 diagnostics and 188 incompletes:

- `43`: 1 standalone `Intl` namespace-value incomplete — the sole architecture stop and only
  blocker to starting `14`;
- `14`: 2 canonical `TK2430` diagnostics for apparent `Function` compatibility;
- `63`: 2 surplus `TK2430` diagnostics at those same sites (parity-only);
- `50`: 8 type-predicate incompletes (independent 1.0 blocker);
- `75`: 179 annotation incompletes: `this` 164, `object` 6, `intrinsic` 5, `symbol` 3, `bigint` 1
  (independent 1.0 blockers).

The verdict remains **NO-GO** until backlog `43` records and implements a superseding architecture
decision for standalone namespace value identity and qualified receivers. Do not infer that `50`
or `75` are optional for 1.0, or that the canonical backlog-14 compatibility work is already
implemented. `42` and `44` remain 1.0 model blockers but are absent from ES5 core.

## Reproduce

```sh
LIB="$(dirname "$(readlink -f "$(which tsc)")")/../lib/lib.es5.d.ts"
tsc --version                                             # must print 6.0.3
grep -cE '^interface '                     "$LIB"         # interfaces
grep -nE '^\s+[A-Za-z]+<[A-Z][^>]*>\('     "$LIB"         # generic methods
comm -12 <(grep -oE '^declare var [A-Za-z0-9]+' "$LIB" | sed 's/declare var //' | sort) \
         <(grep -oE '^interface [A-Za-z0-9]+' "$LIB" | sed 's/interface //' | sort)   # interface+var merges
grep -oE '^interface [A-Za-z0-9]+' "$LIB" | sort | uniq -d   # repeated interface blocks
grep -nE '\(this:' "$LIB"; grep -cE 'ThisType|ThisParameterType' "$LIB"   # this-typing
grep -cE '\benum\b|\bsatisfies\b' "$LIB"                 # 0 in es5 core
```

For the authoritative byte pin, semantic witnesses, raw/synthetic measurements, exact owner split,
and offline checker gate, use
[`tests/fixtures/lib-es5-6.0.3/README.md`](../../tests/fixtures/lib-es5-6.0.3/README.md) and
[`readiness.toml`](../../tests/fixtures/lib-es5-6.0.3/readiness.toml).
