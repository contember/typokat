// Backlog 76 — a hoisted var name with an initializer-inferred type is visible
// before its source declaration, but exact value-type resolution is deferred.
// tsc 6.0.3 --strict: TS2322 on the assignment below. This corpus stays disabled
// until lazy declaration/value-type resolution can recover `number` without
// evaluating the initializer or flow early.

function unannotatedForwardVar(): void {
  inferredVar = "bad"; // error[TK2322]: Type 'string' is not assignable to type 'number'

  {
    var inferredVar = 1;
  }
}
