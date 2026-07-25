// backlog 92 - one error nested inside contextually typed fresh object literals
// is reported once, not 2^d times.
//
// The sibling shape of `nested_arrows.ts`: `wrap`'s parameter STRUCTURALLY contains
// the type variable, and the contextual re-walk is the fresh-literal path rather
// than the arrow path. tsc 6.0.3 --strict reports exactly one TS2304 per line at
// every depth; typokat reports 2^depth byte-identical copies.

declare function wrap<T>(value: { inner: T }): T;

const objects1 = wrap({ inner: undeclaredThing }); // error[TK2304]: Cannot find name 'undeclaredThing'
const objects2 = wrap({ inner: wrap({ inner: undeclaredThing }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const objects3 = wrap({ inner: wrap({ inner: wrap({ inner: undeclaredThing }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const objects4 = wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: undeclaredThing }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const objects5 = wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: undeclaredThing }) }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const objects6 = wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: undeclaredThing }) }) }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const objects7 = wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: undeclaredThing }) }) }) }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
const objects8 = wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: wrap({ inner: undeclaredThing }) }) }) }) }) }) }) }); // error[TK2304]: Cannot find name 'undeclaredThing'
