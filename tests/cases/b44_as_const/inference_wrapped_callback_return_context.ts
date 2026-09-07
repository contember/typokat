enum WrappedReturnMode { Read = "read", Write = "write" }
declare function wrappedReturnIdentity<T>(value: T): T;
declare function wrappedReturnObject<T>(value: T, callback: () => { value: T }): T;
declare function wrappedReturnArray<T>(value: T, callback: () => T[]): T;
declare function wrappedReturnDirect<T>(value: T, callback: () => T): T;

const directEnumReturn = wrappedReturnDirect(WrappedReturnMode.Read, () => "read");
const exactDirectEnumReturn: "read" = directEnumReturn; // error[TK2322]
let directEnumReturnCopy = directEnumReturn;
directEnumReturnCopy = WrappedReturnMode.Write;

const objectEnumReturn = wrappedReturnObject(wrappedReturnIdentity(WrappedReturnMode.Read), () => ({ value: "read" }));
const exactObjectEnumReturn: "read" = objectEnumReturn; // error[TK2322]
let objectEnumReturnCopy = objectEnumReturn;
objectEnumReturnCopy = WrappedReturnMode.Write;

const arrayEnumReturn = wrappedReturnArray(wrappedReturnIdentity(WrappedReturnMode.Read), () => ["read"]);
const exactArrayEnumReturn: "read" = arrayEnumReturn; // error[TK2322]
let arrayEnumReturnCopy = arrayEnumReturn;
arrayEnumReturnCopy = WrappedReturnMode.Write;
