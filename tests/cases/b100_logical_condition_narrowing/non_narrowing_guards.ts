// backlog 100, the soundness direction — compositions that must narrow NOTHING (or
// only the recognized half). Every line here is a place where a composed-condition fix
// could over-narrow and delete a real diagnostic.
// `??` stays outside the recognized-guard subset by design (`build_flow_logical`'s
// `LogicalOperator::Coalesce` arm), and a call is not a type predicate (owner: backlog
// 50), so neither may contribute a fact.
// `tsc 6.0.3 --strict --noEmit` reports exactly the marked lines.

declare function isReady(v: string | null): boolean;

// `??` composed as a condition: neither branch learns anything about `su`.
function coalesceCondition(su: string | undefined, flag: boolean) {
  if (su ?? flag) {
    const a: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  } else {
    const b: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  }
}

// `??` nested inside an `&&`: the recognized conjunct still narrows its own symbol, the
// `??` operand narrows nothing.
function coalesceInsideAnd(
  su: string | undefined,
  nn: number | null,
  flag: boolean,
) {
  if ((su ?? flag) && nn !== null) {
    const a: number = nn;
    const b: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  }
}

// A plain boolean-returning call is not a type predicate: it may not narrow its
// argument, in either operand position.
function callIsNotAPredicate(x: string | null, y: string | null) {
  if (isReady(y) && x !== null) {
    const a: string = x;
    const b: string = y; // error[TK2322]: Type 'null' is not assignable to type 'string'
  }
}

function callOnTheRight(x: string | null, y: string | null) {
  if (x !== null && isReady(y)) {
    const a: string = x;
    const b: string = y; // error[TK2322]: Type 'null' is not assignable to type 'string'
  }
}

// The composed fact is keyed to ONE symbol: a same-typed sibling stays wide.
function narrowingIsSymbolKeyed(x: string | null, y: string | null, flag: boolean) {
  if (x !== null && flag) {
    const a: string = x;
    const b: string = y; // error[TK2322]: Type 'null' is not assignable to type 'string'
  }
}

// An unrecognized comparison shape (the left side is not a narrowable identifier)
// composed with a recognized guard: only the recognized half may narrow.
function unrecognizedComparisonShape(
  su: string | undefined,
  nn: number | null,
) {
  if ((su ?? "fallback") !== "" && nn !== null) {
    const a: number = nn;
    const b: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  }
}

// The `else` of the same shape learns nothing at all.
function unrecognizedComparisonShapeElse(
  su: string | undefined,
  nn: number | null,
) {
  if ((su ?? "fallback") !== "" && nn !== null) {
  } else {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
    const b: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  }
}
