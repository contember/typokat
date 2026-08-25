//! WU4 contract for project-local demand-driven default-library semantics.

use crate::base::UserDeltaProjectInputForTest;
use crate::check::checker::library_compiler::CompleteSourceRouteWorkScopeForTest;
use crate::check::checker::project_local::{
    project_local_semantic_candidate_for_test, ObservationEventForTest,
    ObservationEventKindForTest, ObservationReceiptForTest, ObservationScopeForTest,
    ObservationWitnessKindForTest, ProjectLocalCacheWriteForTest,
    ProjectLocalContributorOriginForTest, ProjectLocalDemandRootForTest,
    ProjectLocalFaultSeamForTest, ProjectLocalForceAllTraceForTest,
    ProjectLocalGlobalContributorForTest, ProjectLocalLibraryRecordForTest,
    ProjectLocalMappedProductGroupForTest, ProjectLocalNamespaceFragmentForTest,
    ProjectLocalOrderForTest, ProjectLocalProductDependencyForTest,
    ProjectLocalProductGroupForTest, ProjectLocalProductSccForTest,
    ProjectLocalPublicationSccCommitForTest, ProjectLocalRepeatedDemandForTest,
    ProjectLocalSemanticFaultForTest, ProjectLocalSemanticPhaseForTest,
    ProjectLocalSemanticProbesForTest, ProjectLocalSemanticRequestForTest,
    ProjectLocalSemanticTraceForTest, ProjectLocalTerminalStateForTest,
    ProjectLocalUnavailableAuditForTest, ProjectLocalUnavailableReasonForTest,
    ProjectLocalUnavailableTerminalForTest, WeakLivenessForTest,
};
use crate::compiler::LibraryCompilerWorkScopeForTest;
use crate::provider::{CompleteSourceBlackBoxOracleForTest, LibraryBaseProvider};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;
use typokat_library_product::{DependencyTarget, LibraryDeclarationProduct};

const ORACLE_BYTES: &[u8] = include_bytes!(
    "../../../tests/cases/sr_project_local_library_candidate/semantic-demand-oracle.json"
);
const ORACLE_SHA256: &str = "4fea94b092cfdbeba4e6ab0c2e25a5fd55f704e067e1f8153e50e09d41593d7e";
const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const TYPESCRIPT_REVISION: &str = "050880ce59e30b356b686bd3144efe24f875ebc8";
const OFFICIAL_FRAME_PATH: &str = "conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatBetweenTupleAndArray.ts";
const OFFICIAL_FRAME_SHA256: &str =
    "8ea2fffa0e48d8727722bb2e52ba98a235d72dbe7ef9848ac355f8c5d82f6a68";

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeProductEvidence {
    groups: Vec<ProjectLocalProductGroupForTest>,
    mapped_groups: Vec<ProjectLocalMappedProductGroupForTest>,
    dependencies: Vec<ProjectLocalProductDependencyForTest>,
    publication_sccs: Vec<ProjectLocalProductSccForTest>,
}

#[derive(Clone)]
struct ObservedCandidateTrace {
    trace: ProjectLocalSemanticTraceForTest,
    observation: ObservationReceiptForTest,
    fault: Option<ProjectLocalSemanticFaultForTest>,
}

struct CandidateEvidence<'a> {
    trace: &'a ProjectLocalSemanticTraceForTest,
    observation: &'a ObservationReceiptForTest,
    fault: Option<ProjectLocalSemanticFaultForTest>,
}

struct CandidateHarnessRequest {
    inputs: Vec<UserDeltaProjectInputForTest<'static>>,
    controls: ProjectLocalSemanticRequestForTest,
}

impl ObservedCandidateTrace {
    const fn evidence(&self) -> CandidateEvidence<'_> {
        CandidateEvidence {
            trace: &self.trace,
            observation: &self.observation,
            fault: self.fault,
        }
    }
}

const fn observation_event(
    stamp: u64,
    kind: ObservationEventKindForTest,
) -> ObservationEventForTest {
    ObservationEventForTest { stamp, kind }
}

fn insert_observation_event(
    receipt: &mut ObservationReceiptForTest,
    index: usize,
    kind: ObservationEventKindForTest,
) {
    for event in &mut receipt.events[index..] {
        event.stamp = event.stamp.saturating_add(1);
    }
    let stamp = receipt.events[index - 1].stamp.saturating_add(1);
    receipt.events.insert(index, observation_event(stamp, kind));
}

const CANDIDATE_SOURCE_PATHS: [&str; 6] = [
    "contract.rs",
    "entry.rs",
    "fault_gate.rs",
    "mod.rs",
    "observation.rs",
    "project_local_pass_port.rs",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CandidateTreeEntryKind {
    Directory,
    File,
}

fn candidate_relative_path(
    root: &std::path::Path,
    path: &std::path::Path,
) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|error| format!("candidate path escaped root: {error}"))?;
    relative
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| "candidate path is not UTF-8".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| components.join("/"))
}

