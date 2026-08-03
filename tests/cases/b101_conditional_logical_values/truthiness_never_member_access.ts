// Backlog 51 — a falsy boolean branch makes the nested `&&` RHS unreachable.
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2339 on both marked lines only.

function impossibleBooleanRhs(value: boolean) {
  return value ? 0 : (value && value.toString()); // error[TK2339]: Property 'toString' does not exist on type 'never'
}

// A reachable boolean RHS keeps the standard Boolean member.
function reachableBooleanRhs(value: boolean) {
  return value && value.toString();
}

// The direct form prevents a fix from special-casing the control-flow shape.
declare const impossible: never;
impossible.toString(); // error[TK2339]: Property 'toString' does not exist on type 'never'
