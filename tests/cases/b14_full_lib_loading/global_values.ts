// tsc 6.0.3 --strict --target es2025: TS2322 x6 and TS2339 below.

console.log("full library", Math.max(1, 2));
const wrongLogResult: number = console.log("void result"); // error[TK2322]: Type 'void' is not assignable to type 'number'

const absolute: number = Math.abs(-2);
const wrongAbsolute: string = Math.abs(-2); // error[TK2322]: Type 'number' is not assignable to type 'string'
Math.notARealMethod(); // error[TK2339]

const timestamp: number = Date.now();
const wrongTimestamp: string = Date.now(); // error[TK2322]: Type 'number' is not assignable to type 'string'

const message: string = new Error("boom").message;
const wrongMessage: number = new Error("boom").message; // error[TK2322]: Type 'string' is not assignable to type 'number'
const counts = new Map<string, number>([["one", 1]]);
const maybeCount: number | undefined = counts.get("one");
const wrongCount: string = counts.get("one"); // error[TK2322]

const names = new Set<string>(["one"]);
const hasName: boolean = names.has("one");
const wrongHasName: string = names.has("one"); // error[TK2322]: Type 'boolean' is not assignable to type 'string'
