// Surface-accounting spec (backlog 73). ENABLED by WU5: annotation lowering now records
// the incomplete surface for unmodeled `TSType` variants before degrading to the error
// type. See tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `lower_annotation_inner` had no `TSTypeQuery` arm, so `typeof Missing`
// lowered to `None` → the error type, and the unresolved value name was never reported.
// WU5 records `annotation-lower/type-query/typeof` first, so it is no longer false-clean.
// tsc 6.0.3 --strict: TS2304 "Cannot find name 'Missing'".

// INCOMPLETE: the `typeof` type-query annotation is not lowered — no diagnostic emitted.
let x: typeof Missing = 1; // incomplete[annotation-lower/type-query/typeof]

// CONTROL (supported): an unresolved TYPE reference is lowered and reported (M22).
let y: Missing = 1; // error[TK2304]: Cannot find name 'Missing'
