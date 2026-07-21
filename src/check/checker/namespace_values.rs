//! Atomic value payloads for attached and standalone namespaces.

use super::assignment::declared_from_init;
use super::calls::{FunctionReservation, RetainedFunctionBodySurface};
use super::context::{
    ClassNamespacePropertyPayload, ClassNamespacePropertySourceOrder, FunctionSurface, Pass,
};
use super::events::UserRecordTicket;
use super::function_groups::FunctionNamespacePayload;
#[cfg(test)]
use super::lexical_events::SourceSite;
use super::lexical_events::{source_ordinal, LexicalOwnerPhase};
use super::source_ordinal_from_origin;
use super::statements::{function_decl_from_statement, function_overload_group};
use crate::binder::declaration::{DeclId, TypeGroupId, ValueStorageId};
use crate::binder::namespace::{
    DeclarationOwner, MergeDeclarationKind, NamespaceId, NamespacePublication,
    NamespaceValueAttachmentDisposition, StandaloneNamespaceValueAttachment,
};
use crate::binder::scope::ScopeId;
use crate::class_semantics::DemandOutcome;
use crate::diagnostics::Diagnostic;
use crate::source::SourceOrdinal;
use crate::span::Span;
use crate::types::repr::{ClassId, FunctionType, ObjectType, PropertyType};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    Class, ClassElement, Declaration, Expression, Function, Statement, TSModuleDeclaration,
    TSModuleDeclarationBody, VariableDeclaration, VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast::ast_kind::AstKind;
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};

pub(in crate::check::checker) struct NamespaceValueRegistry<Ticket: Copy = UserRecordTicket> {
    prepared: FxHashMap<(ScopeId, u32), PreparedNamespaceMember<Ticket>>,
    consumed_fragments: FxHashSet<(ScopeId, u32)>,
    fragment_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    private_fragments: FxHashSet<(ScopeId, u32)>,
    private_unsupported_members: FxHashSet<(ScopeId, u32)>,
    ambient_fragments: FxHashSet<(ScopeId, u32)>,
    prepared_owners: FxHashSet<(ScopeId, String)>,
    standalone_plans: FxHashMap<NamespaceId, StandaloneNamespacePlan<Ticket>>,
    standalone_terminals: FxHashMap<NamespaceId, StandaloneNamespaceTerminal>,
    #[cfg(test)]
    namespace_function_reservations: FxHashMap<DeclId, SourceSite>,
    #[cfg(test)]
    standalone_query_root_calls: u64,
}

#[derive(Clone, Default)]
pub(in crate::check::checker) struct FrozenNamespaceValueTerminals {
    standalone: FxHashMap<NamespaceId, StandaloneNamespaceTerminal>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(in crate::check::checker) enum NamespaceValueUnavailableCause {
    MissingExportedMemberName = 0,
    DuplicateExportedValue = 1,
    UnboundExportedVariable = 2,
    MissingExportedVariableSyntax = 3,
    InvalidUsingDeclaration = 4,
    VariableSurfaceUnavailable = 5,
    UnboundExportedFunction = 6,
    MissingExportedFunctionSyntax = 7,
    UnboundExportedClass = 8,
    UnboundExportedClassIdentity = 9,
    UnboundNestedNamespace = 10,
    UnsupportedExportedMember = 11,
    DeferredExportedMember = 12,
    FunctionNamespacePayloadUnavailable = 13,
    FunctionOwnerCallSurfaceUnavailable = 14,
    FunctionSurfaceUnavailable = 15,
    ClassSurfaceUnavailable = 16,
    NestedNamespaceUnavailable = 17,
    ExistingOwnerUnavailable = 18,
    NamespaceContainmentCycle = 19,
    InvalidPrivateNamespaceMember = 20,
    ClassValueSurfaceUnavailable = 21,
}

#[cfg(test)]
pub(in crate::check::checker) type FrozenNamespaceUnavailableCause = NamespaceValueUnavailableCause;

#[cfg(test)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum FrozenNamespaceValueTerminalSnapshot {
    Ready { storage: ValueStorageId, ty: TypeId },
    Unavailable(FrozenNamespaceUnavailableCause),
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct FrozenNamespaceValueTerminalSnapshotRow {
    pub(in crate::check::checker) namespace: NamespaceId,
    pub(in crate::check::checker) terminal: FrozenNamespaceValueTerminalSnapshot,
}

#[cfg(test)]
pub(in crate::check::checker) type FrozenNamespaceValueTerminalsSnapshotParts =
    Vec<FrozenNamespaceValueTerminalSnapshotRow>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum StandaloneNamespaceTerminal {
    Planned,
    Ready {
        storage: ValueStorageId,
        ty: TypeId,
    },
    Unavailable {
        cause: NamespaceValueUnavailableCause,
    },
}

struct StandaloneNamespacePlan<Ticket: Copy> {
    storage: ValueStorageId,
    properties: Vec<PropertyType>,
    dependencies: Vec<StandaloneNamespaceDependency<Ticket>>,
    unavailable: Option<NamespaceValueUnavailableCause>,
}

struct StandaloneNamespaceDependency<Ticket: Copy> {
    name: String,
    readonly: bool,
    kind: StandaloneNamespaceDependencyKind<Ticket>,
}

#[derive(Copy, Clone)]
struct AliasDependencyFailure<Ticket: Copy> {
    owner: Ticket,
    span: Span,
}

#[derive(Copy, Clone, PartialEq, Eq)]
struct LegalExistingOwnerMerge {
    kind: MergeDeclarationKind,
    storage: ValueStorageId,
}

enum StandaloneNamespaceDependencyKind<Ticket: Copy> {
    Class {
        class: ClassId,
        storage: ValueStorageId,
        declaration: DeclId,
        span: Span,
        static_root_cycle: bool,
    },
    Namespace {
        namespace: NamespaceId,
        alias_failure: Option<AliasDependencyFailure<Ticket>>,
    },
    ExistingValue {
        storage: ValueStorageId,
        alias_failure: Option<AliasDependencyFailure<Ticket>>,
    },
}

enum PreparedNamespaceMember<Ticket: Copy = UserRecordTicket> {
    Variable {
        scope: ScopeId,
        annotation: Option<TypeId>,
    },
    Function {
        scope: ScopeId,
        reservation: FunctionReservation<Ticket>,
    },
    Class {
        scope: ScopeId,
    },
}

struct AttachmentInput {
    owner_scope: ScopeId,
    name: String,
    class_group: Option<TypeGroupId>,
    disposition: NamespaceValueAttachmentDisposition,
    fragments: Vec<FragmentInput>,
    members: Vec<OwnedMemberInput>,
    private_members: Vec<PrivateMemberInput>,
    unavailable_members: Vec<UnavailableMemberInput>,
    has_unavailable_metadata: bool,
}

#[derive(Copy, Clone)]
struct FragmentInput {
    module: ScopeId,
    source_start: u32,
    private_scope: ScopeId,
    ambient: bool,
}

#[derive(Clone)]
struct OwnedMemberInput {
    declaration: DeclId,
    storage: ValueStorageId,
    scope: ScopeId,
    module: ScopeId,
    source: crate::binder::namespace::SourceUnitKey,
    source_ordinal: SourceOrdinal,
    source_unit: crate::source::SourceUnit,
    source_start: u32,
    span: Span,
    owner_span: Span,
    name: String,
    kind: PreparedNamespaceValueKind,
    publication: NamespacePublication,
    ambient: bool,
    readonly: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PreparedNamespaceValueKind {
    Variable,
    Function,
    Class,
}

#[derive(Copy, Clone)]
struct UnavailableMemberInput {
    declaration: DeclId,
    span: Span,
    kind: MergeDeclarationKind,
}

#[derive(Copy, Clone)]
struct PrivateMemberInput {
    declaration: DeclId,
    scope: ScopeId,
    module: ScopeId,
    source: crate::binder::namespace::SourceUnitKey,
    source_ordinal: SourceOrdinal,
    source_unit: crate::source::SourceUnit,
    source_start: u32,
    kind: PreparedNamespaceValueKind,
}

struct StagedVariable {
    input: OwnedMemberInput,
    ty: TypeId,
    annotation: Option<TypeId>,
}

struct StagedFunction<'stmt, 'ast, Ticket: Copy = UserRecordTicket> {
    input: OwnedMemberInput,
    syntax: &'stmt Function<'ast>,
    reservation: FunctionReservation<Ticket>,
}

#[derive(Default)]
struct NamespaceSyntaxIndex<'stmt, 'ast> {
    variables: FxHashMap<u32, (VariableDeclarationKind, &'stmt VariableDeclarator<'ast>)>,
    functions: FxHashMap<u32, &'stmt Function<'ast>>,
    classes: FxHashMap<u32, &'stmt Class<'ast>>,
}

#[derive(Default)]
struct ProjectSyntaxIndex<'stmt, 'ast> {
    variables:
        FxHashMap<(ScopeId, u32), (VariableDeclarationKind, &'stmt VariableDeclarator<'ast>)>,
    functions: FxHashMap<(ScopeId, u32), &'stmt Function<'ast>>,
    classes: FxHashMap<(ScopeId, u32), &'stmt Class<'ast>>,
}

fn project_syntax_index<'stmt, 'ast>(
    modules: &[(ScopeId, &'stmt [Statement<'ast>])],
) -> ProjectSyntaxIndex<'stmt, 'ast> {
    let mut syntax = ProjectSyntaxIndex::default();
    for (module, statements) in modules {
        let mut local = NamespaceSyntaxIndex::default();
        index_namespace_statements(statements, false, &mut local);
        syntax.variables.extend(
            local
                .variables
                .into_iter()
                .map(|(start, item)| ((*module, start), item)),
        );
        syntax.functions.extend(
            local
                .functions
                .into_iter()
                .map(|(start, item)| ((*module, start), item)),
        );
        syntax.classes.extend(
            local
                .classes
                .into_iter()
                .map(|(start, item)| ((*module, start), item)),
        );
    }
    syntax
}

struct StandaloneAttachmentInput {
    namespace: NamespaceId,
    storage: ValueStorageId,
    fragments: Vec<FragmentInput>,
    members: Vec<StandaloneMemberInput>,
}

#[derive(Clone)]
struct StandaloneMemberInput {
    declaration: Option<DeclId>,
    name: Option<String>,
    scope: Option<ScopeId>,
    module: ScopeId,
    source: crate::binder::namespace::SourceUnitKey,
    source_start: u32,
    span: Span,
    local_span: Option<Span>,
    source_ordinal: SourceOrdinal,
    value_storage: Option<ValueStorageId>,
    alias_target_storage: Option<ValueStorageId>,
    ambient: bool,
    child_namespace: Option<NamespaceId>,
    kind: MergeDeclarationKind,
    publication: NamespacePublication,
    has_value_space: bool,
}

impl<Ticket: Copy> Default for NamespaceValueRegistry<Ticket> {
    fn default() -> Self {
        Self {
            prepared: FxHashMap::default(),
            consumed_fragments: FxHashSet::default(),
            fragment_scopes: FxHashMap::default(),
            private_fragments: FxHashSet::default(),
            private_unsupported_members: FxHashSet::default(),
            ambient_fragments: FxHashSet::default(),
            prepared_owners: FxHashSet::default(),
            standalone_plans: FxHashMap::default(),
            standalone_terminals: FxHashMap::default(),
            #[cfg(test)]
            namespace_function_reservations: FxHashMap::default(),
            #[cfg(test)]
            standalone_query_root_calls: 0,
        }
    }
}

impl<Ticket: Copy> NamespaceValueRegistry<Ticket> {
    #[cfg(test)]
    pub(in crate::check::checker) fn freeze_terminals(&self) -> FrozenNamespaceValueTerminals {
        assert!(
            self.standalone_terminals
                .values()
                .all(|terminal| !matches!(terminal, StandaloneNamespaceTerminal::Planned)),
            "frozen namespace terminals are complete"
        );
        FrozenNamespaceValueTerminals {
            standalone: self.standalone_terminals.clone(),
        }
    }

    pub(in crate::check::checker) fn install_frozen_terminals(
        &mut self,
        frozen: FrozenNamespaceValueTerminals,
    ) {
        for (namespace, terminal) in frozen.standalone {
            assert!(
                self.standalone_terminals
                    .insert(namespace, terminal)
                    .is_none(),
                "frozen namespace terminals install into an empty prefix"
            );
        }
    }

    pub(in crate::check::checker) fn standalone_terminal(
        &self,
        namespace: NamespaceId,
    ) -> Option<StandaloneNamespaceTerminal> {
        self.standalone_terminals.get(&namespace).copied()
    }

    #[cfg(test)]
    fn standalone_query_root_calls(&self) -> u64 {
        self.standalone_query_root_calls
    }

