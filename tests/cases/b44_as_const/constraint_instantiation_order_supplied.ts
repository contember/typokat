// One tuple argument exercises all candidate slots without ambiguous per-argument error counts.

declare const invalidConstraintArguments: [number, number, number];
declare function suppliedConstraintGraph<A extends [C, B], B extends { c: C }, C extends { b: B }>(values: [A, B, C]): [A, B, C];
const suppliedConstraintResult = suppliedConstraintGraph(invalidConstraintArguments); // error[TK2345]
const legalSuppliedConstraint: { c: number } = suppliedConstraintResult[2].b;
const illegalSuppliedConstraint: { b: number } = suppliedConstraintResult[1].c; // error[TK2322]

declare class SuppliedConstraintGraph<A extends [C, B], B extends { c: C }, C extends { b: B }> {
  constructor(values: [A, B, C]);
  value: [A, B, C];
}
const suppliedConstraintInstance = new SuppliedConstraintGraph(invalidConstraintArguments); // error[TK2345]
const legalPartialConstraint: { c: number } = suppliedConstraintInstance.value[2].b;
const illegalPartialConstraint: { b: number } = suppliedConstraintInstance.value[1].c; // error[TK2322]

interface ConstraintBShape { c: ConstraintCShape }
interface ConstraintCShape { b: ConstraintBShape }
declare const validConstraintArguments: [[ConstraintCShape, ConstraintBShape], ConstraintBShape, ConstraintCShape];
const validSuppliedConstraint = suppliedConstraintGraph(validConstraintArguments);
const validSuppliedShape: [[ConstraintCShape, ConstraintBShape], ConstraintBShape, ConstraintCShape] = validSuppliedConstraint;
const validPartialConstraint = new SuppliedConstraintGraph(validConstraintArguments);
const validPartialShape: [[ConstraintCShape, ConstraintBShape], ConstraintBShape, ConstraintCShape] = validPartialConstraint.value;
