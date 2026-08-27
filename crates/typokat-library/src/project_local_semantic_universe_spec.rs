//! WU4 contract for project-local demand-driven default-library semantics.

use crate::base::UserDeltaProjectInputForTest;
use crate::check::checker::library_compiler::CompleteSourceRouteWorkScopeForTest;
use crate::check::checker::project_local::{
    project_local_semantic_candidate_for_test, ObservationEventForTest,
    ObservationEventKindForTest, ObservationReceiptForTest, ObservationScopeForTest,
    ObservationWitnessKindForTest, ProjectLocalCacheWriteForTest,
    ProjectLocalCandidateErrorForTest, ProjectLocalContributorKindForTest,
    ProjectLocalContributorOriginForTest, ProjectLocalDemandRootForTest,
    ProjectLocalFaultSeamForTest, ProjectLocalForceAllTraceForTest,
    ProjectLocalGlobalContributorForTest, ProjectLocalGlobalContributorSiteForTest,
    ProjectLocalLibraryRecordForTest, ProjectLocalMappedProductGroupForTest,
    ProjectLocalModuleTypeProbeForTest, ProjectLocalNamespaceFragmentForTest,
    ProjectLocalOrderForTest, ProjectLocalProductDependencyForTest,
    ProjectLocalProductGroupForTest, ProjectLocalProductSccForTest,
    ProjectLocalPublicationSccCommitForTest, ProjectLocalRepeatedDemandForTest,
    ProjectLocalSemanticFaultForTest, ProjectLocalSemanticPhaseForTest,
    ProjectLocalSemanticProbesForTest, ProjectLocalSemanticRequestForTest,
    ProjectLocalSemanticTraceForTest, ProjectLocalTerminalStateForTest,
    ProjectLocalUnavailableAuditForTest, ProjectLocalUnavailableReasonForTest,
    ProjectLocalUnavailableTerminalForTest, ProjectLocalValueProbeForTest, WeakLivenessForTest,
};
use crate::compiler::LibraryCompilerWorkScopeForTest;
use crate::provider::{CompleteSourceBlackBoxOracleForTest, LibraryBaseProvider};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;
use typokat_library_product::{
    ContainerKind, ContainerRecipe, DeclarationFlags, DeclarationGroupRecipe,
    DeclarationLexicalScope, DeclarationPayload, DeclarationRecipe, DeclarationSlots,
    DependencyKind, DependencyLinkRecipe, DependencyRecipe, DependencyTarget,
    LibraryDeclarationProduct, ProductAssembly, RegionKind, RegionRecipe, SignatureRecipe,
    SourceClassification, SourceRecipe, SourceSite, TupleElementRecipe, TypeRecipe, TypeRecipeNode,
    TypeReferenceResolution,
};

const ORACLE_BYTES: &[u8] = include_bytes!(
    "../../../tests/cases/sr_project_local_library_candidate/semantic-demand-oracle.json"
);
const ORACLE_SHA256: &str = "7c9cb807e2ade41b5eb550383ac805bf81bdd4352d9b6f9031bdd0c973c8da30";
const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const TYPESCRIPT_REVISION: &str = "050880ce59e30b356b686bd3144efe24f875ebc8";
const OFFICIAL_FRAME_PATH: &str = "conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatBetweenTupleAndArray.ts";
const OFFICIAL_FRAME_SHA256: &str =
    "8ea2fffa0e48d8727722bb2e52ba98a235d72dbe7ef9848ac355f8c5d82f6a68";

const IDB_CURSOR_CONSUMPTION: &str = r#"
declare const wu4Cursor: IDBCursor;
const wu4CursorKey: IDBValidKey = wu4Cursor.key;
wu4Cursor.continue(wu4CursorKey);
const wu4WrongCursorKey: string = wu4Cursor.key;
const wu4CursorConstructor: IDBCursor = new IDBCursor();
"#;

