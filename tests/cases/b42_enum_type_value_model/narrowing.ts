enum State {
  Ready = "ready",
  Running = "running",
  Done = "done",
}

function equality(state: State): void {
  if (state === State.Ready) {
    const ready: State.Ready = state;
    const wrong: State.Done = state; // error[TK2322]
  } else {
    const remaining: State.Running | State.Done = state;
    const wrong: State.Ready = state; // error[TK2322]
  }
}

function switched(state: State): void {
  switch (state) {
    case State.Ready: {
      const ready: State.Ready = state;
      const wrong: State.Done = state; // error[TK2322]
      break;
    }
    case State.Running: {
      const running: State.Running = state;
      break;
    }
    default: {
      const done: State.Done = state;
      const wrong: State.Ready = state; // error[TK2322]
    }
  }
}

function exhaustive(state: State): void {
  switch (state) {
    case State.Ready:
    case State.Running:
    case State.Done:
      break;
    default: {
      const unreachable: never = state;
    }
  }
}

function rawComparisons(state: State): void {
  if (state === "ready") {
    const ready: State.Ready = state;
  }

  switch (state) {
    case "running": {
      const running: State.Running = state;
      break;
    }
    case "done": {
      const done: State.Done = state;
      break;
    }
  }
}

enum NumericState {
  Zero,
  One,
}

enum OtherNumericState {
  Zero,
}

function invalidComparisons(state: NumericState): void {
  if (state === 99) {} // error[TK2367]
  if (NumericState.Zero === NumericState.One) {} // error[TK2367]
  if (NumericState.Zero === OtherNumericState.Zero) {} // error[TK2367]

  switch (state) {
    case 99: // error[TK2678]
      break;
  }
}
