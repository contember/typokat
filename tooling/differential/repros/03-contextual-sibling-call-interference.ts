// Found by: differential.py fuzz --ref <412f321 binary>  (seed 1, index 8), minimized.
// Pins a call that must stay CLEAN: `over7`'s second overload takes `() => number` and
// `p0.a` is a number, so line 12 is not an error. 412f321 reported TK2345 against the
// first overload — it had served the arrow's phase-1 walk, taken with `p0` unbound, to
// the committed walk. A false positive from the same memo that drops errors elsewhere.
//
// The `each2(p1 => {})` sibling is not decoration. The shrinker kept it because it is
// the one diagnostic both binaries agree on, which holds the exit code at 1 on both
// sides and keeps the signature exactly one diagnostic wide. Remove it and the finding
// becomes `dropped:TK2345 exit:1->0` — a different signature, which the shrink oracle
// refuses rather than reporting a neighbouring bug as the minimization of this one.
declare function each1(step: (value: { a: number }) => void): void;
declare function each2(step: (value: string[]) => string): void;
declare function over7(f: () => string): void;
declare function over7(f: () => number): void;
each1(p0 => {
  each2(p1 => {});
  over7(() => p0.a);
});
