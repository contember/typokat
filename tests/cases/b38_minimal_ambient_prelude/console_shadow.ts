// Backlog 38 — a user value shadows the prelude value without duplicate-name
// noise, exactly as existing M28 prelude aliases do. Cross-checked with tsc 6.0.3 --strict.

export {};

declare const console: { log(value: number): void };

console.log("wrong"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
