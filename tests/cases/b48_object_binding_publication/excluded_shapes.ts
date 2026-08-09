// This slice admits only flat object patterns in variable declarations. Every valid excluded
// pattern remains explicit instead of publishing an error-typed leaf; invalid for-in pins TK2491.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit`; only for-in reports TS2491.

const { outer: { nestedLeaf } } = { outer: { nestedLeaf: 1 } }; // incomplete[bind/binding-pattern/object-pattern]
const [arrayLeaf] = [1]; // incomplete[bind/binding-pattern/array-pattern]
const { keptLeaf, ...restLeaf } = { keptLeaf: 1, extraLeaf: "x" }; // incomplete[bind/binding-pattern/object-pattern]

declare const b48ComputedKey: "computed";
const { [b48ComputedKey]: computedLeaf } = { computed: 1 }; // incomplete[bind/binding-pattern/object-pattern]

function b48Parameter({ parameterLeaf }: { parameterLeaf: number }): void { // incomplete[bind/binding-pattern/object-pattern]
  parameterLeaf;
}

try {
  throw "caught";
} catch ({ catchLeaf }: any) { // incomplete[stmt-check/try-statement/catch-param]
  catchLeaf;
}

for (const { loopLeaf } of [{ loopLeaf: 1 }]) { // incomplete[bind/binding-pattern/object-pattern]
  loopLeaf;
}

for (const { length: keyLength } in { ready: 1 }) { // error[TK2491]
  keyLength;
}
