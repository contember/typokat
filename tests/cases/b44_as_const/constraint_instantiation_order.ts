// The cyclic seed follows constraint instantiation order, not one global parameter sort.

declare function reverseConstraintGraph<A extends [C, B], B extends { c: C }, C extends { b: B }>(): [A, B, C];
const reverseConstraintResult = reverseConstraintGraph();
const legalReverseConstraint: { c: unknown } = reverseConstraintResult[2].b;
const illegalReverseConstraint: { b: unknown } = reverseConstraintResult[1].c; // error[TK2322]

declare function forwardConstraintGraph<A extends [B, C], B extends { c: C }, C extends { b: B }>(): [A, B, C];
const forwardConstraintResult = forwardConstraintGraph();
const legalForwardConstraint: { b: unknown } = forwardConstraintResult[1].c;
const illegalForwardConstraint: { c: unknown } = forwardConstraintResult[2].b; // error[TK2322]

declare function objectConstraintGraph<A extends { a: C; z: B }, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const objectConstraintResult = objectConstraintGraph();
const legalObjectConstraint: { b: unknown } = objectConstraintResult[1].c;
const illegalObjectConstraint: { c: unknown } = objectConstraintResult[2].b; // error[TK2322]
