// tsc 6.0.3 --strict --target es2025: TS2322 x7, TS2345, and TS2339 below.
// `Array<T>` and `ReadonlyArray<T>` ARE the native array types — the library
// interfaces that declare them only carry their member surface — so an annotation
// naming either must have the same identity as `T[]` / `readonly T[]` in BOTH
// relation directions and in every annotation position. Markers are code-only where
// a side is an array/alias layout, per tests/cases/README.md ("Type display").
export {};

declare const numbers: number[];
declare const strings: string[];
declare const frozenNumbers: readonly number[];

// Variable annotations, both directions.
const fromNative: Array<number> = numbers;
const toNative: number[] = fromNative;
const wrongElement: Array<string> = numbers; // error[TK2322]
const wrongNative: string[] = fromNative; // error[TK2322]

const readonlyFromNative: ReadonlyArray<number> = frozenNumbers;
const readonlyToNative: readonly number[] = readonlyFromNative;
const wrongReadonlyElement: ReadonlyArray<string> = numbers; // error[TK2322]

// A mutable array stays assignable to the readonly form.
const widenedToReadonly: ReadonlyArray<number> = numbers;

// Parameter and return positions.
function identity(values: Array<number>): Array<number> {
  return values;
}
const roundTripped: number[] = identity(numbers);
identity(strings); // error[TK2345]

function readonlyIdentity(values: ReadonlyArray<number>): readonly number[] {
  return values;
}
const readonlyRoundTripped: readonly number[] = readonlyIdentity(frozenNumbers);

// Alias, interface member, and class member positions.
type NumberList = Array<number>;
const aliasAnnotated: NumberList = numbers;
const wrongThroughAlias: string[] = aliasAnnotated; // error[TK2322]

interface Holder {
  values: Array<number>;
  frozen: ReadonlyArray<number>;
}
declare const holder: Holder;
const holderValues: number[] = holder.values;
const holderFrozen: readonly number[] = holder.frozen;
const wrongHolderValues: string[] = holder.values; // error[TK2322]

class Box {
  values: Array<number> = [1, 2];
  frozen: ReadonlyArray<number> = [3];
}
const box = new Box();
const boxValues: number[] = box.values;
const wrongBoxValues: string[] = box.values; // error[TK2322]

// Nesting and generic arguments keep the same identity.
declare const nested: Array<Array<number>>;
const nestedNative: number[][] = nested;
const wrongNested: string[][] = nested; // error[TK2322]

// The member surface still comes from the library interface, and `readonly` still
// withholds the mutating members.
const elementLength: number = fromNative.length;
const mapped: number[] = fromNative.map((value) => value + 1);
readonlyFromNative.push(3); // error[TK2339]
