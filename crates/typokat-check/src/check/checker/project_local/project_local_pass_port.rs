//! Neutral observable output types for the project-local checker pass.

pub(super) use crate::binder::bind::LibraryBinderUnit;
pub(super) use crate::check::checker::BoundProjectBinder;
pub(super) use crate::frontend::ProjectProgram;
use crate::source::{LibraryFileOrdinal, ModuleOrdinal, UnitSlot};
pub(super) use typokat_library_product::LibraryDeclarationProduct;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureProbe {
    pub parameter_types: Vec<String>,
    pub return_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedSpanForTest {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedDiagnosticForTest {
    pub span: NormalizedSpanForTest,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub elaboration: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedIncompleteForTest {
    pub span: NormalizedSpanForTest,
    pub id: String,
    pub context: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NormalizedCheckerRecordForTest {
    Diagnostic(NormalizedDiagnosticForTest),
    Incomplete(NormalizedIncompleteForTest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedUserEventRecordForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub module_ordinal: ModuleOrdinal,
    pub source_start: u32,
    pub event_ordinal: usize,
    pub record_ordinal: usize,
    pub record: NormalizedCheckerRecordForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedLibraryRecordForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub owner_start: u32,
    pub record: NormalizedCheckerRecordForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedCheckResultForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub module_ordinal: ModuleOrdinal,
    pub unit_slot: UnitSlot,
    pub diagnostics: Vec<NormalizedDiagnosticForTest>,
    pub incomplete: Vec<NormalizedIncompleteForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedTypeProbeForTest {
    pub name: String,
    pub declaration_count: usize,
    pub member_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedModuleTypeProbeForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub probe: NormalizedTypeProbeForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedCallableMemberProbeForTest {
    pub name: String,
    pub source_ordinal: LibraryFileOrdinal,
    pub reservation_source_ordinal: LibraryFileOrdinal,
    pub source_start: u32,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedValueProbeForTest {
    pub name: String,
    pub declaration_count: usize,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
    pub member_names: Vec<String>,
    pub callable_members: Vec<NormalizedCallableMemberProbeForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedSemanticProbesForTest {
    pub global_types: Vec<NormalizedTypeProbeForTest>,
    pub module_types: Vec<NormalizedModuleTypeProbeForTest>,
    pub global_values: Vec<NormalizedValueProbeForTest>,
}
