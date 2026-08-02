// tsc 6.0.3 --strict --target es2025: TS2349 x2; every other line is clean.

declare const directObject:
  | (() => string)
  | { value: number };
directObject(); // error[TK2349]

declare const memberObject:
  | { method(): string }
  | { method: { value: number } };
memberObject.method(); // error[TK2349]

type WideOverloads = {
  (value: string): "wide";
  (value: number): "number";
};
type NarrowOverloads = {
  (value: "foo"): "literal";
  (value: boolean): "boolean";
};
declare const partialOverload: WideOverloads | NarrowOverloads;
const partialResult: "wide" | "literal" = partialOverload("foo");

type AC = { a: 1; c: 1 };
type BC = { b: 1; c: 1 };
declare const mixedFixedRest:
  | ((first: { a: 1 }, second: { b: 1 }) => "fixed")
  | ((...values: { c: 1 }[]) => "rest");
declare const first: AC;
declare const second: BC;
const mixedResult: "fixed" | "rest" = mixedFixedRest(first, second);

type UnionRestArgs = [value: number] | [value: number, text: string];
declare const unionTupleRest:
  | ((...args: UnionRestArgs) => "left")
  | ((...args: UnionRestArgs) => "right");
const oneArgument: "left" | "right" = unionTupleRest(1);
const twoArguments: "left" | "right" = unionTupleRest(1, "text");
