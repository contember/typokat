//! Production-shaped RED contract for backlog 105.
//!
//! An integration test compiles `typokat-binder` without `cfg(test)`, so this sees the shipped
//! ordering branch rather than the unit-test branch.

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use typokat_binder::binder::bind::ProjectBinderBuilder;
use typokat_binder::binder::namespace::{
    CompilationUnit, ModuleBindingContext, SourceFileKind, SourceUnitKey,
};
use typokat_core::source::{CompilationOrigin, LibraryFileOrdinal};

#[test]
#[ignore = "backlog 105 RED: production currently overwrites library-ordinal fragment order"]
fn crossed_source_keys_still_publish_library_fragments_in_registry_order() {
    let prelude_allocator = Allocator::default();
    let first_allocator = Allocator::default();
    let second_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let first = Parser::new(
        &first_allocator,
        "interface CanonicalType { first: number; }",
        SourceType::d_ts(),
    )
    .parse();
    let second = Parser::new(
        &second_allocator,
        "interface CanonicalType { second: string; }",
        SourceType::d_ts(),
    )
    .parse();
    assert!(prelude.diagnostics.is_empty());
    assert!(first.diagnostics.is_empty());
    assert!(second.diagnostics.is_empty());

    let first_unit = CompilationUnit {
        source: SourceUnitKey(900),
        origin: CompilationOrigin::Library(LibraryFileOrdinal::new(70)),
        binding: ModuleBindingContext::for_program(&first.program, SourceFileKind::DeclarationTs),
    };
    let second_unit = CompilationUnit {
        source: SourceUnitKey(100),
        origin: CompilationOrigin::Library(LibraryFileOrdinal::new(71)),
        binding: ModuleBindingContext::for_program(&second.program, SourceFileKind::DeclarationTs),
    };
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let modules = builder
        .try_add_library_modules(&[(&first.program, first_unit), (&second.program, second_unit)])
        .expect("crossed-key library batch binds");
    let binder = builder.finish(modules[1]);
    let symbol = binder
        .resolve_type(binder.compilation_global, "CanonicalType")
        .expect("reopened interface resolves");
    let group = binder
        .symbols
        .get(symbol)
        .and_then(|symbol| symbol.ty)
        .expect("reopened interface owns one type group");
    let sources = binder
        .type_groups
        .get(group)
        .expect("type group row")
        .fragments
        .iter()
        .map(|fragment| fragment.source)
        .collect::<Vec<_>>();

    assert_eq!(
        sources,
        [SourceUnitKey(900), SourceUnitKey(100)],
        "registry ordinals 70 then 71 define library fragment order"
    );
}
