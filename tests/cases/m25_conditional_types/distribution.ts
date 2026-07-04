// M25 — distributive conditionals: a *naked* type-parameter check type
// distributes over union members (and `never` distributes to `never`); a
// non-naked check type (tuple-wrapped or widened) does not distribute.
// `boolean` expands to `true | false` under distribution (tsc behavior).
// tsc 6.0.3 --strict cross-checked. Union displays are unstable — code-only
// markers on union-typed mismatches.

type ToArray<T> = T extends unknown ? T[] : never;

// string | number distributes to string[] | number[].
type R1 = ToArray<string | number>;
const r1: R1 = ["a"];
const r1b: R1 = [1, 2];
const r1c: R1 = ["a", 1]; // error[TK2322]

// Tuple-wrapping the check type suppresses distribution: (string | number)[].
type ToArrayND<T> = [T] extends [unknown] ? T[] : never;
type R2 = ToArrayND<string | number>;
const r2: R2 = ["a", 1];

// never distributes to never (no members, no branches taken).
type R3 = ToArray<never>;
declare const nv: never;
const r3: R3 = nv;
const r3b: R3 = ["a"]; // error[TK2322]

// The filter pattern (Exclude's core): matching members map to never and drop.
type Excl<T, U> = T extends U ? never : T;
type EX = Excl<"a" | "b" | "c", "a">;
const ex1: EX = "b";
const ex2: EX = "c";
const ex3: EX = "a"; // error[TK2322]

// A union reached through an alias still distributes.
type U = "x" | "y";
type Wrap<T> = T extends "x" ? 1 : 2;
type WU = Wrap<U>;
const wu1: WU = 1;
const wu2: WU = 2;
declare const three: 3;
const wu3: WU = three; // error[TK2322]

// A non-naked check type (T | undefined) does not distribute: the whole union
// fails the extends test and picks the false branch once.
type NotNaked<T> = (T | undefined) extends string ? 1 : 2;
type R5 = NotNaked<"a" | "b">;
const r5: R5 = 2;
const r5b: R5 = 1; // error[TK2322]: Type '1' is not assignable to type '2'

// boolean expands to true | false when distributing.
type BA = ToArray<boolean>;
declare const onlyFalse: false[];
const ba1: BA = onlyFalse;
declare const boolArr: boolean[];
const ba2: BA = boolArr; // error[TK2322]
