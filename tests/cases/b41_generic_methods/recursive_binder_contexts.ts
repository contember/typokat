// Backlog 41 WU4 P0 regression — recursive generic relation contexts must not
// reuse an in-flight assumption across distinct binder specializations.
// Cross-checked with tsc 6.0.3 --strict.

interface DirectLeft {
  self: DirectLeft;
  map<T>(value: DirectLeft): DirectLeft;
}

interface DirectRight {
  self: DirectRight;
  map<U>(value: DirectRight): DirectRight;
}

declare const directLeft: DirectLeft;
declare const directRight: DirectRight;

const directForward: DirectRight = directLeft;
const directReverse: DirectLeft = directRight;

interface StructuralLeft {
  self: StructuralLeft;
  map<T>(value: { self: StructuralLeft; item: T }): { self: StructuralLeft; item: T };
}

interface StructuralRight {
  self: StructuralRight;
  map<U>(value: { self: StructuralRight; item: U }): { self: StructuralRight; item: U };
}

interface StructuralNumber {
  self: StructuralNumber;
  map<T extends number>(value: { self: StructuralNumber; item: T }): { self: StructuralNumber; item: T };
}

interface StructuralString {
  self: StructuralString;
  map<U extends string>(value: { self: StructuralString; item: U }): { self: StructuralString; item: U };
}

declare const structuralLeft: StructuralLeft;
declare const structuralRight: StructuralRight;
declare const structuralNumber: StructuralNumber;
declare const structuralString: StructuralString;

const structuralMismatchFirst: StructuralNumber = structuralString; // error[TK2322]
const structuralForward: StructuralRight = structuralLeft;
const structuralReverse: StructuralLeft = structuralRight;
const structuralMismatchReverse: StructuralString = structuralNumber; // error[TK2322]

interface CallbackLeft {
  self: CallbackLeft;
  map<T>(callback: <U>(value: { self: CallbackLeft; item: T }) => { self: CallbackLeft; item: U }): { self: CallbackLeft; item: T };
}

interface CallbackRight {
  self: CallbackRight;
  map<S>(callback: <V>(value: { self: CallbackRight; item: S }) => { self: CallbackRight; item: V }): { self: CallbackRight; item: S };
}

interface CallbackNumber {
  self: CallbackNumber;
  map<T extends number>(callback: <U>(value: { self: CallbackNumber; item: T }) => { self: CallbackNumber; item: U }): { self: CallbackNumber; item: T };
}

interface CallbackString {
  self: CallbackString;
  map<S extends string>(callback: <V>(value: { self: CallbackString; item: S }) => { self: CallbackString; item: V }): { self: CallbackString; item: S };
}

declare const callbackLeft: CallbackLeft;
declare const callbackRight: CallbackRight;
declare const callbackNumber: CallbackNumber;
declare const callbackString: CallbackString;

const callbackMismatchFirst: CallbackNumber = callbackString; // error[TK2322]
const callbackForward: CallbackRight = callbackLeft;
const callbackReverse: CallbackLeft = callbackRight;
const callbackMismatchReverse: CallbackString = callbackNumber; // error[TK2322]
