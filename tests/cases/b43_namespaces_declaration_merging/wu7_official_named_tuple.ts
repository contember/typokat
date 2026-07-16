// Disabled WU7 oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5
// --module commonjs. Tuple labels are semantically inert; named optional members
// remain owned by backlog 75 and are not part of this named-member slice.

type Wu7NamedFixed = [text: string, count: number];
type Wu7PlainFixed = [string, number];

const wu7NamedFixedOk: Wu7NamedFixed = ["ok", 1];
const wu7PlainFixedOk: Wu7PlainFixed = ["ok", 1];
const wu7NamedFixedAsPlain: Wu7PlainFixed = wu7NamedFixedOk;
const wu7PlainFixedAsNamed: Wu7NamedFixed = wu7PlainFixedOk;
const wu7NamedFixedBad: Wu7NamedFixed = ["ok", "bad"]; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu7PlainFixedBad: Wu7PlainFixed = ["ok", "bad"]; // error[TK2322]: Type 'string' is not assignable to type 'number'

type Wu7NamedRest = [...strs: string[], n2: number];
type Wu7PlainRest = [...string[], number];

const wu7NamedRestOk: Wu7NamedRest = ["a", "b", 1];
const wu7PlainRestOk: Wu7PlainRest = ["a", "b", 1];
const wu7NamedRestAsPlain: Wu7PlainRest = wu7NamedRestOk;
const wu7PlainRestAsNamed: Wu7NamedRest = wu7PlainRestOk;

function wu7NamedRestCall(...args: [...strs: string[], n2: number]): void {}
function wu7PlainRestCall(...args: [...string[], number]): void {}

wu7NamedRestCall(1);
wu7NamedRestCall("a", "b", 1);
wu7PlainRestCall(1);
wu7PlainRestCall("a", "b", 1);
wu7NamedRestCall("a", "bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
wu7PlainRestCall("a", "bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

type Wu7NamedOptional = [value?: string]; // incomplete[annotation-lower/tuple-optional-element/self]

// tsc 6.0.3 reports TS2574 for each non-array rest element below. TK2574 is not
// claimed here; owner75 records the sound lowering boundary instead.
type Wu7PlainNonArrayRest = [string, ...number]; // incomplete[annotation-lower/tuple-rest-element/non-array]
type Wu7NamedNonArrayRest = [first: string, ...rest: number]; // incomplete[annotation-lower/tuple-rest-element/non-array]

// Controls: array, fixed-tuple, and constrained-generic rest containers are valid.
type Wu7PlainArrayRestControl = [string, ...number[]];
type Wu7NamedArrayRestControl = [first: string, ...rest: number[]];
type Wu7PlainTupleRestControl = [string, ...[number, boolean]];
type Wu7NamedTupleRestControl = [first: string, ...rest: [number, boolean]];
type Wu7PlainGenericRestControl<T extends unknown[]> = [string, ...T];
type Wu7NamedGenericRestControl<T extends unknown[]> = [first: string, ...rest: T];

const wu7PlainArrayRestControl: Wu7PlainArrayRestControl = ["ok", 1, 2];
const wu7NamedArrayRestControl: Wu7NamedArrayRestControl = ["ok", 1, 2];
const wu7PlainTupleRestControl: Wu7PlainTupleRestControl = ["ok", 1, true];
const wu7NamedTupleRestControl: Wu7NamedTupleRestControl = ["ok", 1, true];
const wu7PlainGenericRestControl: Wu7PlainGenericRestControl<[number, boolean]> = ["ok", 1, true];
const wu7NamedGenericRestControl: Wu7NamedGenericRestControl<[number, boolean]> = ["ok", 1, true];

// Constrained infer is not modeled yet. Every constraint is withheld at the
// infer declaration, including constraints which happen to be array-like.
type Wu7ConstrainedInferDirect<T> = T extends infer R extends string ? R : never; // incomplete[annotation-lower/infer-type/constraint]
type Wu7ConstrainedInferNonArray<T> = T extends [...infer R extends number] ? R : never; // incomplete[annotation-lower/infer-type/constraint]
type Wu7ConstrainedInferArray<T> = T extends [...infer R extends readonly unknown[]] ? R : never; // incomplete[annotation-lower/infer-type/constraint]

// tsc selects the false branch and reports TS2322 on the following declaration.
// Typokat withholds the alias at the constraint instead of publishing the
// unconstrained number-tuple capture.
type Wu7ConstrainedInferFalseClean<T> = T extends [...infer R extends string[]] ? R : "fallback"; // incomplete[annotation-lower/infer-type/constraint]
const wu7ConstrainedInferFalseClean: Wu7ConstrainedInferFalseClean<[number]> = [1];

// A declaration becomes a reference only in the true branch. It is not visible
// to a sibling type inside the same extends pattern, on either lowering path.
type Wu7InferSameExtendsOrdinary<T> = T extends [infer R, R] ? R : never; // error[TK2304]: Cannot find name 'R'
declare class Wu7InferSameExtendsClass<T> {
  value: T extends [infer R, R] ? R : never; // error[TK2304]: Cannot find name 'R'
}

// The conditional binder shadows an outer class parameter in its true branch.
declare class Wu7InferClassShadow<R> {
  value: string extends infer R ? R : never;
}
declare const wu7InferClassShadow: Wu7InferClassShadow<number>;
const wu7InferClassShadowOk: string = wu7InferClassShadow.value;
const wu7InferClassShadowBad: number = wu7InferClassShadow.value; // error[TK2322]: Type 'string' is not assignable to type 'number'

// An active infer binder is never generic, even when a lexical generic alias has
// the same name. Type arguments must not bypass infer-name resolution.
type Wu7InferAppliedR<T> = T;
type Wu7InferAppliedOrdinary<T> = T extends infer Wu7InferAppliedR ? Wu7InferAppliedR<number> : never; // error[TK2315]: Type 'Wu7InferAppliedR' is not generic
declare class Wu7InferAppliedClass<T> {
  value: T extends infer Wu7InferAppliedR ? Wu7InferAppliedR<number> : never; // error[TK2315]: Type 'Wu7InferAppliedR' is not generic
}

// Bare and parenthesized infer-rest declarations are both valid and preserve the
// precise captured tuple in the true branch.
type Wu7BareInferRestControl<T> = T extends [...infer R] ? R : never;
type Wu7ParenthesizedInferRest<T> = T extends [...(infer R)] ? R : never;
const wu7BareInferRestControl: Wu7BareInferRestControl<[string, number]> = ["ok", 1];
const wu7ParenthesizedInferRestOk: Wu7ParenthesizedInferRest<[string, number]> = ["ok", 1];
const wu7ParenthesizedInferRestBad: Wu7ParenthesizedInferRest<[string, number]> = ["ok", "bad"]; // error[TK2322]: Type 'string' is not assignable to type 'number'

// tsc reports TS1338 for declaring infer in a conditional true branch. This is
// a separate infer-placement validation gap; the tuple-rest boundary must still
// prevent the invalid declaration from becoming a silently published tuple.
type Wu7MisplacedInferRest<T> = T extends unknown ? [...infer R] : never; // incomplete[annotation-lower/tuple-rest-element/non-array]
