//! Test-only codec for the AST-free binder prefix.
//!
//! Nothing in production reads or writes this wire form since the shipped library snapshot
//! was removed; the reference-record projections it also hosts back binder integrity specs.

use super::bind::Binder;
use super::declaration::{
    DeclId, DeclarationKind, DeclarationSite, DeclarationTable, LexicalDeclaration,
    TypeFragmentKind, TypeGroup, TypeGroupFragment, TypeGroupId, TypeGroupTable, ValueStorageId,
};
use super::namespace::*;
use super::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};
use super::symbol::{Symbol, SymbolId, SymbolTable};
use crate::snapshot_codec::SnapshotWriter;
use crate::snapshot_codec::{SnapshotCodecError, SnapshotReader};
use crate::source::{CompilationOrigin, LibraryFileOrdinal, OriginalModuleOrdinal};
use crate::span::Span;
use rustc_hash::FxHashMap;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ops::Range;

const BINDER_SNAPSHOT_VERSION: u32 = 2;

type SnapshotReferenceRecord = (u8, u8, u8, u32, u32);

#[derive(Copy, Clone, PartialEq, Eq)]
enum ReferenceView {
    Full,
    #[cfg(test)]
    Local,
}

impl ReferenceView {
    const fn local_only(self) -> bool {
        match self {
            Self::Full => false,
            #[cfg(test)]
            Self::Local => true,
        }
    }
}

// Shared snapshot reference domains. These discriminants are append-only.
const REF_SCOPE: u8 = 4;
const REF_SYMBOL: u8 = 5;
const REF_DECLARATION: u8 = 6;
const REF_TYPE_GROUP: u8 = 7;
const REF_NAMESPACE: u8 = 8;
const REF_VALUE_STORAGE: u8 = 9;
const REF_SOURCE_UNIT: u8 = 10;
const REF_NAMESPACE_FRAGMENT: u8 = 11;
const REF_NAMESPACE_MEMBER: u8 = 12;
const REF_GLOBAL_AUGMENTATION: u8 = 13;
const REF_DEFERRED_MODULE: u8 = 14;
const REF_EXPORT_CONTEXT: u8 = 15;
const REF_ROOT_ROW: u8 = 17;
const REF_MERGE_PLACEMENT: u8 = 18;
const REF_DEFERRED_CHILD: u8 = 19;
const REF_UMD_EXPORT: u8 = 20;
const REF_DECLARATION_SITE_INDEX: u8 = 21;
const REF_NAMESPACE_KEY_INDEX: u8 = 22;
const REF_CANONICAL_NAMESPACE_INDEX: u8 = 23;
const REF_CANONICAL_SOURCE_UNIT_INDEX: u8 = 24;
const REF_CANONICAL_GLOBAL_INDEX: u8 = 25;
const REF_CANONICAL_DEFERRED_MODULE_INDEX: u8 = 26;
const REF_CANONICAL_DEFERRED_CHILD_INDEX: u8 = 27;
const REF_CANONICAL_UMD_EXPORT_INDEX: u8 = 28;
const REF_CANONICAL_EXPORT_CONTEXT_INDEX: u8 = 29;

// Field discriminants are local to an owner domain and append-only.
const ROOT_MODULE: u8 = 1;
const ROOT_PRELUDE_MODULE: u8 = 2;
const ROOT_COMPILATION_GLOBAL: u8 = 3;
const ROOT_SCRIPT_NAMESPACE: u8 = 4;
const ROOT_NAMESPACE_COMPILATION_GLOBAL: u8 = 5;
const ROOT_NAMESPACE_SCRIPT_NAMESPACE: u8 = 6;

const SCOPE_PARENT: u8 = 1;
const SCOPE_NAMESPACE_PUBLIC: u8 = 2;
const SCOPE_LOCAL_SYMBOL: u8 = 3;
const SCOPE_MODULE_SOURCE: u8 = 4;

const SYMBOL_VALUE: u8 = 1;
const SYMBOL_FUNCTION_VALUE: u8 = 2;
const SYMBOL_TYPE_GROUP: u8 = 3;
const SYMBOL_NAMESPACE: u8 = 4;
const SYMBOL_DECLARATION: u8 = 5;

const DECLARATION_MODULE: u8 = 1;
const DECLARATION_SCOPE: u8 = 2;
const DECLARATION_VALUE: u8 = 3;
const DECLARATION_TYPE_GROUP: u8 = 4;
const DECLARATION_NAMESPACE: u8 = 5;

const TYPE_GROUP_FRAGMENT_DECLARATION: u8 = 1;
const TYPE_GROUP_FRAGMENT_SOURCE: u8 = 2;
const TYPE_GROUP_FRAGMENT_SCOPE: u8 = 3;
const TYPE_GROUP_FRAGMENT_SITE_MODULE: u8 = 4;
const TYPE_GROUP_FRAGMENT_SITE_SCOPE: u8 = 5;

const NAMESPACE_OWNER: u8 = 1;
const NAMESPACE_PUBLIC_SCOPE: u8 = 2;
const NAMESPACE_SYMBOL: u8 = 3;
const NAMESPACE_FRAGMENT: u8 = 4;
const NAMESPACE_STANDALONE_VALUE: u8 = 5;

const FRAGMENT_NAMESPACE: u8 = 1;
const FRAGMENT_DECLARATION: u8 = 2;
const FRAGMENT_SOURCE: u8 = 3;
const FRAGMENT_MODULE: u8 = 4;
const FRAGMENT_PRIVATE_SCOPE: u8 = 5;
const FRAGMENT_LEXICAL_PARENT: u8 = 6;
const FRAGMENT_PUBLIC_SCOPE: u8 = 7;
const FRAGMENT_MEMBER: u8 = 8;

const MEMBER_OWNER: u8 = 1;
const MEMBER_TARGET: u8 = 2;
const MEMBER_DECLARATION: u8 = 3;
const MEMBER_SYMBOL: u8 = 4;
const MEMBER_LOCAL_SYMBOL: u8 = 5;
const MEMBER_SOURCE: u8 = 6;
const MEMBER_EXPORT_CONTEXT: u8 = 7;

const PLACEMENT_OWNER: u8 = 1;
const PLACEMENT_DECLARATION: u8 = 2;
const PLACEMENT_SOURCE: u8 = 3;
const PLACEMENT_NAMESPACE_FRAGMENT: u8 = 4;
const PLACEMENT_ISSUE_DECLARATION: u8 = 5;
const PLACEMENT_ISSUE_SOURCE: u8 = 6;

const GLOBAL_DECLARATION: u8 = 1;
const GLOBAL_SOURCE: u8 = 2;
const GLOBAL_MODULE: u8 = 3;
const GLOBAL_OWNER: u8 = 4;
const GLOBAL_TARGET_SCOPE: u8 = 5;
const GLOBAL_OVERLAY_SCOPE: u8 = 6;
const GLOBAL_MEMBER: u8 = 7;

const DEFERRED_MODULE_DECLARATION: u8 = 1;
const DEFERRED_MODULE_SOURCE: u8 = 2;
const DEFERRED_MODULE_SCOPE: u8 = 3;
const DEFERRED_MODULE_OWNER: u8 = 4;

const DEFERRED_CHILD_MODULE: u8 = 1;
const DEFERRED_CHILD_DECLARATION: u8 = 2;
const DEFERRED_CHILD_SOURCE: u8 = 3;

const UMD_DECLARATION: u8 = 1;
const UMD_SOURCE: u8 = 2;
const UMD_MODULE: u8 = 3;
const UMD_OWNER: u8 = 4;

const EXPORT_CONTEXT_OWNER: u8 = 1;
const EXPORT_CONTEXT_SOURCE: u8 = 2;
const EXPORT_CONTEXT_MEMBER: u8 = 3;

const SOURCE_UNIT_MODULE: u8 = 1;

const DECLARATION_SITE_KEY_SCOPE: u8 = 1;
const DECLARATION_SITE_TARGET: u8 = 2;
const NAMESPACE_KEY_OWNER: u8 = 1;
const NAMESPACE_KEY_TARGET: u8 = 2;
const CANONICAL_INDEX_TARGET: u8 = 1;

fn push_reference(
    records: &mut Vec<SnapshotReferenceRecord>,
    owner_domain: u8,
    target_domain: u8,
    field: u8,
    owner: u32,
    target: u32,
) {
    records.push((owner_domain, target_domain, field, owner, target));
}

fn push_namespace_owner_reference(
    records: &mut Vec<SnapshotReferenceRecord>,
    owner_domain: u8,
    field: u8,
    owner_id: u32,
    owner: NamespaceOwner,
    compilation_global: ScopeId,
) {
    let (target_domain, target) = match owner {
        NamespaceOwner::Lexical(scope) => (REF_SCOPE, scope.0),
        NamespaceOwner::NamespacePublic(namespace) => (REF_NAMESPACE, namespace.0),
        NamespaceOwner::FragmentPrivate(fragment) => (REF_NAMESPACE_FRAGMENT, fragment.0),
        NamespaceOwner::CompilationGlobal => (REF_SCOPE, compilation_global.0),
    };
    push_reference(
        records,
        owner_domain,
        target_domain,
        field,
        owner_id,
        target,
    );
}

fn push_declaration_owner_reference(
    records: &mut Vec<SnapshotReferenceRecord>,
    owner_domain: u8,
    field: u8,
    owner_id: u32,
    owner: DeclarationOwner,
    compilation_global: ScopeId,
) {
    let (target_domain, target) = match owner {
        DeclarationOwner::Lexical(scope) => (REF_SCOPE, scope.0),
        DeclarationOwner::NamespacePublic(namespace) => (REF_NAMESPACE, namespace.0),
        DeclarationOwner::NamespacePrivate(fragment) => (REF_NAMESPACE_FRAGMENT, fragment.0),
        DeclarationOwner::CompilationGlobal => (REF_SCOPE, compilation_global.0),
        DeclarationOwner::DeferredAmbientModule(module) => (REF_DEFERRED_MODULE, module.0),
    };
    push_reference(
        records,
        owner_domain,
        target_domain,
        field,
        owner_id,
        target,
    );
}

fn push_canonical_index_references(
    records: &mut Vec<SnapshotReferenceRecord>,
    owner_domain: u8,
    target_domain: u8,
    owner_start: usize,
    targets: impl IntoIterator<Item = u32>,
) -> Result<(), SnapshotCodecError> {
    for (index, target) in targets.into_iter().enumerate() {
        let owner = u32::try_from(owner_start + index)
            .map_err(|_| SnapshotCodecError::invalid(0, "canonical index exceeds u32"))?;
        push_reference(
            records,
            owner_domain,
            target_domain,
            CANONICAL_INDEX_TARGET,
            owner,
            target,
        );
    }
    Ok(())
}

/// Enumerate every typed binder reference without consulting snapshot wire bytes.
pub(crate) fn snapshot_reference_records(
    binder: &Binder,
) -> Result<Vec<SnapshotReferenceRecord>, SnapshotCodecError> {
    snapshot_reference_records_with_view(binder, ReferenceView::Full)
}

