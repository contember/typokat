// Forward signature demand differs from declaration-first union construction in the pinned oracle.

const forwardUnionConstraintResult = forwardUnionConstraint();
declare function forwardUnionConstraint<A extends C | B, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const legalForwardUnionConstraint: { b: unknown } = forwardUnionConstraintResult[1].c;
const illegalForwardUnionConstraint: { c: unknown } = forwardUnionConstraintResult[2].b; // error[TK2322]
