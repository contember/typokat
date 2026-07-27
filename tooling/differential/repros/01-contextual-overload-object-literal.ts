// Found by: differential.py fuzz --ref <412f321 binary>  (seed 1, index 0), minimized.
// Pins: an overloaded consumer taking a FRESH OBJECT LITERAL whose property depends on
// the enclosing callback's contextually typed parameter. Under 412f321's raw-walk memo
// the phase-1 walk (parameter unbound, `any`) was served to the committed walk, which
// dropped TK2322 and invented TK2769. Backlog 95's second worked example.
declare function each1(step: (value: { a: number }) => void): void;
declare function over2(o: { a: string }): void;
declare function over2(o: { a: number }): void;
each1(p0 => over2({ a: p0 }));
