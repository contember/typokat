const B14EarlierValueDependency = 42;

function B14ForwardValueDependent() {
  return B14EarlierValueDependency;
}

const forwardDirectValue: number = B14ForwardValueDependent();
const forwardDirectWrong: string = B14ForwardValueDependent(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const forwardGlobalThisValue: number = globalThis.B14ForwardValueDependent();
const forwardGlobalThisWrong: string = globalThis.B14ForwardValueDependent(); // error[TK2322]: Type 'number' is not assignable to type 'string'
