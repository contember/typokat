// tsc 6.0.3 --strict --target es2025: TS2322 x3 and TS2339 x3 below. Script var/function/
// namespace declarations contribute globalThis properties; let/const/class stay lexical globals.
const uniqueCount: number = globalThis.B14UniqueGlobal.count;
const wrongGlobalUniqueCount: string = globalThis.B14UniqueGlobal.count; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongUniqueCount: string = B14UniqueGlobal.count; // error[TK2322]: Type 'number' is not assignable to type 'string'

const functionCount: number = globalThis.B14GlobalFunction();
const wrongGlobalFunctionCount: string = globalThis.B14GlobalFunction(); // error[TK2322]: Type 'number' is not assignable to type 'string'
globalThis.B14GlobalLet; // error[TK2339]
globalThis.B14GlobalConst; // error[TK2339]
globalThis.B14GlobalClass; // error[TK2339]

const lexicalLet: number = B14GlobalLet;
const lexicalConst: number = B14GlobalConst;
const lexicalClassValue: number = new B14GlobalClass().value;
