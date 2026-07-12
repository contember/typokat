// tsc 6.0.3 --strict: TS2345 in the dynamic-import source expression.

declare function need(x: number): string;

import(need("bad")); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number' | incomplete[expr-infer/import-expression/self]
