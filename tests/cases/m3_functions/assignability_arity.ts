// M3 — function assignability edge rules (§6.5), beyond the basic variance in
// function_types.ts. Targets are function types -> code-only markers (README
// "Type display format": function-typed messages are not asserted by substring).
//
//  - arity: a source with FEWER parameters is assignable (it ignores the extra
//    arguments); a source needing MORE parameters than the target is rejected.
//  - return: a `void` target return accepts ANY source return type (the value is
//    discarded), so a value-returning function is assignable to `() => void`.
//  - parameters stay CONTRAVARIANT over the common positions.

// FEWER source params: ok — the source ignores the extra `y` argument.
let a: (x: number, y: number) => void = (x: number) => {};

// MORE source params: rejected — the target cannot supply the surplus `y`.
let b: (x: number) => void = (x: number, y: number) => {}; // error[TK2322]

// void target return accepts a value-returning source.
let cb: () => void = () => 1;

// void target return with a void source.
let cb2: () => void = () => {};

// CONTRAVARIANT param widening still holds: target `number` is assignable to the
// source param `unknown`, so this is ok.
let d: (x: number) => void = (x: unknown) => {};

// CONTRAVARIANT param failure: target `string` is NOT assignable to the source
// param `number`.
let e: (x: string) => void = (x: number) => {}; // error[TK2322]

// Non-void return is still checked covariantly: `string` is not assignable to a
// `number` return.
let f: () => number = () => "s"; // error[TK2322]
