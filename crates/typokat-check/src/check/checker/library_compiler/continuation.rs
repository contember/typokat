use super::*;

pub fn compile_library_binder_checkpoint(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<LibraryBinderCheckpoint, InjectedProfileError> {
    with_canonical_library_frontend(sources, |frontend| {
        #[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
        CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS.set(
            CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS
                .get()
                .saturating_add(1),
        );
        let CanonicalLibraryFrontend {
            canonical,
            parsed,
            declaration_manifests,
            binder,
            module_scopes,
            ..
        } = frontend;
        let mut declaration_fragment_batch =
            super::decls::inventory::CanonicalFragmentAuthenticationBatch::default();
        for (((input, _program), module), manifest) in canonical
            .iter()
            .zip(&parsed)
            .zip(module_scopes.iter().copied())
            .zip(declaration_manifests)
        {
            let _inventory =
                super::decls::inventory::BoundTypeDeclarationInventory::build_in_batch(
                    &binder,
                    module,
                    input.source_key,
                    manifest,
                    None,
                    &mut declaration_fragment_batch,
                )
                .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
        }
        let library_units = canonical
            .iter()
            .zip(module_scopes)
            .map(|(input, module)| LibraryBinderUnit {
                ordinal: input.file_ordinal,
                source: input.source_key,
                module,
            })
            .collect();
        Ok(build_library_binder_checkpoint(binder, library_units))
    })
}

pub(super) fn build_library_binder_checkpoint(
    binder: Binder,
    library_units: Vec<LibraryBinderUnit>,
) -> LibraryBinderCheckpoint {
    LibraryBinderCheckpoint::new(binder, library_units)
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinderContinuationModuleSourceForTest {
    pub module: ScopeId,
    pub source: crate::binder::namespace::SourceUnitKey,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
pub struct LibraryBinderContinuationForTest {
    pub bound: super::BoundProjectBinder,
    pub checkpoint_ends: LibraryBinderCheckpointEnds,
    pub ends: LibraryBinderCheckpointEnds,
    pub array_symbol_before_augmentation: SymbolId,
    pub array_type_group_before_augmentation: TypeGroupId,
    pub array_symbol_after_augmentation: SymbolId,
    pub array_type_group_after_augmentation: TypeGroupId,
    pub consumer_array_type_group: TypeGroupId,
    pub augmentation_declaration: DeclId,
    pub appended_scopes: Vec<ScopeId>,
    pub appended_symbols: Vec<SymbolId>,
    pub appended_declarations: Vec<DeclId>,
    pub appended_type_groups: Vec<TypeGroupId>,
    pub appended_namespaces: Vec<NamespaceId>,
    pub appended_value_storages: Vec<ValueStorageId>,
    pub appended_module_sources: Vec<BinderContinuationModuleSourceForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectBindingLookupForTest {
    pub symbol: SymbolId,
    pub value: Option<ValueStorageId>,
    pub type_group: Option<TypeGroupId>,
    pub namespace: Option<NamespaceId>,
    pub blocks_type_lookup: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl LibraryBinderContinuationForTest {
    pub fn normalized_per_path_binding_shape_for_test(&self) -> Vec<String> {
        self.bound
            .normalized
            .normalized_per_path_binding_shape
            .clone()
    }

    pub fn project_sources_for_test(&self) -> &[super::ProjectSourceBindingRow] {
        &self.bound.project_sources
    }

    fn project_module_for_test(&self, path: &str) -> Option<ScopeId> {
        self.bound
            .project_sources
            .iter()
            .find(|row| row.normalized_path == path)
            .map(|row| row.module)
    }

    pub fn lookup_binding_for_test(
        &self,
        path: &str,
        name: &str,
    ) -> Option<ProjectBindingLookupForTest> {
        let module = self.project_module_for_test(path)?;
        let symbol = self.bound.binder.graph.resolve(module, name)?;
        let slots = self.bound.binder.symbols.get(symbol)?;
        Some(ProjectBindingLookupForTest {
            symbol,
            value: slots.value,
            type_group: slots.ty,
            namespace: slots.ns,
            blocks_type_lookup: slots.blocks_type_lookup,
        })
    }

    pub fn import_placeholders_for_test(&self, path: &str) -> Vec<ValueStorageId> {
        let Some(module) = self.project_module_for_test(path) else {
            return Vec::new();
        };
        self.bound
            .module_scopes
            .iter()
            .position(|candidate| *candidate == module)
            .and_then(|index| self.bound.module_placeholders.get(index))
            .into_iter()
            .flatten()
            .filter_map(|placeholder| placeholder.value)
            .collect()
    }

    pub fn script_namespace_root_reservation_for_test(&self, name: &str) -> Option<SymbolId> {
        self.bound
            .binder
            .graph
            .get(self.bound.binder.script_namespace_root)?
            .lookup_local(name)
    }

    pub fn standalone_namespace_value_storage_for_test(
        &self,
        path: &str,
        name: &str,
    ) -> Option<ValueStorageId> {
        let namespace = self.lookup_binding_for_test(path, name)?.namespace?;
        self.bound
            .binder
            .namespaces
            .standalone_value_storage(namespace)
    }

    pub fn attached_namespace_value_disposition_for_test(
        &self,
        path: &str,
        name: &str,
    ) -> Option<crate::binder::namespace::NamespaceValueAttachmentDisposition> {
        let module = self.project_module_for_test(path)?;
        self.bound
            .binder
            .namespace_value_attachment(module, name)
            .map(|attachment| attachment.disposition)
    }
}

pub fn continue_library_project_binder(
    checkpoint: LibraryBinderCheckpoint,
    inputs: Vec<crate::frontend::FileInput>,
) -> Result<super::BoundProjectBinder, String> {
    crate::frontend::run_project_frontend(inputs, |_, units| {
        #[cfg(any(test, feature = "test-utils"))]
        record_user_source_parses_for_test(units.len());
        let bound = super::bind_library_checkpoint_project_programs(checkpoint, units)?;
        #[cfg(any(test, feature = "test-utils"))]
        {
            record_user_source_binds_for_test(units.len());
            super::record_continuation_project_binding_consumed_for_test();
        }
        Ok(bound)
    })
    .into_product()
}

#[cfg(any(test, feature = "test-utils"))]
pub fn continue_library_project_binder_clean_for_test(
    checkpoint: LibraryBinderCheckpoint,
    inputs: Vec<crate::frontend::FileInput>,
) -> Result<super::BoundProjectBinder, String> {
    crate::frontend::run_clean_project_frontend_with_auxiliary(inputs, Vec::new(), |_, _, units| {
        record_user_source_parses_for_test(units.len());
        let bound = super::bind_library_checkpoint_project_programs(checkpoint, units)?;
        record_user_source_binds_for_test(units.len());
        super::record_continuation_project_binding_consumed_for_test();
        Ok(bound)
    })
    .into_product()
    .ok_or_else(|| "user source parse failed before project binding".to_owned())?
}

#[cfg(any(test, feature = "test-utils"))]
pub fn continuation_receipt_for_test(
    checkpoint_ends: LibraryBinderCheckpointEnds,
    array_symbol_before_augmentation: SymbolId,
    array_type_group_before_augmentation: TypeGroupId,
    bound: super::BoundProjectBinder,
) -> Result<LibraryBinderContinuationForTest, String> {
    let binder = &bound.binder;
    let ends = binder.checkpoint_ends();
    let array_symbol_after_augmentation =
        binder
            .resolve_type(binder.compilation_global, "Array")
            .ok_or_else(|| "continued binder lost Array".to_owned())?;
    let array_type_group_after_augmentation = binder
        .symbols
        .get(array_symbol_after_augmentation)
        .and_then(|symbol| symbol.ty)
        .ok_or_else(|| "continued Array lost its type group".to_owned())?;
    // Generic project receipts retain the first appended declaration when Array is untouched.
    let augmentation_declaration = binder
        .type_groups
        .get(array_type_group_after_augmentation)
        .and_then(|group| {
            group
                .fragments
                .iter()
                .find(|fragment| fragment.declaration.index() >= checkpoint_ends.declarations)
        })
        .map(|fragment| fragment.declaration)
        .or_else(|| {
            (checkpoint_ends.declarations < ends.declarations).then(|| {
                DeclId(
                    u32::try_from(checkpoint_ends.declarations)
                        .expect("declaration prefix fits u32"),
                )
            })
        })
        .ok_or_else(|| "continued project appended no declaration".to_owned())?;
    let consumer_array_type_group = bound
        .module_scopes
        .last()
        .and_then(|module| binder.resolve_type(*module, "Array"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty)
        .ok_or_else(|| "consumer cannot resolve continued Array".to_owned())?;
    let appended_module_sources = bound
        .project_sources
        .iter()
        .map(|row| BinderContinuationModuleSourceForTest {
            module: row.module,
            source: row.source,
        })
        .collect();
    Ok(LibraryBinderContinuationForTest {
        bound,
        checkpoint_ends,
        ends,
        array_symbol_before_augmentation,
        array_type_group_before_augmentation,
        array_symbol_after_augmentation,
        array_type_group_after_augmentation,
        consumer_array_type_group,
        augmentation_declaration,
        appended_scopes: (checkpoint_ends.scopes..ends.scopes)
            .map(|id| ScopeId(u32::try_from(id).expect("scope id fits u32")))
            .collect(),
        appended_symbols: (checkpoint_ends.symbols..ends.symbols)
            .map(|id| SymbolId(u32::try_from(id).expect("symbol id fits u32")))
            .collect(),
        appended_declarations: (checkpoint_ends.declarations..ends.declarations)
            .map(|id| DeclId(u32::try_from(id).expect("declaration id fits u32")))
            .collect(),
        appended_type_groups: (checkpoint_ends.type_groups..ends.type_groups)
            .map(|id| TypeGroupId(u32::try_from(id).expect("type-group id fits u32")))
            .collect(),
        appended_namespaces: (checkpoint_ends.namespaces..ends.namespaces)
            .map(|id| NamespaceId(u32::try_from(id).expect("namespace id fits u32")))
            .collect(),
        appended_value_storages: (checkpoint_ends.value_storages..ends.value_storages)
            .map(|id| ValueStorageId(u32::try_from(id).expect("value-storage id fits u32")))
            .collect(),
        appended_module_sources,
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_caller_certified_collision_free_project_with_owned_library(
    state: OwnedLibraryRuntimeState,
    inputs: Vec<crate::frontend::FileInput>,
    expected_base: &OwnedLibraryRuntimeState,
) -> Result<OwnedBaseUserProjectRun, String> {
    let base_ends = state.identity_ends_for_test();
    let initial_visible_names = state.initial_visible_user_names_for_test();
    if !initial_visible_names.is_empty() {
        return Err("fresh project delta exposes prior user names".to_owned());
    }
    let cross_file = std::cell::RefCell::new(None);
    let final_identity = std::cell::RefCell::new(None);
    let state = std::cell::RefCell::new(Some(state));
    super::decls::reset_class_allocation_events_for_test();
    let reports =
        crate::check::test_support::check_project_with_owned_checker_for_test(inputs, |units| {
            let state = state
                .borrow_mut()
                .take()
                .expect("owned project delta is consumed once");
            super::check_project_programs_with_owned_library(
                state,
                units,
                |binder, module_scopes| {
                    let producer = module_scopes
                        .iter()
                        .copied()
                        .find_map(|scope| {
                            let ty = binder
                                .resolve_type(scope, "SharedShape")
                                .and_then(|symbol| binder.symbols.get(symbol))?
                                .ty?;
                            let value = binder
                                .resolve_value(scope, "sharedValue")
                                .and_then(|symbol| binder.symbols.get(symbol))?
                                .value?;
                            Some((scope, ty, value))
                        })
                        .expect("project producer exports both test bindings");
                    let consumer = module_scopes
                        .iter()
                        .copied()
                        .filter(|scope| *scope != producer.0)
                        .find_map(|scope| {
                            let ty = binder
                                .resolve_type(scope, "SharedShape")
                                .and_then(|symbol| binder.symbols.get(symbol))?
                                .ty?;
                            let value = binder
                                .resolve_value(scope, "sharedValue")
                                .and_then(|symbol| binder.symbols.get(symbol))?
                                .value?;
                            Some((ty, value))
                        })
                        .expect("project consumer imports both test bindings");
                    cross_file.replace(Some(OwnedBaseCrossFileWitness {
                        producer_type_group: producer.1,
                        consumer_type_group: consumer.0,
                        producer_value_storage: producer.2,
                        consumer_value_storage: consumer.1,
                    }));
                },
                |pass, final_next_class_id| {
                    final_identity.replace(Some(final_identity_witness(FinalIdentityInspection {
                        base_store_len: base_ends.store,
                        base_value_storage_len: base_ends.value_storages,
                        base_type_group_len: base_ends.type_groups,
                        base_namespace_len: base_ends.namespaces,
                        binder: pass.binder,
                        published: pass.type_environment.published(),
                        interner: pass.interner,
                        decl_types: &pass.decl_types,
                        next_type_param: pass.next_type_param,
                        next_class_id: final_next_class_id,
                        actual_class_ids: super::decls::class_allocation_events_for_test(),
                        references: final_reference_summary(pass, &base_ends),
                        base_row_clone_counts: expected_base
                            .final_base_family_clone_counts_for_test(pass, final_next_class_id),
                        local_rows_written:
                            OwnedLibraryRuntimeState::final_local_rows_written_for_test(pass),
                    })));
                },
            )
            .expect("the owned-base project route's continuation binds")
        });
    Ok(OwnedBaseUserProjectRun {
        reports,
        final_identity: final_identity
            .into_inner()
            .expect("owned project route captures final identities"),
        cross_file: cross_file
            .into_inner()
            .expect("owned project route captures cross-file identities"),
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_caller_certified_collision_free_source_with_owned_library(
    state: OwnedLibraryRuntimeState,
    source: &str,
) -> Result<OwnedBaseUserRun, String> {
    check_caller_certified_collision_free_source_with_owned_library_impl(state, source, false, None)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_caller_certified_collision_free_source_with_base_evidence(
    state: OwnedLibraryRuntimeState,
    source: &str,
    expected_base: &OwnedLibraryRuntimeState,
) -> Result<OwnedBaseUserRun, String> {
    check_caller_certified_collision_free_source_with_owned_library_impl(
        state,
        source,
        false,
        Some(expected_base),
    )
}

#[cfg(test)]
pub(super) fn check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix(
    state: OwnedLibraryRuntimeState,
    source: &str,
) -> Result<OwnedBaseUserRun, String> {
    check_caller_certified_collision_free_source_with_owned_library_impl(state, source, true, None)
}

#[cfg(any(test, feature = "test-utils"))]
fn check_caller_certified_collision_free_source_with_owned_library_impl(
    state: OwnedLibraryRuntimeState,
    source: &str,
    verify_store_prefix: bool,
    expected_base: Option<&OwnedLibraryRuntimeState>,
) -> Result<OwnedBaseUserRun, String> {
    // WU5 owns routing for suffixes that collide with the frozen global base.
    let parse_started = Instant::now();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    if parsed.panicked || !parsed.diagnostics.is_empty() {
        return Err(format!(
            "user source parse failed: {:?}",
            parsed.diagnostics
        ));
    }
    record_user_source_parses_for_test(1);
    let parse = parse_started.elapsed();

    let OwnedLibraryRuntimeState {
        mut interner,
        binder,
        published_types,
        mut decl_types,
        semantic_identities,
        runtime,
        next_type_param,
        next_class_id,
        source_file_count: _,
        library_modules: _,
        replay_index: _,
        private_collision_epoch: _,
    } = state;
    let base_store_len = interner.store().len();
    let base_declared_recipe_len = interner.store().all_declared_recipes().count();
    let base_store_digest = verify_store_prefix
        .then(|| store_prefix_digest(interner.store(), base_store_len, base_declared_recipe_len))
        .transpose()?;
    let base_type_group_count = binder.type_groups.len();
    let base_namespace_count = binder.namespaces.len();
    let base_decl_count = binder.decl_count;
    let base_identity_ends = OwnedBaseFinalIdentityEnds {
        store: base_store_len,
        declared_recipes: base_declared_recipe_len,
        type_params: usize::try_from(next_type_param).expect("base type parameter end fits usize"),
        classes: usize::try_from(next_class_id).expect("base class end fits usize"),
        scopes: binder.graph.len(),
        symbols: binder.symbols.len(),
        declarations: binder.declarations.len(),
        type_groups: base_type_group_count,
        namespaces: base_namespace_count,
        value_storages: usize::try_from(base_decl_count).expect("base storage end fits usize"),
        source_units: binder.checkpoint_ends().next_source,
    };
    let base_max_source_key = binder.max_source_key().0;
    assert_eq!(
        decl_types.len(),
        usize::try_from(base_decl_count).expect("base declaration count fits usize"),
        "owned declaration types cover the complete binder prefix"
    );
    let base_array_group = binder
        .resolve_type(binder.compilation_global, "Array")
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty);
    let base_document_value = binder
        .resolve_value(binder.compilation_global, "document")
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.value);
    let bind_started = Instant::now();
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    let (mut builder, source_key) = ProjectBinderBuilder::resume_frozen_library(binder);
    let unit = CompilationUnit::implementation(source_key, &parsed.program);
    builder.reserve_script_namespace_roots([(&parsed.program, unit)]);
    let (module, _) =
        builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
    let binder = builder
        .finish_frozen_library_continuation(Some(module))
        .map_err(str::to_owned)?;
    decl_types.resize(binder.decl_count);
    let witness_source_key = source_key.0;
    let array_group_stable = base_array_group
        == binder
            .resolve_type(binder.module, "Array")
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty);
    let document_value_stable = base_document_value
        == binder
            .resolve_value(binder.module, "document")
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.value);
    let final_type_group_count = binder.type_groups.len();
    let final_decl_count = binder.decl_count;
    record_user_source_binds_for_test(1);
    let bind = bind_started.elapsed();

    let check_started = Instant::now();
    let final_identity = std::cell::RefCell::new(None);
    super::decls::reset_class_allocation_events_for_test();
    let result = check_bound_user_program_with_final_identity_inspector(
        &mut interner,
        binder,
        &parsed.program,
        manifest,
        BoundUserBase {
            published_types,
            library_semantic_identities: semantic_identities,
            lexical_array_alias: None,
            decl_types,
            next_type_param,
            next_class_id,
            runtime,
            private_collision_epoch: None,
            library_modules: std::sync::Arc::from([]),
        },
        |_, _, _, _, _| {},
        |pass, final_next_class_id| {
            final_identity.replace(Some(final_identity_witness(FinalIdentityInspection {
                base_store_len,
                base_value_storage_len: usize::try_from(base_decl_count)
                    .expect("base storage end fits usize"),
                base_type_group_len: base_type_group_count,
                base_namespace_len: base_namespace_count,
                binder: pass.binder,
                published: pass.type_environment.published(),
                interner: pass.interner,
                decl_types: &pass.decl_types,
                next_type_param: pass.next_type_param,
                next_class_id: final_next_class_id,
                actual_class_ids: super::decls::class_allocation_events_for_test(),
                references: final_reference_summary(pass, &base_identity_ends),
                base_row_clone_counts: expected_base.map_or_else(BTreeMap::new, |expected_base| {
                    expected_base.final_base_family_clone_counts_for_test(pass, final_next_class_id)
                }),
                local_rows_written: OwnedLibraryRuntimeState::final_local_rows_written_for_test(
                    pass,
                ),
            })));
        },
    )?;
    record_user_source_checks_for_test(1);
    let check = check_started.elapsed();
    let final_identity = final_identity
        .into_inner()
        .expect("owned-base route captures final identities after effects");
    let final_store_len = final_identity.ends.store;
    let store_prefix_stable = base_store_digest
        .map(|base_store_digest| {
            store_prefix_digest(interner.store(), base_store_len, base_declared_recipe_len)
                .map(|final_store_digest| final_store_digest == base_store_digest)
        })
        .transpose()?;
    Ok(OwnedBaseUserRun {
        result,
        timings: OwnedBaseUserTimings { parse, bind, check },
        final_identity,
        witness: OwnedBaseContinuationWitness {
            base_store_len,
            final_store_len,
            base_type_group_count,
            final_type_group_count,
            base_decl_count,
            final_decl_count,
            source_key: witness_source_key,
            base_max_source_key,
            array_group_stable,
            document_value_stable,
            store_prefix_stable,
        },
    })
}
