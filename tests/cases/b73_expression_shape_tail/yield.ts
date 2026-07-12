// tsc 6.0.3 --strict: TS2345 on the yielded call.

declare function need(x: number): number;

function* f() { yield need("bad"); } // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number' | incomplete[expr-infer/yield-expression/self]
