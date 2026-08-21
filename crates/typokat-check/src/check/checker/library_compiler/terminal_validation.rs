use super::*;

pub(super) struct ReplayTerminalValidationInputs<'a> {
    pub(super) binder: &'a Binder,
    pub(super) published: &'a PublishedTypeEnvironment,
    pub(super) decl_types: &'a DeclTypes,
    pub(super) namespace_terminals: &'a [FrozenNamespaceValueTerminalSnapshotRow],
    pub(super) runtime: &'a FrozenCheckerRuntimeSnapshotParts,
    pub(super) semantic_identities: Option<&'a LibrarySemanticIdentities>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TerminalClassDependencyValidationWork {
    pub(super) owner_root_summaries: u64,
    pub(super) semantic_node_visits: u64,
    pub(super) semantic_edge_probes: u64,
    pub(super) class_edge_probes: u64,
    pub(super) class_summary_items: u64,
    pub(super) owner_expression_visits: u64,
}

fn intern_terminal_semantic_node(
    nodes: &mut rustc_hash::FxHashMap<(u8, u32), usize>,
    semantic_edges: &mut Vec<Vec<usize>>,
    reverse_edges: &mut Vec<Vec<usize>>,
    class_edges: &mut Vec<Vec<ClassId>>,
    key: (u8, u32),
) -> usize {
    if let Some(node) = nodes.get(&key) {
        return *node;
    }
    let node = semantic_edges.len();
    nodes.insert(key, node);
    semantic_edges.push(Vec::new());
    reverse_edges.push(Vec::new());
    class_edges.push(Vec::new());
    node
}

enum TerminalOwnerExpression {
    Owner(ReplayOwner),
    Union(Vec<usize>),
}

