// backlog 92 - the non-generic-callback discriminator (real `describe`/`it`).
//
// A non-generic signature has no inference phase at all, so this shape also costs
// two walks per level - and again both retain, so the duplicate count is 2^depth.
// The second of the two shapes that hang in the wild.
//
// tsc 6.0.3 --strict: exactly one TS2304 per line, at every depth.

declare function describe(fn: () => void): void;

describe(() => { undeclaredThing; }); // error[TK2304]: Cannot find name 'undeclaredThing'
describe(() => { describe(() => { undeclaredThing; }); }); // error[TK2304]: Cannot find name 'undeclaredThing'
describe(() => { describe(() => { describe(() => { undeclaredThing; }); }); }); // error[TK2304]: Cannot find name 'undeclaredThing'
describe(() => { describe(() => { describe(() => { describe(() => { undeclaredThing; }); }); }); }); // error[TK2304]: Cannot find name 'undeclaredThing'
describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { undeclaredThing; }); }); }); }); }); // error[TK2304]: Cannot find name 'undeclaredThing'
describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { undeclaredThing; }); }); }); }); }); }); // error[TK2304]: Cannot find name 'undeclaredThing'
describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { undeclaredThing; }); }); }); }); }); }); }); // error[TK2304]: Cannot find name 'undeclaredThing'
describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { describe(() => { undeclaredThing; }); }); }); }); }); }); }); }); // error[TK2304]: Cannot find name 'undeclaredThing'
