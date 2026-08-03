// tsc 6.0.3 --strict --target es2025: TS2322 x3 below. This consumer is itself
// an external module; the original RegExp surface and declare-global merge both remain visible.
export {};

const unique: WUUniqueGlobalType = { value: 1 };
const wrongUnique: WUUniqueGlobalType = { value: "wrong" }; // error[TK2322]

const tag: string = /global/.b14Tag();
const wrongTag: number = /global/.b14Tag(); // error[TK2322]: Type 'string' is not assignable to type 'number'
const tested: boolean = /global/.test("global");
const wrongTested: string = /global/.test("global"); // error[TK2322]: Type 'boolean' is not assignable to type 'string'
