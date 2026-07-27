// Backlog 101 — DOCUMENTED DIVERGENCE (over-report). tsc reduces `&&`'s left
// operand to its *definitely* falsy part: `string` → `""`, `number` → `0`,
// `boolean` → `false`. typokat reuses the narrowing engine's truthiness split
// (`NarrowOp::Truthy`), which deliberately does NOT split a falsy-capable
// primitive and only removes the always-falsy `null`/`undefined`, so the `&&`
// result is a safe SUPERSET of tsc's. Same verdict wherever the target admits the
// whole primitive (every other fixture in this corpus); only a target that admits
// just the falsy remainder diverges, and it diverges by over-reporting.
//
// `||` has the mirror gap for a *definitely falsy* left operand (`"" || n`,
// `false || n`), but every such operand is one tsc itself rejects with TS2873
// ("This kind of expression is always falsy"), a code outside typokat's range, so
// there is no clean witness to pin here.
//
// Ledgered in docs/reference/divergences.md.
// tsc 6.0.3 --strict --target es2022 --lib es2022: 0 diagnostics.

declare const flag: boolean;
declare const s: string;
declare const n: number;
declare const nn: number | null;

// tsc says `"" | number` / `false | number`; typokat says `string | number` /
// `boolean | number`.
const emptyOrNumber: "" | number = s && n; // error[TK2322]
const falseOrNumber: false | number = flag && n; // error[TK2322]

// The gap is bounded to that: a nullish operand is split identically on both
// sides, so these rows agree with tsc.
const orNullable: number = nn || n;
const andNullableTarget: boolean | number | null = nn && n;
