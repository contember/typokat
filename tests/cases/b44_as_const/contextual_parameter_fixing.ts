// Only consumed contextual parameter types fix the corresponding outer inference binders.

declare function fixingCallback<T>(value: T, callback: (value: T) => void): T;
const consumedContextResult = fixingCallback("read", (value) => {
  const exactConsumedParameter: "read" = value; // error[TK2322]
});
const exactConsumedContextResult: "read" = consumedContextResult; // error[TK2322]
const omittedContextResult = fixingCallback("read", () => {});
const exactOmittedContextResult: "read" = omittedContextResult;
const annotatedContextResult = fixingCallback("read", (value: "read" | "write") => {});
const exactAnnotatedContextResult: "read" = annotatedContextResult;
const assertedContextResult = fixingCallback("read" as const, (value) => {
  const exactAssertedParameter: "read" = value;
});
const exactAssertedContextResult: "read" = assertedContextResult;

declare function unrelatedFixingCallback<T, U>(value: T, other: U, callback: (other: U) => void): T;
const unrelatedContextResult = unrelatedFixingCallback("read", 1, (other) => {});
const exactUnrelatedContextResult: "read" = unrelatedContextResult;
declare function twoParameterFixingCallback<T, U>(value: T, other: U, callback: (value: T, other: U) => void): U;
const unconsumedSecondResult = twoParameterFixingCallback(1, "read", (value) => {});
const exactUnconsumedSecondResult: "read" = unconsumedSecondResult;

declare function nestedFixingCallback<T>(value: T, callback: { run: (value: T) => void }): T;
const nestedConsumedResult = nestedFixingCallback("read", { run: (value) => {
  const exactNestedConsumedParameter: "read" = value; // error[TK2322]
} });
const exactNestedConsumedResult: "read" = nestedConsumedResult; // error[TK2322]

declare function suffixFixingCallback<T>(callback: (value: T) => void, value: T): T;
const suffixConsumedResult = suffixFixingCallback((value) => {
  const exactSuffixConsumedParameter: "read" = value; // error[TK2322]
}, "read");
const exactSuffixConsumedResult: "read" = suffixConsumedResult; // error[TK2322]

declare function returnOnlyFixingCallback<T>(value: T, callback: () => T): T;
const exactReturnOnlyResult = returnOnlyFixingCallback("read", () => "read" as const);
const exactReturnOnlyCheck: "read" = exactReturnOnlyResult;
const ordinaryReturnOnlyResult = returnOnlyFixingCallback("read", () => "read");
const exactOrdinaryReturnOnlyCheck: "read" = ordinaryReturnOnlyResult; // error[TK2322]
