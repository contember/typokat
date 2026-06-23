// M7 — narrowing enables member access that is otherwise rejected on the union.
// On `{ a: number } | null`, `.a` is not present on every member (null lacks it)
// so typokat reports TK2339; after narrowing out null, the access is fine.
// (Real tsc reports TK18047 "possibly null" here; that refinement is deferred —
// the point of this fixture is that narrowing removes the diagnostic.)

function f(x: { a: number } | null) {
  const bad = x.a; // error[TK2339]
  if (x !== null) {
    const ok: number = x.a; // ok — narrowed to { a: number }
  }
}

function g(x: { a: number } | null) {
  if (x) {
    const ok: number = x.a; // ok — truthy narrowing then member access
  }
}