fn snapshot_reference_records_with_view(
    binder: &Binder,
    view: ReferenceView,
) -> Result<Vec<SnapshotReferenceRecord>, SnapshotCodecError> {
    let mut records = Vec::new();
    let root = 0;
    let root_scope_is_in_view = |_scope: ScopeId| {
        if !view.local_only() {
            return true;
        }
        #[cfg(test)]
        {
            _scope.index() >= binder.graph.base_len_for_test()
        }
        #[cfg(not(test))]
        unreachable!("local reference view is test-only")
    };
    for (field, scope) in [
        (ROOT_MODULE, binder.module),
        (ROOT_PRELUDE_MODULE, binder.prelude_module),
        (ROOT_COMPILATION_GLOBAL, binder.compilation_global),
        (ROOT_SCRIPT_NAMESPACE, binder.script_namespace_root),
    ] {
        if root_scope_is_in_view(scope) {
            push_reference(&mut records, REF_ROOT_ROW, REF_SCOPE, field, root, scope.0);
        }
    }

    let mut record_scope = |owner: u32, scope: &Scope| {
        if let Some(parent) = scope.parent {
            push_reference(
                &mut records,
                REF_SCOPE,
                REF_SCOPE,
                SCOPE_PARENT,
                owner,
                parent.0,
            );
        }
        if let Some(public) = scope.namespace_public {
            push_reference(
                &mut records,
                REF_SCOPE,
                REF_SCOPE,
                SCOPE_NAMESPACE_PUBLIC,
                owner,
                public.0,
            );
        }
        for symbol in scope.symbols.values() {
            push_reference(
                &mut records,
                REF_SCOPE,
                REF_SYMBOL,
                SCOPE_LOCAL_SYMBOL,
                owner,
                symbol.0,
            );
        }
    };
    if view.local_only() {
        #[cfg(test)]
        for (owner, scope) in binder.graph.local_scopes() {
            record_scope(owner.0, scope);
        }
    } else {
        for (index, scope) in binder.graph.snapshot_scopes().enumerate() {
            let owner = u32::try_from(index)
                .map_err(|_| SnapshotCodecError::invalid(0, "scope index exceeds u32"))?;
            record_scope(owner, scope);
        }
    }
    if view.local_only() {
        #[cfg(test)]
        for (scope, source) in binder.snapshot_module_sources().local_iter() {
            push_reference(
                &mut records,
                REF_SCOPE,
                REF_SOURCE_UNIT,
                SCOPE_MODULE_SOURCE,
                scope.0,
                source.0,
            );
        }
    } else {
        for (scope, source) in binder.snapshot_module_sources().iter() {
            push_reference(
                &mut records,
                REF_SCOPE,
                REF_SOURCE_UNIT,
                SCOPE_MODULE_SOURCE,
                scope.0,
                source.0,
            );
        }
    }

    let mut record_symbol = |owner: u32, symbol: &Symbol| {
        if let Some(value) = symbol.value {
            push_reference(
                &mut records,
                REF_SYMBOL,
                REF_VALUE_STORAGE,
                SYMBOL_VALUE,
                owner,
                value.0,
            );
        }
        for value in &symbol.function_values {
            push_reference(
                &mut records,
                REF_SYMBOL,
                REF_VALUE_STORAGE,
                SYMBOL_FUNCTION_VALUE,
                owner,
                value.0,
            );
        }
        if let Some(group) = symbol.ty {
            push_reference(
                &mut records,
                REF_SYMBOL,
                REF_TYPE_GROUP,
                SYMBOL_TYPE_GROUP,
                owner,
                group.0,
            );
        }
        if let Some(namespace) = symbol.ns {
            push_reference(
                &mut records,
                REF_SYMBOL,
                REF_NAMESPACE,
                SYMBOL_NAMESPACE,
                owner,
                namespace.0,
            );
        }
        for declaration in &symbol.declarations {
            push_reference(
                &mut records,
                REF_SYMBOL,
                REF_DECLARATION,
                SYMBOL_DECLARATION,
                owner,
                declaration.0,
            );
        }
    };
    if view.local_only() {
        #[cfg(test)]
        for (owner, symbol) in binder.symbols.local_symbols() {
            record_symbol(owner.0, symbol);
        }
    } else {
        for (index, symbol) in binder.symbols.snapshot_symbols().enumerate() {
            let owner = u32::try_from(index)
                .map_err(|_| SnapshotCodecError::invalid(0, "symbol index exceeds u32"))?;
            record_symbol(owner, symbol);
        }
    }

    let declarations: Box<dyn Iterator<Item = &LexicalDeclaration>> = if view.local_only() {
        #[cfg(test)]
        {
            Box::new(binder.declarations.local_declarations())
        }
        #[cfg(not(test))]
        unreachable!("local reference view is test-only")
    } else {
        Box::new(binder.declarations.iter())
    };
    for declaration in declarations {
        let owner = declaration.id.0;
        push_reference(
            &mut records,
            REF_DECLARATION_SITE_INDEX,
            REF_SCOPE,
            DECLARATION_SITE_KEY_SCOPE,
            owner,
            declaration.site.module.0,
        );
        push_reference(
            &mut records,
            REF_DECLARATION_SITE_INDEX,
            REF_DECLARATION,
            DECLARATION_SITE_TARGET,
            owner,
            declaration.id.0,
        );
        push_reference(
            &mut records,
            REF_DECLARATION,
            REF_SCOPE,
            DECLARATION_MODULE,
            owner,
            declaration.site.module.0,
        );
        if let Some(scope) = declaration.site.scope {
            push_reference(
                &mut records,
                REF_DECLARATION,
                REF_SCOPE,
                DECLARATION_SCOPE,
                owner,
                scope.0,
            );
        }
        if let Some(value) = declaration.value_storage {
            push_reference(
                &mut records,
                REF_DECLARATION,
                REF_VALUE_STORAGE,
                DECLARATION_VALUE,
                owner,
                value.0,
            );
        }
        if let Some(group) = declaration.type_group {
            push_reference(
                &mut records,
                REF_DECLARATION,
                REF_TYPE_GROUP,
                DECLARATION_TYPE_GROUP,
                owner,
                group.0,
            );
        }
        if let Some(namespace) = declaration.namespace {
            push_reference(
                &mut records,
                REF_DECLARATION,
                REF_NAMESPACE,
                DECLARATION_NAMESPACE,
                owner,
                namespace.0,
            );
        }
    }

    let groups: Box<dyn Iterator<Item = &TypeGroup>> = if view.local_only() {
        #[cfg(test)]
        {
            Box::new(binder.type_groups.local_groups())
        }
        #[cfg(not(test))]
        unreachable!("local reference view is test-only")
    } else {
        Box::new(binder.type_groups.iter())
    };
    for group in groups {
        for fragment in &group.fragments {
            let owner = group.id.0;
            push_reference(
                &mut records,
                REF_TYPE_GROUP,
                REF_DECLARATION,
                TYPE_GROUP_FRAGMENT_DECLARATION,
                owner,
                fragment.declaration.0,
            );
            push_reference(
                &mut records,
                REF_TYPE_GROUP,
                REF_SOURCE_UNIT,
                TYPE_GROUP_FRAGMENT_SOURCE,
                owner,
                fragment.source.0,
            );
            push_reference(
                &mut records,
                REF_TYPE_GROUP,
                REF_SCOPE,
                TYPE_GROUP_FRAGMENT_SCOPE,
                owner,
                fragment.scope.0,
            );
            push_reference(
                &mut records,
                REF_TYPE_GROUP,
                REF_SCOPE,
                TYPE_GROUP_FRAGMENT_SITE_MODULE,
                owner,
                fragment.site.module.0,
            );
            if let Some(scope) = fragment.site.scope {
                push_reference(
                    &mut records,
                    REF_TYPE_GROUP,
                    REF_SCOPE,
                    TYPE_GROUP_FRAGMENT_SITE_SCOPE,
                    owner,
                    scope.0,
                );
            }
        }
    }

    let namespace_rows = binder.namespaces.snapshot_reference_rows(view.local_only());
    let primary = namespace_rows.primary;
    let offsets = namespace_rows.offsets;
    if let Some(scope) = primary.compilation_global {
        if root_scope_is_in_view(scope) {
            push_reference(
                &mut records,
                REF_ROOT_ROW,
                REF_SCOPE,
                ROOT_NAMESPACE_COMPILATION_GLOBAL,
                root,
                scope.0,
            );
        }
    }
    if let Some(scope) = primary.script_namespace_root {
        if root_scope_is_in_view(scope) {
            push_reference(
                &mut records,
                REF_ROOT_ROW,
                REF_SCOPE,
                ROOT_NAMESPACE_SCRIPT_NAMESPACE,
                root,
                scope.0,
            );
        }
    }

    for (index, namespace) in primary.namespaces.iter().enumerate() {
        let owner = namespace.id.0;
        push_namespace_owner_reference(
            &mut records,
            REF_NAMESPACE_KEY_INDEX,
            NAMESPACE_KEY_OWNER,
            owner,
            namespace.owner,
            binder.compilation_global,
        );
        push_reference(
            &mut records,
            REF_NAMESPACE_KEY_INDEX,
            REF_NAMESPACE,
            NAMESPACE_KEY_TARGET,
            owner,
            namespace.id.0,
        );
        push_namespace_owner_reference(
            &mut records,
            REF_NAMESPACE,
            NAMESPACE_OWNER,
            owner,
            namespace.owner,
            binder.compilation_global,
        );
        push_reference(
            &mut records,
            REF_NAMESPACE,
            REF_SCOPE,
            NAMESPACE_PUBLIC_SCOPE,
            owner,
            namespace.public_scope.0,
        );
        push_reference(
            &mut records,
            REF_NAMESPACE,
            REF_SYMBOL,
            NAMESPACE_SYMBOL,
            owner,
            namespace.symbol.0,
        );
        for fragment in &namespace.fragments {
            push_reference(
                &mut records,
                REF_NAMESPACE,
                REF_NAMESPACE_FRAGMENT,
                NAMESPACE_FRAGMENT,
                owner,
                fragment.0,
            );
        }
        if let Some(storage) = primary.standalone_value_storages[index] {
            push_reference(
                &mut records,
                REF_NAMESPACE,
                REF_VALUE_STORAGE,
                NAMESPACE_STANDALONE_VALUE,
                owner,
                storage.0,
            );
        }
    }

    for fragment in &primary.fragments {
        let owner = fragment.id.0;
        push_reference(
            &mut records,
            REF_NAMESPACE_FRAGMENT,
            REF_NAMESPACE,
            FRAGMENT_NAMESPACE,
            owner,
            fragment.namespace.0,
        );
        push_reference(
            &mut records,
            REF_NAMESPACE_FRAGMENT,
            REF_DECLARATION,
            FRAGMENT_DECLARATION,
            owner,
            fragment.declaration.0,
        );
        push_reference(
            &mut records,
            REF_NAMESPACE_FRAGMENT,
            REF_SOURCE_UNIT,
            FRAGMENT_SOURCE,
            owner,
            fragment.source.0,
        );
        for (field, scope) in [
            (FRAGMENT_MODULE, fragment.module),
            (FRAGMENT_PRIVATE_SCOPE, fragment.private_scope),
            (FRAGMENT_LEXICAL_PARENT, fragment.lexical_parent),
            (FRAGMENT_PUBLIC_SCOPE, fragment.public_scope),
        ] {
            push_reference(
                &mut records,
                REF_NAMESPACE_FRAGMENT,
                REF_SCOPE,
                field,
                owner,
                scope.0,
            );
        }
        for member in &fragment.members {
            push_reference(
                &mut records,
                REF_NAMESPACE_FRAGMENT,
                REF_NAMESPACE_MEMBER,
                FRAGMENT_MEMBER,
                owner,
                member.0,
            );
        }
    }

    for member in &primary.members {
        let owner = member.id.0;
        let (member_owner_domain, member_owner_target) = match member.owner {
            NamespaceMemberOwner::Fragment(fragment) => (REF_NAMESPACE_FRAGMENT, fragment.0),
            NamespaceMemberOwner::GlobalAugmentation(global) => (REF_GLOBAL_AUGMENTATION, global.0),
            NamespaceMemberOwner::DeferredAmbientModule(module) => (REF_DEFERRED_MODULE, module.0),
        };
        push_reference(
            &mut records,
            REF_NAMESPACE_MEMBER,
            member_owner_domain,
            MEMBER_OWNER,
            owner,
            member_owner_target,
        );
        push_declaration_owner_reference(
            &mut records,
            REF_NAMESPACE_MEMBER,
            MEMBER_TARGET,
            owner,
            member.target,
            binder.compilation_global,
        );
        if let Some(declaration) = member.declaration {
            push_reference(
                &mut records,
                REF_NAMESPACE_MEMBER,
                REF_DECLARATION,
                MEMBER_DECLARATION,
                owner,
                declaration.0,
            );
        }
        if let Some(symbol) = member.symbol {
            push_reference(
                &mut records,
                REF_NAMESPACE_MEMBER,
                REF_SYMBOL,
                MEMBER_SYMBOL,
                owner,
                symbol.0,
            );
        }
        if let Some(symbol) = member.local_symbol {
            push_reference(
                &mut records,
                REF_NAMESPACE_MEMBER,
                REF_SYMBOL,
                MEMBER_LOCAL_SYMBOL,
                owner,
                symbol.0,
            );
        }
        push_reference(
            &mut records,
            REF_NAMESPACE_MEMBER,
            REF_SOURCE_UNIT,
            MEMBER_SOURCE,
            owner,
            member.source.0,
        );
        if let Some(context) = member.export_context {
            push_reference(
                &mut records,
                REF_NAMESPACE_MEMBER,
                REF_EXPORT_CONTEXT,
                MEMBER_EXPORT_CONTEXT,
                owner,
                context.0,
            );
        }
    }

    for (index, (placement_owner, _, participants)) in primary.placements.iter().enumerate() {
        let owner = u32::try_from(offsets.placements + index)
            .map_err(|_| SnapshotCodecError::invalid(0, "merge placement index exceeds u32"))?;
        push_declaration_owner_reference(
            &mut records,
            REF_MERGE_PLACEMENT,
            PLACEMENT_OWNER,
            owner,
            *placement_owner,
            binder.compilation_global,
        );
        for participant in participants {
            push_reference(
                &mut records,
                REF_MERGE_PLACEMENT,
                REF_DECLARATION,
                PLACEMENT_DECLARATION,
                owner,
                participant.declaration.0,
            );
            push_reference(
                &mut records,
                REF_MERGE_PLACEMENT,
                REF_SOURCE_UNIT,
                PLACEMENT_SOURCE,
                owner,
                participant.source.0,
            );
            if let Some(fragment) = participant.namespace_fragment {
                push_reference(
                    &mut records,
                    REF_MERGE_PLACEMENT,
                    REF_NAMESPACE_FRAGMENT,
                    PLACEMENT_NAMESPACE_FRAGMENT,
                    owner,
                    fragment.0,
                );
            }
        }
    }
    let merges: Box<dyn Iterator<Item = &MergeRecord>> = if view.local_only() {
        #[cfg(test)]
        {
            Box::new(binder.namespaces.local_merges())
        }
        #[cfg(not(test))]
        unreachable!("local reference view is test-only")
    } else {
        Box::new(binder.namespaces.merges())
    };
    for (index, merge) in merges.enumerate() {
        let owner = u32::try_from(offsets.placements + index)
            .map_err(|_| SnapshotCodecError::invalid(0, "merge placement index exceeds u32"))?;
        for issue in &merge.placement_issues {
            push_reference(
                &mut records,
                REF_MERGE_PLACEMENT,
                REF_DECLARATION,
                PLACEMENT_ISSUE_DECLARATION,
                owner,
                issue.owner.0,
            );
            push_reference(
                &mut records,
                REF_MERGE_PLACEMENT,
                REF_SOURCE_UNIT,
                PLACEMENT_ISSUE_SOURCE,
                owner,
                issue.source.0,
            );
        }
    }

    for global in &primary.globals {
        let owner = global.id.0;
        push_reference(
            &mut records,
            REF_GLOBAL_AUGMENTATION,
            REF_DECLARATION,
            GLOBAL_DECLARATION,
            owner,
            global.declaration.0,
        );
        push_reference(
            &mut records,
            REF_GLOBAL_AUGMENTATION,
            REF_SOURCE_UNIT,
            GLOBAL_SOURCE,
            owner,
            global.source.0,
        );
        push_reference(
            &mut records,
            REF_GLOBAL_AUGMENTATION,
            REF_SCOPE,
            GLOBAL_MODULE,
            owner,
            global.module.0,
        );
        let (target_domain, target) = match global.owner {
            GlobalOwner::Lexical(scope) => (REF_SCOPE, scope.0),
            GlobalOwner::NamespaceFragment(fragment) => (REF_NAMESPACE_FRAGMENT, fragment.0),
            GlobalOwner::DeferredAmbientModule(module) => (REF_DEFERRED_MODULE, module.0),
        };
        push_reference(
            &mut records,
            REF_GLOBAL_AUGMENTATION,
            target_domain,
            GLOBAL_OWNER,
            owner,
            target,
        );
        for (field, scope) in [
            (GLOBAL_TARGET_SCOPE, global.target_scope),
            (GLOBAL_OVERLAY_SCOPE, global.overlay_scope),
        ] {
            push_reference(
                &mut records,
                REF_GLOBAL_AUGMENTATION,
                REF_SCOPE,
                field,
                owner,
                scope.0,
            );
        }
        for member in &global.members {
            push_reference(
                &mut records,
                REF_GLOBAL_AUGMENTATION,
                REF_NAMESPACE_MEMBER,
                GLOBAL_MEMBER,
                owner,
                member.0,
            );
        }
    }

    for module in &primary.deferred_modules {
        let owner = module.id.0;
        push_reference(
            &mut records,
            REF_DEFERRED_MODULE,
            REF_DECLARATION,
            DEFERRED_MODULE_DECLARATION,
            owner,
            module.declaration.0,
        );
        push_reference(
            &mut records,
            REF_DEFERRED_MODULE,
            REF_SOURCE_UNIT,
            DEFERRED_MODULE_SOURCE,
            owner,
            module.source.0,
        );
        push_reference(
            &mut records,
            REF_DEFERRED_MODULE,
            REF_SCOPE,
            DEFERRED_MODULE_SCOPE,
            owner,
            module.module.0,
        );
        push_declaration_owner_reference(
            &mut records,
            REF_DEFERRED_MODULE,
            DEFERRED_MODULE_OWNER,
            owner,
            module.owner,
            binder.compilation_global,
        );
    }

    for (index, child) in primary.deferred_children.iter().enumerate() {
        let owner = u32::try_from(offsets.deferred_children + index)
            .map_err(|_| SnapshotCodecError::invalid(0, "deferred child index exceeds u32"))?;
        push_reference(
            &mut records,
            REF_DEFERRED_CHILD,
            REF_DEFERRED_MODULE,
            DEFERRED_CHILD_MODULE,
            owner,
            child.module.0,
        );
        if let Some(declaration) = child.declaration {
            push_reference(
                &mut records,
                REF_DEFERRED_CHILD,
                REF_DECLARATION,
                DEFERRED_CHILD_DECLARATION,
                owner,
                declaration.0,
            );
        }
        push_reference(
            &mut records,
            REF_DEFERRED_CHILD,
            REF_SOURCE_UNIT,
            DEFERRED_CHILD_SOURCE,
            owner,
            child.source.0,
        );
    }

    for (index, export) in primary.umd_exports.iter().enumerate() {
        let owner = u32::try_from(offsets.umd_exports + index)
            .map_err(|_| SnapshotCodecError::invalid(0, "UMD export index exceeds u32"))?;
        push_reference(
            &mut records,
            REF_UMD_EXPORT,
            REF_DECLARATION,
            UMD_DECLARATION,
            owner,
            export.declaration.0,
        );
        push_reference(
            &mut records,
            REF_UMD_EXPORT,
            REF_SOURCE_UNIT,
            UMD_SOURCE,
            owner,
            export.source.0,
        );
        push_reference(
            &mut records,
            REF_UMD_EXPORT,
            REF_SCOPE,
            UMD_MODULE,
            owner,
            export.module.0,
        );
        push_declaration_owner_reference(
            &mut records,
            REF_UMD_EXPORT,
            UMD_OWNER,
            owner,
            export.owner,
            binder.compilation_global,
        );
    }

    for context in &primary.export_contexts {
        let owner = context.id.0;
        let (target_domain, target) = match context.owner {
            ExportContextOwner::NamespaceFragment(fragment) => (REF_NAMESPACE_FRAGMENT, fragment.0),
            ExportContextOwner::GlobalAugmentation(global) => (REF_GLOBAL_AUGMENTATION, global.0),
            ExportContextOwner::DeferredAmbientModule(module) => (REF_DEFERRED_MODULE, module.0),
        };
        push_reference(
            &mut records,
            REF_EXPORT_CONTEXT,
            target_domain,
            EXPORT_CONTEXT_OWNER,
            owner,
            target,
        );
        push_reference(
            &mut records,
            REF_EXPORT_CONTEXT,
            REF_SOURCE_UNIT,
            EXPORT_CONTEXT_SOURCE,
            owner,
            context.source.0,
        );
        for member in &context.members {
            push_reference(
                &mut records,
                REF_EXPORT_CONTEXT,
                REF_NAMESPACE_MEMBER,
                EXPORT_CONTEXT_MEMBER,
                owner,
                member.0,
            );
        }
    }

    for unit in &primary.source_units {
        push_reference(
            &mut records,
            REF_SOURCE_UNIT,
            REF_SCOPE,
            SOURCE_UNIT_MODULE,
            unit.source.0,
            unit.module.0,
        );
    }

    push_canonical_index_references(
        &mut records,
        REF_CANONICAL_NAMESPACE_INDEX,
        REF_NAMESPACE,
        offsets.canonical_namespaces,
        namespace_rows.canonical_namespaces,
    )?;
    push_canonical_index_references(
        &mut records,
        REF_CANONICAL_SOURCE_UNIT_INDEX,
        REF_SOURCE_UNIT,
        offsets.canonical_source_units,
        namespace_rows.canonical_source_units,
    )?;
    push_canonical_index_references(
        &mut records,
        REF_CANONICAL_GLOBAL_INDEX,
        REF_GLOBAL_AUGMENTATION,
        offsets.canonical_globals,
        namespace_rows.canonical_globals,
    )?;
    push_canonical_index_references(
        &mut records,
        REF_CANONICAL_DEFERRED_MODULE_INDEX,
        REF_DEFERRED_MODULE,
        offsets.canonical_deferred_modules,
        namespace_rows.canonical_deferred_modules,
    )?;
    push_canonical_index_references(
        &mut records,
        REF_CANONICAL_DEFERRED_CHILD_INDEX,
        REF_DEFERRED_CHILD,
        offsets.canonical_deferred_children,
        namespace_rows.canonical_deferred_children,
    )?;
    push_canonical_index_references(
        &mut records,
        REF_CANONICAL_UMD_EXPORT_INDEX,
        REF_UMD_EXPORT,
        offsets.canonical_umd_exports,
        namespace_rows.canonical_umd_exports,
    )?;
    push_canonical_index_references(
        &mut records,
        REF_CANONICAL_EXPORT_CONTEXT_INDEX,
        REF_EXPORT_CONTEXT,
        offsets.canonical_export_contexts,
        namespace_rows.canonical_export_contexts,
    )?;

    records.sort_unstable();
    Ok(records)
}

