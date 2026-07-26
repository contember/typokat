// Backlog 45 — the operand rule reads the NARROWED type from the flow environment,
// so the same expression is accepted in one branch and rejected in the other.
// tsc 6.0.3 --strict --target es2025: TS2322 x3, TS2362 x1.

declare const value: string | number;

if (typeof value === "number") {
  const doubled: string = value * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
} else {
  const doubled: string = value * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
}

function widened(input: string | number): string {
  if (typeof input !== "number") {
    return input;
  }
  return input * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
}
