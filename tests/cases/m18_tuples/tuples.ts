// M18 — tuple types: [A, B] is fixed-length and positional. An array literal in a
// tuple-typed position is checked positionally (contextual typing); a tuple is also
// assignable to the array of the union of its element types.

const t: [number, string] = [1, "x"];    // ok
const swapped: [number, string] = ["x", 1]; // error[TK2322]
// ^ position 0 is string, not number

const a: number = t[0]; // ok — element at index 0
const b: string = t[1]; // ok — element at index 1
const c: string = t[0]; // error[TK2322]
// ^ t[0] is number

const short: [number, string] = [1];        // error[TK2322]
// ^ too few elements
const long: [number, string] = [1, "x", 2]; // error[TK2322]
// ^ too many elements

const arr: (number | string)[] = t; // ok — tuple assignable to the array of its element union
