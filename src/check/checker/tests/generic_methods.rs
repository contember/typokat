//! WU1 regression tests for persistent generic signature lowering.

use crate::driver::check_source;

fn diagnostic_codes(source: &str) -> Vec<String> {
    let result = check_source(source);
    assert!(
        result.parse_errors.is_empty(),
        "unexpected parse error(s): {:?}",
        result.parse_errors
    );
    result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

fn diagnostic_lines(source: &str) -> Vec<(u32, String)> {
    let result = check_source(source);
    assert!(
        result.parse_errors.is_empty(),
        "unexpected parse error(s): {:?}",
        result.parse_errors
    );
    let line_index = crate::span::LineIndex::new(source);
    let mut diagnostics: Vec<(u32, String)> = result
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                line_index.line_of(diagnostic.span.start),
                diagnostic.code.as_str().to_string(),
            )
        })
        .collect();
    diagnostics.sort();
    diagnostics
}

#[test]
fn generic_class_and_static_method_signatures_lower_in_nested_frames() {
    let source = "\
class Box<T> {
  map<U>(value: U): U { return value; }
}
class Factory {
  static of<U>(value: U): U { return value; }
}
";
    assert!(
        diagnostic_codes(source).is_empty(),
        "generic method parameters must resolve in both surface and body"
    );
}

#[test]
fn generic_object_call_and_construct_signatures_lower_without_name_errors() {
    let source = "\
interface Methods { map<U>(value: U): U; }
type Callable = { <T>(value: T): T };
type Constructable = { new <T>(value: T): { value: T } };
";
    assert!(
        diagnostic_codes(source).is_empty(),
        "generic object signatures must retain their own parameter frames"
    );
}

#[test]
fn generic_signature_constraints_and_defaults_lower_persistently() {
    let source = "\
declare class Defaults<T> {
  value<U extends T = T>(): U;
}
";
    let result = check_source(source);
    assert!(result.parse_errors.is_empty(), "unexpected parse errors");
    assert!(
        result.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.incomplete.is_empty(),
        "signature defaults must no longer be recorded as incomplete"
    );
}

#[test]
fn invalid_signature_default_reports_its_constraint_without_incomplete() {
    let source = "declare function invalidDefault<T extends string = number>(): T;";
    let result = check_source(source);
    assert!(result.parse_errors.is_empty(), "unexpected parse errors");
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        vec!["TK2344"]
    );
    assert!(
        result.incomplete.is_empty(),
        "signature defaults must be validated, not recorded as incomplete"
    );
}

#[test]
fn signature_default_rejects_later_binder_without_persisting_it() {
    let source = "\
declare function forwardDefault<T = U, U = string>(): T;
const fallbackMustStayUnknown: number = forwardDefault();
";
    let result = check_source(source);
    assert!(result.parse_errors.is_empty(), "unexpected parse errors");
    assert_eq!(
        diagnostic_lines(source),
        vec![(1, "TK2744".to_string()), (2, "TK2322".to_string())]
    );
    assert!(
        result.incomplete.is_empty(),
        "a rejected signature default must not become an incomplete surface"
    );
}

#[test]
fn outer_substituted_method_binders_drive_constraints_and_defaults() {
    let source = "\
class Box<T> {
  map<U extends T>(value: U): U { return value; }
}
declare const numericBox: Box<number>;
const explicitNumber: number = numericBox.map<number>(1);
numericBox.map<string>(\"value\");
numericBox.map(\"value\");
declare class Defaults<T> { value<U = T>(): U; }
const defaultWithoutContext = new Defaults<number>().value();
const defaultIsOuterNumber: number = defaultWithoutContext;
const defaultIsNotString: string = defaultWithoutContext;
declare const pair: <T, U = T>(value: T) => U;
const explicitDefault: number = pair<number>(1);
";
    assert_eq!(
        diagnostic_lines(source),
        vec![
            (6, "TK2344".to_string()),
            (7, "TK2345".to_string()),
            (11, "TK2322".to_string()),
        ]
    );
}

#[test]
fn generic_function_and_constructor_annotations_instantiate_persistent_binders() {
    let source = "\
interface Box<T> { value: T; }
declare const identity: <T>(value: T) => T;
const functionInferred: number = identity(1);
const functionExplicit: string = identity<string>(\"value\");
const functionWrongReturn: string = identity(1);
identity<string>(1);
identity<number, string>(1);
declare const GenericBox: new <T>(value: T) => Box<T>;
const constructorInferred: Box<number> = new GenericBox(1);
const constructorExplicit: Box<string> = new GenericBox<string>(\"value\");
const constructorWrongReturn: Box<string> = new GenericBox(1);
new GenericBox<string>(1);
new GenericBox<number, string>(1);
";
    assert_eq!(
        diagnostic_lines(source),
        vec![
            (5, "TK2322".to_string()),
            (6, "TK2345".to_string()),
            (7, "TK2558".to_string()),
            (11, "TK2322".to_string()),
            (12, "TK2345".to_string()),
            (13, "TK2558".to_string()),
        ]
    );
}

#[test]
fn generic_class_overload_implementation_is_checked_under_aligned_binders() {
    let source = "\
class GenericOverloadProbe {
  select<T extends number>(value: T): number;
  select<T extends string>(value: T): string;
  select<T extends number | string>(value: T): number {
    return 1;
  }
}
";
    assert_eq!(diagnostic_lines(source), vec![(3, "TK2394".to_string())]);
}
