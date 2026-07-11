// Backlog 67 — ReturnType's modeled callable constraint must reject non-callable
// arguments before evaluating the conditional body. Cross-checked with tsc 6.0.3 --strict.

type Invalid = ReturnType<number>; // error[TK2344]

type Nullary = ReturnType<() => string>;
const nullary: Nullary = "ok";
const wrongNullary: Nullary = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'

type Unary = ReturnType<(value: number) => boolean>;
const unary: Unary = true;

type Rest = ReturnType<(...values: number[]) => number>;
const rest: Rest = 1;
