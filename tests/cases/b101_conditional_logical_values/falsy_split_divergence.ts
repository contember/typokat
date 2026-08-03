// Backlog 101 — DOCUMENTED DIVERGENCE (over-report). tsc reduces `&&`'s left
// operand to its definitely-falsy part. The `string` → `""` and `number` → `0`
// splits remain deferred. The shared `boolean` → `false` split is now the closure
// target because it also makes an impossible nested `&&` RHS narrow to `never`.
//
// `||` has the mirror gap for a definitely-falsy string/number left operand
// (`"" || n`, `0 || n`), but every such operand is one tsc itself rejects with TS2873
// ("This kind of expression is always falsy"), a code outside typokat's range, so
// there is no clean witness to pin here.
//
// Ledgered in docs/reference/divergences.md.
// tsc 6.0.3 --strict --target es2022 --lib es2022: 0 diagnostics.

declare const flag: boolean;
declare const s: string;
declare const n: number;
declare const nn: number | null;

// The string split remains deferred; the boolean split is the closure target.
const emptyOrNumber: "" | number = s && n; // error[TK2322]
const falseOrNumber: false | number = flag && n;

// The gap is bounded to that: a nullish operand is split identically on both
// sides, so these rows agree with tsc.
const orNullable: number = nn || n;
const andNullableTarget: boolean | number | null = nn && n;
