// tsc 6.0.3 --strict --lib es5 --noEmit: TS2310 is owned by every interface
// binding in source order. An unresolved in-SCC heritage argument keeps its
// independent TS2304. General invalid-cycle recovery is not an oracle; the generic
// alias self-cycle below pins only its deterministic own/external members.

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

// Interface heritage SCCs are alias-transparent, including chained and generic aliases.
type DirectAliasCycle = DirectAliasSelf;
interface DirectAliasSelf extends DirectAliasCycle {} // error[TK2310]: Type 'DirectAliasSelf' recursively references itself as a base type
interface DirectAliasSelf { own: number }

type ChainedAliasCycleFirst = ChainedAliasSelf;
type ChainedAliasCycleSecond = ChainedAliasCycleFirst;
interface ChainedAliasSelf extends ChainedAliasCycleSecond {} // error[TK2310]: Type 'ChainedAliasSelf' recursively references itself as a base type
interface ChainedAliasSelf { own: number }

interface AliasCycleExternal<T> { external: T }
type GenericAliasCycle<T> = GenericAliasSelf<T>;
interface GenericAliasSelf<T> extends AliasCycleExternal<T>, GenericAliasCycle<T> { // error[TK2310]: Type 'GenericAliasSelf<T>' recursively references itself as a base type
  own: T;
}
declare const genericAliasSelf: GenericAliasSelf<number>;
const genericAliasOwnWrong: string = genericAliasSelf.own; // error[TK2322]: Type 'number' is not assignable to type 'string'
const genericAliasExternalWrong: string = genericAliasSelf.external; // error[TK2322]: Type 'number' is not assignable to type 'string'

type MutualAliasForwardToB = MutualAliasForwardB;
type MutualAliasForwardToA = MutualAliasForwardA;
interface MutualAliasForwardA extends MutualAliasForwardToB {} // error[TK2310]: Type 'MutualAliasForwardA' recursively references itself as a base type
interface MutualAliasForwardA { a: number } // error[TK2310]: Type 'MutualAliasForwardA' recursively references itself as a base type
interface MutualAliasForwardB extends MutualAliasForwardToA {} // error[TK2310]: Type 'MutualAliasForwardB' recursively references itself as a base type
interface MutualAliasForwardB { b: number } // error[TK2310]: Type 'MutualAliasForwardB' recursively references itself as a base type

type MutualAliasReverseToA = MutualAliasReverseA;
type MutualAliasReverseToB = MutualAliasReverseB;
interface MutualAliasReverseB extends MutualAliasReverseToA {} // error[TK2310]: Type 'MutualAliasReverseB' recursively references itself as a base type
interface MutualAliasReverseB { b: number } // error[TK2310]: Type 'MutualAliasReverseB' recursively references itself as a base type
interface MutualAliasReverseA extends MutualAliasReverseToB {} // error[TK2310]: Type 'MutualAliasReverseA' recursively references itself as a base type
interface MutualAliasReverseA { a: number } // error[TK2310]: Type 'MutualAliasReverseA' recursively references itself as a base type

// Alias-transparent topology forwards symbolic parameters/defaults and validates every
// group application before admitting a cycle edge.
type IdentityAlias<T> = T;
interface IdentitySelf extends IdentityAlias<IdentitySelf> {} // error[TK2310]: Type 'IdentitySelf' recursively references itself as a base type

type NestedIdentityAlias<T> = T;
interface NestedIdentitySelf extends NestedIdentityAlias<NestedIdentityAlias<NestedIdentitySelf>> {} // error[TK2310]: Type 'NestedIdentitySelf' recursively references itself as a base type

type DefaultIdentityAlias<T = DefaultIdentitySelf> = T;
interface DefaultIdentitySelf extends DefaultIdentityAlias {} // error[TK2310]: Type 'DefaultIdentitySelf' recursively references itself as a base type

type WrongAliasArity<T> = WrongAliasAritySelf;
interface WrongAliasAritySelf extends WrongAliasArity {} // error[TK2314]: Generic type 'WrongAliasArity' requires 1 type argument

type WrongFinalArityAlias<T> = WrongFinalAritySelf<T, T>; // error[TK2314]: Generic type 'WrongFinalAritySelf<T>' requires 1 type argument
interface WrongFinalAritySelf<T> extends WrongFinalArityAlias<T> {}

