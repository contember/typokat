// Deferred ledger / backlog 77 — ReturnType accepts represented callable objects,
// but conditional infer does not extract their return signatures and degrades to
// the permissive error type. Cross-checked with tsc 6.0.3 --strict.

type CallableObject = {
  (value: number): boolean;
  tag: string;
};

type ObjectReturn = ReturnType<CallableObject>;
const objectOk: ObjectReturn = true;
const objectDropped: ObjectReturn = "wrong"; // error[TK2322]

type Overloaded = {
  (value: string): number;
  (value: number): string;
};

type OverloadReturn = ReturnType<Overloaded>;
const overloadOk: OverloadReturn = "last";
const overloadDropped: OverloadReturn = 1; // error[TK2322]
