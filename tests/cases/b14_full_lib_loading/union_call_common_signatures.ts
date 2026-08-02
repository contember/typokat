// tsc 6.0.3 --strict --target es2025: common union calls preserve every constituent.

declare const directCallable: (() => string) | (() => number);
const directBad: boolean = directCallable(); // error[TK2322]

declare const mixedCallable: (() => string) | number;
mixedCallable(); // error[TK2349]

declare const constrainedCallable:
  | (<T extends string>(value: T) => T)
  | (<U extends number>(value: U) => U);
constrainedCallable("value"); // error[TK2349]

declare const mixedRestMethods:
  | { method(): string }
  | { method(...values: number[]): number };
const mixedRestStringArm: number = mixedRestMethods.method(); // error[TK2322]
const mixedRestNumberArm: string = mixedRestMethods.method(1); // error[TK2322]

declare const genericDefaultMethods:
  | { method<T = string>(): T }
  | { method<U = number>(): U };
const genericDefaultResult = genericDefaultMethods.method();
const genericDefaultBad: boolean = genericDefaultResult; // error[TK2322]

type FirstSameShapeOverloads = {
  method(): "first";
  method(): "second";
};
type SecondSameShapeOverloads = {
  method(): "third";
};
declare const sameShapeOverloads: FirstSameShapeOverloads | SecondSameShapeOverloads;
const firstOverloadWins: "first" | "third" = sameShapeOverloads.method();
const secondOverloadDoesNotWin: "second" = sameShapeOverloads.method(); // error[TK2322]
