// tsc 6.0.3 --strict --target es2025: TS2344 x2, TS2314 x2, and TS2339 x2 below.
// A ready eager-application cache may reuse only the completed type. Every occurrence still owns
// and emits its constraint, arity, and downstream member diagnostics independently.

type B14CacheConstrained<T extends { kind: string }> = { value: T };
type B14CachePair<A, B> = { first: A; second: B };
type B14CacheBox<T> = { value: T };

declare const badConstraintFirst: B14CacheConstrained<number>; // error[TK2344]
declare const badConstraintSecond: B14CacheConstrained<number>; // error[TK2344]

declare const badArityFirst: B14CachePair<string>; // error[TK2314]
declare const badAritySecond: B14CachePair<string>; // error[TK2314]

declare const memberFirst: B14CacheBox<string>;
memberFirst.missing; // error[TK2339]
declare const memberSecond: B14CacheBox<string>;
memberSecond.missing; // error[TK2339]
