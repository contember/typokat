// tsc 6.0.3 --strict: a constraint supplied by only one merged declaration is
// effective in either source order and does not produce TS2428.
interface ForwardConstraint<T> {}
interface ForwardConstraint<T extends string> {}
type ForwardConstraintBad = ForwardConstraint<1>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'

interface ReverseConstraint<T extends string> {}
interface ReverseConstraint<T> {}
type ReverseConstraintBad = ReverseConstraint<1>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
