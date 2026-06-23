// M9 — generic interfaces / type aliases instantiated with explicit type args.
// `Box<number>` instantiates the body to `{ value: number }`.

interface Box<T> { value: T; }

const x: Box<number> = { value: 1 };   // ok
const y: Box<number> = { value: "s" }; // error[TK2322]
// ^ value should be number after instantiation

const z: Box<number> = { value: 1, extra: 2 }; // error[TK2353]
// ^ excess property still checked on the instantiated type

type Pair<A, B> = { first: A; second: B };

const p: Pair<number, string> = { first: 1, second: "s" }; // ok
const q: Pair<number, string> = { first: "s", second: 1 }; // error[TK2322]
