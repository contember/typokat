// WU5 - recursive generic application heads retain their argument provenance.
// Cross-checked with tsc 6.0.3 --strict: TS2322 x2 and TS2345 x2 below.

interface RecursiveProtocol<T> {
  advance(): RecursiveProtocol<T>;
}

declare const stringProtocol: RecursiveProtocol<string>;
declare const nestedStringProtocol: RecursiveProtocol<RecursiveProtocol<string>>;

declare function takeProtocol<T>(value: RecursiveProtocol<T>): T;
const directClean: string = takeProtocol(stringProtocol);
const directWrong: number = takeProtocol(stringProtocol); // error[TK2322]: Type 'string' is not assignable to type 'number'

declare function takeNestedProtocol<T>(value: RecursiveProtocol<RecursiveProtocol<T>>): T;
const nestedClean: string = takeNestedProtocol(nestedStringProtocol);
const nestedWrong: number = takeNestedProtocol(nestedStringProtocol); // error[TK2322]: Type 'string' is not assignable to type 'number'

declare function requireNumber(value: number): void;
requireNumber(takeProtocol(stringProtocol)); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

interface AlternateProtocol<T> {
  advance(): AlternateProtocol<T>;
}

declare function recursiveFirst<T>(value: RecursiveProtocol<T>): { tag: "recursive"; item: T };
declare function recursiveFirst<T>(value: AlternateProtocol<T>): { tag: "alternate"; item: T };
const recursiveFirstTag: "recursive" = recursiveFirst(stringProtocol).tag;
const recursiveFirstItem: string = recursiveFirst(stringProtocol).item;

declare function alternateFirst<T>(value: AlternateProtocol<T>): { tag: "alternate"; item: T };
declare function alternateFirst<T>(value: RecursiveProtocol<T>): { tag: "recursive"; item: T };
const alternateFirstTag: "alternate" = alternateFirst(stringProtocol).tag;

interface FiniteProtocol<T> {
  current(): T;
}

declare const finiteStringProtocol: FiniteProtocol<string>;
takeProtocol(finiteStringProtocol); // error[TK2345]
