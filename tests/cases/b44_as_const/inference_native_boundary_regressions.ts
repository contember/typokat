declare function expectedResultBoundary<T>(value: T, callback: (value: T) => void): T;
const expectedBroadResult: number = expectedResultBoundary(1 as const, value => {
  value = 2; // error[TK2322]
});
const expectedBroadRead: number = expectedResultBoundary(1 as const, value => {
  const exactParameter: 1 = value;
});

declare function literalUnionReturn<T extends "read" | "write">(callback: () => T): T;
const literalUnionResult = literalUnionReturn(() => "read");
const exactLiteralUnionResult: "read" = literalUnionResult;
let literalUnionCopy = literalUnionResult;
literalUnionCopy = "write"; // error[TK2322]

declare function numericUnionReturn<T extends 1 | 2>(callback: () => T): T;
const numericUnionResult = numericUnionReturn(() => 1);
const exactNumericUnionResult: 1 = numericUnionResult;
let numericUnionCopy = numericUnionResult;
numericUnionCopy = 2; // error[TK2322]

declare const upperBoundaryFlag: boolean;
declare function equalUpperBoundary<T>(value: T, callback: (value: T) => void): T;
let equalUpperResult = equalUpperBoundary("read", upperBoundaryFlag ? value => {} : (value: "read") => {});
const exactEqualUpperResult: "read" = equalUpperResult; // error[TK2322]
equalUpperResult = "write";
let reversedUpperResult = equalUpperBoundary("read", upperBoundaryFlag ? (value: "read") => {} : value => {});
const exactReversedUpperResult: "read" = reversedUpperResult; // error[TK2322]
reversedUpperResult = "write";

let regularUpperResult = equalUpperBoundary("read" as const, upperBoundaryFlag ? value => {} : (value: "read") => {});
const exactRegularUpperResult: "read" = regularUpperResult;
regularUpperResult = "write"; // error[TK2322]
