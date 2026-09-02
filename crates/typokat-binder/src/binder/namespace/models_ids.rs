use crate::binder::declaration::{DeclId, DeclarationSite, TypeGroupId, ValueStorageId};
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
#[cfg(any(test, feature = "test-utils"))]
use crate::source::LibraryFileOrdinal;
use crate::source::{CompilationOrigin, OriginalModuleOrdinal};
use crate::span::Span;
use oxc_ast::ast::{Program, Statement, TSModuleReference};
use oxc_ast_visit::{walk, Visit};
use std::sync::Arc;

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static CONTINUATION_MERGE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_INSTANCE_FRAGMENT_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_CHILD_FRAGMENT_LOOKUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_ALLOCATION_NAMESPACE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_ATTACHMENT_NAMESPACE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_FRAGMENT_SCOPE_LOOKUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_PLACEMENT_MERGE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_GLOBAL_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_UMD_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_AMBIENT_ALIAS_MEMBER_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_UMD_STATEMENT_QUERIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_GLOBAL_STATEMENT_QUERIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_LIBRARY_SOURCE_LOOKUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_LIBRARY_REPORTING_LOOKUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NamespaceContinuationWorkForTest {
    pub(crate) merge_rows: u64,
    pub(crate) instance_fragment_rows: u64,
    pub(crate) child_fragment_lookups: u64,
    pub(crate) allocation_namespace_rows: u64,
    pub(crate) attachment_namespace_rows: u64,
    pub(crate) fragment_scope_lookups: u64,
    pub(crate) placement_merge_rows: u64,
    pub(crate) global_rows: u64,
    pub(crate) umd_rows: u64,
    pub(crate) ambient_alias_member_rows: u64,
    pub(crate) umd_statement_queries: u64,
    pub(crate) global_statement_queries: u64,
    pub(crate) library_source_lookups: u64,
    pub(crate) library_reporting_lookups: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn namespace_continuation_work_for_test() -> NamespaceContinuationWorkForTest {
    NamespaceContinuationWorkForTest {
        merge_rows: CONTINUATION_MERGE_ROWS.get(),
        instance_fragment_rows: CONTINUATION_INSTANCE_FRAGMENT_ROWS.get(),
        child_fragment_lookups: CONTINUATION_CHILD_FRAGMENT_LOOKUPS.get(),
        allocation_namespace_rows: CONTINUATION_ALLOCATION_NAMESPACE_ROWS.get(),
        attachment_namespace_rows: CONTINUATION_ATTACHMENT_NAMESPACE_ROWS.get(),
        fragment_scope_lookups: CONTINUATION_FRAGMENT_SCOPE_LOOKUPS.get(),
        placement_merge_rows: CONTINUATION_PLACEMENT_MERGE_ROWS.get(),
        global_rows: CONTINUATION_GLOBAL_ROWS.get(),
        umd_rows: CONTINUATION_UMD_ROWS.get(),
        ambient_alias_member_rows: CONTINUATION_AMBIENT_ALIAS_MEMBER_ROWS.get(),
        umd_statement_queries: CONTINUATION_UMD_STATEMENT_QUERIES.get(),
        global_statement_queries: CONTINUATION_GLOBAL_STATEMENT_QUERIES.get(),
        library_source_lookups: CONTINUATION_LIBRARY_SOURCE_LOOKUPS.get(),
        library_reporting_lookups: CONTINUATION_LIBRARY_REPORTING_LOOKUPS.get(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_merge_row() {
    CONTINUATION_MERGE_ROWS.set(CONTINUATION_MERGE_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_instance_fragment_row() {
    CONTINUATION_INSTANCE_FRAGMENT_ROWS.set(CONTINUATION_INSTANCE_FRAGMENT_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_child_fragment_lookup() {
    CONTINUATION_CHILD_FRAGMENT_LOOKUPS.set(CONTINUATION_CHILD_FRAGMENT_LOOKUPS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_allocation_namespace_row() {
    CONTINUATION_ALLOCATION_NAMESPACE_ROWS.set(CONTINUATION_ALLOCATION_NAMESPACE_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_attachment_namespace_row() {
    CONTINUATION_ATTACHMENT_NAMESPACE_ROWS.set(CONTINUATION_ATTACHMENT_NAMESPACE_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_fragment_scope_lookup() {
    CONTINUATION_FRAGMENT_SCOPE_LOOKUPS.set(CONTINUATION_FRAGMENT_SCOPE_LOOKUPS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_placement_merge_row() {
    CONTINUATION_PLACEMENT_MERGE_ROWS.set(CONTINUATION_PLACEMENT_MERGE_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_global_row() {
    CONTINUATION_GLOBAL_ROWS.set(CONTINUATION_GLOBAL_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_umd_row() {
    CONTINUATION_UMD_ROWS.set(CONTINUATION_UMD_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_ambient_alias_member_row() {
    CONTINUATION_AMBIENT_ALIAS_MEMBER_ROWS.set(CONTINUATION_AMBIENT_ALIAS_MEMBER_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_umd_statement_query() {
    CONTINUATION_UMD_STATEMENT_QUERIES.set(CONTINUATION_UMD_STATEMENT_QUERIES.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_global_statement_query() {
    CONTINUATION_GLOBAL_STATEMENT_QUERIES.set(CONTINUATION_GLOBAL_STATEMENT_QUERIES.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_library_source_lookup() {
    CONTINUATION_LIBRARY_SOURCE_LOOKUPS.set(CONTINUATION_LIBRARY_SOURCE_LOOKUPS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_continuation_library_reporting_lookup() {
    CONTINUATION_LIBRARY_REPORTING_LOOKUPS.set(CONTINUATION_LIBRARY_REPORTING_LOOKUPS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub struct NamespaceContinuationWorkScopeForTest(NamespaceContinuationWorkForTest);

#[cfg(any(test, feature = "test-utils"))]
impl NamespaceContinuationWorkScopeForTest {
    pub fn start() -> Self {
        Self(namespace_continuation_work_for_test())
    }

    pub fn finish(self) -> NamespaceContinuationWorkForTest {
        let end = namespace_continuation_work_for_test();
        NamespaceContinuationWorkForTest {
            merge_rows: end.merge_rows.saturating_sub(self.0.merge_rows),
            instance_fragment_rows: end
                .instance_fragment_rows
                .saturating_sub(self.0.instance_fragment_rows),
            child_fragment_lookups: end
                .child_fragment_lookups
                .saturating_sub(self.0.child_fragment_lookups),
            allocation_namespace_rows: end
                .allocation_namespace_rows
                .saturating_sub(self.0.allocation_namespace_rows),
            attachment_namespace_rows: end
                .attachment_namespace_rows
                .saturating_sub(self.0.attachment_namespace_rows),
            fragment_scope_lookups: end
                .fragment_scope_lookups
                .saturating_sub(self.0.fragment_scope_lookups),
            placement_merge_rows: end
                .placement_merge_rows
                .saturating_sub(self.0.placement_merge_rows),
            global_rows: end.global_rows.saturating_sub(self.0.global_rows),
            umd_rows: end.umd_rows.saturating_sub(self.0.umd_rows),
            ambient_alias_member_rows: end
                .ambient_alias_member_rows
                .saturating_sub(self.0.ambient_alias_member_rows),
            umd_statement_queries: end
                .umd_statement_queries
                .saturating_sub(self.0.umd_statement_queries),
            global_statement_queries: end
                .global_statement_queries
                .saturating_sub(self.0.global_statement_queries),
            library_source_lookups: end
                .library_source_lookups
                .saturating_sub(self.0.library_source_lookups),
            library_reporting_lookups: end
                .library_reporting_lookups
                .saturating_sub(self.0.library_reporting_lookups),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static PLACEMENT_ROW_PROBES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Placement participant rows visited to resolve one declaration — the merge-group rows
/// `push_placement` compares, plus every by-declaration syntax read it answers.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlacementLookupWorkForTest {
    pub(crate) row_probes: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn placement_lookup_work_for_test() -> PlacementLookupWorkForTest {
    PlacementLookupWorkForTest {
        row_probes: PLACEMENT_ROW_PROBES.get(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_placement_row_probe() {
    PLACEMENT_ROW_PROBES.set(PLACEMENT_ROW_PROBES.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub struct PlacementLookupWorkScopeForTest(PlacementLookupWorkForTest);

#[cfg(any(test, feature = "test-utils"))]
impl PlacementLookupWorkScopeForTest {
    pub fn start() -> Self {
        Self(placement_lookup_work_for_test())
    }

    pub fn finish(self) -> PlacementLookupWorkForTest {
        let end = placement_lookup_work_for_test();
        PlacementLookupWorkForTest {
            row_probes: end.row_probes.saturating_sub(self.0.row_probes),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static FINALIZATION_CLASSIFICATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FINALIZATION_MERGE_PARTICIPANT_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FINALIZATION_MERGE_INDEX_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FINALIZATION_ATTACHMENT_MERGE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FINALIZATION_ALIAS_MEMBER_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Project-wide rows re-processed by namespace finalization. Every field is derived from
/// the accumulated project, so a per-module finalization pass makes each of them grow with
/// the file split at constant program size.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NamespaceFinalizationWorkForTest {
    /// Times `classify` rebuilt the canonical indexes over the whole accumulated project.
    pub(crate) classifications: u64,
    /// Placement participants cloned, sorted and classified into the merges vector.
    pub(crate) merge_participant_rows: u64,
    /// Merge records re-keyed into `merge_indices`, each cloning its name.
    pub(crate) merge_index_rows: u64,
    /// Merge records scanned to fill namespace value attachments.
    pub(crate) attachment_merge_rows: u64,
    /// Member rows scanned to resolve local ambient export alias targets.
    pub(crate) alias_member_rows: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn namespace_finalization_work_for_test() -> NamespaceFinalizationWorkForTest {
    NamespaceFinalizationWorkForTest {
        classifications: FINALIZATION_CLASSIFICATIONS.get(),
        merge_participant_rows: FINALIZATION_MERGE_PARTICIPANT_ROWS.get(),
        merge_index_rows: FINALIZATION_MERGE_INDEX_ROWS.get(),
        attachment_merge_rows: FINALIZATION_ATTACHMENT_MERGE_ROWS.get(),
        alias_member_rows: FINALIZATION_ALIAS_MEMBER_ROWS.get(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_finalization_classification() {
    FINALIZATION_CLASSIFICATIONS.set(FINALIZATION_CLASSIFICATIONS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_finalization_merge_participant_rows(rows: usize) {
    FINALIZATION_MERGE_PARTICIPANT_ROWS.set(
        FINALIZATION_MERGE_PARTICIPANT_ROWS
            .get()
            .saturating_add(u64::try_from(rows).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_finalization_merge_index_row() {
    FINALIZATION_MERGE_INDEX_ROWS.set(FINALIZATION_MERGE_INDEX_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_finalization_attachment_merge_row() {
    FINALIZATION_ATTACHMENT_MERGE_ROWS.set(FINALIZATION_ATTACHMENT_MERGE_ROWS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_finalization_alias_member_rows(rows: usize) {
    FINALIZATION_ALIAS_MEMBER_ROWS.set(
        FINALIZATION_ALIAS_MEMBER_ROWS
            .get()
            .saturating_add(u64::try_from(rows).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub struct NamespaceFinalizationWorkScopeForTest(NamespaceFinalizationWorkForTest);

#[cfg(any(test, feature = "test-utils"))]
impl NamespaceFinalizationWorkScopeForTest {
    pub fn start() -> Self {
        Self(namespace_finalization_work_for_test())
    }

    pub fn finish(self) -> NamespaceFinalizationWorkForTest {
        let end = namespace_finalization_work_for_test();
        NamespaceFinalizationWorkForTest {
            classifications: end.classifications.saturating_sub(self.0.classifications),
            merge_participant_rows: end
                .merge_participant_rows
                .saturating_sub(self.0.merge_participant_rows),
            merge_index_rows: end.merge_index_rows.saturating_sub(self.0.merge_index_rows),
            attachment_merge_rows: end
                .attachment_merge_rows
                .saturating_sub(self.0.attachment_merge_rows),
            alias_member_rows: end
                .alias_member_rows
                .saturating_sub(self.0.alias_member_rows),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static MERGE_KEY_NAME_ALLOCATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static MERGE_KEY_NAME_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static NON_MERGING_PARTICIPANT_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FIRST_PARTICIPANT_HEAP_ALLOCATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static NON_MERGING_MERGE_RECORDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PLACEMENT_ORDER_GROUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PLACEMENT_ROW_REORDERS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Per-declaration substrate the merge lane pays for before anything merges. Every field is
/// keyed to one declaration, so a project of declarations that never merge grows all of them
/// even though its merge set never changes.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MergeSubstrateWorkForTest {
    /// Owned `String`s allocated to name a merge key or merge record. Interned names cost one
    /// allocation per *distinct* name; owned ones cost one per key, record and lookup.
    pub(crate) merge_key_name_allocations: u64,
    /// Bytes of those owned name strings, so a repeated name is visible as repeated bytes.
    pub(crate) merge_key_name_bytes: u64,
    /// `MergeParticipant` rows the substrate retains for a group holding one declaration —
    /// a declaration that, by the time classification runs, merged with nothing.
    pub(crate) non_merging_participant_rows: u64,
    pub(crate) first_participant_heap_allocations: u64,
    /// Merge records built for such a group: a classification of one declaration against
    /// itself, plus its `merge_indices` key.
    pub(crate) non_merging_merge_records: u64,
    /// Placement groups visited by the one-shot instance-state and canonical-order pass.
    pub(crate) placement_order_groups: u64,
    /// Placement groups the canonical-order pass actually reordered. The rows are appended in
    /// ascending `(source, span)` order, so this stays zero — it is the witness that sorting
    /// them where they live cannot move the canonical row order.
    pub(crate) placement_row_reorders: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn merge_substrate_work_for_test() -> MergeSubstrateWorkForTest {
    MergeSubstrateWorkForTest {
        merge_key_name_allocations: MERGE_KEY_NAME_ALLOCATIONS.get(),
        merge_key_name_bytes: MERGE_KEY_NAME_BYTES.get(),
        non_merging_participant_rows: NON_MERGING_PARTICIPANT_ROWS.get(),
        first_participant_heap_allocations: FIRST_PARTICIPANT_HEAP_ALLOCATIONS.get(),
        non_merging_merge_records: NON_MERGING_MERGE_RECORDS.get(),
        placement_order_groups: PLACEMENT_ORDER_GROUPS.get(),
        placement_row_reorders: PLACEMENT_ROW_REORDERS.get(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_placement_row_reorder() {
    PLACEMENT_ROW_REORDERS.set(PLACEMENT_ROW_REORDERS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_placement_order_group() {
    PLACEMENT_ORDER_GROUPS.set(PLACEMENT_ORDER_GROUPS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_merge_key_name_allocation(name: &str) {
    MERGE_KEY_NAME_ALLOCATIONS.set(MERGE_KEY_NAME_ALLOCATIONS.get() + 1);
    MERGE_KEY_NAME_BYTES.set(
        MERGE_KEY_NAME_BYTES
            .get()
            .saturating_add(u64::try_from(name.len()).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_non_merging_participant_rows(rows: usize) {
    NON_MERGING_PARTICIPANT_ROWS.set(
        NON_MERGING_PARTICIPANT_ROWS
            .get()
            .saturating_add(u64::try_from(rows).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_first_participant_heap_allocation() {
    FIRST_PARTICIPANT_HEAP_ALLOCATIONS.set(FIRST_PARTICIPANT_HEAP_ALLOCATIONS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_non_merging_merge_record() {
    NON_MERGING_MERGE_RECORDS.set(NON_MERGING_MERGE_RECORDS.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub struct MergeSubstrateWorkScopeForTest(MergeSubstrateWorkForTest);

#[cfg(any(test, feature = "test-utils"))]
impl MergeSubstrateWorkScopeForTest {
    pub fn start() -> Self {
        Self(merge_substrate_work_for_test())
    }

    pub fn finish(self) -> MergeSubstrateWorkForTest {
        let end = merge_substrate_work_for_test();
        MergeSubstrateWorkForTest {
            merge_key_name_allocations: end
                .merge_key_name_allocations
                .saturating_sub(self.0.merge_key_name_allocations),
            merge_key_name_bytes: end
                .merge_key_name_bytes
                .saturating_sub(self.0.merge_key_name_bytes),
            non_merging_participant_rows: end
                .non_merging_participant_rows
                .saturating_sub(self.0.non_merging_participant_rows),
            first_participant_heap_allocations: end
                .first_participant_heap_allocations
                .saturating_sub(self.0.first_participant_heap_allocations),
            non_merging_merge_records: end
                .non_merging_merge_records
                .saturating_sub(self.0.non_merging_merge_records),
            placement_order_groups: end
                .placement_order_groups
                .saturating_sub(self.0.placement_order_groups),
            placement_row_reorders: end
                .placement_row_reorders
                .saturating_sub(self.0.placement_row_reorders),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NamespaceId(pub u32);

impl NamespaceId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("namespace id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NamespaceFragmentId(pub u32);

impl NamespaceFragmentId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("namespace fragment id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NamespaceMemberId(pub(crate) u32);

impl NamespaceMemberId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("namespace member id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct GlobalAugmentationId(pub(crate) u32);

impl GlobalAugmentationId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("global augmentation id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DeferredModuleId(pub u32);

impl DeferredModuleId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("deferred module id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ExportContextId(pub(crate) u32);

impl ExportContextId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("export context id fits usize")
    }
}

/// Run-stable project source ordering key. Project mode derives it from normalized paths.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceUnitKey(pub u32);

impl SourceUnitKey {
    pub(crate) const PRELUDE: Self = Self(0);
    pub const SINGLE_SOURCE: Self = Self(1);
}

pub type ExactKey = SourceUnitKey;

pub const fn exact_key(index: u32) -> ExactKey {
    SourceUnitKey(index)
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum SourceFileKind {
    ImplementationTs,
    ImplementationMts,
    ImplementationCts,
    DeclarationTs,
    DeclarationMts,
    DeclarationCts,
}

impl SourceFileKind {
    pub fn is_declaration(self) -> bool {
        !matches!(
            self,
            Self::ImplementationTs | Self::ImplementationMts | Self::ImplementationCts
        )
    }

    fn implies_external_module(self) -> bool {
        matches!(self, Self::ImplementationMts | Self::ImplementationCts)
    }
}

/// Canonical filename classifier shared by parsing, binding, and source-only preflight.
pub fn source_file_kind(name: &str) -> SourceFileKind {
    if name.ends_with(".d.mts") {
        SourceFileKind::DeclarationMts
    } else if name.ends_with(".d.cts") {
        SourceFileKind::DeclarationCts
    } else if name.ends_with(".d.ts") {
        SourceFileKind::DeclarationTs
    } else if name.ends_with(".mts") {
        SourceFileKind::ImplementationMts
    } else if name.ends_with(".cts") {
        SourceFileKind::ImplementationCts
    } else {
        SourceFileKind::ImplementationTs
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ModuleBindingContext {
    pub source_file_kind: SourceFileKind,
    pub external_module: bool,
}

impl Default for ModuleBindingContext {
    fn default() -> Self {
        Self {
            source_file_kind: SourceFileKind::ImplementationTs,
            external_module: false,
        }
    }
}

impl ModuleBindingContext {
    pub fn for_program(program: &Program<'_>, source_file_kind: SourceFileKind) -> Self {
        Self {
            source_file_kind,
            external_module: source_file_kind.implies_external_module()
                || has_external_module_indicator(program),
        }
    }

    pub fn declaration_file(self) -> bool {
        self.source_file_kind.is_declaration()
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CompilationUnit {
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub binding: ModuleBindingContext,
}

/// A source-origin index write that cannot participate in semantic checking.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CompilationOriginIndexConflict {
    ConflictingOrigin {
        source: SourceUnitKey,
        retained: CompilationOrigin,
        attempted: CompilationOrigin,
    },
    SealedPrefixWrite {
        source: SourceUnitKey,
        attempted: CompilationOrigin,
    },
}

impl CompilationUnit {
    pub fn implementation(source: SourceUnitKey, program: &Program<'_>) -> Self {
        Self {
            source,
            origin: CompilationOrigin::User(OriginalModuleOrdinal::new(0)),
            binding: ModuleBindingContext::for_program(program, SourceFileKind::ImplementationTs),
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn library(
        source: SourceUnitKey,
        file_ordinal: LibraryFileOrdinal,
        program: &Program<'_>,
    ) -> Self {
        Self {
            source,
            origin: CompilationOrigin::Library(file_ordinal),
            binding: ModuleBindingContext::for_program(program, SourceFileKind::DeclarationTs),
        }
    }
}

#[derive(Default)]
struct ImportMetaVisitor {
    found: bool,
}

impl<'a> Visit<'a> for ImportMetaVisitor {
    fn visit_meta_property(&mut self, property: &oxc_ast::ast::MetaProperty<'a>) {
        if property.meta.name == "import" && property.property.name == "meta" {
            self.found = true;
        }
        walk::walk_meta_property(self, property);
    }
}

/// Structural external-module classification; parser source goals are intentionally ignored.
pub fn has_external_module_indicator(program: &Program<'_>) -> bool {
    let top_level_indicator = program.body.iter().any(|statement| match statement {
        Statement::ImportDeclaration(_)
        | Statement::ExportAllDeclaration(_)
        | Statement::ExportDefaultDeclaration(_)
        | Statement::ExportNamedDeclaration(_)
        | Statement::TSExportAssignment(_) => true,
        Statement::TSImportEqualsDeclaration(declaration) => matches!(
            declaration.module_reference,
            TSModuleReference::ExternalModuleReference(_)
        ),
        Statement::TSNamespaceExportDeclaration(_) => false,
        _ => false,
    });
    if top_level_indicator {
        return true;
    }
    let mut visitor = ImportMetaVisitor::default();
    visitor.visit_program(program);
    visitor.found
}

/// Typed identity owner; text paths and spans never participate in namespace identity.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum NamespaceOwner {
    Lexical(ScopeId),
    NamespacePublic(NamespaceId),
    FragmentPrivate(NamespaceFragmentId),
    CompilationGlobal,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum NamespacePublication {
    Private,
    Explicit,
    AmbientDefault,
    DottedImplicit,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Namespace {
    pub id: NamespaceId,
    pub owner: NamespaceOwner,
    pub name: String,
    pub public_scope: ScopeId,
    pub symbol: SymbolId,
    pub fragments: Vec<NamespaceFragmentId>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NamespaceFragment {
    pub id: NamespaceFragmentId,
    pub namespace: NamespaceId,
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub source_start: u32,
    pub module: ScopeId,
    pub private_scope: ScopeId,
    pub lexical_parent: ScopeId,
    pub public_scope: ScopeId,
    pub ambient: bool,
    pub publication: NamespacePublication,
    pub instance_state: NamespaceInstanceState,
    pub members: Vec<NamespaceMemberId>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DeclarationOwner {
    Lexical(ScopeId),
    NamespacePublic(NamespaceId),
    NamespacePrivate(NamespaceFragmentId),
    CompilationGlobal,
    DeferredAmbientModule(DeferredModuleId),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum NamespaceMemberOwner {
    Fragment(NamespaceFragmentId),
    GlobalAugmentation(GlobalAugmentationId),
    DeferredAmbientModule(DeferredModuleId),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum MetadataName {
    Identifier(String),
    StringLiteral(String),
}

impl MetadataName {
    pub fn text(&self) -> &str {
        match self {
            Self::Identifier(name) | Self::StringLiteral(name) => name,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AliasContext {
    ValidAmbient,
    InvalidFutureTk1194,
    InvalidAugmentationFutureTk2666,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AliasSpaceIntent {
    Type,
    UnresolvedValueOrType,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum QualifiedTypePathDeferredReason {
    Import,
    Enum,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QualifiedTypePathResolution {
    TypeGroup(TypeGroupId),
    MissingRoot {
        segment: usize,
    },
    TypeOnlyRoot {
        segment: usize,
    },
    MissingMember {
        segment: usize,
    },
    TypeOnlyIntermediate {
        segment: usize,
    },
    ValueOnlyLeaf {
        segment: usize,
    },
    Unavailable {
        segment: usize,
    },
    Deferred {
        segment: usize,
        reason: QualifiedTypePathDeferredReason,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QualifiedValuePathResolution {
    Value(ValueStorageId),
    MissingRoot {
        segment: usize,
    },
    TypeOnlyRoot {
        segment: usize,
    },
    MissingMember {
        segment: usize,
    },
    NamespaceOnlyLeaf {
        segment: usize,
    },
    Unavailable {
        segment: usize,
    },
    Deferred {
        segment: usize,
        reason: QualifiedTypePathDeferredReason,
    },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LocalAmbientExportAliasFailure {
    pub origin: CompilationOrigin,
    pub local_span: Span,
    pub local_name: String,
    pub kind: LocalAmbientExportAliasFailureKind,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LocalAmbientExportAliasFailureKind {
    Missing,
    NonLocal,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum NamespaceInstanceState {
    NonInstantiated,
    Instantiated,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum VariableKind {
    Var,
    Let,
    Const,
    Using,
    AwaitUsing,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ImportBindingForm {
    Named,
    Default,
    Namespace,
    ImportEquals,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ImportSyntaxFacts {
    pub form: ImportBindingForm,
    pub outer_type_only: bool,
    pub specifier_type_only: bool,
    pub external_reference: bool,
    pub exported: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DeclarationSyntaxFacts {
    None,
    Variable(VariableKind),
    Import(ImportSyntaxFacts),
    Enum { constant: bool },
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeclarationSpaces {
    pub value: bool,
    pub r#type: bool,
    pub namespace: bool,
}

impl DeclarationSpaces {
    pub(super) const NONE: Self = Self {
        value: false,
        r#type: false,
        namespace: false,
    };
    pub(super) const VALUE: Self = Self {
        value: true,
        r#type: false,
        namespace: false,
    };
    pub(super) const TYPE: Self = Self {
        value: false,
        r#type: true,
        namespace: false,
    };
    pub(super) const VALUE_TYPE: Self = Self {
        value: true,
        r#type: true,
        namespace: false,
    };
    pub(super) const ALIAS: Self = Self {
        value: true,
        r#type: true,
        namespace: false,
    };
    pub(super) const NAMESPACE: Self = Self {
        value: false,
        r#type: false,
        namespace: true,
    };
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MergeDeclarationKind {
    Variable,
    Function,
    Class,
    TypeAlias,
    Interface,
    Enum,
    Namespace,
    ImportAlias,
    DeferredExport,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NamespaceMember {
    pub id: NamespaceMemberId,
    pub owner: NamespaceMemberOwner,
    pub target: DeclarationOwner,
    pub declaration: Option<DeclId>,
    pub symbol: Option<SymbolId>,
    pub local_symbol: Option<SymbolId>,
    pub name: Option<String>,
    pub local_name: Option<MetadataName>,
    pub exported_name: Option<MetadataName>,
    pub declaration_span: Span,
    pub specifier_span: Option<Span>,
    pub binding_span: Span,
    pub local_span: Option<Span>,
    pub exported_span: Option<Span>,
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub module_specifier: Option<MetadataName>,
    pub outer_type_only: bool,
    pub specifier_type_only: bool,
    pub alias_context: Option<AliasContext>,
    pub alias_resolution: Option<ExportResolutionDisposition>,
    pub alias_space_intent: Option<AliasSpaceIntent>,
    pub export_context: Option<ExportContextId>,
    pub syntax: DeclarationSyntaxFacts,
    pub spaces: DeclarationSpaces,
    pub kind: MergeDeclarationKind,
    pub publication: NamespacePublication,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GlobalOwner {
    Lexical(ScopeId),
    NamespaceFragment(NamespaceFragmentId),
    DeferredAmbientModule(DeferredModuleId),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GlobalPlacement {
    DirectExternalModule,
    DirectScript,
    DeferredAmbientModule,
    NestedNamespace,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GlobalIssue {
    FutureTk2669,
    FutureTk2670,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlobalAugmentation {
    pub id: GlobalAugmentationId,
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub module: ScopeId,
    pub owner: GlobalOwner,
    pub body_span: Span,
    pub diagnostic_span: Span,
    pub target_scope: ScopeId,
    pub overlay_scope: ScopeId,
    pub placement: GlobalPlacement,
    pub issues: Vec<GlobalIssue>,
    pub declared: bool,
    pub members: Vec<NamespaceMemberId>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeferredAmbientModule {
    pub id: DeferredModuleId,
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub module: ScopeId,
    pub owner: DeclarationOwner,
    pub kind: DeferredModuleKind,
    pub specifier: String,
    pub span: Span,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DeferredModuleKind {
    AmbientExternalModule,
    ModuleAugmentation,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DeferredChildKind {
    OrdinaryDeclaration,
    ExportDeclaration,
    NamespaceDeclaration,
    DeferredExport,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeferredAmbientChild {
    pub module: DeferredModuleId,
    pub declaration: Option<DeclId>,
    pub kind: DeferredChildKind,
    pub name: Option<MetadataName>,
    pub span: Span,
    pub binding_span: Option<Span>,
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ExportContextOwner {
    NamespaceFragment(NamespaceFragmentId),
    GlobalAugmentation(GlobalAugmentationId),
    DeferredAmbientModule(DeferredModuleId),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ExportContextKind {
    NamedList,
    WrappedDeclaration,
    ExportAll,
    ExportDefault,
    ExportAssignment,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ExportSyntaxDisposition {
    Valid,
    FutureTk1194,
    FutureTk1319,
    FutureTk1063,
    FutureTk2666,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ExportResolutionDisposition {
    NotRequired,
    DeferredBacklog15,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExportContext {
    pub id: ExportContextId,
    pub owner: ExportContextOwner,
    pub kind: ExportContextKind,
    pub syntax: ExportSyntaxDisposition,
    pub resolution: ExportResolutionDisposition,
    pub has_module_specifier: bool,
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub span: Span,
    pub members: Vec<NamespaceMemberId>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum UmdContext {
    FutureTk1316Nested,
    FutureTk1314NonExternal,
    FutureTk1315Implementation,
    DeferredValidBacklog15,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UmdNamespaceExport {
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub module: ScopeId,
    pub owner: DeclarationOwner,
    pub name: String,
    pub span: Span,
    pub context: UmdContext,
    pub export_assignment_target: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MergeSlotDisposition {
    Empty,
    Single,
    AdmittedMerge,
    Deferred,
    Rejected,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MergeSlotState {
    pub declarations: usize,
    pub disposition: MergeSlotDisposition,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MergeSlotSummary {
    pub value: MergeSlotState,
    pub r#type: MergeSlotState,
    pub namespace: MergeSlotState,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MergeCompositionKind {
    IndependentOccupiedSlots,
    InterfaceGroup,
    FunctionGroup,
    VariableGroup,
    NamespaceGroup,
    ClassInterface,
    FunctionNamespace,
    ClassNamespace,
    InterfaceNamespace,
    VariableNamespaceNonInstantiated,
    VariableNamespaceRuntime,
    ImportNamespace(ImportBindingForm),
    EnumComposition,
    ConflictingValueDeclarations,
    ConflictingTypeDeclarations,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MergeDisposition {
    Admitted,
    DeferredBacklog15,
    DeferredBacklog42,
    RejectedFutureTk2567,
    RejectedFutureTk2440,
    RejectedRedeclaration,
    RejectedRuntimeNamespace,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct MergeComposition {
    pub kind: MergeCompositionKind,
    pub disposition: MergeDisposition,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MergeClassification {
    pub slots: MergeSlotSummary,
    pub compositions: Vec<MergeComposition>,
    pub disposition: MergeDisposition,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PlacementIssueKind {
    FutureTk2434,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PlacementIssue {
    pub kind: PlacementIssueKind,
    pub owner: DeclId,
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub span: Span,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MergeParticipant {
    pub declaration: DeclId,
    pub kind: MergeDeclarationKind,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub span: Span,
    pub ambient: bool,
    pub spaces: DeclarationSpaces,
    pub syntax: DeclarationSyntaxFacts,
    pub namespace_fragment: Option<NamespaceFragmentId>,
    pub namespace_instance: Option<NamespaceInstanceState>,
    pub binding_span: Span,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MergeRecord {
    pub owner: DeclarationOwner,
    /// Shares the interned allocation behind the group's [`NameId`]; derefs as `&str`.
    pub name: Arc<str>,
    /// Shares the placement map's participant vector — classification reads the rows the
    /// placement lane already owns instead of copying them.
    pub declarations: Arc<Vec<MergeParticipant>>,
    pub classification: MergeClassification,
    pub placement_issues: Vec<PlacementIssue>,
}

/// Whether the checker may construct one enum descriptor for a canonical type group.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum EnumGroupBuildability {
    Buildable,
    RejectedFutureTk2567,
}

/// Enum merge issue retained for exact checker diagnostic ownership.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum EnumGroupIssueKind {
    FutureTk2567,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct EnumGroupIssue {
    pub kind: EnumGroupIssueKind,
    pub owner: DeclId,
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub span: Span,
}

/// Frozen binder input for one enum descriptor, keyed by its real type group.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EnumGroupView<'a> {
    pub group: TypeGroupId,
    pub owner: DeclarationOwner,
    pub name: &'a str,
    pub declarations: Vec<MergeParticipant>,
    pub namespace: Option<NamespaceId>,
    pub namespace_declarations: Vec<MergeParticipant>,
    pub buildability: EnumGroupBuildability,
    pub issues: Vec<EnumGroupIssue>,
    pub placement_issues: Vec<PlacementIssue>,
}

/// Whole-group decision consumed by the class/function namespace lanes.
///
/// The decision is intentionally derived from the aggregate classification, never
/// from one apparently legal pair. The exact enum/function/namespace chimera keeps
/// its callable recovery separate from admitted publication.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum NamespaceValueAttachmentDisposition {
    AdmittedFunction,
    AdmittedClass,
    AdmittedEnum,
    DeferredFunctionBacklog42,
    TypeContainerOnly,
    Rejected(MergeDisposition),
}

/// One exported namespace value declaration exposed to an owner draft or typed recovery.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct AttachedNamespaceValueMember<'a> {
    pub member: NamespaceMemberId,
    pub declaration: DeclId,
    pub name: &'a str,
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub scope: ScopeId,
    pub site: DeclarationSite,
    pub value_storage: Option<ValueStorageId>,
    pub symbol: Option<SymbolId>,
    pub kind: MergeDeclarationKind,
    pub variable_kind: Option<VariableKind>,
    pub publication: NamespacePublication,
    pub ambient: bool,
}

/// Frozen binder view of all namespace fragments attached to one same-name group.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NamespaceValueAttachment<'a> {
    pub owner: DeclarationOwner,
    pub name: &'a str,
    pub symbol: SymbolId,
    pub disposition: NamespaceValueAttachmentDisposition,
    pub fragments: Vec<&'a NamespaceFragment>,
    pub members: Vec<AttachedNamespaceValueMember<'a>>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct StandaloneNamespaceValueMember<'a> {
    pub member: NamespaceMemberId,
    pub declaration: Option<DeclId>,
    pub symbol: Option<SymbolId>,
    pub name: Option<&'a str>,
    pub source: SourceUnitKey,
    pub site: Option<DeclarationSite>,
    pub declaration_span: Span,
    pub local_span: Option<Span>,
    pub origin: CompilationOrigin,
    pub value_storage: Option<ValueStorageId>,
    pub alias_target_storage: Option<ValueStorageId>,
    pub ambient: bool,
    pub child_namespace: Option<NamespaceId>,
    pub kind: MergeDeclarationKind,
    pub publication: NamespacePublication,
    pub spaces: DeclarationSpaces,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StandaloneNamespaceValueAttachment<'a> {
    pub namespace: NamespaceId,
    pub storage: ValueStorageId,
    pub(crate) symbol: SymbolId,
    pub fragments: Vec<&'a NamespaceFragment>,
    pub members: Vec<StandaloneNamespaceValueMember<'a>>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceUnitRecord {
    pub source: SourceUnitKey,
    pub origin: CompilationOrigin,
    pub module: ScopeId,
    pub context: ModuleBindingContext,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) struct NamespaceKey {
    pub(super) owner: NamespaceOwner,
    pub(super) name: String,
}

/// A merge-group name the table stores exactly once. Merge keys and merge records carry this
/// id instead of an owned `String`, so a name costs one allocation per project, not one per
/// key, record and lookup (backlog 94 item 1).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct NameId(pub(super) u32);

impl NameId {
    pub(super) fn index(self) -> usize {
        usize::try_from(self.0).expect("interned name id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub(super) struct MergeKey {
    pub(super) owner: DeclarationOwner,
    pub(super) name: NameId,
}
