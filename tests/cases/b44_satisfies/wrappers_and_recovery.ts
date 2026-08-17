// tsc 6.0.3 --strict: wrappers do not bypass the operator, and unresolved/error
// recovery does not add a second relation diagnostic.

let value = 1;

(value satisfies number)++;
((value satisfies number)!)++; // incomplete[expr-infer/non-null-assertion/self]

const unresolved = Missing satisfies number; // error[TK2304]: Cannot find name 'Missing'

declare const wrong: string;
const parenthesized = ((wrong) satisfies number); // error[TK1360]

const nested = (("wrong" satisfies number) satisfies number); // error[TK1360] | error[TK1360]
