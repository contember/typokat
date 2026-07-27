// Backlog 87 — the first chain that exceeds the cap. `A16` nests 16 `p` levels, so
// levels 1-16 render in full and the 17th (the `leaf` wrapper) is the first level
// beyond REASON_DEPTH_LIMIT. Exactly one wrapper is omitted, which pins the cap from
// above: a cap of 17 would render this chain whole and emit no elision line at all.
// The innermost cause is always retained after the elision, so the reader still learns
// what actually mismatched.

interface A0 { leaf: string; }
interface B0 { leaf: number; }
interface A1 { p: A0; }
interface B1 { p: B0; }
interface A2 { p: A1; }
interface B2 { p: B1; }
interface A3 { p: A2; }
interface B3 { p: B2; }
interface A4 { p: A3; }
interface B4 { p: B3; }
interface A5 { p: A4; }
interface B5 { p: B4; }
interface A6 { p: A5; }
interface B6 { p: B5; }
interface A7 { p: A6; }
interface B7 { p: B6; }
interface A8 { p: A7; }
interface B8 { p: B7; }
interface A9 { p: A8; }
interface B9 { p: B8; }
interface A10 { p: A9; }
interface B10 { p: B9; }
interface A11 { p: A10; }
interface B11 { p: B10; }
interface A12 { p: A11; }
interface B12 { p: B11; }
interface A13 { p: A12; }
interface B13 { p: B12; }
interface A14 { p: A13; }
interface B14 { p: B13; }
interface A15 { p: A14; }
interface B15 { p: B14; }
interface A16 { p: A15; }
interface B16 { p: B15; }

declare const a16: A16;
const firstElision: B16 = a16; // error[TK2322]: ... 1 more nested level omitted.
const retainsLeaf: B16 = a16; // error[TK2322]: Type 'string' is not assignable to type 'number'.