    #[cfg(test)]
    fn record_namespace_function_reservation(&mut self, declaration: DeclId, source: SourceSite) {
        if let Some(previous) = self
            .namespace_function_reservations
            .insert(declaration, source)
        {
            assert_eq!(previous, source, "one exact callable reservation source");
        }
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn namespace_function_reservation(
        &self,
        declaration: DeclId,
    ) -> Option<SourceSite> {
        self.namespace_function_reservations
            .get(&declaration)
            .copied()
    }

    fn insert_standalone_plan(
        &mut self,
        namespace: NamespaceId,
        plan: StandaloneNamespacePlan<Ticket>,
    ) {
        assert!(
            self.standalone_plans.insert(namespace, plan).is_none(),
            "standalone namespace planned once"
        );
        assert!(
            self.standalone_terminals
                .insert(namespace, StandaloneNamespaceTerminal::Planned)
                .is_none(),
            "standalone namespace terminal reserved once"
        );
    }

    fn is_prepared_owner(&self, scope: ScopeId, name: &str) -> bool {
        self.prepared_owners.contains(&(scope, name.to_owned()))
    }

    fn mark_prepared_owner(&mut self, scope: ScopeId, name: String) {
        assert!(
            self.prepared_owners.insert((scope, name)),
            "attached namespace owner prepared twice"
        );
    }

    fn insert_member(
        &mut self,
        module: ScopeId,
        source_start: u32,
        member: PreparedNamespaceMember<Ticket>,
    ) {
        assert!(
            self.prepared
                .insert((module, source_start), member)
                .is_none(),
            "attached namespace member prepared twice"
        );
    }

    fn take_member(
        &mut self,
        module: ScopeId,
        source_start: u32,
    ) -> Option<PreparedNamespaceMember<Ticket>> {
        self.prepared.remove(&(module, source_start))
    }

    fn function_scope(&self, module: ScopeId, source_start: u32) -> Option<ScopeId> {
        match self.prepared.get(&(module, source_start)) {
            Some(PreparedNamespaceMember::Function { scope, .. }) => Some(*scope),
            Some(
                PreparedNamespaceMember::Variable { .. } | PreparedNamespaceMember::Class { .. },
            )
            | None => None,
        }
    }

    fn take_function(
        &mut self,
        module: ScopeId,
        source_start: u32,
    ) -> Option<(ScopeId, FunctionReservation<Ticket>)> {
        match self.prepared.remove(&(module, source_start)) {
            Some(PreparedNamespaceMember::Function { scope, reservation }) => {
                Some((scope, reservation))
            }
            Some(member @ PreparedNamespaceMember::Variable { .. }) => {
                self.prepared.insert((module, source_start), member);
                None
            }
            Some(member @ PreparedNamespaceMember::Class { .. }) => {
                self.prepared.insert((module, source_start), member);
                None
            }
            None => None,
        }
    }

    fn consume_fragment(&mut self, module: ScopeId, source_start: u32, private_scope: ScopeId) {
        assert!(
            self.consumed_fragments.insert((module, source_start)),
            "namespace fragment consumed exactly once"
        );
        assert!(
            self.fragment_scopes
                .insert((module, source_start), private_scope)
                .is_none(),
            "namespace fragment scope installed exactly once"
        );
    }

    fn is_consumed_fragment(&self, module: ScopeId, source_start: u32) -> bool {
        self.consumed_fragments.contains(&(module, source_start))
    }

    fn fragment_scope(&self, module: ScopeId, source_start: u32) -> Option<ScopeId> {
        self.fragment_scopes.get(&(module, source_start)).copied()
    }

    fn mark_private_fragment(&mut self, module: ScopeId, source_start: u32) {
        self.private_fragments.insert((module, source_start));
    }

    fn take_private_fragment(&mut self, module: ScopeId, source_start: u32) -> bool {
        self.private_fragments.remove(&(module, source_start))
    }

    fn mark_private_unsupported_member(&mut self, module: ScopeId, source_start: u32) {
        assert!(
            self.private_unsupported_members
                .insert((module, source_start)),
            "private unsupported namespace member prepared once"
        );
    }

    fn take_private_unsupported_member(&mut self, module: ScopeId, source_start: u32) -> bool {
        self.private_unsupported_members
            .remove(&(module, source_start))
    }

    fn mark_ambient_fragment(&mut self, module: ScopeId, source_start: u32) {
        self.ambient_fragments.insert((module, source_start));
    }

    fn is_ambient_fragment(&self, module: ScopeId, source_start: u32) -> bool {
        self.ambient_fragments.contains(&(module, source_start))
    }
}

#[cfg(test)]
impl FrozenNamespaceValueTerminals {
    pub(in crate::check::checker) fn snapshot_parts(
        &self,
    ) -> Result<FrozenNamespaceValueTerminalsSnapshotParts, &'static str> {
        let mut rows = self
            .standalone
            .iter()
            .map(|(&namespace, &terminal)| {
                let terminal = match terminal {
                    StandaloneNamespaceTerminal::Planned => {
                        return Err("snapshot namespace terminal is still planned")
                    }
                    StandaloneNamespaceTerminal::Ready { storage, ty } => {
                        FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty }
                    }
                    StandaloneNamespaceTerminal::Unavailable { cause } => {
                        FrozenNamespaceValueTerminalSnapshot::Unavailable(cause)
                    }
                };
                Ok(FrozenNamespaceValueTerminalSnapshotRow {
                    namespace,
                    terminal,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        rows.sort_by_key(|row| row.namespace.0);
        Ok(rows)
    }

    pub(in crate::check::checker) fn from_snapshot_parts(
        rows: FrozenNamespaceValueTerminalsSnapshotParts,
    ) -> Result<Self, &'static str> {
        if rows
            .windows(2)
            .any(|pair| pair[0].namespace.0 >= pair[1].namespace.0)
        {
            return Err("snapshot namespace terminals are not strictly ordered");
        }
        let standalone = rows
            .into_iter()
            .map(|row| {
                let terminal = match row.terminal {
                    FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } => {
                        StandaloneNamespaceTerminal::Ready { storage, ty }
                    }
                    FrozenNamespaceValueTerminalSnapshot::Unavailable(cause) => {
                        StandaloneNamespaceTerminal::Unavailable { cause }
                    }
                };
                (row.namespace, terminal)
            })
            .collect();
        Ok(Self { standalone })
    }
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    /// Prepare one module's attached namespace values before class/callable publication.
    pub(in crate::check::checker) fn prepare_attached_namespace_values(
        &mut self,
        scope: ScopeId,
        statements: &'ast [Statement<'ast>],
    ) {
        self.prepare_project_attached_namespace_values(&[(scope, statements)]);
    }

    pub(in crate::check::checker) fn prepare_project_attached_namespace_values(
        &mut self,
        modules: &[(ScopeId, &'ast [Statement<'ast>])],
    ) {
        let syntax = project_syntax_index(modules);
        let module_scopes: FxHashSet<ScopeId> = modules.iter().map(|(module, _)| *module).collect();
        for attachment in collect_attachment_inputs(self, &module_scopes) {
            if self
                .namespace_values
                .is_prepared_owner(attachment.owner_scope, &attachment.name)
            {
                continue;
            }
            let owner_scope = attachment.owner_scope;
            let name = attachment.name.clone();
            self.prepare_namespace_attachment(attachment, &syntax);
            self.namespace_values.mark_prepared_owner(owner_scope, name);
        }
    }

