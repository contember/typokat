export {};

declare const b103MixedFreshContinuation: B103MixedFreshContinuation;
const baseTest: boolean = /mixed/.test("mixed");
const mixedTag: string = /mixed/.b103ContinuationTag();
const mixedCount: number = b103MixedFreshContinuation.count;
const wrongBaseTest: string = /mixed/.test("mixed"); // error[TK2322]: Type 'boolean' is not assignable to type 'string'
const wrongMixedTag: number = /mixed/.b103ContinuationTag(); // error[TK2322]: Type 'string' is not assignable to type 'number'
const wrongMixedCount: string = b103MixedFreshContinuation.count; // error[TK2322]: Type 'number' is not assignable to type 'string'
