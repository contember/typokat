declare function contextualIdentity<T>(value: T): T;
declare function contextualCallback<T>(value: T, callback: () => T): T;

let contextualNestedCall = contextualCallback("read" as const, () => contextualIdentity("read"));
const exactContextualNestedCall: "read" = contextualNestedCall;
contextualNestedCall = "write"; // error[TK2322]
declare function contextualFactory<T = string>(): T;
let contextualNestedFactory = contextualCallback("read" as const, () => contextualFactory());
const exactContextualNestedFactory: "read" = contextualNestedFactory;
contextualNestedFactory = "write"; // error[TK2322]
let independentLiteralReturn = contextualCallback("read" as const, () => "read");
const exactIndependentLiteralReturn: "read" = independentLiteralReturn; // error[TK2322]
independentLiteralReturn = "write";

let contextualNestedNumber = contextualCallback(1 as const, () => contextualIdentity(1));
const exactContextualNestedNumber: 1 = contextualNestedNumber;
contextualNestedNumber = 2; // error[TK2322]
let independentNumberReturn = contextualCallback(1 as const, () => 1);
const exactIndependentNumberReturn: 1 = independentNumberReturn; // error[TK2322]
independentNumberReturn = 2;

let contextualNestedBoolean = contextualCallback(true as const, () => contextualIdentity(true));
const exactContextualNestedBoolean: true = contextualNestedBoolean;
contextualNestedBoolean = false; // error[TK2322]
let independentBooleanReturn = contextualCallback(true as const, () => true);
const exactIndependentBooleanReturn: true = independentBooleanReturn; // error[TK2322]
independentBooleanReturn = false;

const regularCallbackReturn = contextualCallback("read", () => "read" as const);
const exactRegularCallbackReturn: "read" = regularCallbackReturn;
let regularCallbackCopy = regularCallbackReturn;
regularCallbackCopy = "write"; // error[TK2322]

declare function constrainedCallback<T extends string>(value: T, callback: () => T): T;
const constrainedCallbackResult = constrainedCallback("read", () => "read");
const exactConstrainedCallback: "read" = constrainedCallbackResult;
declare function incompatibleCallback<T>(value: T, callback: () => T): T;
incompatibleCallback(1, () => "wrong"); // error[TK2322]
