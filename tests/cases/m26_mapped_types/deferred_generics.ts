// M26 — a mapped type over an UNRESOLVED type parameter stays deferred and
// relates conservatively, mirroring the M25 deferred-conditional model:
// identical deferred nodes are assignable, nothing else is assignable in
// either direction. NOTE the documented divergence: tsc has a special
// homomorphic-identity rule (`T` is assignable to `{ [K in keyof T]: T[K] }`),
// so the `ret1` marker below is a typokat over-report (safe direction) —
// see tests/cases/README.md. tsc 6.0.3 cross-checked otherwise.

type Ident<T> = { [K in keyof T]: T[K] };

// tsc: ok (homomorphic identity rule) — typokat: conservative deferral.
function ret1<T>(t: T): Ident<T> {
  return t; // error[TK2322]
}

// An identical deferred mapped type is assignable to itself.
function ret2<T>(t: Ident<T>): Ident<T> {
  return t;
}

// Nothing else is assignable into a deferred mapped type.
function ret3<T>(t: T): Ident<T> {
  return 5; // error[TK2322]
}

// Call sites instantiate and evaluate (the deferral ends there).
declare function mk<T>(t: T): Ident<T>;
const c1: { v: number } = mk({ v: 1 });
const c2: { v: string } = mk({ v: 1 }); // error[TK2322]
