// Backlog 109 — optional/defaulted fixed parameters admit explicit undefined.
// Cross-checked with tsc 6.0.3 --strict --noEmit.

declare const maybeNumber: number | undefined;
declare const maybeWrong: string | undefined;

function optionalNumber(value?: number): void {
  const optionalBody: number | undefined = value;
}

optionalNumber(undefined);
optionalNumber((undefined));
optionalNumber(maybeNumber);
function forwardOptional(value: number | undefined): void {
  optionalNumber(value);
}
function forwardOpaqueOptional<U extends number | undefined>(value: U): void {
  optionalNumber(value);
}
function rejectOpaqueOptional<U extends string | undefined>(value: U): void {
  optionalNumber(value); // error[TK2345]
}
optionalNumber(null); // error[TK2345]
optionalNumber("wrong"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
optionalNumber(maybeWrong); // error[TK2345]
optionalNumber(1, 2); // error[TK2554]: Expected 0-1 arguments, but got 2

function defaultedNumber(value: number = 1): void {
  const defaultedBody: number = value;
  const undefinedBody: undefined = value; // error[TK2322]: Type 'number' is not assignable to type 'undefined'
}

defaultedNumber(undefined);
defaultedNumber(maybeNumber);

function requiredNumber(value: number): void {}
requiredNumber(undefined); // error[TK2345]: Argument of type 'undefined' is not assignable to parameter of type 'number'

function optionalNever(value?: never): void {}
optionalNever(undefined);
function requiredNever(value: never): void {}
requiredNever(undefined); // error[TK2345]: Argument of type 'undefined' is not assignable to parameter of type 'never'

function optionalBoundary(value?: number): void {}
declare const anyValue: any;
declare const unknownValue: unknown;
declare const neverValue: never;
optionalBoundary(anyValue);
optionalBoundary(unknownValue); // error[TK2345]
optionalBoundary(neverValue);
optionalBoundary(MissingValue); // error[TK2304]: Cannot find name 'MissingValue'

function defaultBoundary(value: number = 1): void {}
declare const invalidDefaultUnion: string | undefined;
defaultBoundary(invalidDefaultUnion); // error[TK2345]
