// Module-local declarations with library spellings are not library semantic identities. The
// preflight must keep this project on the fast path, and bridges must still use native shapes.
interface Array<T> {
  moduleLocalArray: T;
}
interface String {
  moduleLocalString: boolean;
}
interface Object {
  moduleLocalObject: boolean;
}
interface Function {
  moduleLocalFunction: boolean;
}
const RegExp = { moduleLocalRegExp: true };

declare const declaredArray: Array<number>;
const declaredArrayValue: number = declaredArray.moduleLocalArray;
declare const declaredString: String;
const declaredStringValue: boolean = declaredString.moduleLocalString;
declare const declaredObject: Object;
const declaredObjectValue: boolean = declaredObject.moduleLocalObject;
declare const declaredFunction: Function;
const declaredFunctionValue: boolean = declaredFunction.moduleLocalFunction;
const declaredRegExpValue: boolean = RegExp.moduleLocalRegExp;

[1, 2].moduleLocalArray; // error[TK2339]
"native".moduleLocalString; // error[TK2339]
({ native: true }).moduleLocalObject; // error[TK2339]
function nativeFunction(): void {}
nativeFunction.moduleLocalFunction; // error[TK2339]
/native/.moduleLocalRegExp; // error[TK2339]

export function loadCount(): Promise<number> {
  return Promise.resolve([1, 2, 3].map((value) => value + 1).length);
}
