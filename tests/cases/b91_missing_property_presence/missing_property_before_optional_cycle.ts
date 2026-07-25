// ADR-0016 review finding — a cached failure must not change WHICH failure a later,
// logically independent statement reports.
//
// `N2 <: N0` fails twice over: the optional `p0` cannot be satisfied, and the required
// `p1` is absent. tsc reports the missing property (TS2741). typokat agrees whenever it
// decides `N2 <: N0` on a cold cache, because `relate_objects` walks target properties
// in sorted order and `p0`'s value relation *succeeds* through the assume-true fixpoint
// of §6.3 — so the walk reaches `p1` and reports it missing.
//
// Checking `const x` first leaves `p0`'s sub-relation in the durable cache as `false`
// (a verdict genuinely reached from that entry point). Once ADR-0016 made a cached
// `false` authoritative, `p0` now fails for `const y`, the walk stops there, and the
// headline becomes TK2322 instead. The verdict never flips — `N2 <: N0` is false either
// way — only the reported cause moves, and it moves because of an unrelated statement.
//
// The fix is a presence pass over required target properties BEFORE any value relation
// (tsc's `getUnmatchedProperty`), which makes `p1` decide the outcome regardless of what
// `p0` did. This file and its two siblings pin that the answer is the same in every
// statement order; they are the acceptance spec for that pass.

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

// Warms the cache: deciding `N1 <: N0` decides `p0`'s inner relation as `false`.
const x: N0 = a; // error[TK2322]

// Must stay the missing-property headline even though `const x` ran first.
const y: N0 = b; // error[TK2741]: Property 'p1' is missing in type
