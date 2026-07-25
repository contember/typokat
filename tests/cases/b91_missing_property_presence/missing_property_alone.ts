// ADR-0016 review finding, ordering 3 of 3 — the cache-warming statement deleted.
// See `missing_property_before_optional_cycle.ts` for the full analysis.
//
// Nothing populates the relation cache before `const y`, so this is the reference
// answer the other two orderings must match. It is also what `tsc --strict` reports.

interface N0 {
  p0?: N2 & N0;
  p1: N0;
}
interface N1 {
  p0: N1;
  p1: N2 | N0;
}
interface N2 {
  p0: N1;
}

declare const b: N2;

const y: N0 = b; // error[TK2741]: Property 'p1' is missing in type
