// Backlog 102, the fail-closed half. A `var` whose name is already a default-library global
// targets a row inside the frozen prefix, which a delta may never mutate (ADR-0011). The write
// cannot be performed — but it must not vanish either: it is recorded at the declaration and
// surfaces as an incomplete outcome. Making the merge itself WORK is backlog 103.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2403 on the declaration
// ("Variable 'document' must be of type 'Document'") plus TS2322 on both reads, because the
// library `Document` type wins. typokat reproduces both TS2322s and has no TS2403 equivalent;
// that gap is ledgered in docs/reference/divergences.md under backlog 103.
declare var document: number; // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global

const documentAsNumber: number = document; // error[TK2322]
const documentAsString: string = document; // error[TK2322]
