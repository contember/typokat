const forwardGlobalThisValue: number = globalThis.B14DeferredGlobalThisContributor();
const forwardGlobalThisWrong: string = globalThis.B14DeferredGlobalThisContributor(); // error[TK2322]: Type 'number' is not assignable to type 'string'
