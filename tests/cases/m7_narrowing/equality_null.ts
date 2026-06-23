// M7 — equality narrowing against null / undefined (`===` / `!==`).

function f(x: { a: number } | null) {
  if (x !== null) {
    const ok: { a: number } = x; // ok — null excluded in the then-branch
  }
}

function g(x: { a: number } | null) {
  if (x === null) {
    // x is null here
  } else {
    const ok: { a: number } = x; // ok — the else of `=== null` excludes null
  }
}

function h(y: { a: number } | undefined) {
  if (y !== undefined) {
    const ok: { a: number } = y; // ok — undefined excluded
  }
}
