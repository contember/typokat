//! Test contract for the project-local semantic candidate.

use super::project_local_pass_port::NormalizedCheckerRecordForTest;
#[cfg(any(test, feature = "test-utils"))]
use super::project_local_pass_port::{
    NormalizedDiagnosticForTest, NormalizedIncompleteForTest, NormalizedTypeProbeForTest,
    SignatureProbe,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalProductGroupForTest {
    pub product_key: usize,
    pub container_key: usize,
    pub compilation_global: bool,
    pub name: String,
    pub has_type: bool,
    pub has_value: bool,
    pub has_namespace: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalProductDependencyForTest {
    pub owner: usize,
    pub target: usize,
    pub kind: String,
    pub source_path: String,
    pub source_start: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalMappedProductGroupForTest {
    pub product: ProjectLocalProductGroupForTest,
    pub local_symbol: usize,
    pub local_type_group: Option<usize>,
    pub local_value: Option<usize>,
    pub local_namespace: Option<usize>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalSccForTest {
    pub members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalProductSccForTest {
    pub members: Vec<ProjectLocalProductGroupForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectLocalDemandRootForTest {
    ProductGroup(ProjectLocalProductGroupForTest),
    GlobalObject,
    UserNamespace(String),
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalOrderForTest {
    Forward,
    Reverse,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalSemanticFaultForTest {
    OmitDependencyEdge,
    PublishBeforeSccComplete,
    WriteResolvingToDurableCache,
    PublishStaleGlobalObject,
    InvokeForceAllFromOrdinaryDemand,
    PublishProvisionalUnavailableWrite,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone)]
pub struct ProjectLocalSemanticRequestForTest {
    pub case_id: &'static str,
    pub source_order: ProjectLocalOrderForTest,
    pub demand_order: ProjectLocalOrderForTest,
    pub demand_roots: Vec<ProjectLocalDemandRootForTest>,
    pub force_all: bool,
    pub fault: Option<ProjectLocalSemanticFaultForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalSourceInputForTest {
    pub path: String,
    pub byte_len: usize,
    pub sha256: String,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalTerminalStateForTest {
    Resolving,
    Ready,
    Unavailable(ProjectLocalUnavailableReasonForTest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalUnavailableReasonForTest {
    UnsupportedProductRecipe,
    #[cfg(any(test, feature = "test-utils"))]
    UnsupportedUserDeclaration,
    #[cfg(any(test, feature = "test-utils"))]
    InvalidUserDeclaration,
    #[cfg(any(test, feature = "test-utils"))]
    CyclicUserDeclaration,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalNamespaceFragmentForTest {
    pub source_path: String,
    pub origin: ProjectLocalContributorOriginForTest,
    pub source_start: u32,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalUnavailableTerminalForTest {
    pub name: String,
    pub fragments: Vec<ProjectLocalNamespaceFragmentForTest>,
    pub reason: ProjectLocalUnavailableReasonForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalUnavailableAuditForTest {
    pub terminal: ProjectLocalUnavailableTerminalForTest,
    pub discarded_provisional_writes: Vec<String>,
    pub published_provisional_writes: Vec<String>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalSccPublicationForTest {
    pub scc: ProjectLocalSccForTest,
    pub reserved_members: Vec<String>,
    pub published_members: Vec<String>,
    pub observed_before_complete: Vec<String>,
    pub state_before_publish: ProjectLocalTerminalStateForTest,
    pub final_state: ProjectLocalTerminalStateForTest,
    pub discarded_provisional_writes: Vec<String>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalPublicationSccCommitForTest {
    pub scc: ProjectLocalProductSccForTest,
    pub reserved_members: Vec<ProjectLocalProductGroupForTest>,
    pub published_members: Vec<ProjectLocalProductGroupForTest>,
    pub observed_before_complete: Vec<ProjectLocalProductGroupForTest>,
    pub state_before_publish: ProjectLocalTerminalStateForTest,
    pub final_state: ProjectLocalTerminalStateForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalCacheWriteForTest {
    pub owner: ProjectLocalProductGroupForTest,
    pub state: ProjectLocalTerminalStateForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalRepeatedDemandForTest {
    pub owner: ProjectLocalProductGroupForTest,
    pub first_identity: String,
    pub repeated_identity: String,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalForceAllTraceForTest {
    pub requested_product_groups: Vec<ProjectLocalProductGroupForTest>,
    pub additional_materialized_product_groups: Vec<ProjectLocalProductGroupForTest>,
    pub additional_materialized_owner_work_count: usize,
    pub production_validation_roots: Vec<ProjectLocalProductGroupForTest>,
    pub terminal_census: Vec<ProjectLocalForceAllTerminalRowForTest>,
    pub semantic_probes: ProjectLocalSemanticProbesForTest,
    pub library_records: Vec<ProjectLocalLibraryRecordForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalForceAllTerminalRowForTest {
    pub group: ProjectLocalProductGroupForTest,
    pub terminal: ProjectLocalForceAllTerminalForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectLocalForceAllTerminalForTest {
    Ready {
        type_surface: Option<ProjectLocalForceAllReadySurfaceForTest>,
        value_surface: Option<ProjectLocalForceAllReadySurfaceForTest>,
    },
    Unavailable(ProjectLocalUnavailableReasonForTest),
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectLocalForceAllReadySurfaceForTest {
    Object(ProjectLocalForceAllObjectShapeForTest),
    Alias(ProjectLocalForceAllAliasTerminalForTest),
    Other(ProjectLocalForceAllTypeKindForTest),
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalForceAllObjectShapeForTest {
    pub property_count: usize,
    pub call_signature_count: usize,
    pub construct_signature_count: usize,
    pub has_string_index: bool,
    pub has_number_index: bool,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectLocalForceAllAliasTerminalForTest {
    Ready {
        parameter_count: usize,
        body: Box<ProjectLocalForceAllReadySurfaceForTest>,
    },
    Unavailable(ProjectLocalForceAllAliasUnavailableForTest),
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalForceAllAliasUnavailableForTest {
    DirectCycle,
    UnsupportedBody,
    ComponentFailure,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalForceAllTypeKindForTest {
    Intrinsic(ProjectLocalForceAllIntrinsicKindForTest),
    Literal(ProjectLocalForceAllLiteralKindForTest),
    Object,
    Union,
    Intersection,
    Function,
    TypeParameter,
    Array,
    Tuple,
    Readonly,
    Conditional,
    Instantiation,
    Infer,
    Mapped,
    MappedValue,
    Template,
    Keyof,
    ClassInstance,
    DeferredIndexedAccess,
    Declared,
    Enum,
    EnumMember,
    AliasTemplate,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalForceAllLiteralKindForTest {
    Number,
    String,
    Boolean,
    WellKnownSymbol,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalForceAllIntrinsicKindForTest {
    Error,
    Any,
    Unknown,
    Never,
    Void,
    Null,
    Undefined,
    Boolean,
    Number,
    String,
    Uppercase,
    Lowercase,
    Capitalize,
    Uncapitalize,
    ThisType,
    OmitThisParameter,
    Object,
    BigInt,
    Symbol,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalUserEventRecordForTest {
    pub source_path: String,
    pub source_start: u32,
    pub event_ordinal: usize,
    pub record_ordinal: usize,
    pub record: NormalizedCheckerRecordForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalCheckResultForTest {
    pub source_path: String,
    pub diagnostics: Vec<NormalizedDiagnosticForTest>,
    pub incomplete: Vec<NormalizedIncompleteForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalModuleTypeProbeForTest {
    pub source_path: String,
    pub probe: NormalizedTypeProbeForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalCallableMemberProbeForTest {
    pub name: String,
    pub source_path: String,
    pub reservation_source_path: String,
    pub source_start: u32,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalValueProbeForTest {
    pub name: String,
    pub declaration_count: usize,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
    pub member_names: Vec<String>,
    pub callable_members: Vec<ProjectLocalCallableMemberProbeForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalSemanticProbesForTest {
    pub global_types: Vec<NormalizedTypeProbeForTest>,
    pub module_types: Vec<ProjectLocalModuleTypeProbeForTest>,
    pub global_values: Vec<ProjectLocalValueProbeForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectLocalContributorOriginForTest {
    Library,
    User,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectLocalContributorKindForTest {
    Ordinary,
    Namespace,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalGlobalContributorSiteForTest {
    pub source_path: String,
    pub origin: ProjectLocalContributorOriginForTest,
    pub span_start: u32,
    pub span_end: u32,
    pub kind: ProjectLocalContributorKindForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalGlobalContributorForTest {
    pub name: String,
    pub has_value: bool,
    pub has_type: bool,
    pub has_namespace: bool,
    pub sites: Vec<ProjectLocalGlobalContributorSiteForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalLibraryRecordForTest {
    pub source_path: String,
    pub owner_start: u32,
    pub record: NormalizedCheckerRecordForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalSemanticTraceForTest {
    pub case_id: String,
    pub project_session_identity: u64,
    pub publisher_session_identity: u64,
    pub source_inputs: Vec<ProjectLocalSourceInputForTest>,
    pub source_visit_rows: Vec<String>,
    pub admitted_product_groups: Vec<ProjectLocalProductGroupForTest>,
    pub mapped_product_groups: Vec<ProjectLocalMappedProductGroupForTest>,
    pub product_dependencies: Vec<ProjectLocalProductDependencyForTest>,
    pub product_publication_sccs: Vec<ProjectLocalProductSccForTest>,
    pub demand_root_visits: Vec<ProjectLocalDemandRootForTest>,
    pub materialized_product_groups: Vec<ProjectLocalProductGroupForTest>,
    pub materialized_owner_work_count: usize,
    pub production_validation_roots: Vec<ProjectLocalProductGroupForTest>,
    pub production_library_records: Vec<ProjectLocalLibraryRecordForTest>,
    pub semantic_probes: ProjectLocalSemanticProbesForTest,
    pub user_event_ledger: Vec<ProjectLocalUserEventRecordForTest>,
    pub user_check_results: Vec<ProjectLocalCheckResultForTest>,
    pub unavailable_audits: Vec<ProjectLocalUnavailableAuditForTest>,
    pub scc_publications: Vec<ProjectLocalSccPublicationForTest>,
    pub publication_scc_commits: Vec<ProjectLocalPublicationSccCommitForTest>,
    pub durable_cache_writes: Vec<ProjectLocalCacheWriteForTest>,
    pub final_global_contributors: Vec<ProjectLocalGlobalContributorForTest>,
    pub published_global_contributors: Vec<ProjectLocalGlobalContributorForTest>,
    pub repeated_demands: Vec<ProjectLocalRepeatedDemandForTest>,
    pub ordinary_force_all_invocations: Vec<String>,
    pub force_all: Option<ProjectLocalForceAllTraceForTest>,
}
