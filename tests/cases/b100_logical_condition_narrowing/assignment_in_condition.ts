// backlog 100 x backlog 53 — assignments inside a composed condition.
// `build_flow_logical` deliberately JOINS the skipped operand's flow with the right
// operand's end so an assignment performed by the right operand survives the whole
// expression. A composed-condition fix must keep that join for the post-expression
// cursor while giving the guarded BRANCH the composed fact — dropping the join here
// silently deletes every diagnostic in this file.
// The `x = null;` line before each case is what installs the narrowed start state
// (a declaration initializer does not narrow in typokat; a real `=` does).
// `tsc 6.0.3 --strict --noEmit` reports exactly the marked lines.

declare function maybeString(): string | null;

// The shipped backlog-53 shapes: the assignment survives past the expression.
function andRhsAssignmentSurvives(flag: boolean) {
  let x: string | null = null;
  x = null;
  flag && (x = "s");
  const a: null = x; // error[TK2322]: Type 'string' is not assignable to type 'null'
}

function orRhsAssignmentSurvives(flag: boolean) {
  let x: string | null = null;
  x = null;
  flag || (x = "s");
  const a: null = x; // error[TK2322]: Type 'string' is not assignable to type 'null'
}

// The same, with the composed condition driving an `if`: after the statement the state
// is the join of both branches, so neither the assignment nor the skip may be lost.
function assignmentInAnIfCondition(flag: boolean) {
  let x: string | null = null;
  x = null;
  if (flag && (x = "s")) {
  }
  const a: null = x; // error[TK2322]: Type 'string' is not assignable to type 'null'
}

// Inside the branch the assignment HAS run and its value was truthy.
function assignedValueNarrowsTheBranch(flag: boolean) {
  let x: string | null = null;
  x = null;
  if (flag && (x = "s")) {
    const a: string = x;
  }
}

// The soundness case: the right operand reassigns the guarded variable, so the LEFT
// guard's fact is stale and may not survive into the branch.
function reassignmentInvalidatesTheGuard(x: string | null) {
  if (x !== null && (x = maybeString()) !== undefined) {
    const a: string = x; // error[TK2322]: Type 'null' is not assignable to type 'string'
  }
}

// The same shape with a literal assignment: the branch must read the ASSIGNED type,
// not the left guard's.
function reassignmentToNullWins(x: string | null) {
  if (x !== null && (x = null) === null) {
    const a: null = x;
    const b: string = x; // error[TK2322]: Type 'null' is not assignable to type 'string'
  }
}

// A later conjunct's guard sees the state an earlier conjunct's assignment left
// behind, and the post-`if` join still carries both senses.
function assignmentThenGuardInTheSameChain(flag: boolean) {
  let x: string | null = null;
  x = "seed";
  if (flag && (x = maybeString()) !== undefined && x !== null) {
    const a: string = x;
  }
  const b: null = x; // error[TK2322]: Type 'string' is not assignable to type 'null'
}
