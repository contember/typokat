// ADR-0016 review finding, ordering 2 of 3 — the same two statements, swapped.
// See `missing_property_before_optional_cycle.ts` for the full analysis.
//
// Here `const y` is checked on a cold cache, so it already reports the missing property
// today. This file is the control: it must keep reporting exactly what the other two
// orderings report, which is what makes the trio a determinism pin rather than three
// unrelated assertions.

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

declare const a: N1;
declare const b: N2;

const y: N0 = b; // error[TK2741]: Property 'p1' is missing in type

const x: N0 = a; // error[TK2322]
