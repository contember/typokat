// Backlog 109 — optional acceptance must preserve contextual argument obligations.
// Cross-checked with tsc 6.0.3 --strict --noEmit.

function contextualObject(value?: { tag: "ok" }): void {}
contextualObject(undefined);
contextualObject({ tag: "bad" }); // error[TK2322]

function contextualArray(value?: { items: number[] }): void {}
contextualArray(undefined);
contextualArray({ items: ["bad"] }); // error[TK2322]

function contextualCallback(value?: (input: number) => string): void {}
contextualCallback(undefined);
contextualCallback((input) => input.toString());
contextualCallback((input) => true); // error[TK2345]: Argument of type '(input: number) => boolean' is not assignable to parameter of type '(input: number) => string'
