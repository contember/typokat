// Erased alias arguments must not select a cyclic seed in either signature fixer.

type SuppliedConstraintSecond<X, Y> = Y;
declare const erasedConstraintArguments: [number, number, number];
declare function suppliedErasedGraph<A extends SuppliedConstraintSecond<C, B>, B extends { c: C }, C extends { b: B }>(values: [A, B, C]): [A, B, C];
const suppliedErasedResult = suppliedErasedGraph(erasedConstraintArguments); // error[TK2345]
const legalSuppliedErased: { b: number } = suppliedErasedResult[1].c;
const illegalSuppliedErased: { c: number } = suppliedErasedResult[2].b; // error[TK2322]

declare class SuppliedErasedGraph<A extends SuppliedConstraintSecond<C, B>, B extends { c: C }, C extends { b: B }> {
  constructor(values: [A, B, C]);
  value: [A, B, C];
}
const suppliedErasedInstance = new SuppliedErasedGraph(erasedConstraintArguments); // error[TK2345]
const legalPartialErased: { b: number } = suppliedErasedInstance.value[1].c;
const illegalPartialErased: { c: number } = suppliedErasedInstance.value[2].b; // error[TK2322]
