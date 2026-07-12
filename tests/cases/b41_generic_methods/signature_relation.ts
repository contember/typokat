// Backlog 41 — generic-signature relation, including alpha-equivalent binders
// and incompatible parameter/constraint shapes. Cross-checked with tsc 6.0.3
// --strict.

interface GenericMethodSource {
  map<T>(value: T): T;
}

interface GenericMethodTarget {
  map<U>(value: U): U;
}

declare const genericMethodSource: GenericMethodSource;
declare const genericMethodTarget: GenericMethodTarget;

const alphaMethodForward: GenericMethodTarget = genericMethodSource;
const alphaMethodReverse: GenericMethodSource = genericMethodTarget;

interface GenericCallSource {
  <T>(value: T): T;
}

interface GenericCallTarget {
  <U>(value: U): U;
}

declare const genericCallSource: GenericCallSource;
declare const genericCallTarget: GenericCallTarget;

const alphaCallForward: GenericCallTarget = genericCallSource;
const alphaCallReverse: GenericCallSource = genericCallTarget;

interface GenericConstructSource {
  new <T>(value: T): { value: T };
}

interface GenericConstructTarget {
  new <U>(value: U): { value: U };
}

declare const genericConstructSource: GenericConstructSource;
declare const genericConstructTarget: GenericConstructTarget;

const alphaConstructForward: GenericConstructTarget = genericConstructSource;
const alphaConstructReverse: GenericConstructSource = genericConstructTarget;

interface OneParameter {
  map<T>(value: T): T;
}

interface TwoRequiredParameters {
  map<T, U>(value: T, extra: U): T;
}

declare const twoRequiredParameters: TwoRequiredParameters;
const badParameterArity: OneParameter = twoRequiredParameters; // error[TK2322]

interface NumberConstrained {
  map<T extends number>(value: T): T;
}

interface StringConstrained {
  map<T extends string>(value: T): T;
}

declare const stringConstrained: StringConstrained;
const badConstraint: NumberConstrained = stringConstrained; // error[TK2322]

declare const genericIdentity: GenericCallSource;
const specificIdentity: (value: string) => string = genericIdentity;

declare const specificString: (value: string) => string;
const genericFromSpecific: GenericCallSource = specificString; // error[TK2322]
