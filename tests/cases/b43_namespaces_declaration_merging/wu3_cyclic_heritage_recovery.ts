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

// A singleton self-SCC diagnoses only the fragment carrying the self edge.
interface SelfMergedCycle extends SelfMergedCycle {} // error[TK2310]: Type 'SelfMergedCycle' recursively references itself as a base type
interface SelfMergedCycle { value: number }

// Every reopening fragment of a multi-group mutual SCC owns TS2310, even when
// that fragment has no heritage clause of its own.
interface MutualMergedA extends MutualMergedB {} // error[TK2310]: Type 'MutualMergedA' recursively references itself as a base type
interface MutualMergedA { a: number } // error[TK2310]: Type 'MutualMergedA' recursively references itself as a base type
interface MutualMergedB extends MutualMergedA {} // error[TK2310]: Type 'MutualMergedB' recursively references itself as a base type
interface MutualMergedB { b: number } // error[TK2310]: Type 'MutualMergedB' recursively references itself as a base type

// Ordinary recursive members remain complete; only intra-SCC heritage edges are excluded.
interface RecursiveMemberControl<T> {
  value: T;
  next: RecursiveMemberControl<T>;
}

declare const recursiveMemberControl: RecursiveMemberControl<number>;
const recursiveMemberValue: number = recursiveMemberControl.next.value;
