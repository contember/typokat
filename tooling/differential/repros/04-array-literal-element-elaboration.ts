// Found by: differential.py fuzz --ref tsc  (seed 3, index 25), minimized.
// A live typokat/tsc divergence, not a regression: a fresh ARRAY LITERAL argument with
// two mismatching elements draws ONE TK2345 on the argument from typokat and TWO
// TS2322 elaborations (one per element) from tsc 6.0.3. Both reject. The allowlist
// cancels the first pair by rule; the surviving TS2322 is what keeps this file here,
// so a change in either direction (typokat dropping the error, or growing per-element
// elaboration) shows up as a scoreboard diff.
declare function each1<T>(items: T[], step: (value: T) => void): void;
declare function each2(step: (value: { a: { b: number } }) => void): void;
declare function want3(xs: number[]): void;
each1([{ a: "s", b: 1 }], p0 => each2(p1 => want3([p0, p1])));
