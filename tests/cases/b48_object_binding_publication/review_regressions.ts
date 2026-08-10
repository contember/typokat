// WU2 adversarial review: suppressed default diagnostics, excluded clean exits, and default order.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit` reports TS2339 + TS2345 on line 7 and
// TS2448 on line 18. The excluded shapes are tsc-clean but remain explicit typokat boundaries.

declare function b48ReviewNeedNumber(value: number): number;
declare const b48ReviewMissingSource: { present: number };
const { missingReview = () => b48ReviewNeedNumber("bad") } = b48ReviewMissingSource; // error[TK2339] | error[TK2345]

const { ...b48ReviewRestOnly } = { present: 1 }; // incomplete[bind/binding-pattern/object-pattern]
const { present: [b48ReviewNestedArray] } = { present: [1] }; // incomplete[bind/binding-pattern/object-pattern]

// Existing F4 private/protected parameter diagnostics already make that path non-clean.
// An otherwise-clean excluded parameter still needs an explicit boundary without a body read.
function b48ReviewUnusedParameter({ b48ReviewLeaf }: { b48ReviewLeaf: number }): void {} // incomplete[bind/binding-pattern/object-pattern]

// TK2448 remains owned by backlog 47; this slice must not silently accept the forward default.
const {
  b48ReviewBefore = b48ReviewLater, // incomplete[bind/binding-pattern/object-default-order]: object binding default references a later leaf
  b48ReviewLater = 1,
}: { b48ReviewBefore?: number; b48ReviewLater?: number } = {};
