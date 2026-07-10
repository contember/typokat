// Surface-accounting spec (backlog 73). ENABLED by WU4: a `for-in`/`for-of` whose left
// is a pre-declared assignment target (not a fresh `let`/`const`) is not re-typed, so
// the per-iteration assignability of the element to the existing binding is unchecked.
// `declare_for_left` records the incomplete surface before the drop. See
// tests/cases/README.md ("Surface-accounting corpus").
//
// tsc 6.0.3 --strict: TS2322 — `number` element is not assignable to `s: string`.

let s: string;
for (s of [1, 2, 3]) { // incomplete[stmt-check/assignment-target/self]
}

// CONTROL (supported): a fresh `const` target IS typed as the element type, so a bad
// use inside the body reports.
for (const n of [1, 2, 3]) {
  const bad: string = n; // error[TK2322]: Type 'number' is not assignable to type 'string'
}
