// tsc 6.0.3 --strict: interface order is clean; ordinary namespace-first function/class
// pairs emit TS2434; reverse interface and ambient keep-pairs retain their surfaces (TS2322 x6).
namespace ReverseInterface {
  export interface Options { enabled: boolean }
}
interface ReverseInterface { instance: number }
let reverseInterfaceInstance: ReverseInterface = { instance: 1 };
let reverseInterfaceOptions: ReverseInterface.Options = { enabled: true };
const reverseInterfaceInstanceWrong: string = reverseInterfaceInstance.instance; // error[TK2322]: Type 'number' is not assignable to type 'string'
const reverseInterfaceOptionsWrong: number = reverseInterfaceOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

namespace ReverseFunction { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export const tag: string = "function";
}
function ReverseFunction(value: number): number { return value; }

namespace ReverseClass { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export const tag: string = "class";
}
class ReverseClass { instance: number = 1; }

declare namespace AmbientReverseFunction {
  const tag: string;
  interface Options { enabled: boolean }
}
declare function AmbientReverseFunction(value: number): number;
const ambientFunctionCall: number = AmbientReverseFunction(1);
const ambientFunctionTag: string = AmbientReverseFunction.tag;
let ambientFunctionOptions: AmbientReverseFunction.Options;
const ambientFunctionReturnWrong: string = AmbientReverseFunction(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const ambientFunctionTagWrong: number = AmbientReverseFunction.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'

declare namespace AmbientReverseClass {
  const tag: string;
  interface Options { enabled: boolean }
}
declare class AmbientReverseClass { instance: number }
const ambientClassInstance: AmbientReverseClass = new AmbientReverseClass();
const ambientClassTag: string = AmbientReverseClass.tag;
let ambientClassOptions: AmbientReverseClass.Options;
const ambientClassInstanceWrong: string = ambientClassInstance.instance; // error[TK2322]: Type 'number' is not assignable to type 'string'
const ambientClassTagWrong: number = AmbientReverseClass.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
