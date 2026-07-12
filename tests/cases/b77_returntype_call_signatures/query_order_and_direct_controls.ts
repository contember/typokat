// Backlog 77 — extracting one object-call return does not contaminate a second
// callable object; direct function ReturnType stays unchanged.

type BooleanCallable = { (value: number): boolean; kind: "boolean" };
type StringCallable = { (value: number): string; kind: "string" };

const booleanBad: ReturnType<BooleanCallable> = "wrong"; // error[TK2322]
const stringBad: ReturnType<StringCallable> = false; // error[TK2322]

type DirectFunction = (value: number) => boolean;
const directOk: ReturnType<DirectFunction> = true;
const directBad: ReturnType<DirectFunction> = "wrong"; // error[TK2322]
