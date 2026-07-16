// WU4 — tsc 6.0.3 --strict --noEmit --lib es5 --module commonjs:
// TS2434, TS2345, TS2322 x14, and TS2769 x2 below.

function Wu4ForwardFunction(value: number): number {
  return value;
}
namespace Wu4ForwardFunction {
  export const tag: string = "forward";
  export interface Options {
    enabled: boolean;
  }
}

const wu4ForwardFunctionCall: number = Wu4ForwardFunction(1);
const wu4ForwardFunctionTag: string = Wu4ForwardFunction.tag;
const wu4ForwardFunctionOptions: Wu4ForwardFunction.Options = { enabled: true };
Wu4ForwardFunction("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
const wu4ForwardFunctionCallWrong: string = Wu4ForwardFunction(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4ForwardFunctionTagWrong: number = Wu4ForwardFunction.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4ForwardFunctionOptionsWrong: number = wu4ForwardFunctionOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

function Wu4ForwardOverload(value: number): number;
function Wu4ForwardOverload(value: string): string;
function Wu4ForwardOverload(value: number | string): number | string {
  return value;
}
namespace Wu4ForwardOverload {
  export const tag: string = "overload";
  export interface Options {
    value: number;
  }
}

const wu4ForwardOverloadNumber: number = Wu4ForwardOverload(1);
const wu4ForwardOverloadString: string = Wu4ForwardOverload("ok");
const wu4ForwardOverloadTag: string = Wu4ForwardOverload.tag;
const wu4ForwardOverloadOptions: Wu4ForwardOverload.Options = { value: 1 };
Wu4ForwardOverload(true); // error[TK2769]
const wu4ForwardOverloadTagWrong: number = Wu4ForwardOverload(Wu4ForwardOverload.tag); // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4ForwardOverloadOptionsWrong: string = Wu4ForwardOverload(wu4ForwardOverloadOptions.value); // error[TK2322]: Type 'number' is not assignable to type 'string'

namespace Wu4ReverseFunction { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export const tag: string = "reverse";
  export interface Nested {
    enabled: boolean;
  }
}
function Wu4ReverseFunction(value: number): number {
  return value;
}

const wu4ReverseFunctionCall: number = Wu4ReverseFunction(1);
const wu4ReverseFunctionTag: string = Wu4ReverseFunction.tag;
declare const wu4ReverseFunctionNested: Wu4ReverseFunction.Nested;
const wu4ReverseFunctionCallWrong: string = Wu4ReverseFunction(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4ReverseFunctionTagWrong: number = Wu4ReverseFunction.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4ReverseFunctionNestedWrong: number = wu4ReverseFunctionNested.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

declare function Wu4AmbientForwardFunction(value: number): number;
declare namespace Wu4AmbientForwardFunction {
  const tag: string;
  interface Options {
    enabled: boolean;
  }
}

const wu4AmbientForwardFunctionCall: number = Wu4AmbientForwardFunction(1);
const wu4AmbientForwardFunctionTag: string = Wu4AmbientForwardFunction.tag;
const wu4AmbientForwardFunctionOptions: Wu4AmbientForwardFunction.Options = { enabled: true };
const wu4AmbientForwardFunctionCallWrong: string = Wu4AmbientForwardFunction(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4AmbientForwardFunctionTagWrong: number = Wu4AmbientForwardFunction.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4AmbientForwardFunctionOptionsWrong: number = wu4AmbientForwardFunctionOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

declare namespace Wu4AmbientReverseOverload {
  const tag: string;
  interface Options {
    value: number;
  }
}
declare function Wu4AmbientReverseOverload(value: number): number;
declare function Wu4AmbientReverseOverload(value: string): string;

const wu4AmbientReverseOverloadNumber: number = Wu4AmbientReverseOverload(1);
const wu4AmbientReverseOverloadString: string = Wu4AmbientReverseOverload("ok");
const wu4AmbientReverseOverloadTag: string = Wu4AmbientReverseOverload.tag;
const wu4AmbientReverseOverloadOptions: Wu4AmbientReverseOverload.Options = { value: 1 };
Wu4AmbientReverseOverload(true); // error[TK2769]
const wu4AmbientReverseOverloadNumberWrong: string = Wu4AmbientReverseOverload(wu4AmbientReverseOverloadOptions.value); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4AmbientReverseOverloadStringWrong: number = Wu4AmbientReverseOverload(Wu4AmbientReverseOverload.tag); // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4AmbientReverseOverloadOptionsWrong: string = wu4AmbientReverseOverloadOptions.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
