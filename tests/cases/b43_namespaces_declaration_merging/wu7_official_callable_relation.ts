// WU7 strict-tsc 6.0.3 oracle for callable rest/fixed relation parity.
// I3, I6D, I10C, and I17 mirror the official
// subtypingWithCallSignaturesWithRestParameters.ts cases.

interface OfficialRestRelationBase {
  onlyRest: (...args: number[]) => number;
  fixedThenRest: (x: number, ...rest: number[]) => number;
  optionalThenRest: (x: number, y?: string, ...rest: number[]) => number;
  optionalPrefixThenRest: (x?: number, y?: string, ...rest: number[]) => number;
}

interface OfficialI3Shape extends OfficialRestRelationBase {
  onlyRest: (x: number) => number;
}

interface OfficialI6DShape extends OfficialRestRelationBase {
  fixedThenRest: (x: number, y: number) => number;
}

interface OfficialI10CShape extends OfficialRestRelationBase { // error[TK2430]: Interface 'OfficialI10CShape' incorrectly extends interface 'OfficialRestRelationBase'
  optionalThenRest: (x: number, ...rest: number[]) => number;
}

interface OfficialI17Shape extends OfficialRestRelationBase { // error[TK2430]: Interface 'OfficialI17Shape' incorrectly extends interface 'OfficialRestRelationBase'
  optionalPrefixThenRest: (...args: number[]) => number;
}

// Absorbing surplus fixed parameters must still compare their types with the
// target rest element instead of becoming an unconditional arity exemption.
interface FixedAgainstRestBase {
  callable: (x: number, ...rest: number[]) => number;
}

interface IncompatibleFixedAgainstRest extends FixedAgainstRestBase { // error[TK2430]: Interface 'IncompatibleFixedAgainstRest' incorrectly extends interface 'FixedAgainstRestBase'
  callable: (x: number, y: string) => number;
}

// A source rest must cover every remaining target shape component: fixed
// prefix, variadic middle, and fixed suffix.
interface SourceRestAgainstTargetRestBase {
  callable: (x: number, ...rest: string[]) => number;
}

interface IncompatibleSourceRestAgainstTargetRest extends SourceRestAgainstTargetRestBase { // error[TK2430]: Interface 'IncompatibleSourceRestAgainstTargetRest' incorrectly extends interface 'SourceRestAgainstTargetRestBase'
  callable: (x: number, ...rest: number[]) => number;
}

interface SourceRestAgainstTargetSuffixBase {
  callable: (...args: [number, ...number[], string]) => number;
}

interface IncompatibleSourceRestAgainstTargetSuffix extends SourceRestAgainstTargetSuffixBase { // error[TK2430]: Interface 'IncompatibleSourceRestAgainstTargetSuffix' incorrectly extends interface 'SourceRestAgainstTargetSuffixBase'
  callable: (...args: number[]) => number;
}

// A source tuple-rest suffix remains a required positional obligation. It must
// neither disappear against a pure target rest nor be replaced by the source's
// variadic middle against a fixed target.
interface PureNumberRestTarget {
  callable: (...args: number[]) => number;
}

interface IncompatibleStringSuffixAgainstPureRest extends PureNumberRestTarget { // error[TK2430]: Interface 'IncompatibleStringSuffixAgainstPureRest' incorrectly extends interface 'PureNumberRestTarget'
  callable: (...args: [...number[], string]) => number;
}

interface CompatiblePureRestAgainstPureRest extends PureNumberRestTarget {
  callable: (...args: number[]) => number;
}

interface FixedNumberPairTarget {
  callable: (first: number, second: number) => number;
}

interface IncompatibleStringSuffixAgainstFixed extends FixedNumberPairTarget { // error[TK2430]: Interface 'IncompatibleStringSuffixAgainstFixed' incorrectly extends interface 'FixedNumberPairTarget'
  callable: (...args: [...number[], string]) => number;
}

interface CompatibleNumberSuffixAgainstFixed extends FixedNumberPairTarget {
  callable: (...args: [...number[], number]) => number;
}

// A target suffix moves with the supplied arity. The source must accept every
// possible target length, even when its parameter types are otherwise broad.
interface VariadicBeforeStringSuffixTarget {
  callable: (...args: [...number[], string]) => number;
}

interface IncompatibleRequiredPrefixAgainstMovingSuffix extends VariadicBeforeStringSuffixTarget { // error[TK2430]: Interface 'IncompatibleRequiredPrefixAgainstMovingSuffix' incorrectly extends interface 'VariadicBeforeStringSuffixTarget'
  callable: (first: unknown, ...rest: unknown[]) => number;
}

interface IncompatibleFiniteOptionalAgainstMovingSuffix extends VariadicBeforeStringSuffixTarget { // error[TK2430]: Interface 'IncompatibleFiniteOptionalAgainstMovingSuffix' incorrectly extends interface 'VariadicBeforeStringSuffixTarget'
  callable: (first?: unknown) => number;
}