fn enumerate_candidate_tree(
    root: &std::path::Path,
) -> Result<Vec<(String, CandidateTreeEntryKind)>, String> {
    let root_metadata = std::fs::symlink_metadata(root)
        .map_err(|error| format!("candidate root metadata failed: {error}"))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("candidate root is not a real directory".to_owned());
    }
    let mut pending = std::fs::read_dir(root)
        .map_err(|error| format!("candidate root is unreadable: {error}"))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("candidate directory entry is unreadable: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut entries = Vec::new();
    while let Some(path) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("candidate path metadata failed: {error}"))?;
        let relative = candidate_relative_path(root, &path)?;
        if metadata.file_type().is_symlink() {
            return Err(format!("candidate symlink {relative}"));
        }
        if metadata.is_dir() {
            entries.push((relative, CandidateTreeEntryKind::Directory));
            let children = std::fs::read_dir(&path)
                .map_err(|error| format!("candidate directory is unreadable: {error}"))?
                .map(|entry| {
                    entry.map(|entry| entry.path()).map_err(|error| {
                        format!("candidate directory entry is unreadable: {error}")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            pending.extend(children);
        } else if metadata.is_file() {
            entries.push((relative, CandidateTreeEntryKind::File));
        } else {
            return Err(format!("unknown candidate entry {relative}"));
        }
    }
    entries.sort();
    Ok(entries)
}

fn validate_candidate_tree_entries(
    entries: &[(String, CandidateTreeEntryKind)],
) -> Result<(), String> {
    let actual = entries.iter().cloned().collect::<BTreeMap<_, _>>();
    if actual.len() != entries.len() {
        return Err("duplicate candidate tree path".to_owned());
    }
    let expected = CANDIDATE_SOURCE_PATHS
        .into_iter()
        .map(|path| (path.to_owned(), CandidateTreeEntryKind::File))
        .collect::<BTreeMap<_, _>>();
    if actual != expected {
        return Err(format!(
            "candidate tree differs from exact allowlist: {actual:?}"
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct CandidateImport {
    path: String,
    binding: Option<String>,
    renamed: bool,
    glob: bool,
}

fn flatten_candidate_use(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    imports: &mut Vec<CandidateImport>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            flatten_candidate_use(&path.tree, prefix, imports);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let mut path = prefix.clone();
            path.push(name.ident.to_string());
            imports.push(CandidateImport {
                path: path.join("::"),
                binding: Some(name.ident.to_string()),
                renamed: false,
                glob: false,
            });
        }
        syn::UseTree::Rename(rename) => {
            let mut path = prefix.clone();
            path.push(rename.ident.to_string());
            imports.push(CandidateImport {
                path: path.join("::"),
                binding: Some(rename.rename.to_string()),
                renamed: true,
                glob: false,
            });
        }
        syn::UseTree::Glob(_) => imports.push(CandidateImport {
            path: format!("{}::*", prefix.join("::")),
            binding: None,
            renamed: false,
            glob: true,
        }),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                flatten_candidate_use(item, prefix, imports);
            }
        }
    }
}

fn candidate_import_allowed(relative: &str, import: &CandidateImport) -> bool {
    if import.renamed {
        return false;
    }
    if import.glob {
        return relative == "mod.rs" && matches!(import.path.as_str(), "contract::*" | "entry::*");
    }
    if relative == "mod.rs" && import.path.starts_with("observation::") {
        return true;
    }
    if import.path.starts_with("super::") {
        return relative != "mod.rs"
            && !import.path["super::".len()..]
                .split("::")
                .any(|segment| segment == "super");
    }
    if matches!(import.path.as_str(), "std::fmt" | "std::cell::RefCell") {
        return true;
    }
    if relative != "project_local_pass_port.rs" {
        return false;
    }
    matches!(
        import.path.as_str(),
        "crate::binder::bind::LibraryBinderUnit"
            | "crate::check::checker::BoundProjectBinder"
            | "crate::frontend::ProjectProgram"
            | "crate::source::LibraryFileOrdinal"
            | "crate::source::ModuleOrdinal"
            | "crate::source::UnitSlot"
            | "typokat_library_product::LibraryDeclarationProduct"
    )
}

fn candidate_path_text(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

struct CandidateSourceAudit<'a> {
    relative: &'a str,
    local_roots: BTreeSet<String>,
    errors: Vec<String>,
}

impl<'a> CandidateSourceAudit<'a> {
    fn new(relative: &'a str, file: &syn::File) -> Self {
        let mut local_roots = [
            "Box", "Err", "None", "Ok", "Option", "Result", "Self", "Some", "String", "Vec",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        for item in &file.items {
            let name = match item {
                syn::Item::Const(item) => Some(&item.ident),
                syn::Item::Enum(item) => Some(&item.ident),
                syn::Item::Fn(item) => Some(&item.sig.ident),
                syn::Item::Macro(item) => item.ident.as_ref(),
                syn::Item::Mod(item) => Some(&item.ident),
                syn::Item::Static(item) => Some(&item.ident),
                syn::Item::Struct(item) => Some(&item.ident),
                syn::Item::Trait(item) => Some(&item.ident),
                syn::Item::TraitAlias(item) => Some(&item.ident),
                syn::Item::Type(item) => Some(&item.ident),
                syn::Item::Union(item) => Some(&item.ident),
                _ => None,
            };
            if let Some(name) = name {
                local_roots.insert(name.to_string());
            }
            if let syn::Item::Use(item) = item {
                let mut imports = Vec::new();
                flatten_candidate_use(&item.tree, &mut Vec::new(), &mut imports);
                for import in imports {
                    if let Some(binding) = import.binding {
                        local_roots.insert(binding);
                    }
                }
            }
        }
        Self {
            relative,
            local_roots,
            errors: Vec::new(),
        }
    }

    fn reject(&mut self, message: impl Into<String>) {
        self.errors
            .push(format!("{}: {}", self.relative, message.into()));
    }

    fn validate_module(&mut self, item: &syn::ItemMod) {
        if item.content.is_some() {
            self.reject(format!("inline module {}", item.ident));
            return;
        }
        let allowed = self.relative == "mod.rs"
            && matches!(
                item.ident.to_string().as_str(),
                "contract" | "entry" | "fault_gate" | "observation" | "project_local_pass_port"
            );
        if !allowed {
            self.reject(format!("undeclared external module {}", item.ident));
        }
    }

    fn validate_path(&mut self, path: &syn::Path) {
        if path.leading_colon.is_some() {
            self.reject(format!("absolute path {}", candidate_path_text(path)));
            return;
        }
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        if matches!(
            segments.last().map(String::as_str),
            Some("append_observation" | "record_observation")
        ) && !matches!(
            self.relative,
            "session.rs" | "terminal.rs" | "cache.rs" | "fault_gate.rs" | "global_owner.rs"
        ) {
            self.reject("audit hook outside an owning module");
        }
        if segments.len() < 2 {
            return;
        }
        let first = &segments[0];
        let text = segments.join("::");
        let allowed = match first.as_str() {
            "std" => {
                text == "std::fmt"
                    || text.starts_with("std::fmt::")
                    || text == "std::cell::RefCell"
                    || text.starts_with("std::cell::RefCell::")
            }
            "core" => false,
            "crate" => {
                self.relative == "project_local_pass_port.rs"
                    && [
                        "crate::binder::bind::LibraryBinderUnit",
                        "crate::check::checker::BoundProjectBinder",
                        "crate::frontend::ProjectProgram",
                        "crate::source::LibraryFileOrdinal",
                        "crate::source::ModuleOrdinal",
                        "crate::source::UnitSlot",
                    ]
                    .iter()
                    .any(|allowed| text == *allowed || text.starts_with(&format!("{allowed}::")))
            }
            "typokat_library_product" => {
                self.relative == "project_local_pass_port.rs"
                    && (text == "typokat_library_product::LibraryDeclarationProduct"
                        || text.starts_with("typokat_library_product::LibraryDeclarationProduct::"))
            }
            "super" => {
                self.relative != "mod.rs" && !segments[1..].iter().any(|segment| segment == "super")
            }
            _ => self.local_roots.contains(first),
        };
        if !allowed {
            self.reject(format!("unapproved path {text}"));
        }
    }
}

impl<'ast> Visit<'ast> for CandidateSourceAudit<'_> {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        let name = candidate_path_text(attribute.path());
        if name == "path" {
            self.reject("#[path] attribute");
        } else if name == "derive" {
            let allowed = attribute
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
                )
                .map(|paths| {
                    paths.iter().all(|path| {
                        matches!(
                            candidate_path_text(path).as_str(),
                            "Clone" | "Copy" | "Debug" | "Eq" | "Ord" | "PartialEq" | "PartialOrd"
                        )
                    })
                })
                .unwrap_or(false);
            if !allowed {
                self.reject("unapproved derive attribute");
            }
        } else if name == "doc" {
            let is_literal = matches!(
                &attribute.meta,
                syn::Meta::NameValue(value)
                    if matches!(&value.value, syn::Expr::Lit(literal) if matches!(&literal.lit, syn::Lit::Str(_)))
            );
            if !is_literal {
                self.reject("non-literal doc attribute");
            }
        } else {
            self.reject(format!("unapproved attribute {name}"));
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if item.leading_colon.is_some() {
            self.reject("absolute import");
        }
        let mut imports = Vec::new();
        flatten_candidate_use(&item.tree, &mut Vec::new(), &mut imports);
        for import in imports {
            if !candidate_import_allowed(self.relative, &import) {
                self.reject(format!("unapproved import {}", import.path));
            }
        }
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.validate_module(item);
        syn::visit::visit_item_mod(self, item);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        self.reject(format!("extern crate {}", item.ident));
    }

    fn visit_item_foreign_mod(&mut self, item: &'ast syn::ItemForeignMod) {
        self.reject("foreign module");
        syn::visit::visit_item_foreign_mod(self, item);
    }

    fn visit_signature(&mut self, signature: &'ast syn::Signature) {
        if signature.unsafety.is_some() {
            self.reject(format!("unsafe function {}", signature.ident));
        }
        syn::visit::visit_signature(self, signature);
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if item.unsafety.is_some() {
            self.reject("unsafe impl");
        }
        syn::visit::visit_item_impl(self, item);
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        if item.unsafety.is_some() {
            self.reject(format!("unsafe trait {}", item.ident));
        }
        syn::visit::visit_item_trait(self, item);
    }

    fn visit_type_bare_fn(&mut self, item: &'ast syn::TypeBareFn) {
        if item.unsafety.is_some() {
            self.reject("unsafe function type");
        }
        syn::visit::visit_type_bare_fn(self, item);
    }

    fn visit_expr_unsafe(&mut self, item: &'ast syn::ExprUnsafe) {
        self.reject("unsafe block");
        syn::visit::visit_expr_unsafe(self, item);
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if matches!(item.mutability, syn::StaticMutability::Mut(_)) {
            self.reject(format!("mutable static {}", item.ident));
        }
        syn::visit::visit_item_static(self, item);
    }

    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        let name = candidate_path_text(&item.path);
        if name != "thread_local" {
            self.reject(format!("unapproved macro {name}"));
            return;
        }
        match syn::parse2::<syn::File>(item.tokens.clone()) {
            Ok(file) => {
                let mut nested = CandidateSourceAudit::new(self.relative, &file);
                nested.local_roots.extend(self.local_roots.iter().cloned());
                nested.visit_file(&file);
                self.errors.extend(nested.errors);
            }
            Err(error) => self.reject(format!("opaque thread_local body: {error}")),
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.validate_path(path);
        syn::visit::visit_path(self, path);
    }
}

fn validate_candidate_rust_source(relative: &str, source: &str) -> Result<(), String> {
    let file = syn::parse_file(source)
        .map_err(|error| format!("{relative}: candidate source is invalid Rust: {error}"))?;
    let mut audit = CandidateSourceAudit::new(relative, &file);
    audit.visit_file(&file);
    if audit.errors.is_empty() {
        Ok(())
    } else {
        Err(audit.errors.join("; "))
    }
}

fn manifest_declares_typokat_library_dependency(value: &toml::Value) -> bool {
    let Some(table) = value.as_table() else {
        return false;
    };
    for dependency_section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dependencies) = table
            .get(dependency_section)
            .and_then(toml::Value::as_table)
        {
            for (name, specification) in dependencies {
                if name == "typokat-library"
                    || specification
                        .as_table()
                        .and_then(|fields| fields.get("package"))
                        .and_then(toml::Value::as_str)
                        == Some("typokat-library")
                {
                    return true;
                }
            }
        }
    }
    table
        .values()
        .any(manifest_declares_typokat_library_dependency)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateValidationError {
    InvalidSession,
    SourceOrder,
    ProductAuthority,
    DemandOrder,
    ProductGraph,
    PassiveOwnerCensus,
    DuplicateOwnerEvent,
    MaterializedTraceDrift,
    ObservableOutput,
    StaleGlobalObject,
    OrdinaryForceAll,
    PublicationSccCensus,
    PrematureSccPublication,
    AtomicSccPublication,
    ResolvingDurableCache,
    DurableCacheCensus,
    RepeatedIdentity,
    InternalPrematureSccPublication,
    InternalAtomicSccPublication,
    ProvisionalUnavailablePublished,
    UnavailableCensus,
    ForceAllPresence,
    ForceAllCensus,
    PassiveSessionLifecycle,
    CachePublisherScope,
    PassiveCacheCensus,
    FaultNotReached,
    FaultSeamMismatch,
    FaultAfterReceipt,
}

#[test]
fn official_semantic_demand_frame_is_exact_and_admitted() {
    let frame = include_bytes!(
        "../../../tests/cases/sr_project_local_library_candidate/official_assignment_compat_between_tuple_and_array.ts"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(frame)),
        OFFICIAL_FRAME_SHA256
    );
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../tooling/official-suite/corpus/manifest.json"
    ))
    .expect("valid official-suite manifest");
    assert_eq!(manifest["sha"], TYPESCRIPT_REVISION);
    assert_eq!(manifest["transport"]["revision"], TYPESCRIPT_REVISION);
    assert!(
        include_str!("../../../tooling/official-suite/scoreboard.txt")
            .lines()
            .any(|row| row.starts_with("IN\t") && row.ends_with(OFFICIAL_FRAME_PATH))
    );
}

#[test]
fn product_native_authority_is_complete_and_preserves_the_exact_partition() {
    let product = LibraryBaseProvider::new()
        .declaration_product()
        .expect("packaged declaration product");
    let evidence = native_product_evidence(&product);
    assert_eq!(evidence.groups.len(), product.group_count());
    assert_eq!(
        evidence.publication_sccs.len(),
        product.dependency_sccs().len()
    );
    assert_eq!(
        evidence.dependencies.len(),
        product
            .dependency_rows()
            .iter()
            .filter(|row| matches!(row.target, DependencyTarget::Group(_)))
            .count()
    );
    let concat = evidence
        .publication_sccs
        .iter()
        .find(|scc| scc.members.iter().any(|row| row.name == "ConcatArray"))
        .expect("ConcatArray publication SCC");
    assert_eq!(concat.members.len(), 1);
    assert_eq!(concat.members[0].name, "ConcatArray");
    let partition = evidence
        .publication_sccs
        .iter()
        .flat_map(|scc| scc.members.iter().map(|row| row.product_key))
        .collect::<BTreeSet<_>>();
    assert_eq!(partition.len(), evidence.groups.len());
}

#[test]
fn unavailable_terminals_use_candidate_native_typed_reasons() {
    let terminals = expected_unavailable_terminals();
    assert_eq!(terminals.len(), 8);
    assert!(terminals.iter().all(|terminal| {
        !terminal.fragments.is_empty()
            && matches!(
                ProjectLocalTerminalStateForTest::Unavailable(terminal.reason),
                ProjectLocalTerminalStateForTest::Unavailable(_)
            )
    }));
    assert!(matches!(
        ProjectLocalTerminalStateForTest::Unavailable(
            ProjectLocalUnavailableReasonForTest::UnsupportedProductRecipe
        ),
        ProjectLocalTerminalStateForTest::Unavailable(_)
    ));
}

#[test]
fn complete_source_black_box_oracle_matches_frozen_wu4_receipt() {
    let provider = LibraryBaseProvider::new();
    let product = provider
        .declaration_product()
        .expect("packaged declaration product");
    let native = native_product_evidence(&product);
    let actual = complete_source_oracles(&provider);
    assert_black_box_controls(&actual);
    let library_records = actual[0].1.library_records.clone();
    for (case_id, oracle) in &actual {
        assert_eq!(
            oracle.library_records, library_records,
            "library force_all drifted for {case_id}"
        );
    }
    let value = json!({
        "schema": 3,
        "profile_identity": PROFILE_IDENTITY,
        "product_native": native_evidence_value(&native),
        "library_force_all": library_records_value(&library_records),
        "cases": actual.iter().map(|(id, oracle)| black_box_case_value(id, oracle)).collect::<Vec<_>>()
    });
    if std::env::var_os("TYPOKAT_PRINT_WU4_ORACLE").is_some() {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("serializable WU4 oracle")
        );
        return;
    }
    assert_eq!(format!("{:x}", Sha256::digest(ORACLE_BYTES)), ORACLE_SHA256);
    assert_eq!(value, frozen_oracle());
}

#[test]
fn project_local_source_boundary_has_no_legacy_or_concurrency_capability() {
    let checker_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../typokat-check/src/check/checker/project_local");
    let entries = enumerate_candidate_tree(&checker_root).expect("candidate tree is readable");
    validate_candidate_tree_entries(&entries).expect("candidate tree matches the exact allowlist");
    for relative in CANDIDATE_SOURCE_PATHS {
        let source = std::fs::read_to_string(checker_root.join(relative))
            .expect("candidate source is readable");
        validate_candidate_rust_source(relative, &source)
            .unwrap_or_else(|error| panic!("candidate AST boundary failed: {error}"));
    }
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../typokat-check/Cargo.toml"),
    )
    .expect("checker manifest is readable");
    let manifest = manifest
        .parse::<toml::Value>()
        .expect("checker manifest is valid TOML");
    assert!(!manifest_declares_typokat_library_dependency(&manifest));
}

