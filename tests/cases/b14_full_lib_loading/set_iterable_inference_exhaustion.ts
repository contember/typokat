// tsc 6.0.3 --strict --target es2025: TS2322 x4 and TS2345 x2 below,
// with no excessive-depth error.

declare const b14InferenceStrings: Set<string>;

declare function b14Take<T>(values: Iterable<T>): T[];
const b14SingleTakeClean: string[] = b14Take(b14InferenceStrings);
const b14SingleTakeWrong: number[] = b14Take(b14InferenceStrings); // error[TK2322]
b14Take(b14InferenceStrings).push(1); // error[TK2345]

declare function b14OverloadedTake<T>(values: Iterable<T>): T[];
declare function b14OverloadedTake<T>(values: Iterable<T>): T[];
const b14OverloadedTakeClean: string[] = b14OverloadedTake(b14InferenceStrings);

const b14ArrayFromSetClean: string[] = Array.from(b14InferenceStrings);
const b14ArrayFromSetWrong: number[] = Array.from(b14InferenceStrings); // error[TK2322]

declare const b14NestedStrings: Set<Set<string>>;
declare function b14NestedTake<T>(values: Iterable<Set<T>>): T[];
const b14NestedTakeClean: string[] = b14NestedTake(b14NestedStrings);
const b14NestedTakeWrong: number[] = b14NestedTake(b14NestedStrings); // error[TK2322]

declare function b14IterableFirst<T>(values: Iterable<T>): { tag: "iterable"; item: T };
declare function b14IterableFirst<T>(values: Set<T>): { tag: "set"; item: T };
const b14IterableFirstTag: "iterable" = b14IterableFirst(b14InferenceStrings).tag;

declare function b14SetFirst<T>(values: Set<T>): { tag: "set"; item: T };
declare function b14SetFirst<T>(values: Iterable<T>): { tag: "iterable"; item: T };
const b14SetFirstTag: "set" = b14SetFirst(b14InferenceStrings).tag;

interface B14FiniteBox<T> {
  item: T;
}

declare const b14FiniteStringBox: B14FiniteBox<string>;
b14Take(b14FiniteStringBox); // error[TK2345]
declare function b14FiniteTake<T>(value: B14FiniteBox<T>): T[];
declare function b14FiniteTake<T>(value: B14FiniteBox<T>): T[];
const b14FiniteTakeClean: string[] = b14FiniteTake(b14FiniteStringBox);
const b14FiniteTakeWrong: number[] = b14FiniteTake(b14FiniteStringBox); // error[TK2322]
