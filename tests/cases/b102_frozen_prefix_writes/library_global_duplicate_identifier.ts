// Backlog 102, the fail-closed half — the cross-space duplicate shape. `isNaN` is a library
// function; redeclaring it as a `var` of a different type is a duplicate identifier for tsc.
// The delta cannot rewrite the frozen symbol row, so the write is recorded rather than dropped.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2300 twice (once in the
// library source, once on the declaration below) plus TS2322 on both reads, because the library
// `(number: number) => boolean` type wins. typokat reproduces both TS2322s and has no TS2300
// equivalent; that gap is ledgered in docs/reference/divergences.md under backlog 103.
declare var isNaN: number; // incomplete[bind/frozen-library-global/merge-refused]

const isNaNAsNumber: number = isNaN; // error[TK2322]: Type '(number: number) => boolean' is not assignable to type 'number'
const isNaNAsString: string = isNaN; // error[TK2322]: Type '(number: number) => boolean' is not assignable to type 'string'
