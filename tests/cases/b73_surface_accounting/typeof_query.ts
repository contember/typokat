// Surface-accounting spec (backlog 73). DISABLED until WU2 and annotation-lowering
// wiring (WU5) land. See tests/cases/README.md ("Surface-accounting corpus").
//
// Silent skip: `lower_annotation_inner` has no `TSTypeQuery` arm
// (src/check/checker/annotations/mod.rs:174 `_ => return None`), so `typeof Missing`
// lowers to `None` → the error type, and the unresolved value name is never reported.
// tsc 6.0.3 --strict: TS2304 "Cannot find name 'Missing'".

// INCOMPLETE: the `typeof` type-query annotation is not lowered — currently 0 diagnostics.
let x: typeof Missing = 1; // incomplete[annotation-lower/type-query/typeof]

// CONTROL (supported): an unresolved TYPE reference is lowered and reported (M22).
let y: Missing = 1; // error[TK2304]: Cannot find name 'Missing'
