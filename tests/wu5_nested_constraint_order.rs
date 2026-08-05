//! RED contract for source-order diagnostics from nested constrained applications.

use typokat::driver::check_source;

// TypeScript 6.0.3 with `--strict --noEmit` reports the outer constraint before its
// nested argument constraint, while left-to-right sibling order stays unchanged.
const SOURCE: &str = r#"interface Base<T extends number> { v: T }
interface Outer<T extends Base<number>> { v: T }
declare const nestedBoth: Outer<Base<string>>;
declare const nestedInner: { x: Base<string> };
declare function callable(value: Outer<Base<string>>): void;
"#;

#[test]
fn nested_constraints_report_parent_before_child_in_source_order() {
    let output = check_source(SOURCE).expect("the production default library initializes");

    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code.as_str(), &SOURCE[diagnostic.span.range()],))
            .collect::<Vec<_>>(),
        [
            ("TK2344", "Base<string>"),
            ("TK2344", "string"),
            ("TK2344", "string"),
            ("TK2344", "Base<string>"),
            ("TK2344", "string"),
        ],
        "parent constraints must precede their nested child constraints"
    );
}
