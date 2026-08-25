//! Test contract for the project-local semantic candidate.

use super::project_local_pass_port::{
    NormalizedCheckerRecordForTest, NormalizedDiagnosticForTest, NormalizedIncompleteForTest,
    NormalizedTypeProbeForTest, SignatureProbe,
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalSccForTest {
    pub members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalProductSccForTest {
    pub members: Vec<ProjectLocalProductGroupForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectLocalDemandRootForTest {
    ProductGroup(ProjectLocalProductGroupForTest),
    GlobalObject,
    UserNamespace(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalOrderForTest {
    Forward,
    Reverse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalSemanticFaultForTest {
    OmitDependencyEdge,
    PublishBeforeSccComplete,
    WriteResolvingToDurableCache,
    PublishStaleGlobalObject,
    InvokeForceAllFromOrdinaryDemand,
    PublishProvisionalUnavailableWrite,
}

#[derive(Clone)]
pub struct ProjectLocalSemanticRequestForTest {
    pub case_id: &'static str,
    pub source_order: ProjectLocalOrderForTest,
    pub demand_order: ProjectLocalOrderForTest,
    pub demand_roots: Vec<ProjectLocalDemandRootForTest>,
    pub force_all: bool,
    pub fault: Option<ProjectLocalSemanticFaultForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalSourceInputForTest {
    pub path: String,
    pub byte_len: usize,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalTerminalStateForTest {
    Resolving,
    Ready,
    Unavailable(ProjectLocalUnavailableReasonForTest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalUnavailableReasonForTest {
    UnsupportedProductRecipe,
    UnsupportedUserDeclaration,
    InvalidUserDeclaration,
    CyclicUserDeclaration,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalNamespaceFragmentForTest {
    pub source_path: String,
    pub origin: ProjectLocalContributorOriginForTest,
    pub source_start: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalUnavailableTerminalForTest {
    pub name: String,
    pub fragments: Vec<ProjectLocalNamespaceFragmentForTest>,
    pub reason: ProjectLocalUnavailableReasonForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalUnavailableAuditForTest {
    pub terminal: ProjectLocalUnavailableTerminalForTest,
    pub discarded_provisional_writes: Vec<String>,
    pub published_provisional_writes: Vec<String>,
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalPublicationSccCommitForTest {
    pub scc: ProjectLocalProductSccForTest,
    pub reserved_members: Vec<ProjectLocalProductGroupForTest>,
    pub published_members: Vec<ProjectLocalProductGroupForTest>,
    pub observed_before_complete: Vec<ProjectLocalProductGroupForTest>,
    pub state_before_publish: ProjectLocalTerminalStateForTest,
    pub final_state: ProjectLocalTerminalStateForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalCacheWriteForTest {
    pub owner: ProjectLocalProductGroupForTest,
    pub state: ProjectLocalTerminalStateForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalRepeatedDemandForTest {
    pub owner: ProjectLocalProductGroupForTest,
    pub first_identity: String,
    pub repeated_identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalForceAllTraceForTest {
    pub materialized_product_groups: Vec<ProjectLocalProductGroupForTest>,
    pub materialized_owner_work_count: usize,
    pub semantic_probes: ProjectLocalSemanticProbesForTest,
    pub library_records: Vec<ProjectLocalLibraryRecordForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalUserEventRecordForTest {
    pub source_path: String,
    pub source_start: u32,
    pub event_ordinal: usize,
    pub record_ordinal: usize,
    pub record: NormalizedCheckerRecordForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalCheckResultForTest {
    pub source_path: String,
    pub diagnostics: Vec<NormalizedDiagnosticForTest>,
    pub incomplete: Vec<NormalizedIncompleteForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalModuleTypeProbeForTest {
    pub source_path: String,
    pub probe: NormalizedTypeProbeForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalCallableMemberProbeForTest {
    pub name: String,
    pub source_path: String,
    pub reservation_source_path: String,
    pub source_start: u32,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalValueProbeForTest {
    pub name: String,
    pub declaration_count: usize,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
    pub member_names: Vec<String>,
    pub callable_members: Vec<ProjectLocalCallableMemberProbeForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalSemanticProbesForTest {
    pub global_types: Vec<NormalizedTypeProbeForTest>,
    pub module_types: Vec<ProjectLocalModuleTypeProbeForTest>,
    pub global_values: Vec<ProjectLocalValueProbeForTest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectLocalContributorOriginForTest {
    Library,
    User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectLocalContributorKindForTest {
    Ordinary,
    Namespace,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalGlobalContributorSiteForTest {
    pub source_path: String,
    pub origin: ProjectLocalContributorOriginForTest,
    pub span_start: u32,
    pub span_end: u32,
    pub kind: ProjectLocalContributorKindForTest,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectLocalGlobalContributorForTest {
    pub name: String,
    pub has_value: bool,
    pub has_type: bool,
    pub has_namespace: bool,
    pub sites: Vec<ProjectLocalGlobalContributorSiteForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalLibraryRecordForTest {
    pub source_path: String,
    pub owner_start: u32,
    pub record: NormalizedCheckerRecordForTest,
}

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
