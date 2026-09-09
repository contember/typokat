use super::super::check_project_programs_inner;
use super::super::events::{EventKey, UserRecordTicket};
use super::super::lexical_events::LexicalOwnerPhase;
use super::super::reporting_record::CheckerRecord;
use crate::check::test_support::on_raw_check_worker;
use crate::frontend::{run_project_frontend, FileInput};
use crate::source::{ModuleOrdinal, SourceOrdinal, SourceUnit, UnitSlot};
use std::cell::RefCell;

enum Owner<'a> {
    Initializer(&'a str),
    Callable(&'a str, LexicalOwnerPhase),
}

fn offset(source: &str, needle: &str) -> u32 {
    u32::try_from(source.find(needle).expect("unique owner source text")).expect("source offset")
}

fn assert_owners(source: &str, expected: &[(&str, &str, Owner<'_>)]) {
    on_raw_check_worker(|| {
        let keys = RefCell::new(Vec::new());
        let run = run_project_frontend(
            vec![FileInput {
                name: "/r2a/main.ts".to_owned(),
                source: source.to_owned(),
            }],
            |interner, units| {
                check_project_programs_inner(
                    interner,
                    units,
                    None,
                    None,
                    |_, reservations, _| {
                        let module = ModuleOrdinal::new(0);
                        let unit = SourceUnit::User {
                            module_ordinal: module,
                            unit_slot: UnitSlot::new(0),
                        };
                        let expected_keys = expected
                            .iter()
                            .map(|(_, _, owner)| {
                                let (start, ticket): (u32, UserRecordTicket) = match owner {
                                    Owner::Initializer(text) => {
                                        let start = offset(source, text);
                                        (
                                            start,
                                            reservations
                                                .initializer_owner_at(unit, start)
                                                .expect("initializer owner"),
                                        )
                                    }
                                    Owner::Callable(text, phase) => {
                                        let start = offset(source, text);
                                        (
                                            start,
                                            reservations
                                                .owner_at(
                                                    SourceOrdinal::User(module),
                                                    start,
                                                    *phase,
                                                )
                                                .expect("callable owner")
                                                .ticket,
                                        )
                                    }
                                };
                                EventKey {
                                    module_ordinal: module,
                                    source_start: start,
                                    event_ordinal: ticket.event.index(),
                                    record_ordinal: ticket.record_ordinal,
                                }
                            })
                            .collect::<Vec<_>>();
                        *keys.borrow_mut() = expected_keys;
                    },
                    |_, _, _, _, _, _, _| {},
                    |records| {
                        assert_eq!(records.len(), expected.len(), "{records:?}");
                        for ((code, text, _), key) in expected.iter().zip(keys.borrow().iter()) {
                            let matches = records
                                .iter()
                                .filter(|(actual_key, record)| {
                                    let CheckerRecord::Diagnostic(diagnostic) = record else {
                                        return false;
                                    };
                                    actual_key == key
                                        && diagnostic.code.as_str() == *code
                                        && &source[diagnostic.span.range()] == *text
                                })
                                .count();
                            assert_eq!(
                                matches, 1,
                                "expected {code} on {text:?} at {key:?}; got {records:?}"
                            );
                        }
                    },
                )
                .expect("check owner project")
            },
        );
        assert!(run.parse_errors.iter().all(Vec::is_empty));
        assert_eq!(run.product.len(), 1);
        assert!(
            run.product[0].incomplete.is_empty(),
            "{:?}",
            run.product[0].incomplete
        );
        assert_eq!(run.product[0].diagnostics.len(), expected.len());
    })
    .expect("owner worker");
}

#[test]
fn raw_and_contextual_defaults_keep_the_reserved_assignment_owner() {
    for declaration in [
        "const callback =",
        "const callback: (value?: number) => number =",
    ] {
        let source = format!("{declaration} (value: number = \"bad\") => value;");
        assert_owners(
            &source,
            &[("TK2322", "value", Owner::Initializer("\"bad\""))],
        );
    }
    let source = "const callback: (value?: number) => number = (value = \"bad\") => value;";
    assert_owners(
        source,
        &[("TK2322", "value", Owner::Initializer("\"bad\""))],
    );
}

#[test]
fn nested_default_argument_and_independent_body_have_distinct_exact_owners() {
    let source = r#"declare function number(value: number): number;
const callback: (value?: number) => number = (value = number("bad")) => {
    const body: string = 1;
    return value;
};"#;
    assert_owners(
        source,
        &[
            (
                "TK2345",
                "\"bad\"",
                Owner::Callable("(value = number", LexicalOwnerPhase::Body),
            ),
            ("TK2322", "body", Owner::Initializer("1;")),
        ],
    );
}

#[test]
fn selected_overload_discards_other_candidate_default_effects() {
    let source = r#"declare function choose<T = unknown>(tag: "no", callback: (value?: string) => string): void;
declare function choose<T = unknown>(tag: "yes", callback: (value?: number) => number): void;
choose("yes", (value = "bad") => { const body: string = 1; return value; });"#;
    assert_owners(
        source,
        &[
            ("TK2322", "value", Owner::Initializer("\"bad\"")),
            ("TK2322", "body", Owner::Initializer("1;")),
        ],
    );
}

#[test]
fn rejected_contextual_default_assignment_does_not_escape_candidate_rollback() {
    let source = r#"declare function choose<T = unknown>(tag: "no", callback: (value?: number) => number): void;
declare function choose<T = unknown>(tag: "yes", callback: (value?: string) => string): void;
choose("yes", (value = "valid") => { const body: string = 1; return value; });"#;
    assert_owners(source, &[("TK2322", "body", Owner::Initializer("1;"))]);
}

#[test]
fn rejected_overloads_release_only_raw_default_and_body_effects() {
    let source = r#"declare function choose(callback: (value?: number) => number): void;
declare function choose(callback: (value?: number) => boolean): void;
const result = choose((value: number = "bad") => { const body: string = 1; return "wrong"; });"#;
    assert_owners(
        source,
        &[
            ("TK2322", "value", Owner::Initializer("\"bad\"")),
            ("TK2322", "body", Owner::Initializer("1;")),
            (
                "TK2769",
                "(value: number = \"bad\") => { const body: string = 1; return \"wrong\"; }",
                Owner::Callable("result = choose", LexicalOwnerPhase::Deferred),
            ),
        ],
    );
}

#[test]
fn nested_selected_calls_do_not_duplicate_default_argument_effects() {
    let source = r#"declare function number(value: number): number;
declare function wrap<T>(callback: () => T): T;
declare function accept(callback: (value?: number) => number): number;
const result = wrap(() => accept((value: number = number("bad")) => { const body: string = 1; return value; }));"#;
    assert_owners(
        source,
        &[
            (
                "TK2345",
                "\"bad\"",
                Owner::Callable("(value: number = number", LexicalOwnerPhase::Body),
            ),
            ("TK2322", "body", Owner::Initializer("1;")),
        ],
    );
}

#[test]
fn unavailable_annotation_keeps_nested_default_effects_without_fake_assignment() {
    let source = r#"declare function number(value: number): number;
const callback: (value?: number) => number = (value: Missing = number("bad")) => { const body: string = 1; return 1; };"#;
    assert_owners(
        source,
        &[
            (
                "TK2304",
                "Missing",
                Owner::Callable("(value: Missing", LexicalOwnerPhase::Immediate),
            ),
            (
                "TK2345",
                "\"bad\"",
                Owner::Callable("(value: Missing", LexicalOwnerPhase::Body),
            ),
            ("TK2322", "body", Owner::Initializer("1;")),
        ],
    );
}

#[test]
fn repeated_checker_sessions_keep_default_and_body_counts() {
    use crate::binder::bind::ProjectBinderBuilder;
    use crate::binder::declaration::SourceDeclarationManifest;
    use crate::binder::namespace::{
        source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
    };
    use crate::binder::product::{project_library_declaration_product, LibraryProductSource};
    use crate::check::checker::project_local::{CheckerSession, ProjectSourceReexportPolicy};
    use crate::frontend::run_prepared_project_frontend;
    use crate::source::{CompilationOrigin, LibraryFileOrdinal};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use std::sync::Arc;

    on_raw_check_worker(|| {
        let allocator = Allocator::default();
        let library_source = "interface LibraryMarker {}";
        let library = Parser::new(&allocator, library_source, SourceType::d_ts()).parse();
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let manifest = SourceDeclarationManifest::build(&library.program);
        let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
        let unit = CompilationUnit {
            source: SourceUnitKey(1),
            origin: CompilationOrigin::Library(LibraryFileOrdinal::new(0)),
            binding: ModuleBindingContext::for_program(
                &library.program,
                source_file_kind("r2a.d.ts"),
            ),
        };
        let modules = builder
            .try_add_library_modules_with_declaration_manifests(&[(
                &library.program,
                &manifest,
                unit,
            )])
            .expect("library binding");
        let binder = builder.finish(modules[0]);
        let product = Arc::new(
            project_library_declaration_product(
                "r2a",
                &binder,
                &[LibraryProductSource {
                    ordinal: 0,
                    name: "r2a.d.ts",
                    source_sha256: "00",
                    byte_len: u32::try_from(library_source.len()).expect("library length"),
                    external_module: false,
                    module: modules[0],
                    manifest: &manifest,
                    source_text: library_source,
                }],
                &[],
            )
            .expect("library product"),
        );
        let source = r#"declare function number(value: number): number;
declare function accept(callback: (value?: number) => number): void;
accept((value = number("bad")) => { const body: string = 1; return value; });"#;
        let mut previous = None;
        for _ in 0..3 {
            let run = run_prepared_project_frontend(
                vec![FileInput {
                    name: "/r2a/main.ts".to_owned(),
                    source: source.to_owned(),
                }],
                |programs, reexports, defaults| {
                    CheckerSession::new(Arc::clone(&product)).check_project_programs(
                        programs,
                        ProjectSourceReexportPolicy::Admit(reexports),
                        defaults,
                    )
                },
            );
            assert!(run.parse_errors.iter().all(Vec::is_empty));
            let results = run
                .product
                .expect("frontend product")
                .expect("session check");
            assert_eq!(results.len(), 1);
            assert!(
                results[0].incomplete.is_empty(),
                "{:?}",
                results[0].incomplete
            );
            let diagnostics = results[0]
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    (
                        diagnostic.code.as_str(),
                        source[diagnostic.span.range()].to_owned(),
                        diagnostic.rendered_text(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                diagnostics
                    .iter()
                    .map(|(code, span, _)| (*code, span.as_str()))
                    .collect::<Vec<_>>(),
                [("TK2345", "\"bad\""), ("TK2322", "body")]
            );
            if let Some(previous) = &previous {
                assert_eq!(previous, &diagnostics);
            }
            previous = Some(diagnostics);
        }
    })
    .expect("session worker");
}

#[test]
fn single_const_return_keeps_published_readonly_source_identity() {
    for body in [
        "({ tag: \"a\" } as const)",
        "{ return { tag: \"a\" } as const; }",
    ] {
        let source = format!(
            r#"
const callback = (() => {body}) satisfies () => {{ tag: "a" | "b" }};
const result = callback();
const exact: "a" = result.tag;
result.tag = "a";
"#
        );
        let output = crate::check::test_support::check_source(&source);
        assert!(output.parse_errors.is_empty());
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        assert_eq!(output.diagnostics.len(), 1, "{:?}", output.diagnostics);
        assert_eq!(output.diagnostics[0].code.as_str(), "TK2540");
        assert_eq!(&source[output.diagnostics[0].span.range()], "tag");
    }
}

#[test]
fn contextual_return_literal_members_publish_actual_source_literals() {
    let contexts = [
        ("\"a\" | undefined", "\"a\"", None),
        ("\"a\" | null", "\"a\"", None),
        ("\"a\" | number", "\"a\"", None),
        ("1 | string", "1", None),
        ("boolean", "true", None),
        ("true | number", "true", None),
        ("string", "\"a\"", Some("string")),
        ("number", "1", Some("number")),
        ("\"a\" | string", "\"a\"", Some("string")),
        ("1 | number", "1", Some("number")),
        ("\"a\" | number", "1", Some("number")),
        ("1 | string", "\"a\"", Some("string")),
    ];
    for (context, value, widened) in contexts {
        for body in [value.to_owned(), format!("{{ return {value}; }}")] {
            let source = format!(
                "const callback = (() => {body}) satisfies () => {context};\nconst exact: {value} = callback();"
            );
            let output = crate::check::test_support::check_source(&source);
            assert!(
                output.parse_errors.is_empty(),
                "{source}: {:?}",
                output.parse_errors
            );
            assert!(
                output.incomplete.is_empty(),
                "{source}: {:?}",
                output.incomplete
            );
            match widened {
                None => assert!(
                    output.diagnostics.is_empty(),
                    "{source}: {:?}",
                    output.diagnostics
                ),
                Some(widened) => {
                    assert_eq!(
                        output.diagnostics.len(),
                        1,
                        "{source}: {:?}",
                        output.diagnostics
                    );
                    let diagnostic = &output.diagnostics[0];
                    assert_eq!(diagnostic.code.as_str(), "TK2322");
                    assert_eq!(&source[diagnostic.span.range()], "exact");
                    assert!(
                        diagnostic.message.contains(&format!("Type '{widened}'")),
                        "{source}: {diagnostic:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn contextual_container_literals_reject_invalid_downstream_copy_writes() {
    for (context, value, other) in [
        ("\"a\" | number", "\"a\"", "\"b\""),
        ("number | \"a\"", "\"a\"", "\"b\""),
        ("\"a\" | undefined", "\"a\"", "\"b\""),
        ("\"a\" | null", "\"a\"", "\"b\""),
        ("1 | string", "1", "2"),
        ("-1 | string", "-1", "-2"),
        ("true | number", "true", "false"),
    ] {
        for (initializer, access) in [
            (
                format!("{{ a: {value} }} satisfies {{ a: {context} }}"),
                "box.a",
            ),
            (
                format!("{{ a: {value} }} satisfies {{ a?: {context} }}"),
                "box.a",
            ),
            (format!("[{value}] satisfies ({context})[]"), "box[0]"),
            (format!("[{value}] satisfies [{context}]"), "box[0]"),
        ] {
            let source = format!(
                "const box = {initializer};\nlet copy = {access};\ncopy = {value};\ncopy = {other};"
            );
            let output = crate::check::test_support::check_source(&source);
            assert!(
                output.parse_errors.is_empty(),
                "{source}: {:?}",
                output.parse_errors
            );
            assert!(
                output.incomplete.is_empty(),
                "{source}: {:?}",
                output.incomplete
            );
            assert_eq!(
                output.diagnostics.len(),
                1,
                "{source}: {:?}",
                output.diagnostics
            );
            let diagnostic = &output.diagnostics[0];
            assert_eq!(diagnostic.code.as_str(), "TK2322");
            assert_eq!(&source[diagnostic.span.range()], other, "{source}");
            assert_eq!(
                diagnostic.span.range().start,
                source.rfind(other).unwrap(),
                "{source}"
            );
        }
    }
}

#[test]
fn contextual_container_returns_reject_invalid_downstream_copy_writes() {
    for (value, context, access, valid, invalid) in [
        (
            "({ a: \"a\" })",
            "{ a: \"a\" | number }",
            "callback().a",
            "\"a\"",
            "\"b\"",
        ),
        ("[1]", "(1 | string)[]", "callback()[0]", "1", "2"),
        ("[-1]", "[string | -1]", "callback()[0]", "-1", "-2"),
    ] {
        for body in [value.to_owned(), format!("{{ return {value}; }}")] {
            let source = format!(
                "const callback = (() => {body}) satisfies () => {context};\nlet copy = {access};\ncopy = {valid};\ncopy = {invalid};"
            );
            let output = crate::check::test_support::check_source(&source);
            assert!(
                output.parse_errors.is_empty(),
                "{source}: {:?}",
                output.parse_errors
            );
            assert!(
                output.incomplete.is_empty(),
                "{source}: {:?}",
                output.incomplete
            );
            assert_eq!(
                output.diagnostics.len(),
                1,
                "{source}: {:?}",
                output.diagnostics
            );
            let diagnostic = &output.diagnostics[0];
            assert_eq!(diagnostic.code.as_str(), "TK2322");
            assert_eq!(&source[diagnostic.span.range()], invalid, "{source}");
            assert_eq!(
                diagnostic.span.range().start,
                source.rfind(invalid).unwrap(),
                "{source}"
            );
        }
    }
}

#[test]
fn contextual_container_modes_keep_broad_and_reference_boundaries() {
    for (context, value, other) in [
        ("string", "\"a\"", "\"b\""),
        ("\"a\" | string", "\"a\"", "\"b\""),
        ("number", "1", "2"),
        ("1 | number", "1", "2"),
        ("1 | string", "\"a\"", "\"b\""),
        ("\"a\" | number", "1", "2"),
    ] {
        for (initializer, access) in [
            (
                format!("{{ a: {value} }} satisfies {{ a: {context} }}"),
                "box.a",
            ),
            (format!("[{value}] satisfies ({context})[]"), "box[0]"),
            (format!("[{value}] satisfies [{context}]"), "box[0]"),
        ] {
            let source = format!("const box = {initializer}; let copy = {access}; copy = {other};");
            let output = crate::check::test_support::check_source(&source);
            assert!(
                output.parse_errors.is_empty(),
                "{source}: {:?}",
                output.parse_errors
            );
            assert!(
                output.incomplete.is_empty(),
                "{source}: {:?}",
                output.incomplete
            );
            assert!(
                output.diagnostics.is_empty(),
                "{source}: {:?}",
                output.diagnostics
            );
        }
    }
    for (source, errors) in [
        (
            r#"const value = "a"; const box = { a: value } satisfies { a: string }; let copy = box.a; copy = "b";"#,
            0,
        ),
        (
            r#"const value = { a: "a" }; const box = { nested: value } satisfies { nested: { a: string } }; let copy = box.nested.a; copy = "b";"#,
            0,
        ),
        (
            r#"const value = ["a"]; const box = [value] satisfies string[][]; let copy = box[0][0]; copy = "b";"#,
            0,
        ),
        (
            r#"const value: "a" = "a"; const box = { a: value } satisfies { a: string }; let copy = box.a; copy = "b";"#,
            1,
        ),
        (
            r#"const box = { a: "a" as const } satisfies { a: "a" | number }; let copy = box.a; copy = "b";"#,
            1,
        ),
        (
            r#"class C { box = { a: "a" } satisfies { a: "a" | number }; } const c = new C(); let copy = c.box.a; copy = "b";"#,
            1,
        ),
    ] {
        let output = crate::check::test_support::check_source(source);
        assert!(
            output.parse_errors.is_empty(),
            "{source}: {:?}",
            output.parse_errors
        );
        assert!(
            output.incomplete.is_empty(),
            "{source}: {:?}",
            output.incomplete
        );
        assert_eq!(
            output.diagnostics.len(),
            errors,
            "{source}: {:?}",
            output.diagnostics
        );
        for diagnostic in output.diagnostics {
            assert_eq!(diagnostic.code.as_str(), "TK2322");
            assert_eq!(&source[diagnostic.span.range()], "\"b\"", "{source}");
        }
    }
}
