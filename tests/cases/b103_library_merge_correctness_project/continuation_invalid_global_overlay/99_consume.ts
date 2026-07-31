export {};

const invalidArrayLeak = ([] as number[]).b103InvalidArrayLeak(); // error[TK2339]: Property 'b103InvalidArrayLeak' does not exist on type 'number[]'
declare const b103InvalidFreshContinuation: B103InvalidFreshContinuation; // error[TK2304]: Cannot find name 'B103InvalidFreshContinuation'
