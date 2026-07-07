// backlog 55 regression net: after a budget-exhausting root evaluation, later
// evaluations of the OTHER node kinds (conditional, instantiation, mapped,
// eager keyof) must still compute fresh, correct results. These pass at HEAD
// (only the template path leaks); pinned so the fix cannot regress them.
type Deep<T> = T extends {} ? Deep<{ v: T }> : never;
const boom: Deep<{}> = 0; // error[TK2589]
type CondPick<T> = T extends string ? "yes" : "no";
const c1: CondPick<string> = "no"; // error[TK2322]: Type '"no"' is not assignable to type '"yes"'
type Box<T> = { v: T };
const b1: Box<number> = { v: "s" }; // error[TK2322]
type MapId<T> = { [K in keyof T]: T[K] };
const m1: MapId<{ a: number }> = { a: "s" }; // error[TK2322]
const k1: keyof { a: number } = "b"; // error[TK2322]: Type '"b"' is not assignable to type '"a"'
