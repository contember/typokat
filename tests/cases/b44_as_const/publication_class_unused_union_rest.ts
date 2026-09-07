class UnusedUnionRest<T extends number[]> {
  identity = ((value) => value) satisfies (value: number, ...args: T | number[]) => number; // error[TK1360]
  literal = ((value) => 1) satisfies (value: number, ...args: T | number[]) => number; // error[TK1360]
  mixed = ((first, second: number) => 1) satisfies (first: number, ...args: T | number[]) => number; // error[TK1360]
}

const unusedUnionRest = new UnusedUnionRest<number[]>();
unusedUnionRest.identity("bad"); // error[TK2345]
unusedUnionRest.literal("bad"); // error[TK2345]
unusedUnionRest.mixed("bad", 1); // error[TK2345]
