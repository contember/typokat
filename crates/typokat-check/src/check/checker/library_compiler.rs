//! Source-backed compiler for declaration-library profiles and complete-source project routes.

use super::classes::application::ClassTypeParameterDefault;
use super::context::{CertifiedLibraryValues, CheckerRecordBatch, DeclTypes};
use super::decls::TypeDeclReservationReceipt;
use super::events::{CandidateEffects, EventStore, UserRecordTicket};
use super::events_library::{
    library_record_ticket_key, LibraryEventKey, LibraryEventLedger, LibraryEventLedgerError,
    LibraryRecordTicket, LibrarySemanticReportingAdapter,
};
#[cfg(any(test, feature = "test-utils"))]
use super::lexical_events::LexicalReservationAllocator;
use super::lexical_events::LexicalReservations;
use super::lexical_events_library::library_unit;
#[cfg(any(test, feature = "test-utils"))]
use super::lexical_events_library::ExactUnit;
use super::library_identities::LibraryIdentityTerminal;
use super::library_identities::{LibrarySemanticIdentities, NativeArrayGroups};
#[cfg(any(test, feature = "test-utils"))]
use super::library_reporting::independent_library_reporting_site_descriptors;
use super::library_reporting::LibraryReportingConsumer;
#[cfg(test)]
use super::library_reporting::LibraryReportingFamily;
#[cfg(any(test, feature = "test-utils"))]
use super::library_reporting::LibraryReportingReceipt;
#[cfg(any(test, feature = "test-utils"))]
use super::namespace_values::NamespaceValueRegistry;
use super::namespace_values::{
    FrozenNamespaceValueTerminalSnapshot, FrozenNamespaceValueTerminalSnapshotRow,
    FrozenNamespaceValueTerminals,
};
#[cfg(test)]
use super::replay_index::ReplayReverseEdge;
use super::replay_index::{
    admit_generated_collision_replay_index, baseline_record, AdmittedCollisionReplayIndex,
    CollisionReplayConstructionEvidence, CollisionReplayIndex, CollisionReplayOwnerSite,
    CollisionReplayPlan, OwnerSiteStorageMode, ReplayDependencyTrace, ReplayIndexAdmissionError,
    ReplayIndexAdmissionLimits, ReplayIndexGenerationError, ReplayOwner, ReplayOwnerSite,
    ReplayRootSlot,
};
#[cfg(any(test, feature = "test-utils"))]
use super::replay_index::{
    canonicalize_collision_replay_owner_sites, CollisionReplayEventPhase,
    CollisionReplaySiteProvenance,
};
use super::reporting_record::CheckerRecord;
use super::type_groups::{
    PublishedTypeEnvironment, PublishedTypeGroupSurface, PublishedTypeGroupTerminal,
    PublishedTypeParameterDefault,
};
use super::{
    attach_class_bindings_from_inventory, attach_type_decl_owners, build_pass_with_tickets,
    enqueue_ambient_context_diagnostics, enqueue_enum_merge_diagnostics,
    enqueue_local_ambient_export_alias_diagnostics, enqueue_namespace_placement_diagnostics,
    finish_semantic_effects, private_combined_record_ticket_key,
    reserve_type_decls_for_combined_library_from_inventory,
    reserve_type_decls_for_combined_user_from_inventory, reserve_type_decls_from_inventory,
    FrozenCheckerRuntimeMetadata, FrozenCheckerRuntimeSnapshotParts, PassReporting,
    PassReportingPlan, PrivateCombinedRecordTicket,
};
#[cfg(any(test, feature = "test-utils"))]
use super::{check_bound_user_program_with_final_identity_inspector, BoundUserBase};
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::bind::LibraryBinderCheckpointEnds;
use crate::binder::bind::{
    ImportPlaceholder, LibraryBinderCheckpoint, LibraryBinderUnit, ProjectBinderBuilder,
};
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::declaration::TypeFragmentKind;
use crate::binder::declaration::{
    source_global_binding_census_with_provenance, SourceBindingSlot, SourceDeclarationManifest,
    SourceGlobalContributorKind, TypeGroupId, ValueStorageId,
};
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::declaration::{DeclId, DeclarationKind};
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::namespace::MergeDeclarationKind;
use crate::binder::namespace::{
    exact_key, source_file_kind, CompilationUnit, ExactKey, ExportContextKind,
    ExportSyntaxDisposition, ModuleBindingContext, NamespaceId, SourceFileKind,
};
use crate::binder::roots::{collect_root_rows, LibraryRootProjection, RootNameRow};
use crate::binder::scope::ScopeId;
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::class_semantics::CanonicalPublishedClassTerminal;
#[cfg(any(test, feature = "test-utils"))]
use crate::class_semantics::DemandOutcome;
use crate::class_semantics::OwnedPublishedClassTerminal;
#[cfg(any(test, feature = "test-utils"))]
use crate::diagnostics::render_type;
use crate::diagnostics::{render_to_writer_with_format, DiagnosticFormat};
use crate::source::{
    CompilationOrigin, LibraryFileOrdinal, ModuleOrdinal, SourceOrdinal, SourceUnit, UnitSlot,
};
use crate::span::Span;
use crate::types::repr::ClassId;
use crate::types::repr::TypeParamId;
#[cfg(any(test, feature = "test-utils"))]
use crate::types::repr::{
    EnumValue, IntrinsicKind, LiteralValue, ModifierOp, PropertyKey, TypeTag, Visibility,
    WellKnownSymbol,
};
use crate::types::repr::{ObjectType, PropertyType};
#[cfg(any(test, feature = "test-utils"))]
use crate::types::store::Store;
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;
#[cfg(any(test, feature = "test-utils"))]
use sha2::{Digest, Sha256};
use std::cell::Cell;
#[cfg(any(test, feature = "test-utils"))]
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Condvar, Mutex};
#[cfg(any(test, feature = "test-utils"))]
use std::time::{Duration, Instant};

