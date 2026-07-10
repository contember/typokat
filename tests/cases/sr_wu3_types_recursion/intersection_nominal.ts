// WU3 / finding 8 — when the SOURCE of an assignability check is an intersection,
// private/protected members are related structurally because `nominal_origin_ok`
// is skipped on the intersection path (objects.rs:775). So a plain structural
// intersection is wrongly accepted where the target class has a private member.
// tsc rejects (a class with a private member requires the same declaration).
// DISABLED at HEAD; enabling exposes the missing TK2322. Cross-checked vs
// tsc 6.0.3 --strict (TS2322, "Property 'p' is private …"; asserted code-only
// because the rejection renders both sides structurally — see divergences).

class C {
  private p: number = 1;
}

declare const other: { p: number } & { q: number };

// witness: structural intersection is NOT the class C (private `p`).
const bad: C = other; // error[TK2322]

// --- controls ---
// same-class value stays accepted.
const good: C = new C();
// a structural (non-nominal) target of the same intersection source is fine.
const pub: { p: number } = other;