const AUGMENTED_DEFAULTED_GENERIC: &str = r#"
interface ProgressEvent<T extends EventTarget = EventTarget> {
    wu4DefaultedTarget(): T;
}
declare const wu4Progress: ProgressEvent;
const wu4Target: EventTarget = wu4Progress.wu4DefaultedTarget();
const wu4WrongTarget: string = wu4Progress.wu4DefaultedTarget();
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeProductEvidence {
    groups: Vec<ProjectLocalProductGroupForTest>,
    mapped_groups: Vec<ProjectLocalMappedProductGroupForTest>,
    module_probe_sources: BTreeMap<usize, String>,
    probe_evidence: Vec<StableProbeEvidence>,
    dependencies: Vec<ProjectLocalProductDependencyForTest>,
    containment_targets: BTreeMap<usize, Vec<usize>>,
    publication_sccs: Vec<ProjectLocalProductSccForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StableProbeCoordinate {
    GlobalType { name: String },
    GlobalValue { name: String },
    ModuleType { source_path: String, name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StableProbeSlot {
    Type(usize),
    Value(usize),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableProbeEvidence {
    product_key: usize,
    mapped_slot: StableProbeSlot,
    coordinate: StableProbeCoordinate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublishBeforeSccCompleteContract {
    canonical_roots: Vec<ProjectLocalDemandRootForTest>,
    selected_owner: ProjectLocalProductGroupForTest,
    selected_scc: ProjectLocalProductSccForTest,
    clean_scc: ProjectLocalProductSccForTest,
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

const CANDIDATE_SOURCE_PATHS: [&str; 12] = [
    "cache.rs",
    "contract.rs",
    "coordinator.rs",
    "entry.rs",
    "fault_gate.rs",
    "global_owner.rs",
    "lowerer.rs",
    "mod.rs",
    "observation.rs",
    "project_local_pass_port.rs",
    "session.rs",
    "terminal.rs",
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
    if matches!(
        import.path.as_str(),
        "std::fmt"
            | "std::cell::Cell"
            | "std::cell::RefCell"
            | "std::collections::BTreeMap"
            | "std::collections::BTreeSet"
            | "std::rc::Rc"
            | "std::rc::Weak"
            | "std::sync::atomic::AtomicU64"
            | "std::sync::atomic::Ordering"
    ) {
        return true;
    }
    if relative != "project_local_pass_port.rs" {
        return false;
    }
    candidate_adapter_external_path_allowed(&import.path)
}

fn candidate_adapter_external_path_allowed(path: &str) -> bool {
    matches!(
        path,
        "crate::binder::bind::LibraryBinderUnit"
            | "crate::binder::declaration::SourceGlobalContributorKind"
            | "crate::binder::declaration::TypeGroupId"
            | "crate::binder::declaration::ValueStorageId"
            | "crate::binder::declaration::source_global_binding_census_with_provenance"
            | "crate::binder::declaration::SourceBindingSlot"
            | "crate::binder::namespace::NamespaceId"
            | "crate::binder::roots::collect_final_global_object_contributors"
            | "crate::binder::scope::ScopeId"
            | "crate::binder::symbol::SymbolId"
            | "crate::check::checker::annotations::project_local_well_known_symbol_from_name"
            | "crate::check::checker::context::ProjectLocalDemandSnapshot"
            | "crate::check::checker::context::ProjectLocalTypeOverlayDraft"
            | "crate::check::checker::context::ProjectLocalUserDemandGateway"
            | "crate::check::checker::eval::contains_deferred_argument"
            | "crate::check::checker::eval::contains_deferred_keyof"
            | "crate::check::checker::eval::keyof_of_type"
            | "crate::check::checker::library_identities::LibrarySemanticIdentities"
            | "crate::check::checker::namespace_values::ProjectLocalNamespaceSeed"
            | "crate::check::checker::reporting_record::CheckerRecord"
            | "crate::check::checker::type_groups::PublishedTypeParameterDefault"
            | "crate::check::checker::type_groups::PublishedTypeEnvironment"
            | "crate::check::checker::type_groups::PublishedTypeGroup"
            | "crate::check::checker::type_groups::PublishedTypeGroupSurface"
            | "crate::check::checker::type_groups::PublishedTypeGroupTerminal"
            | "crate::check::checker::type_groups::PublishedTypeGroupUnavailable"
            | "crate::check::checker::type_groups::TypeGroupUnavailableCause"
            | "crate::check::checker::check_bound_project_local_programs"
            | "crate::check::checker::BoundProjectBinder"
            | "crate::check::checker::ProjectLocalUserPassPlan"
            | "crate::check::checker::ProjectLocalUserNamespaceUnavailableReason::Unsupported"
            | "crate::check::checker::ProjectLocalUserNamespaceUnavailableReason::Invalid"
            | "crate::check::checker::ProjectLocalUserNamespaceUnavailableReason::Cyclic"
            | "crate::check::query::SemanticQueryCoordinator"
            | "crate::check::query::SemanticQueryState"
            | "crate::class_semantics::DemandOutcome"
            | "crate::class_semantics::PublishedClasses"
            | "crate::diagnostics::render_reason_chain"
            | "crate::diagnostics::render_type"
            | "crate::diagnostics::Diagnostic"
            | "crate::diagnostics::IncompleteSurface"
            | "crate::diagnostics::Severity"
            | "crate::frontend::ProjectProgram"
            | "crate::relate::relation::RelationOutcome"
            | "crate::source::LibraryFileOrdinal"
            | "crate::source::ModuleOrdinal"
            | "crate::source::UnitSlot"
            | "crate::source::CompilationOrigin"
            | "crate::types::repr::ConditionalType"
            | "crate::types::repr::AliasId"
            | "crate::types::repr::FunctionType"
            | "crate::types::repr::MappedType"
            | "crate::types::repr::ModifierOp"
            | "crate::types::repr::ObjectType"
            | "crate::types::repr::ParameterType"
            | "crate::types::repr::PropertyType"
            | "crate::types::repr::TemplateType"
            | "crate::types::repr::TupleRestType"
            | "crate::types::repr::TupleType"
            | "crate::types::repr::TypeTag"
            | "crate::types::repr::WellKnownSymbol"
            | "crate::types::repr::number_to_string"
            | "crate::types::substitute"
            | "crate::types::GenericTypeParam"
            | "crate::types::Interner"
            | "crate::types::LiteralValue"
            | "crate::types::PropertyKey"
            | "crate::types::TypeId"
            | "crate::types::TypeParamId"
            | "crate::types::Visibility"
            | "rustc_hash::FxHashMap"
            | "typokat_library_product::Accessibility"
            | "typokat_library_product::ContainerKind"
            | "typokat_library_product::DeclarationGroupRecipe"
            | "typokat_library_product::DeclarationKey"
            | "typokat_library_product::DeclarationPayload"
            | "typokat_library_product::DependencyKind"
            | "typokat_library_product::DependencyTarget"
            | "typokat_library_product::IntrinsicMarker"
            | "typokat_library_product::KeywordKind"
            | "typokat_library_product::LexicalStepKind"
            | "typokat_library_product::LibraryDeclarationProduct"
            | "typokat_library_product::LiteralRecipe"
            | "typokat_library_product::MappedModifier"
            | "typokat_library_product::MemberKey"
            | "typokat_library_product::MemberKind"
            | "typokat_library_product::MemberRecipe"
            | "typokat_library_product::ParameterPattern"
            | "typokat_library_product::PropertyNameRecipe"
            | "typokat_library_product::QualifiedPathRecipe"
            | "typokat_library_product::QualifiedPathResolutionOutcome"
            | "typokat_library_product::QualifiedPathRole"
            | "typokat_library_product::QualifiedPathUnresolvedCause"
            | "typokat_library_product::RegionKey"
            | "typokat_library_product::SignatureKey"
            | "typokat_library_product::SourceKey"
            | "typokat_library_product::SourceSite"
            | "typokat_library_product::SymbolicValueNameRecipe"
            | "typokat_library_product::TypeOperator"
            | "typokat_library_product::TypeParameterKey"
            | "typokat_library_product::TypeRecipeKey"
            | "typokat_library_product::TypeRecipeNode"
            | "typokat_library_product::TypeReferenceResolution"
            | "typokat_library_product::ValueReferenceResolution"
            | "typokat_library_product::sha256_hex"
            | "typokat_types::alias_semantics::PublishedAliasSurface"
            | "typokat_types::alias_semantics::PublishedAliasTerminal"
            | "typokat_types::alias_semantics::PublishedAliasUnavailable"
            | "typokat_types::alias_semantics::PublishedAliases"
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
            "Box",
            "Default",
            "Err",
            "IntoIterator",
            "None",
            "Ok",
            "Option",
            "Result",
            "Self",
            "Some",
            "String",
            "Vec",
            "f64",
            "u32",
            "usize",
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
                "cache"
                    | "contract"
                    | "coordinator"
                    | "entry"
                    | "fault_gate"
                    | "global_owner"
                    | "lowerer"
                    | "observation"
                    | "project_local_pass_port"
                    | "session"
                    | "terminal"
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
        if segments
            .last()
            .is_some_and(|segment| segment.starts_with("record_"))
            && !matches!(
                self.relative,
                "session.rs" | "terminal.rs" | "cache.rs" | "fault_gate.rs" | "global_owner.rs"
            )
        {
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
                    || text == "std::cell::Cell"
                    || text.starts_with("std::cell::Cell::")
                    || text == "std::cell::RefCell"
                    || text.starts_with("std::cell::RefCell::")
                    || text == "std::collections::BTreeMap"
                    || text.starts_with("std::collections::BTreeMap::")
                    || text == "std::collections::BTreeSet"
                    || text.starts_with("std::collections::BTreeSet::")
                    || text == "std::rc::Rc"
                    || text.starts_with("std::rc::Rc::")
                    || text == "std::rc::Weak"
                    || text.starts_with("std::rc::Weak::")
                    || text == "std::sync::atomic::AtomicU64"
                    || text.starts_with("std::sync::atomic::AtomicU64::")
                    || text == "std::sync::atomic::Ordering"
                    || text.starts_with("std::sync::atomic::Ordering::")
                    || text == "std::mem::take"
            }
            "core" => false,
            "crate" => {
                self.relative == "project_local_pass_port.rs"
                    && candidate_adapter_external_path_allowed(&text)
            }
            "typokat_library_product" => {
                self.relative == "project_local_pass_port.rs"
                    && candidate_adapter_external_path_allowed(&text)
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
        if matches!(
            name.as_str(),
            "debug_assert" | "debug_assert_eq" | "format" | "matches" | "unreachable" | "vec"
        ) {
            return;
        }
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
    FaultTargetContract,
    FaultCommitContract,
    FaultActivationOrder,
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
    let exact_module_sources = product
        .groups()
        .iter()
        .enumerate()
        .filter_map(|(product_key, group)| {
            let container = &product.containers()[group.container.index()];
            if container.kind != typokat_library_product::ContainerKind::ExternalModule
                || container.parent.is_some()
            {
                return None;
            }
            let source = &product.sources()[container.anchor?.source.index()];
            Some((product_key, product.strings()[source.name.index()].clone()))
        })
        .collect::<BTreeMap<_, _>>();
    assert!(!exact_module_sources.is_empty());
    assert_eq!(evidence.module_probe_sources, exact_module_sources);
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
fn demanded_probe_projection_uses_mapped_slot_and_visibility_coordinates() {
    let group = |product_key, container_key, compilation_global, name: &str, has_value| {
        ProjectLocalProductGroupForTest {
            product_key,
            container_key,
            compilation_global,
            name: name.to_owned(),
            has_type: true,
            has_value,
            has_namespace: false,
        }
    };
    let mapped =
        |product: &ProjectLocalProductGroupForTest, local_symbol, local_type_group, local_value| {
            ProjectLocalMappedProductGroupForTest {
                product: product.clone(),
                local_symbol,
                local_type_group: Some(local_type_group),
                local_value,
                local_namespace: None,
            }
        };
    let evidence = |product_key, mapped_slot, coordinate| StableProbeEvidence {
        product_key,
        mapped_slot,
        coordinate,
    };
    let global_homonym = group(0, 0, true, "Homonym", true);
    let module_root = group(1, 1, false, "ModuleRoot", false);
    let module_homonym = group(2, 1, false, "Homonym", false);
    let user_homonym_module = group(3, 2, false, "UserOnly", false);
    let groups = vec![
        global_homonym.clone(),
        module_root.clone(),
        module_homonym.clone(),
        user_homonym_module.clone(),
    ];
    let mapped_groups = vec![
        mapped(&global_homonym, 41, 73, Some(89)),
        mapped(&module_root, 43, 79, None),
        mapped(&module_homonym, 47, 83, None),
        mapped(&user_homonym_module, 53, 97, None),
    ];
    let module_path = "/lib/module-a.d.ts";
    let user_module_path = "/lib/module-user.d.ts";
    let probe_evidence = vec![
        evidence(
            0,
            StableProbeSlot::Type(73),
            StableProbeCoordinate::GlobalType {
                name: "Homonym".to_owned(),
            },
        ),
        evidence(
            0,
            StableProbeSlot::Value(89),
            StableProbeCoordinate::GlobalValue {
                name: "Homonym".to_owned(),
            },
        ),
        evidence(
            1,
            StableProbeSlot::Type(79),
            StableProbeCoordinate::ModuleType {
                source_path: module_path.to_owned(),
                name: "ModuleRoot".to_owned(),
            },
        ),
        evidence(
            2,
            StableProbeSlot::Type(83),
            StableProbeCoordinate::ModuleType {
                source_path: module_path.to_owned(),
                name: "Homonym".to_owned(),
            },
        ),
        evidence(
            3,
            StableProbeSlot::Type(97),
            StableProbeCoordinate::ModuleType {
                source_path: user_module_path.to_owned(),
                name: "UserOnly".to_owned(),
            },
        ),
    ];
    let native = NativeProductEvidence {
        groups: groups.clone(),
        mapped_groups,
        module_probe_sources: [
            (1, module_path.to_owned()),
            (2, module_path.to_owned()),
            (3, user_module_path.to_owned()),
        ]
        .into_iter()
        .collect(),
        probe_evidence,
        dependencies: vec![ProjectLocalProductDependencyForTest {
            owner: 1,
            target: 2,
            kind: "Reference".to_owned(),
            source_path: module_path.to_owned(),
            source_start: 10,
        }],
        containment_targets: BTreeMap::new(),
        publication_sccs: groups
            .iter()
            .cloned()
            .map(|member| ProjectLocalProductSccForTest {
                members: vec![member],
            })
            .collect(),
    };
    assert!(stable_probe_evidence_matches_mapped_slots(&native));
    let mut missing_evidence = native.clone();
    missing_evidence.probe_evidence.remove(0);
    assert!(!stable_probe_evidence_matches_mapped_slots(
        &missing_evidence
    ));
    let mut duplicate_evidence = native.clone();
    duplicate_evidence
        .probe_evidence
        .push(duplicate_evidence.probe_evidence[0].clone());
    assert!(!stable_probe_evidence_matches_mapped_slots(
        &duplicate_evidence
    ));
    let mut duplicate_coordinate = native.clone();
    let homonym_coordinate = duplicate_coordinate
        .probe_evidence
        .iter()
        .find_map(|row| match &row.coordinate {
            StableProbeCoordinate::ModuleType { name, .. } if name == "Homonym" => {
                Some(row.coordinate.clone())
            }
            _ => None,
        })
        .expect("module homonym coordinate");
    duplicate_coordinate
        .probe_evidence
        .iter_mut()
        .find(|row| {
            matches!(
                &row.coordinate,
                StableProbeCoordinate::ModuleType { name, .. } if name == "ModuleRoot"
            )
        })
        .expect("module root coordinate")
        .coordinate = homonym_coordinate;
    assert!(!stable_probe_evidence_matches_mapped_slots(
        &duplicate_coordinate
    ));
    let mut wrong_module_source = native.clone();
    let module = wrong_module_source
        .probe_evidence
        .iter_mut()
        .find(|row| matches!(row.coordinate, StableProbeCoordinate::ModuleType { .. }))
        .expect("module evidence");
    let StableProbeCoordinate::ModuleType { source_path, .. } = &mut module.coordinate else {
        unreachable!("selected module evidence")
    };
    *source_path = "/wrong/module.d.ts".to_owned();
    assert!(!stable_probe_evidence_matches_mapped_slots(
        &wrong_module_source
    ));
    let mut out_of_order = native.clone();
    out_of_order.probe_evidence.swap(0, 1);
    assert!(!stable_probe_evidence_matches_mapped_slots(&out_of_order));
    let probes = ProjectLocalSemanticProbesForTest {
        global_types: vec![type_probe("Homonym"), type_probe("UserOnly")],
        module_types: vec![
            module_type_probe(module_path, "ModuleRoot"),
            module_type_probe(module_path, "Homonym"),
            module_type_probe(user_module_path, "UserOnly"),
        ],
        global_values: vec![value_probe("Homonym"), value_probe("UserOnly")],
    };

    let global_closure = demanded_product_closure(
        &native,
        &[ProjectLocalDemandRootForTest::ProductGroup(
            global_homonym.clone(),
        )],
    )
    .expect("global closure");
    assert_eq!(global_closure, vec![global_homonym]);
    let global = demanded_probe_projection(&probes, &native, &global_closure, &[]);
    assert_eq!(global.global_types, vec![type_probe("Homonym")]);
    assert_eq!(global.global_values, vec![value_probe("Homonym")]);
    assert!(global.module_types.is_empty());

    let module_closure = demanded_product_closure(
        &native,
        &[ProjectLocalDemandRootForTest::ProductGroup(module_root)],
    )
    .expect("module edge closure");
    assert_eq!(module_closure, vec![groups[1].clone(), module_homonym]);
    let module = demanded_probe_projection(&probes, &native, &module_closure, &[]);
    assert!(module.global_types.is_empty());
    assert!(module.global_values.is_empty());
    assert_eq!(
        module.module_types,
        vec![
            module_type_probe(module_path, "ModuleRoot"),
            module_type_probe(module_path, "Homonym"),
        ]
    );

    let user_contributor = ProjectLocalGlobalContributorForTest {
        name: "UserOnly".to_owned(),
        has_value: true,
        has_type: true,
        has_namespace: false,
        sites: vec![ProjectLocalGlobalContributorSiteForTest {
            source_path: "/user/global.ts".to_owned(),
            origin: ProjectLocalContributorOriginForTest::User,
            span_start: 0,
            span_end: 8,
            kind: ProjectLocalContributorKindForTest::Ordinary,
        }],
    };
    let user = demanded_probe_projection(&probes, &native, &[], &[user_contributor]);
    assert_eq!(user.global_types, vec![type_probe("UserOnly")]);
    assert_eq!(user.global_values, vec![value_probe("UserOnly")]);
    assert!(user.module_types.is_empty());
}

#[test]
fn array_projection_includes_global_iterator_and_excludes_module_homonym() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let inputs = case_inputs("array");
    let product_native = native_product_evidence(&product);
    let native = mapped_native_evidence(&provider, &product, &inputs, &product_native);
    let roots = case_demand_roots("array", &native);
    let materialized = demanded_product_closure(&native, &roots).expect("Array demand closure");
    let oracle = provider
        .complete_source_black_box_oracle_for_test(&inputs)
        .expect("Array black-box observables");
    assert!(oracle
        .force_all_semantic_probes
        .module_types
        .iter()
        .any(|probe| probe.probe.name == "Iterator"));
    let projected = demanded_probe_projection(
        &oracle.force_all_semantic_probes,
        &native,
        &materialized,
        &oracle.global_contributors,
    );

    assert!(projected
        .global_types
        .iter()
        .any(|probe| probe.name == "Iterator"));
    assert!(projected
        .module_types
        .iter()
        .all(|probe| probe.probe.name != "Iterator"));
    let iterator_owners = native
        .probe_evidence
        .iter()
        .filter(|row| match &row.coordinate {
            StableProbeCoordinate::GlobalType { name }
            | StableProbeCoordinate::GlobalValue { name }
            | StableProbeCoordinate::ModuleType { name, .. } => name == "Iterator",
        })
        .collect::<Vec<_>>();
    assert!(iterator_owners.iter().any(|row| {
        materialized
            .iter()
            .any(|group| group.product_key == row.product_key)
            && matches!(row.coordinate, StableProbeCoordinate::GlobalType { .. })
    }));
    assert!(iterator_owners.iter().any(|row| {
        !materialized
            .iter()
            .any(|group| group.product_key == row.product_key)
            && matches!(row.coordinate, StableProbeCoordinate::ModuleType { .. })
    }));
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
    broken.trace.product_dependencies[0].target = native.groups[2].product_key;
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &broken,
        CandidateValidationError::ProductAuthority,
    );
    broken = trace.clone();
    broken
        .trace
        .product_dependencies
        .push(native.dependencies[0].clone());
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &broken,
        CandidateValidationError::ProductAuthority,
    );
    broken = trace.clone();
    broken.trace.materialized_product_groups.pop();
    assert_invalid(
        &native,
        &oracle,
        &roots,
        &broken,
        CandidateValidationError::MaterializedTraceDrift,
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

    let (fault_native, _, fault_roots, mut seam) = valid_control_trace();
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
            &fault_native,
            &fault_roots,
            &seam.observation,
            Some(ProjectLocalSemanticFaultForTest::OmitDependencyEdge),
        ),
        Err(CandidateValidationError::FaultSeamMismatch)
    );
    let (_, _, _, missing) = valid_control_trace();
    assert_eq!(
        validate_fault_observation(
            &fault_native,
            &fault_roots,
            &missing.observation,
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
            &fault_native,
            &fault_roots,
            &late.observation,
            Some(ProjectLocalSemanticFaultForTest::OmitDependencyEdge),
        ),
        Err(CandidateValidationError::FaultAfterReceipt)
    );
}

#[test]
fn project_local_source_lookups_drive_demands_in_both_source_orders() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let product_native = native_product_evidence(&product);
    let mut project_sessions = BTreeSet::new();
    let mut publisher_sessions = BTreeSet::new();
    let mut call_count = 0usize;
    for (case_id, inputs) in oracle_inputs() {
        for source_order in [
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Reverse,
        ] {
            let mut ordered_inputs = inputs.clone();
            if source_order == ProjectLocalOrderForTest::Reverse {
                ordered_inputs.reverse();
            }
            let native =
                mapped_native_evidence(&provider, &product, &ordered_inputs, &product_native);
            let oracle = provider
                .complete_source_black_box_oracle_for_test(&ordered_inputs)
                .expect("black-box complete-source shadow");
            for demand_order in [
                ProjectLocalOrderForTest::Forward,
                ProjectLocalOrderForTest::Reverse,
            ] {
                let roots = case_demand_roots(case_id, &native);
                let trace = candidate_trace(
                    &provider,
                    CandidateHarnessRequest {
                        inputs: ordered_inputs.clone(),
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
fn recursive_product_aliases_preserve_structural_semantics() {
    for (kind, source, expected_code) in [
        (
            RecursiveAliasIndirection::Array,
            "declare const wu4Recursive: Wu4RecursiveArray; const wu4View: Wu4RecursiveArray[] = wu4Recursive; const wu4Back: Wu4RecursiveArray = wu4View; const wu4EdgeWrong: string = wu4Recursive[0];",
            "TK2322",
        ),
        (
            RecursiveAliasIndirection::Tuple,
            "declare const wu4Recursive: Wu4RecursiveTuple; const wu4View: [Wu4RecursiveTuple] = wu4Recursive; const wu4Back: Wu4RecursiveTuple = wu4View; const wu4EdgeWrong: string = wu4Recursive[0];",
            "TK2322",
        ),
        (
            RecursiveAliasIndirection::Function,
            "declare const wu4Recursive: Wu4RecursiveFunction; const wu4View: () => Wu4RecursiveFunction = wu4Recursive; const wu4Back: Wu4RecursiveFunction = wu4View; const wu4EdgeWrong: string = wu4Recursive();",
            "TK2322",
        ),
    ] {
        let product = synthetic_recursive_alias_product(kind);
        let group_name = recursive_alias_name(kind);
        let inputs = vec![input("/candidate/recursive-alias.ts", source)];
        let (result, observation) = candidate_result_with_product(
            product,
            inputs,
            ProjectLocalSemanticRequestForTest {
                case_id: "recursive-alias",
                source_order: ProjectLocalOrderForTest::Forward,
                demand_order: ProjectLocalOrderForTest::Forward,
                demand_roots: Vec::new(),
                force_all: false,
                fault: None,
            },
        );
        let trace = result
            .unwrap_or_else(|error| panic!("{kind:?} recursive alias failed: {error:?}"));
        let result = trace
            .user_check_results
            .iter()
            .find(|row| row.source_path == "/candidate/recursive-alias.ts")
            .expect("recursive alias user result");
        let codes = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            [expected_code],
            "{kind:?} changed {group_name}: {:?}",
            result.incomplete
        );
        let edge_start = u32::try_from(
            source
                .find("const wu4EdgeWrong")
                .expect("recursive edge assertion"),
        )
        .expect("recursive edge start");
        let edge_end = u32::try_from(source.len()).expect("recursive edge end");
        let diagnostic = result.diagnostics.first().expect("recursive edge error");
        assert!(
            diagnostic.span.start >= edge_start && diagnostic.span.end <= edge_end,
            "{kind:?} diagnosed a clean round-trip instead of the recursive edge"
        );
        assert!(result.incomplete.is_empty(), "{kind:?} exhausted {group_name}");
        assert!(observation.events.iter().any(|event| {
            matches!(event.kind, ObservationEventKindForTest::ReceiptBuilt)
        }));
    }
}

#[test]
fn real_idb_cursor_consumption_has_no_partial_ready_or_panic() {
    let provider = LibraryBaseProvider::new();
    let inputs = vec![input(
        "/candidate/idb-cursor-consumption.ts",
        IDB_CURSOR_CONSUMPTION,
    )];
    let oracle = provider
        .complete_source_black_box_oracle_for_test(&inputs)
        .expect("complete-source IDBCursor authority");
    assert!(oracle.user_check_results.iter().any(|row| {
        row.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TK2322")
    }));
    let trace = candidate_trace(
        &provider,
        candidate_request("idb-cursor", inputs, Vec::new(), false, None),
    );
    assert_eq!(trace.trace.user_event_ledger, oracle.user_event_ledger);
    assert_eq!(trace.trace.user_check_results, oracle.user_check_results);
}

#[test]
fn augmented_defaulted_generic_finishes_without_partial_default_or_panic() {
    let provider = LibraryBaseProvider::new();
    let inputs = vec![input(
        "/candidate/augmented-defaulted-generic.ts",
        AUGMENTED_DEFAULTED_GENERIC,
    )];
    let oracle = provider
        .complete_source_black_box_oracle_for_test(&inputs)
        .expect("complete-source augmented generic authority");
    assert!(oracle.user_check_results.iter().any(|row| {
        row.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TK2322")
    }));
    let trace = candidate_trace(
        &provider,
        candidate_request("augmented-default", inputs, Vec::new(), false, None),
    );
    assert_eq!(trace.trace.user_event_ledger, oracle.user_event_ledger);
    assert_eq!(trace.trace.user_check_results, oracle.user_check_results);
}

#[test]
fn publish_before_scc_complete_fails_at_real_seam_before_receipt() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let product_native = native_product_evidence(&product);
    let inputs = case_inputs("array");
    let native = mapped_native_evidence(&provider, &product, &inputs, &product_native);
    let roots = publish_before_scc_complete_contract(&native)
        .expect("authenticated fault SCC")
        .canonical_roots;
    let (result, observation) = candidate_result_with_product(
        (*product).clone(),
        inputs,
        ProjectLocalSemanticRequestForTest {
            case_id: "array",
            source_order: ProjectLocalOrderForTest::Forward,
            demand_order: ProjectLocalOrderForTest::Forward,
            demand_roots: roots,
            force_all: false,
            fault: Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        },
    );
    let fault_events = observation
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                ObservationEventKindForTest::FaultActivated { .. }
                    | ObservationEventKindForTest::DurableCacheWrite { .. }
                    | ObservationEventKindForTest::ReceiptBuilt
            )
        })
        .collect::<Vec<_>>();
    assert!(
        matches!(result, Err(ProjectLocalCandidateErrorForTest::UserPass(_))),
        "unexpected publish-before-SCC events: {fault_events:?}"
    );
    assert!(observation
        .events
        .iter()
        .all(|event| { !matches!(event.kind, ObservationEventKindForTest::ReceiptBuilt) }));
}

#[test]
fn request_product_roots_do_not_change_source_driven_output_or_passive_work() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let inputs = case_inputs("array");
    let product_native = native_product_evidence(&product);
    let native = mapped_native_evidence(&provider, &product, &inputs, &product_native);
    let canonical = case_demand_roots("array", &native);
    let canonical_closure = demanded_product_closure(&native, &canonical)
        .expect("independent canonical closure")
        .into_iter()
        .map(|group| group.product_key)
        .collect::<BTreeSet<_>>();
    let spurious = native
        .groups
        .iter()
        .find(|group| {
            group.compilation_global
                && group.name == "IDBCursor"
                && !canonical_closure.contains(&group.product_key)
        })
        .cloned()
        .expect("independent spurious product root");
    let oracle = provider
        .complete_source_black_box_oracle_for_test(&inputs)
        .expect("Array observable authority");
    let mut observations = Vec::new();
    for roots in [
        canonical,
        Vec::new(),
        vec![ProjectLocalDemandRootForTest::ProductGroup(spurious)],
    ] {
        let trace = candidate_trace(
            &provider,
            candidate_request("array", inputs.clone(), roots, false, None),
        );
        assert_eq!(trace.trace.user_event_ledger, oracle.user_event_ledger);
        assert_eq!(trace.trace.user_check_results, oracle.user_check_results);
        observations.push((
            trace.trace.demand_root_visits.clone(),
            trace.trace.materialized_product_groups.clone(),
            trace.trace.semantic_probes,
            trace.trace.final_global_contributors,
            trace.trace.durable_cache_writes,
            trace.trace.repeated_demands,
            passive_ordinary_owner_keys(&trace.observation),
            passive_cache_event_rows(&trace.observation),
        ));
    }
    assert!(observations.windows(2).all(|rows| rows[0] == rows[1]));
}

#[test]
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
            publish_before_scc_complete_contract(&native)
                .expect("authenticated Symbol publication SCC")
                .canonical_roots
        } else {
            case_demand_roots(case_id, &native)
        };
        if fault == ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete {
            let sibling = candidate_trace(
                &provider,
                candidate_request(case_id, inputs.clone(), roots.clone(), false, None),
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
                sibling.evidence(),
            )
            .expect("identical canonical Array roots are valid without fault injection");
            let (result, observation) = candidate_result_with_product(
                (*product).clone(),
                inputs,
                ProjectLocalSemanticRequestForTest {
                    case_id,
                    source_order: ProjectLocalOrderForTest::Forward,
                    demand_order: ProjectLocalOrderForTest::Forward,
                    demand_roots: roots,
                    force_all: false,
                    fault: Some(fault),
                },
            );
            assert!(matches!(
                result,
                Err(ProjectLocalCandidateErrorForTest::UserPass(_))
            ));
            assert!(observation
                .events
                .iter()
                .all(|event| !matches!(event.kind, ObservationEventKindForTest::ReceiptBuilt)));
            continue;
        }
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
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let product_native = native_product_evidence(&product);
    let native =
        mapped_native_evidence(&provider, &product, &case_inputs("array"), &product_native);
    let contract = publish_before_scc_complete_contract(&native)
        .expect("authenticated Symbol publication SCC");
    assert_eq!(
        contract.canonical_roots,
        case_demand_roots("array", &native)
    );
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

#[test]
fn publish_before_scc_complete_uses_exact_canonical_symbol_contract() {
    let provider = LibraryBaseProvider::new();
    let product = provider.declaration_product().expect("declaration product");
    let product_native = native_product_evidence(&product);
    let native =
        mapped_native_evidence(&provider, &product, &case_inputs("array"), &product_native);
    let contract = publish_before_scc_complete_contract(&native)
        .expect("authenticated Symbol publication SCC");
    assert_eq!(
        contract.canonical_roots,
        case_demand_roots("array", &native)
    );
    assert_eq!(
        contract
            .selected_scc
            .members
            .iter()
            .map(|member| member.name.as_str())
            .collect::<BTreeSet<_>>(),
        ["Symbol", "SymbolConstructor"].into_iter().collect()
    );
    assert!(contract
        .selected_scc
        .members
        .iter()
        .all(|member| member.compilation_global));
    let canonical_closure = demanded_product_closure(&native, &contract.canonical_roots)
        .expect("canonical Array closure")
        .into_iter()
        .map(|group| group.product_key)
        .collect::<BTreeSet<_>>();
    assert!(contract
        .selected_scc
        .members
        .iter()
        .all(|member| canonical_closure.contains(&member.product_key)));
    let demanded_nontrivial_sccs = native
        .publication_sccs
        .iter()
        .filter(|scc| {
            scc.members.len() > 1
                && scc
                    .members
                    .iter()
                    .any(|member| canonical_closure.contains(&member.product_key))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        demanded_nontrivial_sccs,
        vec![&contract.selected_scc, &contract.clean_scc]
    );
    assert_eq!(
        contract
            .clean_scc
            .members
            .iter()
            .map(|member| member.name.as_str())
            .collect::<BTreeSet<_>>(),
        ["FlatArray", "ReadonlyArray"].into_iter().collect()
    );
    validate_publish_before_scc_complete_roots(&native, &contract.canonical_roots)
        .expect("exact canonical Array roots");
    let mut missing_root = contract.canonical_roots.clone();
    missing_root.pop().expect("canonical Array root");
    assert_eq!(
        validate_publish_before_scc_complete_roots(&native, &missing_root),
        Err(CandidateValidationError::FaultTargetContract)
    );
    let replaced = vec![ProjectLocalDemandRootForTest::ProductGroup(
        contract.selected_owner.clone(),
    )];
    assert_eq!(
        validate_publish_before_scc_complete_roots(&native, &replaced),
        Err(CandidateValidationError::FaultTargetContract)
    );
    let singleton = native
        .publication_sccs
        .iter()
        .find_map(|scc| {
            let [member] = scc.members.as_slice() else {
                return None;
            };
            member.compilation_global.then_some(member.clone())
        })
        .expect("independent compilation-global singleton");
    let mut singleton_roots = contract.canonical_roots.clone();
    singleton_roots.push(ProjectLocalDemandRootForTest::ProductGroup(singleton));
    assert_eq!(
        validate_publish_before_scc_complete_roots(&native, &singleton_roots),
        Err(CandidateValidationError::FaultTargetContract)
    );
    let module_homonym = native
        .probe_evidence
        .iter()
        .find_map(|evidence| match &evidence.coordinate {
            StableProbeCoordinate::ModuleType { .. } => native
                .groups
                .get(evidence.product_key)
                .filter(|group| !group.compilation_global)
                .cloned(),
            _ => None,
        })
        .expect("external-module probe owner");
    let mut module_roots = contract.canonical_roots.clone();
    module_roots.push(ProjectLocalDemandRootForTest::ProductGroup(module_homonym));
    assert_eq!(
        validate_publish_before_scc_complete_roots(&native, &module_roots),
        Err(CandidateValidationError::FaultTargetContract)
    );
    let wrong_scc = native
        .publication_sccs
        .iter()
        .find(|scc| scc.members.len() > 1 && **scc != contract.selected_scc)
        .expect("independent wrong nontrivial SCC");
    let mut wrong_scc_roots = contract.canonical_roots.clone();
    wrong_scc_roots.push(ProjectLocalDemandRootForTest::ProductGroup(
        wrong_scc.members[0].clone(),
    ));
    assert_eq!(
        validate_publish_before_scc_complete_roots(&native, &wrong_scc_roots),
        Err(CandidateValidationError::FaultTargetContract)
    );

    let selected_index = native
        .publication_sccs
        .iter()
        .position(|scc| *scc == contract.selected_scc)
        .expect("selected SCC index");
    let mut duplicate_member = native.clone();
    duplicate_member.publication_sccs[selected_index].members[1] =
        duplicate_member.publication_sccs[selected_index].members[0].clone();
    assert_eq!(
        publish_before_scc_complete_contract(&duplicate_member),
        Err(CandidateValidationError::FaultTargetContract)
    );
    let mut wrong_member = native.clone();
    wrong_member.publication_sccs[selected_index].members[1] = wrong_scc.members[0].clone();
    assert_eq!(
        publish_before_scc_complete_contract(&wrong_member),
        Err(CandidateValidationError::FaultTargetContract)
    );
    let mut missing_edge = native.clone();
    let selected_keys = contract
        .selected_scc
        .members
        .iter()
        .map(|member| member.product_key)
        .collect::<BTreeSet<_>>();
    missing_edge.dependencies.retain(|edge| {
        edge.owner != contract.selected_owner.product_key || !selected_keys.contains(&edge.target)
    });
    assert_eq!(
        publish_before_scc_complete_contract(&missing_edge),
        Err(CandidateValidationError::FaultTargetContract)
    );
    let mut duplicate_edge = native.clone();
    let edge = duplicate_edge
        .dependencies
        .iter()
        .find(|edge| selected_keys.contains(&edge.owner) && selected_keys.contains(&edge.target))
        .cloned()
        .expect("selected internal dependency");
    duplicate_edge.dependencies.push(edge);
    assert_eq!(
        publish_before_scc_complete_contract(&duplicate_edge),
        Err(CandidateValidationError::FaultTargetContract)
    );

    let materialized = demanded_product_closure(&native, &contract.canonical_roots)
        .expect("canonical materialized closure");
    let materialized_keys = materialized
        .iter()
        .map(|group| group.product_key)
        .collect::<BTreeSet<_>>();
    let mut commits = native
        .publication_sccs
        .iter()
        .filter(|scc| {
            scc.members
                .iter()
                .any(|member| materialized_keys.contains(&member.product_key))
        })
        .map(|scc| ProjectLocalPublicationSccCommitForTest {
            scc: scc.clone(),
            reserved_members: scc.members.clone(),
            published_members: scc.members.clone(),
            observed_before_complete: Vec::new(),
            state_before_publish: ProjectLocalTerminalStateForTest::Resolving,
            final_state: ProjectLocalTerminalStateForTest::Ready,
        })
        .collect::<Vec<_>>();
    let premature_index = commits
        .iter()
        .position(|commit| commit.scc == contract.selected_scc)
        .expect("selected commit");
    commits[premature_index].observed_before_complete = contract.selected_scc.members.clone();
    assert_eq!(
        validate_commits(
            &native,
            &contract.canonical_roots,
            &materialized,
            &commits,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::PrematureSccPublication)
    );
    let mut wrong_published_member = commits.clone();
    wrong_published_member[premature_index]
        .published_members
        .pop();
    assert_eq!(
        validate_commits(
            &native,
            &contract.canonical_roots,
            &materialized,
            &wrong_published_member,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultCommitContract)
    );
    let mut duplicate_published_member = commits.clone();
    duplicate_published_member[premature_index]
        .published_members
        .push(contract.selected_scc.members[0].clone());
    assert_eq!(
        validate_commits(
            &native,
            &contract.canonical_roots,
            &materialized,
            &duplicate_published_member,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultCommitContract)
    );
    let exact_second_commit = commits
        .iter()
        .position(|commit| commit.scc == contract.clean_scc)
        .expect("exact ReadonlyArray / FlatArray commit");
    let mut wrong_commit_scc = commits.clone();
    wrong_commit_scc[exact_second_commit].observed_before_complete =
        wrong_commit_scc[exact_second_commit].scc.members.clone();
    assert_eq!(
        validate_commits(
            &native,
            &contract.canonical_roots,
            &materialized,
            &wrong_commit_scc,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultCommitContract)
    );

    let selected_member_keys = contract
        .selected_scc
        .members
        .iter()
        .map(|member| member.product_key)
        .collect::<BTreeSet<_>>();
    let first_selected_cache = *selected_member_keys
        .iter()
        .next()
        .expect("selected cache owner");
    let fault_kind = ObservationEventKindForTest::FaultActivated {
        fault: ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete,
        seam: ProjectLocalFaultSeamForTest::SccCompletion,
    };
    let mut events = selected_member_keys
        .iter()
        .enumerate()
        .map(|(index, product_group)| {
            observation_event(
                index as u64 + 1,
                ObservationEventKindForTest::OwnerMaterialized {
                    phase: ProjectLocalSemanticPhaseForTest::Ordinary,
                    product_group: *product_group,
                },
            )
        })
        .collect::<Vec<_>>();
    let fault_index = events.len();
    events.push(observation_event(
        fault_index as u64 + 1,
        fault_kind.clone(),
    ));
    events.push(observation_event(
        fault_index as u64 + 2,
        ObservationEventKindForTest::DurableCacheWrite {
            publisher: 1,
            product_group: first_selected_cache,
        },
    ));
    events.push(observation_event(
        fault_index as u64 + 3,
        ObservationEventKindForTest::ReceiptBuilt,
    ));
    let observation = ObservationReceiptForTest {
        events,
        witness_kind: ObservationWitnessKindForTest::SyntheticSchema,
        session_weak: Vec::new(),
        cache_weak: Vec::new(),
    };
    assert_eq!(
        validate_fault_observation(
            &native,
            &contract.canonical_roots,
            &observation,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Ok(())
    );
    let mut missing_activation = observation.clone();
    missing_activation.events.remove(fault_index);
    assert_eq!(
        validate_fault_observation(
            &native,
            &contract.canonical_roots,
            &missing_activation,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultNotReached)
    );
    let mut wrong_seam = observation.clone();
    wrong_seam.events[fault_index].kind = ObservationEventKindForTest::FaultActivated {
        fault: ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete,
        seam: ProjectLocalFaultSeamForTest::DependencyProjection,
    };
    assert_eq!(
        validate_fault_observation(
            &native,
            &contract.canonical_roots,
            &wrong_seam,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultSeamMismatch)
    );
    let mut duplicate_activation = observation.clone();
    duplicate_activation.events.insert(
        fault_index + 1,
        observation_event(fault_index as u64 + 1, fault_kind),
    );
    assert_eq!(
        validate_fault_observation(
            &native,
            &contract.canonical_roots,
            &duplicate_activation,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultNotReached)
    );
    let mut duplicate_stamp = observation.clone();
    duplicate_stamp.events[fault_index].stamp = duplicate_stamp.events[fault_index - 1].stamp;
    assert_eq!(
        validate_fault_observation(
            &native,
            &contract.canonical_roots,
            &duplicate_stamp,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultActivationOrder)
    );
    let restamp = |observation: &mut ObservationReceiptForTest| {
        for (index, event) in observation.events.iter_mut().enumerate() {
            event.stamp = index as u64 + 1;
        }
    };
    let mut before_any_owner = observation.clone();
    let fault_event = before_any_owner.events.remove(fault_index);
    before_any_owner.events.insert(0, fault_event);
    restamp(&mut before_any_owner);
    assert_eq!(
        validate_fault_observation(
            &native,
            &contract.canonical_roots,
            &before_any_owner,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultActivationOrder)
    );
    let mut after_one_owner = observation.clone();
    let fault_event = after_one_owner.events.remove(fault_index);
    after_one_owner.events.insert(1, fault_event);
    restamp(&mut after_one_owner);
    assert_eq!(
        validate_fault_observation(
            &native,
            &contract.canonical_roots,
            &after_one_owner,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultActivationOrder)
    );
    let mut after_selected_cache = observation.clone();
    let fault_event = after_selected_cache.events.remove(fault_index);
    after_selected_cache
        .events
        .insert(fault_index + 1, fault_event);
    restamp(&mut after_selected_cache);
    assert_eq!(
        validate_fault_observation(
            &native,
            &contract.canonical_roots,
            &after_selected_cache,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultActivationOrder)
    );
    let mut late_stamp = observation;
    let receipt_stamp = late_stamp.events.last().expect("receipt event").stamp;
    late_stamp.events[fault_index].stamp = receipt_stamp;
    assert_eq!(
        validate_fault_observation(
            &native,
            &contract.canonical_roots,
            &late_stamp,
            Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
        ),
        Err(CandidateValidationError::FaultAfterReceipt)
    );
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
    let _ = orders;
    if trace.case_id != case_id
        || trace.project_session_identity == 0
        || trace.publisher_session_identity == 0
        || trace.project_session_identity == trace.publisher_session_identity
    {
        return Err(CandidateValidationError::InvalidSession);
    }
    validate_fault_observation(native, roots, observation, fault)?;
    if trace.source_inputs != oracle.source_inputs
        || trace.source_visit_rows != oracle.source_visit_rows
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
    let requested_controls = roots
        .iter()
        .filter(|root| !matches!(root, ProjectLocalDemandRootForTest::ProductGroup(_)))
        .collect::<Vec<_>>();
    let observed_controls = trace
        .demand_root_visits
        .iter()
        .filter(|root| !matches!(root, ProjectLocalDemandRootForTest::ProductGroup(_)))
        .collect::<Vec<_>>();
    let source_roots = trace
        .demand_root_visits
        .iter()
        .filter_map(|root| match root {
            ProjectLocalDemandRootForTest::ProductGroup(group)
                if native.groups.get(group.product_key) == Some(group) =>
            {
                Some(root.clone())
            }
            ProjectLocalDemandRootForTest::ProductGroup(_) => None,
            _ => None,
        })
        .collect::<Vec<_>>();
    let source_root_count = trace
        .demand_root_visits
        .iter()
        .filter(|root| matches!(root, ProjectLocalDemandRootForTest::ProductGroup(_)))
        .count();
    let unique_source_roots = source_roots.iter().collect::<BTreeSet<_>>();
    if observed_controls != requested_controls
        || source_roots.len() != source_root_count
        || unique_source_roots.len() != source_roots.len()
    {
        return Err(CandidateValidationError::DemandOrder);
    }
    let materialized = demanded_product_closure(native, &source_roots)?;
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
        native,
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
    validate_commits(
        native,
        roots,
        &materialized,
        &trace.publication_scc_commits,
        fault,
    )?;
    validate_cache_and_repeats(&source_roots, &materialized, trace)?;
    validate_internal_publications(case_id, trace)?;
    match (&trace.force_all, force_all_requested) {
        (None, false) => Ok(()),
        (Some(force_all), true) => validate_force_all(native, oracle, force_all, observation),
        _ => Err(CandidateValidationError::ForceAllPresence),
    }
}

fn validate_commits(
    native: &NativeProductEvidence,
    roots: &[ProjectLocalDemandRootForTest],
    materialized: &[ProjectLocalProductGroupForTest],
    commits: &[ProjectLocalPublicationSccCommitForTest],
    fault: Option<ProjectLocalSemanticFaultForTest>,
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
    let committed_sccs = commits
        .iter()
        .map(|commit| &commit.scc)
        .collect::<BTreeSet<_>>();
    if committed_sccs.len() != commits.len()
        || expected.iter().any(|scc| !committed_sccs.contains(scc))
    {
        return Err(CandidateValidationError::PublicationSccCensus);
    }
    let premature_contract =
        if fault == Some(ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete) {
            Some(validate_publish_before_scc_complete_roots(native, roots)?)
        } else {
            None
        };
    let malformed_commit_error = if premature_contract.is_some() {
        CandidateValidationError::FaultCommitContract
    } else {
        CandidateValidationError::PrematureSccPublication
    };
    let mut premature_commits = 0usize;
    for scc in expected {
        let commit = commits
            .iter()
            .find(|commit| commit.scc == *scc)
            .ok_or(CandidateValidationError::PublicationSccCensus)?;
        if commit.scc != *scc
            || commit.reserved_members != scc.members
            || commit.state_before_publish != ProjectLocalTerminalStateForTest::Resolving
        {
            return Err(malformed_commit_error);
        }
        let selected_for_fault = premature_contract
            .as_ref()
            .is_some_and(|contract| contract.selected_scc == *scc);
        if selected_for_fault {
            if commit.observed_before_complete != scc.members
                || commit.published_members != scc.members
                || commit.final_state != ProjectLocalTerminalStateForTest::Ready
            {
                return Err(malformed_commit_error);
            }
            premature_commits = premature_commits.saturating_add(1);
            continue;
        }
        if !commit.observed_before_complete.is_empty() {
            return Err(malformed_commit_error);
        }
        match commit.final_state {
            ProjectLocalTerminalStateForTest::Ready if commit.published_members == scc.members => {}
            ProjectLocalTerminalStateForTest::Unavailable(_)
                if commit.published_members.is_empty() => {}
            _ if premature_contract.is_some() => return Err(malformed_commit_error),
            _ => return Err(CandidateValidationError::AtomicSccPublication),
        }
    }
    if premature_contract.is_some() {
        if premature_commits == 1 {
            Err(CandidateValidationError::PrematureSccPublication)
        } else {
            Err(CandidateValidationError::FaultNotReached)
        }
    } else {
        Ok(())
    }
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
    let observed_hits = observed_hits
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut reported_hits = trace
        .repeated_demands
        .iter()
        .map(|row| row.owner.clone())
        .collect::<Vec<_>>();
    reported_hits.sort();
    reported_hits.dedup();
    if observed_hits != reported_hits {
        return Err(CandidateValidationError::PassiveCacheCensus);
    }
    Ok(())
}

fn validate_fault_observation(
    native: &NativeProductEvidence,
    roots: &[ProjectLocalDemandRootForTest],
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
    if expected_fault == ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete {
        let contract = validate_publish_before_scc_complete_roots(native, roots)?;
        let selected_members = contract
            .selected_scc
            .members
            .iter()
            .map(|member| member.product_key)
            .collect::<BTreeSet<_>>();
        let mut last_owner_stamp = 0;
        for selected_member in &selected_members {
            let owner_stamps = observation
                .events
                .iter()
                .filter_map(|event| match event.kind {
                    ObservationEventKindForTest::OwnerMaterialized {
                        phase: ProjectLocalSemanticPhaseForTest::Ordinary,
                        product_group,
                    } if product_group == *selected_member => Some(event.stamp),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [owner_stamp] = owner_stamps.as_slice() else {
                return Err(CandidateValidationError::FaultActivationOrder);
            };
            last_owner_stamp = last_owner_stamp.max(*owner_stamp);
        }
        let first_cache_stamp = observation
            .events
            .iter()
            .filter_map(|event| match event.kind {
                ObservationEventKindForTest::DurableCacheWrite { product_group, .. }
                    if selected_members.contains(&product_group) =>
                {
                    Some(event.stamp)
                }
                _ => None,
            })
            .min();
        let Some(first_cache_stamp) = first_cache_stamp else {
            return Err(CandidateValidationError::FaultActivationOrder);
        };
        if observation
            .events
            .windows(2)
            .any(|events| events[0].stamp >= events[1].stamp)
            || last_owner_stamp >= *stamp
            || *stamp >= first_cache_stamp
        {
            return Err(CandidateValidationError::FaultActivationOrder);
        }
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
    let module_probe_sources = product
        .groups()
        .iter()
        .enumerate()
        .filter_map(|(product_key, group)| {
            let container = &product.containers()[group.container.index()];
            if container.kind != typokat_library_product::ContainerKind::ExternalModule
                || container.parent.is_some()
            {
                return None;
            }
            let anchor = container.anchor?;
            let source = &product.sources()[anchor.source.index()];
            Some((product_key, product.strings()[source.name.index()].clone()))
        })
        .collect::<BTreeMap<_, _>>();
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
    let containment_targets = product
        .groups()
        .iter()
        .enumerate()
        .filter_map(|(owner, group)| {
            let children = group
                .fragments
                .iter()
                .filter_map(|declaration| {
                    match product.declarations()[declaration.index()].payload {
                        DeclarationPayload::Namespace { child_region }
                        | DeclarationPayload::Global { child_region } => {
                            Some(product.regions()[child_region.index()].container)
                        }
                        _ => None,
                    }
                })
                .flat_map(|container| {
                    product
                        .groups()
                        .iter()
                        .enumerate()
                        .filter_map(move |(index, child)| {
                            (child.container == container
                                && (child.slots.namespace
                                    || product_group_has_runtime_value(product, child)))
                            .then_some(index)
                        })
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            (!children.is_empty()).then_some((owner, children))
        })
        .collect();
    NativeProductEvidence {
        groups,
        mapped_groups: Vec::new(),
        module_probe_sources,
        probe_evidence: Vec::new(),
        dependencies,
        containment_targets,
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
        .collect::<Vec<_>>();
    let probe_evidence = stable_probe_evidence(product, &mapped_groups);
    NativeProductEvidence {
        groups: native.groups.clone(),
        mapped_groups,
        module_probe_sources: native.module_probe_sources.clone(),
        probe_evidence,
        dependencies: native.dependencies.clone(),
        containment_targets: native.containment_targets.clone(),
        publication_sccs: native.publication_sccs.clone(),
    }
}

fn product_group_has_runtime_value(
    product: &LibraryDeclarationProduct,
    group: &typokat_library_product::DeclarationGroupRecipe,
) -> bool {
    group.fragments.iter().any(|declaration| {
        matches!(
            product.declarations()[declaration.index()].payload,
            DeclarationPayload::Variable { .. }
                | DeclarationPayload::Function { .. }
                | DeclarationPayload::Class { .. }
                | DeclarationPayload::Enum { .. }
                | DeclarationPayload::Namespace { .. }
                | DeclarationPayload::Global { .. }
        )
    })
}

fn stable_probe_evidence(
    product: &LibraryDeclarationProduct,
    mapped_groups: &[ProjectLocalMappedProductGroupForTest],
) -> Vec<StableProbeEvidence> {
    let mut evidence = Vec::new();
    for mapped in mapped_groups {
        if mapped.product.compilation_global {
            if let (true, Some(local_type_group)) =
                (mapped.product.has_type, mapped.local_type_group)
            {
                evidence.push(StableProbeEvidence {
                    product_key: mapped.product.product_key,
                    mapped_slot: StableProbeSlot::Type(local_type_group),
                    coordinate: StableProbeCoordinate::GlobalType {
                        name: mapped.product.name.clone(),
                    },
                });
            }
            if let (true, Some(local_value)) = (mapped.product.has_value, mapped.local_value) {
                evidence.push(StableProbeEvidence {
                    product_key: mapped.product.product_key,
                    mapped_slot: StableProbeSlot::Value(local_value),
                    coordinate: StableProbeCoordinate::GlobalValue {
                        name: mapped.product.name.clone(),
                    },
                });
            }
        }
        let group = &product.groups()[mapped.product.product_key];
        let container = &product.containers()[group.container.index()];
        if container.kind == typokat_library_product::ContainerKind::ExternalModule
            && container.parent.is_none()
        {
            if let (true, Some(local_type_group), Some(anchor)) = (
                mapped.product.has_type,
                mapped.local_type_group,
                container.anchor,
            ) {
                let source = &product.sources()[anchor.source.index()];
                evidence.push(StableProbeEvidence {
                    product_key: mapped.product.product_key,
                    mapped_slot: StableProbeSlot::Type(local_type_group),
                    coordinate: StableProbeCoordinate::ModuleType {
                        source_path: product.strings()[source.name.index()].clone(),
                        name: mapped.product.name.clone(),
                    },
                });
            }
        }
    }
    evidence.sort();
    evidence
}

fn stable_probe_evidence_matches_mapped_slots(native: &NativeProductEvidence) -> bool {
    expected_stable_probe_evidence(native).is_some_and(|expected| native.probe_evidence == expected)
}

fn expected_stable_probe_evidence(
    native: &NativeProductEvidence,
) -> Option<Vec<StableProbeEvidence>> {
    let mapped_product_keys = native
        .mapped_groups
        .iter()
        .map(|mapped| mapped.product.product_key)
        .collect::<BTreeSet<_>>();
    if native.mapped_groups.len() != native.groups.len()
        || mapped_product_keys.len() != native.groups.len()
        || native
            .groups
            .iter()
            .enumerate()
            .any(|(product_key, group)| group.product_key != product_key)
        || native
            .mapped_groups
            .iter()
            .any(|mapped| native.groups.get(mapped.product.product_key) != Some(&mapped.product))
        || native.module_probe_sources.iter().any(|(product_key, _)| {
            native
                .groups
                .get(*product_key)
                .is_none_or(|group| group.compilation_global)
        })
    {
        return None;
    }

    let mut expected = Vec::new();
    for mapped in &native.mapped_groups {
        if mapped.product.compilation_global {
            if let (true, Some(identity)) = (mapped.product.has_type, mapped.local_type_group) {
                expected.push(StableProbeEvidence {
                    product_key: mapped.product.product_key,
                    mapped_slot: StableProbeSlot::Type(identity),
                    coordinate: StableProbeCoordinate::GlobalType {
                        name: mapped.product.name.clone(),
                    },
                });
            }
            if let (true, Some(identity)) = (mapped.product.has_value, mapped.local_value) {
                expected.push(StableProbeEvidence {
                    product_key: mapped.product.product_key,
                    mapped_slot: StableProbeSlot::Value(identity),
                    coordinate: StableProbeCoordinate::GlobalValue {
                        name: mapped.product.name.clone(),
                    },
                });
            }
        }
        if let (true, Some(identity), Some(source_path)) = (
            mapped.product.has_type,
            mapped.local_type_group,
            native.module_probe_sources.get(&mapped.product.product_key),
        ) {
            expected.push(StableProbeEvidence {
                product_key: mapped.product.product_key,
                mapped_slot: StableProbeSlot::Type(identity),
                coordinate: StableProbeCoordinate::ModuleType {
                    source_path: source_path.clone(),
                    name: mapped.product.name.clone(),
                },
            });
        }
    }
    expected.sort();
    expected
        .windows(2)
        .all(|rows| rows[0] < rows[1])
        .then_some(expected)
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
    for (owner, targets) in &native.containment_targets {
        dependencies
            .entry(*owner)
            .or_default()
            .extend(targets.iter().copied());
    }
    let mut closure = Vec::new();
    let mut seen = BTreeSet::new();
    for root in roots.iter().filter_map(|root| match root {
        ProjectLocalDemandRootForTest::ProductGroup(group) => Some(group.product_key),
        _ => None,
    }) {
        let mut pending = vec![root];
        let mut root_closure = BTreeSet::new();
        while let Some(owner) = pending.pop() {
            for member in sccs
                .get(&owner)
                .ok_or(CandidateValidationError::ProductGraph)?
            {
                if root_closure.insert(*member) {
                    pending.extend(dependencies.get(member).into_iter().flatten().copied());
                }
            }
        }
        closure.extend(root_closure.into_iter().filter(|key| seen.insert(*key)));
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
    native: &NativeProductEvidence,
    materialized: &[ProjectLocalProductGroupForTest],
    contributors: &[ProjectLocalGlobalContributorForTest],
) -> ProjectLocalSemanticProbesForTest {
    assert!(
        stable_probe_evidence_matches_mapped_slots(native),
        "stable probe evidence must preserve the exact product-to-mapped-slot relation"
    );
    let materialized = materialized
        .iter()
        .map(|row| row.product_key)
        .collect::<BTreeSet<_>>();
    let mut coordinates = native
        .probe_evidence
        .iter()
        .filter(|row| materialized.contains(&row.product_key))
        .map(|row| row.coordinate.clone())
        .collect::<BTreeSet<_>>();
    for contributor in contributors.iter().filter(|row| {
        row.sites
            .iter()
            .any(|site| site.origin == ProjectLocalContributorOriginForTest::User)
    }) {
        if contributor.has_type {
            coordinates.insert(StableProbeCoordinate::GlobalType {
                name: contributor.name.clone(),
            });
        }
        if contributor.has_value {
            coordinates.insert(StableProbeCoordinate::GlobalValue {
                name: contributor.name.clone(),
            });
        }
    }
    ProjectLocalSemanticProbesForTest {
        global_types: probes
            .global_types
            .iter()
            .filter(|row| {
                coordinates.contains(&StableProbeCoordinate::GlobalType {
                    name: row.name.clone(),
                })
            })
            .cloned()
            .collect(),
        module_types: probes
            .module_types
            .iter()
            .filter(|row| {
                coordinates.contains(&StableProbeCoordinate::ModuleType {
                    source_path: row.source_path.clone(),
                    name: row.probe.name.clone(),
                })
            })
            .cloned()
            .collect(),
        global_values: probes
            .global_values
            .iter()
            .filter(|row| {
                coordinates.contains(&StableProbeCoordinate::GlobalValue {
                    name: row.name.clone(),
                })
            })
            .cloned()
            .collect(),
    }
}

fn publish_before_scc_complete_contract(
    native: &NativeProductEvidence,
) -> Result<PublishBeforeSccCompleteContract, CandidateValidationError> {
    if !stable_probe_evidence_matches_mapped_slots(native) {
        return Err(CandidateValidationError::FaultTargetContract);
    }
    let expected_names = ["Symbol", "SymbolConstructor"]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let candidates = native
        .publication_sccs
        .iter()
        .filter(|scc| {
            scc.members.len() > 1
                && scc.members.iter().all(|member| member.compilation_global)
                && scc
                    .members
                    .iter()
                    .map(|member| member.name.as_str())
                    .collect::<BTreeSet<_>>()
                    == expected_names
        })
        .collect::<Vec<_>>();
    let [selected_scc] = candidates.as_slice() else {
        return Err(CandidateValidationError::FaultTargetContract);
    };
    let selected_keys = selected_scc
        .members
        .iter()
        .map(|member| member.product_key)
        .collect::<BTreeSet<_>>();
    if selected_keys.len() != selected_scc.members.len()
        || selected_scc.members.iter().any(|member| {
            native.groups.get(member.product_key) != Some(member)
                || native
                    .mapped_groups
                    .iter()
                    .filter(|mapped| mapped.product == *member)
                    .count()
                    != 1
        })
    {
        return Err(CandidateValidationError::FaultTargetContract);
    }
    if native.dependencies.iter().collect::<BTreeSet<_>>().len() != native.dependencies.len() {
        return Err(CandidateValidationError::FaultTargetContract);
    }
    let mut internal_edges = BTreeMap::<usize, Vec<usize>>::new();
    for edge in native
        .dependencies
        .iter()
        .filter(|edge| selected_keys.contains(&edge.owner) && selected_keys.contains(&edge.target))
    {
        internal_edges
            .entry(edge.owner)
            .or_default()
            .push(edge.target);
    }
    let strongly_connected = selected_keys.iter().all(|start| {
        let mut reached = BTreeSet::new();
        let mut pending = vec![*start];
        while let Some(owner) = pending.pop() {
            if reached.insert(owner) {
                pending.extend(internal_edges.get(&owner).into_iter().flatten().copied());
            }
        }
        reached == selected_keys
    });
    if !strongly_connected {
        return Err(CandidateValidationError::FaultTargetContract);
    }

    let selected_owner = selected_scc
        .members
        .iter()
        .find(|member| member.name == "SymbolConstructor")
        .cloned()
        .ok_or(CandidateValidationError::FaultTargetContract)?;
    let mapped = native
        .mapped_groups
        .iter()
        .find(|mapped| mapped.product == selected_owner)
        .ok_or(CandidateValidationError::FaultTargetContract)?;
    let mapped_type = mapped
        .local_type_group
        .ok_or(CandidateValidationError::FaultTargetContract)?;
    let expected_evidence = StableProbeEvidence {
        product_key: selected_owner.product_key,
        mapped_slot: StableProbeSlot::Type(mapped_type),
        coordinate: StableProbeCoordinate::GlobalType {
            name: selected_owner.name.clone(),
        },
    };
    if !native.probe_evidence.contains(&expected_evidence)
        || native.probe_evidence.iter().any(|evidence| {
            evidence.product_key == selected_owner.product_key
                && matches!(
                    evidence.coordinate,
                    StableProbeCoordinate::ModuleType { .. }
                )
        })
    {
        return Err(CandidateValidationError::FaultTargetContract);
    }

    let canonical_roots = case_demand_roots("array", native);
    let canonical_closure = demanded_product_closure(native, &canonical_roots)?
        .into_iter()
        .map(|group| group.product_key)
        .collect::<BTreeSet<_>>();
    if !selected_scc
        .members
        .iter()
        .all(|member| canonical_closure.contains(&member.product_key))
    {
        return Err(CandidateValidationError::FaultTargetContract);
    }
    let demanded_nontrivial_sccs = native
        .publication_sccs
        .iter()
        .filter(|scc| {
            scc.members.len() > 1
                && scc
                    .members
                    .iter()
                    .any(|member| canonical_closure.contains(&member.product_key))
        })
        .collect::<Vec<_>>();
    let [first_scc, clean_scc] = demanded_nontrivial_sccs.as_slice() else {
        return Err(CandidateValidationError::FaultTargetContract);
    };
    let clean_names = clean_scc
        .members
        .iter()
        .map(|member| member.name.as_str())
        .collect::<BTreeSet<_>>();
    if **first_scc != **selected_scc
        || clean_scc.members.len() != 2
        || clean_names != ["FlatArray", "ReadonlyArray"].into_iter().collect()
        || clean_scc.members.iter().any(|member| {
            if !member.compilation_global {
                return true;
            }
            let mapped = native
                .mapped_groups
                .iter()
                .find(|mapped| mapped.product == *member);
            let Some(mapped_type) = mapped.and_then(|mapped| mapped.local_type_group) else {
                return true;
            };
            !native.probe_evidence.contains(&StableProbeEvidence {
                product_key: member.product_key,
                mapped_slot: StableProbeSlot::Type(mapped_type),
                coordinate: StableProbeCoordinate::GlobalType {
                    name: member.name.clone(),
                },
            })
        })
    {
        return Err(CandidateValidationError::FaultTargetContract);
    }
    Ok(PublishBeforeSccCompleteContract {
        canonical_roots,
        selected_owner,
        selected_scc: (*selected_scc).clone(),
        clean_scc: (*clean_scc).clone(),
    })
}

fn validate_publish_before_scc_complete_roots(
    native: &NativeProductEvidence,
    roots: &[ProjectLocalDemandRootForTest],
) -> Result<PublishBeforeSccCompleteContract, CandidateValidationError> {
    let contract = publish_before_scc_complete_contract(native)?;
    if roots != contract.canonical_roots {
        return Err(CandidateValidationError::FaultTargetContract);
    }
    Ok(contract)
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
    let type_group = |name: &str| {
        native
            .groups
            .iter()
            .find(|row| row.name == name && row.compilation_global && row.has_type)
            .cloned()
            .map(ProjectLocalDemandRootForTest::ProductGroup)
            .unwrap_or_else(|| panic!("at least one product type group named {name}"))
    };
    match case_id {
        "reflect" => [vec![type_group("Array")], groups("Reflect")].concat(),
        "array" => vec![
            groups("ConcatArray")
                .into_iter()
                .next()
                .expect("ConcatArray product group"),
            type_group("Array"),
            groups("Reflect")
                .into_iter()
                .next()
                .expect("Reflect product group"),
        ],
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
        "mixed-forward" | "mixed-reverse" => groups("Array"),
        "isolation-a" | "isolation-b" => [groups("Array"), vec![type_group("Object")]].concat(),
        "official-assignment-compat-between-tuple-and-array" => {
            vec![type_group("Array")]
        }
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
    let mapped_groups = groups
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
        .collect::<Vec<_>>();
    let probe_evidence = mapped_groups
        .iter()
        .map(|mapped| StableProbeEvidence {
            product_key: mapped.product.product_key,
            mapped_slot: StableProbeSlot::Type(
                mapped
                    .local_type_group
                    .expect("control product has a mapped type slot"),
            ),
            coordinate: StableProbeCoordinate::GlobalType {
                name: mapped.product.name.clone(),
            },
        })
        .collect();
    let native = NativeProductEvidence {
        groups: groups.clone(),
        mapped_groups,
        module_probe_sources: BTreeMap::new(),
        probe_evidence,
        dependencies: dependencies.clone(),
        containment_targets: BTreeMap::new(),
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

fn type_probe(name: &str) -> crate::check::checker::library_compiler::NormalizedTypeProbeForTest {
    crate::check::checker::library_compiler::NormalizedTypeProbeForTest {
        name: name.to_owned(),
        declaration_count: 1,
        member_names: Vec::new(),
    }
}

fn module_type_probe(source_path: &str, name: &str) -> ProjectLocalModuleTypeProbeForTest {
    ProjectLocalModuleTypeProbeForTest {
        source_path: source_path.to_owned(),
        probe: type_probe(name),
    }
}

fn value_probe(name: &str) -> ProjectLocalValueProbeForTest {
    ProjectLocalValueProbeForTest {
        name: name.to_owned(),
        declaration_count: 1,
        call_signature_count: 0,
        signatures: Vec::new(),
        member_names: Vec::new(),
        callable_members: Vec::new(),
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

fn candidate_result_with_product(
    product: LibraryDeclarationProduct,
    inputs: Vec<UserDeltaProjectInputForTest<'static>>,
    controls: ProjectLocalSemanticRequestForTest,
) -> (
    Result<ProjectLocalSemanticTraceForTest, ProjectLocalCandidateErrorForTest>,
    ObservationReceiptForTest,
) {
    let observation_scope = ObservationScopeForTest::start();
    let product = std::sync::Arc::new(product);
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
    (result, observation_scope.finish())
}

fn passive_ordinary_owner_keys(observation: &ObservationReceiptForTest) -> Vec<usize> {
    observation
        .events
        .iter()
        .filter_map(|event| match event.kind {
            ObservationEventKindForTest::OwnerMaterialized {
                phase: ProjectLocalSemanticPhaseForTest::Ordinary,
                product_group,
            } => Some(product_group),
            _ => None,
        })
        .collect()
}

fn passive_cache_event_rows(observation: &ObservationReceiptForTest) -> Vec<String> {
    observation
        .events
        .iter()
        .filter_map(|event| match event.kind {
            ObservationEventKindForTest::DurableCacheWrite {
                publisher: _,
                product_group,
            } => Some(format!("write:{product_group}")),
            ObservationEventKindForTest::DurableCacheHit {
                publisher: _,
                product_group,
            } => Some(format!("hit:{product_group}")),
            _ => None,
        })
        .collect()
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
fn frozen_oracle() -> Value {
    serde_json::from_slice(ORACLE_BYTES).expect("valid WU4 oracle")
}

#[derive(Clone, Copy, Debug)]
enum RecursiveAliasIndirection {
    Array,
    Tuple,
    Function,
}

const fn recursive_alias_name(kind: RecursiveAliasIndirection) -> &'static str {
    match kind {
        RecursiveAliasIndirection::Array => "Wu4RecursiveArray",
        RecursiveAliasIndirection::Tuple => "Wu4RecursiveTuple",
        RecursiveAliasIndirection::Function => "Wu4RecursiveFunction",
    }
}

fn synthetic_recursive_alias_product(kind: RecursiveAliasIndirection) -> LibraryDeclarationProduct {
    let mut assembly = ProductAssembly::new();
    let source = assembly.next_source_key().expect("synthetic source key");
    let root = assembly.next_region_key().expect("synthetic root region");
    let container = assembly
        .add_container(ContainerRecipe {
            parent: None,
            kind: ContainerKind::CompilationGlobal,
            name: None,
            anchor: None,
        })
        .expect("synthetic global container");
    let source_name = assembly
        .intern("wu4-recursive-alias.d.ts")
        .expect("synthetic source name");
    let digest = assembly.intern("00").expect("synthetic digest");
    assembly
        .add_source(SourceRecipe {
            ordinal: 0,
            name: source_name,
            sha256: digest,
            byte_len: 128,
            classification: SourceClassification::Script,
            root_region: root,
        })
        .expect("synthetic source");
    assembly
        .add_region(RegionRecipe {
            parent: None,
            container,
            kind: RegionKind::Source,
            site: SourceSite {
                source,
                start: 0,
                end: 128,
            },
        })
        .expect("synthetic root");
    let name = assembly
        .intern(recursive_alias_name(kind))
        .expect("recursive alias name");
    let group = assembly.next_group_key().expect("recursive alias group");
    let reference = assembly
        .add_type_recipe(TypeRecipe {
            site: SourceSite {
                source,
                start: 30,
                end: 40,
            },
            node: TypeRecipeNode::Reference {
                path: Box::new([name]),
                resolution: TypeReferenceResolution::Group(group),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner: group,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("recursive reference");
    let value = match kind {
        RecursiveAliasIndirection::Array => assembly
            .add_type_recipe(TypeRecipe {
                site: SourceSite {
                    source,
                    start: 25,
                    end: 42,
                },
                node: TypeRecipeNode::Array(reference),
            })
            .expect("recursive array"),
        RecursiveAliasIndirection::Tuple => assembly
            .add_type_recipe(TypeRecipe {
                site: SourceSite {
                    source,
                    start: 25,
                    end: 42,
                },
                node: TypeRecipeNode::Tuple {
                    elements: Box::new([TupleElementRecipe {
                        name: None,
                        value: reference,
                        optional: false,
                        rest: false,
                    }]),
                },
            })
            .expect("recursive tuple"),
        RecursiveAliasIndirection::Function => {
            let signature = assembly
                .add_signature(SignatureRecipe {
                    type_parameters: Box::new([]),
                    parameters: Box::new([]),
                    result: Some(reference),
                })
                .expect("recursive function signature");
            assembly
                .add_type_recipe(TypeRecipe {
                    site: SourceSite {
                        source,
                        start: 25,
                        end: 42,
                    },
                    node: TypeRecipeNode::Function(signature),
                })
                .expect("recursive function")
        }
    };
    let declaration = assembly
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 50,
            },
            binding_site: SourceSite {
                source,
                start: 6,
                end: 24,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::Region(root),
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            flags: DeclarationFlags {
                ambient: true,
                ..DeclarationFlags::default()
            },
            payload: DeclarationPayload::TypeAlias {
                type_parameters: Box::new([]),
                value,
            },
        })
        .expect("recursive alias declaration");
    let actual_group = assembly
        .add_group(DeclarationGroupRecipe {
            container,
            name,
            slots: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            fragments: Box::new([declaration]),
        })
        .expect("recursive alias group");
    assert_eq!(actual_group, group);
    assembly.add_dependency(DependencyRecipe {
        owner: group,
        target: DependencyTarget::Group(group),
        kind: DependencyKind::Reference,
        site: SourceSite {
            source,
            start: 30,
            end: 40,
        },
    });
    assembly
        .finish(format!("wu4-recursive-{kind:?}"))
        .expect("valid recursive alias product")
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
