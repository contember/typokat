//! Regression specs for class members that reference default-library classes.

use typokat::driver::check_source;

// `/run/user/1000/fnm_multishells/1002937_1784884227968/bin/tsc --strict --target es2015
// --noEmit --pretty false <file>` is clean for both sources with TypeScript 6.0.3.
const NUMERIC_PROPERTY: &str = "class C { 1: Date; }\n";
const NUMERIC_INDEX_NEAR_MISS: &str = "class C { [x: number]: Date; }\n";

#[test]
fn numeric_property_with_library_class_type_stays_on_the_typed_result_channel() {
    let output = check_source(NUMERIC_PROPERTY)
        .expect("a user class dependency must not escape through the infrastructure channel");

    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
}

#[test]
fn numeric_index_signature_near_miss_returns_its_declared_incomplete() {
    let output = check_source(NUMERIC_INDEX_NEAR_MISS)
        .expect("the index-signature near miss must remain an ordinary typed result");

    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.incomplete.len(), 1, "{:?}", output.incomplete);
    assert_eq!(output.incomplete[0].id, "class/class-index-signature/self");
}
