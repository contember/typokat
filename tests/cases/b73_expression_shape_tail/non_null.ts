// tsc 6.0.3 --strict: TS2304; typokat must also expose the unsupported wrapper.

const n = Missing!; // error[TK2304]: Cannot find name 'Missing' | incomplete[expr-infer/non-null-assertion/self]
