// tsc 6.0.3 --strict --target es2025: TS2322 x2 below.
const rootValue: number = B14RootNamespace.value;
const rootGlobalThisValue: number = globalThis.B14RootNamespace.value;
const wrongRootGlobalThisValue: string = globalThis.B14RootNamespace.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongRootValue: string = B14RootNamespace.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
