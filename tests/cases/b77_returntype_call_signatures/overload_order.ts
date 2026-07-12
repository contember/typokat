// Backlog 77 — ReturnType uses the last represented overload signature, preserving
// source order instead of selecting the first or unioning every return.

type ForwardOverload = {
  (value: string): number;
  (value: number): string;
};

type ForwardReturn = ReturnType<ForwardOverload>;
const forwardOk: ForwardReturn = "last";
const forwardBad: ForwardReturn = 1; // error[TK2322]

type ReverseOverload = {
  (value: number): string;
  (value: string): number;
};

type ReverseReturn = ReturnType<ReverseOverload>;
const reverseOk: ReverseReturn = 1;
const reverseBad: ReverseReturn = "first"; // error[TK2322]
