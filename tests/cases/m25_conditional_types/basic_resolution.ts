// M25 — conditional types resolve when the check type is concrete: the extends
// test runs through the relation engine, then the matching branch (with the
// check type's inferences substituted) is the result. tsc 6.0.3 --strict
// cross-checked.

type IsString<T> = T extends string ? true : false;

const a: IsString<string> = true;
const b: IsString<number> = false;
const c: IsString<string> = false; // error[TK2322]: Type 'false' is not assignable to type 'true'
const d: IsString<number> = true; // error[TK2322]: Type 'true' is not assignable to type 'false'

// A literal check type extends its primitive.
type E1 = "hi" extends string ? 1 : 2;
const e1: E1 = 1;
const e1b: E1 = 2; // error[TK2322]: Type '2' is not assignable to type '1'

// The extends test is structural.
type HasX = { x: number };
type T1 = { x: number; y: string } extends HasX ? "yes" : "no";
const t1: T1 = "yes";
const t1b: T1 = "no"; // error[TK2322]

// Nested conditionals resolve innermost-out.
type Classify<T> = T extends string ? "str" : T extends number ? "num" : "other";
const n1: Classify<string> = "str";
const n2: Classify<number> = "num";
const n3: Classify<boolean> = "other";
const n4: Classify<number> = "str"; // error[TK2322]: Type '"str"' is not assignable to type '"num"'

// The check type substitutes into the selected branch.
type Same<T> = T extends string ? T : never;
const s1: Same<"a"> = "a";
const s2: Same<"a"> = "b"; // error[TK2322]: Type '"b"' is not assignable to type '"a"'

// A conditional result feeds an ordinary relation like any other type.
function takesNum(n: E1): void {}
takesNum(1);
takesNum(2); // error[TK2345]: Argument of type '2' is not assignable to parameter of type '1'
