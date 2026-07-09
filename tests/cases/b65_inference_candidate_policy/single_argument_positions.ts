// backlog 65 - multiple T occurrences inside one argument still must fix and
// replay against the contextual shape instead of becoming an accepting union.

declare function tupleOneArg<T>(value: [T, T]): void;
tupleOneArg([1, "s"]); // error[TK2322]

declare function needsTuple(value: [number, string]): void;
needsTuple([1]); // error[TK2345]

declare function objectOneArg<T>(value: { a: T; b: T }): void;
objectOneArg({ a: 1, b: "s" }); // error[TK2322]

class TupleBox<T> {
  constructor(value: [T, T]) {}
}

new TupleBox([1, "s"]); // error[TK2322]