#[test]
fn project_local_source_boundary_rejects_obfuscated_capabilities() {
    let mut entries = CANDIDATE_SOURCE_PATHS
        .into_iter()
        .map(|path| (path.to_owned(), CandidateTreeEntryKind::File))
        .collect::<Vec<_>>();
    entries.extend([
        ("hidden".to_owned(), CandidateTreeEntryKind::Directory),
        ("hidden/mod.rs".to_owned(), CandidateTreeEntryKind::File),
    ]);
    assert!(validate_candidate_tree_entries(&entries).is_err());

    let rejected = [
        ("#[ path = \"../hidden.rs\" ] mod hidden;", "#[path]"),
        ("use std::{thread as hidden};", "aliased std::thread"),
        ("use std :: fs;", "spaced std::fs"),
        ("fn hidden() { include ! (\"payload.rs\"); }", "include"),
        (
            "fn hidden() { include_str!(\"payload.rs\"); }",
            "include_str",
        ),
        (
            "fn hidden() { include_bytes!(\"payload.bin\"); }",
            "include_bytes",
        ),
        (
            "#[doc = include_str!(\"payload.md\")] struct Hidden;",
            "attribute source inclusion",
        ),
        ("extern crate core;", "extern crate"),
        ("fn hidden() { unsafe {} }", "unsafe block"),
        ("use ::std::fmt;", "absolute import"),
        (
            "use super::library_compiler;",
            "root import escapes candidate tree",
        ),
        (
            "fn hidden() { super::library_compiler::compile(); }",
            "root path escapes candidate tree",
        ),
        ("mod hidden { fn concealed() {} }", "inline module"),
        ("mod hidden;", "undeclared external module"),
    ];
    for (source, label) in rejected {
        let relative = if label.starts_with("root ") {
            "mod.rs"
        } else {
            "entry.rs"
        };
        assert!(
            validate_candidate_rust_source(relative, source).is_err(),
            "boundary accepted {label}"
        );
    }
    let aliased_manifest = r#"
        [target.'cfg(test)'.dev-dependencies]
        hidden = { package = "typokat-library", path = "../typokat-library" }
    "#
    .parse::<toml::Value>()
    .expect("adversarial manifest is valid TOML");
    assert!(manifest_declares_typokat_library_dependency(
        &aliased_manifest
    ));
}

#[test]
fn passive_legacy_scopes_observe_direct_legacy_work() {
    let provider = LibraryBaseProvider::new();
    let inputs = case_inputs("reflect");
    let route_scope = CompleteSourceRouteWorkScopeForTest::start();
    provider
        .complete_source_black_box_oracle_for_test(&inputs)
        .expect("direct complete-source shadow");
    let route_work = route_scope.finish();
    assert_ne!(
        route_work,
        crate::check::checker::library_compiler::CompleteSourceRouteWorkForTest::default()
    );

    let profile = crate::profile::ExactLibraryProfile::load_packaged().expect("packaged profile");
    let compiler_scope = LibraryCompilerWorkScopeForTest::start();
    crate::compiler::LibraryCompiler::new()
        .compile_binder_checkpoint(&profile)
        .expect("direct legacy binder compiler");
    let compiler_work = compiler_scope.finish();
    assert_eq!(compiler_work.compiles, 1);
    assert!(compiler_work.parses > 0);
    assert!(compiler_work.binds > 0);
}

#[test]
fn candidate_native_validator_rejects_each_known_broken_control() {
    let (native, oracle, roots, trace) = valid_control_trace();
    validate_synthetic_candidate_trace(
        "control",
        &native,
        &oracle,
        &roots,
        (
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        false,
        trace.evidence(),
    )
    .expect("control trace is valid");

    let mut broken = trace.clone();
    broken.trace.product_dependencies.pop();
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &broken,
        CandidateValidationError::ProductAuthority,
    );
    broken = trace.clone();
    broken.trace.publication_scc_commits[0]
        .observed_before_complete
        .push(native.groups[0].clone());
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &broken,
        CandidateValidationError::PrematureSccPublication,
    );
    broken = trace.clone();
    broken.trace.durable_cache_writes[0].state = ProjectLocalTerminalStateForTest::Resolving;
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &broken,
        CandidateValidationError::ResolvingDurableCache,
    );
    broken = trace.clone();
    broken.trace.published_global_contributors.clear();
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &broken,
        CandidateValidationError::StaleGlobalObject,
    );
    broken = trace.clone();
    broken
        .trace
        .ordinary_force_all_invocations
        .push("ordinary".to_owned());
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &broken,
        CandidateValidationError::OrdinaryForceAll,
    );
    broken = trace;
    broken.trace.repeated_demands[0].repeated_identity = "different".to_owned();
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &broken,
        CandidateValidationError::RepeatedIdentity,
    );

    let (_, _, _, mut duplicate) = valid_control_trace();
    insert_observation_event(
        &mut duplicate.observation,
        2,
        ObservationEventKindForTest::OwnerMaterialized {
            phase: ProjectLocalSemanticPhaseForTest::Ordinary,
            product_group: 0,
        },
    );
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &duplicate,
        CandidateValidationError::DuplicateOwnerEvent,
    );
}

#[test]
fn raw_controls_reject_provisional_escape_and_fault_seam_drift() {
    let (_, _, _, mut provisional) = valid_control_trace();
    provisional
        .trace
        .unavailable_audits
        .push(ProjectLocalUnavailableAuditForTest {
            terminal: ProjectLocalUnavailableTerminalForTest {
                name: "Provisional".to_owned(),
                fragments: Vec::new(),
                reason: ProjectLocalUnavailableReasonForTest::UnsupportedUserDeclaration,
            },
            discarded_provisional_writes: vec!["staged".to_owned()],
            published_provisional_writes: vec!["escaped".to_owned()],
        });
    assert_eq!(
        validate_internal_publications("control", &provisional.trace),
        Err(CandidateValidationError::ProvisionalUnavailablePublished)
    );

    let (_, _, _, mut seam) = valid_control_trace();
    insert_observation_event(
        &mut seam.observation,
        1,
        ObservationEventKindForTest::FaultActivated {
            fault: ProjectLocalSemanticFaultForTest::OmitDependencyEdge,
            seam: ProjectLocalFaultSeamForTest::GlobalObjectPublication,
        },
    );
    assert_eq!(
        validate_fault_observation(
            &seam.observation,
            Some(ProjectLocalSemanticFaultForTest::OmitDependencyEdge),
        ),
        Err(CandidateValidationError::FaultSeamMismatch)
    );
    assert_eq!(
        validate_fault_observation(
            &valid_control_trace().3.observation,
            Some(ProjectLocalSemanticFaultForTest::OmitDependencyEdge),
        ),
        Err(CandidateValidationError::FaultNotReached)
    );

    let (_, _, _, mut late) = valid_control_trace();
    insert_observation_event(
        &mut late.observation,
        7,
        ObservationEventKindForTest::FaultActivated {
            fault: ProjectLocalSemanticFaultForTest::OmitDependencyEdge,
            seam: ProjectLocalFaultSeamForTest::DependencyProjection,
        },
    );
    assert_eq!(
        validate_fault_observation(
            &late.observation,
            Some(ProjectLocalSemanticFaultForTest::OmitDependencyEdge),
        ),
        Err(CandidateValidationError::FaultAfterReceipt)
    );
}

#[test]
#[ignore = "RED until the WU4 project-local semantic coordinator exists"]
fn project_local_demands_match_candidate_native_authority_in_both_orders() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let product_native = native_product_evidence(&product);
    let mut project_sessions = BTreeSet::new();
    let mut publisher_sessions = BTreeSet::new();
    let mut call_count = 0usize;
    for (case_id, inputs) in oracle_inputs() {
        let native = mapped_native_evidence(&provider, &product, &inputs, &product_native);
        let oracle = provider
            .complete_source_black_box_oracle_for_test(&inputs)
            .expect("black-box complete-source shadow");
        for source_order in [
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Reverse,
        ] {
            for demand_order in [
                ProjectLocalOrderForTest::Forward,
                ProjectLocalOrderForTest::Reverse,
            ] {
                let roots = case_demand_roots(case_id, &native);
                let trace = candidate_trace(
                    &provider,
                    CandidateHarnessRequest {
                        inputs: inputs.clone(),
                        controls: ProjectLocalSemanticRequestForTest {
                            case_id,
                            source_order,
                            demand_order,
                            demand_roots: roots.clone(),
                            force_all: false,
                            fault: None,
                        },
                    },
                );
                assert!(
                    project_sessions.insert(trace.trace.project_session_identity),
                    "project session identity was reused"
                );
                assert!(
                    publisher_sessions.insert(trace.trace.publisher_session_identity),
                    "publisher session identity was reused"
                );
                call_count = call_count.saturating_add(1);
                validate_candidate_trace(
                    case_id,
                    &native,
                    &oracle,
                    &roots,
                    (source_order, demand_order),
                    false,
                    trace.evidence(),
                )
                .unwrap_or_else(|error| panic!("{case_id}: {error:?}"));
            }
        }
    }
    assert_eq!(project_sessions.len(), call_count);
    assert_eq!(publisher_sessions.len(), call_count);
}

#[test]
#[ignore = "RED until the WU4 project-local semantic coordinator exists"]
fn project_local_sessions_isolate_a_b_and_repeated_a() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let product_native = native_product_evidence(&product);
    let mut traces = Vec::new();
    for case_id in ["isolation-a", "isolation-b", "isolation-a"] {
        let inputs = case_inputs(case_id);
        let native = mapped_native_evidence(&provider, &product, &inputs, &product_native);
        let roots = case_demand_roots(case_id, &native);
        let oracle = provider
            .complete_source_black_box_oracle_for_test(&inputs)
            .expect("black-box complete-source shadow");
        let trace = candidate_trace(
            &provider,
            candidate_request(case_id, inputs, roots.clone(), false, None),
        );
        validate_candidate_trace(
            case_id,
            &native,
            &oracle,
            &roots,
            (
                ProjectLocalOrderForTest::Forward,
                ProjectLocalOrderForTest::Forward,
            ),
            false,
            trace.evidence(),
        )
        .unwrap_or_else(|error| panic!("{case_id}: {error:?}"));
        traces.push(trace);
    }
    let project_sessions = traces
        .iter()
        .map(|trace| trace.trace.project_session_identity)
        .collect::<BTreeSet<_>>();
    let publisher_sessions = traces
        .iter()
        .map(|trace| trace.trace.publisher_session_identity)
        .collect::<BTreeSet<_>>();
    assert_eq!(project_sessions.len(), 3);
    assert_eq!(publisher_sessions.len(), 3);
    assert_eq!(
        traces[0].trace.semantic_probes,
        traces[2].trace.semantic_probes
    );
    assert_ne!(
        traces[0].trace.semantic_probes,
        traces[1].trace.semantic_probes
    );
}

