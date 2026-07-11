// Backlog 74 — multiple `var` declarations share one binding, but a later
// annotation must not retroactively contextualize an earlier initializer or move
// its annotation records. Cross-checked against tsc 6.0.3 --strict. TS2403
// redeclaration diagnostics remain backlog 18; the independent TK2322 is required.

var conflicting = "bad";
var conflicting: number;
const conflictingRead: number = conflicting; // error[TK2322]: Type 'string' is not assignable to type 'number'

var stagedOnce = 1;
var stagedOnce: MissingVarAnnotation; // error[TK2304]: Cannot find name 'MissingVarAnnotation'

function parameterCollision(parameterValue: string): void {
  parameterValue = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
  var parameterValue: number;
}

function callableCollision(value: number): void {}
var callableCollision: number;
callableCollision("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
