// Backlog 102, the fail-closed half — the block-scoped shape. `const JSON` collides with the
// library's own `JSON` value; the delta cannot rewrite the frozen symbol row, so the write is
// recorded rather than dropped. The redeclaration diagnostic itself is backlog 103's.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2451 four times (three inside
// the library sources, once on the declaration below) plus TS2322 on both reads, because the
// library `JSON` value wins. typokat reproduces both TS2322s and has no TS2451 equivalent;
// that gap is ledgered in docs/reference/divergences.md under backlog 103.
const JSON = 1; // incomplete[bind/frozen-library-global/merge-refused]

const jsonAsNumber: number = JSON; // error[TK2322]
const jsonAsString: string = JSON; // error[TK2322]
