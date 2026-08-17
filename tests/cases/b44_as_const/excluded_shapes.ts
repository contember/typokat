// Explicit exclusion ledger for the original TypeScript const-context PR surface.
// These rows require accounting only; this sprint does not model their const semantics.

const spreadObjectSource = { value: 1 };
const excludedObjectSpread = ({ ...spreadObjectSource } as const); // incomplete[expr-infer/object-literal/spread-element]

const spreadArraySource = [1, 2];
const excludedArraySpread = ([...spreadArraySource] as const); // incomplete[expr-infer/array-literal/spread-element]
const excludedArrayElision = ([1, , 2] as const); // incomplete[expr-infer/array-literal/elision]

const computedKey = "computed";
const excludedComputedKey = ({ [computedKey]: 1 } as const); // incomplete[expr-infer/object-literal/computed-key]

// Backlog 75 owns const-context handling for object methods and invalid operands.
const excludedObjectMethod = ({ method() { return 1; } } as const); // incomplete[expr-infer/as-const/object-method]
const excludedInvalidOperand = ((1 + 1) as const); // incomplete[expr-infer/as-const/invalid-operand]
