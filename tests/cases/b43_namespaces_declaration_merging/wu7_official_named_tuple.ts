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
