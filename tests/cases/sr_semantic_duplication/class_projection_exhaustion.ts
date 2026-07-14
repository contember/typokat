// Semantic-duplication architecture gate — public class-projection exhaustion boundaries.
// tsc 6.0.3 --strict accepts the infinitely matching structural pairs through its recursive cutoff;
// it reports the deliberate sibling/early primitive mismatches and candidate observability control.
// typokat reaches its bounded
// frontier first in the exhaustion-before-sibling case and owns one projection-budget record.

class ExhaustionSource<T> {
  next!: ExhaustionSource<T[]>;
}

class ExhaustionTarget<T> {
  next!: ExhaustionTarget<T[]>;
}

declare const exhaustionSource: ExhaustionSource<string>;

const exhaustedAssignment: ExhaustionTarget<string> = exhaustionSource; // error[TK2322] | incomplete[relation/class-projection-budget]: class projection budget exhausted

function acceptsExhaustionTarget(value: ExhaustionTarget<string>): void {}
acceptsExhaustionTarget(exhaustionSource); // error[TK2345] | incomplete[relation/class-projection-budget]: class projection budget exhausted

function returnsExhaustionTarget(): ExhaustionTarget<string> {
  return exhaustionSource; // error[TK2322] | incomplete[relation/class-projection-budget]: class projection budget exhausted
}

// A deferred override cannot turn exhaustion into a clean compatibility verdict.
class ExhaustedOverrideBase {
  recursive!: ExhaustionTarget<string>;
}

class ExhaustedOverrideDerived extends ExhaustedOverrideBase {
  declare recursive: ExhaustionSource<string>; // error[TK2416]: Property 'recursive' in type 'ExhaustedOverrideDerived' is not assignable to the same property in base type 'ExhaustedOverrideBase' | incomplete[relation/class-projection-budget]: class projection budget exhausted
}

// Identity succeeds without expanding the non-regular spine.
const samePairDemand: ExhaustionSource<string> = exhaustionSource;

class ExhaustionBeforeMismatchSource<T> {
  aNext!: ExhaustionBeforeMismatchSource<T[]>;
  zKind!: "source";
}

class ExhaustionBeforeMismatchTarget<T> {
  aNext!: ExhaustionBeforeMismatchTarget<T[]>;
  zKind!: "target";
}

declare const exhaustionBeforeMismatch: ExhaustionBeforeMismatchSource<string>;
const exhaustedBeforeSiblingMismatch: ExhaustionBeforeMismatchTarget<string> = exhaustionBeforeMismatch; // error[TK2322] | incomplete[relation/class-projection-budget]: class projection budget exhausted

class MismatchBeforeExhaustionSource<T> {
  aKind!: "source";
  zNext!: MismatchBeforeExhaustionSource<T[]>;
}

class MismatchBeforeExhaustionTarget<T> {
  aKind!: "target";
  zNext!: MismatchBeforeExhaustionTarget<T[]>;
}

declare const mismatchBeforeExhaustion: MismatchBeforeExhaustionSource<string>;
const earlyMismatchWins: MismatchBeforeExhaustionTarget<string> = mismatchBeforeExhaustion; // error[TK2322]

interface WinnerBeforeExhaustedCandidate {
  (value: ExhaustionSource<string>): "winner";
  (value: ExhaustionTarget<string>): "later";
}

declare const winnerBeforeExhaustedCandidate: WinnerBeforeExhaustedCandidate;
const earlyWinner: "winner" = winnerBeforeExhaustedCandidate(exhaustionSource);

interface ExhaustedCandidateBeforeWinner {
  (value: ExhaustionTarget<string>): "exhausted";
  (value: ExhaustionSource<string>): "winner";
}

declare const exhaustedCandidateBeforeWinner: ExhaustedCandidateBeforeWinner;
const exhaustedCandidate: "impossible" = exhaustedCandidateBeforeWinner(exhaustionSource); // incomplete[relation/class-projection-budget]: class projection budget exhausted

declare function inferAfterExhaustedCandidate<T>(value: ExhaustionTarget<T>, fallback: T): T;
const inferredAfterExhaustion: string = inferAfterExhaustedCandidate(exhaustionSource, "fallback"); // error[TK2345] | incomplete[relation/class-projection-budget]: class projection budget exhausted