    /// Stage every standalone group in this module without publishing its root slot.
    pub(in crate::check::checker) fn prepare_standalone_namespace_values(
        &mut self,
        module: ScopeId,
        statements: &'ast [Statement<'ast>],
    ) {
        self.prepare_project_standalone_namespace_values(&[(module, statements)]);
    }

    pub(in crate::check::checker) fn prepare_project_standalone_namespace_values(
        &mut self,
        modules: &[(ScopeId, &'ast [Statement<'ast>])],
    ) {
        #[cfg(test)]
        let query_roots_before = crate::check::query::query_demand_measure().root_calls;
        let syntax = project_syntax_index(modules);
        for attachment in collect_all_standalone_attachment_inputs(self) {
            if self
                .namespace_values
                .standalone_terminal(attachment.namespace)
                .is_some()
            {
                continue;
            }
            self.prepare_standalone_namespace_attachment(attachment, &syntax);
        }
        #[cfg(test)]
        {
            self.namespace_values.standalone_query_root_calls +=
                crate::check::query::query_demand_measure().root_calls - query_roots_before;
        }
    }

    fn prepare_standalone_namespace_attachment(
        &mut self,
        attachment: StandaloneAttachmentInput,
        syntax: &ProjectSyntaxIndex<'_, '_>,
    ) {
        for fragment in &attachment.fragments {
            if fragment.ambient {
                self.namespace_values
                    .mark_ambient_fragment(fragment.module, fragment.source_start);
            }
            self.namespace_values.consume_fragment(
                fragment.module,
                fragment.source_start,
                fragment.private_scope,
            );
        }

        let private_invalid = self.prepare_standalone_private_members(&attachment, syntax);

        let public_members = attachment
            .members
            .iter()
            .filter(|member| {
                standalone_member_participates(self, member)
                    && !matches!(member.publication, NamespacePublication::Private)
            })
            .cloned()
            .collect::<Vec<_>>();
        let legal_existing_merges = legal_existing_owner_merges(&public_members);

        let mut first_members: FxHashMap<
            String,
            (
                MergeDeclarationKind,
                Option<ValueStorageId>,
                Option<NamespaceId>,
            ),
        > = FxHashMap::default();
        let mut unavailable = private_invalid
            .then_some(NamespaceValueUnavailableCause::InvalidPrivateNamespaceMember);
        for member in &public_members {
            let Some(name) = member.name.as_ref() else {
                unavailable = Some(NamespaceValueUnavailableCause::MissingExportedMemberName);
                continue;
            };
            let facts = (member.kind, member.value_storage, member.child_namespace);
            if let Some(first) = first_members.insert(name.clone(), facts) {
                let repeated_overload = first.0 == MergeDeclarationKind::Function
                    && member.kind == MergeDeclarationKind::Function
                    && first.1.is_some()
                    && first.1 == member.value_storage;
                let repeated_namespace = first.0 == MergeDeclarationKind::Namespace
                    && member.kind == MergeDeclarationKind::Namespace
                    && first.1 == member.value_storage
                    && first.2 == member.child_namespace;
                if !(legal_existing_merges.contains_key(name)
                    || repeated_overload
                    || repeated_namespace)
                {
                    if let (Some(declaration), Some(kind)) = (
                        member.declaration,
                        prepared_namespace_value_kind(member.kind),
                    ) {
                        let (id, context) = namespace_payload_duplicate(kind);
                        self.record_namespace_attachment_unavailable(
                            declaration,
                            member.span,
                            id,
                            context,
                        );
                    }
                    unavailable = Some(NamespaceValueUnavailableCause::DuplicateExportedValue);
                }
            }
        }

        let mut variables = Vec::new();
        let mut functions = Vec::new();
        let mut properties = Vec::new();
        let mut dependencies = Vec::new();
        for member in &public_members {
            self.select_standalone_member_source(member);
            let Some(name) = member.name.clone() else {
                continue;
            };
            match member.kind {
                MergeDeclarationKind::Variable => {
                    let (Some(declaration), Some(scope), Some(storage)) =
                        (member.declaration, member.scope, member.value_storage)
                    else {
                        unavailable = Some(NamespaceValueUnavailableCause::UnboundExportedVariable);
                        continue;
                    };
                    let Some((kind, declarator)) = syntax
                        .variables
                        .get(&(member.module, member.source_start))
                        .copied()
                    else {
                        unavailable =
                            Some(NamespaceValueUnavailableCause::MissingExportedVariableSyntax);
                        continue;
                    };
                    let using_diagnostic = namespace_using_diagnostic(
                        kind,
                        member.publication,
                        member.ambient,
                        member.span,
                    );
                    let invalid_using =
                        invalid_namespace_using(kind, member.publication, member.ambient);
                    if let Some(diagnostic) = using_diagnostic {
                        self.record_namespace_attachment_diagnostic(declaration, diagnostic);
                        unavailable = Some(NamespaceValueUnavailableCause::InvalidUsingDeclaration);
                    }
                    let annotation = match &declarator.type_annotation {
                        Some(annotation) => self.lower_namespace_member_annotation(
                            declaration,
                            scope,
                            &annotation.type_annotation,
                        ),
                        None => None,
                    };
                    let ty = match (&declarator.type_annotation, annotation) {
                        (Some(_), Some(annotation)) => Some(annotation),
                        (Some(_), None) => None,
                        (None, _) => declarator.init.as_ref().and_then(|initializer| {
                            query_free_initializer_type(self, kind, initializer)
                        }),
                    };
                    let Some(ty) = ty else {
                        if declarator.type_annotation.is_none() {
                            self.record_namespace_attachment_unavailable(
                                declaration,
                                member.span,
                                "decl/variable-declaration/namespace-payload-inferred-initializer",
                                "namespace member initializer cannot be finalized before root publication",
                            );
                        }
                        unavailable =
                            Some(NamespaceValueUnavailableCause::VariableSurfaceUnavailable);
                        continue;
                    };
                    if !invalid_using {
                        let mut property = PropertyType::public(name, ty);
                        property.readonly = kind.is_const();
                        properties.push(property);
                    }
                    variables.push((
                        storage,
                        member.module,
                        member.source_start,
                        scope,
                        annotation,
                        ty,
                    ));
                }
                MergeDeclarationKind::Function => {
                    let (Some(scope), Some(storage)) = (member.scope, member.value_storage) else {
                        unavailable = Some(NamespaceValueUnavailableCause::UnboundExportedFunction);
                        continue;
                    };
                    let Some(function) = syntax
                        .functions
                        .get(&(member.module, member.source_start))
                        .copied()
                    else {
                        unavailable =
                            Some(NamespaceValueUnavailableCause::MissingExportedFunctionSyntax);
                        continue;
                    };
                    let reservation =
                        self.reserve_namespace_function(member.declaration, scope, function);
                    functions.push(StagedFunction {
                        input: OwnedMemberInput {
                            declaration: member.declaration.expect("function declaration"),
                            storage,
                            scope,
                            module: member.module,
                            source: member.source,
                            source_ordinal: member.source_ordinal,
                            source_unit: self.current_source,
                            source_start: member.source_start,
                            span: member.span,
                            owner_span: member.span,
                            name,
                            kind: PreparedNamespaceValueKind::Function,
                            publication: member.publication,
                            ambient: member.ambient,
                            readonly: false,
                        },
                        syntax: function,
                        reservation,
                    });
                }
                MergeDeclarationKind::Class => {
                    let (Some(declaration), Some(storage), Some(scope)) =
                        (member.declaration, member.value_storage, member.scope)
                    else {
                        unavailable = Some(NamespaceValueUnavailableCause::UnboundExportedClass);
                        continue;
                    };
                    let Some(class) = self
                        .lexical_events
                        .classes()
                        .iter()
                        .filter_map(|reservation| reservation.binding.as_ref())
                        .find(|binding| binding.value_decl == Some(storage))
                        .map(|binding| binding.class_id)
                    else {
                        unavailable =
                            Some(NamespaceValueUnavailableCause::UnboundExportedClassIdentity);
                        continue;
                    };
                    let static_root_cycle = syntax
                        .classes
                        .get(&(member.module, member.source_start))
                        .is_some_and(|class| {
                            class_has_static_root_reference(
                                self.binder,
                                member.module,
                                scope,
                                class,
                                attachment.storage,
                            )
                        });
                    dependencies.push(StandaloneNamespaceDependency {
                        name,
                        readonly: false,
                        kind: StandaloneNamespaceDependencyKind::Class {
                            class,
                            storage,
                            declaration,
                            span: member.span,
                            static_root_cycle,
                        },
                    });
                }
                MergeDeclarationKind::Namespace => {
                    if legal_existing_merges.contains_key(&name) {
                        continue;
                    }
                    let Some(child) = member.child_namespace else {
                        unavailable = Some(NamespaceValueUnavailableCause::UnboundNestedNamespace);
                        continue;
                    };
                    if self
                        .binder
                        .namespaces
                        .standalone_value_storage(child)
                        .is_some()
                    {
                        let repeated = dependencies.iter().any(|dependency| {
                            dependency.name == name
                                && matches!(
                                    dependency.kind,
                                    StandaloneNamespaceDependencyKind::Namespace {
                                        namespace,
                                        ..
                                    }
                                        if namespace == child
                                )
                        });
                        if !repeated {
                            dependencies.push(StandaloneNamespaceDependency {
                                name,
                                readonly: false,
                                kind: StandaloneNamespaceDependencyKind::Namespace {
                                    namespace: child,
                                    alias_failure: None,
                                },
                            });
                        }
                    } else if let Some(storage) = member.value_storage {
                        let repeated = dependencies.iter().any(|dependency| {
                            dependency.name == name
                                && matches!(
                                    dependency.kind,
                                    StandaloneNamespaceDependencyKind::ExistingValue {
                                        storage: existing,
                                        ..
                                    } if existing == storage
                                )
                        });
                        if !repeated {
                            dependencies.push(StandaloneNamespaceDependency {
                                name,
                                readonly: false,
                                kind: StandaloneNamespaceDependencyKind::ExistingValue {
                                    storage,
                                    alias_failure: None,
                                },
                            });
                        }
                    }
                }
                MergeDeclarationKind::Enum | MergeDeclarationKind::ImportAlias => {
                    if let (Some(declaration), Some((id, context))) = (
                        member.declaration,
                        namespace_payload_unavailable(member.kind),
                    ) {
                        self.record_namespace_attachment_unavailable(
                            declaration,
                            member.span,
                            id,
                            context,
                        );
                    }
                    unavailable = Some(NamespaceValueUnavailableCause::UnsupportedExportedMember);
                }
                MergeDeclarationKind::DeferredExport => {
                    let local_span = member
                        .local_span
                        .expect("namespace export alias retains its local span");
                    let alias_failure = AliasDependencyFailure {
                        owner: self
                            .lexical_events
                            .export_alias_owner(member.source_ordinal, local_span)
                            .expect("namespace export alias has one exact lexical owner")
                            .ticket,
                        span: local_span,
                    };
                    if let Some(storage) = member.alias_target_storage {
                        let kind = match self.binder.standalone_namespace_for_storage(storage) {
                            Some(namespace) => StandaloneNamespaceDependencyKind::Namespace {
                                namespace,
                                alias_failure: Some(alias_failure),
                            },
                            None => StandaloneNamespaceDependencyKind::ExistingValue {
                                storage,
                                alias_failure: Some(alias_failure),
                            },
                        };
                        dependencies.push(StandaloneNamespaceDependency {
                            name,
                            // Export aliases are writable in tsc even when their target is const.
                            readonly: false,
                            kind,
                        });
                    } else {
                        self.with_ticket_effects(alias_failure.owner, |pass| {
                            pass.record_incomplete(
                                "decl/export-specifier/namespace-payload-unavailable",
                                alias_failure.span,
                                "namespace export alias target is unavailable",
                            );
                        });
                        unavailable = Some(NamespaceValueUnavailableCause::DeferredExportedMember);
                    }
                }
                MergeDeclarationKind::TypeAlias | MergeDeclarationKind::Interface => {}
            }
        }

        let function_properties = self.stage_namespace_function_properties(&functions);
        if let Some(mut function_properties) = function_properties {
            for property in &mut function_properties {
                let Some(merge) = legal_existing_merges.get(&property.name) else {
                    continue;
                };
                if merge.kind != MergeDeclarationKind::Function {
                    continue;
                }
                let Some(payload) = self
                    .function_groups
                    .namespace_payload_for_value(merge.storage)
                else {
                    unavailable =
                        Some(NamespaceValueUnavailableCause::FunctionNamespacePayloadUnavailable);
                    continue;
                };
                let call_signatures = match self.interner.store().tag(property.ty) {
                    crate::types::repr::TypeTag::Function => vec![property.ty],
                    crate::types::repr::TypeTag::Object => self
                        .interner
                        .store()
                        .object_type(property.ty)
                        .map(|object| object.call_signatures.clone())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                if call_signatures.is_empty() {
                    unavailable =
                        Some(NamespaceValueUnavailableCause::FunctionOwnerCallSurfaceUnavailable);
                    continue;
                }
                property.ty = self.interner.intern_object(ObjectType {
                    properties: payload.to_vec(),
                    call_signatures,
                    ..Default::default()
                });
            }
            properties.extend(function_properties);
        } else {
            unavailable = Some(NamespaceValueUnavailableCause::FunctionSurfaceUnavailable);
        }
        for (storage, module, source_start, scope, annotation, ty) in variables {
            if self.decl_types.get(storage).is_none() {
                self.decl_types.set(storage, ty);
            }
            self.namespace_values.insert_member(
                module,
                source_start,
                PreparedNamespaceMember::Variable { scope, annotation },
            );
        }
        for function in functions {
            let property_ty = properties
                .iter()
                .find(|property| property.name == function.input.name)
                .map(|property| property.ty);
            if let Some(property_ty) = property_ty {
                if self.decl_types.get(function.input.storage).is_none() {
                    self.decl_types.set(function.input.storage, property_ty);
                }
            }
            self.namespace_values.insert_member(
                self.binder
                    .declarations
                    .get(function.input.declaration)
                    .map_or(self.current_module, |declaration| declaration.site.module),
                function.input.source_start,
                PreparedNamespaceMember::Function {
                    scope: function.input.scope,
                    reservation: function.reservation,
                },
            );
        }
        for dependency in &dependencies {
            if let StandaloneNamespaceDependencyKind::Class { declaration, .. } = &dependency.kind {
                if let Some(site) = self
                    .binder
                    .declarations
                    .get(*declaration)
                    .map(|row| row.site)
                {
                    self.namespace_values.insert_member(
                        site.module,
                        site.declaration_span.start,
                        PreparedNamespaceMember::Class {
                            scope: site.scope.expect("namespace class scope"),
                        },
                    );
                }
            }
        }
        self.namespace_values.insert_standalone_plan(
            attachment.namespace,
            StandaloneNamespacePlan {
                storage: attachment.storage,
                properties,
                dependencies,
                unavailable,
            },
        );
    }

    fn select_standalone_member_source(&mut self, member: &StandaloneMemberInput) {
        self.current_module = member.module;
        if let Some(source) = member
            .declaration
            .and_then(|declaration| self.lexical_events.declaration_source(declaration))
        {
            self.current_source = source.unit;
        }
    }

    fn prepare_standalone_private_members(
        &mut self,
        attachment: &StandaloneAttachmentInput,
        syntax: &ProjectSyntaxIndex<'_, '_>,
    ) -> bool {
        let private = attachment
            .members
            .iter()
            .filter(|member| {
                member.has_value_space
                    && matches!(member.publication, NamespacePublication::Private)
            })
            .collect::<Vec<_>>();
        if private.is_empty() {
            return false;
        }
        let mut invalid = false;
        for fragment in &attachment.fragments {
            self.namespace_values
                .mark_private_fragment(fragment.module, fragment.source_start);
        }
        for member in private {
            self.select_standalone_member_source(member);
            let Some(scope) = member.scope else {
                continue;
            };
            match member.kind {
                MergeDeclarationKind::Variable => {
                    let Some((kind, declarator)) = syntax
                        .variables
                        .get(&(member.module, member.source_start))
                        .copied()
                    else {
                        continue;
                    };
                    if let Some(declaration) = member.declaration {
                        if let Some(diagnostic) = namespace_using_diagnostic(
                            kind,
                            member.publication,
                            member.ambient,
                            member.span,
                        ) {
                            self.record_namespace_attachment_diagnostic(declaration, diagnostic);
                            invalid = true;
                        }
                    }
                    let annotation = member.declaration.and_then(|declaration| {
                        declarator.type_annotation.as_ref().and_then(|annotation| {
                            self.lower_namespace_member_annotation(
                                declaration,
                                scope,
                                &annotation.type_annotation,
                            )
                        })
                    });
                    let ty = annotation.or_else(|| {
                        declarator.init.as_ref().and_then(|initializer| {
                            query_free_initializer_type(self, kind, initializer)
                        })
                    });
                    if let (Some(storage), Some(ty)) = (member.value_storage, ty) {
                        if self.decl_types.get(storage).is_none() {
                            self.decl_types.set(storage, ty);
                        }
                    }
                    self.namespace_values.insert_member(
                        member.module,
                        member.source_start,
                        PreparedNamespaceMember::Variable { scope, annotation },
                    );
                }
                MergeDeclarationKind::Function => {
                    let Some(function) = syntax
                        .functions
                        .get(&(member.module, member.source_start))
                        .copied()
                    else {
                        continue;
                    };
                    let reservation =
                        self.reserve_namespace_function(member.declaration, scope, function);
                    if let (Some(storage), FunctionReservation::Ready(surface)) =
                        (member.value_storage, &reservation)
                    {
                        if (function.body.is_none() || surface.declared_return.is_some())
                            && self.decl_types.get(storage).is_none()
                        {
                            self.decl_types.set(storage, surface.function_ty);
                        }
                    }
                    self.namespace_values.insert_member(
                        member.module,
                        member.source_start,
                        PreparedNamespaceMember::Function { scope, reservation },
                    );
                }
                MergeDeclarationKind::Class
                    if syntax
                        .classes
                        .contains_key(&(member.module, member.source_start)) =>
                {
                    self.namespace_values.insert_member(
                        member.module,
                        member.source_start,
                        PreparedNamespaceMember::Class { scope },
                    );
                }
                MergeDeclarationKind::Enum | MergeDeclarationKind::ImportAlias => {
                    self.namespace_values
                        .mark_private_unsupported_member(member.module, member.source_start);
                }
                _ => {}
            }
        }
        invalid
    }

    /// Resolve class/nested dependencies and publish each ready root exactly once.
    pub(in crate::check::checker) fn finalize_standalone_namespace_values(&mut self) {
        #[cfg(test)]
        let query_roots_before = crate::check::query::query_demand_measure().root_calls;
        let order = self
            .binder
            .standalone_namespace_value_attachments()
            .into_iter()
            .map(|attachment| attachment.namespace)
            .collect::<Vec<_>>();
        let mut remaining = order;
        while !remaining.is_empty() {
            let mut next = Vec::new();
            let mut progressed = false;
            for namespace in remaining {
                let Some(plan) = self.namespace_values.standalone_plans.get(&namespace) else {
                    continue;
                };
                if let Some(cause) = plan.unavailable {
                    self.namespace_values.standalone_terminals.insert(
                        namespace,
                        StandaloneNamespaceTerminal::Unavailable { cause },
                    );
                    progressed = true;
                    continue;
                }
                let mut properties = Vec::new();
                let mut waiting = false;
                let mut unavailable = None;
                let mut static_cycles = Vec::new();
                let mut alias_failures: Vec<AliasDependencyFailure<Ticket>> = Vec::new();
                for dependency in &plan.dependencies {
                    let ty = match &dependency.kind {
                        StandaloneNamespaceDependencyKind::Class {
                            class,
                            storage,
                            declaration,
                            span,
                            static_root_cycle,
                        } => {
                            if *static_root_cycle {
                                static_cycles.push((*declaration, *span));
                                unavailable =
                                    Some(NamespaceValueUnavailableCause::ClassSurfaceUnavailable);
                                None
                            } else {
                                match self
                                    .staged_published_classes
                                    .as_ref()
                                    .expect("class publication precedes namespace finalization")
                                    .published_class(*class)
                                {
                                    DemandOutcome::Ready(_) => {
                                        match self.decl_types.get(*storage) {
                                            Some(ty) => Some(ty),
                                            None => {
                                                unavailable = Some(
                                                    NamespaceValueUnavailableCause::ClassValueSurfaceUnavailable,
                                                );
                                                None
                                            }
                                        }
                                    }
                                    DemandOutcome::Exhausted(_) => {
                                        unavailable = Some(
                                            NamespaceValueUnavailableCause::ClassSurfaceUnavailable,
                                        );
                                        None
                                    }
                                }
                            }
                        }
                        StandaloneNamespaceDependencyKind::Namespace {
                            namespace: child,
                            alias_failure,
                        } => match self.namespace_values.standalone_terminal(*child) {
                            Some(StandaloneNamespaceTerminal::Ready { ty, .. }) => Some(ty),
                            Some(StandaloneNamespaceTerminal::Unavailable { .. }) => {
                                if let Some(failure) = alias_failure {
                                    alias_failures.push(*failure);
                                }
                                unavailable = Some(
                                    NamespaceValueUnavailableCause::NestedNamespaceUnavailable,
                                );
                                None
                            }
                            Some(StandaloneNamespaceTerminal::Planned) | None => {
                                waiting = true;
                                None
                            }
                        },
                        StandaloneNamespaceDependencyKind::ExistingValue {
                            storage,
                            alias_failure,
                        } => match self.decl_types.get(*storage) {
                            Some(ty) => Some(ty),
                            None => {
                                if let Some(failure) = alias_failure {
                                    alias_failures.push(*failure);
                                }
                                unavailable =
                                    Some(NamespaceValueUnavailableCause::ExistingOwnerUnavailable);
                                None
                            }
                        },
                    };
                    if let Some(ty) = ty {
                        let mut property = PropertyType::public(dependency.name.clone(), ty);
                        property.readonly = dependency.readonly;
                        properties.push(property);
                    }
                }
                for (declaration, span) in static_cycles {
                    self.record_namespace_attachment_unavailable(
                        declaration,
                        span,
                        "decl/class-declaration/namespace-payload-static-cycle",
                        "namespace class static surface depends on its root",
                    );
                }
                for failure in alias_failures {
                    self.with_ticket_effects(failure.owner, |pass| {
                        pass.record_incomplete(
                            "decl/export-specifier/namespace-payload-unavailable",
                            failure.span,
                            "namespace export alias target is unavailable",
                        );
                    });
                }
                if let Some(cause) = unavailable {
                    self.namespace_values.standalone_terminals.insert(
                        namespace,
                        StandaloneNamespaceTerminal::Unavailable { cause },
                    );
                    progressed = true;
                    continue;
                }
                if waiting {
                    next.push(namespace);
                    continue;
                }
                let plan = self
                    .namespace_values
                    .standalone_plans
                    .remove(&namespace)
                    .expect("ready namespace plan exists");
                let mut all_properties = plan.properties;
                all_properties.extend(properties);
                let ty = self.interner.intern_object(ObjectType {
                    properties: all_properties,
                    ..Default::default()
                });
                assert!(self.decl_types.get(plan.storage).is_none());
                self.decl_types.set(plan.storage, ty);
                self.namespace_values.standalone_terminals.insert(
                    namespace,
                    StandaloneNamespaceTerminal::Ready {
                        storage: plan.storage,
                        ty,
                    },
                );
                progressed = true;
            }
            if !progressed {
                for namespace in next.drain(..) {
                    self.namespace_values.standalone_terminals.insert(
                        namespace,
                        StandaloneNamespaceTerminal::Unavailable {
                            cause: NamespaceValueUnavailableCause::NamespaceContainmentCycle,
                        },
                    );
                }
                break;
            }
            remaining = next;
        }
        #[cfg(test)]
        {
            self.namespace_values.standalone_query_root_calls +=
                crate::check::query::query_demand_measure().root_calls - query_roots_before;
        }
    }

    fn prepare_namespace_attachment(
        &mut self,
        attachment: AttachmentInput,
        syntax: &ProjectSyntaxIndex<'_, '_>,
    ) {
        for fragment in &attachment.fragments {
            if fragment.ambient {
                self.namespace_values
                    .mark_ambient_fragment(fragment.module, fragment.source_start);
            }
            self.namespace_values.consume_fragment(
                fragment.module,
                fragment.source_start,
                fragment.private_scope,
            );
        }
        let private_invalid = self.prepare_private_namespace_members(&attachment, syntax);
        let mut unavailable = private_invalid
            || !attachment.unavailable_members.is_empty()
            || attachment.has_unavailable_metadata;
        if unavailable {
            for member in &attachment.unavailable_members {
                if let Some((id, context)) = namespace_payload_unavailable(member.kind) {
                    self.record_namespace_attachment_unavailable(
                        member.declaration,
                        member.span,
                        id,
                        context,
                    );
                }
            }
        }

        let mut first_kinds = FxHashMap::default();
        let mut has_duplicates = false;
        for member in &attachment.members {
            let Some(first_kind) = first_kinds.get(&member.name).copied() else {
                first_kinds.insert(member.name.clone(), member.kind);
                continue;
            };
            let Some(duplicate) = duplicate_property_kind(first_kind, member.kind) else {
                continue;
            };
            let (id, context) = namespace_payload_duplicate(duplicate);
            self.record_namespace_member_unavailable(member, id, context);
            has_duplicates = true;
        }
        if has_duplicates {
            unavailable = true;
        }

        let mut variables = Vec::new();
        let mut functions = Vec::new();
        for member in &attachment.members {
            self.select_attached_member_source(member);
            match member.kind {
                PreparedNamespaceValueKind::Variable => {
                    let Some((kind, declarator)) = syntax
                        .variables
                        .get(&(member.module, member.source_start))
                        .copied()
                    else {
                        // The binder and syntax index share exact declaration starts.
                        unavailable = true;
                        continue;
                    };
                    if let Some(diagnostic) = namespace_using_diagnostic(
                        kind,
                        member.publication,
                        member.ambient,
                        member.span,
                    ) {
                        self.record_namespace_attachment_diagnostic(member.declaration, diagnostic);
                        unavailable = true;
                    }
                    if invalid_namespace_using(kind, member.publication, member.ambient) {
                        unavailable = true;
                    }
                    let annotation = match &declarator.type_annotation {
                        Some(annotation) => self.lower_namespace_member_annotation(
                            member.declaration,
                            member.scope,
                            &annotation.type_annotation,
                        ),
                        None => None,
                    };
                    let ty = match (&declarator.type_annotation, annotation) {
                        (Some(_), Some(annotation)) => Some(annotation),
                        (Some(_), None) => {
                            unavailable = true;
                            continue;
                        }
                        (None, _) => declarator.init.as_ref().and_then(|initializer| {
                            query_free_initializer_type(self, kind, initializer)
                        }),
                    };
                    let Some(ty) = ty else {
                        self.record_namespace_member_unavailable(
                            member,
                            "decl/variable-declaration/namespace-payload-inferred-initializer",
                            "namespace member initializer cannot be finalized before owner publication",
                        );
                        unavailable = true;
                        continue;
                    };
                    variables.push(StagedVariable {
                        input: member.clone(),
                        ty,
                        annotation,
                    });
                }
                PreparedNamespaceValueKind::Function => {
                    let Some(function) = syntax
                        .functions
                        .get(&(member.module, member.source_start))
                        .copied()
                    else {
                        // The binder and syntax index share exact declaration starts.
                        unavailable = true;
                        continue;
                    };
                    let reservation = self.reserve_namespace_function(
                        Some(member.declaration),
                        member.scope,
                        function,
                    );
                    functions.push(StagedFunction {
                        input: member.clone(),
                        syntax: function,
                        reservation,
                    });
                }
                PreparedNamespaceValueKind::Class => {
                    let span = syntax
                        .classes
                        .get(&(member.module, member.source_start))
                        .map_or(member_span(member), |class| Span::from_oxc(class.span));
                    if !has_duplicates {
                        self.record_namespace_attachment_unavailable(
                            member.declaration,
                            span,
                            "decl/class-declaration/namespace-payload-static-cycle",
                            "attached namespace class value depends on class publication",
                        );
                    }
                    unavailable = true;
                    if syntax
                        .classes
                        .contains_key(&(member.module, member.source_start))
                    {
                        self.namespace_values.insert_member(
                            member.module,
                            member.source_start,
                            PreparedNamespaceMember::Class {
                                scope: member.scope,
                            },
                        );
                    }
                }
            }
        }

        let mut properties = variables
            .iter()
            .map(|variable| {
                let mut property = PropertyType::public(variable.input.name.clone(), variable.ty);
                property.readonly = variable.input.readonly;
                property
            })
            .collect::<Vec<_>>();
        let function_properties = self.stage_namespace_function_properties(&functions);
        match function_properties {
            Some(function_properties) => properties.extend(function_properties),
            None => unavailable = true,
        }

        for variable in variables {
            if self.decl_types.get(variable.input.storage).is_none() {
                self.decl_types.set(variable.input.storage, variable.ty);
            }
            self.namespace_values.insert_member(
                variable.input.module,
                variable.input.source_start,
                PreparedNamespaceMember::Variable {
                    scope: variable.input.scope,
                    annotation: variable.annotation,
                },
            );
        }
        for function in functions {
            let property_ty = properties
                .iter()
                .find(|property| property.name == function.input.name)
                .map(|property| property.ty);
            if let Some(property_ty) = property_ty {
                if self.decl_types.get(function.input.storage).is_none() {
                    self.decl_types.set(function.input.storage, property_ty);
                }
            }
            self.namespace_values.insert_member(
                function.input.module,
                function.input.source_start,
                PreparedNamespaceMember::Function {
                    scope: function.input.scope,
                    reservation: function.reservation,
                },
            );
        }
        if unavailable {
            self.install_unavailable_function_payload(&attachment);
            return;
        }
        match attachment.disposition {
            NamespaceValueAttachmentDisposition::AdmittedFunction => {
                let installed = self.install_function_namespace_payload(
                    attachment.owner_scope,
                    &attachment.name,
                    FunctionNamespacePayload::Ready(properties.clone()),
                );
                assert!(installed, "admitted function attachment must install");
            }
            NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42 => {
                let installed = self.install_function_namespace_payload(
                    attachment.owner_scope,
                    &attachment.name,
                    FunctionNamespacePayload::Ready(properties.clone()),
                );
                assert!(installed, "deferred function recovery must install");
            }
            NamespaceValueAttachmentDisposition::AdmittedClass => {
                let group = attachment
                    .class_group
                    .expect("admitted class attachment has a type group");
                let payload = self.class_namespace_property_payload(&attachment, properties);
                assert!(
                    self.install_class_namespace_payload(group, payload),
                    "class namespace payload installed twice"
                );
            }
            NamespaceValueAttachmentDisposition::TypeContainerOnly
            | NamespaceValueAttachmentDisposition::Rejected(_) => {
                unreachable!("collector retains only admitted value attachments")
            }
        }
    }

    fn prepare_private_namespace_members(
        &mut self,
        attachment: &AttachmentInput,
        syntax: &ProjectSyntaxIndex<'_, '_>,
    ) -> bool {
        let mut invalid = false;
        if !attachment.private_members.is_empty() {
            for fragment in &attachment.fragments {
                self.namespace_values
                    .mark_private_fragment(fragment.module, fragment.source_start);
            }
        }
        for member in &attachment.private_members {
            self.select_private_attached_member_source(member);
            match member.kind {
                PreparedNamespaceValueKind::Variable => {
                    let Some((kind, declarator)) = syntax
                        .variables
                        .get(&(member.module, member.source_start))
                        .copied()
                    else {
                        continue;
                    };
                    if let Some(diagnostic) = namespace_using_diagnostic(
                        kind,
                        NamespacePublication::Private,
                        false,
                        Span::from_oxc(declarator.span),
                    ) {
                        self.record_namespace_attachment_diagnostic(member.declaration, diagnostic);
                        invalid = true;
                    }
                    let annotation = match &declarator.type_annotation {
                        Some(annotation) => self.lower_namespace_member_annotation(
                            member.declaration,
                            member.scope,
                            &annotation.type_annotation,
                        ),
                        None => None,
                    };
                    self.namespace_values.insert_member(
                        member.module,
                        member.source_start,
                        PreparedNamespaceMember::Variable {
                            scope: member.scope,
                            annotation,
                        },
                    );
                }
                PreparedNamespaceValueKind::Function => {
                    let Some(function) = syntax
                        .functions
                        .get(&(member.module, member.source_start))
                        .copied()
                    else {
                        continue;
                    };
                    let reservation = self.reserve_namespace_function(
                        Some(member.declaration),
                        member.scope,
                        function,
                    );
                    self.namespace_values.insert_member(
                        member.module,
                        member.source_start,
                        PreparedNamespaceMember::Function {
                            scope: member.scope,
                            reservation,
                        },
                    );
                }
                PreparedNamespaceValueKind::Class => {
                    if syntax
                        .classes
                        .contains_key(&(member.module, member.source_start))
                    {
                        self.namespace_values.insert_member(
                            member.module,
                            member.source_start,
                            PreparedNamespaceMember::Class {
                                scope: member.scope,
                            },
                        );
                    }
                }
            }
        }
        invalid
    }

    fn select_attached_member_source(&mut self, member: &OwnedMemberInput) {
        debug_assert_eq!(source_ordinal(member.source_unit), member.source_ordinal);
        self.current_module = member.module;
        self.current_source = member.source_unit;
    }

    fn select_private_attached_member_source(&mut self, member: &PrivateMemberInput) {
        debug_assert_eq!(source_ordinal(member.source_unit), member.source_ordinal);
        self.current_module = member.module;
        self.current_source = member.source_unit;
    }

    fn reserve_namespace_function(
        &mut self,
        declaration: Option<DeclId>,
        scope: ScopeId,
        function: &Function<'_>,
    ) -> FunctionReservation<Ticket> {
        let callable = self
            .lexical_events
            .callable_at(source_ordinal(self.current_source), function.span.start)
            .and_then(|site| self.lexical_events.callable(site))
            .expect("namespace function has preallocated callable tickets");
        let tickets = callable.tickets;
        #[cfg(test)]
        self.namespace_values.record_namespace_function_reservation(
            declaration.expect("namespace function declaration"),
            callable.source,
        );
        #[cfg(not(test))]
        let _ = declaration;
        self.with_ticket_effects(tickets.signature, |pass| {
            let (mut lowered, child_failures) =
                pass.lower_namespace_callable_surface(scope, function, tickets.signature);
            let mut failures = lowered.failure.take().into_iter().collect::<Vec<_>>();
            failures.extend(child_failures);
            let unavailable = !failures.is_empty() || lowered.params.iter().any(Option::is_none);
            for failure in failures {
                pass.record_namespace_surface_failure(
                    failure,
                    tickets.signature,
                    Span::from_oxc(function.span),
                );
            }
            if function.return_type.is_none() {
                lowered.declared_return = None;
            }
            let type_param_frame = function
                .type_parameters
                .as_deref()
                .into_iter()
                .flat_map(|declaration| declaration.params.iter())
                .zip(&lowered.type_params)
                .map(|(parameter, generic)| {
                    (
                        parameter.name.name.to_string(),
                        pass.interner
                            .intern_type_param(generic.id, parameter.name.name.as_str()),
                    )
                })
                .collect();
            if unavailable {
                return FunctionReservation::Unavailable(RetainedFunctionBodySurface {
                    type_param_frame,
                    receiver: lowered.receiver,
                    params: lowered.params,
                    declared_return: lowered.declared_return,
                    tickets: Some(tickets),
                });
            }
            let params = lowered
                .params
                .into_iter()
                .map(|parameter| parameter.expect("ready namespace callable parameter"))
                .collect::<Vec<_>>();
            let ret = lowered.declared_return.unwrap_or_else(|| {
                let well_known = pass.interner.well_known();
                if function.body.is_some() {
                    well_known.unknown
                } else {
                    well_known.void
                }
            });
            let function_ty = pass.interner.intern_function(FunctionType {
                type_params: lowered.type_params.clone(),
                receiver: lowered.receiver,
                params: params.clone(),
                ret,
            });
            FunctionReservation::Ready(FunctionSurface {
                receiver: lowered.receiver,
                params,
                generic_params: lowered.type_params,
                type_param_frame,
                declared_return: lowered.declared_return,
                function_ty,
                tickets: Some(tickets),
            })
        })
    }

    fn lower_namespace_member_annotation(
        &mut self,
        declaration: DeclId,
        scope: ScopeId,
        annotation: &oxc_ast::ast::TSType<'_>,
    ) -> Option<TypeId> {
        let owner = self
            .lexical_events
            .declaration_owner(declaration)
            .expect("namespace member annotation has one exact owner")
            .ticket;
        self.with_ticket_effects(owner, |pass| {
            let (result, child_failures) =
                pass.lower_namespace_type_surface(scope, annotation, owner);
            let (ty, primary_failure) = match result {
                Ok(ty) => (Some(ty), None),
                Err(failure) => (None, Some(failure)),
            };
            let unavailable = primary_failure.is_some() || !child_failures.is_empty();
            for failure in primary_failure.into_iter().chain(child_failures) {
                pass.record_namespace_surface_failure(
                    failure,
                    owner,
                    Span::from_oxc(annotation.span()),
                );
            }
            if unavailable {
                None
            } else {
                ty
            }
        })
    }

    fn class_namespace_property_payload(
        &self,
        attachment: &AttachmentInput,
        properties: Vec<PropertyType>,
    ) -> Vec<ClassNamespacePropertyPayload<Ticket>> {
        properties
            .into_iter()
            .map(|property| {
                let member = attachment
                    .members
                    .iter()
                    .find(|member| member.name == property.name)
                    .expect("published namespace property has one exact declaration");
                let owner = self
                    .lexical_events
                    .declaration_owner(member.declaration)
                    .expect("published namespace property retains its exact owner")
                    .ticket;
                ClassNamespacePropertyPayload {
                    property,
                    declaration: member.declaration,
                    owner_span: member.owner_span,
                    source_order: ClassNamespacePropertySourceOrder {
                        source: member.source,
                        source_start: member.span.start,
                        declaration_ordinal: member.declaration.0,
                    },
                    owner,
                }
            })
            .collect()
    }

    fn stage_namespace_function_properties(
        &mut self,
        functions: &[StagedFunction<'_, '_, Ticket>],
    ) -> Option<Vec<PropertyType>> {
        let mut order = Vec::new();
        let mut groups: FxHashMap<String, Vec<usize>> = FxHashMap::default();
        for (index, function) in functions.iter().enumerate() {
            if !groups.contains_key(&function.input.name) {
                order.push(function.input.name.clone());
            }
            groups
                .entry(function.input.name.clone())
                .or_default()
                .push(index);
        }
        let mut properties = Vec::new();
        for name in order {
            let indices = groups.get(&name).expect("function group index exists");
            let mut signatures = Vec::new();
            if indices.len() == 1 {
                let function = &functions[indices[0]];
                match &function.reservation {
                    FunctionReservation::Ready(surface)
                        if function.syntax.body.is_none() || surface.declared_return.is_some() =>
                    {
                        signatures.push(surface.function_ty);
                    }
                    FunctionReservation::Ready(_) => {
                        self.record_namespace_member_unavailable(
                            &function.input,
                            "decl/function-declaration/namespace-payload-inferred-return",
                            "namespace member return cannot be inferred before owner publication",
                        );
                        return None;
                    }
                    FunctionReservation::Unavailable(_) => return None,
                }
            } else {
                for index in indices {
                    let function = &functions[*index];
                    if function.syntax.body.is_some() {
                        continue;
                    }
                    match &function.reservation {
                        FunctionReservation::Ready(surface) => signatures.push(surface.function_ty),
                        FunctionReservation::Unavailable(_) => return None,
                    }
                }
                if signatures.is_empty() {
                    return None;
                }
            }
            let ty = if signatures.len() == 1 {
                signatures[0]
            } else {
                self.interner.intern_object(ObjectType {
                    call_signatures: signatures,
                    ..Default::default()
                })
            };
            properties.push(PropertyType::public(name, ty));
        }
        Some(properties)
    }

    fn install_unavailable_function_payload(&mut self, attachment: &AttachmentInput) {
        let expected = match attachment.disposition {
            NamespaceValueAttachmentDisposition::AdmittedFunction => {
                "admitted function attachment must install"
            }
            NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42 => {
                "deferred function recovery must install"
            }
            NamespaceValueAttachmentDisposition::AdmittedClass
            | NamespaceValueAttachmentDisposition::TypeContainerOnly
            | NamespaceValueAttachmentDisposition::Rejected(_) => return,
        };
        let installed = self.install_function_namespace_payload(
            attachment.owner_scope,
            &attachment.name,
            FunctionNamespacePayload::Unavailable { owner: None },
        );
        assert!(installed, "{expected}");
    }

    fn record_namespace_member_unavailable(
        &mut self,
        member: &OwnedMemberInput,
        id: &str,
        context: &str,
    ) {
        self.record_namespace_attachment_unavailable(
            member.declaration,
            member_span(member),
            id,
            context,
        );
    }

    fn record_namespace_attachment_unavailable(
        &mut self,
        declaration: DeclId,
        span: Span,
        id: &str,
        context: &str,
    ) {
        let owner = self
            .lexical_events
            .declaration_owner(declaration)
            .expect("attached namespace source has one exact declaration owner");
        self.with_ticket_effects(owner.ticket, |pass| {
            pass.record_incomplete(id, span, context);
        });
    }

    fn record_namespace_attachment_diagnostic(
        &mut self,
        declaration: DeclId,
        diagnostic: Diagnostic,
    ) {
        let owner = self
            .lexical_events
            .declaration_owner(declaration)
            .expect("namespace diagnostic source has one exact owner");
        self.with_ticket_effects(owner.ticket, |pass| pass.emit_diagnostic(diagnostic));
    }

    /// Consume a prepared namespace fragment at its source position.
    pub(in crate::check::checker) fn check_prepared_namespace_declaration(
        &mut self,
        declaration: &TSModuleDeclaration<'_>,
    ) -> bool {
        let consumed = self
            .namespace_values
            .is_consumed_fragment(self.current_module, declaration.span.start);
        let has_private_checks = self
            .namespace_values
            .take_private_fragment(self.current_module, declaration.span.start);
        let ambient = self
            .namespace_values
            .is_ambient_fragment(self.current_module, declaration.span.start);
        if !consumed && !has_private_checks {
            return false;
        }
        let scope = self
            .namespace_values
            .fragment_scope(self.current_module, declaration.span.start)
            .expect("consumed namespace fragment retains its private scope");
        self.check_prepared_namespace_body(declaration, ambient, scope);
        consumed
    }

    fn check_prepared_namespace_body(
        &mut self,
        declaration: &TSModuleDeclaration<'_>,
        ambient: bool,
        scope: ScopeId,
    ) {
        match &declaration.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                self.check_prepared_namespace_statements(scope, &block.body, ambient)
            }
            Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
                assert!(
                    self.check_prepared_namespace_declaration(nested),
                    "prepared dotted namespace child is consumed"
                );
            }
            None => {}
        }
    }

    fn check_prepared_namespace_statements(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
        ambient: bool,
    ) {
        let mut index = 0;
        while index < statements.len() {
            if function_decl_from_statement(&statements[index]).is_some() {
                let end =
                    function_overload_group(statements, index).map_or(index + 1, |(_, end)| end);
                if self.check_prepared_namespace_function_group(&statements[index..end], ambient) {
                    index = end;
                    continue;
                }
            }
            let statement = &statements[index];
            match statement {
                Statement::VariableDeclaration(declaration) => {
                    self.check_prepared_namespace_variable(declaration)
                }
                Statement::FunctionDeclaration(function) => {
                    self.check_prepared_namespace_function(function)
                }
                Statement::ClassDeclaration(class) => self.check_class_for_namespace(class),
                Statement::TSEnumDeclaration(_) | Statement::TSImportEqualsDeclaration(_)
                    if self.namespace_values.take_private_unsupported_member(
                        self.current_module,
                        statement.span().start,
                    ) =>
                {
                    let mut no_return = None;
                    self.check_stmt(scope, statement, None, &mut no_return);
                }
                Statement::TSModuleDeclaration(nested) => {
                    self.check_prepared_namespace_declaration(nested);
                }
                Statement::ExportNamedDeclaration(export) => {
                    if let Some(declaration) = &export.declaration {
                        match declaration {
                            Declaration::VariableDeclaration(declaration) => {
                                self.check_prepared_namespace_variable(declaration)
                            }
                            Declaration::FunctionDeclaration(function) => {
                                self.check_prepared_namespace_function(function)
                            }
                            Declaration::ClassDeclaration(class) => {
                                self.check_class_for_namespace(class)
                            }
                            Declaration::TSModuleDeclaration(nested) => {
                                self.check_prepared_namespace_declaration(nested);
                            }
                            _ => {}
                        }
                    }
                }
                Statement::BlockStatement(_)
                | Statement::IfStatement(_)
                | Statement::SwitchStatement(_)
                | Statement::WhileStatement(_)
                | Statement::LabeledStatement(_)
                | Statement::ForStatement(_)
                | Statement::ForInStatement(_)
                | Statement::ForOfStatement(_)
                | Statement::DoWhileStatement(_)
                | Statement::ThrowStatement(_)
                | Statement::TryStatement(_)
                | Statement::ExpressionStatement(_) => {
                    let mut no_return = None;
                    self.check_stmt(scope, statement, None, &mut no_return);
                }
                _ => {}
            }
            index += 1;
        }
    }

    fn check_prepared_namespace_function_group(
        &mut self,
        statements: &[Statement<'_>],
        ambient: bool,
    ) -> bool {
        let functions = statements
            .iter()
            .filter_map(function_decl_from_statement)
            .collect::<Vec<_>>();
        let Some(scope) = functions.first().and_then(|function| {
            self.namespace_values
                .function_scope(self.current_module, function.span.start)
        }) else {
            return false;
        };
        if functions.iter().any(|function| {
            self.namespace_values
                .function_scope(self.current_module, function.span.start)
                != Some(scope)
        }) {
            return false;
        }
        let mut surfaces = FxHashMap::default();
        for function in functions {
            let Some((_, reservation)) = self
                .namespace_values
                .take_function(self.current_module, function.span.start)
            else {
                return false;
            };
            surfaces.insert(function.span.start, reservation);
        }
        self.validate_reserved_namespace_function_group(scope, statements, &mut surfaces, ambient)
    }

    fn check_prepared_namespace_variable(&mut self, declaration: &VariableDeclaration<'_>) {
        for declarator in &declaration.declarations {
            let Some(PreparedNamespaceMember::Variable { scope, annotation }) = self
                .namespace_values
                .take_member(self.current_module, declarator.span.start)
            else {
                continue;
            };
            self.with_lexical_effects(declarator.span.start, LexicalOwnerPhase::Deferred, |pass| {
                if let Some(initializer) = &declarator.init {
                    pass.check_pattern_annotated_initializer(
                        scope,
                        annotation,
                        &declarator.id,
                        initializer,
                    );
                }
            });
        }
    }

    fn check_prepared_namespace_function(&mut self, function: &Function<'_>) {
        let Some(PreparedNamespaceMember::Function { scope, reservation }) = self
            .namespace_values
            .take_member(self.current_module, function.span.start)
        else {
            return;
        };
        match reservation {
            FunctionReservation::Ready(surface) => {
                self.fill_reserved_function(scope, function, &surface);
            }
            FunctionReservation::Unavailable(surface) => {
                self.check_retained_function_body(scope, function, &surface);
            }
        }
    }

    fn check_class_for_namespace(&mut self, class: &Class<'_>) {
        let Some(PreparedNamespaceMember::Class { scope }) = self
            .namespace_values
            .take_member(self.current_module, class.span.start)
        else {
            return;
        };
        self.check_class(scope, class);
    }
}

