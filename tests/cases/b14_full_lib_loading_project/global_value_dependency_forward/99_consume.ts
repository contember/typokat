const projectForwardDirectValue: number = B14ProjectForwardValueDependent();
const projectForwardDirectWrong: string = B14ProjectForwardValueDependent(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const projectForwardGlobalThisValue: number = globalThis.B14ProjectForwardValueDependent();
const projectForwardGlobalThisWrong: string = globalThis.B14ProjectForwardValueDependent(); // error[TK2322]: Type 'number' is not assignable to type 'string'
