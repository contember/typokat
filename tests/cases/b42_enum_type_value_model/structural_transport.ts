enum Mode {
  Read = "read",
  Write = "write",
}

function identity<T>(value: T): T {
  return value;
}

const inferredMember = identity(Mode.Read);
const inferredExact: Mode.Read = inferredMember;
let mutable = Mode.Read;
mutable = Mode.Write;
const mutableFlowExact: Mode.Write = mutable;

const array: Mode[] = [Mode.Read, Mode.Write];
const object: { mode: Mode } = { mode: Mode.Read };
const union: Mode | undefined = Mode.Write;
const impossibleIntersection: Mode.Read & "read" = Mode.Read; // error[TK2322]

const wrongArray: Mode[] = ["read"]; // error[TK2322]
const wrongObject: { mode: Mode } = { mode: "write" }; // error[TK2322]
const wrongUnion: Mode | undefined = "read"; // error[TK2322]

function acceptMode<T extends Mode>(value: T): T {
  return value;
}

acceptMode("read"); // error[TK2345]
