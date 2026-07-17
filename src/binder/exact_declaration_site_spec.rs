//! Disabled RED spec for the binder-owned exact declaration-site index.
//!
//! Activate with `#[cfg(test)] mod exact_declaration_site_spec;` in `binder/mod.rs`
//! after [`Binder`] retains `BindState::declarations_by_site` at `finish` and exposes
//! this minimal crate-private API:
//!
//! ```ignore
//! pub(crate) fn exact_declaration_at(
//!     &self,
//!     syntax_module: ScopeId,
//!     binding_start: u32,
//!     kind: DeclarationKind,
//! ) -> Option<&LexicalDeclaration>;
//! ```
//!
//! The key is the syntax-owning module scope, binding-leaf start, and declaration kind.
//! The returned row owns the canonical [`super::declaration::DeclId`], exact spans, lexical scope,
//! and independent storage/group identities. A source-prewalk occurrence is admitted only after
//! semantic binding attaches its lexical scope; an unattached inventory row returns `None`.
//!
//! The implementation contract is one retained binder hash index followed by dense declaration-
//! table lookup. It must not resolve through the compilation-global scope, declaration name,
//! `SourceUnitKey`, group order, or hash iteration. Checker cutover deletes
//! `exact_type_fragment_at` and replaces the scan inside `visit_bound_type` with this API; the
//! checker may retain neither a full-scan fallback nor a duplicate exact-site index.

use super::bind::{bind_module_with_prelude, Binder, ProjectBinderBuilder};
use super::declaration::{DeclarationKind, LexicalDeclaration, TypeFragmentKind, TypeGroupId};
use super::namespace::{CompilationUnit, SourceUnitKey};
use super::scope::{ScopeId, ScopeKind};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn offset(source: &str, needle: &str) -> u32 {
    u32::try_from(source.find(needle).expect("fixture contains binding"))
        .expect("fixture offset fits u32")
}

fn offsets(source: &str, needle: &str) -> Vec<u32> {
    source
        .match_indices(needle)
        .map(|(start, _)| u32::try_from(start).expect("fixture offset fits u32"))
        .collect()
}

fn exact(
    binder: &Binder,
    module: ScopeId,
    binding_start: u32,
    kind: DeclarationKind,
) -> &LexicalDeclaration {
    let declaration = binder
        .exact_declaration_at(module, binding_start, kind)
        .expect("exact admitted declaration");
    let canonical = binder
        .declarations
        .get(declaration.id)
        .expect("exact lookup returns a canonical declaration-table row");
    assert!(std::ptr::eq(declaration, canonical));
    declaration
}

fn bind_libraries<'ast>(
    programs: &[(&'ast Program<'ast>, SourceUnitKey, LibraryFileOrdinal)],
) -> (Binder, Vec<ScopeId>) {
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    assert!(prelude.diagnostics.is_empty());
    let units = programs
        .iter()
        .map(|(program, source, file)| {
            (*program, CompilationUnit::library(*source, *file, program))
        })
        .collect::<Vec<_>>();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let modules = builder.add_library_modules(&units);
    let last = modules.last().copied().expect("library batch is non-empty");
    (builder.finish(last), modules)
}

fn origin_for_module(binder: &Binder, module: ScopeId) -> CompilationOrigin {
    binder
        .namespaces
        .source_units()
        .find(|unit| unit.module == module)
        .map(|unit| unit.origin)
        .expect("bound library module has retained origin")
}

fn group_of(declaration: &LexicalDeclaration) -> TypeGroupId {
    declaration.type_group.expect("type declaration has group")
}

