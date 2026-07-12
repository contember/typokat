// tsc 6.0.3 --strict: TS2345 in the source expression.

declare function need(x: number): number;

need("bad") satisfies number; // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number' | incomplete[expr-infer/satisfies-expression/self]