#[cfg(test)]
pub(crate) fn snapshot_reference_records_for_test(binder: &Binder) -> Vec<SnapshotReferenceRecord> {
    snapshot_reference_records(binder).expect("typed binder references enumerate")
}

#[cfg(test)]
pub(crate) fn local_snapshot_reference_records_for_test(
    binder: &Binder,
) -> Vec<SnapshotReferenceRecord> {
    snapshot_reference_records_with_view(binder, ReferenceView::Local)
        .expect("typed local binder references enumerate")
}

fn invalid(reader: &SnapshotReader<'_>, message: impl Into<String>) -> SnapshotCodecError {
    SnapshotCodecError::invalid(reader.position(), message)
}

fn write_len(writer: &mut SnapshotWriter, len: usize) -> Result<(), SnapshotCodecError> {
    writer.usize(len)
}

fn read_len(reader: &mut SnapshotReader<'_>) -> Result<usize, SnapshotCodecError> {
    reader.collection_len(1)
}

fn write_option<T>(
    writer: &mut SnapshotWriter,
    value: Option<T>,
    write: impl FnOnce(&mut SnapshotWriter, T) -> Result<(), SnapshotCodecError>,
) -> Result<(), SnapshotCodecError> {
    writer.bool(value.is_some());
    if let Some(value) = value {
        write(writer, value)?;
    }
    Ok(())
}

fn read_option<T>(
    reader: &mut SnapshotReader<'_>,
    read: impl FnOnce(&mut SnapshotReader<'_>) -> Result<T, SnapshotCodecError>,
) -> Result<Option<T>, SnapshotCodecError> {
    if reader.bool()? {
        Ok(Some(read(reader)?))
    } else {
        Ok(None)
    }
}

fn write_vec<T>(
    writer: &mut SnapshotWriter,
    values: &[T],
    mut write: impl FnMut(&mut SnapshotWriter, &T) -> Result<(), SnapshotCodecError>,
) -> Result<(), SnapshotCodecError> {
    write_len(writer, values.len())?;
    for value in values {
        write(writer, value)?;
    }
    Ok(())
}

fn read_vec<T>(
    reader: &mut SnapshotReader<'_>,
    mut read: impl FnMut(&mut SnapshotReader<'_>) -> Result<T, SnapshotCodecError>,
) -> Result<Vec<T>, SnapshotCodecError> {
    let len = read_len(reader)?;
    (0..len).map(|_| read(reader)).collect()
}

fn write_scope_id(writer: &mut SnapshotWriter, value: ScopeId) {
    writer.u32(value.0);
}

fn read_scope_id(reader: &mut SnapshotReader<'_>) -> Result<ScopeId, SnapshotCodecError> {
    Ok(ScopeId(reader.u32()?))
}

fn write_symbol_id(writer: &mut SnapshotWriter, value: SymbolId) {
    writer.u32(value.0);
}

fn read_symbol_id(reader: &mut SnapshotReader<'_>) -> Result<SymbolId, SnapshotCodecError> {
    Ok(SymbolId(reader.u32()?))
}

fn write_decl_id(writer: &mut SnapshotWriter, value: DeclId) {
    writer.u32(value.0);
}

fn read_decl_id(reader: &mut SnapshotReader<'_>) -> Result<DeclId, SnapshotCodecError> {
    Ok(DeclId(reader.u32()?))
}

fn write_type_group_id(writer: &mut SnapshotWriter, value: TypeGroupId) {
    writer.u32(value.0);
}

fn read_type_group_id(reader: &mut SnapshotReader<'_>) -> Result<TypeGroupId, SnapshotCodecError> {
    Ok(TypeGroupId(reader.u32()?))
}

fn write_value_storage_id(writer: &mut SnapshotWriter, value: ValueStorageId) {
    writer.u32(value.0);
}

fn read_value_storage_id(
    reader: &mut SnapshotReader<'_>,
) -> Result<ValueStorageId, SnapshotCodecError> {
    Ok(ValueStorageId(reader.u32()?))
}

fn write_span(writer: &mut SnapshotWriter, value: Span) {
    writer.u32(value.start);
    writer.u32(value.end);
}

fn read_span(reader: &mut SnapshotReader<'_>) -> Result<Span, SnapshotCodecError> {
    let start = reader.u32()?;
    let end = reader.u32()?;
    if start > end {
        return Err(invalid(reader, "span start exceeds end"));
    }
    Ok(Span::new(start, end))
}

fn write_source_key(writer: &mut SnapshotWriter, value: SourceUnitKey) {
    writer.u32(value.0);
}

fn read_source_key(reader: &mut SnapshotReader<'_>) -> Result<SourceUnitKey, SnapshotCodecError> {
    Ok(SourceUnitKey(reader.u32()?))
}

fn write_origin(writer: &mut SnapshotWriter, value: CompilationOrigin) {
    match value {
        CompilationOrigin::User(ordinal) => {
            writer.u8(0);
            writer.u64(u64::try_from(ordinal.index()).expect("module ordinal fits u64"));
        }
        CompilationOrigin::Library(ordinal) => {
            writer.u8(1);
            writer.u64(u64::try_from(ordinal.index()).expect("library ordinal fits u64"));
        }
    }
}

fn read_origin(reader: &mut SnapshotReader<'_>) -> Result<CompilationOrigin, SnapshotCodecError> {
    let tag = reader.u8()?;
    let index = usize::try_from(reader.u64()?)
        .map_err(|_| invalid(reader, "source ordinal exceeds usize"))?;
    match tag {
        0 => Ok(CompilationOrigin::User(OriginalModuleOrdinal::new(index))),
        1 => Ok(CompilationOrigin::Library(LibraryFileOrdinal::new(index))),
        _ => Err(invalid(reader, "invalid compilation origin")),
    }
}

macro_rules! fieldless_enum_codec {
    ($write:ident, $read:ident, $ty:ty, [$($variant:path => $tag:expr),+ $(,)?]) => {
        fn $write(writer: &mut SnapshotWriter, value: $ty) {
            let tag = match value {
                $($variant => $tag),+
            };
            writer.u8(tag);
        }

        fn $read(reader: &mut SnapshotReader<'_>) -> Result<$ty, SnapshotCodecError> {
            match reader.u8()? {
                $($tag => Ok($variant),)+
                _ => Err(invalid(reader, concat!("invalid ", stringify!($ty), " discriminant"))),
            }
        }
    };
}

fieldless_enum_codec!(write_scope_kind, read_scope_kind, ScopeKind, [
    ScopeKind::Module => 0,
    ScopeKind::Function => 1,
    ScopeKind::Block => 2,
    ScopeKind::NamespacePublic => 3,
    ScopeKind::NamespacePrivate => 4,
    ScopeKind::CompilationGlobal => 5,
    ScopeKind::ScriptNamespaceRoot => 6,
    ScopeKind::GlobalOverlay => 7,
]);

fieldless_enum_codec!(write_declaration_kind, read_declaration_kind, DeclarationKind, [
    DeclarationKind::Variable => 0,
    DeclarationKind::Function => 1,
    DeclarationKind::Class => 2,
    DeclarationKind::Parameter => 3,
    DeclarationKind::CatchParameter => 4,
    DeclarationKind::Import => 5,
    DeclarationKind::TypeAlias => 6,
    DeclarationKind::Interface => 7,
    DeclarationKind::Enum => 8,
    DeclarationKind::Namespace => 9,
    DeclarationKind::ImportEquals => 10,
    DeclarationKind::NamespaceExport => 11,
    DeclarationKind::Global => 12,
]);

fieldless_enum_codec!(write_fragment_kind, read_fragment_kind, TypeFragmentKind, [
    TypeFragmentKind::TypeAlias => 0,
    TypeFragmentKind::Interface => 1,
    TypeFragmentKind::Class => 2,
]);

fn encode_scopes(
    writer: &mut SnapshotWriter,
    graph: &ScopeGraph,
) -> Result<(), SnapshotCodecError> {
    write_len(writer, graph.snapshot_len())?;
    for scope in graph.snapshot_scopes() {
        write_option(writer, scope.parent, |writer, value| {
            write_scope_id(writer, value);
            Ok(())
        })?;
        write_option(writer, scope.namespace_public, |writer, value| {
            write_scope_id(writer, value);
            Ok(())
        })?;
        write_scope_kind(writer, scope.kind);
        let mut symbols = scope.symbols.iter().collect::<Vec<_>>();
        symbols.sort_by(|left, right| left.0.cmp(right.0));
        write_len(writer, symbols.len())?;
        for (name, symbol) in symbols {
            writer.string(name)?;
            write_symbol_id(writer, *symbol);
        }
    }
    Ok(())
}

fn decode_scopes(reader: &mut SnapshotReader<'_>) -> Result<ScopeGraph, SnapshotCodecError> {
    let scopes = read_vec(reader, |reader| {
        let parent = read_option(reader, read_scope_id)?;
        let namespace_public = read_option(reader, read_scope_id)?;
        let kind = read_scope_kind(reader)?;
        let mut symbols = FxHashMap::default();
        for _ in 0..read_len(reader)? {
            let name = reader.string()?.to_owned();
            let symbol = read_symbol_id(reader)?;
            if symbols.insert(name, symbol).is_some() {
                return Err(invalid(reader, "duplicate scope symbol name"));
            }
        }
        Ok(Scope {
            parent,
            namespace_public,
            kind,
            symbols,
        })
    })?;
    let mut graph = ScopeGraph::new();
    for scope in scopes {
        graph.push(scope);
    }
    Ok(graph)
}

fn encode_symbols(
    writer: &mut SnapshotWriter,
    symbols: &SymbolTable,
) -> Result<(), SnapshotCodecError> {
    write_len(writer, symbols.len())?;
    for symbol in symbols.snapshot_symbols() {
        writer.string(&symbol.name)?;
        write_option(writer, symbol.value, |writer, value| {
            write_value_storage_id(writer, value);
            Ok(())
        })?;
        writer.bool(symbol.blocks_value_lookup);
        write_vec(writer, &symbol.function_values, |writer, value| {
            write_value_storage_id(writer, *value);
            Ok(())
        })?;
        write_option(writer, symbol.ty, |writer, value| {
            write_type_group_id(writer, value);
            Ok(())
        })?;
        writer.bool(symbol.owns_type_group);
        writer.bool(symbol.blocks_type_lookup);
        writer.bool(symbol.blocks_namespace_lookup);
        write_option(writer, symbol.ns, |writer, value| {
            writer.u32(value.0);
            Ok(())
        })?;
        write_vec(writer, &symbol.declarations, |writer, value| {
            write_decl_id(writer, *value);
            Ok(())
        })?;
    }
    Ok(())
}

fn decode_symbols(reader: &mut SnapshotReader<'_>) -> Result<SymbolTable, SnapshotCodecError> {
    let rows = read_vec(reader, |reader| {
        Ok(Symbol {
            name: reader.string()?.to_owned(),
            value: read_option(reader, read_value_storage_id)?,
            blocks_value_lookup: reader.bool()?,
            function_values: read_vec(reader, read_value_storage_id)?,
            ty: read_option(reader, read_type_group_id)?,
            owns_type_group: reader.bool()?,
            blocks_type_lookup: reader.bool()?,
            blocks_namespace_lookup: reader.bool()?,
            ns: read_option(reader, |reader| Ok(NamespaceId(reader.u32()?)))?,
            declarations: read_vec(reader, read_decl_id)?,
        })
    })?;
    let mut symbols = SymbolTable::new();
    for row in rows {
        symbols.push(row);
    }
    Ok(symbols)
}

fn write_declaration_site(
    writer: &mut SnapshotWriter,
    site: DeclarationSite,
) -> Result<(), SnapshotCodecError> {
    write_scope_id(writer, site.module);
    write_option(writer, site.scope, |writer, value| {
        write_scope_id(writer, value);
        Ok(())
    })?;
    write_span(writer, site.declaration_span);
    write_span(writer, site.binding_span);
    Ok(())
}

fn read_declaration_site(
    reader: &mut SnapshotReader<'_>,
) -> Result<DeclarationSite, SnapshotCodecError> {
    Ok(DeclarationSite {
        module: read_scope_id(reader)?,
        scope: read_option(reader, read_scope_id)?,
        declaration_span: read_span(reader)?,
        binding_span: read_span(reader)?,
    })
}

fn encode_declarations(
    writer: &mut SnapshotWriter,
    table: &DeclarationTable,
) -> Result<(), SnapshotCodecError> {
    let rows = table.iter().collect::<Vec<_>>();
    write_vec(writer, &rows, |writer, row| {
        write_decl_id(writer, row.id);
        write_declaration_kind(writer, row.kind);
        write_declaration_site(writer, row.site)?;
        write_option(writer, row.value_storage, |writer, value| {
            write_value_storage_id(writer, value);
            Ok(())
        })?;
        write_option(writer, row.type_group, |writer, value| {
            write_type_group_id(writer, value);
            Ok(())
        })?;
        write_option(writer, row.namespace, |writer, value| {
            writer.u32(value.0);
            Ok(())
        })
    })
}

fn decode_declarations(
    reader: &mut SnapshotReader<'_>,
) -> Result<DeclarationTable, SnapshotCodecError> {
    let rows = read_vec(reader, |reader| {
        Ok(LexicalDeclaration {
            id: read_decl_id(reader)?,
            kind: read_declaration_kind(reader)?,
            site: read_declaration_site(reader)?,
            value_storage: read_option(reader, read_value_storage_id)?,
            type_group: read_option(reader, read_type_group_id)?,
            namespace: read_option(reader, |reader| Ok(NamespaceId(reader.u32()?)))?,
        })
    })?;
    DeclarationTable::from_snapshot(rows).map_err(|message| invalid(reader, message))
}

fn encode_type_groups(
    writer: &mut SnapshotWriter,
    table: &TypeGroupTable,
) -> Result<(), SnapshotCodecError> {
    let rows = table.iter().collect::<Vec<_>>();
    write_vec(writer, &rows, |writer, group| {
        write_type_group_id(writer, group.id);
        writer.string(&group.name)?;
        write_vec(writer, &group.fragments, |writer, fragment| {
            write_decl_id(writer, fragment.declaration);
            write_source_key(writer, fragment.source);
            write_scope_id(writer, fragment.scope);
            write_declaration_site(writer, fragment.site)?;
            write_fragment_kind(writer, fragment.kind);
            Ok(())
        })
    })
}

fn decode_type_groups(
    reader: &mut SnapshotReader<'_>,
) -> Result<TypeGroupTable, SnapshotCodecError> {
    let rows = read_vec(reader, |reader| {
        Ok(TypeGroup {
            id: read_type_group_id(reader)?,
            name: reader.string()?.to_owned(),
            fragments: read_vec(reader, |reader| {
                Ok(TypeGroupFragment {
                    declaration: read_decl_id(reader)?,
                    source: read_source_key(reader)?,
                    scope: read_scope_id(reader)?,
                    site: read_declaration_site(reader)?,
                    kind: read_fragment_kind(reader)?,
                })
            })?,
        })
    })?;
    TypeGroupTable::from_snapshot(rows).map_err(|message| invalid(reader, message))
}

fieldless_enum_codec!(write_namespace_publication, read_namespace_publication, NamespacePublication, [
    NamespacePublication::Private => 0,
    NamespacePublication::Explicit => 1,
    NamespacePublication::AmbientDefault => 2,
    NamespacePublication::DottedImplicit => 3,
]);
fieldless_enum_codec!(write_instance_state, read_instance_state, NamespaceInstanceState, [
    NamespaceInstanceState::NonInstantiated => 0,
    NamespaceInstanceState::Instantiated => 1,
]);
fieldless_enum_codec!(write_alias_context, read_alias_context, AliasContext, [
    AliasContext::ValidAmbient => 0,
    AliasContext::InvalidFutureTk1194 => 1,
    AliasContext::InvalidAugmentationFutureTk2666 => 2,
]);
fieldless_enum_codec!(write_alias_space, read_alias_space, AliasSpaceIntent, [
    AliasSpaceIntent::Type => 0,
    AliasSpaceIntent::UnresolvedValueOrType => 1,
]);
fieldless_enum_codec!(write_export_resolution, read_export_resolution, ExportResolutionDisposition, [
    ExportResolutionDisposition::NotRequired => 0,
    ExportResolutionDisposition::DeferredBacklog15 => 1,
]);
fieldless_enum_codec!(write_variable_kind, read_variable_kind, VariableKind, [
    VariableKind::Var => 0,
    VariableKind::Let => 1,
    VariableKind::Const => 2,
    VariableKind::Using => 3,
    VariableKind::AwaitUsing => 4,
]);
fieldless_enum_codec!(write_import_form, read_import_form, ImportBindingForm, [
    ImportBindingForm::Named => 0,
    ImportBindingForm::Default => 1,
    ImportBindingForm::Namespace => 2,
    ImportBindingForm::ImportEquals => 3,
]);
fieldless_enum_codec!(write_merge_kind, read_merge_kind, MergeDeclarationKind, [
    MergeDeclarationKind::Variable => 0,
    MergeDeclarationKind::Function => 1,
    MergeDeclarationKind::Class => 2,
    MergeDeclarationKind::TypeAlias => 3,
    MergeDeclarationKind::Interface => 4,
    MergeDeclarationKind::Enum => 5,
    MergeDeclarationKind::Namespace => 6,
    MergeDeclarationKind::ImportAlias => 7,
    MergeDeclarationKind::DeferredExport => 8,
]);
fieldless_enum_codec!(write_global_placement, read_global_placement, GlobalPlacement, [
    GlobalPlacement::DirectExternalModule => 0,
    GlobalPlacement::DirectScript => 1,
    GlobalPlacement::DeferredAmbientModule => 2,
    GlobalPlacement::NestedNamespace => 3,
]);
fieldless_enum_codec!(write_global_issue, read_global_issue, GlobalIssue, [
    GlobalIssue::FutureTk2669 => 0,
    GlobalIssue::FutureTk2670 => 1,
]);
fieldless_enum_codec!(write_deferred_module_kind, read_deferred_module_kind, DeferredModuleKind, [
    DeferredModuleKind::AmbientExternalModule => 0,
    DeferredModuleKind::ModuleAugmentation => 1,
]);
fieldless_enum_codec!(write_deferred_child_kind, read_deferred_child_kind, DeferredChildKind, [
    DeferredChildKind::OrdinaryDeclaration => 0,
    DeferredChildKind::ExportDeclaration => 1,
    DeferredChildKind::NamespaceDeclaration => 2,
    DeferredChildKind::DeferredExport => 3,
]);
fieldless_enum_codec!(write_export_context_kind, read_export_context_kind, ExportContextKind, [
    ExportContextKind::NamedList => 0,
    ExportContextKind::WrappedDeclaration => 1,
    ExportContextKind::ExportAll => 2,
    ExportContextKind::ExportDefault => 3,
    ExportContextKind::ExportAssignment => 4,
]);
fieldless_enum_codec!(write_export_syntax, read_export_syntax, ExportSyntaxDisposition, [
    ExportSyntaxDisposition::Valid => 0,
    ExportSyntaxDisposition::FutureTk1194 => 1,
    ExportSyntaxDisposition::FutureTk1319 => 2,
    ExportSyntaxDisposition::FutureTk1063 => 3,
    ExportSyntaxDisposition::FutureTk2666 => 4,
]);
fieldless_enum_codec!(write_umd_context, read_umd_context, UmdContext, [
    UmdContext::FutureTk1316Nested => 0,
    UmdContext::FutureTk1314NonExternal => 1,
    UmdContext::FutureTk1315Implementation => 2,
    UmdContext::DeferredValidBacklog15 => 3,
]);

fn write_namespace_owner(writer: &mut SnapshotWriter, value: NamespaceOwner) {
    match value {
        NamespaceOwner::Lexical(scope) => {
            writer.u8(0);
            write_scope_id(writer, scope);
        }
        NamespaceOwner::NamespacePublic(namespace) => {
            writer.u8(1);
            writer.u32(namespace.0);
        }
        NamespaceOwner::FragmentPrivate(fragment) => {
            writer.u8(2);
            writer.u32(fragment.0);
        }
        NamespaceOwner::CompilationGlobal => writer.u8(3),
    }
}

fn read_namespace_owner(
    reader: &mut SnapshotReader<'_>,
) -> Result<NamespaceOwner, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(NamespaceOwner::Lexical(read_scope_id(reader)?)),
        1 => Ok(NamespaceOwner::NamespacePublic(NamespaceId(reader.u32()?))),
        2 => Ok(NamespaceOwner::FragmentPrivate(NamespaceFragmentId(
            reader.u32()?,
        ))),
        3 => Ok(NamespaceOwner::CompilationGlobal),
        _ => Err(invalid(reader, "invalid namespace owner")),
    }
}

