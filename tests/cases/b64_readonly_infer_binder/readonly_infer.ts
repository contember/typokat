type ReadonlyArrayElem<T> = T extends readonly (infer U)[] ? U : never;
type RArray = ReadonlyArrayElem<readonly string[]>;

const arrayBad: RArray = 5; // error[TK2322]: Type 'number' is not assignable to type 'string'
const arrayOk: RArray = "ok";

type ReadonlyTupleHead<T> = T extends readonly [infer Head, infer Tail] ? Head : never;
type RHead = ReadonlyTupleHead<readonly [number, string]>;

const tupleBad: RHead = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'
const tupleOk: RHead = 1;

type ReadonlyNested<T> = T extends readonly (readonly (infer U)[])[] ? U : never;
type RNested = ReadonlyNested<readonly (readonly boolean[])[]>;

const nestedBad: RNested = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'boolean'
const nestedOk: RNested = true;

type MutableArrayElem<T> = T extends (infer U)[] ? U : never;
type MArray = MutableArrayElem<string[]>;

const mutableBad: MArray = 5; // error[TK2322]: Type 'number' is not assignable to type 'string'
const mutableOk: MArray = "ok";
