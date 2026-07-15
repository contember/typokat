use typokat::diagnostics::DiagnosticCode;
use typokat::driver::check_source;

#[test]
fn exhausted_class_default_constraint_does_not_fabricate_tk2344() {
    let mut source = String::new();
    for prefix in ['A', 'B'] {
        for index in 0..130 {
            if index == 129 {
                source.push_str(&format!("class {prefix}{index} {{ value!: string }}\n"));
            } else {
                source.push_str(&format!(
                    "class {prefix}{index} {{ next!: {prefix}{} }}\n",
                    index + 1
                ));
            }
        }
    }
    source.push_str("class C { method<T extends A0 = B0>(): void {} }\n");

    let result = check_source(&source);
    assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
    assert!(result
        .incomplete
        .iter()
        .any(|record| record.id == "relation/class-projection-budget"));
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != DiagnosticCode::TK2344),
        "an exhausted relation is not proof that the default violates its constraint"
    );
}
