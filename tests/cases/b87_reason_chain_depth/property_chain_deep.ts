// Backlog 87 — a chain far past the cap collapses to a constant number of lines:
// 16 rendered levels, one elision line, and the innermost cause. `A40` nests 40 `p`
// levels plus the `leaf` wrapper, so 41 - 16 = 25 levels are omitted. The elided count
// is exact, not approximate, so a reader can tell how much was dropped.
// The byte-count and line-width bounds this produces are pinned directly in
// `src/diagnostics/tests.rs` (a marker substring is trimmed, so it cannot assert indent).

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
interface A17 { p: A16; }
interface B17 { p: B16; }
interface A18 { p: A17; }
interface B18 { p: B17; }
interface A19 { p: A18; }
interface B19 { p: B18; }
interface A20 { p: A19; }
interface B20 { p: B19; }
interface A21 { p: A20; }
interface B21 { p: B20; }
interface A22 { p: A21; }
interface B22 { p: B21; }
interface A23 { p: A22; }
interface B23 { p: B22; }
interface A24 { p: A23; }
interface B24 { p: B23; }
interface A25 { p: A24; }
interface B25 { p: B24; }
interface A26 { p: A25; }
interface B26 { p: B25; }
interface A27 { p: A26; }
interface B27 { p: B26; }
interface A28 { p: A27; }
interface B28 { p: B27; }
interface A29 { p: A28; }
interface B29 { p: B28; }
interface A30 { p: A29; }
interface B30 { p: B29; }
interface A31 { p: A30; }
interface B31 { p: B30; }
interface A32 { p: A31; }
interface B32 { p: B31; }
interface A33 { p: A32; }
interface B33 { p: B32; }
interface A34 { p: A33; }
interface B34 { p: B33; }
interface A35 { p: A34; }
interface B35 { p: B34; }
interface A36 { p: A35; }
interface B36 { p: B35; }
interface A37 { p: A36; }
interface B37 { p: B36; }
interface A38 { p: A37; }
interface B38 { p: B37; }
interface A39 { p: A38; }
interface B39 { p: B38; }
interface A40 { p: A39; }
interface B40 { p: B39; }

declare const a40: A40;
const deepElision: B40 = a40; // error[TK2322]: ... 25 more nested levels omitted.
const deepLeaf: B40 = a40; // error[TK2322]: Type 'string' is not assignable to type 'number'.
