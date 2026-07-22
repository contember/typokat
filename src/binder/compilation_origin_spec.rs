//! Binder compilation-origin provenance contract.
//!
//! | Contract | Executable or review witness |
//! | --- | --- |
//! | `crate::source` stays acyclic | Existing `ModuleOrdinal`, `UnitSlot`, and `OriginalModuleOrdinal` move into the neutral module; it imports neither binder nor checker. |
//! | Origin is retained, never reconstructed | Compilation units and the inventoried outcome records carry `CompilationOrigin` directly. |
//! | Missing source provenance fails | `compilation_origin_for_source` returns `None`; it never fabricates user ordinal zero. |
//! | Reporting stays downstream | This binder-only spec proves retained provenance; the checker-side `library_reporting` spec proves the single real consumer and its typed ledger receipts. |
//!
//! Inventoried outcome families: local ambient export failure, placement issue, global
//! augmentation issue, UMD export/context, export context, namespace member, and standalone
//! namespace value member.
//!
//! These origin-bearing records remain crate-private so neutral provenance never leaks through
//! public fields; the public driver/checker APIs and default behavior remain unchanged.

use super::bind::{Binder, ProjectBinderBuilder};
use super::namespace::{
    CompilationUnit, ExportSyntaxDisposition, GlobalIssue, LocalAmbientExportAliasFailureKind,
    ModuleBindingContext, PlacementIssueKind, SourceFileKind, SourceUnitKey, UmdContext,
};
use crate::source::{CompilationOrigin, LibraryFileOrdinal, OriginalModuleOrdinal};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn bind_unit(
    source: &str,
    source_key: SourceUnitKey,
    origin: CompilationOrigin,
    source_file_kind: SourceFileKind,
) -> Binder {
    let prelude_allocator = Allocator::default();
    let source_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let parsed = Parser::new(&source_allocator, source, SourceType::ts()).parse();
    assert!(!prelude.panicked, "empty prelude must parse");
    assert!(!parsed.panicked, "binder provenance witness must parse");

    let unit = CompilationUnit {
        source: source_key,
        origin,
        binding: ModuleBindingContext::for_program(&parsed.program, source_file_kind),
    };
    assert_eq!(unit.origin, origin);

    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let (module, placeholders) = builder.add_module(&parsed.program, &[], unit);
    assert!(placeholders.is_empty());
    builder.finish(module)
}

fn bind_declaration_unit(
    source: &str,
    source_key: SourceUnitKey,
    origin: CompilationOrigin,
) -> Binder {
    bind_unit(source, source_key, origin, SourceFileKind::DeclarationTs)
}

#[test]
fn compilation_origin_variants_preserve_exact_ordinal_domains() {
    let user = CompilationOrigin::User(OriginalModuleOrdinal::new(9));
    let library = CompilationOrigin::Library(LibraryFileOrdinal::new(81));

    assert!(matches!(
        user,
        CompilationOrigin::User(ordinal) if ordinal == OriginalModuleOrdinal::new(9)
    ));
    assert!(matches!(
        library,
        CompilationOrigin::Library(ordinal) if ordinal == LibraryFileOrdinal::new(81)
    ));
}

#[test]
fn namespace_provenance_has_no_zero_reconstruction_or_public_private_leak() {
    let source = include_str!("namespace.rs");
    let production = source.split_once("#[cfg(test)]\nmod tests");
    assert!(production.is_some());
    let Some((production, _)) = production else {
        return;
    };
    let compact = production
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();

    assert!(!production.contains("original_module_for_source"));
    assert!(!compact.contains("unwrap_or(OriginalModuleOrdinal(0))"));
    let origin_lookup_definitions = production
        .matches("fn compilation_origin_for_source(")
        .count();
    let origin_lookup_references = production.matches("compilation_origin_for_source(").count();
    assert_eq!(origin_lookup_definitions, 1);
    assert_eq!(origin_lookup_references, origin_lookup_definitions);
    for record in [
        "CompilationUnit",
        "LocalAmbientExportAliasFailure",
        "PlacementIssue",
        "GlobalAugmentation",
        "UmdNamespaceExport",
        "ExportContext",
        "NamespaceMember",
        "StandaloneNamespaceValueMember",
        "SourceUnitRecord",
    ] {
        assert!(
            production.contains(&format!("pub(crate) struct {record}")),
            "origin-bearing record must stay crate-private: {record}"
        );
        assert!(
            !production.contains(&format!("pub struct {record}")),
            "origin-bearing record exposes a crate-private source type: {record}"
        );
    }
    assert!(production.contains("pub(crate) struct SourceUnitKey"));
    assert!(!include_str!("../source.rs").contains("SourceUnitKey"));
}

