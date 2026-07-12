// tsc 6.0.3 --strict: TS2345 in the tagged-template interpolation.

declare function need(x: number): string;
declare function tag(parts: unknown, value: string): number;

tag`${need("bad")}`; // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number' | incomplete[expr-infer/tagged-template/self]