fn collect_attachment_inputs<Ticket: Copy + PartialEq>(
    pass: &Pass<'_, '_, Ticket>,
    modules: &FxHashSet<ScopeId>,
) -> Vec<AttachmentInput> {
    let mut inputs = Vec::new();
    for record in pass.binder.namespaces.merges() {
        let Some(owner_scope) = declaration_owner_scope(pass, record.owner) else {
            continue;
        };
        let Some(attachment) = pass
            .binder
            .namespace_value_attachment(owner_scope, &record.name)
        else {
            continue;
        };
        let exposes_value_attachment = match attachment.disposition {
            NamespaceValueAttachmentDisposition::AdmittedFunction => true,
            NamespaceValueAttachmentDisposition::AdmittedClass => true,
            NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42 => true,
            NamespaceValueAttachmentDisposition::TypeContainerOnly
            | NamespaceValueAttachmentDisposition::Rejected(_) => false,
        };
        if !exposes_value_attachment
            || !attachment
                .fragments
                .iter()
                .any(|fragment| modules.contains(&fragment.module))
        {
            continue;
        }
        let value_member_count = attachment
            .fragments
            .iter()
            .filter(|fragment| modules.contains(&fragment.module))
            .flat_map(|fragment| fragment.members.iter())
            .filter_map(|member| pass.binder.namespaces.member(*member))
            .filter(|member| {
                namespace_member_participates_in_payload(member.spaces.value, member.publication)
            })
            .count();
        let mut private_members = attachment
            .fragments
            .iter()
            .filter(|fragment| modules.contains(&fragment.module))
            .flat_map(|fragment| fragment.members.iter())
            .filter_map(|member| pass.binder.namespaces.member(*member))
            .filter(|member| {
                member.spaces.value
                    && matches!(member.publication, NamespacePublication::Private)
                    && matches!(
                        member.kind,
                        MergeDeclarationKind::Variable
                            | MergeDeclarationKind::Function
                            | MergeDeclarationKind::Class
                    )
            })
            .filter_map(|member| {
                let declaration = member.declaration?;
                let site = pass.binder.declarations.get(declaration)?.site;
                let source = pass.lexical_events.declaration_source(declaration)?;
                let ordinal = source_ordinal_from_origin(member.origin);
                debug_assert_eq!(source_ordinal(source.unit), ordinal);
                Some((
                    declaration,
                    PrivateMemberInput {
                        declaration,
                        scope: site.scope?,
                        module: site.module,
                        source: member.source,
                        source_ordinal: ordinal,
                        source_unit: source.unit,
                        source_start: site.declaration_span.start,
                        kind: prepared_namespace_value_kind(member.kind)?,
                    },
                ))
            })
            .collect::<Vec<_>>();
        private_members.sort_by_key(|(declaration, member)| {
            (member.source, member.source_start, declaration.0)
        });
        private_members.dedup_by_key(|(declaration, _)| *declaration);
        let private_members = private_members
            .into_iter()
            .map(|(_, member)| member)
            .collect::<Vec<_>>();
        let mut members = Vec::new();
        let mut unavailable_members = Vec::new();
        for member in &attachment.members {
            if !modules.contains(&member.site.module) {
                continue;
            }
            let Some(storage) = member.value_storage else {
                unavailable_members.push(UnavailableMemberInput {
                    declaration: member.declaration,
                    span: member.site.declaration_span,
                    kind: member.kind,
                });
                continue;
            };
            let Some(kind) = prepared_namespace_value_kind(member.kind) else {
                unavailable_members.push(UnavailableMemberInput {
                    declaration: member.declaration,
                    span: member.site.declaration_span,
                    kind: member.kind,
                });
                continue;
            };
            let source = pass
                .lexical_events
                .declaration_source(member.declaration)
                .expect("attached namespace member has exact source reservation");
            let ordinal = source_ordinal_from_origin(member.origin);
            debug_assert_eq!(source_ordinal(source.unit), ordinal);
            members.push(OwnedMemberInput {
                declaration: member.declaration,
                storage,
                scope: member.scope,
                module: member.site.module,
                source: member.source,
                source_ordinal: ordinal,
                source_unit: source.unit,
                source_start: member.site.declaration_span.start,
                span: member.site.declaration_span,
                owner_span: member.site.binding_span,
                name: member.name.to_owned(),
                kind,
                publication: member.publication,
                ambient: member.ambient,
                readonly: member.variable_kind
                    == Some(crate::binder::namespace::VariableKind::Const),
            });
        }
        let class_group = pass
            .binder
            .symbols
            .get(attachment.symbol)
            .and_then(|symbol| symbol.ty);
        let selected_member_count = attachment
            .members
            .iter()
            .filter(|member| modules.contains(&member.site.module))
            .count();
        let has_unavailable_metadata = value_member_count != selected_member_count;
        inputs.push(AttachmentInput {
            owner_scope,
            name: attachment.name.to_owned(),
            class_group,
            disposition: attachment.disposition,
            fragments: attachment
                .fragments
                .iter()
                .filter(|fragment| modules.contains(&fragment.module))
                .map(|fragment| FragmentInput {
                    module: fragment.module,
                    source_start: fragment.source_start,
                    private_scope: fragment.private_scope,
                    ambient: fragment.ambient,
                })
                .collect(),
            private_members,
            unavailable_members,
            has_unavailable_metadata,
            members,
        });
    }
    inputs
}

