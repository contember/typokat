// Backlog 77 — ReturnType extracts the return from a callable object even when
// it also has ordinary properties. Cross-checked with tsc 6.0.3 --strict.

type CallableObject = {
  (value: number): boolean;
  tag: string;
};

type ObjectReturn = ReturnType<CallableObject>;
const objectOk: ObjectReturn = true;
const objectBad: ObjectReturn = "wrong"; // error[TK2322]
