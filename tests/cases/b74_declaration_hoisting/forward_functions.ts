// Backlog 74 — ordinary and generic function declarations are callable before
// their source position. Cross-checked against tsc 6.0.3 --strict.

forwardOrdinary("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
forwardOrdinary(); // error[TK2554]: Expected 1 arguments, but got 0
const forwardReturn: string = forwardOrdinary(1); // error[TK2322]: Type 'number' is not assignable to type 'string'

function forwardOrdinary(value: number): number {
  return value;
}

forwardOrdinary("after"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

function genericContainer(): void {
  const inferredWrong: string = forwardGeneric(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
  forwardGeneric<string>(1); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
  forwardConstrained<boolean>(true); // error[TK2344]: Type 'boolean' does not satisfy the constraint 'string'

  function forwardGeneric<T>(value: T): T {
    return value;
  }

  function forwardConstrained<T extends string>(value: T): T {
    return value;
  }

  const inferredAfter: string = forwardGeneric(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
}

function mutuallyReferential(): void {
  first("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

  function first(value: number): number {
    return second(value);
  }

  function second(value: number): number {
    return first(value);
  }
}

// A switch CaseBlock is one lexical scope: a declaration in a later clause is
// visible to an earlier clause even though each consequent is checked separately.
function switchClauseForward(tag: number): void {
  switch (tag) {
    case 0:
      switchLater("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
      break;
    default:
      function switchLater(value: number): void {}
  }
}
