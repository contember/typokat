export {};

declare const direct: B103DirectGlobalShape;
declare const nested: B103NestedGlobalNamespace.NestedShape;

const directValue: string = direct.direct.value;
const nestedValue: string = nested.nested.value;
const globalValue: string = b103GlobalValue.value;
const callValue: string = b103GlobalCall(b103GlobalValue).value;

const wrongDirect: number = direct.direct.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wrongNested: number = nested.nested.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wrongGlobal: number = b103GlobalValue.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wrongCall: number = b103GlobalCall(b103GlobalValue).value; // error[TK2322]: Type 'string' is not assignable to type 'number'
