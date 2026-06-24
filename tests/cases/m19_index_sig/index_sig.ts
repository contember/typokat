// M19 — index signatures: { [k: string]: T } / { [i: number]: T }. Any key of the
// index kind yields the value type; an object literal is assignable when every
// property value is assignable to the index value type.

const dict: { [k: string]: number } = { a: 1, b: 2 }; // ok — all values number
const bad: { [k: string]: number } = { a: 1, b: "x" }; // error[TK2322]
// ^ value "x" is not assignable to the index value type number

const v: number = dict["a"];  // ok — index access yields the value type
const v2: number = dict.a;    // ok — property access goes through the index signature
const v3: string = dict["a"]; // error[TK2322]

let key: string = "k";
const v4: number = dict[key]; // ok — dynamic string key

const empty: { [k: string]: number } = {}; // ok — empty object

const nums: { [i: number]: string } = { 0: "a", 1: "b" }; // ok — number index signature
const s: string = nums[0];    // ok
const sBad: number = nums[0]; // error[TK2322]