#[test]
fn production_cutover_retains_one_binder_index_and_removes_checker_scans() {
    let binder_source = include_str!("bind.rs");
    let binder_production = binder_source
        .split_once("#[cfg(test)]\nmod tests")
        .map(|(production, _)| production)
        .expect("binder test boundary");
    let binder_fields = binder_production
        .split_once("pub struct Binder {")
        .and_then(|(_, rest)| rest.split_once("\n}"))
        .map(|(fields, _)| fields)
        .expect("Binder fields");
    assert_eq!(binder_fields.matches("declarations_by_site:").count(), 1);
    let binder_fields_compact = binder_fields
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(
        binder_fields_compact.contains("FxHashMap<(ScopeId,u32,DeclarationKind),DeclId>"),
        "Binder retains the exact three-part source-site key"
    );
    let bind_state_fields = binder_production
        .split_once("pub(crate) struct BindState {")
        .and_then(|(_, rest)| rest.split_once("\n}"))
        .map(|(fields, _)| fields)
        .expect("BindState fields");
    assert_eq!(
        bind_state_fields.matches("declarations_by_site:").count(),
        1
    );
    let bind_state_fields_compact = bind_state_fields
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(bind_state_fields_compact.contains("FxHashMap<(ScopeId,u32,DeclarationKind),DeclId>"));
    let binder_compact = binder_production
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(binder_compact.contains("declarations_by_site:self.state.declarations_by_site"));

    let exact_lookup = binder_production
        .split_once("pub(crate) fn exact_declaration_at(")
        .and_then(|(_, rest)| rest.split_once("\n    }"))
        .map(|(body, _)| body)
        .expect("binder exact-site API");
    let exact_lookup = exact_lookup
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(
        exact_lookup.contains(".declarations_by_site.get(&(syntax_module,binding_start,kind))"),
        "exact lookup uses the three-part key directly"
    );
    assert!(exact_lookup.contains("self.declarations.get"));
    for forbidden_scan in [
        ".iter(",
        ".values(",
        ".keys(",
        ".find(",
        ".find_map(",
        ".flat_map(",
        ".position(",
    ] {
        assert!(
            !exact_lookup.contains(forbidden_scan),
            "exact lookup contains scan primitive {forbidden_scan}"
        );
    }
    assert!(!exact_lookup.contains("type_groups"));
    assert!(!exact_lookup.contains("source_units"));

    let checker_source = include_str!("../check/checker/decls/mod.rs");
    assert!(!checker_source.contains("fn exact_type_fragment_at("));
    assert!(!checker_source.contains("declarations_by_site"));
    let checker_root = include_str!("../check/checker/mod.rs");
    assert!(!checker_root.contains("exact_type_fragment_at"));
    assert!(!checker_root.contains("declarations_by_site"));
    let attach_class_bindings = checker_root
        .split_once("fn attach_class_bindings<")
        .and_then(|(_, rest)| rest.split_once("\nfn "))
        .map(|(body, _)| body)
        .expect("class binding attachment path");
    let attach_class_bindings = attach_class_bindings
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(attach_class_bindings.contains(".exact_declaration_at("));
    assert!(
        attach_class_bindings.contains(".value_storage"),
        "class binding consumes storage from the exact lexical declaration row"
    );
    for forbidden_name_lookup in [
        "value_decl_id(",
        ".resolve_value(",
        ".lookup_local(",
        ".iter(",
        ".find(",
        ".find_map(",
        "letSome(name)=",
        ".map(|id|id.name.as_str())",
    ] {
        assert!(
            !attach_class_bindings.contains(forbidden_name_lookup),
            "class binding re-resolves by name through {forbidden_name_lookup}"
        );
    }
    let visit_bound_type = checker_source
        .split_once("fn visit_bound_type<'ast>(")
        .and_then(|(_, rest)| rest.split_once("\nfn walk_type_decl_namespace"))
        .map(|(body, _)| body)
        .expect("checker bound-type visitor");
    let visit_bound_type = visit_bound_type
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(visit_bound_type.contains(".exact_declaration_at("));
    assert!(!visit_bound_type.contains("type_groups"));
    assert!(!visit_bound_type.contains("source_units"));
    assert!(!visit_bound_type.contains(".iter("));
}

