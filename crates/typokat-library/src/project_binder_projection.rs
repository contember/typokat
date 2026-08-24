//! Candidate-only stable normalization of an already-built project binder.

use crate::binder::namespace::{
    DeclarationOwner, DeclarationSyntaxFacts, MergeDeclarationKind, MergeDisposition, NamespaceId,
    NamespaceInstanceState, NamespaceOwner, SourceUnitKey, VariableKind,
};
use crate::binder::scope::ScopeId;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeclarationProjection {
    pub(crate) scopes: Vec<String>,
    pub(crate) declarations: Vec<String>,
    pub(crate) groups: Vec<String>,
    pub(crate) namespaces: Vec<String>,
    pub(crate) attachments: Vec<String>,
    pub(crate) augmentations: Vec<String>,
    pub(crate) global_contributors: Vec<String>,
    pub(crate) modules: Vec<String>,
}

impl DeclarationProjection {
    pub(crate) fn digest(&self) -> String {
        let mut text = String::new();
        for (kind, rows) in [
            ("scope", &self.scopes),
            ("declaration", &self.declarations),
            ("group", &self.groups),
            ("namespace", &self.namespaces),
            ("attachment", &self.attachments),
            ("augmentation", &self.augmentations),
            ("global", &self.global_contributors),
            ("module", &self.modules),
        ] {
            for row in rows {
                text.push_str(kind);
                text.push('|');
                text.push_str(row);
                text.push('\n');
            }
        }
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }
}