fn collect_all_standalone_attachment_inputs<Ticket: Copy + PartialEq>(
    pass: &Pass<'_, '_, Ticket>,
) -> Vec<StandaloneAttachmentInput> {
    pass.binder
        .standalone_namespace_value_attachments()
        .into_iter()
        .filter_map(|attachment| standalone_attachment_input(pass, attachment))
        .collect()
}

fn standalone_attachment_input<Ticket: Copy + PartialEq>(
    pass: &Pass<'_, '_, Ticket>,
    attachment: StandaloneNamespaceValueAttachment<'_>,
) -> Option<StandaloneAttachmentInput> {
    pass.binder.namespaces.get(attachment.namespace)?;
    let fallback_module = attachment.fragments.first()?.module;
    let fragments = attachment
        .fragments
        .iter()
        .map(|fragment| FragmentInput {
            module: fragment.module,
            source_start: fragment.source_start,
            private_scope: fragment.private_scope,
            ambient: fragment.ambient,
        })
        .collect();
    let members = attachment
        .members
        .into_iter()
        .map(|member| StandaloneMemberInput {
            declaration: member.declaration,
            name: member.name.map(str::to_owned),
            scope: member.site.and_then(|site| site.scope),
            module: member.site.map_or(fallback_module, |site| site.module),
            source: member.source,
            source_start: member.site.map_or(member.declaration_span.start, |site| {
                site.declaration_span.start
            }),
            span: member
                .site
                .map_or(member.declaration_span, |site| site.declaration_span),
            local_span: member.local_span,
            source_ordinal: source_ordinal_from_origin(member.origin),
            value_storage: member.value_storage,
            alias_target_storage: member.alias_target_storage,
            ambient: member.ambient,
            child_namespace: member.child_namespace,
            kind: member.kind,
            publication: member.publication,
            has_value_space: member.spaces.value,
        })
        .collect();
    Some(StandaloneAttachmentInput {
        namespace: attachment.namespace,
        storage: attachment.storage,
        fragments,
        members,
    })
}

