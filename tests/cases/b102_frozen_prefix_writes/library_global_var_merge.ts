// Backlog 103 correctness tier, incompatible `var` collision control. The declaration must be
// processed in the private epoch without replacing the library `document` meaning downstream.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2403 on the declaration
// ("Variable 'document' must be of type 'Document'") plus TS2322 on both reads, because the
// library `Document` type wins. typokat reproduces both TS2322s and has no TS2403 equivalent;
// that gap is ledgered in docs/reference/divergences.md under backlog 103.
declare var document: number;

const documentAsNumber: number = document; // error[TK2322]
const documentAsString: string = document; // error[TK2322]
