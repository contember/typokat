// Backlog 38 follow-up — a same-space user value shadows ambient Math.
// Cross-checked with tsc 6.0.3 --strict.

export {};

declare const Math: { abs(value: string): string };

Math.abs(1); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
const shadowed: string = Math.abs("one");
