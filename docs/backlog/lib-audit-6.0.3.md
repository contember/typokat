# TS 6.0.3 `lib.d.ts` surface audit (es5 core)

The pinned-standard-library construct audit required by the 1.0 manifest
([`completion-1.0.toml`](completion-1.0.toml), `[meta].lib_audit_artifact`). It records,
reproducibly, **which TypeScript constructs the standard-library surface uses** and
classifies each against typokat's shipped milestones (M0–M33, B41 generic signatures, and B70
receiver typing) and the remaining track-A backlog (`42`–`44`). It is an **audit only** — no `lib.d.ts`
loading is implemented here (backlog `14`); the audit is what tells us what `14` is
blocked on.

## Pin (reproducible inputs)

| Input | Value |
|---|---|
| TypeScript version | **6.0.3** (`tsc --version`) |
| Official-suite `PINNED_SHA` | the `v6.0.3` tag (`tooling/official-suite/tsofficial.py`) |
| Audited file | `lib.es5.d.ts` (the mandatory es5 core; 4599 lines, 80 interfaces) |
| Resolved from | `$(dirname $(readlink -f $(which tsc)))/../lib/lib.es5.d.ts` |
| Method | `grep`/`comm` construct-family probes (commands recorded per row below) |

es5 core is the **minimum** surface: `lib.d.ts` itself is a reference file that pulls in
`lib.es5.d.ts` + `lib.dom.d.ts` + …; es5 is the part every non-`--lib` program loads and
the part whose own source text exercises nearly the whole type model. The DOM and
`es2015.*` libs add more (symbol/computed keys, `[Symbol.iterator]`, more namespaces) but
do not remove any es5 requirement, so es5 is the sound floor for "what blocks `14`".

## Classification table

`✓ shipped` = typokat models it today (milestone cited); `✗ blocks 14` = a silently-permissive
family that must land before the lib can be loaded soundly (owner cited).

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
| **Declaration merging** (interface+`var` same name; repeated `interface` blocks) | `comm -12` vars∩ifaces; `uniq -d` ifaces | 28 pairs + `Date`/`Number`/`String` | **✗ blocks 14 → `43`** — every constructor (`Array`, `String`, `Object`, `Math`, `JSON`, …) is an `interface X` + `declare var X: XConstructor` merge |
| **`namespace`** (type side) | `declare namespace ` | 1 (`Intl`) | **✗ blocks 14 → `43`** |
| **`this`-parameter typing + `ThisType<T>`** | `\(this:`; `ThisType\|ThisParameterType\|OmitThisParameter` | 16 + 7 | **✓ shipped (B70)** — explicit receiver slots, `ThisType<T>`, and `ThisParameterType`/`OmitThisParameter`; member projection/loading remain `14` |
| `enum` | `\benum\b` | **0** | ✓ not used by es5 core (needed for full model completeness → `42`, not for `14`) |
| `satisfies` / `as const` | `\bsatisfies\b`, `as const` | **0** | ✓ not used by es5 core (full model completeness → `44`, not for `14`) |
| Symbol / computed keys (`[Symbol.x]`) | `\[Symbol\.` | **0** in es5 | out of es5 (arrives with `es2015.iterable`; Tier B) |

## Headline — what actually blocks item `14`

One family in the es5 core is still silently permissive in typokat and gates a **sound**
`lib.d.ts` load:

1. **`43` namespaces + declaration merging** — the entire constructor surface is
   `interface X` merged with `declare var X: XConstructor` (28 pairs), plus multi-block
   `interface Date`/`Number`/`String` merges and `declare namespace Intl`. Multi-slot symbols
   give value/type separation but not the *merge* (the var's type must see the merged interface).
Generic method, call, and construct signatures are now represented persistently (B41), but the
library still cannot expose `Array.map`, `Function.bind`, or other members until `14` provides
member projection and declaration loading. `42` (enums) and `44` (`satisfies`/`as const`) are confirmed **absent from es5 core** (0 uses),
so they do not gate `14` — they gate DoD condition 1 (full model completeness) for real-world
code and other libs. This matches the note already in backlog `42` ("not needed by lib.d.ts
itself").

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
