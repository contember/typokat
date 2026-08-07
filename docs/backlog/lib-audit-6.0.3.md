# TS 6.0.3 `lib.d.ts` surface audit (es5 core)

The pinned-standard-library construct audit required by the 1.0 manifest
([`completion-1.0.toml`](completion-1.0.toml), `[meta].lib_audit_artifact`). It records,
reproducibly, **which TypeScript constructs the standard-library surface uses** and
classifies each against typokat's shipped model and the remaining owners. It is an **audit and
explicit-input readiness proof**, not the production `lib.d.ts` loader. It established backlog
`14`'s historical start gate; the production cutover subsequently shipped and was archived after
exact-`d1aa6d4` remote CI and final WU7 **PASS** with zero unresolved HIGH/MEDIUM findings. The
machine-enforced result is
[`readiness.toml`](../../tests/fixtures/lib-es5-6.0.3/readiness.toml).

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
| Proof commits | `b424e74`, `5951968`, `3f641ea`; standalone namespace-value checker `23bad42` |

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
| **Generic methods (method-level `<T>`/`<U>`)** | `^\s+\w+<[A-Z][^>]*>\(` | pervasive | **✓ shipped (B41)** — persistent generic method/call/construct signatures; production member projection and loading shipped through the archived closure sprint |
| **Declaration merging** (interface+`var` same name; repeated `interface` blocks) | committed semantic witnesses | 28 pairs + `Date`/`Number`/`String` | **✓ shipped** — pair type/value witnesses and repeated-interface deep members reject the wrong types |
| **`namespace` type side** | `declare namespace Intl` | 1 | **✓ shipped** — `Intl.CollatorOptions` resolves and checks |
| **Standalone namespace value** | `Intl.Collator()` | 1 | **✓ shipped (WU6A / ADR-0010)** — `deep.Intl.value` rejects with `TK2322`, matching tsc, without an incomplete |
| **Type predicates** | annotation lowering | 8 | **1.0 owner → `50`**; explicit incompletes, independent of loader start |
| **Polymorphic `this` / intrinsic / symbol / bigint annotations** | annotation lowering | 173 | **1.0 owner → `75`** (164/5/3/1); explicit incompletes, independent of loader start; `object` is shipped |
| **Callable heritage compatibility** | `CallableFunction`/`NewableFunction extends Function` | 2 canonical + 2 surplus `TK2430` | canonical compatibility and surplus cardinality → parity-only `63`; neither blocks the shipped loader route |
| **`this`-parameter typing + `ThisType<T>`** | `\(this:`; `ThisType\|ThisParameterType\|OmitThisParameter` | 16 + 7 | **✓ shipped (B70)** — explicit receiver slots, `ThisType<T>`, and `ThisParameterType`/`OmitThisParameter`; production projection and loading shipped through the archived closure sprint |
| `enum` | `\benum\b` | **0** | ✓ not used by es5 core (needed for full model completeness → `42`, not for `14`) |
| `satisfies` / `as const` | `\bsatisfies\b`, `as const` | **0** | ✓ not used by es5 core (full model completeness → `44`, not for `14`) |
| Symbol / computed keys (`[Symbol.x]`) | `\[Symbol\.` | **0** in es5 | out of es5 (arrives with `es2015.iterable`; Tier B) |

## Headline — historical GO for starting backlog 14

The type-side namespace and declaration-merging work is real: all 28 constructor pairs,
`Date`/`Number`/`String` reopenings, `Intl` type and value access, and local `Array<T>` heritage are
proven. The current raw pinned artifact produces exactly 4 diagnostics and 181
incompletes:

- `63`: 4 `TK2430` diagnostics at the two canonical sites: 2 compatibility diagnostics plus 2
  surplus diagnostics (parity-only);
- `50`: 8 type-predicate incompletes (independent 1.0 blocker);
- `75`: 173 annotation incompletes: `this` 164, `intrinsic` 5, `symbol` 3, `bigint` 1
  (independent 1.0 blockers).

The machine verdict was **GO for starting backlog 14**: no raw or semantic witness retains a
namespace owner, and `deep.Intl.value` is one of exactly 66 synthetic `TK2322` diagnostics with no `TK2304` or
added incomplete. The namespace/declaration-merging lifecycle is closed, so loader work may start;
this verdict alone did not mean that the standard library was loaded. The production loader has
since shipped under the archived closure sprint, while backlog `63` owns canonical heritage
compatibility and surplus cardinality as separate parity work. Owners `50` and `75` remain
mandatory model work; `42` and `44`
remain 1.0 model blockers but are absent from ES5 core.

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
