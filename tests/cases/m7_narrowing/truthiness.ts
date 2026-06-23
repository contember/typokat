// M7 — truthiness narrowing removes null / undefined from a union in the
// then-branch of `if (x)`. (We use object-or-null unions so the then-branch is
// unambiguously the object — objects are always truthy.)

function f(x: { a: number } | null) {
  const wide: { a: number } = x; // error[TK2322]
  if (x) {
    const ok: { a: number } = x; // ok — truthy removes null
  }
}

function g(y: { a: number } | undefined) {
  if (y) {
    const ok: { a: number } = y; // ok — truthy removes undefined
  }
}

// `!x` flips the branches: the else-branch is the truthy (object) one.
function h(z: { a: number } | null) {
  if (!z) {
    // z is null here
  } else {
    const ok: { a: number } = z; // ok — else of `!z` is truthy
  }
}
