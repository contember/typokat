// backlog 65 - expanded array, tuple, and rest candidates still replay args.
// Each negative call has one argument that should fail after T is fixed.

declare function arrayThenScalar<T>(items: T[], value: T): void;
arrayThenScalar([1, 2], "s"); // error[TK2345]
arrayThenScalar([1, 2], 3);

declare const literalTuple: [1, 2];
arrayThenScalar(literalTuple, "s"); // error[TK2345]
arrayThenScalar(literalTuple, 1);

declare function tupleThenScalar<T>(items: [T, T], value: T): void;
tupleThenScalar([1, 2], "s"); // error[TK2345]
tupleThenScalar([1, 2], 3);

declare function restSame<T>(...args: T[]): void;
restSame(1, "s"); // error[TK2345]
restSame(1, 2, 3);

declare function fixedThenRest<T>(head: T, ...tail: T[]): void;
fixedThenRest(1, 2, "s"); // error[TK2345]
fixedThenRest(1, 2, 3);