fn write_declaration_owner(writer: &mut SnapshotWriter, value: DeclarationOwner) {
    match value {
        DeclarationOwner::Lexical(scope) => {
            writer.u8(0);
            write_scope_id(writer, scope);
        }
        DeclarationOwner::NamespacePublic(namespace) => {
            writer.u8(1);
            writer.u32(namespace.0);
        }
        DeclarationOwner::NamespacePrivate(fragment) => {
            writer.u8(2);
            writer.u32(fragment.0);
        }
        DeclarationOwner::CompilationGlobal => writer.u8(3),
        DeclarationOwner::DeferredAmbientModule(module) => {
            writer.u8(4);
            writer.u32(module.0);
        }
    }
}

fn read_declaration_owner(
    reader: &mut SnapshotReader<'_>,
) -> Result<DeclarationOwner, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(DeclarationOwner::Lexical(read_scope_id(reader)?)),
        1 => Ok(DeclarationOwner::NamespacePublic(NamespaceId(
            reader.u32()?,
        ))),
        2 => Ok(DeclarationOwner::NamespacePrivate(NamespaceFragmentId(
            reader.u32()?,
        ))),
        3 => Ok(DeclarationOwner::CompilationGlobal),
        4 => Ok(DeclarationOwner::DeferredAmbientModule(DeferredModuleId(
            reader.u32()?,
        ))),
        _ => Err(invalid(reader, "invalid declaration owner")),
    }
}

fn write_member_owner(writer: &mut SnapshotWriter, value: NamespaceMemberOwner) {
    match value {
        NamespaceMemberOwner::Fragment(id) => {
            writer.u8(0);
            writer.u32(id.0);
        }
        NamespaceMemberOwner::GlobalAugmentation(id) => {
            writer.u8(1);
            writer.u32(id.0);
        }
        NamespaceMemberOwner::DeferredAmbientModule(id) => {
            writer.u8(2);
            writer.u32(id.0);
        }
    }
}

fn read_member_owner(
    reader: &mut SnapshotReader<'_>,
) -> Result<NamespaceMemberOwner, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(NamespaceMemberOwner::Fragment(NamespaceFragmentId(
            reader.u32()?,
        ))),
        1 => Ok(NamespaceMemberOwner::GlobalAugmentation(
            GlobalAugmentationId(reader.u32()?),
        )),
        2 => Ok(NamespaceMemberOwner::DeferredAmbientModule(
            DeferredModuleId(reader.u32()?),
        )),
        _ => Err(invalid(reader, "invalid namespace member owner")),
    }
}

fn write_metadata_name(
    writer: &mut SnapshotWriter,
    value: &MetadataName,
) -> Result<(), SnapshotCodecError> {
    match value {
        MetadataName::Identifier(name) => {
            writer.u8(0);
            writer.string(name)?;
        }
        MetadataName::StringLiteral(name) => {
            writer.u8(1);
            writer.string(name)?;
        }
    }
    Ok(())
}

fn read_metadata_name(reader: &mut SnapshotReader<'_>) -> Result<MetadataName, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(MetadataName::Identifier(reader.string()?.to_owned())),
        1 => Ok(MetadataName::StringLiteral(reader.string()?.to_owned())),
        _ => Err(invalid(reader, "invalid metadata name")),
    }
}

fn write_import_facts(writer: &mut SnapshotWriter, facts: ImportSyntaxFacts) {
    write_import_form(writer, facts.form);
    writer.bool(facts.outer_type_only);
    writer.bool(facts.specifier_type_only);
    writer.bool(facts.external_reference);
    writer.bool(facts.exported);
}

fn read_import_facts(
    reader: &mut SnapshotReader<'_>,
) -> Result<ImportSyntaxFacts, SnapshotCodecError> {
    Ok(ImportSyntaxFacts {
        form: read_import_form(reader)?,
        outer_type_only: reader.bool()?,
        specifier_type_only: reader.bool()?,
        external_reference: reader.bool()?,
        exported: reader.bool()?,
    })
}

fn write_syntax_facts(writer: &mut SnapshotWriter, facts: DeclarationSyntaxFacts) {
    match facts {
        DeclarationSyntaxFacts::None => writer.u8(0),
        DeclarationSyntaxFacts::Variable(kind) => {
            writer.u8(1);
            write_variable_kind(writer, kind);
        }
        DeclarationSyntaxFacts::Import(facts) => {
            writer.u8(2);
            write_import_facts(writer, facts);
        }
        DeclarationSyntaxFacts::Enum { constant } => {
            writer.u8(3);
            writer.bool(constant);
        }
    }
}

fn read_syntax_facts(
    reader: &mut SnapshotReader<'_>,
) -> Result<DeclarationSyntaxFacts, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(DeclarationSyntaxFacts::None),
        1 => Ok(DeclarationSyntaxFacts::Variable(read_variable_kind(
            reader,
        )?)),
        2 => Ok(DeclarationSyntaxFacts::Import(read_import_facts(reader)?)),
        3 => Ok(DeclarationSyntaxFacts::Enum {
            constant: reader.bool()?,
        }),
        _ => Err(invalid(reader, "invalid declaration syntax facts")),
    }
}

fn write_spaces(writer: &mut SnapshotWriter, spaces: DeclarationSpaces) {
    writer.bool(spaces.value);
    writer.bool(spaces.r#type);
    writer.bool(spaces.namespace);
}

fn read_spaces(reader: &mut SnapshotReader<'_>) -> Result<DeclarationSpaces, SnapshotCodecError> {
    Ok(DeclarationSpaces {
        value: reader.bool()?,
        r#type: reader.bool()?,
        namespace: reader.bool()?,
    })
}

fn write_source_file_kind(writer: &mut SnapshotWriter, kind: SourceFileKind) {
    let tag = match kind {
        SourceFileKind::ImplementationTs => 0,
        SourceFileKind::ImplementationMts => 1,
        SourceFileKind::ImplementationCts => 2,
        SourceFileKind::DeclarationTs => 3,
        SourceFileKind::DeclarationMts => 4,
        SourceFileKind::DeclarationCts => 5,
    };
    writer.u8(tag);
}

fn read_source_file_kind(
    reader: &mut SnapshotReader<'_>,
) -> Result<SourceFileKind, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(SourceFileKind::ImplementationTs),
        1 => Ok(SourceFileKind::ImplementationMts),
        2 => Ok(SourceFileKind::ImplementationCts),
        3 => Ok(SourceFileKind::DeclarationTs),
        4 => Ok(SourceFileKind::DeclarationMts),
        5 => Ok(SourceFileKind::DeclarationCts),
        _ => Err(invalid(reader, "invalid source file kind")),
    }
}

fn write_global_owner(writer: &mut SnapshotWriter, owner: GlobalOwner) {
    match owner {
        GlobalOwner::Lexical(scope) => {
            writer.u8(0);
            write_scope_id(writer, scope);
        }
        GlobalOwner::NamespaceFragment(fragment) => {
            writer.u8(1);
            writer.u32(fragment.0);
        }
        GlobalOwner::DeferredAmbientModule(module) => {
            writer.u8(2);
            writer.u32(module.0);
        }
    }
}

fn read_global_owner(reader: &mut SnapshotReader<'_>) -> Result<GlobalOwner, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(GlobalOwner::Lexical(read_scope_id(reader)?)),
        1 => Ok(GlobalOwner::NamespaceFragment(NamespaceFragmentId(
            reader.u32()?,
        ))),
        2 => Ok(GlobalOwner::DeferredAmbientModule(DeferredModuleId(
            reader.u32()?,
        ))),
        _ => Err(invalid(reader, "invalid global owner")),
    }
}

fn write_export_owner(writer: &mut SnapshotWriter, owner: ExportContextOwner) {
    match owner {
        ExportContextOwner::NamespaceFragment(id) => {
            writer.u8(0);
            writer.u32(id.0);
        }
        ExportContextOwner::GlobalAugmentation(id) => {
            writer.u8(1);
            writer.u32(id.0);
        }
        ExportContextOwner::DeferredAmbientModule(id) => {
            writer.u8(2);
            writer.u32(id.0);
        }
    }
}

fn read_export_owner(
    reader: &mut SnapshotReader<'_>,
) -> Result<ExportContextOwner, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(ExportContextOwner::NamespaceFragment(NamespaceFragmentId(
            reader.u32()?,
        ))),
        1 => Ok(ExportContextOwner::GlobalAugmentation(
            GlobalAugmentationId(reader.u32()?),
        )),
        2 => Ok(ExportContextOwner::DeferredAmbientModule(DeferredModuleId(
            reader.u32()?,
        ))),
        _ => Err(invalid(reader, "invalid export context owner")),
    }
}

fn write_namespace(writer: &mut SnapshotWriter, row: &Namespace) -> Result<(), SnapshotCodecError> {
    writer.u32(row.id.0);
    write_namespace_owner(writer, row.owner);
    writer.string(&row.name)?;
    write_scope_id(writer, row.public_scope);
    write_symbol_id(writer, row.symbol);
    write_vec(writer, &row.fragments, |writer, id| {
        writer.u32(id.0);
        Ok(())
    })
}

fn read_namespace(reader: &mut SnapshotReader<'_>) -> Result<Namespace, SnapshotCodecError> {
    Ok(Namespace {
        id: NamespaceId(reader.u32()?),
        owner: read_namespace_owner(reader)?,
        name: reader.string()?.to_owned(),
        public_scope: read_scope_id(reader)?,
        symbol: read_symbol_id(reader)?,
        fragments: read_vec(reader, |reader| Ok(NamespaceFragmentId(reader.u32()?)))?,
    })
}

fn write_fragment(
    writer: &mut SnapshotWriter,
    row: &NamespaceFragment,
) -> Result<(), SnapshotCodecError> {
    writer.u32(row.id.0);
    writer.u32(row.namespace.0);
    write_decl_id(writer, row.declaration);
    write_source_key(writer, row.source);
    write_origin(writer, row.origin);
    writer.u32(row.source_start);
    write_scope_id(writer, row.module);
    write_scope_id(writer, row.private_scope);
    write_scope_id(writer, row.lexical_parent);
    write_scope_id(writer, row.public_scope);
    writer.bool(row.ambient);
    write_namespace_publication(writer, row.publication);
    write_instance_state(writer, row.instance_state);
    write_vec(writer, &row.members, |writer, id| {
        writer.u32(id.0);
        Ok(())
    })
}

fn read_fragment(reader: &mut SnapshotReader<'_>) -> Result<NamespaceFragment, SnapshotCodecError> {
    Ok(NamespaceFragment {
        id: NamespaceFragmentId(reader.u32()?),
        namespace: NamespaceId(reader.u32()?),
        declaration: read_decl_id(reader)?,
        source: read_source_key(reader)?,
        origin: read_origin(reader)?,
        source_start: reader.u32()?,
        module: read_scope_id(reader)?,
        private_scope: read_scope_id(reader)?,
        lexical_parent: read_scope_id(reader)?,
        public_scope: read_scope_id(reader)?,
        ambient: reader.bool()?,
        publication: read_namespace_publication(reader)?,
        instance_state: read_instance_state(reader)?,
        members: read_vec(reader, |reader| Ok(NamespaceMemberId(reader.u32()?)))?,
    })
}

fn write_member(
    writer: &mut SnapshotWriter,
    row: &NamespaceMember,
) -> Result<(), SnapshotCodecError> {
    writer.u32(row.id.0);
    write_member_owner(writer, row.owner);
    write_declaration_owner(writer, row.target);
    write_option(writer, row.declaration, |writer, id| {
        write_decl_id(writer, id);
        Ok(())
    })?;
    write_option(writer, row.symbol, |writer, id| {
        write_symbol_id(writer, id);
        Ok(())
    })?;
    write_option(writer, row.local_symbol, |writer, id| {
        write_symbol_id(writer, id);
        Ok(())
    })?;
    write_option(writer, row.name.as_deref(), |writer, value| {
        writer.string(value)
    })?;
    write_option(writer, row.local_name.as_ref(), write_metadata_name)?;
    write_option(writer, row.exported_name.as_ref(), write_metadata_name)?;
    write_span(writer, row.declaration_span);
    write_option(writer, row.specifier_span, |writer, value| {
        write_span(writer, value);
        Ok(())
    })?;
    write_span(writer, row.binding_span);
    write_option(writer, row.local_span, |writer, value| {
        write_span(writer, value);
        Ok(())
    })?;
    write_option(writer, row.exported_span, |writer, value| {
        write_span(writer, value);
        Ok(())
    })?;
    write_source_key(writer, row.source);
    write_origin(writer, row.origin);
    write_option(writer, row.module_specifier.as_ref(), write_metadata_name)?;
    writer.bool(row.outer_type_only);
    writer.bool(row.specifier_type_only);
    write_option(writer, row.alias_context, |writer, value| {
        write_alias_context(writer, value);
        Ok(())
    })?;
    write_option(writer, row.alias_resolution, |writer, value| {
        write_export_resolution(writer, value);
        Ok(())
    })?;
    write_option(writer, row.alias_space_intent, |writer, value| {
        write_alias_space(writer, value);
        Ok(())
    })?;
    write_option(writer, row.export_context, |writer, value| {
        writer.u32(value.0);
        Ok(())
    })?;
    write_syntax_facts(writer, row.syntax);
    write_spaces(writer, row.spaces);
    write_merge_kind(writer, row.kind);
    write_namespace_publication(writer, row.publication);
    Ok(())
}

