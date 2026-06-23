// M10 — multiple inference candidates for one type parameter unify to their union.

function both<T>(a: T, b: T): T { return a; }

const u: number | string = both(1, "s"); // ok — T inferred number | string
const n: number = both(1, "s");          // error[TK2322]
const ok2: number = both(1, 2);          // ok — T inferred number
