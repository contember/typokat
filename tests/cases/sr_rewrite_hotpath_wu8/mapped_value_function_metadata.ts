// tsc 6.0.3 --strict: only the bad argument and final assignment report errors.
// T[K] must rewrite both generic metadata slots and the ordinary signature children.

type GenericMap<T> = {
    [K in keyof T]: <P extends T[K] = T[K]>(value?: T[K]) => T[K];
};

type Result = GenericMap<{ text: string; count: number }>;
declare const result: Result;

const text: string = result.text();
const count: number = result.count();
const textSignature: <P extends string = string>(value?: string) => string = result.text;
result.text(1); // error[TK2345]
const rejected: number = result.text(); // error[TK2322]
