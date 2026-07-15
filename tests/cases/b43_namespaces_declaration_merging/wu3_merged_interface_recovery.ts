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

// Dependent defaults retain the raw earlier-binder reference. Explicit arguments
// substitute sequentially at application time for both interfaces and aliases.
interface DependentInterfaceDefault<T = string, U = T> { u: U }
declare const dependentInterfaceDefault: DependentInterfaceDefault<number>;
const dependentInterfaceNumber: number = dependentInterfaceDefault.u;
const dependentInterfaceString: string = dependentInterfaceDefault.u; // error[TK2322]: Type 'number' is not assignable to type 'string'

type DependentAliasDefault<T = string, U = T> = { u: U };
declare const dependentAliasDefault: DependentAliasDefault<number>;
const dependentAliasNumber: number = dependentAliasDefault.u;
const dependentAliasString: string = dependentAliasDefault.u; // error[TK2322]: Type 'number' is not assignable to type 'string'

// Merged header identity compares raw binder references, not eagerly substituted
// default values.
interface IdenticalDependentHeader<T = string, U = T> { first: U }
interface IdenticalDependentHeader<T = string, U = T> { second: U }

interface MismatchedDependentHeader<T = string, U = T> {} // error[TK2428]: All declarations of 'MismatchedDependentHeader' must have identical type parameters
interface MismatchedDependentHeader<T = string, U = string> {} // error[TK2428]: All declarations of 'MismatchedDependentHeader' must have identical type parameters

// Declaration-time validation uses the raw constraint/default pair.
type InvalidDependentConstraint<T = string, U extends T = string> = U; // error[TK2344]: Type 'string' does not satisfy the constraint 'T'
type ValidDependentConstraint<T = string, U extends T = T> = U;

// A default may reference only a strictly earlier type parameter.
type SelfAliasDefault<T = T> = T; // error[TK2744]: Type parameter defaults can only reference previously declared type parameters
interface SelfInterfaceDefault<T = T> {} // error[TK2744]: Type parameter defaults can only reference previously declared type parameters

type RequiredAfterOptionalAlias<T = string, U> = [T, U]; // error[TK2706]: Required type parameters may not follow optional type parameters
interface RequiredAfterOptionalInterface<T = string, U> {} // error[TK2706]: Required type parameters may not follow optional type parameters

// Every reopening header owns descriptor validation even when its one-sided
// metadata is merge-compatible and therefore does not produce TK2428.
interface LaterSelfDefault<T> {}
interface LaterSelfDefault<T = T> {} // error[TK2744]: Type parameter defaults can only reference previously declared type parameters

interface LaterRequiredAfterOptional<T, U> {}
interface LaterRequiredAfterOptional<T = string, U> {} // error[TK2706]: Required type parameters may not follow optional type parameters

interface LaterInvalidConstraintDefault<T extends string> {}
interface LaterInvalidConstraintDefault<T extends string = number> {} // error[TK2344]: Type 'number' does not satisfy the constraint 'string'

interface LaterCircularConstraint<T> {}
interface LaterCircularConstraint<T extends T> {} // error[TK2313]: Type parameter 'T' has a circular constraint