fn read_member(reader: &mut SnapshotReader<'_>) -> Result<NamespaceMember, SnapshotCodecError> {
    Ok(NamespaceMember {
        id: NamespaceMemberId(reader.u32()?),
        owner: read_member_owner(reader)?,
        target: read_declaration_owner(reader)?,
        declaration: read_option(reader, read_decl_id)?,
        symbol: read_option(reader, read_symbol_id)?,
        local_symbol: read_option(reader, read_symbol_id)?,
        name: read_option(reader, |reader| Ok(reader.string()?.to_owned()))?,
        local_name: read_option(reader, read_metadata_name)?,
        exported_name: read_option(reader, read_metadata_name)?,
        declaration_span: read_span(reader)?,
        specifier_span: read_option(reader, read_span)?,
        binding_span: read_span(reader)?,
        local_span: read_option(reader, read_span)?,
        exported_span: read_option(reader, read_span)?,
        source: read_source_key(reader)?,
        origin: read_origin(reader)?,
        module_specifier: read_option(reader, read_metadata_name)?,
        outer_type_only: reader.bool()?,
        specifier_type_only: reader.bool()?,
        alias_context: read_option(reader, read_alias_context)?,
        alias_resolution: read_option(reader, read_export_resolution)?,
        alias_space_intent: read_option(reader, read_alias_space)?,
        export_context: read_option(reader, |reader| Ok(ExportContextId(reader.u32()?)))?,
        syntax: read_syntax_facts(reader)?,
        spaces: read_spaces(reader)?,
        kind: read_merge_kind(reader)?,
        publication: read_namespace_publication(reader)?,
    })
}

fn write_participant(
    writer: &mut SnapshotWriter,
    row: &MergeParticipant,
) -> Result<(), SnapshotCodecError> {
    write_decl_id(writer, row.declaration);
    write_merge_kind(writer, row.kind);
    write_source_key(writer, row.source);
    write_origin(writer, row.origin);
    write_span(writer, row.span);
    writer.bool(row.ambient);
    write_spaces(writer, row.spaces);
    write_syntax_facts(writer, row.syntax);
    write_option(writer, row.namespace_fragment, |writer, id| {
        writer.u32(id.0);
        Ok(())
    })?;
    write_option(writer, row.namespace_instance, |writer, value| {
        write_instance_state(writer, value);
        Ok(())
    })?;
    write_span(writer, row.binding_span);
    Ok(())
}

fn read_participant(
    reader: &mut SnapshotReader<'_>,
) -> Result<MergeParticipant, SnapshotCodecError> {
    Ok(MergeParticipant {
        declaration: read_decl_id(reader)?,
        kind: read_merge_kind(reader)?,
        source: read_source_key(reader)?,
        origin: read_origin(reader)?,
        span: read_span(reader)?,
        ambient: reader.bool()?,
        spaces: read_spaces(reader)?,
        syntax: read_syntax_facts(reader)?,
        namespace_fragment: read_option(reader, |reader| Ok(NamespaceFragmentId(reader.u32()?)))?,
        namespace_instance: read_option(reader, read_instance_state)?,
        binding_span: read_span(reader)?,
    })
}

fn write_global(
    writer: &mut SnapshotWriter,
    row: &GlobalAugmentation,
) -> Result<(), SnapshotCodecError> {
    writer.u32(row.id.0);
    write_decl_id(writer, row.declaration);
    write_source_key(writer, row.source);
    write_origin(writer, row.origin);
    write_scope_id(writer, row.module);
    write_global_owner(writer, row.owner);
    write_span(writer, row.body_span);
    write_span(writer, row.diagnostic_span);
    write_scope_id(writer, row.target_scope);
    write_scope_id(writer, row.overlay_scope);
    write_global_placement(writer, row.placement);
    write_vec(writer, &row.issues, |writer, value| {
        write_global_issue(writer, *value);
        Ok(())
    })?;
    writer.bool(row.declared);
    write_vec(writer, &row.members, |writer, value| {
        writer.u32(value.0);
        Ok(())
    })
}

fn read_global(reader: &mut SnapshotReader<'_>) -> Result<GlobalAugmentation, SnapshotCodecError> {
    Ok(GlobalAugmentation {
        id: GlobalAugmentationId(reader.u32()?),
        declaration: read_decl_id(reader)?,
        source: read_source_key(reader)?,
        origin: read_origin(reader)?,
        module: read_scope_id(reader)?,
        owner: read_global_owner(reader)?,
        body_span: read_span(reader)?,
        diagnostic_span: read_span(reader)?,
        target_scope: read_scope_id(reader)?,
        overlay_scope: read_scope_id(reader)?,
        placement: read_global_placement(reader)?,
        issues: read_vec(reader, read_global_issue)?,
        declared: reader.bool()?,
        members: read_vec(reader, |reader| Ok(NamespaceMemberId(reader.u32()?)))?,
    })
}

fn write_deferred_module(
    writer: &mut SnapshotWriter,
    row: &DeferredAmbientModule,
) -> Result<(), SnapshotCodecError> {
    writer.u32(row.id.0);
    write_decl_id(writer, row.declaration);
    write_source_key(writer, row.source);
    write_origin(writer, row.origin);
    write_scope_id(writer, row.module);
    write_declaration_owner(writer, row.owner);
    write_deferred_module_kind(writer, row.kind);
    writer.string(&row.specifier)?;
    write_span(writer, row.span);
    Ok(())
}

fn read_deferred_module(
    reader: &mut SnapshotReader<'_>,
) -> Result<DeferredAmbientModule, SnapshotCodecError> {
    Ok(DeferredAmbientModule {
        id: DeferredModuleId(reader.u32()?),
        declaration: read_decl_id(reader)?,
        source: read_source_key(reader)?,
        origin: read_origin(reader)?,
        module: read_scope_id(reader)?,
        owner: read_declaration_owner(reader)?,
        kind: read_deferred_module_kind(reader)?,
        specifier: reader.string()?.to_owned(),
        span: read_span(reader)?,
    })
}

fn write_deferred_child(
    writer: &mut SnapshotWriter,
    row: &DeferredAmbientChild,
) -> Result<(), SnapshotCodecError> {
    writer.u32(row.module.0);
    write_option(writer, row.declaration, |writer, value| {
        write_decl_id(writer, value);
        Ok(())
    })?;
    write_deferred_child_kind(writer, row.kind);
    write_option(writer, row.name.as_ref(), write_metadata_name)?;
    write_span(writer, row.span);
    write_option(writer, row.binding_span, |writer, value| {
        write_span(writer, value);
        Ok(())
    })?;
    write_source_key(writer, row.source);
    write_origin(writer, row.origin);
    Ok(())
}

fn read_deferred_child(
    reader: &mut SnapshotReader<'_>,
) -> Result<DeferredAmbientChild, SnapshotCodecError> {
    Ok(DeferredAmbientChild {
        module: DeferredModuleId(reader.u32()?),
        declaration: read_option(reader, read_decl_id)?,
        kind: read_deferred_child_kind(reader)?,
        name: read_option(reader, read_metadata_name)?,
        span: read_span(reader)?,
        binding_span: read_option(reader, read_span)?,
        source: read_source_key(reader)?,
        origin: read_origin(reader)?,
    })
}

fn write_umd(
    writer: &mut SnapshotWriter,
    row: &UmdNamespaceExport,
) -> Result<(), SnapshotCodecError> {
    write_decl_id(writer, row.declaration);
    write_source_key(writer, row.source);
    write_origin(writer, row.origin);
    write_scope_id(writer, row.module);
    write_declaration_owner(writer, row.owner);
    writer.string(&row.name)?;
    write_span(writer, row.span);
    write_umd_context(writer, row.context);
    Ok(())
}

fn read_umd(reader: &mut SnapshotReader<'_>) -> Result<UmdNamespaceExport, SnapshotCodecError> {
    Ok(UmdNamespaceExport {
        declaration: read_decl_id(reader)?,
        source: read_source_key(reader)?,
        origin: read_origin(reader)?,
        module: read_scope_id(reader)?,
        owner: read_declaration_owner(reader)?,
        name: reader.string()?.to_owned(),
        span: read_span(reader)?,
        context: read_umd_context(reader)?,
    })
}

fn write_export_context(
    writer: &mut SnapshotWriter,
    row: &ExportContext,
) -> Result<(), SnapshotCodecError> {
    writer.u32(row.id.0);
    write_export_owner(writer, row.owner);
    write_export_context_kind(writer, row.kind);
    write_export_syntax(writer, row.syntax);
    write_export_resolution(writer, row.resolution);
    writer.bool(row.has_module_specifier);
    write_source_key(writer, row.source);
    write_origin(writer, row.origin);
    write_span(writer, row.span);
    write_vec(writer, &row.members, |writer, value| {
        writer.u32(value.0);
        Ok(())
    })
}

fn read_export_context(
    reader: &mut SnapshotReader<'_>,
) -> Result<ExportContext, SnapshotCodecError> {
    Ok(ExportContext {
        id: ExportContextId(reader.u32()?),
        owner: read_export_owner(reader)?,
        kind: read_export_context_kind(reader)?,
        syntax: read_export_syntax(reader)?,
        resolution: read_export_resolution(reader)?,
        has_module_specifier: reader.bool()?,
        source: read_source_key(reader)?,
        origin: read_origin(reader)?,
        span: read_span(reader)?,
        members: read_vec(reader, |reader| Ok(NamespaceMemberId(reader.u32()?)))?,
    })
}

fn write_source_unit(
    writer: &mut SnapshotWriter,
    row: &SourceUnitRecord,
) -> Result<(), SnapshotCodecError> {
    write_source_key(writer, row.source);
    write_origin(writer, row.origin);
    write_scope_id(writer, row.module);
    write_source_file_kind(writer, row.context.source_file_kind);
    writer.bool(row.context.external_module);
    Ok(())
}

fn read_source_unit(
    reader: &mut SnapshotReader<'_>,
) -> Result<SourceUnitRecord, SnapshotCodecError> {
    Ok(SourceUnitRecord {
        source: read_source_key(reader)?,
        origin: read_origin(reader)?,
        module: read_scope_id(reader)?,
        context: ModuleBindingContext {
            source_file_kind: read_source_file_kind(reader)?,
            external_module: reader.bool()?,
        },
    })
}

fn encode_namespaces(
    writer: &mut SnapshotWriter,
    table: &NamespaceTable,
) -> Result<(), SnapshotCodecError> {
    let primary = table.snapshot_primary();
    write_vec(writer, &primary.namespaces, write_namespace)?;
    write_vec(
        writer,
        &primary.standalone_value_storages,
        |writer, value| {
            write_option(writer, *value, |writer, value| {
                write_value_storage_id(writer, value);
                Ok(())
            })
        },
    )?;
    write_vec(writer, &primary.fragments, write_fragment)?;
    write_vec(writer, &primary.members, write_member)?;
    write_len(writer, primary.placements.len())?;
    for (owner, name, declarations) in &primary.placements {
        write_declaration_owner(writer, *owner);
        writer.string(name)?;
        write_vec(writer, declarations, write_participant)?;
    }
    write_vec(writer, &primary.globals, write_global)?;
    write_vec(writer, &primary.deferred_modules, write_deferred_module)?;
    write_vec(writer, &primary.deferred_children, write_deferred_child)?;
    write_vec(writer, &primary.umd_exports, write_umd)?;
    write_vec(writer, &primary.export_contexts, write_export_context)?;
    write_vec(writer, &primary.source_units, write_source_unit)?;
    write_option(writer, primary.compilation_global, |writer, value| {
        write_scope_id(writer, value);
        Ok(())
    })?;
    write_option(writer, primary.script_namespace_root, |writer, value| {
        write_scope_id(writer, value);
        Ok(())
    })?;
    writer.bool(primary.library_shared_globals);
    Ok(())
}

fn decode_namespaces(
    reader: &mut SnapshotReader<'_>,
) -> Result<NamespaceTable, SnapshotCodecError> {
    let namespaces = read_vec(reader, read_namespace)?;
    let standalone_value_storages =
        read_vec(reader, |reader| read_option(reader, read_value_storage_id))?;
    let fragments = read_vec(reader, read_fragment)?;
    let members = read_vec(reader, read_member)?;
    let mut placements = Vec::new();
    for _ in 0..read_len(reader)? {
        placements.push((
            read_declaration_owner(reader)?,
            reader.string()?.to_owned(),
            read_vec(reader, read_participant)?,
        ));
    }
    let primary = NamespaceSnapshotPrimary {
        namespaces,
        standalone_value_storages,
        fragments,
        members,
        placements,
        globals: read_vec(reader, read_global)?,
        deferred_modules: read_vec(reader, read_deferred_module)?,
        deferred_children: read_vec(reader, read_deferred_child)?,
        umd_exports: read_vec(reader, read_umd)?,
        export_contexts: read_vec(reader, read_export_context)?,
        source_units: read_vec(reader, read_source_unit)?,
        compilation_global: read_option(reader, read_scope_id)?,
        script_namespace_root: read_option(reader, read_scope_id)?,
        library_shared_globals: reader.bool()?,
    };
    NamespaceTable::from_snapshot_primary(primary).map_err(|message| invalid(reader, message))
}

fn id_in_range(id: u32, len: usize) -> bool {
    usize::try_from(id).ok().is_some_and(|id| id < len)
}