type UnresolvedArgumentAlias<T> = UnresolvedArgumentSelf<T>;
interface UnresolvedArgumentSelf<T> extends UnresolvedArgumentAlias<MissingArgument> {} // error[TK2310]: Type 'UnresolvedArgumentSelf<T>' recursively references itself as a base type | error[TK2304]: Cannot find name 'MissingArgument'

type QualifiedAlias<T> = QualifiedAliasCycle.Target<T>;
namespace QualifiedAliasCycle {
  export interface Target<T> extends QualifiedAlias<T> {} // error[TK2310]: Type 'Target<T>' recursively references itself as a base type
}

type IntersectionAliasCycle = IntersectionAliasSelf & { marker: number };
interface IntersectionAliasSelf extends IntersectionAliasCycle {} // error[TK2310]: Type 'IntersectionAliasSelf' recursively references itself as a base type

// Alias-transparent intersection summaries preserve absorber kind. `any` erases
// the recursive terminal, `never` remains an unsupported heritage form, and
// `unknown` is neutral so the recursive terminal remains visible.
type AnyHeritageAbsorber = any;
type AnyWrappedCycle = AnyHeritageAbsorber & AnyWrappedSelf;
interface AnyWrappedSelf extends AnyWrappedCycle {}

type NeverHeritageAbsorber = never;
type NeverWrappedCycle = NeverHeritageAbsorber & NeverWrappedSelf;
interface NeverWrappedSelf extends NeverWrappedCycle {} // incomplete[interface/heritage/topology]

type UnknownHeritageNeutral = unknown;
type UnknownWrappedCycle = UnknownHeritageNeutral & UnknownWrappedSelf;
interface UnknownWrappedSelf extends UnknownWrappedCycle {} // error[TK2310]: Type 'UnknownWrappedSelf' recursively references itself as a base type

// Empty terminal sets do not imply valid object heritage. Aliased unknown and
// primitive families retain TS2312 accounting instead of becoming false-clean.
type UnknownOnlyHeritageAlias = unknown;
interface UnknownOnlyHeritageDerived extends UnknownOnlyHeritageAlias {} // incomplete[interface/heritage/topology]

type StringHeritageAlias = string;
interface StringHeritageDerived extends StringHeritageAlias {} // incomplete[interface/heritage/topology]

type StringLiteralHeritageAlias = "literal";
interface StringLiteralHeritageDerived extends StringLiteralHeritageAlias {} // incomplete[interface/heritage/topology]

interface NoncyclicHeritageTerminal { value: number }
type StringIntersectionHeritageAlias = string & NoncyclicHeritageTerminal;
interface StringIntersectionHeritageDerived extends StringIntersectionHeritageAlias {} // incomplete[interface/heritage/topology]

type StringSelfIntersectionHeritageAlias = string & StringSelfIntersectionHeritageDerived;
interface StringSelfIntersectionHeritageDerived extends StringSelfIntersectionHeritageAlias {} // error[TK2310]: Type 'StringSelfIntersectionHeritageDerived' recursively references itself as a base type | incomplete[interface/heritage/topology]

// Valid empty/object-shaped controls remain accepted.
type AnyOnlyHeritageAlias = any;
interface AnyOnlyHeritageDerived extends AnyOnlyHeritageAlias {}

type TypeLiteralHeritageAlias = { value: number };
interface TypeLiteralHeritageDerived extends TypeLiteralHeritageAlias {}

type ObjectHeritageAlias = object;
interface ObjectHeritageDerived extends ObjectHeritageAlias {}

type PoisonedIntersectionAlias = PoisonedIntersectionSelf & MissingIntersectionBase; // error[TK2304]: Cannot find name 'MissingIntersectionBase'
interface PoisonedIntersectionSelf extends PoisonedIntersectionAlias {}

// Ordinary recursive members remain complete; only intra-SCC heritage edges are excluded.
interface RecursiveMemberControl<T> {
  value: T;
  next: RecursiveMemberControl<T>;
}

declare const recursiveMemberControl: RecursiveMemberControl<number>;
const recursiveMemberValue: number = recursiveMemberControl.next.value;
