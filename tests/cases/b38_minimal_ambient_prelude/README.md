# Backlog 38 fixture ledger

This disabled, spec-only corpus defines the entire ambient surface proposed for
the first implementation round. Each admitted declaration has been checked against
`tsc 6.0.3 --strict`; the corpus stays disabled until the implementation commit.

| Surface | Proposed prelude shape | Witness | Decision |
| --- | --- | --- | --- |
| `console` | value with `log`, `warn`, and `error` rest methods over `unknown[]` | `console_values.ts`, `console_shadow.ts` | Admit: all arguments flow safely to `unknown`, and the calls need no generic methods. |
| `Math` | value with `abs`, `floor`, `ceil`, `round`, `max`, `min`, and `random` | `math.ts` | Admit: non-generic numeric signatures use the shipped rest/call model. |
| primitive wrapper members | `String` / `Number` interfaces | focused WU1 probe | Reject: intrinsic string/number member lookup does not consult such interfaces at HEAD. |
| array instance members | `Array<T>` interface additions | focused WU1 probe | Reject: `T[]` has its own representation; `length` already works, while `pop` remains absent. No new array-member architecture belongs here. |
| `JSON` | `parse` / `stringify` | design review | Reject: the standard surface relies on permissive `any`-shaped semantics; this prelude must not introduce an unsound approximation. |

The deliberately absent members are pinned by `console.missing` and `Math.missing`
witnesses. Their omissions remain part of the honest minimal-prelude limitation,
not a claim that typokat models the full standard library.