fn legal_existing_owner_merges(
    members: &[StandaloneMemberInput],
) -> FxHashMap<String, LegalExistingOwnerMerge> {
    let mut merged = FxHashMap::default();
    let mut groups: FxHashMap<&str, Vec<&StandaloneMemberInput>> = FxHashMap::default();
    for member in members {
        if let Some(name) = member.name.as_deref() {
            groups.entry(name).or_default().push(member);
        }
    }
    for (name, group) in groups {
        let Some(owner) = group.iter().copied().find(|member| {
            matches!(
                member.kind,
                MergeDeclarationKind::Function | MergeDeclarationKind::Class
            )
        }) else {
            continue;
        };
        let Some(storage) = owner.value_storage else {
            continue;
        };
        let mut namespace_count = 0usize;
        let mut owner_count = 0usize;
        let exact_pair = group.iter().all(|member| {
            if member.value_storage != Some(storage) {
                return false;
            }
            match member.kind {
                MergeDeclarationKind::Namespace => {
                    namespace_count += 1;
                    true
                }
                kind if kind == owner.kind => {
                    owner_count += 1;
                    owner.kind == MergeDeclarationKind::Function || owner_count == 1
                }
                _ => false,
            }
        });
        if exact_pair && namespace_count > 0 && owner_count > 0 {
            merged.insert(
                name.to_owned(),
                LegalExistingOwnerMerge {
                    kind: owner.kind,
                    storage,
                },
            );
        }
    }
    merged
}

struct RootReferenceVisitor<'a> {
    binder: &'a crate::binder::Binder,
    module: ScopeId,
    scope: ScopeId,
    storage: ValueStorageId,
    found: bool,
    scope_stack: Vec<ScopeId>,
}

impl<'ast> Visit<'ast> for RootReferenceVisitor<'_> {
    fn enter_node(&mut self, kind: AstKind<'ast>) {
        let next = match kind {
            AstKind::Function(function) => self
                .binder
                .fn_scopes
                .get(&(self.module, function.span.start))
                .copied(),
            AstKind::ArrowFunctionExpression(arrow) => self
                .binder
                .fn_scopes
                .get(&(self.module, arrow.span.start))
                .copied(),
            AstKind::BlockStatement(block) => self
                .binder
                .block_scopes
                .get(&(self.module, block.span.start))
                .copied(),
            AstKind::ForStatement(statement) => self
                .binder
                .block_scopes
                .get(&(self.module, statement.span.start))
                .copied(),
            AstKind::CatchClause(clause) => self
                .binder
                .block_scopes
                .get(&(self.module, clause.span.start))
                .copied(),
            _ => return,
        };
        self.scope_stack.push(self.scope);
        if let Some(next) = next {
            self.scope = next;
        }
    }

    fn leave_node(&mut self, kind: AstKind<'ast>) {
        if matches!(
            kind,
            AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::BlockStatement(_)
                | AstKind::ForStatement(_)
                | AstKind::CatchClause(_)
        ) {
            self.scope = self
                .scope_stack
                .pop()
                .expect("root-reference lexical scope stack is balanced");
        }
    }

    fn visit_identifier_reference(&mut self, identifier: &oxc_ast::ast::IdentifierReference<'ast>) {
        if self
            .binder
            .resolve_value(self.scope, identifier.name.as_str())
            .and_then(|symbol| self.binder.symbols.get(symbol))
            .and_then(|symbol| symbol.value)
            == Some(self.storage)
        {
            self.found = true;
        }
        walk::walk_identifier_reference(self, identifier);
    }

    fn visit_for_in_statement(&mut self, statement: &oxc_ast::ast::ForInStatement<'ast>) {
        self.visit_expression(&statement.right);
        let saved = self.scope;
        if let Some(scope) = self
            .binder
            .block_scopes
            .get(&(self.module, statement.span.start))
            .copied()
        {
            self.scope = scope;
        }
        self.visit_for_statement_left(&statement.left);
        self.visit_statement(&statement.body);
        self.scope = saved;
    }

    fn visit_for_of_statement(&mut self, statement: &oxc_ast::ast::ForOfStatement<'ast>) {
        self.visit_expression(&statement.right);
        let saved = self.scope;
        if let Some(scope) = self
            .binder
            .block_scopes
            .get(&(self.module, statement.span.start))
            .copied()
        {
            self.scope = scope;
        }
        self.visit_for_statement_left(&statement.left);
        self.visit_statement(&statement.body);
        self.scope = saved;
    }

    fn visit_switch_statement(&mut self, statement: &oxc_ast::ast::SwitchStatement<'ast>) {
        self.visit_expression(&statement.discriminant);
        let saved = self.scope;
        if let Some(scope) = self
            .binder
            .block_scopes
            .get(&(self.module, statement.span.start))
            .copied()
        {
            self.scope = scope;
        }
        for case in &statement.cases {
            self.visit_switch_case(case);
        }
        self.scope = saved;
    }
}

fn class_has_static_root_reference(
    binder: &crate::binder::Binder,
    module: ScopeId,
    scope: ScopeId,
    class: &Class<'_>,
    storage: ValueStorageId,
) -> bool {
    class.body.body.iter().any(|element| {
        let ClassElement::PropertyDefinition(property) = element else {
            return false;
        };
        let Some(initializer) = property
            .value
            .as_ref()
            .filter(|_| property.r#static && property.type_annotation.is_none())
        else {
            return false;
        };
        let mut visitor = RootReferenceVisitor {
            binder,
            module,
            scope,
            storage,
            found: false,
            scope_stack: Vec::new(),
        };
        visitor.visit_expression(initializer);
        visitor.found
    })
}

fn prepared_namespace_value_kind(kind: MergeDeclarationKind) -> Option<PreparedNamespaceValueKind> {
    match kind {
        MergeDeclarationKind::Variable => Some(PreparedNamespaceValueKind::Variable),
        MergeDeclarationKind::Function => Some(PreparedNamespaceValueKind::Function),
        MergeDeclarationKind::Class => Some(PreparedNamespaceValueKind::Class),
        MergeDeclarationKind::TypeAlias
        | MergeDeclarationKind::Interface
        | MergeDeclarationKind::Enum
        | MergeDeclarationKind::Namespace
        | MergeDeclarationKind::ImportAlias
        | MergeDeclarationKind::DeferredExport => None,
    }
}

fn namespace_payload_unavailable(
    kind: MergeDeclarationKind,
) -> Option<(&'static str, &'static str)> {
    match kind {
        MergeDeclarationKind::Enum => Some((
            "decl/enum-declaration/namespace-payload-unavailable",
            "attached namespace enum value is not modeled",
        )),
        MergeDeclarationKind::ImportAlias => Some((
            "decl/import-equals/namespace-payload-unavailable",
            "attached namespace import-equals value is not modeled",
        )),
        MergeDeclarationKind::Variable
        | MergeDeclarationKind::Function
        | MergeDeclarationKind::Class
        | MergeDeclarationKind::TypeAlias
        | MergeDeclarationKind::Interface
        | MergeDeclarationKind::Namespace
        | MergeDeclarationKind::DeferredExport => None,
    }
}

fn duplicate_property_kind(
    first: PreparedNamespaceValueKind,
    later: PreparedNamespaceValueKind,
) -> Option<PreparedNamespaceValueKind> {
    match (first, later) {
        (PreparedNamespaceValueKind::Function, PreparedNamespaceValueKind::Function) => None,
        (_, later) => Some(later),
    }
}

fn namespace_payload_duplicate(kind: PreparedNamespaceValueKind) -> (&'static str, &'static str) {
    match kind {
        PreparedNamespaceValueKind::Variable => (
            "decl/variable-declaration/namespace-payload-duplicate-value",
            "namespace variable duplicates an earlier exported value",
        ),
        PreparedNamespaceValueKind::Function => (
            "decl/function-declaration/namespace-payload-duplicate-value",
            "namespace function duplicates an earlier exported value",
        ),
        PreparedNamespaceValueKind::Class => (
            "decl/class-declaration/namespace-payload-duplicate-value",
            "namespace class duplicates an earlier exported value",
        ),
    }
}

fn namespace_member_participates_in_payload(
    has_value_space: bool,
    publication: NamespacePublication,
) -> bool {
    has_value_space && !matches!(publication, NamespacePublication::Private)
}

fn standalone_member_participates<Ticket: Copy + PartialEq>(
    pass: &Pass<'_, '_, Ticket>,
    member: &StandaloneMemberInput,
) -> bool {
    member.has_value_space
        || member.kind == MergeDeclarationKind::DeferredExport
        || (member.kind == MergeDeclarationKind::Namespace
            && member.child_namespace.is_some_and(|namespace| {
                pass.binder.namespaces.aggregate_instance_state(namespace)
                    == Some(crate::binder::namespace::NamespaceInstanceState::Instantiated)
            }))
}

fn namespace_using_diagnostic(
    kind: VariableDeclarationKind,
    _publication: NamespacePublication,
    ambient: bool,
    span: Span,
) -> Option<Diagnostic> {
    match kind {
        VariableDeclarationKind::Using if ambient => {
            Some(Diagnostic::ambient_using_not_allowed(span))
        }
        VariableDeclarationKind::AwaitUsing => {
            Some(Diagnostic::namespace_await_using_not_allowed(span))
        }
        VariableDeclarationKind::Var
        | VariableDeclarationKind::Let
        | VariableDeclarationKind::Const
        | VariableDeclarationKind::Using => None,
    }
}

fn invalid_namespace_using(
    kind: VariableDeclarationKind,
    publication: NamespacePublication,
    ambient: bool,
) -> bool {
    match kind {
        VariableDeclarationKind::Using => {
            ambient || matches!(publication, NamespacePublication::Explicit)
        }
        VariableDeclarationKind::AwaitUsing => true,
        VariableDeclarationKind::Var
        | VariableDeclarationKind::Let
        | VariableDeclarationKind::Const => false,
    }
}

fn declaration_owner_scope<Ticket: Copy + PartialEq>(
    pass: &Pass<'_, '_, Ticket>,
    owner: DeclarationOwner,
) -> Option<ScopeId> {
    match owner {
        DeclarationOwner::Lexical(scope) => Some(scope),
        DeclarationOwner::NamespacePublic(namespace) => pass
            .binder
            .namespaces
            .get(namespace)
            .map(|namespace| namespace.public_scope),
        DeclarationOwner::NamespacePrivate(fragment) => pass
            .binder
            .namespaces
            .fragment(fragment)
            .map(|fragment| fragment.private_scope),
        DeclarationOwner::CompilationGlobal => Some(pass.binder.compilation_global),
        DeclarationOwner::DeferredAmbientModule(_) => None,
    }
}

fn query_free_initializer_type<Ticket: Copy + PartialEq>(
    pass: &mut Pass<'_, '_, Ticket>,
    kind: VariableDeclarationKind,
    initializer: &Expression<'_>,
) -> Option<TypeId> {
    let ty = match initializer {
        Expression::StringLiteral(literal) => {
            pass.interner
                .intern_literal(crate::types::repr::LiteralValue::String(
                    literal.value.to_string(),
                ))
        }
        Expression::NumericLiteral(literal) => pass
            .interner
            .intern_literal(crate::types::repr::LiteralValue::Number(literal.value)),
        Expression::BooleanLiteral(literal) => pass
            .interner
            .intern_literal(crate::types::repr::LiteralValue::Boolean(literal.value)),
        Expression::NullLiteral(_) => pass.interner.well_known().null,
        Expression::ParenthesizedExpression(parenthesized) => {
            return query_free_initializer_type(pass, kind, &parenthesized.expression)
        }
        _ => return None,
    };
    Some(declared_from_init(pass.interner, kind, ty))
}

fn member_span(member: &OwnedMemberInput) -> Span {
    member.span
}

fn index_namespace_statements<'stmt, 'ast>(
    statements: &'stmt [Statement<'ast>],
    inside_namespace: bool,
    index: &mut NamespaceSyntaxIndex<'stmt, 'ast>,
) {
    for statement in statements {
        match statement {
            Statement::TSModuleDeclaration(declaration) => index_namespace_body(declaration, index),
            Statement::ExportNamedDeclaration(export) => {
                if let Some(declaration) = &export.declaration {
                    index_namespace_declaration(declaration, inside_namespace, index);
                }
            }
            Statement::VariableDeclaration(declaration) if inside_namespace => {
                index_namespace_variable(declaration, index)
            }
            Statement::FunctionDeclaration(function) if inside_namespace => {
                index.functions.insert(function.span.start, function);
            }
            Statement::ClassDeclaration(class) if inside_namespace => {
                index.classes.insert(class.span.start, class);
            }
            _ => {}
        }
    }
}

