// tsc 6.0.3 --strict --noEmit: TS2428 x2, TS2314, and four downstream TS2322
// recovery witnesses.
interface RenamedBinderRecovery<T> { // error[TK2428]: All declarations of 'RenamedBinderRecovery' must have identical type parameters
  a: T;
}
interface RenamedBinderRecovery<U> { // error[TK2428]: All declarations of 'RenamedBinderRecovery' must have identical type parameters
  b: U;
}

type MissingRecoveredArgument = RenamedBinderRecovery<number>; // error[TK2314]: Generic type 'RenamedBinderRecovery<T, U>' requires 2 type argument(s)

declare const renamedBinderRecovery: RenamedBinderRecovery<number, string>;
const recoveredA: number = renamedBinderRecovery.a;
const recoveredB: string = renamedBinderRecovery.b;
const recoveredAWrong: string = renamedBinderRecovery.a; // error[TK2322]: Type 'number' is not assignable to type 'string'
const recoveredBWrong: number = renamedBinderRecovery.b; // error[TK2322]: Type 'string' is not assignable to type 'number'

// A later declaration may legally extend the arity when every added parameter has a default.
interface DefaultedArityRecovery<T> {
  first: T;
}
interface DefaultedArityRecovery<T, V = string> {
  second: V;
}

declare const defaultedArityRecovery: DefaultedArityRecovery<number>;
const defaultedFirst: number = defaultedArityRecovery.first;
const defaultedSecond: string = defaultedArityRecovery.second;
const defaultedSecondWrong: number = defaultedArityRecovery.second; // error[TK2322]: Type 'string' is not assignable to type 'number'

interface ReverseDefaultedArityRecovery<T, V = string> {
  second: V;
}
interface ReverseDefaultedArityRecovery<T> {
  first: T;
}

declare const reverseDefaultedArityRecovery: ReverseDefaultedArityRecovery<number>;
const reverseDefaultedFirst: number = reverseDefaultedArityRecovery.first;
const reverseDefaultedSecond: string = reverseDefaultedArityRecovery.second;
const reverseDefaultedSecondWrong: number = reverseDefaultedArityRecovery.second; // error[TK2322]: Type 'string' is not assignable to type 'number'
