//! WU1 regression tests for persistent generic signature lowering.

use crate::check::test_support::check_source;

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
fn static_method_class_binder_diagnostics_keep_all_five_exact_spans() {
    let source = "\
class StaticBinderSurface<T> {
  static capture<U extends T = T>(
    value: T,
  ): T {
    const bodyValue: T = value;
    return bodyValue;
  }
}
";
    let result = check_source(source);
    assert!(
        result.parse_errors.is_empty(),
        "unexpected parse error(s): {:?}",
        result.parse_errors
    );
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
    assert_eq!(result.diagnostics.len(), 5);
    for diagnostic in &result.diagnostics {
        assert_eq!(diagnostic.code.as_str(), "TK2302");
        assert_eq!(
            diagnostic.message,
            "Static members cannot reference class type parameters."
        );
        assert_eq!(&source[diagnostic.span.range()], "T");
    }

    let generic = source.find("U extends T = T").unwrap();
    let parameter = source.find("value: T").unwrap();
    let return_type = source.find("): T").unwrap();
    let body = source.find("bodyValue: T").unwrap();
    let expected = vec![
        generic + "U extends ".len()..generic + "U extends T".len(),
        generic + "U extends T = ".len()..generic + "U extends T = T".len(),
        parameter + "value: ".len()..parameter + "value: T".len(),
        return_type + "): ".len()..return_type + "): T".len(),
        body + "bodyValue: ".len()..body + "bodyValue: T".len(),
    ];
    let actual: Vec<std::ops::Range<usize>> = result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.span.range())
        .collect();
    assert_eq!(actual, expected);
    actual
        .windows(2)
        .for_each(|pair| assert_ne!(pair[0], pair[1]));
}

#[test]
fn class_diagnostics_replay_in_lexical_event_order() {
    let source = "\
const earlierNonClass: string = 1;
class DiagnosticOrderSurface {
  method<T extends string = number>(
    value: MissingParameter,
  ): MissingReturn {
    const bodyOnly: string = 2;
    return value;
  }
}
const laterNonClass: number = \"later\";
";
    let result = check_source(source);
    assert!(
        result.parse_errors.is_empty(),
        "unexpected parse error(s): {:?}",
        result.parse_errors
    );
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
    let lines = crate::span::LineIndex::new(source);
    let actual: Vec<(&str, u32, std::ops::Range<usize>, &str)> = result
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code.as_str(),
                lines.line_of(diagnostic.span.start),
                diagnostic.span.range(),
                &source[diagnostic.span.range()],
            )
        })
        .collect();

    let default_start = source.find("number>(").unwrap();
    let parameter_start = source.find("MissingParameter").unwrap();
    let return_start = source.find("MissingReturn").unwrap();
    let earlier_start = source.find("earlierNonClass").unwrap();
    let body_start = source.find("bodyOnly").unwrap();
    let later_start = source.find("laterNonClass").unwrap();
    let default = default_start..default_start + "number".len();
    let parameter = parameter_start..parameter_start + "MissingParameter".len();
    let return_type = return_start..return_start + "MissingReturn".len();
    let earlier = earlier_start..earlier_start + "earlierNonClass".len();
    let body = body_start..body_start + "bodyOnly".len();
    let later = later_start..later_start + "laterNonClass".len();
    let expected = vec![
        ("TK2322", 1, earlier, "earlierNonClass"),
        ("TK2344", 3, default, "number"),
        ("TK2304", 4, parameter, "MissingParameter"),
        ("TK2304", 5, return_type, "MissingReturn"),
        ("TK2322", 6, body, "bodyOnly"),
        ("TK2322", 10, later, "laterNonClass"),
    ];
    assert_eq!(actual, expected);
}

