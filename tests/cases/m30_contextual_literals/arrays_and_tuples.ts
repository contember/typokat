// M30 — array and tuple literals use contextual element targets in declaration,
// argument, and return positions.

const literalArrayOk: ("a" | "b")[] = ["a", "b"];
const literalArrayBad: ("a" | "b")[] = ["a", "c"]; // error[TK2322]

declare function takesFlags(flags: (1 | 2)[]): void;
takesFlags([1, 2]);
takesFlags([1, 3]); // error[TK2345]

declare function takesPair(pair: [1, "x"]): void;
takesPair([1, "x"]);
takesPair([2, "x"]); // error[TK2345]

function pairOk(): [1, "x"] {
  return [1, "x"];
}

function pairBad(): [1, "x"] {
  return [1, "y"]; // error[TK2322]
}

type Board = { cells: [1, 2] };

const boardOk: Board = { cells: [1, 2] };
const boardBad: Board = { cells: [1, 3] }; // error[TK2322]

// No target context: array literals still infer widened array element types.
const inferredArray = ["a"];
const inferredArrayElement: "a" = inferredArray[0]; // error[TK2322]
