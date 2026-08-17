// tsc 6.0.3 --strict: the top-level operator owns TS1360, while contextual member
// errors and fresh excess properties keep their established codes.

"wrong" satisfies number; // error[TK1360]

({ value: 1 }) satisfies { value: number; missing: string }; // error[TK1360]

[1] satisfies [number, number]; // error[TK1360]

({ value: "wrong" }) satisfies { value: number }; // error[TK2322]: Type 'string' is not assignable to type 'number'

({ value: 1, extra: true }) satisfies { value: number }; // error[TK2353]

({ nested: { value: 1, extra: true } }) satisfies { nested: { value: number } }; // error[TK2353]

const nestedMemberMismatch = {
  nested: {
    value: "wrong", // error[TK2322]: Type 'string' is not assignable to type 'number'
  },
} satisfies { nested: { value: number } };

const contextualReturnMismatch = ((value) =>
  "wrong" // error[TK2322]: Type 'string' is not assignable to type 'number'
) satisfies (value: number) => number;

const explicitReturnMismatch =
  ((value: number) => "wrong") satisfies (value: number) => number; // error[TK1360]

const nestedExplicitReturnMismatch = {
  callback: (value: number) => "wrong", // error[TK2322]: Type 'string' is not assignable to type 'number'
} satisfies { callback: (value: number) => number };

const mismatchAndExcess = {
  value: "wrong", // error[TK2322]: Type 'string' is not assignable to type 'number'
  extra: true,
} satisfies { value: number };

const twoMismatches = {
  first: "wrong", // error[TK2322]: Type 'string' is not assignable to type 'number'
  second: false, // error[TK2322]: Type 'boolean' is not assignable to type 'string'
} satisfies { first: number; second: string };

const mismatchAndMissing = {
  value: "wrong", // error[TK2322]: Type 'string' is not assignable to type 'number'
} satisfies { value: number; missing: string };

const duplicateNestedPaths = {
  left: { value: 1 },
  right: {
    value: "wrong", // error[TK2322]: Type 'string' is not assignable to type 'number'
  },
} satisfies {
  left: { value: number };
  right: { value: number };
};

const duplicateTuplePaths = [
  [1],
  ["wrong"], // error[TK2322]: Type 'string' is not assignable to type 'number'
] satisfies [[number], [number]];

declare const wholeUnion: { kind: "a"; value: string } | { kind: "b"; value: boolean };
wholeUnion satisfies { kind: "a" | "b"; value: number }; // error[TK1360]
