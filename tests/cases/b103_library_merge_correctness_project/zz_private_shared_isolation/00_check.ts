export {};

[1, 2].b103Ordered; // error[TK2339]
[1, 2].fullLibBenchFirst; // error[TK2339]
/isolated/.b103Tag; // error[TK2339]
const missingUmd = B103Umd; // error[TK2304]
const missingGlobal = B103GlobalThisValue; // error[TK2304]
globalThis.B103GlobalThisValue; // error[TK2339]

const mapped: number[] = [1, 2].map((value) => value + 1);
const tested: boolean = /isolated/.test("isolated");
