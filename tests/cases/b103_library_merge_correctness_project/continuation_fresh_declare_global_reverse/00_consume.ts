export {};

declare const b103FreshContinuation: B103FreshContinuation;
const freshLabel: string = b103FreshContinuation.label;
const wrongFreshLabel: number = b103FreshContinuation.label; // error[TK2322]: Type 'string' is not assignable to type 'number'
