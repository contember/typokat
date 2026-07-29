// Backlog 103 correctness tier, block-scoped collision control. `const JSON` collides with the
// library's own `JSON` value and must be processed in the private epoch without replacing the
// library meaning used by downstream reads.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2451 four times (three inside
// the library sources, once on the declaration below) plus TS2322 on both reads, because the
// library `JSON` value wins. typokat reproduces both TS2322s and has no TS2451 equivalent;
// that gap is ledgered in docs/reference/divergences.md under backlog 103.
const JSON = 1;

const jsonAsNumber: number = JSON; // error[TK2322]
const jsonAsString: string = JSON; // error[TK2322]
