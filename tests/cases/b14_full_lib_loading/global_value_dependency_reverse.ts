function B14ReverseValueDependent() {
  return B14LaterValueDependency;
}

const B14LaterValueDependency = 42;

const reverseDirectValue: number = B14ReverseValueDependent();
const reverseDirectWrong: string = B14ReverseValueDependent(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const reverseGlobalThisValue: number = globalThis.B14ReverseValueDependent();
const reverseGlobalThisWrong: string = globalThis.B14ReverseValueDependent(); // error[TK2322]: Type 'number' is not assignable to type 'string'
