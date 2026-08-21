use super::*;

impl OwnedLibraryRuntimeState {
    pub fn complete_replay_binder_checkpoint(
        &self,
    ) -> Result<LibraryBinderCheckpoint, &'static str> {
        let binder = self.binder.fork_sparse_collision_delta()?;
        let library_units = self
            .library_modules
            .iter()
            .copied()
            .enumerate()
            .map(|(ordinal, module)| {
                let source = binder
                    .source_for_module(module)
                    .ok_or("complete replay module has no retained source key")?;
                Ok(LibraryBinderUnit {
                    ordinal: LibraryFileOrdinal::new(ordinal),
                    source,
                    module,
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        Ok(build_library_binder_checkpoint(binder, library_units))
    }

    #[cfg(test)]
    fn collect_library_modules(
        binder: &Binder,
        source_file_count: u32,
    ) -> Result<std::sync::Arc<[ScopeId]>, &'static str> {
        let count =
            usize::try_from(source_file_count).map_err(|_| "library source count overflows")?;
        let mut modules = vec![None; count];
        for (scope, source) in binder.module_sources().iter() {
            let Some(index) = source
                .0
                .checked_sub(1)
                .and_then(|source| usize::try_from(source).ok())
            else {
                continue;
            };
            if let Some(module) = modules.get_mut(index) {
                *module = Some(*scope);
            }
        }
        modules
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .map(Into::into)
            .ok_or("library runtime is missing a source module")
    }

    pub(in crate::check::checker) fn into_user_project_base(
        self,
    ) -> (Interner, Binder, super::BoundUserBase) {
        let Self {
            interner,
            binder,
            published_types,
            decl_types,
            semantic_identities,
            runtime,
            next_type_param,
            next_class_id,
            source_file_count: _,
            library_modules,
            replay_index: _,
            private_collision_epoch,
        } = self;
        (
            interner,
            binder,
            super::BoundUserBase {
                published_types,
                library_semantic_identities: semantic_identities,
                lexical_array_alias: None,
                decl_types,
                next_type_param,
                next_class_id,
                runtime,
                private_collision_epoch,
                library_modules,
            },
        )
    }

    pub fn freeze_as_library_base(&mut self) -> Result<(), &'static str> {
        self.interner.freeze_as_base()?;
        self.binder.freeze_as_base()?;
        self.published_types.freeze_as_base()?;
        self.decl_types.freeze_as_base()?;
        self.runtime.freeze_as_base()?;
        #[cfg(any(test, feature = "test-utils"))]
        record_complete_source_route_work_for_test(|work| {
            work.frozen_base_seals = work.frozen_base_seals.saturating_add(1);
        });
        Ok(())
    }

    pub fn fork_collision_free_user_delta(
        &self,
        _capability: CollisionFreeUserDeltaCapability,
    ) -> Result<Self, &'static str> {
        self.fork_user_delta()
    }