#[test]
fn identical_script_sites_keep_distinct_declarations_in_one_global_group() {
    let source = "interface SharedShape { member: number; }";
    let first_allocator = Allocator::default();
    let second_allocator = Allocator::default();
    let first = Parser::new(&first_allocator, source, SourceType::d_ts()).parse();
    let second = Parser::new(&second_allocator, source, SourceType::d_ts()).parse();
    assert!(first.diagnostics.is_empty());
    assert!(second.diagnostics.is_empty());

    let first_source = SourceUnitKey(501);
    let second_source = SourceUnitKey(502);
    let first_file = LibraryFileOrdinal::new(21);
    let second_file = LibraryFileOrdinal::new(22);
    let (binder, modules) = bind_libraries(&[
        (&first.program, first_source, first_file),
        (&second.program, second_source, second_file),
    ]);
    let binding_start = offset(source, "SharedShape");
    let first_declaration = exact(
        &binder,
        modules[0],
        binding_start,
        DeclarationKind::Interface,
    );
    let second_declaration = exact(
        &binder,
        modules[1],
        binding_start,
        DeclarationKind::Interface,
    );

    assert!(binder
        .exact_declaration_at(
            binder.compilation_global,
            binding_start,
            DeclarationKind::Interface,
        )
        .is_none());
    assert_ne!(first_declaration.id, second_declaration.id);
    assert_eq!(first_declaration.site.module, modules[0]);
    assert_eq!(second_declaration.site.module, modules[1]);
    assert_eq!(
        first_declaration.site.scope,
        Some(binder.compilation_global)
    );
    assert_eq!(
        second_declaration.site.scope,
        Some(binder.compilation_global)
    );
    assert_eq!(group_of(first_declaration), group_of(second_declaration));
    assert_eq!(
        first_declaration.site.declaration_span,
        second_declaration.site.declaration_span
    );
    assert_eq!(
        first_declaration.site.binding_span,
        second_declaration.site.binding_span
    );
    assert_eq!(
        &source[first_declaration.site.declaration_span.range()],
        source
    );
    assert_eq!(
        &source[first_declaration.site.binding_span.range()],
        "SharedShape"
    );
    assert_eq!(
        origin_for_module(&binder, first_declaration.site.module),
        CompilationOrigin::Library(first_file)
    );
    assert_eq!(
        origin_for_module(&binder, second_declaration.site.module),
        CompilationOrigin::Library(second_file)
    );

    let group = binder
        .type_groups
        .get(group_of(first_declaration))
        .expect("shared global group");
    assert_eq!(
        group
            .fragments
            .iter()
            .map(|fragment| (fragment.declaration, fragment.site.module, fragment.source))
            .collect::<Vec<_>>(),
        [
            (first_declaration.id, modules[0], first_source),
            (second_declaration.id, modules[1], second_source),
        ]
    );
}

#[test]
fn reverse_library_input_keeps_each_canonical_source_owner_and_identity() {
    // This compares the library batch's ordinal-canonical allocation only; DeclId is not a
    // cross-run stable identity outside that explicitly canonical binder mode.
    let source = "interface OrderedShape { member: number; }";
    let first_allocator = Allocator::default();
    let second_allocator = Allocator::default();
    let first = Parser::new(&first_allocator, source, SourceType::d_ts()).parse();
    let second = Parser::new(&second_allocator, source, SourceType::d_ts()).parse();
    assert!(first.diagnostics.is_empty());
    assert!(second.diagnostics.is_empty());

    let first_row = (
        &first.program,
        SourceUnitKey(900),
        LibraryFileOrdinal::new(30),
    );
    let second_row = (
        &second.program,
        SourceUnitKey(100),
        LibraryFileOrdinal::new(31),
    );
    let (forward, forward_modules) = bind_libraries(&[first_row, second_row]);
    let (reverse, reverse_modules) = bind_libraries(&[second_row, first_row]);
    let binding_start = offset(source, "OrderedShape");

    let snapshot = |binder: &Binder, module: ScopeId| {
        let declaration = exact(binder, module, binding_start, DeclarationKind::Interface);
        (
            declaration.id,
            declaration.site,
            group_of(declaration),
            origin_for_module(binder, module),
        )
    };
    let forward_first = snapshot(&forward, forward_modules[0]);
    let forward_second = snapshot(&forward, forward_modules[1]);
    let reverse_first = snapshot(&reverse, reverse_modules[1]);
    let reverse_second = snapshot(&reverse, reverse_modules[0]);

    assert_eq!(forward_first, reverse_first);
    assert_eq!(forward_second, reverse_second);
    assert_ne!(forward_first.0, forward_second.0);
    assert_eq!(forward_first.2, forward_second.2);
    assert_eq!(
        forward_first.3,
        CompilationOrigin::Library(LibraryFileOrdinal::new(30))
    );
    assert_eq!(
        forward_second.3,
        CompilationOrigin::Library(LibraryFileOrdinal::new(31))
    );
}

