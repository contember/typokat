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