#[test]
#[ignore = "RED until the WU4 project-local semantic coordinator exists"]
fn project_local_sccs_publish_atomically_and_repeat_identities() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let product_native = native_product_evidence(&product);
    for case_id in ["array", "unavailable", "mixed-forward"] {
        let inputs = case_inputs(case_id);
        let native = mapped_native_evidence(&provider, &product, &inputs, &product_native);
        let roots = case_demand_roots(case_id, &native);
        let trace = candidate_trace(
            &provider,
            candidate_request(case_id, inputs.clone(), roots.clone(), false, None),
        );
        let oracle = provider
            .complete_source_black_box_oracle_for_test(&inputs)
            .expect("black-box complete-source shadow");
        validate_candidate_trace(
            case_id,
            &native,
            &oracle,
            &roots,
            (
                ProjectLocalOrderForTest::Forward,
                ProjectLocalOrderForTest::Forward,
            ),
            false,
            trace.evidence(),
        )
        .unwrap_or_else(|error| panic!("{case_id}: {error:?}"));
    }
}

#[test]
#[ignore = "RED until the WU4 project-local semantic coordinator exists"]
fn project_local_force_all_matches_black_box_observations() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let product_native = native_product_evidence(&product);
    for case_id in [
        "array",
        "unavailable",
        "official-assignment-compat-between-tuple-and-array",
    ] {
        let inputs = case_inputs(case_id);
        let native = mapped_native_evidence(&provider, &product, &inputs, &product_native);
        let oracle = provider
            .complete_source_black_box_oracle_for_test(&inputs)
            .expect("black-box complete-source shadow");
        let roots = case_demand_roots(case_id, &native);
        let trace = candidate_trace(
            &provider,
            candidate_request(case_id, inputs, roots.clone(), true, None),
        );
        validate_candidate_trace(
            case_id,
            &native,
            &oracle,
            &roots,
            (
                ProjectLocalOrderForTest::Forward,
                ProjectLocalOrderForTest::Forward,
            ),
            true,
            trace.evidence(),
        )
        .unwrap_or_else(|error| panic!("{case_id}: {error:?}"));
    }
}

#[test]
#[ignore = "RED until the WU4 project-local semantic coordinator exists"]
fn injected_candidate_faults_reach_candidate_native_guards() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let product_native = native_product_evidence(&product);
    for (fault, case_id) in fault_scenarios() {
        let inputs = case_inputs(case_id);
        let native = mapped_native_evidence(&provider, &product, &inputs, &product_native);
        let oracle = provider
            .complete_source_black_box_oracle_for_test(&inputs)
            .expect("black-box complete-source shadow");
        let roots = if fault == ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete {
            let cycle = native
                .publication_sccs
                .iter()
                .find(|scc| scc.members.len() > 1)
                .expect("real nontrivial product SCC");
            vec![ProjectLocalDemandRootForTest::ProductGroup(
                cycle.members[0].clone(),
            )]
        } else {
            case_demand_roots(case_id, &native)
        };
        let trace = candidate_trace(
            &provider,
            candidate_request(case_id, inputs.clone(), roots.clone(), false, Some(fault)),
        );
        assert_eq!(
            validate_candidate_trace(
                case_id,
                &native,
                &oracle,
                &roots,
                (
                    ProjectLocalOrderForTest::Forward,
                    ProjectLocalOrderForTest::Forward,
                ),
                false,
                trace.evidence(),
            ),
            Err(expected_fault_error(fault))
        );
    }
}

#[test]
fn candidate_fault_scenarios_use_their_canonical_reachable_seams() {
    let product = LibraryBaseProvider::new()
        .declaration_product()
        .expect("declaration product");
    let native = native_product_evidence(&product);
    assert!(native
        .publication_sccs
        .iter()
        .any(|scc| scc.members.len() > 1));
    for (fault, case_id) in fault_scenarios() {
        match fault {
            ProjectLocalSemanticFaultForTest::PublishProvisionalUnavailableWrite => {
                assert_eq!(case_id, "unavailable")
            }
            ProjectLocalSemanticFaultForTest::PublishStaleGlobalObject => {
                assert_eq!(case_id, "global-this")
            }
            _ => assert_eq!(case_id, "array"),
        }
    }
}

fn validate_candidate_trace(
    case_id: &str,
    native: &NativeProductEvidence,
    oracle: &CompleteSourceBlackBoxOracleForTest,
    roots: &[ProjectLocalDemandRootForTest],
    orders: (ProjectLocalOrderForTest, ProjectLocalOrderForTest),
    force_all_requested: bool,
    candidate: CandidateEvidence<'_>,
) -> Result<(), CandidateValidationError> {
    validate_candidate_trace_with_weak_mode(
        case_id,
        native,
        oracle,
        roots,
        orders,
        ValidationMode {
            force_all_requested,
            allow_synthetic_weak_schema: false,
        },
        candidate,
    )
}

fn validate_synthetic_candidate_trace(
    case_id: &str,
    native: &NativeProductEvidence,
    oracle: &CompleteSourceBlackBoxOracleForTest,
    roots: &[ProjectLocalDemandRootForTest],
    orders: (ProjectLocalOrderForTest, ProjectLocalOrderForTest),
    force_all_requested: bool,
    candidate: CandidateEvidence<'_>,
) -> Result<(), CandidateValidationError> {
    validate_candidate_trace_with_weak_mode(
        case_id,
        native,
        oracle,
        roots,
        orders,
        ValidationMode {
            force_all_requested,
            allow_synthetic_weak_schema: true,
        },
        candidate,
    )
}

#[derive(Clone, Copy)]
struct ValidationMode {
    force_all_requested: bool,
    allow_synthetic_weak_schema: bool,
}

fn validate_candidate_trace_with_weak_mode(
    case_id: &str,
    native: &NativeProductEvidence,
    oracle: &CompleteSourceBlackBoxOracleForTest,
    roots: &[ProjectLocalDemandRootForTest],
    orders: (ProjectLocalOrderForTest, ProjectLocalOrderForTest),
    mode: ValidationMode,
    candidate: CandidateEvidence<'_>,
) -> Result<(), CandidateValidationError> {
    let ValidationMode {
        force_all_requested,
        allow_synthetic_weak_schema,
    } = mode;
    let CandidateEvidence {
        trace,
        observation,
        fault,
    } = candidate;
    let (source_order, demand_order) = orders;
    if trace.case_id != case_id
        || trace.project_session_identity == 0
        || trace.publisher_session_identity == 0
        || trace.project_session_identity == trace.publisher_session_identity
    {
        return Err(CandidateValidationError::InvalidSession);
    }
    validate_fault_observation(observation, fault)?;
    if trace.source_inputs != oracle.source_inputs
        || trace.source_visit_rows != ordered(&oracle.source_visit_rows, source_order)
    {
        return Err(CandidateValidationError::SourceOrder);
    }
    if trace.admitted_product_groups != native.groups
        || trace.mapped_product_groups != native.mapped_groups
        || trace.product_dependencies != native.dependencies
        || trace.product_publication_sccs != native.publication_sccs
    {
        return Err(CandidateValidationError::ProductAuthority);
    }
    if trace.demand_root_visits != ordered(roots, demand_order) {
        return Err(CandidateValidationError::DemandOrder);
    }
    let materialized = demanded_product_closure(native, roots)?;
    let ordinary_observed = observed_product_groups(
        native,
        observation,
        ProjectLocalSemanticPhaseForTest::Ordinary,
    )?;
    if ordinary_observed != materialized
        || trace.materialized_product_groups != ordinary_observed
        || trace.materialized_owner_work_count != ordinary_observed.len()
        || ordinary_observed.len() >= native.groups.len()
    {
        return Err(CandidateValidationError::MaterializedTraceDrift);
    }
    validate_passive_session_lifecycle(trace, observation, allow_synthetic_weak_schema)?;
    let expected_probes = demanded_probe_projection(
        &oracle.force_all_semantic_probes,
        &materialized,
        &oracle.global_contributors,
    );
    if trace.semantic_probes != expected_probes
        || trace.user_event_ledger != oracle.user_event_ledger
        || trace.user_check_results != oracle.user_check_results
    {
        return Err(CandidateValidationError::ObservableOutput);
    }
    if trace.final_global_contributors != oracle.global_contributors
        || trace.published_global_contributors != trace.final_global_contributors
    {
        return Err(CandidateValidationError::StaleGlobalObject);
    }
    if !trace.ordinary_force_all_invocations.is_empty() {
        return Err(CandidateValidationError::OrdinaryForceAll);
    }
    validate_commits(native, &materialized, &trace.publication_scc_commits)?;
    validate_cache_and_repeats(roots, &materialized, trace)?;
    validate_internal_publications(case_id, trace)?;
    match (&trace.force_all, force_all_requested) {
        (None, false) => Ok(()),
        (Some(force_all), true) => validate_force_all(native, oracle, force_all, observation),
        _ => Err(CandidateValidationError::ForceAllPresence),
    }
}

fn validate_commits(
    native: &NativeProductEvidence,
    materialized: &[ProjectLocalProductGroupForTest],
    commits: &[ProjectLocalPublicationSccCommitForTest],
) -> Result<(), CandidateValidationError> {
    let keys = materialized
        .iter()
        .map(|row| row.product_key)
        .collect::<BTreeSet<_>>();
    let expected = native
        .publication_sccs
        .iter()
        .filter(|scc| {
            scc.members
                .iter()
                .any(|row| keys.contains(&row.product_key))
        })
        .collect::<Vec<_>>();
    if commits.len() != expected.len() {
        return Err(CandidateValidationError::PublicationSccCensus);
    }
    for (scc, commit) in expected.into_iter().zip(commits) {
        if commit.scc != *scc
            || commit.reserved_members != scc.members
            || !commit.observed_before_complete.is_empty()
            || commit.state_before_publish != ProjectLocalTerminalStateForTest::Resolving
        {
            return Err(CandidateValidationError::PrematureSccPublication);
        }
        match commit.final_state {
            ProjectLocalTerminalStateForTest::Ready if commit.published_members == scc.members => {}
            ProjectLocalTerminalStateForTest::Unavailable(_)
                if commit.published_members.is_empty() => {}
            _ => return Err(CandidateValidationError::AtomicSccPublication),
        }
    }
    Ok(())
}

