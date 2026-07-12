// Backlog 41 — generic declaration defaults validate their constraint and source
// order. Cross-checked with tsc 6.0.3 --strict.

declare function invalidDefault<T extends string = number>(): T; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
declare function forwardDefault<T = U, U = string>(): T; // error[TK2744]: Type parameter defaults can only reference previously declared type parameters