#[test]
fn inventoried_binder_outcomes_retain_exact_library_owner_for_downstream_reporting() {
    let local_origin = CompilationOrigin::Library(LibraryFileOrdinal::new(10));
    let local = bind_declaration_unit(
        "declare namespace AliasOutput { interface Local {} export { A as B }; export { Local as A }; }",
        SourceUnitKey(110),
        local_origin,
    );
    let source_units = local.namespaces.source_units().collect::<Vec<_>>();
    assert_eq!(source_units.len(), 1);
    let Some(source_unit) = source_units.first() else {
        return;
    };
    assert_eq!(source_unit.origin, local_origin);
    let local_failures = local.local_ambient_export_alias_failures();
    assert_eq!(local_failures.len(), 1);
    let Some(local_failure) = local_failures.first() else {
        return;
    };
    assert_eq!(local_failure.origin, local_origin);
    assert_eq!(
        local_failure.kind,
        LocalAmbientExportAliasFailureKind::NonLocal
    );

    let placement_origin = CompilationOrigin::Library(LibraryFileOrdinal::new(11));
    let placement = bind_unit(
        "namespace Late { export const value = 1; } function Late(): void {}",
        SourceUnitKey(111),
        placement_origin,
        SourceFileKind::ImplementationTs,
    );
    let placement_issues = placement.namespaces.placement_issues().collect::<Vec<_>>();
    assert_eq!(placement_issues.len(), 1);
    let Some(placement_issue) = placement_issues.first() else {
        return;
    };
    assert_eq!(placement_issue.origin, placement_origin);
    assert_eq!(placement_issue.kind, PlacementIssueKind::FutureTk2434);

    let global_origin = CompilationOrigin::Library(LibraryFileOrdinal::new(12));
    let global = bind_declaration_unit(
        "declare global { interface InvalidScriptGlobal {} }",
        SourceUnitKey(112),
        global_origin,
    );
    let globals = global.namespaces.globals().collect::<Vec<_>>();
    assert_eq!(globals.len(), 1);
    let Some(global_issue) = globals.first() else {
        return;
    };
    assert_eq!(global_issue.origin, global_origin);
    assert_eq!(global_issue.issues, [GlobalIssue::FutureTk2669]);

    let umd_origin = CompilationOrigin::Library(LibraryFileOrdinal::new(13));
    let umd = bind_unit(
        "export as namespace ScriptUmd;",
        SourceUnitKey(113),
        umd_origin,
        SourceFileKind::ImplementationTs,
    );
    let umd_exports = umd.namespaces.umd_exports().collect::<Vec<_>>();
    assert_eq!(umd_exports.len(), 1);
    let Some(umd_export) = umd_exports.first() else {
        return;
    };
    assert_eq!(umd_export.origin, umd_origin);
    assert_eq!(umd_export.context, UmdContext::FutureTk1314NonExternal);

    let export_origin = CompilationOrigin::Library(LibraryFileOrdinal::new(14));
    let export_context = bind_declaration_unit(
        "declare namespace Exported { export default function f(): void; }",
        SourceUnitKey(114),
        export_origin,
    );
    let export_contexts = export_context
        .namespaces
        .export_contexts()
        .collect::<Vec<_>>();
    assert_eq!(export_contexts.len(), 1);
    let Some(export_issue) = export_contexts.first() else {
        return;
    };
    assert_eq!(export_issue.origin, export_origin);
    assert_eq!(export_issue.syntax, ExportSyntaxDisposition::FutureTk1319);

    let member_origin = CompilationOrigin::Library(LibraryFileOrdinal::new(15));
    let member_binder = bind_declaration_unit(
        "declare namespace MemberRoot { const value: number; }",
        SourceUnitKey(115),
        member_origin,
    );
    let roots = member_binder.namespaces.namespaces().collect::<Vec<_>>();
    assert_eq!(roots.len(), 1);
    let Some(root) = roots.first() else {
        return;
    };
    let Some(fragment_id) = root.fragments.first() else {
        return;
    };
    let Some(fragment) = member_binder.namespaces.fragment(*fragment_id) else {
        return;
    };
    let Some(member_id) = fragment.members.first() else {
        return;
    };
    let Some(member) = member_binder.namespaces.member(*member_id) else {
        return;
    };
    assert_eq!(member.origin, member_origin);

    let standalone_origin = CompilationOrigin::Library(LibraryFileOrdinal::new(16));
    let standalone_binder = bind_declaration_unit(
        "declare namespace StandaloneRoot { const value: number; }",
        SourceUnitKey(116),
        standalone_origin,
    );
    let attachments = standalone_binder.local_standalone_namespace_value_attachments();
    assert_eq!(attachments.len(), 1);
    let Some(attachment) = attachments.first() else {
        return;
    };
    let Some(standalone_member) = attachment.members.first() else {
        return;
    };
    assert_eq!(standalone_member.origin, standalone_origin);
    assert_eq!(
        [
            local_failure.origin,
            placement_issue.origin,
            global_issue.origin,
            umd_export.origin,
            export_issue.origin,
            member.origin,
            standalone_member.origin,
        ],
        [
            CompilationOrigin::Library(LibraryFileOrdinal::new(10)),
            CompilationOrigin::Library(LibraryFileOrdinal::new(11)),
            CompilationOrigin::Library(LibraryFileOrdinal::new(12)),
            CompilationOrigin::Library(LibraryFileOrdinal::new(13)),
            CompilationOrigin::Library(LibraryFileOrdinal::new(14)),
            CompilationOrigin::Library(LibraryFileOrdinal::new(15)),
            CompilationOrigin::Library(LibraryFileOrdinal::new(16)),
        ]
    );
}

#[test]
fn missing_source_origin_is_none_and_never_user_zero() {
    let source_key = SourceUnitKey(31);
    let origin = CompilationOrigin::Library(LibraryFileOrdinal::new(4));
    let binder = bind_declaration_unit("interface Present {}", source_key, origin);

    assert_eq!(
        binder.namespaces.compilation_origin_for_source(source_key),
        Some(origin)
    );
    assert_eq!(
        binder
            .namespaces
            .compilation_origin_for_source(SourceUnitKey(999)),
        None
    );
    assert_ne!(
        binder
            .namespaces
            .compilation_origin_for_source(SourceUnitKey(999)),
        Some(CompilationOrigin::User(OriginalModuleOrdinal::new(0)))
    );
}
