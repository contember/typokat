// tsc 6.0.3 --strict: TS2345 on the awaited call.

declare function need(x: number): number;

async function f() { await need("bad"); } // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number' | incomplete[expr-infer/await-expression/self]