#[cfg(not(any(test, feature = "test-utils")))]
use super::BoundUserBase;
use super::{
    bind_authoritative_project_core_with_module_admission,
    bind_library_checkpoint_project_programs, build_validated_default_module_plan,
    build_validated_source_reexport_plan, context, decls, emit_test_incomplete, events_library,
    library_identities, replay_index, seed_module_placeholder_errors, type_groups,
    AuthoritativeProjectBinderFinish, BoundProjectBinder, CheckResult, ModuleDeclarationSpans,
    Pass, ProjectModuleAdmission, UserReportingOwner, ValidatedDefaultModulePlan,
};
#[cfg(any(test, feature = "test-utils"))]
use super::{
    check_project_programs_with_owned_library,
    record_continuation_project_binding_consumed_for_test, ProjectSourceBindingRow,
};

mod canonical_bytes;
mod collision_plan;
mod compile_api;
mod compile_engine;
mod complete_source;
mod continuation;
mod probes;
mod profile;
mod replay_control;
mod runtime_model;
mod runtime_state;
mod runtime_witness;
mod terminal_validation;
#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "test-utils"))]
pub use canonical_bytes::*;
#[cfg(not(any(test, feature = "test-utils")))]
use canonical_bytes::*;
use collision_plan::*;
pub use compile_api::*;
use compile_engine::*;
pub use complete_source::*;
pub use continuation::*;
#[cfg(any(test, feature = "test-utils"))]
pub use probes::*;
#[cfg(not(any(test, feature = "test-utils")))]
use probes::*;
pub use profile::*;
pub use replay_control::*;
pub use runtime_model::*;
#[cfg(any(test, feature = "test-utils"))]
pub use runtime_state::*;
#[cfg(any(test, feature = "test-utils"))]
pub use runtime_witness::*;
use terminal_validation::*;
