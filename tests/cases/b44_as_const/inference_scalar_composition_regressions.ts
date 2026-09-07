declare function compositionIdentity<T>(value: T): T;
declare function compositionCallback<T>(value: T, callback: (value: T) => void): T;
declare function compositionObjectCallback<T>(value: T, callback: { run: (value: T) => void }): T;
declare function compositionRest<T>(value: T, callback: (...values: T[]) => void): T;
declare function compositionReturn<T>(value: T, callback: () => T): T;

const functionResult = compositionCallback("read", function(value) {});
const exactFunctionResult: "read" = functionResult; // error[TK2322]
const nestedFunctionResult = compositionObjectCallback("read", { run: function(value) {} });
const exactNestedFunctionResult: "read" = nestedFunctionResult; // error[TK2322]
const arrowResult = compositionCallback("read", value => {});
const exactArrowResult: "read" = arrowResult; // error[TK2322]
const omittedFunctionResult = compositionCallback("read", function() {});
const exactOmittedFunctionResult: "read" = omittedFunctionResult;

const restResult = compositionRest(compositionIdentity("read"), (...values) => {});
const exactRestResult: "read" = restResult; // error[TK2322]
const regularRestResult = compositionRest(compositionIdentity("read" as const), (...values) => {});
const exactRegularRestResult: "read" = regularRestResult;
let regularRestCopy = regularRestResult;
regularRestCopy = "write"; // error[TK2322]

enum CompositionMode { Read = "read", Write = "write" }
const enumRestResult = compositionRest(compositionIdentity(CompositionMode.Read), (...values) => {});
const exactEnumRestResult: CompositionMode.Read = enumRestResult; // error[TK2322]
const enumReturnResult = compositionReturn(compositionIdentity(CompositionMode.Read), () => "read");
const exactEnumReturnResult: "read" = enumReturnResult; // error[TK2322]
let enumReturnCopy = compositionReturn(compositionIdentity(CompositionMode.Read), () => "read");
enumReturnCopy = CompositionMode.Write;
let textReturnCopy = compositionReturn(compositionIdentity("read"), () => "read");
textReturnCopy = "write";
