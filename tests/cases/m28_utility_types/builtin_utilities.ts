// M28 — the standard utility types are available as BUILT-IN aliases (a
// prelude compilation unit; no lib.d.ts needed): Partial, Required,
// Readonly, Record, Pick, Omit, Exclude, Extract, NonNullable, ReturnType.
// Each is the ordinary mapped/conditional definition evaluated by the
// M25–M27 machinery. tsc 6.0.3 --strict cross-checked.

type P = { a: number; b?: string };

const p1: Partial<P> = {};
const p2: Partial<P> = { a: 1 };
const p3: Partial<P> = { a: "s" }; // error[TK2322]

const r1: Required<P> = { a: 1, b: "s" };
const r2: Required<P> = { a: 1 }; // error[TK2741]: 'b' is missing

declare const ro: Readonly<P>;
const ro1: number = ro.a;
function wr(): void {
  ro.a = 2; // error[TK2540]: read-only
}

const rc1: Record<"x" | "y", number> = { x: 1, y: 2 };
const rc2: Record<"x" | "y", number> = { x: 1 }; // error[TK2741]: 'y' is missing

const pk1: Pick<P, "a"> = { a: 1 };
const pk2: Pick<P, "a"> = { a: 1, b: "s" }; // error[TK2353]
declare const pk3: Pick<P, "q">; // error[TK2344]: does not satisfy the constraint

const ex1: Exclude<"a" | "b" | "c", "a"> = "b";
const ex2: Exclude<"a" | "b" | "c", "a"> = "a"; // error[TK2322]

const et1: Extract<"a" | "b", "a" | "c"> = "a";
const et2: Extract<"a" | "b", "a" | "c"> = "b"; // error[TK2322]

const nn1: NonNullable<string | null | undefined> = "x";
declare const nul: null;
const nn2: NonNullable<string | null> = nul; // error[TK2322]

const rt1: ReturnType<() => number> = 1;
const rt2: ReturnType<() => number> = "s"; // error[TK2322]: Type 'string' is not assignable to type 'number'

const om1: Omit<P, "b"> = { a: 1 };
const om2: Omit<P, "b"> = { a: 1, b: "s" }; // error[TK2353]
const om3: Omit<P, "a"> = { a: 1 }; // error[TK2353]