#[test]
fn class_type_parameter_headers_replay_each_default_after_its_constraint() {
    let source = "\
class HeaderOrder<
  First extends MissingHeader.C1 = MissingHeader.D1,
  Second extends MissingHeader.C2 = MissingHeader.D2,
> {}
";
    let result = check_source(source);
    assert!(
        result.parse_errors.is_empty(),
        "unexpected parse error(s): {:?}",
        result.parse_errors
    );
    let actual = result
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code.as_str(),
                diagnostic.span.range(),
                &source[diagnostic.span.range()],
            )
        })
        .collect::<Vec<_>>();
    let expected = [
        "MissingHeader.C1",
        "MissingHeader.D1",
        "MissingHeader.C2",
        "MissingHeader.D2",
    ]
    .into_iter()
    .map(|qualified| {
        let start = source
            .find(qualified)
            .expect("qualified header name exists");
        (
            "TK2503",
            start..start + "MissingHeader".len(),
            "MissingHeader",
        )
    })
    .collect::<Vec<_>>();
    assert_eq!(actual, expected);
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
fn intersection_constraints_normalize_deferred_conjuncts_before_relation() {
    let source = "\
interface WeakKeyTypes { object: object; }
type WeakKey = WeakKeyTypes[keyof WeakKeyTypes];
declare const weak: WeakKey;
interface A { x: number; }
interface Types { a: A; }
type AL = Types[keyof Types];
declare const al: AL;
const selected: A = al;
type Good = AL & { y?: never; };
declare const good: Good;
const direct: A = good;
interface Box<T extends AL> { value: T; }
declare function make<T extends AL & { y?: never }>(): Box<T>;
const boxed: Box<AL> = make();
";
    let result = check_source(source);
    assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
    assert!(
        result.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.incomplete.is_empty(),
        "unexpected incomplete records: {:?}",
        result.incomplete
    );
}

#[test]
fn deferred_intersection_constraint_normalizes_on_its_first_demand() {
    let source = "\
interface Buffer { byteLength: number; slice(): Buffer; }
interface BufferTypes { Buffer: Buffer; }
type BufferLike = BufferTypes[keyof BufferTypes];
interface View<T extends BufferLike> { buffer: T; }
interface ViewConstructor {
  new <T extends BufferLike & { marker?: never }>(buffer: T): View<T>;
}
";
    let result = check_source(source);
    assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
}

#[test]
fn normalized_intersection_members_cannot_hide_nested_family_conflicts() {
    let source = "\
interface NestedTypes { value: { a: number } & { b: number }; }
type Nested = NestedTypes[keyof NestedTypes];
type NestedSource = Nested & {};
declare const nested: NestedSource;
const nestedReject: { a?: string } = nested;
interface UnionTypes { value: { a: number } | {}; }
type SelectedUnion = UnionTypes[keyof UnionTypes];
type UnionSource = SelectedUnion & {};
declare const union: UnionSource;
const unionReject: { a?: string } = union;
";
    let result = check_source(source);
    assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        vec!["TK2322", "TK2322"]
    );
    assert!(
        result.incomplete.is_empty(),
        "unexpected incomplete records: {:?}",
        result.incomplete
    );
}

#[test]
fn keyof_object_and_empty_structural_object_are_both_never() {
    let source = "\
const objectKey: keyof object = 'x';
const emptyKey: keyof {} = 'x';
";
    let result = check_source(source);
    assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        vec!["TK2322", "TK2322"]
    );
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
}

#[test]
fn signature_default_rejects_later_binder_without_persisting_it() {
    let source = "\
declare function forwardDefault<T = U, U = string>(): T;
const fallbackMustStayUnknown: number = forwardDefault();
";
    let result = check_source(source);
    assert!(result.parse_errors.is_empty(), "unexpected parse errors");
    assert_eq!(diagnostic_lines(source), vec![(1, "TK2744".to_string())]);
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
fn contextual_callback_arguments_keep_argument_diagnostic_ownership() {
    let source = "\
declare function acceptsCallback(callback: (value: number) => number): void;
acceptsCallback((value: number) => \"wrong\");
const assignedCallback: (value: number) => number = (value: number) => \"wrong\";
";
    assert_eq!(
        diagnostic_lines(source),
        vec![(2, "TK2345".to_string()), (3, "TK2322".to_string())]
    );
}

#[test]
fn generic_rest_callback_and_tuple_rest_callbacks_receive_positional_context() {
    let source = "\
interface EventMap { pair: [number, string]; }
interface Client {
  on<K extends keyof EventMap>(event: K, listener: (...args: EventMap[K]) => void): void;
}
declare const client: Client;
declare function acceptsString(value: string): number;
client.on(\"pair\", (first, second) => acceptsString(second));
declare function callbacks(
  values: [(value: number) => number, ...((value: string) => number)[]],
): void;
callbacks([value => value, value => acceptsString(value), value => acceptsString(value)]);
";
    assert!(
        diagnostic_codes(source).is_empty(),
        "generic rest and tuple-rest callbacks must receive their positional parameter types"
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
