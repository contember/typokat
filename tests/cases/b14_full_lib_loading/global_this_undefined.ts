// tsc 6.0.3 --strict --target es2025: TS2322 x5 below. Both lexical shadows are clean.

const globalAbsolute: number = globalThis.Math.abs(-1);
const wrongGlobalAbsolute: string = globalThis.Math.abs(-1); // error[TK2322]: Type 'number' is not assignable to type 'string'

const globalUndefined: undefined = globalThis.undefined;
const wrongGlobalUndefined: string = globalThis.undefined; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
const directUndefined: undefined = undefined;
const wrongUndefined: string = undefined; // error[TK2322]: Type 'undefined' is not assignable to type 'string'

function b14ShadowUndefined(): string {
  const undefined = "local";
  return undefined;
}

const wrongShadow: number = b14ShadowUndefined(); // error[TK2322]: Type 'string' is not assignable to type 'number'

function b14ShadowGlobalThis(): number {
  const globalThis = { local: 1 };
  return globalThis.local;
}

const wrongGlobalThisShadow: string = b14ShadowGlobalThis(); // error[TK2322]: Type 'number' is not assignable to type 'string'