    fn fork_user_delta(&self) -> Result<Self, &'static str> {
        #[cfg(any(test, feature = "test-utils"))]
        USER_DELTA_FORKS.set(USER_DELTA_FORKS.get().saturating_add(1));
        Ok(Self {
            interner: self.interner.fork_delta()?,
            binder: self.binder.fork_delta()?,
            published_types: self.published_types.fork_delta()?,
            decl_types: self.decl_types.fork_delta()?,
            semantic_identities: self.semantic_identities.clone(),
            runtime: self.runtime.fork_delta()?,
            next_type_param: self.next_type_param,
            next_class_id: self.next_class_id,
            source_file_count: self.source_file_count,
            library_modules: std::sync::Arc::clone(&self.library_modules),
            replay_index: None,
            private_collision_epoch: None,
        })
    }

    pub fn fork_sparse_collision_epoch(&self) -> Result<Self, &'static str> {
        Ok(Self {
            interner: self.interner.fork_delta()?,
            binder: self.binder.fork_sparse_collision_delta()?,
            published_types: self.published_types.fork_sparse_delta()?,
            decl_types: self.decl_types.fork_sparse_delta()?,
            semantic_identities: self.semantic_identities.clone(),
            runtime: self.runtime.fork_sparse_delta()?,
            next_type_param: self.next_type_param,
            next_class_id: self.next_class_id,
            source_file_count: self.source_file_count,
            library_modules: std::sync::Arc::clone(&self.library_modules),
            replay_index: None,
            private_collision_epoch: None,
        })
    }

    pub fn install_private_collision_replay(
        self,
        plan: std::sync::Arc<CollisionReplayPlan>,
        seeds: Vec<PrivateCollisionReplaySeed>,
    ) -> Result<Self, &'static str> {
        let permit = acquire_private_collision_replay_permit()?;
        self.install_private_collision_replay_with_permit(plan, seeds, permit)
    }

    pub fn install_private_collision_replay_with_permit(
        mut self,
        plan: std::sync::Arc<CollisionReplayPlan>,
        seeds: Vec<PrivateCollisionReplaySeed>,
        permit: PrivateCollisionReplayPermitToken,
    ) -> Result<Self, &'static str> {
        if self.private_collision_epoch.is_some() || seeds.is_empty() {
            return Err("private collision replay installs exactly once with nonempty seeds");
        }
        let mut affected_owners = BTreeSet::new();
        let mut value_winners = BTreeMap::new();
        for seed in &seeds {
            record_private_replay_scheduler_work_for_test("seed-pops", 1);
            if seed.global_object {
                record_private_replay_scheduler_work_for_test("owner-set-probes", 1);
                if affected_owners.insert(ReplayOwner::GlobalObject) {
                    record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                    record_private_replay_scheduler_work_for_test("seed-pushes", 1);
                }
            }
            let Some(root) = plan.root_slot(&seed.name) else {
                continue;
            };
            if seed.value {
                if let Some(value) = root.value {
                    value_winners.insert(root.name.clone(), value);
                }
            }
            // A same-name global declaration can affect merge disposition across spaces even
            // when its own syntax contributes only one slot. Seed every retained root meaning;
            // the compact reverse closure removes no-op consumers later.
            for owner in [
                root.value.map(ReplayOwner::Value),
                root.ty.map(ReplayOwner::TypeGroup),
                root.namespace.map(ReplayOwner::Namespace),
            ]
            .into_iter()
            .flatten()
            {
                record_private_replay_scheduler_work_for_test("owner-set-probes", 1);
                if affected_owners.insert(owner) {
                    record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                    record_private_replay_scheduler_work_for_test("seed-pushes", 1);
                }
            }
            // A user reopening a global namespace can mutate rows owned by that namespace
            // without naming the nested declaration in preflight. Seed every exact library owner
            // declared directly in the retained namespace; post-bind mutation validation below
            // still proves that the conservative expansion covered the actual changed rows.
            if seed.namespace {
                if let Some(namespace) = root.namespace {
                    for owner in plan.namespace_direct_owners(namespace) {
                        if affected_owners.insert(owner) {
                            record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                            record_private_replay_scheduler_work_for_test("seed-pushes", 1);
                        }
                    }
                }
            }
            for consumer in plan.root_consumers(&seed.name) {
                record_private_replay_scheduler_work_for_test("owner-set-probes", 1);
                if affected_owners.insert(consumer.consumer) {
                    record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                    record_private_replay_scheduler_work_for_test("seed-pushes", 1);
                }
            }
        }
        let mut pending = affected_owners.iter().copied().collect::<Vec<_>>();
        record_private_replay_scheduler_work_for_test("replay-allocations", pending.len());
        record_private_replay_scheduler_work_for_test("scc-queue-pushes", pending.len());
        let mut cursor = 0;
        while let Some(owner) = pending.get(cursor).copied() {
            cursor += 1;
            record_private_replay_scheduler_work_for_test("scc-queue-pops", 1);
            for edge in plan.reverse_consumers(owner) {
                record_private_replay_scheduler_work_for_test("edge-probes", 1);
                record_private_replay_scheduler_work_for_test("owner-set-probes", 1);
                if affected_owners.insert(edge.consumer) {
                    record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                    pending.push(edge.consumer);
                    record_private_replay_scheduler_work_for_test("scc-queue-pushes", 1);
                }
            }
        }
        let mut owner_sites = Vec::new();
        for owner in &affected_owners {
            owner_sites.extend_from_slice(plan.owner_sites_for(*owner));
        }
        record_private_replay_scheduler_work_for_test("replay-allocations", 1);
        let library_record_baselines =
            PrivateCollisionEpoch::baselines_for(&plan, &affected_owners);
        self.private_collision_epoch = Some(PrivateCollisionEpoch {
            affected_owners,
            mutation_owners: BTreeSet::new(),
            value_winners,
            owner_sites,
            library_record_baselines,
            sources: Vec::new(),
            complete_source_replay: false,
            plan,
            _permit: permit,
            #[cfg(any(test, feature = "test-utils"))]
            state_drop_witness: Some(PrivateCollisionStateDropWitnessForTest),
        });
        #[cfg(any(test, feature = "test-utils"))]
        record_private_replay_scale_hook_for_test(|trace, event| {
            trace.sparse_replay_invocations = trace.sparse_replay_invocations.saturating_add(1);
            trace.private_work_started = event;
        });
        Ok(self)
    }

    pub(super) fn install_complete_source_replay_plan_with_permit(
        mut self,
        plan: std::sync::Arc<CollisionReplayPlan>,
        seeds: &[PrivateCollisionReplaySeed],
        permit: PrivateCollisionReplayPermitToken,
    ) -> Result<Self, &'static str> {
        if self.private_collision_epoch.is_some() {
            return Err("complete source replay installs exactly once");
        }
        let materialized = plan.complete_source_materialization();
        // A complete continuation can replace an actually colliding root with its user suffix.
        let mut affected_owners = materialized.owners;
        for seed in seeds {
            if seed.global_object {
                affected_owners.insert(ReplayOwner::GlobalObject);
            }
            if !seed.value {
                continue;
            }
            let Some(root) = plan.root_slot(&seed.name) else {
                continue;
            };
            affected_owners.extend(root.value.map(ReplayOwner::Value));
        }
        let value_winners = seeds
            .iter()
            .filter(|seed| seed.value)
            .filter_map(|seed| {
                plan.root_slot(&seed.name)
                    .and_then(|root| root.value)
                    .map(|value| (seed.name.clone(), value))
            })
            .collect();
        self.private_collision_epoch = Some(PrivateCollisionEpoch {
            affected_owners,
            mutation_owners: BTreeSet::new(),
            value_winners,
            owner_sites: materialized.owner_sites,
            library_record_baselines: materialized.baseline_records,
            sources: Vec::new(),
            complete_source_replay: true,
            plan,
            _permit: permit,
            #[cfg(any(test, feature = "test-utils"))]
            state_drop_witness: Some(PrivateCollisionStateDropWitnessForTest),
        });
        Ok(self)
    }

    pub fn private_collision_source_ordinals(
        &self,
    ) -> Result<Vec<LibraryFileOrdinal>, &'static str> {
        let epoch = self
            .private_collision_epoch
            .as_ref()
            .ok_or("private collision replay is not installed")?;
        if epoch.complete_source_replay {
            return Ok((0..self.source_file_count)
                .map(|ordinal| {
                    LibraryFileOrdinal::new(usize::try_from(ordinal).unwrap_or(usize::MAX))
                })
                .collect());
        }
        let mut ordinals = epoch
            .owner_sites
            .iter()
            .filter(|site| !matches!(site.owner, ReplayOwner::GlobalObject))
            .map(|site| site.file_ordinal)
            .collect::<Vec<_>>();
        if ordinals.is_empty() {
            if let Some(site) = epoch
                .owner_sites
                .iter()
                .find(|site| matches!(site.owner, ReplayOwner::GlobalObject))
            {
                ordinals.push(site.file_ordinal);
            }
        }
        ordinals.sort();
        ordinals.dedup();
        Ok(ordinals)
    }

    pub fn install_private_collision_sources(
        mut self,
        mut sources: Vec<PrivateCollisionReplaySource>,
    ) -> Result<Self, &'static str> {
        let expected = self.private_collision_source_ordinals()?;
        let epoch = self
            .private_collision_epoch
            .as_mut()
            .ok_or("private collision replay is not installed")?;
        if !epoch.sources.is_empty() {
            return Err("private collision sources install exactly once");
        }
        sources.sort_by_key(|source| source.file_ordinal);
        let observed = sources
            .iter()
            .map(|source| source.file_ordinal)
            .collect::<Vec<_>>();
        if observed != expected {
            return Err("private collision sources do not cover the scheduled ordinals");
        }
        #[cfg(any(test, feature = "test-utils"))]
        with_private_replay_scale_control_for_test(|active| {
            active.trace.sparse_library_source_units = active
                .trace
                .sparse_library_source_units
                .saturating_add(u64::try_from(sources.len()).unwrap_or(u64::MAX));
        });
        epoch.sources = sources;
        Ok(self)
    }

    pub fn take_private_collision_sources(
        &mut self,
    ) -> Result<Vec<PrivateCollisionReplaySource>, &'static str> {
        let epoch = self
            .private_collision_epoch
            .as_mut()
            .ok_or("private collision replay is not installed")?;
        Ok(std::mem::take(&mut epoch.sources))
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn private_collision_affected_owners_for_test(
        &self,
    ) -> Result<BTreeSet<ReplayOwner>, &'static str> {
        self.private_collision_epoch
            .as_ref()
            .map(|epoch| epoch.affected_owners.clone())
            .ok_or("private collision replay is not installed")
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn private_collision_owner_sites_for_test(
        &self,
    ) -> Result<Vec<super::replay_index::CollisionReplayOwnerSite>, &'static str> {
        self.private_collision_epoch
            .as_ref()
            .map(|epoch| epoch.owner_sites.clone())
            .ok_or("private collision replay is not installed")
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn fork_user_delta_for_test(&self) -> Result<Self, &'static str> {
        self.fork_user_delta()
    }

    #[cfg(test)]
    pub(super) fn shares_checker_base_with(&self, other: &Self) -> bool {
        self.published_types
            .shares_base_with(&other.published_types)
            && self.decl_types.shares_base_with(&other.decl_types)
            && self.runtime.shares_base_with(&other.runtime)
            && match (&self.semantic_identities, &other.semantic_identities) {
                (Some(left), Some(right)) => left.shares_storage_with(right),
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
    }

    /// Reference-row counts of the frozen base: store, interner identity, binder.
    ///
    /// A user delta that leaked a row into the base would move one of these.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn reference_record_counts_for_test(&self) -> [usize; 3] {
        let (store, interner) = self.interner.reference_records_for_test();
        let binder = crate::binder::references::reference_records(&self.binder)
            .expect("frozen binder projects its reference rows")
            .len();
        [store.len(), interner.len(), binder]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn storage_identity_for_test(&self) -> [usize; 8] {
        [
            std::ptr::from_ref(self).addr(),
            std::ptr::from_ref(&self.interner).addr(),
            std::ptr::from_ref(self.interner.store()).addr(),
            std::ptr::from_ref(&self.binder).addr(),
            std::ptr::from_ref(&self.published_types).addr(),
            std::ptr::from_ref(&self.decl_types).addr(),
            std::ptr::from_ref(&self.runtime).addr(),
            self.semantic_identities
                .as_ref()
                .map_or(0, |identities| std::ptr::from_ref(identities).addr()),
        ]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn inner_base_allocation_identity_for_test(&self) -> usize {
        self.interner.store().base_allocation_identity_for_test()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn initial_visible_user_names_for_test(&self) -> BTreeSet<String> {
        self.binder.local_names_for_test()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn install_user_delta_drop_witness_for_test(
        &mut self,
        discarded: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        self.interner
            .install_user_delta_drop_witness_for_test(discarded);
    }

    /// Frozen id prefixes in `FrozenLibraryPrefixes` order: types, type params, classes,
    /// scopes, symbols, declarations, type groups, namespaces, value storages.
    pub fn library_prefixes(&self) -> Result<[usize; 9], &'static str> {
        Ok([
            self.interner.store().len(),
            usize::try_from(self.next_type_param)
                .map_err(|_| "type parameter end exceeds usize")?,
            usize::try_from(self.next_class_id).map_err(|_| "class end exceeds usize")?,
            self.binder.graph.len(),
            self.binder.symbols.len(),
            self.binder.declarations.len(),
            self.binder.type_groups.len(),
            self.binder.namespaces.len(),
            self.decl_types.len(),
        ])
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn identity_ends_for_test(&self) -> OwnedBaseFinalIdentityEnds {
        OwnedBaseFinalIdentityEnds {
            store: self.interner.store().len(),
            declared_recipes: self.interner.store().all_declared_recipes().count(),
            type_params: usize::try_from(self.next_type_param)
                .expect("type parameter end fits usize"),
            classes: usize::try_from(self.next_class_id).expect("class end fits usize"),
            scopes: self.binder.graph.len(),
            symbols: self.binder.symbols.len(),
            declarations: self.binder.declarations.len(),
            type_groups: self.binder.type_groups.len(),
            namespaces: self.binder.namespaces.len(),
            value_storages: self.decl_types.len(),
            source_units: self.binder.checkpoint_ends().next_source,
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn named_type_for_test(&self, name: &str) -> Option<TypeId> {
        let group = self
            .binder
            .resolve_type(self.binder.compilation_global, name)
            .and_then(|symbol| self.binder.symbols.get(symbol))?
            .ty?;
        let PublishedTypeGroupTerminal::Ready(group) = self.published_types.groups().get(group)?
        else {
            return None;
        };
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) | PublishedTypeGroupSurface::Enum(ty) => {
                Some(ty)
            }
            PublishedTypeGroupSurface::Class(class) => {
                match self.published_types.classes().published_class(class) {
                    DemandOutcome::Ready(surface) => Some(surface.instance_template()),
                    DemandOutcome::Exhausted(_) => None,
                }
            }
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn normalized_root_projection_for_test(&self, names: &[String]) -> Vec<String> {
        let mut projection = names
            .iter()
            .map(|name| {
                let symbol = self
                    .binder
                    .graph
                    .get(self.binder.compilation_global)
                    .and_then(|scope| scope.symbols.get(name))
                    .and_then(|symbol| self.binder.symbols.get(*symbol));
                let value = symbol.and_then(|symbol| symbol.value);
                let namespace = symbol.and_then(|symbol| symbol.ns);
                let ty = symbol
                    .and_then(|symbol| symbol.ty)
                    .and_then(|group| self.published_types.groups().get(group))
                    .map(|terminal| match terminal {
                        PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                            PublishedTypeGroupSurface::Template(template)
                            | PublishedTypeGroupSurface::Enum(template) => {
                                render_type(self.interner.store(), template, false)
                            }
                            PublishedTypeGroupSurface::Class(class) => {
                                match self.published_types.classes().published_class(class) {
                                    DemandOutcome::Ready(surface) => render_type(
                                        self.interner.store(),
                                        surface.instance_template(),
                                        false,
                                    ),
                                    DemandOutcome::Exhausted(_) => "class:unavailable".to_owned(),
                                }
                            }
                        },
                        PublishedTypeGroupTerminal::Unavailable(cause) => {
                            format!("unavailable:{:?}", cause.cause)
                        }
                    });
                format!(
                    "{name}:value={}:type={}:namespace={}",
                    value.is_some(),
                    ty.unwrap_or_else(|| "absent".to_owned()),
                    namespace.is_some()
                )
            })
            .collect::<Vec<_>>();
        projection.sort();
        projection
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn normalized_semantic_identities_for_test(&self) -> Vec<String> {
        self.semantic_identities
            .as_ref()
            .map_or_else(Vec::new, |identities| {
                identities.normalized_projection_for_test(self.interner.store())
            })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn frozen_structural_object_probe_for_test(&self) -> Option<(TypeId, ObjectType)> {
        self.interner.frozen_structural_object_probe_for_test()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn reintern_structural_type_for_test(
        &self,
        descriptor: ObjectType,
    ) -> Result<(TypeId, usize), &'static str> {
        let mut delta = self.fork_user_delta_for_test()?;
        let before = delta.interner.store().len();
        let resolved = delta.interner.intern_object(descriptor);
        Ok((resolved, delta.interner.store().len() - before))
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn final_base_family_clone_counts_for_test<Ticket: Copy + PartialEq>(
        &self,
        pass: &super::context::Pass<'_, '_, Ticket>,
        final_next_class_id: u32,
    ) -> BTreeMap<&'static str, u64> {
        let store = self
            .interner
            .store()
            .base_family_sharing_with(pass.interner.store());
        let interner = self.interner.base_index_family_sharing_with(pass.interner);
        let binder = self.binder.base_family_sharing_with(pass.binder);
        let published = self
            .published_types
            .base_family_sharing_with(pass.type_environment.published());
        let identities = match (
            self.semantic_identities.as_ref(),
            pass.library_semantic_identities.as_ref(),
        ) {
            (Some(base), Some(final_state)) => base.shares_storage_with(final_state),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        };
        let runtime = &self.runtime;
        let checks = [
            ("store.rows", store[0]),
            ("store.payload-tables", store[1]),
            ("store.type-param-constraints", store[2]),
            ("store.frozen-type-params", store[3]),
            ("store.template-names", store[4]),
            ("interner.dedup-buckets", interner[0]),
            ("interner.reserved-terminals", interner[1]),
            ("interner.declared-recipes", interner[2]),
            ("interner.well-known", interner[3]),
            ("binder.scopes", binder[0]),
            ("binder.symbols", binder[1]),
            ("binder.declarations", binder[2]),
            ("binder.declaration-site-index", binder[3]),
            ("binder.type-groups", binder[4]),
            ("binder.namespaces", binder[5]),
            ("binder.namespace-indexes", binder[6]),
            ("binder.module-sources", binder[7]),
            (
                "decl-types.slots",
                self.decl_types.shares_base_with(&pass.decl_types),
            ),
            ("published-types.groups", published[0]),
            ("published-types.classes", published[1]),
            (
                "namespace-terminals",
                pass.namespace_values
                    .terminals_share_base_with(&runtime.namespace_terminals),
            ),
            (
                "function-groups.symbols",
                runtime
                    .named_function_symbols
                    .shares_base_with(&pass.named_function_symbols),
            ),
            (
                "class.application-parameters",
                runtime
                    .class_application_parameters
                    .shares_base_with(&pass.class_application_parameters),
            ),
            ("class.parameter-defaults", published[0]),
            (
                "class.parents",
                runtime.class_parents.shares_base_with(&pass.class_parents),
            ),
            (
                "class.names",
                runtime.class_names.shares_base_with(&pass.class_names),
            ),
            (
                "class.new-metadata",
                runtime
                    .class_new_metadata
                    .shares_base_with(&pass.class_new_metadata),
            ),
            (
                "class.value-identities",
                runtime
                    .class_value_bindings
                    .shares_base_with(&pass.class_value_bindings),
            ),
            (
                "class.aliases",
                runtime
                    .class_value_aliases
                    .shares_base_with(&pass.class_value_aliases)
                    && runtime
                        .standalone_namespace_value_aliases
                        .shares_base_with(&pass.standalone_namespace_value_aliases),
            ),
            ("semantic-identities", identities),
            (
                "next-ids",
                pass.next_type_param >= self.next_type_param
                    && final_next_class_id >= self.next_class_id,
            ),
        ];
        checks
            .into_iter()
            .map(|(family, shared)| (family, u64::from(!shared)))
            .collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn final_local_rows_written_for_test<Ticket: Copy + PartialEq>(
        pass: &super::context::Pass<'_, '_, Ticket>,
    ) -> u64 {
        let store = pass.interner.store().local_family_row_counts_for_test();
        let interner = pass.interner.local_index_row_counts_for_test();
        let binder = pass.binder.local_family_row_counts_for_test();
        let published = pass
            .type_environment
            .published()
            .local_family_row_counts_for_test();
        let published_replacements = pass
            .type_environment
            .published()
            .replacement_family_row_counts_for_test();
        let rows = store.into_iter().sum::<usize>()
            + interner.into_iter().sum::<usize>()
            + binder.into_iter().sum::<usize>()
            + pass.decl_types.local_len()
            + pass.decl_types.replacement_len()
            + published.into_iter().sum::<usize>()
            + published_replacements.into_iter().sum::<usize>()
            + pass.namespace_values.local_terminal_row_count_for_test()
            + pass
                .namespace_values
                .replacement_terminal_row_count_for_test()
            + pass.named_function_symbols.local_len()
            + pass.class_application_parameters.local_len()
            + pass.class_application_parameters.replacement_len()
            + pass.class_parents.local_len()
            + pass.class_parents.replacement_len()
            + pass.class_names.local_len()
            + pass.class_names.replacement_len()
            + pass.class_new_metadata.local_len()
            + pass.class_new_metadata.replacement_len()
            + pass.class_value_bindings.local_len()
            + pass.class_value_bindings.replacement_len()
            + pass.class_value_aliases.local_len()
            + pass.class_value_aliases.replacement_len()
            + pass.standalone_namespace_value_aliases.local_len()
            + pass.standalone_namespace_value_aliases.replacement_len();
        u64::try_from(rows).expect("local row count fits u64")
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn type_count(&self) -> usize {
        self.interner.store().len()
    }

    pub(in crate::check::checker) fn into_product_parts(
        self,
    ) -> Result<OwnedLibraryRuntimeProductParts, &'static str> {
        if self
            .semantic_identities
            .as_ref()
            .is_some_and(|identities| !identities.all_ready())
        {
            return Err("product installed semantic identities are not all ready");
        }
        let parts = OwnedLibraryRuntimeProductParts {
            interner: self.interner,
            binder: self.binder,
            published_types: self.published_types.product_parts()?,
            decl_types: self.decl_types.snapshot_slots(),
            semantic_identities: self
                .semantic_identities
                .as_ref()
                .map(super::library_identities::LibrarySemanticIdentities::product_parts),
            runtime: self.runtime.snapshot_parts()?,
            next_type_param: self.next_type_param,
            next_class_id: self.next_class_id,
            source_file_count: self.source_file_count,
        };
        validate_owned_library_product_parts(&parts)?;
        Ok(parts)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn replay_index(&self) -> Option<&AdmittedCollisionReplayIndex> {
        self.replay_index.as_deref()
    }

    /// Restore a runtime from its own product parts; the base it yields carries no replay index.
    #[cfg(test)]
    pub(in crate::check::checker) fn from_product_parts(
        parts: OwnedLibraryRuntimeProductParts,
    ) -> Result<Self, &'static str> {
        validate_owned_library_product_parts(&parts)?;
        let library_modules =
            Self::collect_library_modules(&parts.binder, parts.source_file_count)?;
        let decl_types = DeclTypes::from_snapshot_slots(parts.decl_types, parts.binder.decl_count)?;
        let semantic_identities = parts
            .semantic_identities
            .map(super::library_identities::LibrarySemanticIdentities::from_product_parts)
            .transpose()?;
        if semantic_identities
            .as_ref()
            .is_some_and(|identities| !identities.all_ready())
        {
            return Err("product installed semantic identities are not all ready");
        }
        Ok(Self {
            interner: parts.interner,
            binder: parts.binder,
            published_types: PublishedTypeEnvironment::from_product_parts(parts.published_types)?,
            decl_types,
            semantic_identities,
            runtime: FrozenCheckerRuntimeMetadata::from_snapshot_parts(parts.runtime)?,
            next_type_param: parts.next_type_param,
            next_class_id: parts.next_class_id,
            source_file_count: parts.source_file_count,
            library_modules,
            replay_index: None,
            private_collision_epoch: None,
        })
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_synthetic_padding_base_for_test(
    namespace_count: usize,
) -> Result<OwnedLibraryRuntimeState, String> {
    let mut source = String::new();
    for index in 0..namespace_count {
        source.push_str(&format!(
            "declare namespace Pad{index} {{\n\
             export class Box<T> {{ value: T; }}\n\
             export interface Shape {{ value: string; }}\n\
             export const value: string;\n\
             }}\n"
        ));
    }
    let injected = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "synthetic-padding.d.ts",
        source: &source,
    }];
    let (_, mut state) = compile_owned_injected_profile(&injected)
        .map_err(|error| format!("synthetic padding base failed: {error:?}"))?;
    state.freeze_as_library_base().map_err(str::to_owned)?;
    Ok(state)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn with_forced_collision_identity_selection_pending<T>(run: impl FnOnce() -> T) -> T {
    super::library_identities::with_forced_collision_identity_selection_pending(run)
}