pub(crate) fn normalize_project_binder(
    library_units: &[crate::binder::bind::LibraryBinderUnit],
    bound: &crate::check::checker::BoundProjectBinder,
) -> DeclarationProjection {
    let binder = &bound.binder;
    let mut source_names = library_units
        .iter()
        .map(|unit| (unit.source, format!("library:{:02}", unit.ordinal.index())))
        .collect::<BTreeMap<_, _>>();
    source_names.extend(
        bound
            .project_sources
            .iter()
            .map(|row| (row.source, format!("user:{}", row.normalized_path))),
    );
    let module_names = library_units
        .iter()
        .map(|unit| (unit.module.0, source_names[&unit.source].clone()))
        .chain(
            bound
                .project_sources
                .iter()
                .map(|row| (row.module.0, source_names[&row.source].clone())),
        )
        .collect();
    let context = ProjectionContext {
        binder,
        source_names,
        module_names,
    };

    let mut scopes = (0..binder.graph.len())
        .filter_map(|index| {
            let id = ScopeId(u32::try_from(index).ok()?);
            let row = binder.graph.get(id)?;
            Some(format!(
                "{}|kind={:?}|parent={}|public={}",
                context.scope(id),
                row.kind,
                row.parent
                    .map(|parent| context.scope(parent))
                    .unwrap_or_else(|| "none".to_owned()),
                row.namespace_public
                    .map(|public| context.scope(public))
                    .unwrap_or_else(|| "none".to_owned())
            ))
        })
        .collect::<Vec<_>>();
    scopes.sort();

    let mut declarations = binder
        .declarations
        .iter()
        .map(|declaration| {
            let source = binder
                .source_for_module(declaration.site.module)
                .map(|source| context.source(source))
                .unwrap_or_else(|| "prelude".to_owned());
            let scope = declaration
                .site
                .scope
                .map(|scope| context.scope(scope))
                .unwrap_or_else(|| "none".to_owned());
            format!(
                "{source}|{:?}|{}:{}|{}:{}|scope={scope}|v={}|t={}|n={}",
                declaration.kind,
                declaration.site.declaration_span.start,
                declaration.site.declaration_span.end,
                declaration.site.binding_span.start,
                declaration.site.binding_span.end,
                declaration
                    .value_storage
                    .map(|storage| context.value_storage(storage))
                    .unwrap_or_else(|| "none".to_owned()),
                declaration
                    .type_group
                    .map(|group| context.type_group(group))
                    .unwrap_or_else(|| "none".to_owned()),
                declaration
                    .namespace
                    .map(|namespace| context.namespace(namespace))
                    .unwrap_or_else(|| "none".to_owned())
            )
        })
        .collect::<Vec<_>>();
    declarations.sort();

    let mut groups = binder
        .type_groups
        .iter()
        .map(|group| {
            let fragments = group
                .fragments
                .iter()
                .map(|fragment| {
                    format!(
                        "{}:{}:{:?}:{}",
                        context.source(fragment.source),
                        fragment.site.binding_span.start,
                        fragment.kind,
                        context.scope(fragment.scope)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let containers = group
                .fragments
                .iter()
                .map(|fragment| context.scope(fragment.scope))
                .collect::<BTreeSet<_>>();
            let mut slots = BTreeSet::new();
            for fragment in &group.fragments {
                if let Some(symbol) = binder
                    .graph
                    .resolve(fragment.scope, &group.name)
                    .and_then(|symbol| binder.symbols.get(symbol))
                {
                    if let Some(storage) = symbol.value {
                        slots.insert(format!("value={}", context.value_storage(storage)));
                    }
                    if let Some(group) = symbol.ty {
                        slots.insert(format!("type={}", context.type_group(group)));
                    }
                    if let Some(namespace) = symbol.ns {
                        slots.insert(format!("namespace={}", context.namespace(namespace)));
                    }
                }
            }
            format!(
                "{}|containers={containers:?}|slots={slots:?}|{fragments}",
                group.name
            )
        })
        .collect::<Vec<_>>();
    groups.sort();

    let mut namespaces = binder
        .namespaces
        .namespaces()
        .map(|namespace| {
            let fragments = namespace
                .fragments
                .iter()
                .filter_map(|id| binder.namespaces.fragment(*id))
                .map(|fragment| {
                    let members = fragment
                        .members
                        .iter()
                        .filter_map(|member| binder.namespaces.member(*member))
                        .map(|member| {
                            format!(
                                "{}:{}:{:?}:{:?}:{:?}:target={}:decl={}:symbol={}",
                                context.source(member.source),
                                member.binding_span.start,
                                member.name,
                                member.spaces,
                                member.kind,
                                context.declaration_owner(member.target),
                                member
                                    .declaration
                                    .map(|id| context.declaration(id))
                                    .unwrap_or_else(|| "none".to_owned()),
                                member
                                    .symbol
                                    .map(|id| context.symbol(id))
                                    .unwrap_or_else(|| "none".to_owned())
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("+");
                    format!(
                        "{}:{}:parent={}:private={}:public={}:members={members}",
                        context.source(fragment.source),
                        fragment.source_start,
                        context.scope(fragment.lexical_parent),
                        context.scope(fragment.private_scope),
                        context.scope(fragment.public_scope)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{}|owner={}|public={}|fragments={fragments}|standalone={}",
                namespace.name,
                context.namespace_owner(namespace.owner),
                context.scope(namespace.public_scope),
                binder
                    .namespaces
                    .standalone_value_storage(namespace.id)
                    .map(|storage| context.value_storage(storage))
                    .unwrap_or_else(|| "none".to_owned())
            )
        })
        .collect::<Vec<_>>();
    namespaces.sort();

    let mut attachments = binder
        .namespaces
        .merges()
        .filter_map(|merge| {
            binder
                .namespace_value_attachment_for_owner(merge.owner, &merge.name)
                .map(|attachment| {
                    let fragments = attachment
                        .fragments
                        .iter()
                        .map(|fragment| {
                            format!(
                                "{}:{}:{}",
                                context.namespace(fragment.namespace),
                                context.source(fragment.source),
                                fragment.source_start
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let members = attachment
                        .members
                        .iter()
                        .map(|member| {
                            format!(
                                "{}:{}:{}:decl={}:value={}:symbol={}:kind={:?}:publication={:?}",
                                context.source(member.source),
                                member.site.binding_span.start,
                                member.name,
                                context.declaration(member.declaration),
                                member
                                    .value_storage
                                    .map(|storage| context.value_storage(storage))
                                    .unwrap_or_else(|| "none".to_owned()),
                                member
                                    .symbol
                                    .map(|symbol| context.symbol(symbol))
                                    .unwrap_or_else(|| "none".to_owned()),
                                member.kind,
                                member.publication
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    format!(
                        "merge|owner={}|name={}|symbol={}|disposition={:?}|fragments={fragments}|members={members}",
                        context.declaration_owner(attachment.owner),
                        attachment.name,
                        context.symbol(attachment.symbol),
                        attachment.disposition
                    )
                })
        })
        .collect::<Vec<_>>();
    attachments.extend(
        binder
            .all_standalone_namespace_value_attachments()
            .into_iter()
            .map(|attachment| {
                let fragments = attachment
                    .fragments
                    .iter()
                    .map(|fragment| {
                        format!(
                            "{}:{}:{}",
                            context.namespace(fragment.namespace),
                            context.source(fragment.source),
                            fragment.source_start
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let members = attachment
                    .members
                    .iter()
                    .map(|member| {
                        format!(
                            "{}:{}:{:?}:decl={}:value={}:alias={}:symbol={}:child={}:spaces={:?}",
                            context.source(member.source),
                            member.declaration_span.start,
                            member.name,
                            member
                                .declaration
                                .map(|declaration| context.declaration(declaration))
                                .unwrap_or_else(|| "none".to_owned()),
                            member
                                .value_storage
                                .map(|storage| context.value_storage(storage))
                                .unwrap_or_else(|| "none".to_owned()),
                            member
                                .alias_target_storage
                                .map(|storage| context.value_storage(storage))
                                .unwrap_or_else(|| "none".to_owned()),
                            member
                                .symbol
                                .map(|symbol| context.symbol(symbol))
                                .unwrap_or_else(|| "none".to_owned()),
                            member
                                .child_namespace
                                .map(|namespace| context.namespace(namespace))
                                .unwrap_or_else(|| "none".to_owned()),
                            member.spaces
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "standalone|namespace={}|storage={}|fragments={fragments}|members={members}",
                    context.namespace(attachment.namespace),
                    context.value_storage(attachment.storage)
                )
            }),
    );
    attachments.sort();

    let mut augmentations = binder
        .namespaces
        .globals()
        .map(|global| {
            let members = global
                .members
                .iter()
                .filter_map(|member| binder.namespaces.member(*member))
                .map(|member| {
                    format!(
                        "{}:{}:{:?}:{:?}:{:?}:target={}:decl={}:symbol={}",
                        context.source(member.source),
                        member.binding_span.start,
                        member.name,
                        member.spaces,
                        member.kind,
                        context.declaration_owner(member.target),
                        member
                            .declaration
                            .map(|id| context.declaration(id))
                            .unwrap_or_else(|| "none".to_owned()),
                        member
                            .symbol
                            .map(|id| context.symbol(id))
                            .unwrap_or_else(|| "none".to_owned())
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{}|{}:{}|{:?}|declared={}|target={}|overlay={}|members={members}",
                context.source(global.source),
                global.body_span.start,
                global.body_span.end,
                global.placement,
                global.declared,
                context.scope(global.target_scope),
                context.scope(global.overlay_scope)
            )
        })
        .collect::<Vec<_>>();
    augmentations.sort();

    let mut global_contributors = binder
        .namespaces
        .merges()
        .filter(|merge| merge.owner == DeclarationOwner::CompilationGlobal)
        .filter(|merge| merge.classification.disposition == MergeDisposition::Admitted)
        .filter_map(|merge| {
            let ordinary = merge.declarations.iter().any(|declaration| {
                declaration.kind == MergeDeclarationKind::Function
                    || matches!(
                        declaration.syntax,
                        DeclarationSyntaxFacts::Variable(VariableKind::Var)
                    )
            });
            let namespace = merge
                .declarations
                .iter()
                .find_map(|declaration| declaration.namespace_fragment)
                .and_then(|fragment| binder.namespaces.fragment(fragment))
                .is_some_and(|fragment| {
                    binder
                        .namespaces
                        .aggregate_instance_state(fragment.namespace)
                        == Some(NamespaceInstanceState::Instantiated)
                });
            (ordinary || namespace).then(|| {
                let mut declarations = merge
                    .declarations
                    .iter()
                    .map(|declaration| {
                        format!(
                            "{}:{}:{:?}:{:?}",
                            context.source(declaration.source),
                            declaration.binding_span.start,
                            declaration.spaces,
                            declaration.kind
                        )
                    })
                    .collect::<Vec<_>>();
                declarations.sort();
                format!("{}|{}", merge.name, declarations.join(","))
            })
        })
        .collect::<Vec<_>>();
    global_contributors.sort();

    let mut modules = bound
        .project_sources
        .iter()
        .map(|row| {
            format!(
                "{}|kind={:?}|external={}",
                row.normalized_path, row.source_file_kind, row.external_module
            )
        })
        .collect::<Vec<_>>();
    modules.extend(normalized_import_export_rows(bound));
    modules.sort();

    DeclarationProjection {
        scopes,
        declarations,
        groups,
        namespaces,
        attachments,
        augmentations,
        global_contributors,
        modules,
    }
}

#[cfg(test)]
fn normalized_import_export_rows(bound: &crate::check::checker::BoundProjectBinder) -> Vec<String> {
    bound.normalized.normalized_import_export_shape.clone()
}

#[cfg(not(test))]
fn normalized_import_export_rows(bound: &crate::check::checker::BoundProjectBinder) -> Vec<String> {
    bound
        .project_sources
        .iter()
        .zip(&bound.module_placeholders)
        .map(|(source, placeholders)| {
            format!(
                "{}|placeholder_values={}",
                source.normalized_path,
                placeholders
                    .iter()
                    .filter(|placeholder| placeholder.value.is_some())
                    .count()
            )
        })
        .collect()
}

struct ProjectionContext<'a> {
    binder: &'a crate::binder::bind::Binder,
    source_names: BTreeMap<SourceUnitKey, String>,
    module_names: BTreeMap<u32, String>,
}

impl ProjectionContext<'_> {
    fn source(&self, source: SourceUnitKey) -> String {
        self.source_names
            .get(&source)
            .cloned()
            .unwrap_or_else(|| "prelude".to_owned())
    }

    fn scope(&self, scope: ScopeId) -> String {
        if scope == self.binder.prelude_module {
            return "scope:prelude".to_owned();
        }
        if scope == self.binder.compilation_global {
            return "scope:compilation-global".to_owned();
        }
        if scope == self.binder.script_namespace_root {
            return "scope:script-namespace-root".to_owned();
        }
        if let Some(module) = self.module_names.get(&scope.0) {
            return format!("scope:module:{module}");
        }
        if let Some(namespace) = self
            .binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.public_scope == scope)
        {
            return format!("scope:namespace-public:{}", self.namespace(namespace.id));
        }
        if let Some(fragment) = self
            .binder
            .namespaces
            .namespaces()
            .flat_map(|namespace| namespace.fragments.iter().copied())
            .filter_map(|fragment| self.binder.namespaces.fragment(fragment))
            .find(|fragment| fragment.private_scope == scope)
        {
            return format!(
                "scope:namespace-private:{}:{}:{}",
                self.namespace(fragment.namespace),
                self.source(fragment.source),
                fragment.source_start
            );
        }
        if let Some(global) = self
            .binder
            .namespaces
            .globals()
            .find(|global| global.overlay_scope == scope)
        {
            return format!(
                "scope:global-overlay:{}:{}",
                self.source(global.source),
                global.body_span.start
            );
        }
        if let Some(((module, start), _)) = self
            .binder
            .fn_scopes
            .iter()
            .find(|(_, candidate)| **candidate == scope)
        {
            return format!("scope:function:{}:{start}", self.scope(*module));
        }
        if let Some(((module, start), _)) = self
            .binder
            .block_scopes
            .iter()
            .find(|(_, candidate)| **candidate == scope)
        {
            return format!("scope:block:{}:{start}", self.scope(*module));
        }
        let kind = self
            .binder
            .graph
            .get(scope)
            .map(|row| format!("{:?}", row.kind))
            .unwrap_or_else(|| "missing".to_owned());
        format!("scope:unanchored:{kind}")
    }

    fn namespace(&self, namespace: NamespaceId) -> String {
        self.binder
            .namespaces
            .get(namespace)
            .map(|namespace| {
                format!(
                    "{}::{}",
                    self.namespace_owner(namespace.owner),
                    namespace.name
                )
            })
            .unwrap_or_else(|| "namespace:missing".to_owned())
    }

    fn namespace_owner(&self, owner: NamespaceOwner) -> String {
        match owner {
            NamespaceOwner::Lexical(scope) => self.scope(scope),
            NamespaceOwner::NamespacePublic(namespace) => self.namespace(namespace),
            NamespaceOwner::FragmentPrivate(fragment) => self
                .binder
                .namespaces
                .fragment(fragment)
                .map(|fragment| {
                    format!(
                        "{}:private:{}:{}",
                        self.namespace(fragment.namespace),
                        self.source(fragment.source),
                        fragment.source_start
                    )
                })
                .unwrap_or_else(|| "namespace-fragment:missing".to_owned()),
            NamespaceOwner::CompilationGlobal => "scope:compilation-global".to_owned(),
        }
    }

    fn declaration_owner(&self, owner: DeclarationOwner) -> String {
        match owner {
            DeclarationOwner::Lexical(scope) => self.scope(scope),
            DeclarationOwner::NamespacePublic(namespace) => self.namespace(namespace),
            DeclarationOwner::NamespacePrivate(fragment) => self
                .binder
                .namespaces
                .fragment(fragment)
                .map(|fragment| {
                    format!(
                        "{}:private:{}:{}",
                        self.namespace(fragment.namespace),
                        self.source(fragment.source),
                        fragment.source_start
                    )
                })
                .unwrap_or_else(|| "namespace-fragment:missing".to_owned()),
            DeclarationOwner::CompilationGlobal => "scope:compilation-global".to_owned(),
            DeclarationOwner::DeferredAmbientModule(module) => self
                .binder
                .namespaces
                .deferred_module(module)
                .map(|module| {
                    format!(
                        "ambient:{}:{}:{}",
                        self.source(module.source),
                        module.span.start,
                        module.specifier
                    )
                })
                .unwrap_or_else(|| "ambient:missing".to_owned()),
        }
    }

    fn declaration(&self, id: crate::binder::declaration::DeclId) -> String {
        self.binder
            .declarations
            .get(id)
            .map(|declaration| {
                let source = self
                    .binder
                    .source_for_module(declaration.site.module)
                    .map(|source| self.source(source))
                    .unwrap_or_else(|| "prelude".to_owned());
                format!(
                    "{source}:{}:{}:{:?}",
                    declaration.site.binding_span.start,
                    declaration.site.binding_span.end,
                    declaration.kind
                )
            })
            .unwrap_or_else(|| "declaration:missing".to_owned())
    }

    fn value_storage(&self, storage: crate::binder::declaration::ValueStorageId) -> String {
        if let Some(declaration) = self
            .binder
            .declarations
            .iter()
            .find(|declaration| declaration.value_storage == Some(storage))
        {
            return format!("declaration:{}", self.declaration(declaration.id));
        }
        self.binder
            .namespaces
            .namespaces()
            .find(|namespace| {
                self.binder
                    .namespaces
                    .standalone_value_storage(namespace.id)
                    == Some(storage)
            })
            .map(|namespace| format!("namespace:{}", self.namespace(namespace.id)))
            .unwrap_or_else(|| "value-storage:missing".to_owned())
    }

    fn type_group(&self, id: crate::binder::declaration::TypeGroupId) -> String {
        self.binder
            .type_groups
            .get(id)
            .map(|group| {
                let fragments = group
                    .fragments
                    .iter()
                    .map(|fragment| {
                        format!(
                            "{}:{}:{:?}:{}",
                            self.source(fragment.source),
                            fragment.site.binding_span.start,
                            fragment.kind,
                            self.scope(fragment.scope)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{}[{fragments}]", group.name)
            })
            .unwrap_or_else(|| "type-group:missing".to_owned())
    }

    fn symbol(&self, id: crate::binder::symbol::SymbolId) -> String {
        self.binder
            .symbols
            .get(id)
            .map(|symbol| {
                format!(
                    "{}|value={}|type={}|namespace={}",
                    symbol.name,
                    symbol
                        .value
                        .map(|storage| self.value_storage(storage))
                        .unwrap_or_else(|| "none".to_owned()),
                    symbol
                        .ty
                        .map(|group| self.type_group(group))
                        .unwrap_or_else(|| "none".to_owned()),
                    symbol
                        .ns
                        .map(|namespace| self.namespace(namespace))
                        .unwrap_or_else(|| "none".to_owned())
                )
            })
            .unwrap_or_else(|| "symbol:missing".to_owned())
    }
}
