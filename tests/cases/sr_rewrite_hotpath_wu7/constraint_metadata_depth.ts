// tsc 6.0.3 --strict: only the final number assignment reports TS2322.
// Shallow ordered aliases build a deep metadata graph without parser nesting.

type ConstraintMeta00 = Uppercase<"leaf">;
type ConstraintMeta01 = <P extends ConstraintMeta00>() => P;
type ConstraintMeta02 = <P = ConstraintMeta01>() => P;
type ConstraintMeta03 = <P extends ConstraintMeta02>() => P;
type ConstraintMeta04 = <P = ConstraintMeta03>() => P;
type ConstraintMeta05 = <P extends ConstraintMeta04>() => P;
type ConstraintMeta06 = <P = ConstraintMeta05>() => P;
type ConstraintMeta07 = <P extends ConstraintMeta06>() => P;
type ConstraintMeta08 = <P = ConstraintMeta07>() => P;
type ConstraintMeta09 = <P extends ConstraintMeta08>() => P;
type ConstraintMeta10 = <P = ConstraintMeta09>() => P;
type ConstraintMeta11 = <P extends ConstraintMeta10>() => P;
type ConstraintMeta12 = <P = ConstraintMeta11>() => P;
type ConstraintMeta13 = <P extends ConstraintMeta12>() => P;
type ConstraintMeta14 = <P = ConstraintMeta13>() => P;
type ConstraintMeta15 = <P extends ConstraintMeta14>() => P;
type ConstraintMeta16 = <P = ConstraintMeta15>() => P;

declare const constraintValue: ConstraintMeta16;
declare function fixConstraint<T extends ConstraintMeta16>(value: T): T;
const preservedConstraint: ConstraintMeta16 = fixConstraint(constraintValue);
const rejectedConstraint: number = fixConstraint(constraintValue); // error[TK2322]
