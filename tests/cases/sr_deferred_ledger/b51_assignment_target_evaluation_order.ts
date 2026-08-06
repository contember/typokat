// Deferred ledger / backlog 51 — assignment-target side effects run before the
// right-hand side. typokat currently checks the RHS first and drops tsc's TS2339.
// This corpus stays disabled until the flow graph preserves that evaluation order.
// Cross-checked against tsc 6.0.3 --strict. Asserted code-only.

let value: string | number;
let target: any;

value = 1;
(value = "", target).property = value.toExponential(); // error[TK2339]

// Control: without the target-side assignment, the number method stays valid.
value = 1;
target.property = value.toExponential();