fn validate_cache_and_repeats(
    roots: &[ProjectLocalDemandRootForTest],
    materialized: &[ProjectLocalProductGroupForTest],
    trace: &ProjectLocalSemanticTraceForTest,
) -> Result<(), CandidateValidationError> {
    if trace
        .durable_cache_writes
        .iter()
        .any(|row| row.state == ProjectLocalTerminalStateForTest::Resolving)
    {
        return Err(CandidateValidationError::ResolvingDurableCache);
    }
    let mut cached = trace
        .durable_cache_writes
        .iter()
        .map(|row| row.owner.clone())
        .collect::<Vec<_>>();
    let mut expected = materialized.to_vec();
    cached.sort();
    expected.sort();
    if cached != expected {
        return Err(CandidateValidationError::DurableCacheCensus);
    }
    let mut repeated = trace
        .repeated_demands
        .iter()
        .map(|row| row.owner.clone())
        .collect::<Vec<_>>();
    let mut expected_repeated = roots
        .iter()
        .filter_map(|root| match root {
            ProjectLocalDemandRootForTest::ProductGroup(group) => Some(group.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    repeated.sort();
    expected_repeated.sort();
    expected_repeated.dedup();
    if repeated != expected_repeated
        || trace
            .repeated_demands
            .iter()
            .any(|row| row.first_identity != row.repeated_identity)
    {
        return Err(CandidateValidationError::RepeatedIdentity);
    }
    Ok(())
}

fn validate_internal_publications(
    case_id: &str,
    trace: &ProjectLocalSemanticTraceForTest,
) -> Result<(), CandidateValidationError> {
    for publication in &trace.scc_publications {
        if publication.reserved_members != publication.scc.members
            || !publication.observed_before_complete.is_empty()
            || publication.state_before_publish != ProjectLocalTerminalStateForTest::Resolving
        {
            return Err(CandidateValidationError::InternalPrematureSccPublication);
        }
        match publication.final_state {
            ProjectLocalTerminalStateForTest::Ready
                if publication.published_members == publication.scc.members => {}
            ProjectLocalTerminalStateForTest::Unavailable(_)
                if publication.published_members.is_empty()
                    && !publication.discarded_provisional_writes.is_empty() => {}
            _ => return Err(CandidateValidationError::InternalAtomicSccPublication),
        }
    }
    if trace.unavailable_audits.iter().any(|audit| {
        audit.discarded_provisional_writes.is_empty()
            || !audit.published_provisional_writes.is_empty()
    }) {
        return Err(CandidateValidationError::ProvisionalUnavailablePublished);
    }
    validate_unavailable_census(case_id, trace)?;
    Ok(())
}

fn validate_unavailable_census(
    case_id: &str,
    trace: &ProjectLocalSemanticTraceForTest,
) -> Result<(), CandidateValidationError> {
    if case_id != "unavailable" {
        return Ok(());
    }
    let expected = expected_unavailable_terminals();
    let expected_names = expected
        .iter()
        .map(|terminal| terminal.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut audits = trace.unavailable_audits.iter().collect::<Vec<_>>();
    audits.sort_by(|left, right| left.terminal.name.cmp(&right.terminal.name));
    if audits.len() != expected.len()
        || audits.iter().zip(&expected).any(|(audit, terminal)| {
            audit.terminal != *terminal
                || audit.discarded_provisional_writes.is_empty()
                || !audit.published_provisional_writes.is_empty()
        })
    {
        return Err(CandidateValidationError::UnavailableCensus);
    }
    let mut publications = trace
        .scc_publications
        .iter()
        .filter(|publication| {
            publication
                .scc
                .members
                .iter()
                .any(|member| expected_names.contains(member.as_str()))
        })
        .collect::<Vec<_>>();
    publications.sort_by(|left, right| left.scc.members.cmp(&right.scc.members));
    if publications.len() != expected.len() {
        return Err(CandidateValidationError::UnavailableCensus);
    }
    for (publication, terminal) in publications.into_iter().zip(expected) {
        if publication.scc.members != vec![terminal.name]
            || publication.reserved_members != publication.scc.members
            || !publication.published_members.is_empty()
            || !publication.observed_before_complete.is_empty()
            || publication.state_before_publish != ProjectLocalTerminalStateForTest::Resolving
            || publication.final_state
                != ProjectLocalTerminalStateForTest::Unavailable(terminal.reason)
            || publication.discarded_provisional_writes.is_empty()
        {
            return Err(CandidateValidationError::UnavailableCensus);
        }
    }
    Ok(())
}

fn expected_unavailable_terminals() -> Vec<ProjectLocalUnavailableTerminalForTest> {
    use ProjectLocalUnavailableReasonForTest::{
        CyclicUserDeclaration, InvalidUserDeclaration, UnsupportedUserDeclaration,
    };
    const PATH: &str = "/candidate/unavailable.ts";
    const SOURCE: &str = include_str!(
        "../../../tests/cases/b43_namespaces_declaration_merging/wu6a_unavailable_ledger.ts"
    );
    let rows = [
        ("Wu6aClassStaticUnavailable", CyclicUserDeclaration),
        ("Wu6aDuplicateUnavailable", InvalidUserDeclaration),
        ("Wu6aEnumUnavailable", UnsupportedUserDeclaration),
        ("Wu6aImportUnavailable", UnsupportedUserDeclaration),
        (
            "Wu6aInferredInitializerUnavailable",
            UnsupportedUserDeclaration,
        ),
        ("Wu6aInferredReturnUnavailable", UnsupportedUserDeclaration),
        ("Wu6aTdzUnavailable", UnsupportedUserDeclaration),
        ("Wu6aTypeQueryUnavailable", UnsupportedUserDeclaration),
    ];
    rows.into_iter()
        .map(|(name, reason)| {
            let needle = format!("namespace {name}");
            let source_start = u32::try_from(
                SOURCE
                    .find(&needle)
                    .unwrap_or_else(|| panic!("missing unavailable namespace {name}")),
            )
            .expect("unavailable namespace offset fits u32");
            ProjectLocalUnavailableTerminalForTest {
                name: name.to_owned(),
                fragments: vec![ProjectLocalNamespaceFragmentForTest {
                    source_path: PATH.to_owned(),
                    origin: ProjectLocalContributorOriginForTest::User,
                    source_start,
                }],
                reason,
            }
        })
        .collect()
}

fn validate_force_all(
    native: &NativeProductEvidence,
    oracle: &CompleteSourceBlackBoxOracleForTest,
    force_all: &ProjectLocalForceAllTraceForTest,
    observation: &ObservationReceiptForTest,
) -> Result<(), CandidateValidationError> {
    let observed = observed_product_groups(
        native,
        observation,
        ProjectLocalSemanticPhaseForTest::ForceAll,
    )?;
    if observed != native.groups
        || force_all.materialized_product_groups != observed
        || force_all.materialized_owner_work_count != observed.len()
        || force_all.semantic_probes != oracle.force_all_semantic_probes
        || force_all.library_records != oracle.library_records
    {
        return Err(CandidateValidationError::ForceAllCensus);
    }
    Ok(())
}

fn observed_product_groups(
    native: &NativeProductEvidence,
    observation: &ObservationReceiptForTest,
    phase: ProjectLocalSemanticPhaseForTest,
) -> Result<Vec<ProjectLocalProductGroupForTest>, CandidateValidationError> {
    let keys = observation
        .events
        .iter()
        .filter_map(|event| match event.kind {
            ObservationEventKindForTest::OwnerMaterialized {
                phase: event_phase,
                product_group,
            } if event_phase == phase => Some(product_group),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    if keys.iter().any(|key| !seen.insert(*key)) {
        return Err(CandidateValidationError::DuplicateOwnerEvent);
    }
    keys.into_iter()
        .map(|key| {
            native
                .groups
                .get(key)
                .filter(|group| group.product_key == key)
                .cloned()
                .ok_or(CandidateValidationError::PassiveOwnerCensus)
        })
        .collect()
}

fn validate_passive_session_lifecycle(
    trace: &ProjectLocalSemanticTraceForTest,
    observation: &ObservationReceiptForTest,
    allow_synthetic_weak_schema: bool,
) -> Result<(), CandidateValidationError> {
    if observation
        .events
        .windows(2)
        .any(|rows| rows[0].stamp >= rows[1].stamp)
    {
        return Err(CandidateValidationError::PassiveSessionLifecycle);
    }
    let creates = observation
        .events
        .iter()
        .filter_map(|event| match event.kind {
            ObservationEventKindForTest::SessionCreated { project, publisher } => {
                Some((project, publisher))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let drops = observation
        .events
        .iter()
        .filter_map(|event| match event.kind {
            ObservationEventKindForTest::SessionDropped { publisher } => Some(publisher),
            _ => None,
        })
        .collect::<Vec<_>>();
    let receipt_count = observation
        .events
        .iter()
        .filter(|event| matches!(event.kind, ObservationEventKindForTest::ReceiptBuilt))
        .count();
    let witness_kind_ok = observation.witness_kind
        == ObservationWitnessKindForTest::ActualWeakRegistry
        || (allow_synthetic_weak_schema
            && observation.witness_kind == ObservationWitnessKindForTest::SyntheticSchema);
    if !witness_kind_ok
        || creates
            != vec![(
                trace.project_session_identity,
                trace.publisher_session_identity,
            )]
        || drops != vec![trace.publisher_session_identity]
        || receipt_count != 1
        || observation.session_weak
            != vec![WeakLivenessForTest {
                identity: trace.project_session_identity,
                upgraded_at_finish: false,
            }]
        || observation.cache_weak
            != vec![WeakLivenessForTest {
                identity: trace.publisher_session_identity,
                upgraded_at_finish: false,
            }]
    {
        return Err(CandidateValidationError::PassiveSessionLifecycle);
    }
    let observed_cache = observation
        .events
        .iter()
        .filter_map(|event| match event.kind {
            ObservationEventKindForTest::DurableCacheWrite {
                publisher,
                product_group,
            } => Some((publisher, product_group)),
            ObservationEventKindForTest::DurableCacheHit { .. } => None,
            _ => None,
        })
        .map(|(publisher, product_group)| {
            if publisher != trace.publisher_session_identity {
                return Err(CandidateValidationError::CachePublisherScope);
            }
            trace
                .admitted_product_groups
                .get(product_group)
                .filter(|group| group.product_key == product_group)
                .cloned()
                .ok_or(CandidateValidationError::PassiveCacheCensus)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let reported_cache = trace
        .durable_cache_writes
        .iter()
        .map(|row| row.owner.clone())
        .collect::<Vec<_>>();
    if observed_cache != reported_cache {
        return Err(CandidateValidationError::PassiveCacheCensus);
    }
    let observed_hits = observation
        .events
        .iter()
        .filter_map(|event| match event.kind {
            ObservationEventKindForTest::DurableCacheHit {
                publisher,
                product_group,
            } => Some((publisher, product_group)),
            _ => None,
        })
        .map(|(publisher, product_group)| {
            if publisher != trace.publisher_session_identity {
                return Err(CandidateValidationError::CachePublisherScope);
            }
            trace
                .admitted_product_groups
                .get(product_group)
                .filter(|group| group.product_key == product_group)
                .cloned()
                .ok_or(CandidateValidationError::PassiveCacheCensus)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let reported_hits = trace
        .repeated_demands
        .iter()
        .map(|row| row.owner.clone())
        .collect::<Vec<_>>();
    if observed_hits != reported_hits {
        return Err(CandidateValidationError::PassiveCacheCensus);
    }
    Ok(())
}

fn validate_fault_observation(
    observation: &ObservationReceiptForTest,
    injection: Option<ProjectLocalSemanticFaultForTest>,
) -> Result<(), CandidateValidationError> {
    let receipt_stamp = observation
        .events
        .iter()
        .find_map(|event| {
            matches!(event.kind, ObservationEventKindForTest::ReceiptBuilt).then_some(event.stamp)
        })
        .ok_or(CandidateValidationError::PassiveSessionLifecycle)?;
    let activated = observation
        .events
        .iter()
        .filter_map(|event| match event.kind {
            ObservationEventKindForTest::FaultActivated { fault, seam } => {
                Some((event.stamp, fault, seam))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some(expected_fault) = injection else {
        return if activated.is_empty() {
            Ok(())
        } else {
            Err(CandidateValidationError::FaultSeamMismatch)
        };
    };
    let [(stamp, fault, seam)] = activated.as_slice() else {
        return Err(CandidateValidationError::FaultNotReached);
    };
    if *fault != expected_fault || *seam != expected_fault_seam(expected_fault) {
        return Err(CandidateValidationError::FaultSeamMismatch);
    }
    if *stamp >= receipt_stamp {
        return Err(CandidateValidationError::FaultAfterReceipt);
    }
    Ok(())
}

fn native_product_evidence(product: &LibraryDeclarationProduct) -> NativeProductEvidence {
    let groups = product
        .groups()
        .iter()
        .enumerate()
        .map(|(product_key, group)| ProjectLocalProductGroupForTest {
            product_key,
            container_key: group.container.index(),
            compilation_global: matches!(
                product.containers()[group.container.index()].kind,
                typokat_library_product::ContainerKind::CompilationGlobal
            ),
            name: product.strings()[group.name.index()].clone(),
            has_type: group.slots.r#type,
            has_value: group.slots.value,
            has_namespace: group.slots.namespace,
        })
        .collect::<Vec<_>>();
    let mut dependencies = product
        .dependency_rows()
        .iter()
        .filter_map(|row| {
            let DependencyTarget::Group(target) = row.target else {
                return None;
            };
            let source = &product.sources()[row.site.source.index()];
            Some(ProjectLocalProductDependencyForTest {
                owner: row.owner.index(),
                target: target.index(),
                kind: format!("{:?}", row.kind),
                source_path: product.strings()[source.name.index()].clone(),
                source_start: row.site.start,
            })
        })
        .collect::<Vec<_>>();
    dependencies.sort();
    let publication_sccs = product
        .dependency_sccs()
        .into_iter()
        .map(|members| ProjectLocalProductSccForTest {
            members: members
                .into_iter()
                .map(|key| groups[key.index()].clone())
                .collect(),
        })
        .collect();
    NativeProductEvidence {
        groups,
        mapped_groups: Vec::new(),
        dependencies,
        publication_sccs,
    }
}

fn mapped_native_evidence(
    provider: &LibraryBaseProvider,
    product: &LibraryDeclarationProduct,
    inputs: &[UserDeltaProjectInputForTest<'_>],
    native: &NativeProductEvidence,
) -> NativeProductEvidence {
    let prepared = provider
        .prepare_project_declaration_universe_for_test(inputs, false)
        .expect("independent project-local binder map");
    let map = prepared
        .bound
        .library_product_map_for_test()
        .expect("project-local binder map");
    let mapped_groups = native
        .groups
        .iter()
        .map(|group| {
            let key = product
                .group_key_at(group.product_key)
                .expect("validated product group key");
            let binding = map.group(key).expect("mapped product group");
            ProjectLocalMappedProductGroupForTest {
                product: group.clone(),
                local_symbol: binding.symbol().index(),
                local_type_group: binding.ty().map(|identity| identity.index()),
                local_value: binding.value().map(|identity| identity.index()),
                local_namespace: binding.namespace().map(|identity| identity.index()),
            }
        })
        .collect();
    NativeProductEvidence {
        groups: native.groups.clone(),
        mapped_groups,
        dependencies: native.dependencies.clone(),
        publication_sccs: native.publication_sccs.clone(),
    }
}

fn demanded_product_closure(
    native: &NativeProductEvidence,
    roots: &[ProjectLocalDemandRootForTest],
) -> Result<Vec<ProjectLocalProductGroupForTest>, CandidateValidationError> {
    let groups = native
        .groups
        .iter()
        .map(|row| (row.product_key, row))
        .collect::<BTreeMap<_, _>>();
    let mut sccs = BTreeMap::<usize, Vec<usize>>::new();
    for scc in &native.publication_sccs {
        let members = scc
            .members
            .iter()
            .map(|row| row.product_key)
            .collect::<Vec<_>>();
        for member in &members {
            if sccs.insert(*member, members.clone()).is_some() {
                return Err(CandidateValidationError::ProductGraph);
            }
        }
    }
    if sccs.len() != native.groups.len() {
        return Err(CandidateValidationError::ProductGraph);
    }
    let mut dependencies = BTreeMap::<usize, Vec<usize>>::new();
    for row in &native.dependencies {
        dependencies.entry(row.owner).or_default().push(row.target);
    }
    let mut pending = roots
        .iter()
        .filter_map(|root| match root {
            ProjectLocalDemandRootForTest::ProductGroup(group) => Some(group.product_key),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut closure = BTreeSet::new();
    while let Some(owner) = pending.pop() {
        for member in sccs
            .get(&owner)
            .ok_or(CandidateValidationError::ProductGraph)?
        {
            if closure.insert(*member) {
                pending.extend(dependencies.get(member).into_iter().flatten().copied());
            }
        }
    }
    closure
        .into_iter()
        .map(|key| {
            groups
                .get(&key)
                .cloned()
                .cloned()
                .ok_or(CandidateValidationError::ProductGraph)
        })
        .collect()
}

fn demanded_probe_projection(
    probes: &ProjectLocalSemanticProbesForTest,
    materialized: &[ProjectLocalProductGroupForTest],
    contributors: &[ProjectLocalGlobalContributorForTest],
) -> ProjectLocalSemanticProbesForTest {
    let mut names = materialized
        .iter()
        .map(|row| row.name.as_str())
        .collect::<BTreeSet<_>>();
    names.extend(
        contributors
            .iter()
            .filter(|row| {
                row.sites
                    .iter()
                    .any(|site| site.origin == ProjectLocalContributorOriginForTest::User)
            })
            .map(|row| row.name.as_str()),
    );
    ProjectLocalSemanticProbesForTest {
        global_types: probes
            .global_types
            .iter()
            .filter(|row| names.contains(row.name.as_str()))
            .cloned()
            .collect(),
        module_types: probes
            .module_types
            .iter()
            .filter(|row| names.contains(row.probe.name.as_str()))
            .cloned()
            .collect(),
        global_values: probes
            .global_values
            .iter()
            .filter(|row| names.contains(row.name.as_str()))
            .cloned()
            .collect(),
    }
}

fn case_demand_roots(
    case_id: &str,
    native: &NativeProductEvidence,
) -> Vec<ProjectLocalDemandRootForTest> {
    let groups = |name: &str| {
        let rows = native
            .groups
            .iter()
            .filter(|row| row.name == name && row.compilation_global)
            .cloned()
            .collect::<Vec<_>>();
        assert!(!rows.is_empty(), "at least one product group named {name}");
        rows.into_iter()
            .map(ProjectLocalDemandRootForTest::ProductGroup)
            .collect::<Vec<_>>()
    };
    match case_id {
        "reflect" => groups("Reflect"),
        "array" => [groups("ConcatArray"), groups("Array"), groups("Reflect")].concat(),
        "global-this" => vec![ProjectLocalDemandRootForTest::GlobalObject],
        "unavailable" => [
            "Wu6aTypeQueryUnavailable",
            "Wu6aInferredInitializerUnavailable",
            "Wu6aInferredReturnUnavailable",
            "Wu6aEnumUnavailable",
            "Wu6aImportUnavailable",
            "Wu6aDuplicateUnavailable",
            "Wu6aTdzUnavailable",
            "Wu6aClassStaticUnavailable",
        ]
        .into_iter()
        .map(|name| ProjectLocalDemandRootForTest::UserNamespace(name.to_owned()))
        .collect(),
        "mixed-forward"
        | "mixed-reverse"
        | "isolation-a"
        | "isolation-b"
        | "official-assignment-compat-between-tuple-and-array" => groups("Array"),
        _ => panic!("unknown WU4 case {case_id}"),
    }
}

fn assert_black_box_controls(actual: &[(&str, CompleteSourceBlackBoxOracleForTest)]) {
    let forward = oracle_case(actual, "mixed-forward");
    let reverse = oracle_case(actual, "mixed-reverse");
    assert_eq!(
        (
            &forward.global_contributors,
            &forward.force_all_semantic_probes,
            &forward.user_event_ledger,
            &forward.user_check_results
        ),
        (
            &reverse.global_contributors,
            &reverse.force_all_semantic_probes,
            &reverse.user_event_ledger,
            &reverse.user_check_results
        ),
        "source permutation changed normalized observables"
    );
    let a = semantic_probe_rows(&oracle_case(actual, "isolation-a").force_all_semantic_probes);
    let b = semantic_probe_rows(&oracle_case(actual, "isolation-b").force_all_semantic_probes);
    assert!(
        a.iter().any(|row| row.contains("projectAOnly"))
            && a.iter().all(|row| !row.contains("projectBOnly"))
    );
    assert!(
        b.iter().any(|row| row.contains("projectBOnly"))
            && b.iter().all(|row| !row.contains("projectAOnly"))
    );
    let official = oracle_case(actual, "official-assignment-compat-between-tuple-and-array");
    assert_eq!(
        official
            .user_check_results
            .iter()
            .map(|row| row.diagnostics.len())
            .sum::<usize>(),
        2
    );
    assert!(official
        .user_check_results
        .iter()
        .all(|row| row.incomplete.is_empty()));
    for name in ["B103GlobalThisValue", "B103GlobalThisCall"] {
        assert!(oracle_case(actual, "global-this")
            .global_contributors
            .iter()
            .any(|row| {
                row.name == name
                    && row
                        .sites
                        .iter()
                        .any(|site| site.origin == ProjectLocalContributorOriginForTest::User)
            }));
    }
}

fn valid_control_trace() -> (
    NativeProductEvidence,
    CompleteSourceBlackBoxOracleForTest,
    Vec<ProjectLocalDemandRootForTest>,
    ObservedCandidateTrace,
) {
    let first = product_group(0, "First");
    let second = product_group(1, "Second");
    let third = product_group(2, "Isolated");
    let materialized = vec![first.clone(), second.clone()];
    let groups = vec![first.clone(), second.clone(), third.clone()];
    let first_dependency = ProjectLocalProductDependencyForTest {
        owner: 0,
        target: 1,
        kind: "Reference".to_owned(),
        source_path: "lib.control.d.ts".to_owned(),
        source_start: 10,
    };
    let second_dependency = ProjectLocalProductDependencyForTest {
        owner: 1,
        target: 0,
        kind: "Reference".to_owned(),
        source_path: "lib.control.d.ts".to_owned(),
        source_start: 20,
    };
    let dependencies = vec![first_dependency, second_dependency];
    let sccs = vec![
        ProjectLocalProductSccForTest {
            members: materialized.clone(),
        },
        ProjectLocalProductSccForTest {
            members: vec![third],
        },
    ];
    let native = NativeProductEvidence {
        groups: groups.clone(),
        mapped_groups: groups
            .iter()
            .cloned()
            .enumerate()
            .map(
                |(identity, product)| ProjectLocalMappedProductGroupForTest {
                    product,
                    local_symbol: identity,
                    local_type_group: Some(identity),
                    local_value: None,
                    local_namespace: None,
                },
            )
            .collect(),
        dependencies: dependencies.clone(),
        publication_sccs: sccs.clone(),
    };
    let roots = vec![ProjectLocalDemandRootForTest::ProductGroup(first.clone())];
    let contributors = vec![ProjectLocalGlobalContributorForTest {
        name: "Control".to_owned(),
        has_value: true,
        has_type: false,
        has_namespace: false,
        sites: Vec::new(),
    }];
    let probes = empty_probes();
    let oracle = CompleteSourceBlackBoxOracleForTest {
        source_inputs: Vec::new(),
        source_visit_rows: Vec::new(),
        global_contributors: contributors.clone(),
        force_all_semantic_probes: probes.clone(),
        user_event_ledger: Vec::new(),
        user_check_results: Vec::new(),
        library_records: Vec::new(),
    };
    let commits = sccs[..1]
        .iter()
        .cloned()
        .map(|scc| ProjectLocalPublicationSccCommitForTest {
            reserved_members: scc.members.clone(),
            published_members: scc.members.clone(),
            observed_before_complete: Vec::new(),
            scc,
            state_before_publish: ProjectLocalTerminalStateForTest::Resolving,
            final_state: ProjectLocalTerminalStateForTest::Ready,
        })
        .collect();
    let trace = ProjectLocalSemanticTraceForTest {
        case_id: "control".to_owned(),
        project_session_identity: 1,
        publisher_session_identity: 2,
        source_inputs: Vec::new(),
        source_visit_rows: Vec::new(),
        admitted_product_groups: groups.clone(),
        mapped_product_groups: native.mapped_groups.clone(),
        product_dependencies: dependencies,
        product_publication_sccs: sccs,
        demand_root_visits: roots.clone(),
        materialized_product_groups: materialized.clone(),
        materialized_owner_work_count: materialized.len(),
        semantic_probes: probes,
        user_event_ledger: Vec::new(),
        user_check_results: Vec::new(),
        unavailable_audits: Vec::new(),
        scc_publications: Vec::new(),
        publication_scc_commits: commits,
        durable_cache_writes: materialized
            .iter()
            .cloned()
            .map(|owner| ProjectLocalCacheWriteForTest {
                owner,
                state: ProjectLocalTerminalStateForTest::Ready,
            })
            .collect(),
        final_global_contributors: contributors.clone(),
        published_global_contributors: contributors,
        repeated_demands: vec![ProjectLocalRepeatedDemandForTest {
            owner: first,
            first_identity: "type:1".to_owned(),
            repeated_identity: "type:1".to_owned(),
        }],
        ordinary_force_all_invocations: Vec::new(),
        force_all: None,
    };
    let observation = ObservationReceiptForTest {
        events: vec![
            observation_event(
                1,
                ObservationEventKindForTest::SessionCreated {
                    project: 1,
                    publisher: 2,
                },
            ),
            observation_event(
                2,
                ObservationEventKindForTest::OwnerMaterialized {
                    phase: ProjectLocalSemanticPhaseForTest::Ordinary,
                    product_group: 0,
                },
            ),
            observation_event(
                3,
                ObservationEventKindForTest::DurableCacheWrite {
                    publisher: 2,
                    product_group: 0,
                },
            ),
            observation_event(
                4,
                ObservationEventKindForTest::OwnerMaterialized {
                    phase: ProjectLocalSemanticPhaseForTest::Ordinary,
                    product_group: 1,
                },
            ),
            observation_event(
                5,
                ObservationEventKindForTest::DurableCacheWrite {
                    publisher: 2,
                    product_group: 1,
                },
            ),
            observation_event(
                6,
                ObservationEventKindForTest::DurableCacheHit {
                    publisher: 2,
                    product_group: 0,
                },
            ),
            observation_event(7, ObservationEventKindForTest::ReceiptBuilt),
            observation_event(
                8,
                ObservationEventKindForTest::SessionDropped { publisher: 2 },
            ),
        ],
        witness_kind: ObservationWitnessKindForTest::SyntheticSchema,
        session_weak: vec![WeakLivenessForTest {
            identity: 1,
            upgraded_at_finish: false,
        }],
        cache_weak: vec![WeakLivenessForTest {
            identity: 2,
            upgraded_at_finish: false,
        }],
    };
    (
        native,
        oracle,
        roots,
        ObservedCandidateTrace {
            trace,
            observation,
            fault: None,
        },
    )
}

fn assert_invalid(
    native: &NativeProductEvidence,
    oracle: &CompleteSourceBlackBoxOracleForTest,
    roots: &[ProjectLocalDemandRootForTest],
    trace: &ObservedCandidateTrace,
    expected: CandidateValidationError,
) {
    assert_eq!(
        validate_synthetic_candidate_trace(
            "control",
            native,
            oracle,
            roots,
            (
                ProjectLocalOrderForTest::Forward,
                ProjectLocalOrderForTest::Forward,
            ),
            false,
            trace.evidence(),
        ),
        Err(expected)
    );
}

fn product_group(product_key: usize, name: &str) -> ProjectLocalProductGroupForTest {
    ProjectLocalProductGroupForTest {
        product_key,
        container_key: 0,
        compilation_global: true,
        name: name.to_owned(),
        has_type: true,
        has_value: false,
        has_namespace: false,
    }
}

fn empty_probes() -> ProjectLocalSemanticProbesForTest {
    ProjectLocalSemanticProbesForTest {
        global_types: Vec::new(),
        module_types: Vec::new(),
        global_values: Vec::new(),
    }
}

fn candidate_request(
    case_id: &'static str,
    inputs: Vec<UserDeltaProjectInputForTest<'static>>,
    demand_roots: Vec<ProjectLocalDemandRootForTest>,
    force_all: bool,
    fault: Option<ProjectLocalSemanticFaultForTest>,
) -> CandidateHarnessRequest {
    CandidateHarnessRequest {
        inputs,
        controls: ProjectLocalSemanticRequestForTest {
            case_id,
            source_order: ProjectLocalOrderForTest::Forward,
            demand_order: ProjectLocalOrderForTest::Forward,
            demand_roots,
            force_all,
            fault,
        },
    }
}

const fn expected_fault_seam(
    fault: ProjectLocalSemanticFaultForTest,
) -> ProjectLocalFaultSeamForTest {
    match fault {
        ProjectLocalSemanticFaultForTest::OmitDependencyEdge => {
            ProjectLocalFaultSeamForTest::DependencyProjection
        }
        ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete => {
            ProjectLocalFaultSeamForTest::SccCompletion
        }
        ProjectLocalSemanticFaultForTest::WriteResolvingToDurableCache => {
            ProjectLocalFaultSeamForTest::DurableCacheCommit
        }
        ProjectLocalSemanticFaultForTest::PublishStaleGlobalObject => {
            ProjectLocalFaultSeamForTest::GlobalObjectPublication
        }
        ProjectLocalSemanticFaultForTest::InvokeForceAllFromOrdinaryDemand => {
            ProjectLocalFaultSeamForTest::OrdinaryDemandDispatch
        }
        ProjectLocalSemanticFaultForTest::PublishProvisionalUnavailableWrite => {
            ProjectLocalFaultSeamForTest::UnavailableRollback
        }
    }
}

const fn fault_scenarios() -> [(ProjectLocalSemanticFaultForTest, &'static str); 6] {
    [
        (
            ProjectLocalSemanticFaultForTest::OmitDependencyEdge,
            "array",
        ),
        (
            ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete,
            "array",
        ),
        (
            ProjectLocalSemanticFaultForTest::WriteResolvingToDurableCache,
            "array",
        ),
        (
            ProjectLocalSemanticFaultForTest::PublishStaleGlobalObject,
            "global-this",
        ),
        (
            ProjectLocalSemanticFaultForTest::InvokeForceAllFromOrdinaryDemand,
            "array",
        ),
        (
            ProjectLocalSemanticFaultForTest::PublishProvisionalUnavailableWrite,
            "unavailable",
        ),
    ]
}

const fn expected_fault_error(fault: ProjectLocalSemanticFaultForTest) -> CandidateValidationError {
    match fault {
        ProjectLocalSemanticFaultForTest::OmitDependencyEdge => {
            CandidateValidationError::ProductAuthority
        }
        ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete => {
            CandidateValidationError::PrematureSccPublication
        }
        ProjectLocalSemanticFaultForTest::WriteResolvingToDurableCache => {
            CandidateValidationError::ResolvingDurableCache
        }
        ProjectLocalSemanticFaultForTest::PublishStaleGlobalObject => {
            CandidateValidationError::StaleGlobalObject
        }
        ProjectLocalSemanticFaultForTest::InvokeForceAllFromOrdinaryDemand => {
            CandidateValidationError::OrdinaryForceAll
        }
        ProjectLocalSemanticFaultForTest::PublishProvisionalUnavailableWrite => {
            CandidateValidationError::ProvisionalUnavailablePublished
        }
    }
}

fn candidate_trace(
    provider: &LibraryBaseProvider,
    request: CandidateHarnessRequest,
) -> ObservedCandidateTrace {
    let CandidateHarnessRequest { inputs, controls } = request;
    let fault = controls.fault;
    let provider_before = provider.measurement_for_test();
    let route_scope = CompleteSourceRouteWorkScopeForTest::start();
    let compiler_scope = LibraryCompilerWorkScopeForTest::start();
    let observation_scope = ObservationScopeForTest::start();
    let product = provider
        .declaration_product()
        .expect("candidate immutable product");
    let product_for_candidate = std::sync::Arc::clone(&product);
    let files = inputs
        .iter()
        .map(|input| crate::frontend::FileInput {
            name: input.path.to_owned(),
            source: input.source.to_owned(),
        })
        .collect::<Vec<_>>();
    let run = crate::frontend::run_prepared_project_frontend(
        files,
        move |programs, source_reexports, default_modules| {
            let (library_units, bound) =
                crate::check::checker::bind_library_product_project_programs(
                    &product_for_candidate,
                    programs,
                    source_reexports,
                    default_modules,
                )?;
            Ok::<_, crate::check::checker::LibraryProductProjectBindingError>(
                project_local_semantic_candidate_for_test(
                    &product_for_candidate,
                    library_units,
                    bound,
                    programs,
                    controls,
                ),
            )
        },
    );
    let result = run
        .product
        .expect("candidate prepared programs")
        .expect("candidate project-local binder");
    let observation = observation_scope.finish();
    let compiler_work = compiler_scope.finish();
    let route_work = route_scope.finish();
    assert_eq!(
        compiler_work,
        crate::compiler::LibraryCompilerWorkForTest::default(),
        "candidate route entered LibraryCompiler"
    );
    assert_eq!(
        route_work,
        crate::check::checker::library_compiler::CompleteSourceRouteWorkForTest::default(),
        "candidate route entered complete-source/frozen/replay work"
    );
    assert_eq!(
        provider.measurement_for_test(),
        provider_before,
        "candidate route initialized or published the frozen provider"
    );
    ObservedCandidateTrace {
        trace: result.expect("WU4 candidate trace"),
        observation,
        fault,
    }
}

fn complete_source_oracles(
    provider: &LibraryBaseProvider,
) -> Vec<(&'static str, CompleteSourceBlackBoxOracleForTest)> {
    oracle_inputs()
        .into_iter()
        .map(|(id, inputs)| {
            let oracle = provider
                .complete_source_black_box_oracle_for_test(&inputs)
                .unwrap_or_else(|error| panic!("complete-source {id}: {error}"));
            (id, oracle)
        })
        .collect()
}

fn oracle_case<'a>(
    oracles: &'a [(&str, CompleteSourceBlackBoxOracleForTest)],
    id: &str,
) -> &'a CompleteSourceBlackBoxOracleForTest {
    &oracles
        .iter()
        .find(|(actual, _)| *actual == id)
        .unwrap_or_else(|| panic!("missing WU4 case {id}"))
        .1
}

fn native_evidence_value(native: &NativeProductEvidence) -> Value {
    json!({
        "group_count": native.groups.len(), "group_digest": rows_digest(&product_group_rows(&native.groups)),
        "dependency_count": native.dependencies.len(), "dependency_digest": rows_digest(&product_dependency_rows(&native.dependencies)),
        "publication_scc_count": native.publication_sccs.len(), "publication_scc_digest": rows_digest(&product_scc_rows(&native.publication_sccs)),
    })
}

fn library_records_value(records: &[ProjectLocalLibraryRecordForTest]) -> Value {
    json!({
        "count": records.len(),
        "diagnostics": records.iter().filter(|row| matches!(row.record, crate::check::checker::library_compiler::NormalizedCheckerRecordForTest::Diagnostic(_))).count(),
        "incompletes": records.iter().filter(|row| matches!(row.record, crate::check::checker::library_compiler::NormalizedCheckerRecordForTest::Incomplete(_))).count(),
        "digest": rows_digest(&library_record_rows(records)),
    })
}

fn black_box_case_value(id: &str, oracle: &CompleteSourceBlackBoxOracleForTest) -> Value {
    let probes = semantic_probe_rows(&oracle.force_all_semantic_probes);
    json!({
        "id": id,
        "source_input_count": oracle.source_inputs.len(), "source_input_digest": rows_digest(&source_input_rows(oracle)),
        "source_visit_digest": rows_digest(&oracle.source_visit_rows),
        "global_contributor_count": oracle.global_contributors.len(), "global_contributor_digest": rows_digest(&global_contributor_rows(oracle)),
        "semantic_probe_count": probes.len(), "semantic_probe_digest": rows_digest(&probes),
        "user_event_record_count": oracle.user_event_ledger.len(), "user_event_digest": rows_digest(&user_event_rows(oracle)),
        "user_result_count": oracle.user_check_results.len(), "user_result_digest": rows_digest(&user_result_rows(oracle)),
    })
}

fn product_group_rows(rows: &[ProjectLocalProductGroupForTest]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{}|{}|{}|{:?}|{}|{}|{}",
                row.product_key,
                row.container_key,
                row.compilation_global,
                row.name,
                row.has_type,
                row.has_value,
                row.has_namespace
            )
        })
        .collect()
}
fn product_dependency_rows(rows: &[ProjectLocalProductDependencyForTest]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{}|{}|{}|{:?}|{}",
                row.owner, row.target, row.kind, row.source_path, row.source_start
            )
        })
        .collect()
}
fn product_scc_rows(rows: &[ProjectLocalProductSccForTest]) -> Vec<String> {
    rows.iter()
        .map(|row| encode_rows(&product_group_rows(&row.members)))
        .collect()
}
fn source_input_rows(oracle: &CompleteSourceBlackBoxOracleForTest) -> Vec<String> {
    oracle
        .source_inputs
        .iter()
        .map(|row| format!("{:?}|{}|{}", row.path, row.byte_len, row.sha256))
        .collect()
}
fn library_record_rows(rows: &[ProjectLocalLibraryRecordForTest]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{:?}|{}|{}",
                row.source_path,
                row.owner_start,
                checker_record_row(&row.record)
            )
        })
        .collect()
}
fn user_event_rows(oracle: &CompleteSourceBlackBoxOracleForTest) -> Vec<String> {
    oracle
        .user_event_ledger
        .iter()
        .map(|row| {
            format!(
                "{:?}|{}|{}|{}|{}",
                row.source_path,
                row.source_start,
                row.event_ordinal,
                row.record_ordinal,
                checker_record_row(&row.record)
            )
        })
        .collect()
}
fn user_result_rows(oracle: &CompleteSourceBlackBoxOracleForTest) -> Vec<String> {
    oracle
        .user_check_results
        .iter()
        .map(|row| {
            let diagnostics = row
                .diagnostics
                .iter()
                .map(diagnostic_row)
                .collect::<Vec<_>>();
            let incomplete = row
                .incomplete
                .iter()
                .map(incomplete_row)
                .collect::<Vec<_>>();
            format!(
                "{:?}|{}|{}",
                row.source_path,
                encode_rows(&diagnostics),
                encode_rows(&incomplete)
            )
        })
        .collect()
}
fn checker_record_row(
    row: &crate::check::checker::library_compiler::NormalizedCheckerRecordForTest,
) -> String {
    match row {
        crate::check::checker::library_compiler::NormalizedCheckerRecordForTest::Diagnostic(
            row,
        ) => format!("diagnostic:{}", diagnostic_row(row)),
        crate::check::checker::library_compiler::NormalizedCheckerRecordForTest::Incomplete(
            row,
        ) => format!("incomplete:{}", incomplete_row(row)),
    }
}
fn diagnostic_row(
    row: &crate::check::checker::library_compiler::NormalizedDiagnosticForTest,
) -> String {
    format!(
        "{}-{}|{:?}|{:?}|{:?}|{}",
        row.span.start,
        row.span.end,
        row.code,
        row.severity,
        row.message,
        encode_rows(&row.elaboration)
    )
}
fn incomplete_row(
    row: &crate::check::checker::library_compiler::NormalizedIncompleteForTest,
) -> String {
    format!(
        "{}-{}|{:?}|{:?}",
        row.span.start, row.span.end, row.id, row.context
    )
}
fn semantic_probe_rows(probes: &ProjectLocalSemanticProbesForTest) -> Vec<String> {
    let mut rows = Vec::new();
    rows.extend(probes.global_types.iter().map(|row| {
        format!(
            "global-type={:?}|{}|{}",
            row.name,
            row.declaration_count,
            encode_rows(&row.member_names)
        )
    }));
    rows.extend(probes.module_types.iter().map(|row| {
        format!(
            "module-type={:?}|{:?}|{}|{}",
            row.source_path,
            row.probe.name,
            row.probe.declaration_count,
            encode_rows(&row.probe.member_names)
        )
    }));
    rows.extend(probes.global_values.iter().map(|row| {
        let signatures = row.signatures.iter().map(signature_row).collect::<Vec<_>>();
        let members = row
            .callable_members
            .iter()
            .map(|member| {
                let signatures = member
                    .signatures
                    .iter()
                    .map(signature_row)
                    .collect::<Vec<_>>();
                format!(
                    "{:?}|{:?}|{:?}|{}|{}|{}",
                    member.name,
                    member.source_path,
                    member.reservation_source_path,
                    member.source_start,
                    member.call_signature_count,
                    encode_rows(&signatures)
                )
            })
            .collect::<Vec<_>>();
        format!(
            "global-value={:?}|{}|{}|{}|{}|{}",
            row.name,
            row.declaration_count,
            row.call_signature_count,
            encode_rows(&signatures),
            encode_rows(&row.member_names),
            encode_rows(&members)
        )
    }));
    rows.sort();
    rows
}
fn signature_row(row: &crate::check::checker::library_compiler::SignatureProbe) -> String {
    format!(
        "{}|{:?}",
        encode_rows(&row.parameter_types),
        row.return_type
    )
}
fn global_contributor_rows(oracle: &CompleteSourceBlackBoxOracleForTest) -> Vec<String> {
    oracle
        .global_contributors
        .iter()
        .map(|row| {
            let sites = row
                .sites
                .iter()
                .map(|site| {
                    format!(
                        "{:?}|{:?}|{}-{}|{:?}",
                        site.source_path, site.origin, site.span_start, site.span_end, site.kind
                    )
                })
                .collect::<Vec<_>>();
            format!(
                "{:?}|{}|{}|{}|{}",
                row.name,
                row.has_value,
                row.has_type,
                row.has_namespace,
                encode_rows(&sites)
            )
        })
        .collect()
}
fn rows_digest(rows: &[String]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"typokat/wu4/candidate-native-rows/v1");
    for row in rows {
        digest.update(u64::try_from(row.len()).unwrap_or(u64::MAX).to_be_bytes());
        digest.update(row.as_bytes());
    }
    format!("{:x}", digest.finalize())
}
fn encode_rows(rows: &[String]) -> String {
    rows.iter()
        .map(|row| format!("{}:{row}", row.len()))
        .collect::<Vec<_>>()
        .join("|")
}
fn ordered<T: Clone>(rows: &[T], order: ProjectLocalOrderForTest) -> Vec<T> {
    let mut rows = rows.to_vec();
    if order == ProjectLocalOrderForTest::Reverse {
        rows.reverse();
    }
    rows
}
fn frozen_oracle() -> Value {
    serde_json::from_slice(ORACLE_BYTES).expect("valid WU4 oracle")
}
fn case_inputs(id: &str) -> Vec<UserDeltaProjectInputForTest<'static>> {
    oracle_inputs()
        .into_iter()
        .find(|(actual, _)| *actual == id)
        .unwrap_or_else(|| panic!("missing WU4 case {id}"))
        .1
}

fn oracle_inputs() -> Vec<(&'static str, Vec<UserDeltaProjectInputForTest<'static>>)> {
    vec![
        ("reflect", vec![input("/candidate/reflect_packaged.ts", include_str!("../../../tests/cases/sr_project_local_library_candidate/reflect_packaged.ts"))]),
        ("array", vec![input("/candidate/array_library_scc.ts", include_str!("../../../tests/cases/sr_project_local_library_candidate/array_library_scc.ts"))]),
        ("global-this", vec![
            input("/candidate/global-this/00_global.ts", include_str!("../../../tests/cases/b103_library_merge_correctness_project/global_this_contributor/00_global.ts")),
            input("/candidate/global-this/99_consume.ts", include_str!("../../../tests/cases/b103_library_merge_correctness_project/global_this_contributor/99_consume.ts")),
        ]),
        ("unavailable", vec![input("/candidate/unavailable.ts", include_str!("../../../tests/cases/b43_namespaces_declaration_merging/wu6a_unavailable_ledger.ts"))]),
        ("mixed-forward", mixed_inputs(false)), ("mixed-reverse", mixed_inputs(true)),
        ("isolation-a", vec![
            input("/candidate/isolation/00_augment.ts", include_str!("../../../tests/cases/sr_project_local_library_candidate/isolation_a/00_augment.ts")),
            input("/candidate/isolation/99_consume.ts", include_str!("../../../tests/cases/sr_project_local_library_candidate/isolation_a/99_consume.ts")),
        ]),
        ("isolation-b", vec![
            input("/candidate/isolation/00_augment.ts", include_str!("../../../tests/cases/sr_project_local_library_candidate/isolation_b/00_augment.ts")),
            input("/candidate/isolation/99_consume.ts", include_str!("../../../tests/cases/sr_project_local_library_candidate/isolation_b/99_consume.ts")),
        ]),
        ("official-assignment-compat-between-tuple-and-array", vec![input(
            "/candidate/official/types/typeRelationships/assignmentCompatibility/assignmentCompatBetweenTupleAndArray.ts",
            include_str!("../../../tests/cases/sr_project_local_library_candidate/official_assignment_compat_between_tuple_and_array.ts"),
        )]),
    ]
}
fn mixed_inputs(reverse: bool) -> Vec<UserDeltaProjectInputForTest<'static>> {
    let script = input("/candidate/mixed/00_script.ts", include_str!("../../../tests/cases/sr_project_local_library_candidate/mixed_group_forward/00_script.ts"));
    let module = input("/candidate/mixed/50_module.ts", include_str!("../../../tests/cases/sr_project_local_library_candidate/mixed_group_forward/50_module.ts"));
    let consume = input("/candidate/mixed/99_consume.ts", include_str!("../../../tests/cases/sr_project_local_library_candidate/mixed_group_forward/99_consume.ts"));
    if reverse {
        vec![consume, module, script]
    } else {
        vec![script, module, consume]
    }
}
const fn input(path: &'static str, source: &'static str) -> UserDeltaProjectInputForTest<'static> {
    UserDeltaProjectInputForTest { path, source }
}