fn index_namespace_declaration<'stmt, 'ast>(
    declaration: &'stmt Declaration<'ast>,
    inside_namespace: bool,
    index: &mut NamespaceSyntaxIndex<'stmt, 'ast>,
) {
    match declaration {
        Declaration::TSModuleDeclaration(declaration) => index_namespace_body(declaration, index),
        Declaration::VariableDeclaration(declaration) if inside_namespace => {
            index_namespace_variable(declaration, index)
        }
        Declaration::FunctionDeclaration(function) if inside_namespace => {
            index.functions.insert(function.span.start, function);
        }
        Declaration::ClassDeclaration(class) if inside_namespace => {
            index.classes.insert(class.span.start, class);
        }
        _ => {}
    }
}

fn index_namespace_variable<'stmt, 'ast>(
    declaration: &'stmt VariableDeclaration<'ast>,
    index: &mut NamespaceSyntaxIndex<'stmt, 'ast>,
) {
    for declarator in &declaration.declarations {
        index
            .variables
            .insert(declarator.span.start, (declaration.kind, declarator));
    }
}

fn index_namespace_body<'stmt, 'ast>(
    declaration: &'stmt TSModuleDeclaration<'ast>,
    index: &mut NamespaceSyntaxIndex<'stmt, 'ast>,
) {
    match &declaration.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            index_namespace_statements(&block.body, true, index)
        }
        Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
            index_namespace_body(nested, index)
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::check_program_with_namespace_value_inspector;
    use super::{
        duplicate_property_kind, namespace_member_participates_in_payload,
        namespace_payload_duplicate, namespace_payload_unavailable, prepared_namespace_value_kind,
        project_syntax_index, FrozenNamespaceUnavailableCause,
        FrozenNamespaceValueTerminalSnapshot, FrozenNamespaceValueTerminalSnapshotRow,
        FrozenNamespaceValueTerminals, MergeDeclarationKind, NamespacePublication,
        PreparedNamespaceValueKind, StandaloneNamespaceTerminal,
    };
    use crate::binder::declaration::ValueStorageId;
    use crate::binder::namespace::NamespaceId;
    use crate::binder::namespace::NamespaceInstanceState;
    use crate::binder::scope::ScopeId;
    use crate::check::checker::events_library::LibraryEventLedger;
    use crate::check::checker::lexical_events::LexicalReservations;
    use crate::check::query::reset_query_demand_measure;
    use crate::driver::check_source;
    use crate::source::{LibraryFileOrdinal, SourceOrdinal, SourceUnit};
    use crate::types::{Interner, TypeId};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use rustc_hash::FxHashMap;

    #[test]
    fn frozen_namespace_snapshot_round_trips_closed_terminals() {
        let causes = [
            FrozenNamespaceUnavailableCause::MissingExportedMemberName,
            FrozenNamespaceUnavailableCause::DuplicateExportedValue,
            FrozenNamespaceUnavailableCause::UnboundExportedVariable,
            FrozenNamespaceUnavailableCause::MissingExportedVariableSyntax,
            FrozenNamespaceUnavailableCause::InvalidUsingDeclaration,
            FrozenNamespaceUnavailableCause::VariableSurfaceUnavailable,
            FrozenNamespaceUnavailableCause::UnboundExportedFunction,
            FrozenNamespaceUnavailableCause::MissingExportedFunctionSyntax,
            FrozenNamespaceUnavailableCause::UnboundExportedClass,
            FrozenNamespaceUnavailableCause::UnboundExportedClassIdentity,
            FrozenNamespaceUnavailableCause::UnboundNestedNamespace,
            FrozenNamespaceUnavailableCause::UnsupportedExportedMember,
            FrozenNamespaceUnavailableCause::DeferredExportedMember,
            FrozenNamespaceUnavailableCause::FunctionNamespacePayloadUnavailable,
            FrozenNamespaceUnavailableCause::FunctionOwnerCallSurfaceUnavailable,
            FrozenNamespaceUnavailableCause::FunctionSurfaceUnavailable,
            FrozenNamespaceUnavailableCause::ClassSurfaceUnavailable,
            FrozenNamespaceUnavailableCause::NestedNamespaceUnavailable,
            FrozenNamespaceUnavailableCause::ExistingOwnerUnavailable,
            FrozenNamespaceUnavailableCause::NamespaceContainmentCycle,
            FrozenNamespaceUnavailableCause::InvalidPrivateNamespaceMember,
            FrozenNamespaceUnavailableCause::ClassValueSurfaceUnavailable,
        ];
        let mut standalone = causes
            .into_iter()
            .enumerate()
            .map(|(index, cause)| {
                (
                    NamespaceId(u32::try_from(index).expect("cause index fits u32")),
                    StandaloneNamespaceTerminal::Unavailable { cause },
                )
            })
            .collect::<FxHashMap<_, _>>();
        standalone.insert(
            NamespaceId(30),
            StandaloneNamespaceTerminal::Ready {
                storage: ValueStorageId(11),
                ty: TypeId(13),
            },
        );
        let frozen = FrozenNamespaceValueTerminals { standalone };

        let parts = frozen.snapshot_parts().expect("snapshot terminals");
        let restored =
            FrozenNamespaceValueTerminals::from_snapshot_parts(parts.clone()).expect("restore");

        assert_eq!(restored.snapshot_parts(), Ok(parts));
    }

    #[test]
    fn frozen_namespace_snapshot_rejects_noncanonical_input() {
        assert!(FrozenNamespaceValueTerminals::from_snapshot_parts(vec![
            FrozenNamespaceValueTerminalSnapshotRow {
                namespace: NamespaceId(3),
                terminal: FrozenNamespaceValueTerminalSnapshot::Unavailable(
                    FrozenNamespaceUnavailableCause::DeferredExportedMember,
                ),
            },
            FrozenNamespaceValueTerminalSnapshotRow {
                namespace: NamespaceId(1),
                terminal: FrozenNamespaceValueTerminalSnapshot::Ready {
                    storage: ValueStorageId(2),
                    ty: TypeId(4),
                },
            },
        ])
        .is_err());
    }

    #[test]
    fn project_attached_callables_keep_exact_sources_at_identical_offsets() {
        let alpha_source = "declare namespace Attached { function alpha(value: string): string; }";
        let bravo_source = "declare namespace Attached { function bravo(value: number): number; }";
        let alpha_allocator = Allocator::default();
        let bravo_allocator = Allocator::default();
        let alpha = Parser::new(&alpha_allocator, alpha_source, SourceType::ts()).parse();
        let bravo = Parser::new(&bravo_allocator, bravo_source, SourceType::ts()).parse();
        assert!(!alpha.panicked);
        assert!(!bravo.panicked);

        let alpha_scope = ScopeId(10);
        let bravo_scope = ScopeId(20);
        let alpha_file = LibraryFileOrdinal::new(4);
        let bravo_file = LibraryFileOrdinal::new(9);
        let alpha_start = alpha_source.find("function").expect("alpha function") as u32;
        let bravo_start = bravo_source.find("function").expect("bravo function") as u32;
        assert_eq!(alpha_start, bravo_start);

        let forward = project_syntax_index(&[
            (alpha_scope, alpha.program.body.as_slice()),
            (bravo_scope, bravo.program.body.as_slice()),
        ]);
        let reverse = project_syntax_index(&[
            (bravo_scope, bravo.program.body.as_slice()),
            (alpha_scope, alpha.program.body.as_slice()),
        ]);
        for syntax in [&forward, &reverse] {
            assert_eq!(
                syntax
                    .functions
                    .get(&(alpha_scope, alpha_start))
                    .and_then(|function| function.id.as_ref())
                    .map(|id| id.name.as_str()),
                Some("alpha")
            );
            assert_eq!(
                syntax
                    .functions
                    .get(&(bravo_scope, bravo_start))
                    .and_then(|function| function.id.as_ref())
                    .map(|id| id.name.as_str()),
                Some("bravo")
            );
        }

        let mut ledger = LibraryEventLedger::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_library_program(bravo_file, &bravo.program, &mut ledger)
            .expect("reserve bravo first");
        reservations
            .reserve_library_program(alpha_file, &alpha.program, &mut ledger)
            .expect("reserve alpha second");
        let alpha_callable = reservations
            .callable_at(SourceOrdinal::Library(alpha_file), alpha_start)
            .and_then(|site| reservations.callable(site))
            .expect("alpha callable");
        let bravo_callable = reservations
            .callable_at(SourceOrdinal::Library(bravo_file), bravo_start)
            .and_then(|site| reservations.callable(site))
            .expect("bravo callable");
        assert_eq!(
            alpha_callable.source.unit,
            SourceUnit::Library {
                file_ordinal: alpha_file
            }
        );
        assert_eq!(
            bravo_callable.source.unit,
            SourceUnit::Library {
                file_ordinal: bravo_file
            }
        );
        assert_ne!(alpha_callable.tickets, bravo_callable.tickets);
    }

    #[test]
    fn standalone_terminals_are_atomic_distinct_nested_and_query_free() {
        let source = r#"
namespace EqualLeft { export const value: number = 1; }
namespace EqualRight { export const value: number = 1; }
namespace Parent {
  const hidden: number = 1;
  export const fixed: number = 1;
  export namespace Child { export let label: string = "child"; }
}
namespace WithClass {
  export class Box { constructor(public value: number) {} }
}
namespace Dotted.Chain { export const first: number = 1; }
namespace Dotted {
  export namespace Chain { export const second: string = "second"; }
}
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        reset_query_demand_measure();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, interner| {
                let namespace = |name: &str| {
                    binder
                        .namespaces
                        .namespaces()
                        .find(|namespace| namespace.name == name)
                        .unwrap_or_else(|| panic!("{name} namespace"))
                };
                let left = namespace("EqualLeft");
                let right = namespace("EqualRight");
                let left_storage = binder
                    .namespaces
                    .standalone_value_storage(left.id)
                    .expect("left storage");
                let right_storage = binder
                    .namespaces
                    .standalone_value_storage(right.id)
                    .expect("right storage");
                assert_ne!(left_storage, right_storage);
                assert_eq!(
                    binder.standalone_namespace_for_storage(left_storage),
                    Some(left.id)
                );
                let left_ty = decl_types.get(left_storage).expect("left published type");
                let right_ty = decl_types.get(right_storage).expect("right published type");
                assert_eq!(left_ty, right_ty, "equal shapes may hash-cons");

                let parent = namespace("Parent");
                let child = namespace("Child");
                assert_eq!(
                    binder.namespaces.aggregate_instance_state(parent.id),
                    Some(NamespaceInstanceState::Instantiated)
                );
                let StandaloneNamespaceTerminal::Ready { ty: parent_ty, .. } = registry
                    .standalone_terminal(parent.id)
                    .expect("parent terminal")
                else {
                    panic!("parent must be ready")
                };
                assert!(matches!(
                    registry.standalone_terminal(child.id),
                    Some(StandaloneNamespaceTerminal::Ready { .. })
                ));
                let parent_object = interner
                    .store()
                    .object_type(parent_ty)
                    .expect("parent object");
                assert!(parent_object.property("hidden").is_none());
                assert!(parent_object
                    .property("fixed")
                    .is_some_and(|property| property.readonly));
                assert!(parent_object
                    .property("Child")
                    .is_some_and(|property| !property.readonly));

                let with_class = namespace("WithClass");
                assert!(matches!(
                    registry.standalone_terminal(with_class.id),
                    Some(StandaloneNamespaceTerminal::Ready { .. })
                ));
                let dotted = namespace("Dotted");
                let chain = namespace("Chain");
                let StandaloneNamespaceTerminal::Ready { ty: dotted_ty, .. } = registry
                    .standalone_terminal(dotted.id)
                    .expect("dotted root terminal")
                else {
                    panic!("dotted root must be ready")
                };
                let StandaloneNamespaceTerminal::Ready { ty: chain_ty, .. } = registry
                    .standalone_terminal(chain.id)
                    .expect("dotted child terminal")
                else {
                    panic!("dotted child must be ready")
                };
                let dotted_object = interner
                    .store()
                    .object_type(dotted_ty)
                    .expect("dotted root object");
                assert_eq!(
                    dotted_object
                        .properties
                        .iter()
                        .filter(|property| property.name == "Chain")
                        .count(),
                    1,
                    "repeated dotted child members stage one dependency"
                );
                assert_eq!(
                    dotted_object.property("Chain").map(|property| property.ty),
                    Some(chain_ty)
                );
                let chain_object = interner
                    .store()
                    .object_type(chain_ty)
                    .expect("dotted child object");
                assert!(chain_object.property("first").is_some());
                assert!(chain_object.property("second").is_some());
                assert_eq!(registry.standalone_query_root_calls(), 0);
            },
        );
        assert!(result.diagnostics.is_empty());
        assert!(result.incomplete.is_empty());
    }

    #[test]
    fn unavailable_member_withholds_the_whole_root_slot() {
        let source = "declare function source(): number; namespace Unavailable { export const value = source(); } const alias = Unavailable;";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, _| {
                let namespace = binder
                    .namespaces
                    .namespaces()
                    .find(|namespace| namespace.name == "Unavailable")
                    .expect("unavailable namespace");
                let storage = binder
                    .namespaces
                    .standalone_value_storage(namespace.id)
                    .expect("reserved root storage");
                assert!(matches!(
                    registry.standalone_terminal(namespace.id),
                    Some(StandaloneNamespaceTerminal::Unavailable { .. })
                ));
                assert_eq!(decl_types.get(storage), None);
            },
        );
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.incomplete.len(), 1);
        assert_eq!(
            result.incomplete[0].id,
            "decl/variable-declaration/namespace-payload-inferred-initializer"
        );
    }

    #[test]
    fn static_root_cycle_withholds_the_namespace_terminal() {
        let source =
            "namespace Cyclic { export class Box { static root = Cyclic; } } const alias = Cyclic;";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, _| {
                let namespace = binder
                    .namespaces
                    .namespaces()
                    .find(|namespace| namespace.name == "Cyclic")
                    .expect("cyclic namespace");
                let storage = binder
                    .namespaces
                    .standalone_value_storage(namespace.id)
                    .expect("cyclic root storage");
                assert!(matches!(
                    registry.standalone_terminal(namespace.id),
                    Some(StandaloneNamespaceTerminal::Unavailable { .. })
                ));
                assert_eq!(decl_types.get(storage), None);
            },
        );
        assert!(result.diagnostics.is_empty());
        assert_eq!(
            result
                .incomplete
                .iter()
                .filter(|incomplete| {
                    incomplete.id == "decl/class-declaration/namespace-payload-static-cycle"
                })
                .count(),
            1
        );
    }

    #[test]
    fn qualified_annotation_failures_withhold_the_root_atomically() {
        let source = r#"
namespace QualifiedFailure {
  export const good: number = 1;
  export const brokenA: QualifiedFailure.MissingA = 1;
  export const brokenB: QualifiedFailure.MissingB = 1;
}
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, _| {
                let namespace = binder
                    .namespaces
                    .namespaces()
                    .find(|namespace| namespace.name == "QualifiedFailure")
                    .expect("qualified failure namespace");
                let storage = binder
                    .namespaces
                    .standalone_value_storage(namespace.id)
                    .expect("qualified failure storage");
                assert!(matches!(
                    registry.standalone_terminal(namespace.id),
                    Some(StandaloneNamespaceTerminal::Unavailable { .. })
                ));
                assert_eq!(decl_types.get(storage), None);
            },
        );
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2694", "TK2694"]
        );
        assert!(result.incomplete.is_empty());
    }

    #[test]
    fn export_alias_dependencies_wait_for_nested_and_private_callable_surfaces() {
        let source = r#"
declare namespace AliasDependencies {
  namespace HiddenChild { const value: number; }
  export { HiddenChild as Child };
  function hiddenCall(value: number): number;
  export { hiddenCall as call };
  const hiddenFixed: number;
  export { hiddenFixed as fixed };
}
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, interner| {
                let namespace = |name: &str| {
                    binder
                        .namespaces
                        .namespaces()
                        .find(|namespace| namespace.name == name)
                        .unwrap_or_else(|| panic!("{name} namespace"))
                };
                let root = namespace("AliasDependencies");
                let child = namespace("HiddenChild");
                let StandaloneNamespaceTerminal::Ready {
                    storage: root_storage,
                    ty: root_ty,
                } = registry
                    .standalone_terminal(root.id)
                    .expect("alias dependency terminal")
                else {
                    panic!("alias dependency root must be ready")
                };
                let StandaloneNamespaceTerminal::Ready { ty: child_ty, .. } = registry
                    .standalone_terminal(child.id)
                    .expect("nested alias target terminal")
                else {
                    panic!("nested alias target must be ready")
                };
                assert_eq!(decl_types.get(root_storage), Some(root_ty));
                let object = interner
                    .store()
                    .object_type(root_ty)
                    .expect("alias dependency object");
                assert_eq!(
                    object.property("Child").map(|property| property.ty),
                    Some(child_ty)
                );
                assert!(object.property("call").is_some_and(|property| {
                    interner.store().tag(property.ty) == crate::types::repr::TypeTag::Function
                }));
                assert!(object
                    .property("fixed")
                    .is_some_and(|property| !property.readonly));
            },
        );
        assert!(result.diagnostics.is_empty());
        assert!(result.incomplete.is_empty());
    }

    #[test]
    fn legal_owner_namespace_pair_does_not_exempt_a_third_value() {
        let source = r#"
namespace DuplicateLegality {
  export function Owner(): void {}
  export namespace Owner { export const tag: number = 1; }
  export const Owner: number = 1;
}
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, _| {
                let namespace = binder
                    .namespaces
                    .namespaces()
                    .find(|namespace| namespace.name == "DuplicateLegality")
                    .expect("duplicate legality namespace");
                let storage = binder
                    .namespaces
                    .standalone_value_storage(namespace.id)
                    .expect("duplicate legality storage");
                assert!(matches!(
                    registry.standalone_terminal(namespace.id),
                    Some(StandaloneNamespaceTerminal::Unavailable { .. })
                ));
                assert_eq!(decl_types.get(storage), None);
            },
        );
        assert!(result.diagnostics.is_empty());
        assert_eq!(
            result
                .incomplete
                .iter()
                .map(|incomplete| incomplete.id.as_str())
                .collect::<Vec<_>>(),
            ["decl/variable-declaration/namespace-payload-duplicate-value"]
        );
    }

    #[test]
    fn class_dependencies_require_a_published_class_surface() {
        let source = r#"
declare function makeValue(): number;
namespace ReadyClassDependency {
  export class Box { static value: number = 1; }
}
namespace PoisonedClassDependency {
  export class Box { static value = makeValue(); }
}
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, _| {
                let namespace = |name: &str| {
                    binder
                        .namespaces
                        .namespaces()
                        .find(|namespace| namespace.name == name)
                        .unwrap_or_else(|| panic!("{name} namespace"))
                };
                let ready = namespace("ReadyClassDependency");
                let poisoned = namespace("PoisonedClassDependency");
                let ready_storage = binder
                    .namespaces
                    .standalone_value_storage(ready.id)
                    .expect("ready class namespace storage");
                let poisoned_storage = binder
                    .namespaces
                    .standalone_value_storage(poisoned.id)
                    .expect("poisoned class namespace storage");
                assert!(matches!(
                    registry.standalone_terminal(ready.id),
                    Some(StandaloneNamespaceTerminal::Ready { .. })
                ));
                assert!(decl_types.get(ready_storage).is_some());
                assert!(matches!(
                    registry.standalone_terminal(poisoned.id),
                    Some(StandaloneNamespaceTerminal::Unavailable { .. })
                ));
                assert_eq!(decl_types.get(poisoned_storage), None);
            },
        );
        assert!(result.diagnostics.is_empty());
        assert_eq!(
            result
                .incomplete
                .iter()
                .map(|incomplete| incomplete.id.as_str())
                .collect::<Vec<_>>(),
            ["class/property-definition/initializer-inference"]
        );
    }

    #[test]
    fn static_root_cycle_detection_respects_annotations_and_binding_shadowing() {
        let source = r#"
namespace AnnotatedStatic {
  export class Box { static root: unknown = AnnotatedStatic; }
}
namespace ShadowedStatic {
  export class Box {
    static project: (ShadowedStatic: number) => number =
      (ShadowedStatic: number): number => ShadowedStatic;
  }
}
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, _| {
                for name in ["AnnotatedStatic", "ShadowedStatic"] {
                    let namespace = binder
                        .namespaces
                        .namespaces()
                        .find(|namespace| namespace.name == name)
                        .unwrap_or_else(|| panic!("{name} namespace"));
                    let storage = binder
                        .namespaces
                        .standalone_value_storage(namespace.id)
                        .unwrap_or_else(|| panic!("{name} storage"));
                    assert!(matches!(
                        registry.standalone_terminal(namespace.id),
                        Some(StandaloneNamespaceTerminal::Ready { .. })
                    ));
                    assert!(decl_types.get(storage).is_some());
                }
            },
        );
        assert!(result.diagnostics.is_empty());
        assert!(result.incomplete.is_empty());
    }

    #[test]
    fn namespace_using_forms_have_atomic_parsed_terminals() {
        let source = r#"
namespace PrivateUsing {
  using hidden = null;
  export const visible: number = 1;
}
namespace AwaitUsing {
  await using value = null;
}
declare namespace AmbientUsing {
  using value = null;
}
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, interner| {
                let namespace = |name: &str| {
                    binder
                        .namespaces
                        .namespaces()
                        .find(|namespace| namespace.name == name)
                        .unwrap_or_else(|| panic!("{name} namespace"))
                };
                let private = namespace("PrivateUsing");
                let StandaloneNamespaceTerminal::Ready {
                    storage: private_storage,
                    ty: private_ty,
                } = registry
                    .standalone_terminal(private.id)
                    .expect("private using terminal")
                else {
                    panic!("private using namespace must be ready")
                };
                assert_eq!(decl_types.get(private_storage), Some(private_ty));
                let object = interner
                    .store()
                    .object_type(private_ty)
                    .expect("private using namespace object");
                assert!(object.property("visible").is_some());
                assert!(object.property("hidden").is_none());

                for name in ["AwaitUsing", "AmbientUsing"] {
                    let namespace = namespace(name);
                    let storage = binder
                        .namespaces
                        .standalone_value_storage(namespace.id)
                        .unwrap_or_else(|| panic!("{name} storage"));
                    assert!(matches!(
                        registry.standalone_terminal(namespace.id),
                        Some(StandaloneNamespaceTerminal::Unavailable { .. })
                    ));
                    assert_eq!(decl_types.get(storage), None);
                }
            },
        );
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2852", "TK1545"]
        );
        assert!(result.incomplete.is_empty());
    }

    #[test]
    fn unavailable_attachments_still_consume_and_check_every_fragment_body() {
        let source = r#"
function UnavailableOwner(): void {}
namespace UnavailableOwner {
  export enum Mode { One }
  export const bad: number = "bad";
  function hidden(): number { return "bad"; }
}
"#;
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2322", "TK2322"]
        );
        assert_eq!(
            output
                .incomplete
                .iter()
                .map(|incomplete| incomplete.id.as_str())
                .collect::<Vec<_>>(),
            ["decl/enum-declaration/namespace-payload-unavailable"]
        );
    }

    #[test]
    fn private_unsupported_declarations_keep_exact_checks_off_the_ready_payload() {
        let source = r#"
namespace PrivateImportSource { export const value: number = 1; }
namespace PrivateUnsupported {
  export const ready: number = 1;
  enum HiddenMode { A }
  import HiddenAlias = PrivateImportSource;
}
const ready: number = PrivateUnsupported.ready;
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        let mut interner = Interner::with_intrinsics();
        let result = check_program_with_namespace_value_inspector(
            &mut interner,
            &parsed.program,
            |binder, registry, decl_types, interner| {
                let namespace = binder
                    .namespaces
                    .namespaces()
                    .find(|namespace| namespace.name == "PrivateUnsupported")
                    .expect("private unsupported namespace");
                let StandaloneNamespaceTerminal::Ready { storage, ty } = registry
                    .standalone_terminal(namespace.id)
                    .expect("private unsupported terminal")
                else {
                    panic!("private unsupported declarations must not poison the root")
                };
                assert_eq!(decl_types.get(storage), Some(ty));
                let object = interner
                    .store()
                    .object_type(ty)
                    .expect("private unsupported namespace object");
                assert!(object.property("ready").is_some());
                assert!(object.property("HiddenMode").is_none());
                assert!(object.property("HiddenAlias").is_none());
            },
        );
        assert!(result.diagnostics.is_empty());
        assert_eq!(
            result
                .incomplete
                .iter()
                .map(|incomplete| incomplete.id.as_str())
                .collect::<Vec<_>>(),
            ["decl/enum-declaration/self", "decl/import-equals/self"]
        );
    }

    #[test]
    fn private_values_do_not_participate_in_owner_payloads() {
        assert!(!namespace_member_participates_in_payload(
            true,
            NamespacePublication::Private
        ));
        assert!(namespace_member_participates_in_payload(
            true,
            NamespacePublication::Explicit
        ));
        assert!(!namespace_member_participates_in_payload(
            false,
            NamespacePublication::Explicit
        ));
    }

    #[test]
    fn private_classes_check_bodies_without_joining_function_or_class_owner_payloads() {
        for owner in ["function Owner(): void {}", "class Owner {}"] {
            let source = format!(
                "{owner}\n\
                 namespace Owner {{\n\
                   export const tag: string = \"tag\";\n\
                   class Hidden {{\n\
                     field: number = \"bad\";\n\
                     method(): number {{ return \"bad\"; }}\n\
                   }}\n\
                 }}\n\
                 const tag: string = Owner.tag;\n\
                 const wrong: number = Owner.tag;\n\
                 Owner.Hidden;\n"
            );
            let output = check_source(&source);
            assert!(output.parse_errors.is_empty(), "{owner}");
            assert!(output.incomplete.is_empty(), "{owner}");
            let mut codes = output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>();
            codes.sort_unstable();
            assert_eq!(codes, ["TK2322", "TK2322", "TK2322", "TK2339"], "{owner}");
        }
    }

    #[test]
    fn ambient_namespace_signatures_are_declaration_only_but_nonambient_signatures_are_not() {
        let source = r#"
declare function AmbientOwner(): void;
declare namespace AmbientOwner {
  function g(value: number): number;
  function g(value: string): string;
}

function NonAmbientOwner(): void {}
namespace NonAmbientOwner {
  export function missing(value: number): number;
}
"#;
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert!(output.incomplete.is_empty());
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2391"]
        );
    }

    #[test]
    fn unsupported_payload_kinds_keep_exact_incomplete_ids() {
        assert_eq!(
            namespace_payload_unavailable(MergeDeclarationKind::Enum)
                .expect("enum has one exact incomplete kind")
                .0,
            "decl/enum-declaration/namespace-payload-unavailable"
        );
        assert_eq!(
            namespace_payload_unavailable(MergeDeclarationKind::ImportAlias)
                .expect("import alias has one exact incomplete kind")
                .0,
            "decl/import-equals/namespace-payload-unavailable"
        );
        assert_eq!(
            prepared_namespace_value_kind(MergeDeclarationKind::Variable),
            Some(PreparedNamespaceValueKind::Variable)
        );
        assert_eq!(
            prepared_namespace_value_kind(MergeDeclarationKind::Enum),
            None
        );
        assert_eq!(
            namespace_payload_unavailable(MergeDeclarationKind::Variable),
            None
        );
    }

    #[test]
    fn duplicate_payload_properties_allow_only_function_overloads() {
        assert_eq!(
            duplicate_property_kind(
                PreparedNamespaceValueKind::Function,
                PreparedNamespaceValueKind::Function
            ),
            None
        );
        assert_eq!(
            duplicate_property_kind(
                PreparedNamespaceValueKind::Variable,
                PreparedNamespaceValueKind::Variable
            ),
            Some(PreparedNamespaceValueKind::Variable)
        );
        assert_eq!(
            duplicate_property_kind(
                PreparedNamespaceValueKind::Variable,
                PreparedNamespaceValueKind::Function
            ),
            Some(PreparedNamespaceValueKind::Function)
        );
        assert_eq!(
            namespace_payload_duplicate(PreparedNamespaceValueKind::Function).0,
            "decl/function-declaration/namespace-payload-duplicate-value"
        );
        assert_eq!(
            namespace_payload_duplicate(PreparedNamespaceValueKind::Class).0,
            "decl/class-declaration/namespace-payload-duplicate-value"
        );
    }
}
