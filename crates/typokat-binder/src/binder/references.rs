//! Typed reference projection for the AST-free binder prefix.
//!
//! Enumerates every identity edge the binder holds as `(owner_domain, target_domain,
//! field, owner, target)` rows, either over the whole state or over a delta's local
//! rows only. It backs the binder integrity specs and the base/delta leak check in
//! `library_compiler`; nothing here produces bytes.

use super::bind::Binder;
use super::declaration::{LexicalDeclaration, TypeGroup};
use super::namespace::*;
use super::scope::{Scope, ScopeId};
use super::symbol::Symbol;
#[cfg(test)]
use std::collections::BTreeSet;

pub type ReferenceRecord = (u8, u8, u8, u32, u32);

#[derive(Copy, Clone, PartialEq, Eq)]
enum ReferenceView {
    Full,
    #[cfg(any(test, feature = "test-utils"))]
    Local,
}

impl ReferenceView {
    const fn local_only(self) -> bool {
        match self {
            Self::Full => false,
            #[cfg(any(test, feature = "test-utils"))]
            Self::Local => true,
        }
    }
}

// Reference-record domains. These discriminants are append-only.
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
    records: &mut Vec<ReferenceRecord>,
    owner_domain: u8,
    target_domain: u8,
    field: u8,
    owner: u32,
    target: u32,
) {
    records.push((owner_domain, target_domain, field, owner, target));
}

fn push_namespace_owner_reference(
    records: &mut Vec<ReferenceRecord>,
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
    records: &mut Vec<ReferenceRecord>,
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
    records: &mut Vec<ReferenceRecord>,
    owner_domain: u8,
    target_domain: u8,
    owner_start: usize,
    targets: impl IntoIterator<Item = u32>,
) -> Result<(), &'static str> {
    for (index, target) in targets.into_iter().enumerate() {
        let owner =
            u32::try_from(owner_start + index).map_err(|_| "canonical index exceeds u32")?;
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

/// Enumerate every typed binder reference.
pub fn reference_records(binder: &Binder) -> Result<Vec<ReferenceRecord>, &'static str> {
    reference_records_with_view(binder, ReferenceView::Full)
}

fn reference_records_with_view(
    binder: &Binder,
    view: ReferenceView,
) -> Result<Vec<ReferenceRecord>, &'static str> {
    let mut records = Vec::new();
    let root = 0;
    let root_scope_is_in_view = |_scope: ScopeId| {
        if !view.local_only() {
            return true;
        }
        #[cfg(any(test, feature = "test-utils"))]
        {
            _scope.index() >= binder.graph.base_len_for_test()
        }
        #[cfg(not(any(test, feature = "test-utils")))]
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
        #[cfg(any(test, feature = "test-utils"))]
        for (owner, scope) in binder.graph.local_scopes() {
            record_scope(owner.0, scope);
        }
    } else {
        for (index, scope) in binder.graph.all_scopes().enumerate() {
            let owner = u32::try_from(index).map_err(|_| "scope index exceeds u32")?;
            record_scope(owner, scope);
        }
    }
    if view.local_only() {
        #[cfg(any(test, feature = "test-utils"))]
        for (scope, source) in binder.module_sources().local_iter() {
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
        for (scope, source) in binder.module_sources().iter() {
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
        #[cfg(any(test, feature = "test-utils"))]
        for (owner, symbol) in binder.symbols.local_symbols() {
            record_symbol(owner.0, symbol);
        }
    } else {
        for (index, symbol) in binder.symbols.all_symbols().enumerate() {
            let owner = u32::try_from(index).map_err(|_| "symbol index exceeds u32")?;
            record_symbol(owner, symbol);
        }
    }

    let declarations: Box<dyn Iterator<Item = &LexicalDeclaration>> = if view.local_only() {
        #[cfg(any(test, feature = "test-utils"))]
        {
            Box::new(binder.declarations.local_declarations())
        }
        #[cfg(not(any(test, feature = "test-utils")))]
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
        #[cfg(any(test, feature = "test-utils"))]
        {
            Box::new(binder.type_groups.local_groups())
        }
        #[cfg(not(any(test, feature = "test-utils")))]
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

    let namespace_rows = binder.namespaces.reference_rows(view.local_only());
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
            .map_err(|_| "merge placement index exceeds u32")?;
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
        #[cfg(any(test, feature = "test-utils"))]
        {
            Box::new(binder.namespaces.local_merges())
        }
        #[cfg(not(any(test, feature = "test-utils")))]
        unreachable!("local reference view is test-only")
    } else {
        Box::new(binder.namespaces.merges())
    };
    for (index, merge) in merges.enumerate() {
        let owner = u32::try_from(offsets.placements + index)
            .map_err(|_| "merge placement index exceeds u32")?;
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
            .map_err(|_| "deferred child index exceeds u32")?;
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
            .map_err(|_| "UMD export index exceeds u32")?;
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

#[cfg(any(test, feature = "test-utils"))]
pub fn reference_records_for_test(binder: &Binder) -> Vec<ReferenceRecord> {
    reference_records(binder).expect("typed binder references enumerate")
}

#[cfg(any(test, feature = "test-utils"))]
pub fn local_reference_records_for_test(binder: &Binder) -> Vec<ReferenceRecord> {
    reference_records_with_view(binder, ReferenceView::Local)
        .expect("typed local binder references enumerate")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::bind::{bind_module_with_prelude, ProjectBinderBuilder};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

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
        let records = reference_records_for_test(&binder);
        assert_eq!(records.len(), 317, "rich-fixture manifest is append-only");
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

        let primary = binder.namespaces.primary_rows();
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
    }

    #[test]
    fn local_reference_view_matches_full_state_and_excludes_frozen_owners() {
        let all_local = fixture_rich_reference_binder();
        assert_eq!(
            local_reference_records_for_test(&all_local),
            reference_records_for_test(&all_local),
        );

        let mut base = fixture_rich_reference_binder();
        let base_records = reference_records_for_test(&base);
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
        let local = local_reference_records_for_test(&delta);

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
        let original = reference_records_for_test(&reordered);
        let module = reordered
            .graph
            .get_mut(reordered.module)
            .expect("fixture module scope");
        let mut symbols = std::mem::take(&mut module.symbols)
            .into_iter()
            .collect::<Vec<_>>();
        symbols.reverse();
        module.symbols.extend(symbols);
        assert_eq!(reference_records_for_test(&reordered), original);

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
        assert_eq!(reference_records_for_test(&reordered), expected);

        let mut changed_site = fixture_rich_reference_binder();
        let original = reference_records_for_test(&changed_site);
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
        assert_eq!(reference_records_for_test(&changed_site), expected);

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
}
