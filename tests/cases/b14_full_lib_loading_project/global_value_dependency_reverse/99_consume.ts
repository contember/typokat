const projectReverseDirectValue: number = B14ProjectReverseValueDependent();
const projectReverseDirectWrong: string = B14ProjectReverseValueDependent(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const projectReverseGlobalThisValue: number = globalThis.B14ProjectReverseValueDependent();
const projectReverseGlobalThisWrong: string = globalThis.B14ProjectReverseValueDependent(); // error[TK2322]: Type 'number' is not assignable to type 'string'