fn validate_binder(binder: &Binder, offset: usize) -> Result<(), SnapshotCodecError> {
    let scope_len = binder.graph.snapshot_len();
    let symbol_len = binder.symbols.len();
    let declaration_len = binder.declarations.len();
    let type_group_len = binder.type_groups.len();
    let namespace_len = binder.namespaces.len();
    let invalid = |message| SnapshotCodecError::invalid(offset, message);
    for root in [
        binder.module,
        binder.prelude_module,
        binder.compilation_global,
        binder.script_namespace_root,
    ] {
        if !id_in_range(root.0, scope_len) {
            return Err(invalid("binder root scope is out of range"));
        }
    }
    let prelude = binder
        .graph
        .get(binder.prelude_module)
        .ok_or_else(|| invalid("prelude scope is absent"))?;
    let compilation_global = binder
        .graph
        .get(binder.compilation_global)
        .ok_or_else(|| invalid("compilation-global scope is absent"))?;
    let script_root = binder
        .graph
        .get(binder.script_namespace_root)
        .ok_or_else(|| invalid("script namespace root is absent"))?;
    let module = binder
        .graph
        .get(binder.module)
        .ok_or_else(|| invalid("active module scope is absent"))?;
    if binder.prelude_module == binder.compilation_global
        || binder.prelude_module == binder.script_namespace_root
        || binder.compilation_global == binder.script_namespace_root
        || binder.module == binder.prelude_module
        || binder.module == binder.compilation_global
        || binder.module == binder.script_namespace_root
        || prelude.kind != ScopeKind::Module
        || prelude.parent.is_some()
        || compilation_global.kind != ScopeKind::CompilationGlobal
        || compilation_global.parent != Some(binder.prelude_module)
        || script_root.kind != ScopeKind::ScriptNamespaceRoot
        || script_root.parent != Some(binder.compilation_global)
        || module.kind != ScopeKind::Module
        || (binder.module != binder.prelude_module
            && module.parent != Some(binder.script_namespace_root))
    {
        return Err(invalid("binder root scope structure is invalid"));
    }
    if usize::try_from(binder.prelude_type_group_count)
        .ok()
        .is_none_or(|count| count > type_group_len)
    {
        return Err(invalid("prelude type-group count is out of range"));
    }
    for scope in binder.graph.snapshot_scopes() {
        for referenced in [scope.parent, scope.namespace_public].into_iter().flatten() {
            if !id_in_range(referenced.0, scope_len) {
                return Err(invalid("scope edge is out of range"));
            }
        }
        if scope
            .symbols
            .values()
            .any(|id| !id_in_range(id.0, symbol_len))
        {
            return Err(invalid("scope symbol is out of range"));
        }
        if scope.namespace_public.is_some_and(|public| {
            binder
                .graph
                .get(public)
                .is_none_or(|scope| scope.kind != ScopeKind::NamespacePublic)
        }) {
            return Err(invalid("scope namespace-public edge has the wrong kind"));
        }
    }
    for start in 0..scope_len {
        let mut seen = BTreeSet::new();
        let mut current = Some(ScopeId(
            u32::try_from(start).map_err(|_| invalid("scope count exceeds u32"))?,
        ));
        while let Some(scope) = current {
            if !seen.insert(scope.0) {
                return Err(invalid("scope parent graph contains a cycle"));
            }
            current = binder
                .graph
                .get(scope)
                .ok_or_else(|| invalid("scope parent is absent"))?
                .parent;
        }
    }
    let module_scope_count = binder
        .graph
        .snapshot_scopes()
        .filter(|scope| scope.kind == ScopeKind::Module)
        .count();
    if binder.snapshot_module_sources().len() != module_scope_count
        || binder.snapshot_module_sources().iter().any(|(scope, _)| {
            binder
                .graph
                .get(*scope)
                .is_none_or(|scope| scope.kind != ScopeKind::Module)
        })
        || binder.snapshot_module_sources().get(&binder.prelude_module)
            != Some(&SourceUnitKey::PRELUDE)
        || binder
            .snapshot_module_sources()
            .values()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != binder.snapshot_module_sources().len()
    {
        return Err(invalid("module-source index is incomplete or inconsistent"));
    }
    for (index, scope) in binder.graph.snapshot_scopes().enumerate() {
        let id = ScopeId(u32::try_from(index).map_err(|_| invalid("scope count exceeds u32"))?);
        if scope.kind == ScopeKind::Module && !binder.snapshot_module_sources().contains_key(&id) {
            return Err(invalid("module-source index omits a module scope"));
        }
        if scope.kind == ScopeKind::Module
            && id != binder.prelude_module
            && scope.parent != Some(binder.script_namespace_root)
        {
            return Err(invalid("user/library module has the wrong parent root"));
        }
    }
    let valid_module_key = |module: ScopeId| {
        binder
            .graph
            .get(module)
            .is_some_and(|scope| scope.kind == ScopeKind::Module)
    };
    if binder.fn_scopes.iter().any(|((module, _), target)| {
        !valid_module_key(*module)
            || binder
                .graph
                .get(*target)
                .is_none_or(|scope| scope.kind != ScopeKind::Function)
    }) {
        return Err(invalid("retained function-scope map is invalid"));
    }
    if binder.block_scopes.iter().any(|((module, _), target)| {
        !valid_module_key(*module)
            || binder
                .graph
                .get(*target)
                .is_none_or(|scope| scope.kind != ScopeKind::Block)
    }) {
        return Err(invalid("retained block-scope map is invalid"));
    }
    if binder
        .fn_decl_ids
        .iter()
        .any(|((module, _), target)| !valid_module_key(*module) || target.0 >= binder.decl_count)
    {
        return Err(invalid("retained function-declaration map is invalid"));
    }
    let value_in_range = |id: ValueStorageId| id.0 < binder.decl_count;
    let mut value_storages = BTreeSet::new();
    for symbol in binder.symbols.snapshot_symbols() {
        if symbol.value.is_some_and(|id| !value_in_range(id))
            || symbol
                .function_values
                .iter()
                .copied()
                .any(|id| !value_in_range(id))
            || symbol
                .ty
                .is_some_and(|id| !id_in_range(id.0, type_group_len))
            || symbol
                .ns
                .is_some_and(|id| !id_in_range(id.0, namespace_len))
            || symbol
                .declarations
                .iter()
                .any(|id| !id_in_range(id.0, declaration_len))
        {
            return Err(invalid("symbol reference is out of range"));
        }
        value_storages.extend(symbol.value.map(|id| id.0));
        value_storages.extend(symbol.function_values.iter().map(|id| id.0));
    }
    for declaration in binder.declarations.iter() {
        if !id_in_range(declaration.site.module.0, scope_len)
            || declaration
                .site
                .scope
                .is_some_and(|id| !id_in_range(id.0, scope_len))
            || declaration
                .value_storage
                .is_some_and(|id| !value_in_range(id))
            || declaration
                .type_group
                .is_some_and(|id| !id_in_range(id.0, type_group_len))
            || declaration
                .namespace
                .is_some_and(|id| !id_in_range(id.0, namespace_len))
        {
            return Err(invalid("declaration reference is out of range"));
        }
        value_storages.extend(declaration.value_storage.map(|id| id.0));
    }
    for group in binder.type_groups.iter() {
        for fragment in &group.fragments {
            if !id_in_range(fragment.declaration.0, declaration_len)
                || !id_in_range(fragment.scope.0, scope_len)
                || !id_in_range(fragment.site.module.0, scope_len)
                || fragment
                    .site
                    .scope
                    .is_some_and(|id| !id_in_range(id.0, scope_len))
            {
                return Err(invalid("type-group fragment reference is out of range"));
            }
            if binder
                .declarations
                .get(fragment.declaration)
                .is_none_or(|declaration| declaration.type_group != Some(group.id))
            {
                return Err(invalid(
                    "type-group fragment and declaration disagree on ownership",
                ));
            }
            let declaration = binder
                .declarations
                .get(fragment.declaration)
                .ok_or_else(|| invalid("type-group declaration is absent"))?;
            if fragment.site != declaration.site
                || declaration.site.scope != Some(fragment.scope)
                || binder
                    .snapshot_module_sources()
                    .get(&declaration.site.module)
                    != Some(&fragment.source)
            {
                return Err(invalid(
                    "type-group fragment does not match its exact declaration site",
                ));
            }
        }
    }
    for declaration in binder
        .declarations
        .iter()
        .filter(|declaration| declaration.type_group.is_some())
    {
        let Some(group) = declaration.type_group else {
            continue;
        };
        if binder.type_groups.get(group).is_none_or(|group| {
            !group
                .fragments
                .iter()
                .any(|fragment| fragment.declaration == declaration.id)
        }) {
            return Err(invalid(
                "declaration type-group owner has no matching fragment",
            ));
        }
    }
    if binder
        .snapshot_module_sources()
        .keys()
        .any(|scope| !id_in_range(scope.0, scope_len))
    {
        return Err(invalid("module source scope is out of range"));
    }
    let primary = binder.namespaces.snapshot_primary();
    binder
        .namespaces
        .validate_snapshot_canonical()
        .map_err(&invalid)?;
    if primary.compilation_global != Some(binder.compilation_global)
        || primary.script_namespace_root != Some(binder.script_namespace_root)
    {
        return Err(invalid("namespace roots disagree with binder roots"));
    }
    if primary.source_units.iter().any(|unit| {
        !id_in_range(unit.module.0, scope_len)
            || binder.snapshot_module_sources().get(&unit.module) != Some(&unit.source)
    }) {
        return Err(invalid(
            "namespace source ownership disagrees with module index",
        ));
    }
    if primary.namespaces.iter().any(|namespace| {
        !id_in_range(namespace.public_scope.0, scope_len)
            || !id_in_range(namespace.symbol.0, symbol_len)
            || namespace
                .fragments
                .iter()
                .any(|id| !id_in_range(id.0, primary.fragments.len()))
    }) {
        return Err(invalid("namespace reference is out of range"));
    }
    if primary.fragments.iter().any(|fragment| {
        !id_in_range(fragment.namespace.0, namespace_len)
            || !id_in_range(fragment.declaration.0, declaration_len)
            || [
                fragment.module,
                fragment.private_scope,
                fragment.lexical_parent,
                fragment.public_scope,
            ]
            .into_iter()
            .any(|id| !id_in_range(id.0, scope_len))
            || fragment
                .members
                .iter()
                .any(|id| !id_in_range(id.0, primary.members.len()))
    }) {
        return Err(invalid("namespace fragment reference is out of range"));
    }
    if primary
        .standalone_value_storages
        .iter()
        .flatten()
        .copied()
        .any(|id| !value_in_range(id))
    {
        return Err(invalid("namespace value storage is out of range"));
    }
    value_storages.extend(
        primary
            .standalone_value_storages
            .iter()
            .flatten()
            .map(|id| id.0),
    );
    if value_storages.len()
        != usize::try_from(binder.decl_count)
            .map_err(|_| invalid("value-storage count exceeds usize"))?
        || value_storages
            .iter()
            .copied()
            .enumerate()
            .any(|(index, id)| usize::try_from(id).ok() != Some(index))
    {
        return Err(invalid("value-storage identities are not dense"));
    }
    let namespace_owner_valid = |owner: NamespaceOwner| match owner {
        NamespaceOwner::Lexical(scope) => id_in_range(scope.0, scope_len),
        NamespaceOwner::NamespacePublic(namespace) => id_in_range(namespace.0, namespace_len),
        NamespaceOwner::FragmentPrivate(fragment) => {
            id_in_range(fragment.0, primary.fragments.len())
        }
        NamespaceOwner::CompilationGlobal => true,
    };
    let declaration_owner_valid = |owner: DeclarationOwner| match owner {
        DeclarationOwner::Lexical(scope) => id_in_range(scope.0, scope_len),
        DeclarationOwner::NamespacePublic(namespace) => id_in_range(namespace.0, namespace_len),
        DeclarationOwner::NamespacePrivate(fragment) => {
            id_in_range(fragment.0, primary.fragments.len())
        }
        DeclarationOwner::CompilationGlobal => true,
        DeclarationOwner::DeferredAmbientModule(module) => {
            id_in_range(module.0, primary.deferred_modules.len())
        }
    };
    if primary
        .namespaces
        .iter()
        .any(|namespace| !namespace_owner_valid(namespace.owner))
    {
        return Err(invalid("namespace owner is out of range"));
    }
    let mut source_owners = FxHashMap::default();
    for unit in &primary.source_units {
        if source_owners
            .insert(unit.source, (unit.module, unit.origin))
            .is_some()
        {
            return Err(invalid("namespace source-unit index contains a duplicate"));
        }
    }
    if source_owners.len() + 1 != binder.snapshot_module_sources().len()
        || binder
            .snapshot_module_sources()
            .iter()
            .any(|(module, source)| {
                *module != binder.prelude_module
                    && source_owners
                        .get(source)
                        .is_none_or(|(owner_module, _)| owner_module != module)
            })
    {
        return Err(invalid(
            "namespace source-unit index does not cover every non-prelude module",
        ));
    }
    for fragment in &primary.fragments {
        if source_owners.get(&fragment.source) != Some(&(fragment.module, fragment.origin)) {
            return Err(invalid(
                "namespace fragment source ownership is inconsistent",
            ));
        }
    }
    let mut fragment_back_references = vec![0usize; primary.fragments.len()];
    for namespace in &primary.namespaces {
        for fragment_id in &namespace.fragments {
            let Some(fragment) = primary.fragments.get(fragment_id.index()) else {
                return Err(invalid("namespace fragment reference is out of range"));
            };
            if fragment.namespace != namespace.id {
                return Err(invalid("namespace fragment back-reference is inconsistent"));
            }
            fragment_back_references[fragment_id.index()] += 1;
        }
    }
    if fragment_back_references.iter().any(|count| *count != 1) {
        return Err(invalid(
            "each namespace fragment must have exactly one namespace owner",
        ));
    }
    for member in &primary.members {
        let owner_valid = match member.owner {
            NamespaceMemberOwner::Fragment(id) => id_in_range(id.0, primary.fragments.len()),
            NamespaceMemberOwner::GlobalAugmentation(id) => {
                id_in_range(id.0, primary.globals.len())
            }
            NamespaceMemberOwner::DeferredAmbientModule(id) => {
                id_in_range(id.0, primary.deferred_modules.len())
            }
        };
        if !owner_valid
            || !declaration_owner_valid(member.target)
            || member
                .declaration
                .is_some_and(|id| !id_in_range(id.0, declaration_len))
            || member
                .symbol
                .is_some_and(|id| !id_in_range(id.0, symbol_len))
            || member
                .local_symbol
                .is_some_and(|id| !id_in_range(id.0, symbol_len))
            || member
                .export_context
                .is_some_and(|id| !id_in_range(id.0, primary.export_contexts.len()))
            || source_owners
                .get(&member.source)
                .is_none_or(|(_, origin)| *origin != member.origin)
        {
            return Err(invalid("namespace member reference is out of range"));
        }
    }
    let mut fragment_member_back_references = vec![0usize; primary.members.len()];
    for fragment in &primary.fragments {
        for id in &fragment.members {
            let Some(member) = primary.members.get(id.index()) else {
                return Err(invalid("namespace member reference is out of range"));
            };
            if member.owner != NamespaceMemberOwner::Fragment(fragment.id) {
                return Err(invalid(
                    "namespace member owner back-reference is inconsistent",
                ));
            }
            fragment_member_back_references[id.index()] += 1;
        }
    }
    let mut global_member_back_references = vec![0usize; primary.members.len()];
    for global in &primary.globals {
        for id in &global.members {
            let Some(member) = primary.members.get(id.index()) else {
                return Err(invalid("global member reference is out of range"));
            };
            if member.owner != NamespaceMemberOwner::GlobalAugmentation(global.id) {
                return Err(invalid(
                    "global member owner back-reference is inconsistent",
                ));
            }
            global_member_back_references[id.index()] += 1;
        }
    }
    for member in &primary.members {
        let expected = match member.owner {
            NamespaceMemberOwner::Fragment(_) => fragment_member_back_references[member.id.index()],
            NamespaceMemberOwner::GlobalAugmentation(_) => {
                global_member_back_references[member.id.index()]
            }
            NamespaceMemberOwner::DeferredAmbientModule(_) => 0,
        };
        if !matches!(member.owner, NamespaceMemberOwner::DeferredAmbientModule(_)) && expected != 1
        {
            return Err(invalid(
                "namespace member must occur exactly once in its owner list",
            ));
        }
    }
    for (owner, _, participants) in &primary.placements {
        if !declaration_owner_valid(*owner)
            || participants.iter().any(|participant| {
                !id_in_range(participant.declaration.0, declaration_len)
                    || source_owners
                        .get(&participant.source)
                        .is_none_or(|(_, origin)| *origin != participant.origin)
                    || participant
                        .namespace_fragment
                        .is_some_and(|id| !id_in_range(id.0, primary.fragments.len()))
            })
        {
            return Err(invalid("merge placement reference is out of range"));
        }
    }
    for global in &primary.globals {
        let owner_valid = match global.owner {
            GlobalOwner::Lexical(scope) => id_in_range(scope.0, scope_len),
            GlobalOwner::NamespaceFragment(id) => id_in_range(id.0, primary.fragments.len()),
            GlobalOwner::DeferredAmbientModule(id) => {
                id_in_range(id.0, primary.deferred_modules.len())
            }
        };
        if !owner_valid
            || !id_in_range(global.declaration.0, declaration_len)
            || [global.module, global.target_scope, global.overlay_scope]
                .into_iter()
                .any(|id| !id_in_range(id.0, scope_len))
            || source_owners.get(&global.source) != Some(&(global.module, global.origin))
            || global
                .members
                .iter()
                .any(|id| !id_in_range(id.0, primary.members.len()))
        {
            return Err(invalid("global augmentation reference is out of range"));
        }
    }
    for module in &primary.deferred_modules {
        if !id_in_range(module.declaration.0, declaration_len)
            || !id_in_range(module.module.0, scope_len)
            || !declaration_owner_valid(module.owner)
            || source_owners.get(&module.source) != Some(&(module.module, module.origin))
        {
            return Err(invalid("deferred module reference is out of range"));
        }
    }
    if primary.deferred_children.iter().any(|child| {
        !id_in_range(child.module.0, primary.deferred_modules.len())
            || child
                .declaration
                .is_some_and(|id| !id_in_range(id.0, declaration_len))
            || source_owners
                .get(&child.source)
                .is_none_or(|(_, origin)| *origin != child.origin)
    }) {
        return Err(invalid("deferred child reference is out of range"));
    }
    if primary.umd_exports.iter().any(|export| {
        !id_in_range(export.declaration.0, declaration_len)
            || !id_in_range(export.module.0, scope_len)
            || !declaration_owner_valid(export.owner)
            || source_owners.get(&export.source) != Some(&(export.module, export.origin))
    }) {
        return Err(invalid("UMD export reference is out of range"));
    }
    for context in &primary.export_contexts {
        let owner_valid = match context.owner {
            ExportContextOwner::NamespaceFragment(id) => id_in_range(id.0, primary.fragments.len()),
            ExportContextOwner::GlobalAugmentation(id) => id_in_range(id.0, primary.globals.len()),
            ExportContextOwner::DeferredAmbientModule(id) => {
                id_in_range(id.0, primary.deferred_modules.len())
            }
        };
        if !owner_valid
            || source_owners
                .get(&context.source)
                .is_none_or(|(_, origin)| *origin != context.origin)
            || context
                .members
                .iter()
                .any(|id| !id_in_range(id.0, primary.members.len()))
        {
            return Err(invalid("export context reference is out of range"));
        }
    }
    let mut export_member_back_references = vec![0usize; primary.members.len()];
    for context in &primary.export_contexts {
        for id in &context.members {
            let Some(member) = primary.members.get(id.index()) else {
                return Err(invalid("export context member is out of range"));
            };
            if member.export_context != Some(context.id) {
                return Err(invalid(
                    "export context and namespace member disagree on ownership",
                ));
            }
            export_member_back_references[id.index()] += 1;
        }
    }
    for member in &primary.members {
        match member.export_context {
            Some(_) if export_member_back_references[member.id.index()] != 1 => {
                return Err(invalid(
                    "exported namespace member must occur in exactly one export context",
                ));
            }
            None if export_member_back_references[member.id.index()] != 0 => {
                return Err(invalid(
                    "non-export-context member occurs in an export context",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn encode_binder_snapshot(binder: &Binder) -> Result<Vec<u8>, SnapshotCodecError> {
    validate_binder(binder, 0)?;
    let mut writer = SnapshotWriter::new();
    writer.u32(BINDER_SNAPSHOT_VERSION);
    encode_scopes(&mut writer, &binder.graph)?;
    encode_symbols(&mut writer, &binder.symbols)?;
    encode_declarations(&mut writer, &binder.declarations)?;
    encode_type_groups(&mut writer, &binder.type_groups)?;
    encode_namespaces(&mut writer, &binder.namespaces)?;
    write_scope_id(&mut writer, binder.module);
    write_scope_id(&mut writer, binder.prelude_module);
    write_scope_id(&mut writer, binder.compilation_global);
    write_scope_id(&mut writer, binder.script_namespace_root);
    writer.u32(binder.decl_count);
    writer.u32(binder.prelude_type_group_count);
    let mut module_sources = binder.snapshot_module_sources().iter().collect::<Vec<_>>();
    module_sources.sort_by_key(|(scope, _)| scope.0);
    write_len(&mut writer, module_sources.len())?;
    for (scope, source) in module_sources {
        write_scope_id(&mut writer, *scope);
        write_source_key(&mut writer, *source);
    }
    encode_retained_scope_maps_into(&mut writer, binder)?;
    Ok(writer.into_bytes())
}

pub(crate) fn encode_retained_scope_maps(binder: &Binder) -> Result<Vec<u8>, SnapshotCodecError> {
    let mut writer = SnapshotWriter::new();
    encode_retained_scope_maps_into(&mut writer, binder)?;
    Ok(writer.into_bytes())
}

fn encode_retained_scope_maps_into(
    writer: &mut SnapshotWriter,
    binder: &Binder,
) -> Result<(), SnapshotCodecError> {
    encode_scope_map(writer, &binder.fn_scopes)?;
    encode_value_map(writer, &binder.fn_decl_ids)?;
    encode_scope_map(writer, &binder.block_scopes)
}

fn encode_scope_map(
    writer: &mut SnapshotWriter,
    map: &FxHashMap<(ScopeId, u32), ScopeId>,
) -> Result<(), SnapshotCodecError> {
    let mut rows = map.iter().collect::<Vec<_>>();
    rows.sort_by_key(|((module, start), target)| (module.0, *start, target.0));
    write_len(writer, rows.len())?;
    for ((module, start), target) in rows {
        write_scope_id(writer, *module);
        writer.u32(*start);
        write_scope_id(writer, *target);
    }
    Ok(())
}

fn encode_value_map(
    writer: &mut SnapshotWriter,
    map: &FxHashMap<(ScopeId, u32), ValueStorageId>,
) -> Result<(), SnapshotCodecError> {
    let mut rows = map.iter().collect::<Vec<_>>();
    rows.sort_by_key(|((module, start), target)| (module.0, *start, target.0));
    write_len(writer, rows.len())?;
    for ((module, start), target) in rows {
        write_scope_id(writer, *module);
        writer.u32(*start);
        writer.u32(target.0);
    }
    Ok(())
}

fn decode_scope_map(
    reader: &mut SnapshotReader<'_>,
) -> Result<FxHashMap<(ScopeId, u32), ScopeId>, SnapshotCodecError> {
    let mut map = FxHashMap::default();
    for _ in 0..read_len(reader)? {
        let key = (read_scope_id(reader)?, reader.u32()?);
        let target = read_scope_id(reader)?;
        if map.insert(key, target).is_some() {
            return Err(invalid(reader, "duplicate retained scope-map key"));
        }
    }
    Ok(map)
}

fn decode_value_map(
    reader: &mut SnapshotReader<'_>,
) -> Result<FxHashMap<(ScopeId, u32), ValueStorageId>, SnapshotCodecError> {
    let mut map = FxHashMap::default();
    for _ in 0..read_len(reader)? {
        let key = (read_scope_id(reader)?, reader.u32()?);
        let target = ValueStorageId(reader.u32()?);
        if map.insert(key, target).is_some() {
            return Err(invalid(reader, "duplicate retained value-map key"));
        }
    }
    Ok(map)
}

pub(crate) struct RetainedScopeMapSnapshotEvidence {
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "retains the authenticated binder byte range")
    )]
    bytes: Range<usize>,
    sha256: [u8; 32],
}

impl RetainedScopeMapSnapshotEvidence {
    pub(crate) fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
}

pub(crate) struct DecodedBinderSnapshot {
    pub(crate) binder: Binder,
    pub(crate) retained_scope_maps: RetainedScopeMapSnapshotEvidence,
}

pub(crate) fn decode_binder_snapshot_with_evidence(
    bytes: &[u8],
) -> Result<DecodedBinderSnapshot, SnapshotCodecError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader.u32()? != BINDER_SNAPSHOT_VERSION {
        return Err(invalid(&reader, "unsupported binder snapshot version"));
    }
    let graph = decode_scopes(&mut reader)?;
    let symbols = decode_symbols(&mut reader)?;
    let declarations = decode_declarations(&mut reader)?;
    let type_groups = decode_type_groups(&mut reader)?;
    let namespaces = decode_namespaces(&mut reader)?;
    let module = read_scope_id(&mut reader)?;
    let prelude_module = read_scope_id(&mut reader)?;
    let compilation_global = read_scope_id(&mut reader)?;
    let script_namespace_root = read_scope_id(&mut reader)?;
    let decl_count = reader.u32()?;
    let prelude_type_group_count = reader.u32()?;
    let mut module_sources = FxHashMap::default();
    for _ in 0..read_len(&mut reader)? {
        let scope = read_scope_id(&mut reader)?;
        let source = read_source_key(&mut reader)?;
        if module_sources.insert(scope, source).is_some() {
            return Err(invalid(&reader, "duplicate module source scope"));
        }
    }
    let retained_scope_maps_start = reader.position();
    let fn_scopes = decode_scope_map(&mut reader)?;
    let fn_decl_ids = decode_value_map(&mut reader)?;
    let block_scopes = decode_scope_map(&mut reader)?;
    let retained_scope_maps_end = reader.position();
    reader.finish()?;
    let binder = Binder::from_snapshot_parts(
        graph,
        symbols,
        declarations,
        type_groups,
        namespaces,
        module,
        prelude_module,
        compilation_global,
        script_namespace_root,
        decl_count,
        prelude_type_group_count,
        module_sources,
        fn_scopes,
        fn_decl_ids,
        block_scopes,
    );
    validate_binder(&binder, bytes.len())?;
    let retained_scope_maps = retained_scope_maps_start..retained_scope_maps_end;
    let retained_scope_map_bytes = bytes
        .get(retained_scope_maps.clone())
        .ok_or_else(|| SnapshotCodecError::invalid(bytes.len(), "retained-map range is invalid"))?;
    Ok(DecodedBinderSnapshot {
        binder,
        retained_scope_maps: RetainedScopeMapSnapshotEvidence {
            bytes: retained_scope_maps,
            sha256: Sha256::digest(retained_scope_map_bytes).into(),
        },
    })
}

#[cfg_attr(
    not(test),
    allow(dead_code, reason = "preserves the generic binder decode seam")
)]
pub(crate) fn decode_binder_snapshot(bytes: &[u8]) -> Result<Binder, SnapshotCodecError> {
    decode_binder_snapshot_with_evidence(bytes).map(|decoded| decoded.binder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::bind::{bind_module_with_prelude, ProjectBinderBuilder};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn fixture_binder() -> Binder {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(
            &prelude_allocator,
            "interface PreludeShape { ready: boolean }",
            SourceType::ts(),
        )
        .parse();
        let source = Parser::new(
            &source_allocator,
            concat!(
                "interface Shape { value: string }\n",
                "declare namespace Shape { const version: number; namespace Nested { const ok: boolean } }\n",
                "declare namespace Exports { const local: number; export { local as publicLocal }; }\n",
                "declare function callable(value: Shape): void;\n",
            ),
            SourceType::d_ts(),
        )
        .parse();
        assert!(prelude.diagnostics.is_empty());
        assert!(source.diagnostics.is_empty());
        bind_module_with_prelude(&prelude.program, &source.program)
    }

    fn fixture_library_binder() -> Binder {
        let prelude_allocator = Allocator::default();
        let alpha_allocator = Allocator::default();
        let bravo_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let alpha = Parser::new(
            &alpha_allocator,
            "interface Shared { alpha: string } declare namespace Shared { const one: 1 }",
            SourceType::d_ts(),
        )
        .parse();
        let bravo = Parser::new(
            &bravo_allocator,
            "interface Shared { bravo: number } declare namespace Shared { const two: 2 }",
            SourceType::d_ts(),
        )
        .parse();
        let units = [
            (
                &alpha.program,
                CompilationUnit::library(
                    SourceUnitKey(1),
                    LibraryFileOrdinal::new(0),
                    &alpha.program,
                ),
            ),
            (
                &bravo.program,
                CompilationUnit::library(
                    SourceUnitKey(2),
                    LibraryFileOrdinal::new(1),
                    &bravo.program,
                ),
            ),
        ];
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let modules = builder.add_library_modules(&units);
        builder.finish(modules[1])
    }

    fn fixture_rich_reference_binder() -> Binder {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(
            &prelude_allocator,
            "interface PreludeType { ready: boolean } declare const preludeValue: number;",
            SourceType::d_ts(),
        )
        .parse();
        let source = Parser::new(
            &source_allocator,
            concat!(
                "export {};\n",
                "export interface RootType { value: string }\n",
                "export function callable(value: string): string;\n",
                "export function callable(value: number): number;\n",
                "export declare namespace Outer {\n",
                "  const runtime: number;\n",
                "  interface Item { id: string }\n",
                "  export { runtime as publicRuntime };\n",
                "}\n",
                "export declare namespace Second { const other: number; }\n",
                "declare global { interface GlobalType { global: true } const globalValue: number; }\n",
                "declare module 'pkg' { export interface Remote { remote: true } export const remoteValue: number; }\n",
                "export as namespace PackageRoot;\n",
            ),
            SourceType::d_ts(),
        )
        .parse();
        assert!(prelude.diagnostics.is_empty(), "{:?}", prelude.diagnostics);
        assert!(source.diagnostics.is_empty(), "{:?}", source.diagnostics);
        bind_module_with_prelude(&prelude.program, &source.program)
    }

    #[test]
    fn binder_reference_inventory_is_exhaustive_canonical_and_bounded() {
        let binder = fixture_rich_reference_binder();
        let records = snapshot_reference_records_for_test(&binder);
        assert_eq!(records.len(), 313, "rich-fixture manifest is append-only");
        assert!(records.windows(2).all(|rows| rows[0] <= rows[1]));
        assert!(records.iter().all(|record| record.2 <= 31));

        let owner_domains = records
            .iter()
            .map(|record| record.0)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            owner_domains,
            BTreeSet::from([
                REF_SCOPE,
                REF_SYMBOL,
                REF_DECLARATION,
                REF_TYPE_GROUP,
                REF_NAMESPACE,
                REF_SOURCE_UNIT,
                REF_NAMESPACE_FRAGMENT,
                REF_NAMESPACE_MEMBER,
                REF_GLOBAL_AUGMENTATION,
                REF_DEFERRED_MODULE,
                REF_EXPORT_CONTEXT,
                REF_ROOT_ROW,
                REF_MERGE_PLACEMENT,
                REF_DEFERRED_CHILD,
                REF_UMD_EXPORT,
                REF_DECLARATION_SITE_INDEX,
                REF_NAMESPACE_KEY_INDEX,
                REF_CANONICAL_NAMESPACE_INDEX,
                REF_CANONICAL_SOURCE_UNIT_INDEX,
                REF_CANONICAL_GLOBAL_INDEX,
                REF_CANONICAL_DEFERRED_MODULE_INDEX,
                REF_CANONICAL_DEFERRED_CHILD_INDEX,
                REF_CANONICAL_UMD_EXPORT_INDEX,
                REF_CANONICAL_EXPORT_CONTEXT_INDEX,
            ])
        );

        let required_edges = [
            (REF_SCOPE, REF_SCOPE, SCOPE_PARENT),
            (REF_SCOPE, REF_SYMBOL, SCOPE_LOCAL_SYMBOL),
            (REF_SCOPE, REF_SOURCE_UNIT, SCOPE_MODULE_SOURCE),
            (REF_SYMBOL, REF_VALUE_STORAGE, SYMBOL_VALUE),
            (REF_SYMBOL, REF_VALUE_STORAGE, SYMBOL_FUNCTION_VALUE),
            (REF_SYMBOL, REF_TYPE_GROUP, SYMBOL_TYPE_GROUP),
            (REF_SYMBOL, REF_NAMESPACE, SYMBOL_NAMESPACE),
            (REF_SYMBOL, REF_DECLARATION, SYMBOL_DECLARATION),
            (REF_DECLARATION, REF_SCOPE, DECLARATION_MODULE),
            (REF_DECLARATION, REF_SCOPE, DECLARATION_SCOPE),
            (REF_DECLARATION, REF_VALUE_STORAGE, DECLARATION_VALUE),
            (REF_DECLARATION, REF_TYPE_GROUP, DECLARATION_TYPE_GROUP),
            (REF_DECLARATION, REF_NAMESPACE, DECLARATION_NAMESPACE),
            (
                REF_TYPE_GROUP,
                REF_DECLARATION,
                TYPE_GROUP_FRAGMENT_DECLARATION,
            ),
            (REF_NAMESPACE, REF_SCOPE, NAMESPACE_PUBLIC_SCOPE),
            (REF_NAMESPACE, REF_SYMBOL, NAMESPACE_SYMBOL),
            (REF_NAMESPACE, REF_NAMESPACE_FRAGMENT, NAMESPACE_FRAGMENT),
            (
                REF_NAMESPACE_FRAGMENT,
                REF_NAMESPACE_MEMBER,
                FRAGMENT_MEMBER,
            ),
            (
                REF_NAMESPACE_MEMBER,
                REF_EXPORT_CONTEXT,
                MEMBER_EXPORT_CONTEXT,
            ),
            (REF_MERGE_PLACEMENT, REF_DECLARATION, PLACEMENT_DECLARATION),
            (REF_GLOBAL_AUGMENTATION, REF_SCOPE, GLOBAL_OVERLAY_SCOPE),
            (
                REF_DEFERRED_MODULE,
                REF_DECLARATION,
                DEFERRED_MODULE_DECLARATION,
            ),
            (
                REF_DEFERRED_CHILD,
                REF_DEFERRED_MODULE,
                DEFERRED_CHILD_MODULE,
            ),
            (REF_UMD_EXPORT, REF_DECLARATION, UMD_DECLARATION),
            (
                REF_EXPORT_CONTEXT,
                REF_NAMESPACE_MEMBER,
                EXPORT_CONTEXT_MEMBER,
            ),
            (REF_SOURCE_UNIT, REF_SCOPE, SOURCE_UNIT_MODULE),
            (
                REF_DECLARATION_SITE_INDEX,
                REF_SCOPE,
                DECLARATION_SITE_KEY_SCOPE,
            ),
            (
                REF_DECLARATION_SITE_INDEX,
                REF_DECLARATION,
                DECLARATION_SITE_TARGET,
            ),
            (REF_NAMESPACE_KEY_INDEX, REF_NAMESPACE, NAMESPACE_KEY_TARGET),
            (
                REF_CANONICAL_NAMESPACE_INDEX,
                REF_NAMESPACE,
                CANONICAL_INDEX_TARGET,
            ),
            (
                REF_CANONICAL_SOURCE_UNIT_INDEX,
                REF_SOURCE_UNIT,
                CANONICAL_INDEX_TARGET,
            ),
            (
                REF_CANONICAL_GLOBAL_INDEX,
                REF_GLOBAL_AUGMENTATION,
                CANONICAL_INDEX_TARGET,
            ),
            (
                REF_CANONICAL_DEFERRED_MODULE_INDEX,
                REF_DEFERRED_MODULE,
                CANONICAL_INDEX_TARGET,
            ),
            (
                REF_CANONICAL_DEFERRED_CHILD_INDEX,
                REF_DEFERRED_CHILD,
                CANONICAL_INDEX_TARGET,
            ),
            (
                REF_CANONICAL_UMD_EXPORT_INDEX,
                REF_UMD_EXPORT,
                CANONICAL_INDEX_TARGET,
            ),
            (
                REF_CANONICAL_EXPORT_CONTEXT_INDEX,
                REF_EXPORT_CONTEXT,
                CANONICAL_INDEX_TARGET,
            ),
        ];
        for required in required_edges {
            assert!(
                records
                    .iter()
                    .any(|record| (record.0, record.1, record.2) == required),
                "missing reference edge {required:?}"
            );
        }

        let primary = binder.namespaces.snapshot_primary();
        let count = |domain| records.iter().filter(|record| record.0 == domain).count();
        assert_eq!(
            count(REF_DECLARATION_SITE_INDEX),
            binder.declarations.len() * 2
        );
        assert_eq!(count(REF_NAMESPACE_KEY_INDEX), binder.namespaces.len() * 2);
        assert_eq!(
            count(REF_CANONICAL_NAMESPACE_INDEX),
            primary.namespaces.len()
        );
        assert_eq!(
            count(REF_CANONICAL_SOURCE_UNIT_INDEX),
            primary.source_units.len()
        );
        assert_eq!(count(REF_CANONICAL_GLOBAL_INDEX), primary.globals.len());
        assert_eq!(
            count(REF_CANONICAL_DEFERRED_MODULE_INDEX),
            primary.deferred_modules.len()
        );
        assert_eq!(
            count(REF_CANONICAL_DEFERRED_CHILD_INDEX),
            primary.deferred_children.len()
        );
        assert_eq!(
            count(REF_CANONICAL_UMD_EXPORT_INDEX),
            primary.umd_exports.len()
        );
        assert_eq!(
            count(REF_CANONICAL_EXPORT_CONTEXT_INDEX),
            primary.export_contexts.len()
        );

        let bytes = encode_binder_snapshot(&binder).expect("rich binder encodes");
        let decoded = decode_binder_snapshot(&bytes).expect("rich binder decodes");
        assert_eq!(snapshot_reference_records_for_test(&decoded), records);
    }

    #[test]
    fn local_reference_view_matches_full_state_and_excludes_frozen_owners() {
        let all_local = fixture_rich_reference_binder();
        assert_eq!(
            local_snapshot_reference_records_for_test(&all_local),
            snapshot_reference_records_for_test(&all_local),
        );

        let mut base = fixture_rich_reference_binder();
        let base_records = snapshot_reference_records_for_test(&base);
        let mut base_domain_ends = std::collections::BTreeMap::<u8, u32>::new();
        for &(owner_domain, target_domain, _, owner, target) in &base_records {
            base_domain_ends
                .entry(owner_domain)
                .and_modify(|end| *end = (*end).max(owner.saturating_add(1)))
                .or_insert(owner.saturating_add(1));
            base_domain_ends
                .entry(target_domain)
                .and_modify(|end| *end = (*end).max(target.saturating_add(1)))
                .or_insert(target.saturating_add(1));
        }
        base.freeze_as_base().expect("reference fixture freezes");

        let allocator = Allocator::default();
        let source = Parser::new(
            &allocator,
            "export namespace Local { export interface Item {} export const value = 1; }",
            SourceType::ts(),
        )
        .parse();
        assert!(source.diagnostics.is_empty());
        let (mut builder, source_key) =
            ProjectBinderBuilder::resume_frozen_library(base.fork_delta().expect("binder delta"));
        let unit = CompilationUnit::implementation(source_key, &source.program);
        builder.reserve_script_namespace_roots([(&source.program, unit)]);
        let (module, _) = builder.add_module(&source.program, &[], unit);
        let delta = builder
            .finish_frozen_library_continuation(module)
            .expect("reference delta finishes");
        let local = local_snapshot_reference_records_for_test(&delta);

        assert!(local.iter().all(|record| {
            record.0 == REF_ROOT_ROW
                || base_domain_ends
                    .get(&record.0)
                    .is_none_or(|base_end| record.3 >= *base_end)
        }));
        assert!(local.iter().any(|record| {
            base_domain_ends
                .get(&record.1)
                .is_some_and(|base_end| record.4 < *base_end)
        }));
        assert!(local.iter().any(|record| {
            base_domain_ends
                .get(&record.1)
                .is_some_and(|base_end| record.4 >= *base_end)
        }));
    }

    #[test]
    fn binder_reference_inventory_is_order_independent_and_tracks_mutated_roots() {
        let mut reordered = fixture_rich_reference_binder();
        let original = snapshot_reference_records_for_test(&reordered);
        let module = reordered
            .graph
            .get_mut(reordered.module)
            .expect("fixture module scope");
        let mut symbols = std::mem::take(&mut module.symbols)
            .into_iter()
            .collect::<Vec<_>>();
        symbols.reverse();
        module.symbols.extend(symbols);
        assert_eq!(snapshot_reference_records_for_test(&reordered), original);

        let old_module = reordered.module;
        reordered.module = reordered.prelude_module;
        let mut expected = original;
        let old_root = (REF_ROOT_ROW, REF_SCOPE, ROOT_MODULE, 0, old_module.0);
        let position = expected
            .iter()
            .position(|record| *record == old_root)
            .expect("binder module root record");
        expected[position].4 = reordered.prelude_module.0;
        expected.sort_unstable();
        assert_eq!(snapshot_reference_records_for_test(&reordered), expected);

        let mut changed_site = fixture_rich_reference_binder();
        let original = snapshot_reference_records_for_test(&changed_site);
        let replacement_module = changed_site.prelude_module;
        let declaration = changed_site
            .declarations
            .iter()
            .find(|declaration| declaration.site.module != replacement_module)
            .map(|declaration| declaration.id)
            .expect("fixture source declaration");
        let old_module = changed_site
            .declarations
            .get(declaration)
            .expect("fixture declaration")
            .site
            .module;
        changed_site
            .declarations
            .get_mut(declaration)
            .expect("fixture declaration")
            .site
            .module = replacement_module;
        let mut expected = original;
        for (owner_domain, field) in [
            (REF_DECLARATION, DECLARATION_MODULE),
            (REF_DECLARATION_SITE_INDEX, DECLARATION_SITE_KEY_SCOPE),
        ] {
            let old = (owner_domain, REF_SCOPE, field, declaration.0, old_module.0);
            let position = expected
                .iter()
                .position(|record| *record == old)
                .expect("declaration module reference");
            expected[position].4 = replacement_module.0;
        }
        expected.sort_unstable();
        assert_eq!(snapshot_reference_records_for_test(&changed_site), expected);

        let canonical_binder = fixture_rich_reference_binder();
        let mut targets = canonical_binder
            .namespaces
            .namespaces()
            .map(|namespace| namespace.id.0)
            .take(2)
            .collect::<Vec<_>>();
        assert_eq!(targets.len(), 2, "fixture has two valid namespace ids");
        let mut canonical = Vec::new();
        push_canonical_index_references(
            &mut canonical,
            REF_CANONICAL_NAMESPACE_INDEX,
            REF_NAMESPACE,
            0,
            targets.iter().copied(),
        )
        .expect("canonical index references enumerate");
        targets.swap(0, 1);
        let mut swapped = Vec::new();
        push_canonical_index_references(
            &mut swapped,
            REF_CANONICAL_NAMESPACE_INDEX,
            REF_NAMESPACE,
            0,
            targets.iter().copied(),
        )
        .expect("canonical index references enumerate");
        assert_eq!(
            canonical,
            vec![
                (
                    REF_CANONICAL_NAMESPACE_INDEX,
                    REF_NAMESPACE,
                    CANONICAL_INDEX_TARGET,
                    0,
                    targets[1],
                ),
                (
                    REF_CANONICAL_NAMESPACE_INDEX,
                    REF_NAMESPACE,
                    CANONICAL_INDEX_TARGET,
                    1,
                    targets[0],
                ),
            ]
        );
        assert_eq!(
            swapped,
            vec![
                (
                    REF_CANONICAL_NAMESPACE_INDEX,
                    REF_NAMESPACE,
                    CANONICAL_INDEX_TARGET,
                    0,
                    targets[0],
                ),
                (
                    REF_CANONICAL_NAMESPACE_INDEX,
                    REF_NAMESPACE,
                    CANONICAL_INDEX_TARGET,
                    1,
                    targets[1],
                ),
            ]
        );
    }

    #[test]
    fn binder_snapshot_roundtrips_complete_ast_free_state() {
        let binder = fixture_binder();
        let bytes = encode_binder_snapshot(&binder).expect("binder encodes");
        let decoded_with_evidence =
            decode_binder_snapshot_with_evidence(&bytes).expect("binder decodes");
        let retained_scope_maps =
            encode_retained_scope_maps(&binder).expect("retained scope maps encode");
        assert_eq!(
            &bytes[decoded_with_evidence.retained_scope_maps.bytes.clone()],
            retained_scope_maps
        );
        let retained_scope_maps_sha256: [u8; 32] = Sha256::digest(&retained_scope_maps).into();
        assert_eq!(
            decoded_with_evidence.retained_scope_maps.sha256(),
            retained_scope_maps_sha256
        );
        let decoded = decoded_with_evidence.binder;
        assert_eq!(encode_binder_snapshot(&decoded), Ok(bytes));
        assert_eq!(decoded.fn_scopes, binder.fn_scopes);
        assert_eq!(decoded.fn_decl_ids, binder.fn_decl_ids);
        assert_eq!(decoded.block_scopes, binder.block_scopes);
        assert_eq!(
            decoded.resolve_type(decoded.module, "Shape"),
            binder.resolve_type(binder.module, "Shape")
        );
        assert_eq!(
            decoded.resolve_value(decoded.module, "callable"),
            binder.resolve_value(binder.module, "callable")
        );
    }

    #[test]
    fn binder_snapshot_rejects_truncation_and_invalid_roots() {
        let bytes = encode_binder_snapshot(&fixture_binder()).expect("binder encodes");
        assert!(decode_binder_snapshot(&bytes[..bytes.len() - 1]).is_err());
        let mut invalid = bytes;
        let mut reader = SnapshotReader::new(&invalid);
        assert_eq!(reader.u32(), Ok(BINDER_SNAPSHOT_VERSION));
        decode_scopes(&mut reader).expect("scope section");
        decode_symbols(&mut reader).expect("symbol section");
        decode_declarations(&mut reader).expect("declaration section");
        decode_type_groups(&mut reader).expect("type-group section");
        decode_namespaces(&mut reader).expect("namespace section");
        let root_offset = reader.position();
        invalid[root_offset..root_offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode_binder_snapshot(&invalid).is_err());
    }

    #[test]
    fn binder_snapshot_never_panics_on_single_byte_corruption() {
        let bytes = encode_binder_snapshot(&fixture_library_binder()).expect("binder encodes");
        for index in 0..bytes.len() {
            let mut corrupted = bytes.clone();
            corrupted[index] ^= 0x80;
            let decoded = std::panic::catch_unwind(|| decode_binder_snapshot(&corrupted));
            let decoded = decoded
                .unwrap_or_else(|_| panic!("single-byte corruption panicked at offset {index}"));
            if let Ok(decoded) = decoded {
                assert_eq!(
                    encode_binder_snapshot(&decoded),
                    Ok(corrupted),
                    "accepted bytes must already be canonical at offset {index}"
                );
            }
        }
    }

    #[test]
    fn binder_snapshot_rejects_cross_table_invariant_breaks() {
        let mut root_kind = fixture_binder();
        root_kind
            .graph
            .get_mut(root_kind.compilation_global)
            .expect("fixture compilation global")
            .kind = ScopeKind::Block;
        assert!(encode_binder_snapshot(&root_kind).is_err());

        let mut parent_cycle = fixture_binder();
        parent_cycle
            .graph
            .get_mut(parent_cycle.prelude_module)
            .expect("fixture prelude")
            .parent = Some(parent_cycle.module);
        assert!(encode_binder_snapshot(&parent_cycle).is_err());

        let mut sparse_values = fixture_binder();
        sparse_values.decl_count += 1;
        assert!(encode_binder_snapshot(&sparse_values).is_err());

        let mut type_owner = fixture_binder();
        let (declaration, group) = type_owner
            .type_groups
            .iter()
            .find_map(|group| {
                group
                    .fragments
                    .first()
                    .map(|fragment| (fragment.declaration, group.id))
            })
            .expect("fixture type group");
        assert_eq!(
            type_owner
                .declarations
                .get(declaration)
                .and_then(|row| row.type_group),
            Some(group)
        );
        type_owner
            .declarations
            .get_mut(declaration)
            .expect("fixture declaration")
            .type_group = None;
        assert!(encode_binder_snapshot(&type_owner).is_err());

        let namespace = fixture_binder();
        let mut primary = namespace.namespaces.snapshot_primary();
        let context = primary
            .export_contexts
            .iter()
            .find(|context| !context.members.is_empty())
            .map(|context| (context.id, context.members[0]));
        let (context, member) = context.expect("fixture export context with a member");
        assert_eq!(
            primary.members[member.index()].export_context,
            Some(context)
        );
        primary.members[member.index()].export_context = None;
        let mut broken = namespace;
        broken.namespaces =
            NamespaceTable::from_snapshot_primary(primary).expect("classification-safe state");
        assert!(encode_binder_snapshot(&broken).is_err());

        let mut wrong_member_origin = fixture_binder();
        let mut primary = wrong_member_origin.namespaces.snapshot_primary();
        primary.members[0].origin =
            CompilationOrigin::User(OriginalModuleOrdinal::new(usize::MAX / 2));
        wrong_member_origin.namespaces =
            NamespaceTable::from_snapshot_primary(primary).expect("classification-safe state");
        assert!(encode_binder_snapshot(&wrong_member_origin).is_err());

        let mut wrong_context_origin = fixture_binder();
        let mut primary = wrong_context_origin.namespaces.snapshot_primary();
        primary.export_contexts[0].origin =
            CompilationOrigin::User(OriginalModuleOrdinal::new(usize::MAX / 3));
        wrong_context_origin.namespaces =
            NamespaceTable::from_snapshot_primary(primary).expect("classification-safe state");
        assert!(encode_binder_snapshot(&wrong_context_origin).is_err());

        let wrong_participant_origin = fixture_library_binder();
        let mut primary = wrong_participant_origin.namespaces.snapshot_primary();
        primary.placements[0].2[0].origin =
            CompilationOrigin::Library(LibraryFileOrdinal::new(usize::MAX / 4));
        assert!(NamespaceTable::from_snapshot_primary(primary).is_err());
    }

    #[test]
    fn namespace_snapshot_rejects_noncanonical_derived_values_and_ordering() {
        let binder = fixture_library_binder();

        let mut fragment_state = binder.namespaces.snapshot_primary();
        let fragment = fragment_state
            .fragments
            .first_mut()
            .expect("fixture namespace fragment");
        fragment.instance_state = match fragment.instance_state {
            NamespaceInstanceState::NonInstantiated => NamespaceInstanceState::Instantiated,
            NamespaceInstanceState::Instantiated => NamespaceInstanceState::NonInstantiated,
        };
        assert!(NamespaceTable::from_snapshot_primary(fragment_state).is_err());

        let mut participant_state = binder.namespaces.snapshot_primary();
        let participant = participant_state
            .placements
            .iter_mut()
            .flat_map(|(_, _, participants)| participants)
            .find(|participant| participant.namespace_instance.is_some())
            .expect("fixture namespace merge participant");
        participant.namespace_instance = participant.namespace_instance.map(|state| match state {
            NamespaceInstanceState::NonInstantiated => NamespaceInstanceState::Instantiated,
            NamespaceInstanceState::Instantiated => NamespaceInstanceState::NonInstantiated,
        });
        assert!(NamespaceTable::from_snapshot_primary(participant_state).is_err());

        let mut fragment_order = binder.namespaces.snapshot_primary();
        let fragments = fragment_order
            .namespaces
            .iter_mut()
            .find(|namespace| namespace.fragments.len() >= 2)
            .map(|namespace| &mut namespace.fragments)
            .expect("fixture reopened namespace");
        fragments.swap(0, 1);
        assert!(NamespaceTable::from_snapshot_primary(fragment_order).is_err());

        let mut participant_order = binder.namespaces.snapshot_primary();
        let participants = participant_order
            .placements
            .iter_mut()
            .find(|(_, _, participants)| participants.len() >= 2)
            .map(|(_, _, participants)| participants)
            .expect("fixture reopened merge placement");
        participants.swap(0, 1);
        assert!(NamespaceTable::from_snapshot_primary(participant_order).is_err());
    }

    #[test]
    fn binder_snapshot_rejects_type_group_fragment_site_scope_and_source_corruption() {
        let binder = fixture_library_binder();
        let (group, fragment) = binder
            .type_groups
            .iter()
            .find_map(|group| {
                group
                    .fragments
                    .first()
                    .map(|fragment| (group.id, *fragment))
            })
            .expect("fixture type group fragment");

        let mut wrong_site = fixture_library_binder();
        wrong_site
            .type_groups
            .get_mut(group)
            .expect("fixture type group")
            .fragments[0]
            .site
            .binding_span
            .end = fragment.site.binding_span.end.saturating_add(1);
        assert!(encode_binder_snapshot(&wrong_site).is_err());

        let mut wrong_scope = fixture_library_binder();
        let replacement_scope = if fragment.scope == wrong_scope.module {
            wrong_scope.prelude_module
        } else {
            wrong_scope.module
        };
        wrong_scope
            .type_groups
            .get_mut(group)
            .expect("fixture type group")
            .fragments[0]
            .scope = replacement_scope;
        assert!(encode_binder_snapshot(&wrong_scope).is_err());

        let mut wrong_source = fixture_library_binder();
        let replacement_source = wrong_source
            .snapshot_module_sources()
            .values()
            .copied()
            .find(|source| *source != fragment.source)
            .expect("fixture alternate valid source");
        wrong_source
            .type_groups
            .get_mut(group)
            .expect("fixture type group")
            .fragments[0]
            .source = replacement_source;
        assert!(encode_binder_snapshot(&wrong_source).is_err());
    }

    #[test]
    fn namespace_snapshot_rejects_unsafe_refs_before_classification() {
        let binder = fixture_library_binder();
        let mut invalid_member = binder.namespaces.snapshot_primary();
        invalid_member.fragments[0].members = vec![NamespaceMemberId(u32::MAX)];
        let result =
            std::panic::catch_unwind(|| NamespaceTable::from_snapshot_primary(invalid_member));
        assert!(result.is_ok(), "invalid member reference must not panic");
        assert!(result.expect("no panic").is_err());

        let mut empty_library_namespace = binder.namespaces.snapshot_primary();
        empty_library_namespace.namespaces[0].fragments.clear();
        let result = std::panic::catch_unwind(|| {
            NamespaceTable::from_snapshot_primary(empty_library_namespace)
        });
        assert!(result.is_ok(), "empty library namespace must not panic");
        assert!(result.expect("no panic").is_err());
    }

    #[test]
    fn binder_snapshot_preserves_library_source_ownership_and_merge_order() {
        let binder = fixture_library_binder();
        let bytes = encode_binder_snapshot(&binder).expect("library binder encodes");
        let decoded = decode_binder_snapshot(&bytes).expect("library binder decodes");
        assert_eq!(encode_binder_snapshot(&decoded), Ok(bytes));
        let group = decoded
            .resolve_type(decoded.compilation_global, "Shared")
            .and_then(|symbol| decoded.symbols.get(symbol))
            .and_then(|symbol| symbol.ty)
            .and_then(|group| decoded.type_groups.get(group))
            .expect("merged library group survives");
        assert_eq!(group.fragments.len(), 2);
        assert_eq!(group.fragments[0].source, SourceUnitKey(1));
        assert_eq!(group.fragments[1].source, SourceUnitKey(2));
    }
}
