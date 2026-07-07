// backlog 57: `T extends (infer U)[]` over a TUPLE source must bind U to the
// element union (tsc: Elem<[1, 2]> = 2 | 1), not fall through to the unbound
// `unknown` fallback that accepts everything. Type-level inference is
// non-widening, so element literals stay literals.
type Elem<T> = T extends (infer U)[] ? U : never;
type E1 = Elem<[1, 2]>;
const a: E1 = 5; // error[TK2322]
const b: E1 = 1;
const c: E1 = 2;
type E2 = Elem<[1, "x"]>;
const d: E2 = "x";
const e: E2 = true; // error[TK2322]
type E3 = Elem<[]>;
const f: E3 = 1; // error[TK2322]: Type '1' is not assignable to type 'never'
// Control: an ARRAY source must keep failing a TUPLE pattern (false branch).
type P<T> = T extends [infer A, infer B] ? A : never;
type P1 = P<number[]>;
const g: P1 = 1; // error[TK2322]: Type '1' is not assignable to type 'never'
// Regression pin: tuple-vs-tuple positional extraction already works.
type P2 = P<[1, 2]>;
const h2: P2 = 1;
const i2: P2 = 2; // error[TK2322]: Type '2' is not assignable to type '1'
