// tsc 6.0.3 --strict --noEmit: TS2315, TS2707, and TS2344 below.
declare namespace Wu3QualifiedGenericLeaves {
  interface Plain { value: number }
  interface Box<T extends number = 1> { value: T }
  namespace Nested {
    interface Item<T> { value: T }
  }
}

type QualifiedNonGenericLeaf = Wu3QualifiedGenericLeaves.Plain<number>; // error[TK2315]: Type 'Plain' is not generic
type QualifiedWrongArityLeaf = Wu3QualifiedGenericLeaves.Box<1, 2>; // error[TK2707]: Generic type 'Box<T>' requires between 0 and 1 type arguments
type QualifiedConstraintLeaf = Wu3QualifiedGenericLeaves.Box<string>; // error[TK2344]: Type 'string' does not satisfy the constraint 'number'
type QualifiedNestedGenericLeaf = Wu3QualifiedGenericLeaves.Nested.Item<string>;
