// Backlog 41 — a generic declaration default must satisfy its own constraint.
// Cross-checked with tsc 6.0.3 --strict.

declare function invalidDefault<T extends string = number>(): T; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
