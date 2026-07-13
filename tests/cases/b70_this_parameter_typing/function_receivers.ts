// Backlog 70 — an explicit this parameter is a non-positional receiver slot:
// it types the body, constrains calls, and participates in function relation.
// Cross-checked with tsc 6.0.3 --strict.

function needsReceiver(this: { n: number }, value: number): string {
  const receiverNumber: number = this.n;
  this.n.missing(); // error[TK2339]
  return "ok";
}

needsReceiver(1); // error[TK2684]

const sameArity: (value: number) => string = needsReceiver;
const missingPositional: () => string = needsReceiver; // error[TK2322]

type WideReceiver = { tag: string };
type NarrowReceiver = { tag: "narrow" };

declare const acceptsWide: (this: WideReceiver, value: number) => void;
declare const acceptsNarrow: (this: NarrowReceiver, value: number) => void;

const relationOne: (this: NarrowReceiver, value: number) => void = acceptsWide;
const relationTwo: (this: WideReceiver, value: number) => void = acceptsNarrow; // error[TK2322]

declare const receiverless: (value: number) => void;
const receiverMayBeDropped: (value: number) => void = acceptsWide;
const receiverMayBeAdded: (this: WideReceiver, value: number) => void = receiverless;

declare const receiverMember: { method(this: { n: number }): void };
(receiverMember.method)(); // error[TK2684]
((receiverMember).method)(); // error[TK2684]

declare const compatibleReceiverMember: { n: number; method(this: { n: number }): void };
(compatibleReceiverMember.method)();
((compatibleReceiverMember).method)();
