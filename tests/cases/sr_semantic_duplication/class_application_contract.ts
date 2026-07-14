// Semantic-duplication architecture gate — complete generic class-application contract.
// tsc 6.0.3 --strict reports TS2314/TS2707 for type-reference arity, TS2558 for explicit `new`
// arity, TS2304 for every unresolved type name, and TS2322 for partial-publication observability
// controls. It accepts unresolved constructor inference.
// typokat additionally records its deliberate unsupported-default and unresolved-inference outcomes.

class ExactApplication<A, B> {
  first!: A;
  second!: B;
}

type ExactBare = ExactApplication; // error[TK2314]: Generic type 'ExactApplication<A, B>' requires 2 type argument(s)
type ExactFew = ExactApplication<string>; // error[TK2314]
type ExactMany = ExactApplication<string, number, boolean>; // error[TK2314]

// Invalid arity remains the application cause, but every explicit child is still lowered.
type InvalidArityWithUnavailableExtra = ExactApplication<string, number, MissingExtraArgument>; // error[TK2314] | error[TK2304]: Cannot find name 'MissingExtraArgument'
const invalidNewWithUnavailableExtra = new ExactApplication<string, number, MissingNewExtra>(); // error[TK2558] | error[TK2304]: Cannot find name 'MissingNewExtra'

// Valid arity with one unavailable child emits only that child's name error and creates no instance.
type ValidArityWithUnavailableArgument = ExactApplication<string, MissingValidArgument>; // error[TK2304]: Cannot find name 'MissingValidArgument'
const validNewWithUnavailableArgument = new ExactApplication<string, MissingValidNewArgument>(); // error[TK2304]: Cannot find name 'MissingValidNewArgument'
const laterValidUnavailableNewDemand: number = validNewWithUnavailableArgument.first;

const exactNewFew = new ExactApplication<string>(); // error[TK2558]
const exactNewMany = new ExactApplication<string, number, boolean>(); // error[TK2558]
const exactNewReady = new ExactApplication<string, number>();

class RangedApplication<A, B = string> { // incomplete[annotation-lower/type-parameter-default/self]: type-parameter default not lowered
  first!: A;
  second!: B;
}

type RangedBare = RangedApplication; // error[TK2707]: Generic type 'RangedApplication<A, B>' requires between 1 and 2 type arguments
type RangedFewExhausted = RangedApplication<number>; // incomplete[annotation-lower/type-reference/class-default-argument]: class type-parameter default unavailable at application
type RangedExactReady = RangedApplication<number, boolean>;
type RangedMany = RangedApplication<number, string, boolean>; // error[TK2707]

// Later demands of the exhausted application do not replay either default event.
declare const rangedFewExhausted: RangedFewExhausted;
const rangedFewNoReplay = rangedFewExhausted.second;

class UnsupportedDefaultApplication<T = MissingDefaultType> { // incomplete[annotation-lower/type-parameter-default/self]: type-parameter default not lowered
  value!: T;
}

// The declaration owns its unsupported default once; each use that needs it owns one use record.
type UnsupportedDefaultTypeUse = UnsupportedDefaultApplication; // incomplete[annotation-lower/type-reference/class-default-argument]: class type-parameter default unavailable at application
const unsupportedDefaultNewUse = new UnsupportedDefaultApplication(); // incomplete[expr-infer/new-expression/class-default-argument]: class type-parameter default unavailable at application

// An explicit complete vector does not consult the unsupported declaration default.
type UnsupportedDefaultExplicitUse = UnsupportedDefaultApplication<number>;
const unsupportedDefaultExplicitNew = new UnsupportedDefaultApplication<number>();
declare const unsupportedDefaultExplicitUse: UnsupportedDefaultExplicitUse;
const unsupportedDefaultExplicitGood: number = unsupportedDefaultExplicitUse.value;
const unsupportedDefaultExplicitBad: string = unsupportedDefaultExplicitUse.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

// Demanding a failed application later never replays its origin record.
const laterUnsupportedDefaultDemand = unsupportedDefaultNewUse.value;
const laterInvalidNewDemand: number = invalidNewWithUnavailableExtra.first;

class InferenceUnavailableApplication<T> {
  value!: T;
}

// With no explicit argument, constructor candidate, or default, no complete vector exists.
const unavailableConstructorInference = new InferenceUnavailableApplication(); // incomplete[expr-infer/new-expression/class-type-argument-inference]: class type arguments cannot be fully inferred
const laterUnavailableInferenceDemand = unavailableConstructorInference.value;

class OpenFrameApplication<T> {
  self!: OpenFrameApplication<T>;
  pair!: ExactApplication<T, string>;
}

declare const openFrameApplication: OpenFrameApplication<number>;
const openFrameSelfGood: number = openFrameApplication.self.pair.first;
const openFrameSelfBad: string = openFrameApplication.self.pair.first; // error[TK2322]: Type 'number' is not assignable to type 'string'

class InvalidBareSelfApplication<T> {
  self!: InvalidBareSelfApplication; // error[TK2314]
}

class NonGenericApplication {
  value!: number;
}

type NonGenericBareReady = NonGenericApplication;
declare const nonGenericBareReady: NonGenericBareReady;
const nonGenericBareGood: number = nonGenericBareReady.value;
const nonGenericBareBad: string = nonGenericBareReady.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
