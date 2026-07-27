// Backlog 101 — the shapes that must stay clean once the value types are real, so
// the fix cannot buy its diagnostics with false positives.
// tsc 6.0.3 --strict --target es2022 --lib es2022: 0 diagnostics.

declare const flag: boolean;
declare const n: number;
declare const s: string;
declare const nn: number | null;
declare const nu: number | undefined;
declare const nv: never;
declare const nul: null;
declare const obj: { size: number };
declare function wantsNumber(value: number): void;

const sameArms: number = flag ? n : n;
const widerTarget: string | number = flag ? n : s;
const literalTarget: 1 | 2 = flag ? 1 : 2;
const widenedTarget: number = flag ? 1 : 2;
const sameLiteral: 1 = flag ? 1 : 1;

// A logical whose left operand cannot take the other branch is just that left
// operand (tsc's short-circuit): `??` over a non-nullish left, `||` over an
// always-truthy left, `&&` over a never-truthy left.
const coalesceNonNullish: string = s ?? n;
const orAlwaysTruthy: { size: number } = obj || n;
const andNeverTruthy: null = nul && n;

const coalesced: number = nn ?? n;
const coalescedUndefined: number = nu ?? n;
const ored: number = nn || n;
const anded: boolean | number = flag && n;
const andedObject: number = obj && n;

// `never` is the identity element of `|`.
const neverArm: number = flag ? nv : n;
const bothNever: number = flag ? nv : nv;

wantsNumber(flag ? n : n);
wantsNumber(nn ?? n);

function returnsNumber(): number {
  return flag ? n : n;
}

// A `let` inferred from a ternary widens its literal arms, so a later assignment
// of another number is legal (tsc infers `number`, not `1 | 2`).
let counter = flag ? 1 : 2;
counter = 3;

// A `const` keeps them, so the literal union is still available.
const kept = flag ? 1 : 2;
const keptTarget: 1 | 2 = kept;
