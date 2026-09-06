// Constraint dependency work follows the denoted semantic form, not every written alias argument.

type ConstraintSecond<X, Y> = Y;
declare function erasedConstraintGraph<A extends ConstraintSecond<C, B>, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const erasedConstraintResult = erasedConstraintGraph();
const legalErasedConstraint: { b: unknown } = erasedConstraintResult[1].c;
const illegalErasedConstraint: { c: unknown } = erasedConstraintResult[2].b; // error[TK2322]

type ConstraintIndexedSecond<X, Y> = [X, Y][1];
declare function indexedConstraintGraph<A extends ConstraintIndexedSecond<C, B>, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const indexedConstraintResult = indexedConstraintGraph();
const legalIndexedConstraint: { b: unknown } = indexedConstraintResult[1].c;
const illegalIndexedConstraint: { c: unknown } = indexedConstraintResult[2].b; // error[TK2322]

declare function unionConstraintGraph<A extends C | B, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const unionConstraintResult = unionConstraintGraph();
const legalUnionConstraint: { c: unknown } = unionConstraintResult[2].b;
const illegalUnionConstraint: { b: unknown } = unionConstraintResult[1].c; // error[TK2322]

declare function intersectionConstraintGraph<A extends C & B, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const intersectionConstraintResult = intersectionConstraintGraph();
const legalIntersectionConstraint: { c: unknown } = intersectionConstraintResult[2].b;
const illegalIntersectionConstraint: { b: unknown } = intersectionConstraintResult[1].c; // error[TK2322]

interface ConstraintReferenceBox<X, Y> { value: Y }
declare function interfaceConstraintGraph<A extends ConstraintReferenceBox<C, B>, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const interfaceConstraintResult = interfaceConstraintGraph();
const legalInterfaceConstraint: { c: unknown } = interfaceConstraintResult[2].b;
const illegalInterfaceConstraint: { b: unknown } = interfaceConstraintResult[1].c; // error[TK2322]
