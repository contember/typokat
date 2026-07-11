// Backlog 38 — basic non-generic numeric Math methods through the ambient prelude.
// Cross-checked with tsc 6.0.3 --strict.

const absolute: number = Math.abs(-1);
const maximum: number = Math.max(1, 2, 3);
const random: number = Math.random();

Math.abs("wrong"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
Math.max(1, "wrong"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
const wrongResult: string = Math.ceil(1); // error[TK2322]: Type 'number' is not assignable to type 'string'

Math.missing(2, 3); // error[TK2339]: Property 'missing' does not exist
Math.abs(); // error[TK2554]: Expected 1 arguments, but got 0
Math.abs(1, 2); // error[TK2554]: Expected 1 arguments, but got 2
