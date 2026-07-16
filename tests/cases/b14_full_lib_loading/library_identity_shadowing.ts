// tsc 6.0.3 --strict --target es2025: TS2339 x5 below. Module-local names are not
// authoritative library identities for native bridges or intrinsic evaluation.
export {};

interface Array<T> {
  local: T;
}

declare const localArray: Array<number>;
const localValue: number = localArray.local;

const nativeArray = [1, 2];
nativeArray.local; // error[TK2339]

const RegExp = { local: 1 };
const localRegExpValue: number = RegExp.local;
const nativeRegExp = /identity/;
nativeRegExp.local; // error[TK2339]

interface String {
  localString: boolean;
}
declare const localString: String;
const localStringValue: boolean = localString.localString;
"native".localString; // error[TK2339]

interface Object {
  localObject: boolean;
}
declare const localObject: Object;
const localObjectValue: boolean = localObject.localObject;
({ native: true }).localObject; // error[TK2339]

interface Function {
  localFunction: boolean;
}
declare const localFunction: Function;
const localFunctionValue: boolean = localFunction.localFunction;
function nativeFunction(): void {}
nativeFunction.localFunction; // error[TK2339]

type Uppercase<Value extends string> = Value;
type LocalUppercase = Uppercase<"lower">;
const localUppercase: LocalUppercase = "lower";
