// tsc 6.0.3 --strict: TS2304 in the explicit type argument.

declare function fn<T>(value: T): T;

const chosen = fn<Missing>; // error[TK2304]: Cannot find name 'Missing' | incomplete[expr-infer/instantiation-expression/self]