interface CompatibleZeroFiniteAgainstMovingSuffix extends VariadicBeforeStringSuffixTarget {
  callable: () => number;
}

interface CompatibleOptionalPrefixAndRestAgainstMovingSuffix extends VariadicBeforeStringSuffixTarget {
  callable: (first?: unknown, ...rest: unknown[]) => number;
}

// A required suffix remains an obligation when the preceding variadic element
// is never; only a pure never rest retains its permissive fixed-source behavior.
interface NeverBeforeStringSuffixTarget {
  callable: (...args: [...never[], string]) => number;
}

interface IncompatibleFixedAgainstNeverAndSuffix extends NeverBeforeStringSuffixTarget { // error[TK2430]: Interface 'IncompatibleFixedAgainstNeverAndSuffix' incorrectly extends interface 'NeverBeforeStringSuffixTarget'
  callable: (value: string) => number;
}

interface PureNeverRestTarget {
  callable: (...args: never[]) => number;
}

interface CompatibleFixedAgainstPureNeverRest extends PureNeverRestTarget {
  callable: (value: string) => number;
}

// A required tuple-rest suffix still needs a later positional slot when an
// optional fixed prefix precedes it; the prefix cannot satisfy both positions.
interface SingleStringTarget {
  callable: (value: string) => number;
}

interface IncompatibleOptionalPrefixRequiredSuffix extends SingleStringTarget { // error[TK2430]: Interface 'IncompatibleOptionalPrefixRequiredSuffix' incorrectly extends interface 'SingleStringTarget'
  callable: (first?: unknown, ...rest: [...unknown[], string]) => number;
}

interface CompatibleOptionalPrefixPureRest extends SingleStringTarget {
  callable: (first?: unknown, ...rest: unknown[]) => number;
}

// A target tuple rest with both a stable prefix and a moving required suffix
// still exposes at least two arguments. A finite source that consumes one
// argument cannot accept the aggregate tail, while ignoring every argument or
// accepting a pure rest remains valid.
interface PrefixAndMovingSuffixTarget {
  callable: (...args: [unknown, ...unknown[], unknown]) => number;
}

interface IncompatibleFiniteConsumedPrefixAndSuffix extends PrefixAndMovingSuffixTarget { // error[TK2430]: Interface 'IncompatibleFiniteConsumedPrefixAndSuffix' incorrectly extends interface 'PrefixAndMovingSuffixTarget'
  callable: (value: unknown) => number;
}

interface CompatibleIgnorePrefixAndSuffix extends PrefixAndMovingSuffixTarget {
  callable: () => number;
}

interface CompatiblePureRestAgainstPrefixAndSuffix extends PrefixAndMovingSuffixTarget {
  callable: (...args: unknown[]) => number;
}

interface CompatibleOptionalPrefixRestAgainstPrefixAndSuffix extends PrefixAndMovingSuffixTarget {
  callable: (first?: unknown, ...rest: unknown[]) => number;
}

// Conversely, a source moving suffix cannot promise a final argument after a
// target fixed prefix plus pure rest. The same source is valid against one
// fixed argument, and a pure source rest accepts the variadic target.
interface FixedPrefixAndPureRestTarget {
  callable: (value: unknown, ...rest: unknown[]) => number;
}

interface IncompatibleMovingSourceSuffixAgainstFixedRest extends FixedPrefixAndPureRestTarget { // error[TK2430]: Interface 'IncompatibleMovingSourceSuffixAgainstFixedRest' incorrectly extends interface 'FixedPrefixAndPureRestTarget'
  callable: (...args: [...unknown[], unknown]) => number;
}

interface CompatiblePureSourceRestAgainstFixedRest extends FixedPrefixAndPureRestTarget {
  callable: (...args: unknown[]) => number;
}

interface FixedSingleUnknownTarget {
  callable: (value: unknown) => number;
}

interface CompatibleMovingSourceSuffixAgainstFixedSingle extends FixedSingleUnknownTarget {
  callable: (...args: [...unknown[], unknown]) => number;
}

// Strict tsc accepts both assignments below. typokat intentionally keeps these
// two pre-existing safe over-reports until the callable-rest parity tail owned
// by backlog 63 is addressed.
interface ZeroArityTarget {
  callable: () => number;
}

interface ConservativeMovingSuffixAgainstZero extends ZeroArityTarget { // error[TK2430]: Interface 'ConservativeMovingSuffixAgainstZero' incorrectly extends interface 'ZeroArityTarget'
  callable: (...args: [...unknown[], unknown]) => number;
}

interface OptionalSingleUnknownTarget {
  callable: (value?: unknown) => number;
}

interface ConservativeRequiredRestAgainstOptional extends OptionalSingleUnknownTarget { // error[TK2430]: Interface 'ConservativeRequiredRestAgainstOptional' incorrectly extends interface 'OptionalSingleUnknownTarget'
  callable: (value: unknown, ...rest: unknown[]) => number;
}
