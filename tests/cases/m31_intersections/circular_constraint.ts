// M31 — the generic circular-constraint walk (M24) follows intersection members, so
// `T extends T & X` is a circular constraint (tsc TS2313).

function circ<T extends T & { x: number }>(p: T): void {}   // error[TK2313]: circular constraint
