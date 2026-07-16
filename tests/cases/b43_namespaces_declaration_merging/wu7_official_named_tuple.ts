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

// A direct infer rest is valid only in a conditional extends clause and only
// when its own constraint is array-like. tsc reports TS2574 for the first case;
// owner75 withholds the tuple until that diagnostic is modeled.
type Wu7ConstrainedInferNonArray<T> = T extends [...infer R extends number] ? R : never; // incomplete[annotation-lower/tuple-rest-element/non-array]
type Wu7ConstrainedInferArrayControl<T> = T extends [...infer R extends readonly unknown[]] ? R : never;

// tsc reports TS1338 for declaring infer in a conditional true branch. This is
// a separate infer-placement validation gap; the tuple-rest boundary must still
// prevent the invalid declaration from becoming a silently published tuple.
type Wu7MisplacedInferRest<T> = T extends unknown ? [...infer R] : never; // incomplete[annotation-lower/tuple-rest-element/non-array]