/// Flattens every owner expression in one ascending pass. A `Union` is always
/// minted after the inputs it merges, so the vector is already topologically
/// ordered and each expression is summarized exactly once — memoizing only at
/// query roots re-walked shared pass-through chains once per root.
fn terminal_expression_owners(
    expressions: &[TerminalOwnerExpression],
    #[cfg(any(test, feature = "test-utils"))] work: &mut Option<
        &mut TerminalClassDependencyValidationWork,
    >,
) -> Vec<Vec<ReplayOwner>> {
    let mut owners = Vec::<Vec<ReplayOwner>>::with_capacity(expressions.len());
    for (expression, node) in expressions.iter().enumerate() {
        #[cfg(any(test, feature = "test-utils"))]
        record_terminal_owner_expression_visits(work, 1);
        match node {
            TerminalOwnerExpression::Owner(owner) => owners.push(vec![*owner]),
            TerminalOwnerExpression::Union(inputs) => {
                let mut merged = Vec::new();
                for input in inputs.iter().copied() {
                    // Ascending order is the precondition of this pass; a forward
                    // reference would silently truncate the owner closure.
                    assert!(
                        input < expression,
                        "owner expression {expression} merges a later input {input}"
                    );
                    #[cfg(any(test, feature = "test-utils"))]
                    record_terminal_owner_expression_visits(work, owners[input].len());
                    merged.extend_from_slice(&owners[input]);
                }
                merged.sort_unstable();
                merged.dedup();
                owners.push(merged);
            }
        }
    }
    owners
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_semantic_nodes(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.semantic_node_visits = work
            .semantic_node_visits
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_semantic_edges(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.semantic_edge_probes = work
            .semantic_edge_probes
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_class_edges(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.class_edge_probes = work
            .class_edge_probes
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_class_summary_items(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.class_summary_items = work
            .class_summary_items
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_owner_expression_visits(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.owner_expression_visits = work
            .owner_expression_visits
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

pub(super) fn require_terminal_class_dependency_closure(
    trace: &ReplayDependencyTrace,
    sparse_semantic_edges: &BTreeMap<(u8, u32), Vec<(u8, u32)>>,
    sparse_class_edges: &BTreeMap<(u8, u32), Vec<ClassId>>,
    direct: BTreeMap<ReplayOwner, Vec<TypeId>>,
    #[cfg(any(test, feature = "test-utils"))] mut work: Option<
        &mut TerminalClassDependencyValidationWork,
    >,
) {
    const TYPE_DOMAIN: u8 = 1;

    let mut nodes = rustc_hash::FxHashMap::default();
    let mut semantic_edges = Vec::<Vec<usize>>::new();
    let mut reverse_edges = Vec::<Vec<usize>>::new();
    let mut class_edges = Vec::<Vec<ClassId>>::new();
    for (source_key, targets) in sparse_semantic_edges {
        let source = intern_terminal_semantic_node(
            &mut nodes,
            &mut semantic_edges,
            &mut reverse_edges,
            &mut class_edges,
            *source_key,
        );
        for target_key in targets {
            let target = intern_terminal_semantic_node(
                &mut nodes,
                &mut semantic_edges,
                &mut reverse_edges,
                &mut class_edges,
                *target_key,
            );
            semantic_edges[source].push(target);
            reverse_edges[target].push(source);
        }
    }
    for (source_key, classes) in sparse_class_edges {
        let source = intern_terminal_semantic_node(
            &mut nodes,
            &mut semantic_edges,
            &mut reverse_edges,
            &mut class_edges,
            *source_key,
        );
        #[cfg(any(test, feature = "test-utils"))]
        {
            record_terminal_class_edges(&mut work, classes.len());
            record_terminal_class_summary_items(&mut work, classes.len());
        }
        class_edges[source].extend(classes.iter().copied());
    }
    for roots in direct.values() {
        for root in roots {
            intern_terminal_semantic_node(
                &mut nodes,
                &mut semantic_edges,
                &mut reverse_edges,
                &mut class_edges,
                (TYPE_DOMAIN, root.0),
            );
        }
    }
    for classes in &mut class_edges {
        classes.sort_unstable();
        classes.dedup();
    }

    // Condense cycles before propagating terminal classes across shared tails.
    let node_count = semantic_edges.len();
    let mut finish_order = Vec::with_capacity(node_count);
    let mut seen = vec![false; node_count];
    for start in 0..node_count {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        #[cfg(any(test, feature = "test-utils"))]
        record_terminal_semantic_nodes(&mut work, 1);
        let mut pending = vec![(start, 0_usize)];
        while let Some((node, next_edge)) = pending.last_mut() {
            if *next_edge == semantic_edges[*node].len() {
                finish_order.push(*node);
                pending.pop();
                continue;
            }
            let target = semantic_edges[*node][*next_edge];
            *next_edge += 1;
            #[cfg(any(test, feature = "test-utils"))]
            record_terminal_semantic_edges(&mut work, 1);
            if !seen[target] {
                seen[target] = true;
                #[cfg(any(test, feature = "test-utils"))]
                record_terminal_semantic_nodes(&mut work, 1);
                pending.push((target, 0));
            }
        }
    }

    let mut component_of = vec![usize::MAX; node_count];
    let mut component_count = 0;
    while let Some(start) = finish_order.pop() {
        if component_of[start] != usize::MAX {
            continue;
        }
        component_of[start] = component_count;
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            #[cfg(any(test, feature = "test-utils"))]
            record_terminal_semantic_nodes(&mut work, 1);
            for source in &reverse_edges[node] {
                #[cfg(any(test, feature = "test-utils"))]
                record_terminal_semantic_edges(&mut work, 1);
                if component_of[*source] == usize::MAX {
                    component_of[*source] = component_count;
                    pending.push(*source);
                }
            }
        }
        component_count += 1;
    }

    let mut component_dependencies = vec![Vec::<usize>::new(); component_count];
    let mut component_dependents = vec![Vec::<usize>::new(); component_count];
    let mut component_classes = vec![Vec::<ClassId>::new(); component_count];
    for node in 0..node_count {
        #[cfg(any(test, feature = "test-utils"))]
        record_terminal_semantic_nodes(&mut work, 1);
        let component = component_of[node];
        #[cfg(any(test, feature = "test-utils"))]
        {
            record_terminal_class_edges(&mut work, class_edges[node].len());
            record_terminal_class_summary_items(&mut work, class_edges[node].len());
        }
        component_classes[component].extend_from_slice(&class_edges[node]);
        for target in &semantic_edges[node] {
            #[cfg(any(test, feature = "test-utils"))]
            record_terminal_semantic_edges(&mut work, 1);
            let dependency = component_of[*target];
            if component != dependency {
                component_dependencies[component].push(dependency);
            }
        }
    }
    for classes in &mut component_classes {
        classes.sort_unstable();
        classes.dedup();
    }
    for dependencies in &mut component_dependencies {
        dependencies.sort_unstable();
        dependencies.dedup();
    }
    for (component, dependencies) in component_dependencies.iter().enumerate() {
        for dependency in dependencies {
            component_dependents[*dependency].push(component);
        }
    }

    let mut expressions = direct
        .keys()
        .copied()
        .map(TerminalOwnerExpression::Owner)
        .collect::<Vec<_>>();
    let owner_expressions = direct
        .keys()
        .copied()
        .enumerate()
        .map(|(expression, owner)| (owner, expression))
        .collect::<BTreeMap<_, _>>();
    let mut component_owner_inputs = vec![Vec::<usize>::new(); component_count];
    for (owner, roots) in &direct {
        #[cfg(any(test, feature = "test-utils"))]
        if let Some(work) = work.as_deref_mut() {
            work.owner_root_summaries = work
                .owner_root_summaries
                .saturating_add(u64::try_from(roots.len()).unwrap_or(u64::MAX));
        }
        let expression = owner_expressions[owner];
        for root in roots {
            let node = nodes[&(TYPE_DOMAIN, root.0)];
            component_owner_inputs[component_of[node]].push(expression);
        }
    }

    // Owner expressions stay persistent across pass-through chains. They are
    // flattened only where terminal classes make the owner × class output real.
    let mut unresolved_dependents = component_dependents
        .iter()
        .map(Vec::len)
        .collect::<Vec<_>>();
    let mut ready = unresolved_dependents
        .iter()
        .enumerate()
        .filter_map(|(component, count)| (*count == 0).then_some(component))
        .collect::<Vec<_>>();
    let mut processed_components = 0;
    let mut class_owner_inputs = BTreeMap::<ClassId, Vec<usize>>::new();
    while let Some(component) = ready.pop() {
        processed_components += 1;
        #[cfg(any(test, feature = "test-utils"))]
        record_terminal_semantic_nodes(&mut work, 1);

        component_owner_inputs[component].sort_unstable();
        component_owner_inputs[component].dedup();
        let owner_expression = match component_owner_inputs[component].as_slice() {
            [] => None,
            [expression] => Some(*expression),
            inputs => {
                let expression = expressions.len();
                expressions.push(TerminalOwnerExpression::Union(inputs.to_vec()));
                Some(expression)
            }
        };

        if let Some(expression) = owner_expression {
            #[cfg(any(test, feature = "test-utils"))]
            {
                record_terminal_class_edges(&mut work, component_classes[component].len());
                record_terminal_class_summary_items(&mut work, component_classes[component].len());
            }
            for class in &component_classes[component] {
                class_owner_inputs
                    .entry(*class)
                    .or_default()
                    .push(expression);
            }
            for dependency in component_dependencies[component].iter().copied() {
                component_owner_inputs[dependency].push(expression);
            }
        }

        for dependency in component_dependencies[component].iter().copied() {
            #[cfg(any(test, feature = "test-utils"))]
            record_terminal_semantic_edges(&mut work, 1);
            unresolved_dependents[dependency] -= 1;
            if unresolved_dependents[dependency] == 0 {
                ready.push(dependency);
            }
        }
    }
    // An undrained condensation would emit no classes for the stranded components,
    // i.e. silently admit unauthenticated replay dependencies.
    assert_eq!(
        processed_components, component_count,
        "the terminal condensation is acyclic and must drain completely"
    );

    let mut class_expressions = Vec::with_capacity(class_owner_inputs.len());
    for (class, mut inputs) in class_owner_inputs {
        inputs.sort_unstable();
        inputs.dedup();
        let expression = match inputs.as_slice() {
            [expression] => *expression,
            inputs => {
                let expression = expressions.len();
                expressions.push(TerminalOwnerExpression::Union(inputs.to_vec()));
                expression
            }
        };
        class_expressions.push((class, expression));
    }
    let expression_owners = terminal_expression_owners(
        &expressions,
        #[cfg(any(test, feature = "test-utils"))]
        &mut work,
    );
    for (class, expression) in class_expressions {
        let owners = &expression_owners[expression];
        #[cfg(any(test, feature = "test-utils"))]
        {
            record_terminal_class_edges(&mut work, owners.len());
            record_terminal_class_summary_items(&mut work, owners.len());
        }
        for owner in owners {
            trace.require_dependency(*owner, ReplayOwner::Class(class));
        }
    }
}

/// Validate the terminal runtime state the base publishes, and — when a replay trace is present
/// — collect the terminal dependency closure a collision replay index needs.
///
/// The validating half is not about the index. It checks the published class registry is
/// terminal, that every class name and named-function symbol the frozen runtime carries has an
/// owner, and that the library's semantic identities survive an exact recomputation — all of it
/// state the base itself publishes. That half runs on every compile. Only the dependency-edge
/// collection is conditional on assembling an index, so the two paths cannot drift apart.
pub(super) fn validate_terminal_class_dependencies(
    trace: Option<&ReplayDependencyTrace>,
    interner: &Interner,
    inputs: ReplayTerminalValidationInputs<'_>,
) -> Result<(), InjectedProfileError> {
    let require_dependency = |consumer: ReplayOwner, dependency: ReplayOwner| {
        if let Some(trace) = trace {
            trace.require_dependency(consumer, dependency);
        }
    };
    let ReplayTerminalValidationInputs {
        binder,
        published,
        decl_types,
        namespace_terminals,
        runtime,
        semantic_identities,
    } = inputs;
    record_collision_plan_forbidden_work(|work| {
        let projected = binder
            .type_groups
            .len()
            .saturating_add(decl_types.len())
            .saturating_add(namespace_terminals.len())
            .saturating_add(runtime.class_application_parameters.len())
            .saturating_add(runtime.class_new_metadata.len())
            .saturating_add(runtime.class_parents.len());
        let projected = u64::try_from(projected).unwrap_or(u64::MAX);
        work.transitive_terminal_owner_entries = work
            .transitive_terminal_owner_entries
            .saturating_add(projected);
        work.full_semantic_projection_rows =
            work.full_semantic_projection_rows.saturating_add(projected);
        work.eager_all_owner_scc_memberships = work
            .eager_all_owner_scc_memberships
            .saturating_add(u64::try_from(interner.store().len()).unwrap_or(u64::MAX));
    });
    const TYPE_DOMAIN: u8 = 1;
    const CLASS_DOMAIN: u8 = 3;
    const DECLARED_RECIPE_DOMAIN: u8 = 10;

    let references = interner
        .typed_reference_records_for_replay_generation()
        .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?;
    let mut semantic_edges: BTreeMap<(u8, u32), Vec<(u8, u32)>> = BTreeMap::new();
    let mut class_edges: BTreeMap<(u8, u32), Vec<ClassId>> = BTreeMap::new();
    for (owner_domain, target_domain, _, owner, target) in references {
        if !matches!(owner_domain, TYPE_DOMAIN | DECLARED_RECIPE_DOMAIN) {
            continue;
        }
        match target_domain {
            TYPE_DOMAIN | DECLARED_RECIPE_DOMAIN => semantic_edges
                .entry((owner_domain, owner))
                .or_default()
                .push((target_domain, target)),
            CLASS_DOMAIN => class_edges
                .entry((owner_domain, owner))
                .or_default()
                .push(ClassId(target)),
            _ => {}
        }
    }

    let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();
    for index in 0..published.groups().len() {
        let group =
            TypeGroupId(u32::try_from(index).map_err(|_| InjectedProfileError::SourceKeyOverflow)?);
        let Some(PublishedTypeGroupTerminal::Ready(ready)) = published.groups().get(group) else {
            continue;
        };
        let owner = ReplayOwner::TypeGroup(group);
        match ready.surface {
            PublishedTypeGroupSurface::Template(ty) | PublishedTypeGroupSurface::Enum(ty) => {
                direct.entry(owner).or_default().push(ty)
            }
            PublishedTypeGroupSurface::Class(class) => {
                require_dependency(owner, ReplayOwner::Class(class));
            }
        }
        direct
            .entry(owner)
            .or_default()
            .extend(
                ready
                    .parameter_defaults
                    .iter()
                    .filter_map(|default| match default {
                        PublishedTypeParameterDefault::Ready(ty) => Some(*ty),
                        PublishedTypeParameterDefault::Absent
                        | PublishedTypeParameterDefault::Unsupported => None,
                    }),
            );
        direct.entry(owner).or_default().extend(
            ready
                .conflict_alternatives
                .iter()
                .flat_map(|alternative| alternative.types.iter().copied()),
        );
    }
    for (index, ty) in decl_types.snapshot_slots().into_iter().enumerate() {
        let Some(ty) = ty else { continue };
        let storage = ValueStorageId(
            u32::try_from(index).map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
        );
        direct
            .entry(ReplayOwner::Value(storage))
            .or_default()
            .push(ty);
    }
    for row in namespace_terminals {
        let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal else {
            continue;
        };
        let owner = ReplayOwner::Namespace(row.namespace);
        require_dependency(owner, ReplayOwner::Value(storage));
        direct.entry(owner).or_default().push(ty);
    }
    let terminals = published.classes().canonical_terminals().ok_or_else(|| {
        InjectedProfileError::CanonicalProjection(
            "replay generation saw a non-terminal class registry".to_owned(),
        )
    })?;
    record_collision_plan_forbidden_work(|work| {
        work.canonical_terminal_rows = work
            .canonical_terminal_rows
            .saturating_add(u64::try_from(terminals.len()).unwrap_or(u64::MAX));
    });
    for (class, terminal) in &terminals {
        let CanonicalPublishedClassTerminal::Ready(surface) = terminal else {
            continue;
        };
        let types = direct.entry(ReplayOwner::Class(*class)).or_default();
        types.push(surface.instance_template());
        types.push(surface.static_template());
        types.extend(surface.constructor_template());
    }

    for (class, parameters) in &runtime.class_application_parameters {
        let types = direct.entry(ReplayOwner::Class(*class)).or_default();
        for parameter in parameters {
            types.extend(parameter.constraint);
            if let ClassTypeParameterDefault::Ready(default) = parameter.default {
                types.push(default);
            }
        }
    }
    for (class, metadata) in &runtime.class_new_metadata {
        require_dependency(
            ReplayOwner::Class(*class),
            ReplayOwner::Class(metadata.ctor_declaring_class),
        );
    }
    for (class, parent) in &runtime.class_parents {
        require_dependency(ReplayOwner::Class(*class), ReplayOwner::Class(*parent));
    }
    for (alias, target) in &runtime.class_value_aliases {
        require_dependency(ReplayOwner::Value(*alias), ReplayOwner::Value(*target));
    }
    for (storage, binding) in &runtime.class_value_bindings {
        require_dependency(
            ReplayOwner::Value(*storage),
            ReplayOwner::Class(binding.class_id),
        );
    }
    for (alias, target) in &runtime.standalone_namespace_value_aliases {
        require_dependency(ReplayOwner::Value(*alias), ReplayOwner::Value(*target));
    }
    let class_ids = terminals
        .iter()
        .map(|(class, _)| *class)
        .collect::<BTreeSet<_>>();
    if runtime
        .class_names
        .iter()
        .any(|(class, _)| !class_ids.contains(class))
    {
        return Err(InjectedProfileError::CanonicalProjection(
            "replay runtime class name has no published class owner".to_owned(),
        ));
    }
    for symbol in &runtime.named_function_symbols {
        let binding = binder.symbols.get(*symbol).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "replay runtime named function symbol is missing".to_owned(),
            )
        })?;
        let Some(canonical) = binding.function_values.first().copied() else {
            return Err(InjectedProfileError::CanonicalProjection(
                "replay runtime named function has no value owner".to_owned(),
            ));
        };
        for participant in &binding.function_values {
            require_dependency(
                ReplayOwner::Value(canonical),
                ReplayOwner::Value(*participant),
            );
        }
    }
    if let Some(identities) = semantic_identities {
        let recomputed = LibrarySemanticIdentities::select(binder, published, interner.store());
        if &recomputed != identities {
            return Err(InjectedProfileError::CanonicalProjection(
                "replay semantic identities differ from exact recomputation".to_owned(),
            ));
        }
        for terminal in identities.terminals() {
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                return Err(InjectedProfileError::CanonicalProjection(
                    "replay semantic identity is unavailable".to_owned(),
                ));
            };
            direct
                .entry(ReplayOwner::TypeGroup(identity.group))
                .or_default()
                .push(identity.template);
        }
    }

    if let Some(trace) = trace {
        require_terminal_class_dependency_closure(
            trace,
            &semantic_edges,
            &class_edges,
            direct,
            #[cfg(any(test, feature = "test-utils"))]
            None,
        );
    }
    Ok(())
}

pub(super) fn validate_root_census(
    candidates: &BTreeMap<String, crate::binder::declaration::SourceGlobalBindingCandidate>,
    rows: &[RootNameRow],
    explicit_global_this: bool,
) -> Result<(), ReplayIndexGenerationError> {
    let mut by_name = BTreeMap::new();
    for row in rows {
        if by_name.insert(row.name.as_str(), row).is_some() {
            return Err(ReplayIndexGenerationError::DuplicateRootName(
                row.name.clone(),
            ));
        }
    }
    for (name, candidate) in candidates {
        let Some(row) = by_name.get(name.as_str()).copied() else {
            return Err(ReplayIndexGenerationError::InvalidRootSlot(name.clone()));
        };
        for (slot, present) in [
            (SourceBindingSlot::Value, row.value.is_some()),
            (SourceBindingSlot::Type, row.ty.is_some()),
            (SourceBindingSlot::Namespace, row.namespace.is_some()),
        ] {
            if candidate.slots.contains(&slot) != present {
                return Err(ReplayIndexGenerationError::InvalidRootSlot(name.clone()));
            }
        }
    }
    for row in rows {
        if !candidates.contains_key(&row.name) {
            return Err(ReplayIndexGenerationError::InvalidRootSlot(
                row.name.clone(),
            ));
        }
    }
    if explicit_global_this && !by_name.contains_key("globalThis") {
        return Err(ReplayIndexGenerationError::InvalidRootSlot(
            "globalThis".to_owned(),
        ));
    }
    Ok(())
}
