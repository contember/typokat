// Backlog 109 — overload, generic, and fixed-before-rest target selection.
// Cross-checked with tsc 6.0.3 --strict --noEmit.

function select(value?: number): "optional";
function select(value: string): "required";
function select(value?: number | string): "optional" | "required" {
  return typeof value === "string" ? "required" : "optional";
}

const selected: "optional" = select(undefined);
select(true); // error[TK2769]: No overload matches this call

function selectForwarded(value: number | undefined): "optional" {
  return select(value);
}

declare function locked<T>(first: T, second?: T): T;
declare const lockedMaybeNumber: number | undefined;
const lockedLiteral: 1 = locked(1, undefined);
const lockedNumber: number = locked(1, lockedMaybeNumber);

declare function callbackInference<T>(value?: T, callback?: (value: T) => void): void;
callbackInference(lockedMaybeNumber, (value) => {
  const callbackNumber: number = value;
  const callbackUndefined: undefined = value; // error[TK2322]: Type 'number' is not assignable to type 'undefined'
});

function explicitOptional<T>(value?: T): T | undefined {
  return value;
}

const explicitResult: number | undefined = explicitOptional<number>(undefined);

function inferredOptional<T extends number>(seed: T[], value?: T): T[] {
  return seed;
}

const inferredResult: number[] = inferredOptional([1], undefined);

class GenericOptionalMethod {
  method<T extends number>(seed: T[], value?: T): T[] {
    return seed;
  }
}

const genericMethodResult: number[] = new GenericOptionalMethod().method([1], undefined);

function optionalBeforeRest(prefix: string, value?: number, ...tail: boolean[]): void {}
optionalBeforeRest("prefix", undefined, true);
optionalBeforeRest("prefix", (undefined), true, false);
optionalBeforeRest(); // error[TK2555]: Expected at least 1 arguments, but got 0
optionalBeforeRest("prefix", 1, "wrong"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'boolean'

function defaultBeforeRest(prefix: string, value: number = 1, ...tail: boolean[]): void {}
defaultBeforeRest("prefix", undefined, true);

declare function unionRestAlternative(value?: number, ...tail: [] | [boolean]): void;
unionRestAlternative(undefined);
unionRestAlternative(undefined, true);

// Fallback candidates are retained only when a binder has no non-fallback source.
declare function sole<T>(value?: T): T;
const soleUndefined: undefined = sole(undefined);

const lockedReverseValue = locked(undefined, 1);
const lockedReverseUnion: 1 | undefined = lockedReverseValue;
const lockedReverseOne: 1 = lockedReverseValue; // error[TK2322]
const lockedReverseUndefined: undefined = lockedReverseValue; // error[TK2322]
const lockedStringLiteral: "locked" = locked("locked", undefined);
const lockedBooleanLiteral: true = locked(true, undefined);

declare function split<T, U>(first: T, second?: U): [T, U];
const splitFallback: [number, undefined] = split(1, undefined);

declare function unionOptionalTarget<T>(first: T, second?: T | number): T;
const unionTargetLiteral: 1 = unionOptionalTarget(1, undefined);

declare function callbackWinner<T>(value?: T, callback?: (value: T) => void): T;
const callbackNumberWinner: number = callbackWinner(undefined, (value: number): void => {});
const callbackFallbackWinner: undefined = callbackWinner(undefined, (value): void => {
  const callbackFallbackValue: undefined = value;
});

interface GenericLockedConstructor {
  new <T>(first: T, second?: T): { value: T };
}
declare const genericLockedConstructor: GenericLockedConstructor;
const genericLockedResult: { value: 1 } = new genericLockedConstructor(1, undefined);
