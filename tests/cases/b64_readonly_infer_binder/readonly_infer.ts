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

type MutableElem<T> = T extends (infer U)[] ? U : never;
type FromReadonlyArray = MutableElem<readonly string[]>;
const arrayShouldError: FromReadonlyArray = "x"; // error[TK2322]

type MutableHead<T> = T extends [infer H, infer Tail] ? H : never;
type FromReadonlyTuple = MutableHead<readonly [number, string]>;
const tupleShouldError: FromReadonlyTuple = 1; // error[TK2322]

type Leak = ReadonlyArrayElem<{ __typokat_readonly_array_element: number }>;
const shouldError: Leak = 1; // error[TK2322]

declare const roStrings: readonly string[];
const elementShouldError: number = roStrings[0]; // error[TK2322]

declare const roPair: readonly [string, number];
const tupleElementShouldError: number = roPair[0]; // error[TK2322]

type First = (readonly [string, number])[0];
const firstShouldError: First = 1; // error[TK2322]

type Second = (readonly [string, number])[1];
const secondShouldError: Second = "x"; // error[TK2322]
