// M17 — array types: `T[]` and `Array<T>`, array literals, element access, length,
// covariant assignability. (Array methods like push/map need lib.d.ts — deferred;
// only `length` is synthesized for now.)

const nums: number[] = [1, 2, 3]; // ok — element type inferred number
const bad: number[] = [1, "x"];   // error[TK2322]
// ^ literal infers (number | string)[], not assignable to number[]

const n: number = nums[0];       // ok — element access
const s: string = nums[0];       // error[TK2322]
const len: number = nums.length; // ok — synthesized length

let a: number[];
let b: string[];
a = b; // error[TK2322]
// ^ string[] is not assignable to number[]

const empty: number[] = []; // ok — empty array literal

const arr: Array<number> = [1, 2]; // ok — Array<T> syntax
const arrBad: Array<string> = [1]; // error[TK2322]

const nested: number[][] = [[1], [2]]; // ok
const nestedBad: number[][] = [["x"]]; // error[TK2322]
