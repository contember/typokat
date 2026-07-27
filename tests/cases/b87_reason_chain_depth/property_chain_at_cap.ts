// Backlog 87 — the reason-chain depth cap must NOT fire at or below REASON_DEPTH_LIMIT (16).
// `A14` nests 14 `p` levels over `{ leaf }`, so the chain is exactly 16 elaboration
// levels: 14 `p` wrappers, the `leaf` wrapper at level 15, and the leaf mismatch at
// level 16. Every level renders in full — the pinned `leaf` wrapper is the
// discriminator: it is the first line an early cap would swallow into the elision.

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

declare const a14: A14;
const atCap: B14 = a14; // error[TK2322]: Types of property 'leaf' are incompatible.
