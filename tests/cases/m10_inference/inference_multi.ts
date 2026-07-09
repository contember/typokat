// M10/b65 — compatible same-family candidates may form a literal union, but
// incompatible same-parameter arguments are rejected after inference fixes T.

function both<T>(a: T, b: T): T { return a; }

const u: number | string = both(1, "s"); // error[TK2345]
const n: number = both(1, "s");          // error[TK2345]
const ok2: 1 | 2 = both(1, 2);
const bad2: 1 = both(1, 2);              // error[TK2322]
