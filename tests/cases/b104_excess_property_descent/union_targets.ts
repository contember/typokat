// Backlog 104 — where the descent stops: it carries the target down unchanged, so
// it inherits whatever the excess check already does with that target. A union is
// exactly the boundary.
//
// `T | undefined` is unwrapped to `T` by `contextual_literal_target`, so the arms
// are checked and the excess is reported. A union with two or more object shapes
// is NOT (backlog `60`, `ledger/fresh-literal-union-targets`) — tsc picks the
// matching constituent and reports against it, typokat does not run the check at
// all. Those rows carry the diagnostics typokat actually produces (or none), never
// a marker for an error the descent does not deliver.
//
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x5.

interface Shape {
  kind: string;
}

interface Sa {
  a: string;
}

interface Sb {
  b: number;
}

type Circle = { kind: "circle"; radius: number };
type Square = { kind: "square"; size: number };
type Figure = Circle | Square;

declare const flag: boolean;
declare const maybeShape: Shape | undefined;

// `Shape | undefined` — one shape member, so the check runs on both arms.
const optionalTarget: Shape | undefined = flag ? { kind: "circle", extra: 1 } : undefined; // error[TK2353]
const optionalOperand: Shape | undefined = maybeShape ?? { kind: "circle", extra: 1 }; // error[TK2353]

// A two-shape union: tsc reports the excess against `Sa`. typokat runs no excess
// check against a multi-shape union (backlog `60`), and both arms width-subtype
// their constituent, so this row is a DROPPED error, not a passing one.
const twoShapeUnion: Sa | Sb = flag ? { a: "x", extra: 1 } : { b: 1 };

// A discriminated union: tsc reports the excess against `Circle`. typokat again
// skips the excess check, and the arms are additionally not contextually shaped
// against the union, so `kind` widens to `string` and the ordinary assignability
// error is what surfaces — the same verdict, a different reason.
const discriminated: Figure = flag ? { kind: "circle", radius: 1, extra: 1 } : { kind: "square", size: 2 }; // error[TK2322]
const discriminatedAlternate: Figure = flag ? { kind: "circle", radius: 1 } : { kind: "square", size: 2, extra: 1 }; // error[TK2322]
