// Type assertions already publish their asserted type, but validating whether the source and
// asserted types sufficiently overlap (TS2352) is not implemented. Keep that missing semantic
// check explicit without weakening ordinary assertion inference.

interface AssertionSource {
  source: string;
}

interface AssertionTarget {
  target: number;
}

declare const source: AssertionSource;

const invalidAs = source as AssertionTarget; // incomplete[expr-infer/as-assertion/compatibility]
const invalidAngle = <AssertionTarget>source; // incomplete[expr-infer/type-assertion/compatibility]

// Controls: valid assertions retain the asserted type and do not record an incomplete.
const validAs: AssertionSource = source as AssertionSource;
const validAngle: AssertionSource = <AssertionSource>source;
const targetFromAs: AssertionTarget = invalidAs;
const targetFromAngle: AssertionTarget = invalidAngle;
