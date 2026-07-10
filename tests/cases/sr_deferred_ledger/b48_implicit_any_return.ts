// Deferred ledger / backlog 48 — a method signature whose return-type
// annotation is omitted implicitly has an `any` return type; tsc --strict
// reports TS7010 (noImplicitAny family). typokat exits clean — a dropped error
// (under-report), surfaced by the 2026-07-10 WU7-F divergence-census review.
// This corpus stays DISABLED until backlog 48 ships the noImplicitAny family.
// Cross-checked vs tsc 6.0.3 --strict. Asserted code-only.

// witness (dropped error): omitted return annotation on a method signature —
// tsc: TS7010. typokat: silent.
interface I {
  m(); // error[TK7010]
}

// --- control: an annotated method signature stays clean. ---
interface Ok {
  m(): void;
}