#[test]
fn external_private_and_script_global_at_the_same_site_never_cross() {
    let script_source = "interface SameSite { script: number; }";
    let module_source = "interface SameSite { privateMember: string; } export {};";
    let script_allocator = Allocator::default();
    let module_allocator = Allocator::default();
    let script = Parser::new(&script_allocator, script_source, SourceType::d_ts()).parse();
    let module = Parser::new(&module_allocator, module_source, SourceType::d_ts()).parse();
    assert!(script.diagnostics.is_empty());
    assert!(module.diagnostics.is_empty());
    let binding_start = offset(script_source, "SameSite");
    assert_eq!(binding_start, offset(module_source, "SameSite"));

    let script_file = LibraryFileOrdinal::new(40);
    let module_file = LibraryFileOrdinal::new(41);
    let (binder, modules) = bind_libraries(&[
        (&script.program, SourceUnitKey(40), script_file),
        (&module.program, SourceUnitKey(41), module_file),
    ]);
    let script_declaration = exact(
        &binder,
        modules[0],
        binding_start,
        DeclarationKind::Interface,
    );
    let private_declaration = exact(
        &binder,
        modules[1],
        binding_start,
        DeclarationKind::Interface,
    );

    assert_ne!(script_declaration.id, private_declaration.id);
    assert_ne!(group_of(script_declaration), group_of(private_declaration));
    assert_eq!(
        script_declaration.site.scope,
        Some(binder.compilation_global)
    );
    assert_eq!(private_declaration.site.scope, Some(modules[1]));
    assert_eq!(
        origin_for_module(&binder, script_declaration.site.module),
        CompilationOrigin::Library(script_file)
    );
    assert_eq!(
        origin_for_module(&binder, private_declaration.site.module),
        CompilationOrigin::Library(module_file)
    );
    assert_eq!(
        binder
            .resolve_type(modules[0], "SameSite")
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty),
        script_declaration.type_group
    );
    assert_eq!(
        binder
            .resolve_type(modules[1], "SameSite")
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty),
        private_declaration.type_group
    );
}

