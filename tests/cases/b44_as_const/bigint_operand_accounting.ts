// Bigint literals are valid const-assertion operands. Their existing type-model gap remains
// the sole incomplete outcome until backlog 75 implements bigint literal types.

const positiveBigint = 1n as const; // incomplete[expr-infer/bigint-literal/self]
const negativeBigint = -1n as const; // incomplete[expr-infer/bigint-literal/self]
