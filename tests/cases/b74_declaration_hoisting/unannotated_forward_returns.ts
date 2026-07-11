// Backlog 74 / 76 boundary — an unannotated function's parameter surface is
// hoisted, but its return is conservatively unknown until the declaration body
// is checked. Cross-checked against tsc 6.0.3 --strict.
//
// tsc reports TS2322 only for the void targets below. `narrowBefore` and
// `genericBefore` are deliberate safe-direction typokat over-reports until
// backlog 76 adds lazy declaration/value-type resolution. The void witnesses
// forbid the unsound provisional-void model, which would exit clean there.

const unknownBefore: unknown = inferredNumber();
const narrowBefore: number = inferredNumber(); // error[TK2322]
const voidBefore: void = inferredNumber(); // error[TK2322]

function inferredNumber() {
  return 1;
}

const numberAfter: number = inferredNumber();

const genericBefore: number = inferredGeneric(1); // error[TK2322]

function inferredGeneric<T>(value: T) {
  return value;
}

const genericAfter: number = inferredGeneric(1);

function dependentBody(): void {
  return laterNumber(); // error[TK2322]

  function laterNumber() {
    return 1;
  }
}
