// Union construction shares one order; written order per occurrence is insufficient.

declare function firstUnionConstruction<A extends [C | B, B | C], B extends { c: C }, C extends { b: B }>(): [A, B, C];
const firstUnionConstructionResult = firstUnionConstruction();
const illegalFirstUnionConstruction: { b: unknown } = firstUnionConstructionResult[1].c; // error[TK2322]
const legalFirstUnionConstruction: { c: unknown } = firstUnionConstructionResult[2].b;

declare function reverseUnionConstruction<A extends [B | C, C | B], B extends { c: C }, C extends { b: B }>(): [A, B, C];
const reverseUnionConstructionResult = reverseUnionConstruction();
const legalReverseUnionConstruction: { b: unknown } = reverseUnionConstructionResult[1].c;
const illegalReverseUnionConstruction: { c: unknown } = reverseUnionConstructionResult[2].b; // error[TK2322]

type AllocationConstraintSecond<X, Y> = Y;
declare function erasedUnionConstruction<A extends AllocationConstraintSecond<B, C> | B, B extends { c: C }, C extends { b: B }>(): [A, B, C];
const erasedUnionConstructionResult = erasedUnionConstruction();
const legalErasedUnionConstruction: { b: unknown } = erasedUnionConstructionResult[1].c;
const illegalErasedUnionConstruction: { c: unknown } = erasedUnionConstructionResult[2].b; // error[TK2322]
