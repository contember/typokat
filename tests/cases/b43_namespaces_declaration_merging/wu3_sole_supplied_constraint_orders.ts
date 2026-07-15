// tsc 6.0.3 --strict: a constraint supplied by only one merged declaration is
// effective in either source order and does not produce TS2428. Nested generic
// binders compare alpha-equivalently in both constraints and defaults; genuinely
// unequal constraint/default pairs produce one TS2428 per declaration.
interface ForwardConstraint<T> {}
interface ForwardConstraint<T extends string> {}
type ForwardConstraintBad = ForwardConstraint<1>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'

interface ReverseConstraint<T extends string> {}
interface ReverseConstraint<T> {}
type ReverseConstraintBad = ReverseConstraint<1>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'

type NestedHeaderShape<T> = { readonly value: T };

interface AlphaNestedConstraint<T extends NestedHeaderShape<<U>(value: U) => U>> {}
interface AlphaNestedConstraint<T extends NestedHeaderShape<<V>(value: V) => V>> {}

interface AlphaNestedDefault<T = NestedHeaderShape<<U>(value: U) => U>> {}
interface AlphaNestedDefault<T = NestedHeaderShape<<V>(value: V) => V>> {}

interface NestedHeaderConflict< // error[TK2428]: All declarations of 'NestedHeaderConflict' must have identical type parameters
  T extends NestedHeaderShape<<U>(value: U) => U> = NestedHeaderShape<<U>(value: U) => U>
> {}
interface NestedHeaderConflict< // error[TK2428]: All declarations of 'NestedHeaderConflict' must have identical type parameters
  T extends NestedHeaderShape<<V>(value: V) => readonly [V]> = NestedHeaderShape<<V>(value: V) => readonly [V]>
> {}