#[test]
fn ordinary_external_module_global_keeps_overlay_lexical_identity_and_global_publication() {
    let source = "export {}; declare global { interface OrdinaryGlobal {} }";
    let prelude_allocator = Allocator::default();
    let source_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let parsed = Parser::new(&source_allocator, source, SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty());
    assert!(parsed.diagnostics.is_empty());
    let binder = bind_module_with_prelude(&prelude.program, &parsed.program);

    let global_start = offset(source, "global");
    let global_header = exact(
        &binder,
        binder.module,
        global_start,
        DeclarationKind::Global,
    );
    assert_eq!(global_header.site.module, binder.module);
    assert_eq!(global_header.site.scope, Some(binder.module));
    assert!(global_header.type_group.is_none());
    let augmentation = binder
        .namespaces
        .globals()
        .find(|augmentation| augmentation.declaration == global_header.id)
        .expect("exact global header owns augmentation metadata");
    assert!(augmentation.issues.is_empty());
    assert_eq!(augmentation.module, binder.module);
    assert_eq!(augmentation.target_scope, binder.compilation_global);
    assert_eq!(
        binder.global_augmentation_scope(binder.module, global_start),
        Some(augmentation.overlay_scope)
    );
    let overlay = binder
        .graph
        .get(augmentation.overlay_scope)
        .expect("global overlay scope");
    assert_eq!(overlay.kind, ScopeKind::GlobalOverlay);
    assert_eq!(overlay.parent, Some(binder.module));

    let binding_start = offset(source, "OrdinaryGlobal");
    let declaration = exact(
        &binder,
        binder.module,
        binding_start,
        DeclarationKind::Interface,
    );
    assert_eq!(declaration.site.module, binder.module);
    assert_eq!(declaration.site.scope, Some(augmentation.overlay_scope));
    let fragment = binder
        .type_groups
        .get(group_of(declaration))
        .and_then(|group| {
            group
                .fragments
                .iter()
                .find(|fragment| fragment.declaration == declaration.id)
        })
        .expect("global interface fragment");
    assert_eq!(fragment.scope, augmentation.overlay_scope);
    assert_eq!(fragment.site, declaration.site);
    let global_symbol = binder
        .graph
        .get(binder.compilation_global)
        .and_then(|scope| scope.lookup_local("OrdinaryGlobal"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .expect("global publication symbol");
    assert_eq!(global_symbol.ty, declaration.type_group);
    assert!(global_symbol.owns_type_group);
    assert!(binder
        .exact_declaration_at(
            binder.compilation_global,
            binding_start,
            DeclarationKind::Interface,
        )
        .is_none());
}

#[test]
fn namespace_declarations_return_exact_private_and_public_groups() {
    let source = "namespace Box { interface PrivateShape {} export interface PublicShape {} }";
    let prelude_allocator = Allocator::default();
    let source_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let parsed = Parser::new(&source_allocator, source, SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty());
    assert!(parsed.diagnostics.is_empty());
    let binder = bind_module_with_prelude(&prelude.program, &parsed.program);
    let module = binder.module;

    let namespace_start = offset(source, "namespace Box");
    let namespace_header = exact(
        &binder,
        module,
        offset(source, "Box"),
        DeclarationKind::Namespace,
    );
    assert_eq!(namespace_header.site.scope, Some(module));
    let namespace_id = namespace_header
        .namespace
        .expect("namespace header owns exact namespace identity");
    let namespace = binder
        .namespaces
        .get(namespace_id)
        .expect("namespace identity row");
    let namespace_fragment = namespace
        .fragments
        .iter()
        .filter_map(|fragment| binder.namespaces.fragment(*fragment))
        .find(|fragment| fragment.declaration == namespace_header.id)
        .expect("namespace header owns exact fragment");
    assert_eq!(namespace_fragment.module, module);
    let private_scope = binder
        .namespace_fragment_private_scope(module, namespace_start)
        .expect("namespace private scope");
    assert_eq!(namespace_fragment.private_scope, private_scope);
    assert_eq!(
        binder.graph.get(private_scope).map(|scope| scope.kind),
        Some(ScopeKind::NamespacePrivate)
    );

    let private_declaration = exact(
        &binder,
        module,
        offset(source, "PrivateShape"),
        DeclarationKind::Interface,
    );
    let public_declaration = exact(
        &binder,
        module,
        offset(source, "PublicShape"),
        DeclarationKind::Interface,
    );
    assert_eq!(private_declaration.site.scope, Some(private_scope));
    assert_eq!(public_declaration.site.scope, Some(private_scope));
    assert_ne!(group_of(private_declaration), group_of(public_declaration));

    let private_symbol = binder
        .graph
        .get(private_scope)
        .and_then(|scope| scope.lookup_local("PrivateShape"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .expect("private namespace type symbol");
    assert_eq!(private_symbol.ty, private_declaration.type_group);
    assert!(binder
        .graph
        .get(namespace.public_scope)
        .and_then(|scope| scope.lookup_local("PrivateShape"))
        .is_none());
    let public_symbol = binder
        .graph
        .get(namespace.public_scope)
        .and_then(|scope| scope.lookup_local("PublicShape"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .expect("public namespace type symbol");
    assert_eq!(public_symbol.ty, public_declaration.type_group);

    for declaration in [private_declaration, public_declaration] {
        let fragment = binder
            .type_groups
            .get(group_of(declaration))
            .and_then(|group| {
                group
                    .fragments
                    .iter()
                    .find(|fragment| fragment.declaration == declaration.id)
            })
            .expect("namespace interface fragment");
        assert_eq!(fragment.scope, private_scope);
        assert_eq!(fragment.site, declaration.site);
    }
}

#[test]
fn interface_class_interface_merge_keeps_every_exact_fragment_and_class_binding() {
    let source = "interface Composite { first: number } class Composite {} interface Composite { last: string }";
    let prelude_allocator = Allocator::default();
    let source_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let parsed = Parser::new(&source_allocator, source, SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty());
    assert!(parsed.diagnostics.is_empty());
    let binder = bind_module_with_prelude(&prelude.program, &parsed.program);
    let starts = offsets(source, "Composite");
    assert_eq!(starts.len(), 3);
    let kinds = [
        DeclarationKind::Interface,
        DeclarationKind::Class,
        DeclarationKind::Interface,
    ];
    let declarations = starts
        .iter()
        .copied()
        .zip(kinds)
        .map(|(start, kind)| exact(&binder, binder.module, start, kind))
        .collect::<Vec<_>>();
    let group = group_of(declarations[0]);

    assert!(declarations
        .iter()
        .all(|declaration| declaration.type_group == Some(group)));
    assert!(declarations
        .iter()
        .all(|declaration| declaration.site.scope == Some(binder.module)));
    assert!(declarations[1].value_storage.is_some());
    assert!(declarations[0].value_storage.is_none());
    assert!(declarations[2].value_storage.is_none());

    let value_symbol = binder
        .resolve_value(binder.module, "Composite")
        .expect("class value slot");
    let type_symbol = binder
        .resolve_type(binder.module, "Composite")
        .expect("merged type slot");
    assert_eq!(value_symbol, type_symbol);
    let symbol = binder
        .symbols
        .get(value_symbol)
        .expect("class/interface multi-slot symbol");
    assert_eq!(symbol.value, declarations[1].value_storage);
    assert_eq!(symbol.ty, Some(group));
    assert!(symbol.owns_type_group);
    assert_eq!(
        binder
            .declarations
            .get(declarations[1].id)
            .and_then(|declaration| declaration.value_storage),
        symbol.value,
        "checker cutover consumes the exact class row's value storage"
    );

    let bound_group = binder.type_groups.get(group).expect("merged type group");
    assert_eq!(
        bound_group
            .fragments
            .iter()
            .map(|fragment| (fragment.declaration, fragment.kind))
            .collect::<Vec<_>>(),
        [
            (declarations[0].id, TypeFragmentKind::Interface),
            (declarations[1].id, TypeFragmentKind::Class),
            (declarations[2].id, TypeFragmentKind::Interface),
        ]
    );
    for (declaration, fragment) in declarations.iter().zip(&bound_group.fragments) {
        assert_eq!(fragment.site, declaration.site);
        assert_eq!(fragment.scope, binder.module);
        assert_eq!(&source[declaration.site.binding_span.range()], "Composite");
    }
}

#[test]
fn type_alias_returns_its_exact_group_fragment_scope_and_spans() {
    let source = "type ExactAlias = { value: number };";
    let prelude_allocator = Allocator::default();
    let source_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let parsed = Parser::new(&source_allocator, source, SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty());
    assert!(parsed.diagnostics.is_empty());
    let binder = bind_module_with_prelude(&prelude.program, &parsed.program);
    let declaration = exact(
        &binder,
        binder.module,
        offset(source, "ExactAlias"),
        DeclarationKind::TypeAlias,
    );

    assert_eq!(declaration.kind, DeclarationKind::TypeAlias);
    assert_eq!(declaration.site.module, binder.module);
    assert_eq!(declaration.site.scope, Some(binder.module));
    assert_eq!(
        &source[declaration.site.declaration_span.range()],
        "type ExactAlias = { value: number };"
    );
    assert_eq!(&source[declaration.site.binding_span.range()], "ExactAlias");
    let group = binder
        .type_groups
        .get(group_of(declaration))
        .expect("alias type group");
    assert_eq!(group.name, "ExactAlias");
    assert_eq!(group.fragments.len(), 1);
    let fragment = &group.fragments[0];
    assert_eq!(fragment.declaration, declaration.id);
    assert_eq!(fragment.kind, TypeFragmentKind::TypeAlias);
    assert_eq!(fragment.scope, binder.module);
    assert_eq!(fragment.site, declaration.site);
}

#[test]
fn prelude_and_user_identical_sites_are_separate_by_syntax_module() {
    let source = "interface Shadowed { member: number; }";
    let prelude_allocator = Allocator::default();
    let user_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, source, SourceType::ts()).parse();
    let user = Parser::new(&user_allocator, source, SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty());
    assert!(user.diagnostics.is_empty());
    let binder = bind_module_with_prelude(&prelude.program, &user.program);
    let binding_start = offset(source, "Shadowed");
    let prelude_declaration = exact(
        &binder,
        binder.prelude_module,
        binding_start,
        DeclarationKind::Interface,
    );
    let user_declaration = exact(
        &binder,
        binder.module,
        binding_start,
        DeclarationKind::Interface,
    );

    assert_ne!(prelude_declaration.id, user_declaration.id);
    assert_ne!(group_of(prelude_declaration), group_of(user_declaration));
    assert_eq!(prelude_declaration.site.module, binder.prelude_module);
    assert_eq!(user_declaration.site.module, binder.module);
    assert_eq!(prelude_declaration.site.scope, Some(binder.prelude_module));
    assert_eq!(user_declaration.site.scope, Some(binder.module));
    assert_eq!(
        prelude_declaration.site.binding_span,
        user_declaration.site.binding_span
    );
    assert_eq!(
        prelude_declaration.site.declaration_span,
        user_declaration.site.declaration_span
    );

    let prelude_symbol = binder
        .graph
        .get(binder.prelude_module)
        .and_then(|scope| scope.lookup_local("Shadowed"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .expect("prelude-local type symbol");
    let user_symbol = binder
        .graph
        .get(binder.module)
        .and_then(|scope| scope.lookup_local("Shadowed"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .expect("user-local shadowing type symbol");
    assert_eq!(prelude_symbol.ty, prelude_declaration.type_group);
    assert_eq!(user_symbol.ty, user_declaration.type_group);
    assert!(prelude_symbol.owns_type_group);
    assert!(user_symbol.owns_type_group);
}

#[test]
fn wrong_kind_missing_site_and_unadmitted_source_occurrence_return_none() {
    let source = "declare module 'pkg' { interface DeferredShape {} } interface Present {}";
    let prelude_allocator = Allocator::default();
    let source_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let parsed = Parser::new(&source_allocator, source, SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty());
    assert!(parsed.diagnostics.is_empty());
    let binder = bind_module_with_prelude(&prelude.program, &parsed.program);
    let present_start = offset(source, "Present");
    let deferred_start = offset(source, "DeferredShape");

    assert!(binder
        .exact_declaration_at(binder.module, present_start, DeclarationKind::Interface)
        .is_some());
    assert!(binder
        .exact_declaration_at(binder.module, present_start, DeclarationKind::Class)
        .is_none());
    assert!(binder
        .exact_declaration_at(binder.module, u32::MAX, DeclarationKind::Interface)
        .is_none());

    let inventoried = binder
        .declarations
        .iter()
        .find(|declaration| {
            declaration.site.module == binder.module
                && declaration.kind == DeclarationKind::Interface
                && declaration.site.binding_span.start == deferred_start
        })
        .expect("source prewalk inventories deferred ambient-module child");
    assert_eq!(inventoried.site.scope, None);
    let ambient_header = exact(
        &binder,
        binder.module,
        offset(source, "'pkg'"),
        DeclarationKind::Namespace,
    );
    let ambient_module = binder
        .namespaces
        .deferred_modules()
        .find(|module| module.declaration == ambient_header.id)
        .expect("string-literal ambient module is intentionally deferred");
    assert!(binder.namespaces.deferred_children().any(|child| {
        child.module == ambient_module.id && child.declaration == Some(inventoried.id)
    }));
    assert!(binder
        .exact_declaration_at(binder.module, deferred_start, DeclarationKind::Interface)
        .is_none());
}
