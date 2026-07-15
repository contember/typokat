// tsc 6.0.3 --strict --lib es5 --noEmit: TS2310 is owned by every interface
// binding in source order. An unresolved in-SCC heritage argument keeps its
// independent TS2304; invalid-cycle recovery after those diagnostics is not an oracle.

interface SameBinderForwardA<T> extends SameBinderForwardB<T> {} // error[TK2310]: Type 'SameBinderForwardA<T>' recursively references itself as a base type
interface SameBinderForwardB<T> extends SameBinderForwardA<T> {} // error[TK2310]: Type 'SameBinderForwardB<T>' recursively references itself as a base type

interface SameBinderReverseB<T> extends SameBinderReverseA<T> {} // error[TK2310]: Type 'SameBinderReverseB<T>' recursively references itself as a base type
interface SameBinderReverseA<T> extends SameBinderReverseB<T> {} // error[TK2310]: Type 'SameBinderReverseA<T>' recursively references itself as a base type

interface RenamedMissingForwardA<T> extends RenamedMissingForwardB<MissingForward> {} // error[TK2310]: Type 'RenamedMissingForwardA<T>' recursively references itself as a base type | error[TK2304]: Cannot find name 'MissingForward'
interface RenamedMissingForwardB<U> extends RenamedMissingForwardA<U> {} // error[TK2310]: Type 'RenamedMissingForwardB<U>' recursively references itself as a base type

interface RenamedMissingReverseB<U> extends RenamedMissingReverseA<U> {} // error[TK2310]: Type 'RenamedMissingReverseB<U>' recursively references itself as a base type
interface RenamedMissingReverseA<T> extends RenamedMissingReverseB<MissingReverse> {} // error[TK2310]: Type 'RenamedMissingReverseA<T>' recursively references itself as a base type | error[TK2304]: Cannot find name 'MissingReverse'

// Ordinary recursive members remain a complete generic surface; only heritage SCCs poison.
interface RecursiveMemberControl<T> {
  value: T;
  next: RecursiveMemberControl<T>;
}

declare const recursiveMemberControl: RecursiveMemberControl<number>;
const recursiveMemberValue: number = recursiveMemberControl.next.value;
