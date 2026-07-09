// M32 - the real lib-style ReturnType pattern uses a rest parameter. The M28
// prelude used a zero-arity workaround while rest parameters were out of the type
// model. Cross-checked with tsc 6.0.3 --strict.

const rt1: ReturnType<(x: number) => string> = "s";
const rt2: ReturnType<(x: number) => string> = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'

const rt3: ReturnType<(...args: number[]) => boolean> = true;
const rt4: ReturnType<(...args: number[]) => boolean> = "s"; // error[TK2322]: Type 'string' is not assignable to type 'boolean'
