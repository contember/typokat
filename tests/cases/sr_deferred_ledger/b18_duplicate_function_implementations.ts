// Deferred ledger / backlog 18 — duplicate function implementations are not
// diagnosed, and the last implementation can replace the visible callable type.
// The duplicate TS2300/TS2393 records are the primary deferred family; this
// witness pins the independent TS2345 that must not disappear with them.
// Cross-checked against tsc 6.0.3 --strict. This corpus remains disabled.

function duplicateImplementation(value: number): number {
  return value;
}
function duplicateImplementation(value: string): string {
  return value;
}
var duplicateImplementation;
duplicateImplementation("acceptedOnlyByLast"); // error[TK2345]
