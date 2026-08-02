const reverseGlobalThisValue: number = globalThis.B14DeferredGlobalThisContributor();
const reverseGlobalThisWrong: string = globalThis.B14DeferredGlobalThisContributor(); // error[TK2322]: Type 'number' is not assignable to type 'string'
