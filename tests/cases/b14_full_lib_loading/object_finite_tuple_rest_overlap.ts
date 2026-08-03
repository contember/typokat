// tsc 6.0.3 --strict --target es2025: TS2322 x4 below.

interface Object {
  b14FiniteRestElement?: [number];
}

interface Array<T> {
  b14FiniteRestElement?: T;
}

interface ReadonlyArray<T> {
  b14FiniteRestElement?: T;
}

type B14MutableFiniteRest = [...[number]];
type B14ReadonlyFiniteRest = readonly [...[number]];
type B14MutableVariadicRest = [...number[]];
type B14ReadonlyVariadicRest = readonly [...number[]];

declare const b14MutableFiniteRest: B14MutableFiniteRest;
declare const b14ReadonlyFiniteRest: B14ReadonlyFiniteRest;
declare const b14MutableVariadicRest: B14MutableVariadicRest;
declare const b14ReadonlyVariadicRest: B14ReadonlyVariadicRest;

const b14ObjectFromMutableFiniteRest: Object = b14MutableFiniteRest; // error[TK2322]
const b14ObjectFromReadonlyFiniteRest: Object = b14ReadonlyFiniteRest; // error[TK2322]

const b14MutableFiniteRestExactLength: { length: 1 } = b14MutableFiniteRest;
const b14ReadonlyFiniteRestExactLength: { readonly length: 1 } = b14ReadonlyFiniteRest;
const b14MutableVariadicRestExactLength: { length: 1 } = b14MutableVariadicRest; // error[TK2322]
const b14ReadonlyVariadicRestExactLength: { readonly length: 1 } = b14ReadonlyVariadicRest; // error[TK2322]
