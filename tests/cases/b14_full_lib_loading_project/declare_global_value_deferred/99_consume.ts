const deferredGlobalValue: number = B14DeferredGlobalValue;
const deferredGlobalCall: number = B14DeferredGlobalFunction(1);
const deferredGlobalClassValue: number = new B14DeferredGlobalClass().value;

const wrongDeferredGlobalValue: string = B14DeferredGlobalValue; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongDeferredGlobalCall: string = B14DeferredGlobalFunction(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongDeferredGlobalClassValue: string = new B14DeferredGlobalClass().value; // error[TK2322]: Type 'number' is not assignable to type 'string'
