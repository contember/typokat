use super::*;

#[cfg(any(test, feature = "test-utils"))]
pub fn run_injected_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<InjectedProfileRun, InjectedProfileError> {
    compile_owned_injected_profile(sources).map(|(run, _)| run)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_complete_combined_profile_for_test(
    sources: &[InjectedLibrarySource<'_>],
    library_count: usize,
) -> Result<(InjectedProfileRun, OwnedLibraryRuntimeState), InjectedProfileError> {
    with_canonical_frontend(sources, Some(library_count), |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::None,
            LibraryRecordRetention::Collect,
            CanonicalLibraryEvidenceCapture::Skip,
            OwnerSiteStorageMode::Flat,
        )
        .map(|(run, runtime, _)| (run, runtime))
    })
}

/// Bind the packaged library and prepared user programs in one fresh project binder.
#[cfg(any(test, feature = "test-utils"))]
pub fn compile_project_declaration_oracle_one_root_for_test<'ast>(
    sources: &[InjectedLibrarySource<'_>],
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[crate::frontend::ProjectProgram<'ast>],
    source_reexports: &crate::frontend::AdmittedSourceReexports,
) -> Result<(Vec<LibraryBinderUnit>, super::BoundProjectBinder), InjectedProfileError> {
    let canonical = canonical_inputs(sources)?;
    if canonical.len() != library_programs.len() {
        return Err(InjectedProfileError::CanonicalProjection(
            "one-root oracle changed the library source count".to_owned(),
        ));
    }

    let mut parser_export_claims = Vec::new();
    for ((input, source), program) in canonical.iter().zip(sources).zip(library_programs) {
        if input.file_ordinal.index() != program.source_ordinal || source.name != program.name {
            return Err(InjectedProfileError::CanonicalProjection(
                "one-root oracle changed library source identity".to_owned(),
            ));
        }
        if program.parser_panicked {
            return Err(InjectedProfileError::Parse {
                file_ordinal: input.file_ordinal,
                messages: if program.parser_diagnostics.is_empty() {
                    vec!["parser panicked without a diagnostic".to_owned()]
                } else {
                    program
                        .parser_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.rendered.clone())
                        .collect()
                },
            });
        }
        for diagnostic in &program.parser_diagnostics {
            let [span] = diagnostic.labels.as_slice() else {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: program
                        .parser_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.rendered.clone())
                        .collect(),
                });
            };
            if diagnostic.scope.as_deref() != Some("TS")
                || diagnostic.number.as_deref() != Some("1319")
            {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: program
                        .parser_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.rendered.clone())
                        .collect(),
                });
            }
            parser_export_claims.push(ParserExportClaim {
                file_ordinal: input.file_ordinal,
                span: *span,
            });
        }
    }

    let manifests = library_programs
        .iter()
        .map(|program| SourceDeclarationManifest::build(program.program))
        .collect::<Vec<_>>();
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
    let mut builder =
        ProjectBinderBuilder::new_with_declaration_manifest(&prelude.program, &prelude_manifest);
    let library_units = canonical
        .iter()
        .zip(library_programs)
        .map(|(input, program)| {
            (
                program.program,
                CompilationUnit {
                    source: input.source_key,
                    origin: input.origin,
                    binding: ModuleBindingContext::for_program(program.program, input.kind),
                },
            )
        })
        .collect::<Vec<_>>();
    let authenticated_units = library_units
        .iter()
        .zip(&manifests)
        .map(|((program, unit), manifest)| (*program, manifest, *unit))
        .collect::<Vec<_>>();
    let module_scopes = builder
        .try_add_library_modules_with_declaration_manifests(&authenticated_units)
        .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
    let library_units = canonical
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|(input, module)| LibraryBinderUnit {
            ordinal: input.file_ordinal,
            source: input.source_key,
            module,
        })
        .collect::<Vec<_>>();
    builder.begin_one_root_project_for_test();
    let source_offset =
        u32::try_from(canonical.len()).map_err(|_| InjectedProfileError::SourceKeyOverflow)?;
    let bound = super::super::bind_one_root_project_programs_for_test(
        builder,
        units,
        source_reexports,
        source_offset,
    )
    .map_err(InjectedProfileError::Binder)?;
    validate_parser_export_claims(
        &bound.binder,
        parser_export_claims,
        canonical[0].file_ordinal,
    )?;
    record_complete_source_bind_for_test(canonical.len(), units.len());
    Ok((library_units, bound))
}

/// Compile the packaged library and one resolved user project in one semantic publication.
pub fn compile_complete_source_project_programs<'ast>(
    sources: &[InjectedLibrarySource<'_>],
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[crate::frontend::ProjectProgram<'ast>],
) -> Result<Vec<super::CheckResult>, InjectedProfileError> {
    compile_complete_source_project_programs_core(sources, library_programs, units, None, None)
}

/// Compile one Bundler project with frontend-certified named source re-exports.
pub fn compile_complete_source_project_programs_with_source_reexports<'ast>(
    sources: &[InjectedLibrarySource<'_>],
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[crate::frontend::ProjectProgram<'ast>],
    source_reexports: &crate::frontend::AdmittedSourceReexports,
) -> Result<Vec<super::CheckResult>, InjectedProfileError> {
    compile_complete_source_project_programs_core(
        sources,
        library_programs,
        units,
        Some(source_reexports),
        None,
    )
}

/// Compile one Bundler project with certified named and default module surfaces.
pub fn compile_complete_source_project_programs_with_default_modules<'ast>(
    sources: &[InjectedLibrarySource<'_>],
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[crate::frontend::ProjectProgram<'ast>],
    source_reexports: &crate::frontend::AdmittedSourceReexports,
    default_modules: &crate::frontend::DefaultModuleCandidates,
) -> Result<Vec<super::CheckResult>, InjectedProfileError> {
    compile_complete_source_project_programs_core(
        sources,
        library_programs,
        units,
        Some(source_reexports),
        Some(default_modules),
    )
}

fn compile_complete_source_project_programs_core<'ast>(
    sources: &[InjectedLibrarySource<'_>],
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[crate::frontend::ProjectProgram<'ast>],
    source_reexports: Option<&crate::frontend::AdmittedSourceReexports>,
    default_modules: Option<&crate::frontend::DefaultModuleCandidates>,
) -> Result<Vec<super::CheckResult>, InjectedProfileError> {
    let default_module_plan = default_modules
        .map(|candidates| super::build_validated_default_module_plan(candidates, units))
        .transpose()
        .map_err(|error| InjectedProfileError::CanonicalProjection(error.message().to_owned()))?;
    let frontend = complete_source_project_frontend(
        sources,
        library_programs,
        units,
        source_reexports,
        default_module_plan.as_ref(),
    )?;
    let (run, _runtime, replay_plan) = compile_owned_injected_frontend(
        frontend,
        ReplayIndexPlan::None,
        LibraryRecordRetention::Collect,
        CanonicalLibraryEvidenceCapture::Capture,
        OwnerSiteStorageMode::Flat,
    )?;
    if replay_plan.is_some() {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source project retained a replay plan".to_owned(),
        ));
    }

    Ok(run.user_results)
}

fn complete_source_project_frontend<'source, 'ast>(
    sources: &[InjectedLibrarySource<'source>],
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[crate::frontend::ProjectProgram<'ast>],
    source_reexports: Option<&crate::frontend::AdmittedSourceReexports>,
    default_module: Option<&super::ValidatedDefaultModulePlan<'_>>,
) -> Result<CanonicalLibraryFrontend<'source, 'ast>, InjectedProfileError> {
    #[cfg(any(test, feature = "test-utils"))]
    let manifest_build_scope =
        crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest::start();
    let source_reexport_plan = source_reexports
        .map(|admitted| super::build_validated_source_reexport_plan(admitted, units))
        .transpose()
        .map_err(|error| InjectedProfileError::SourceReexportEvidence(error.message()))?;
    let mut canonical = canonical_inputs(sources)?;
    if canonical.len() != library_programs.len() {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source frontend changed the library source count".to_owned(),
        ));
    }
    let mut parser_export_claims = Vec::new();
    let mut parsed = Vec::with_capacity(canonical.len().saturating_add(units.len()));
    for ((input, source), program) in canonical.iter().zip(sources).zip(library_programs) {
        if input.file_ordinal.index() != program.source_ordinal || source.name != program.name {
            return Err(InjectedProfileError::CanonicalProjection(
                "complete-source frontend changed library source identity".to_owned(),
            ));
        }
        if program.parser_panicked {
            return Err(InjectedProfileError::Parse {
                file_ordinal: input.file_ordinal,
                messages: if program.parser_diagnostics.is_empty() {
                    vec!["parser panicked without a diagnostic".to_owned()]
                } else {
                    program
                        .parser_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.rendered.clone())
                        .collect()
                },
            });
        }
        for diagnostic in &program.parser_diagnostics {
            let [span] = diagnostic.labels.as_slice() else {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: program
                        .parser_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.rendered.clone())
                        .collect(),
                });
            };
            if diagnostic.scope.as_deref() != Some("TS")
                || diagnostic.number.as_deref() != Some("1319")
            {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: program
                        .parser_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.rendered.clone())
                        .collect(),
                });
            }
            parser_export_claims.push(ParserExportClaim {
                file_ordinal: input.file_ordinal,
                span: *span,
            });
        }
        parsed.push(program.program);
    }
    let mut declaration_manifests = parsed
        .iter()
        .map(|program| SourceDeclarationManifest::build(program))
        .collect::<Vec<_>>();

    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
    let mut builder =
        ProjectBinderBuilder::new_with_declaration_manifest(&prelude.program, &prelude_manifest);
    let library_units = canonical
        .iter()
        .zip(&parsed)
        .map(|(input, program)| {
            (
                *program,
                CompilationUnit {
                    source: input.source_key,
                    origin: input.origin,
                    binding: ModuleBindingContext::for_program(program, input.kind),
                },
            )
        })
        .collect::<Vec<_>>();
    let library_units_with_manifests = library_units
        .iter()
        .zip(&declaration_manifests)
        .map(|((program, unit), manifest)| (*program, manifest, *unit))
        .collect::<Vec<_>>();
    let mut module_scopes = builder
        .try_add_library_modules_with_declaration_manifests(&library_units_with_manifests)
        .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
    let mut library_binder = builder.finish(module_scopes.last().copied().unwrap_or(ScopeId(0)));
    let collision_root_provenance =
        library_binder
            .take_library_root_projection()
            .ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "library binder did not retain its one-pass root projection".to_owned(),
                )
            })?;
    let checkpoint_units = canonical
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|(input, module)| LibraryBinderUnit {
            ordinal: input.file_ordinal,
            source: input.source_key,
            module,
        })
        .collect();
    let checkpoint = build_library_binder_checkpoint(library_binder, checkpoint_units);
    let checkpoint_ends = checkpoint.checkpoint_ends();
    let (builder, _) = checkpoint.into_continuation();
    let source_offset = u32::try_from(checkpoint_ends.next_source)
        .map_err(|_| InjectedProfileError::SourceKeyOverflow)?
        .checked_sub(1)
        .ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "library source prefix omits the prelude".to_owned(),
            )
        })?;
    let mut user_events = EventStore::default();
    let mut combined_lexical_events: LexicalReservations<PrivateCombinedRecordTicket> =
        LexicalReservations::default();
    let user_declaration_manifests = units
        .iter()
        .map(|unit| SourceDeclarationManifest::build(unit.program))
        .collect::<Vec<_>>();
    for (unit, manifest) in units.iter().zip(&user_declaration_manifests) {
        combined_lexical_events
            .reserve_private_continuation_program_with_declaration_manifest(
                unit.module_ordinal,
                unit.unit_slot,
                unit.program,
                manifest,
                unit.compilation_unit.binding,
                &mut user_events,
            )
            .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;
    }
    if let Some(plan) = default_module {
        for (unit, occurrence) in units.iter().zip(&plan.exports_by_owner) {
            let Some(occurrence) = occurrence else {
                continue;
            };
            let (kind, span) = match occurrence.kind() {
                crate::frontend::DefaultExportOccurrenceKind::DirectClass
                    if occurrence.declaration_anonymous() == Some(true) =>
                {
                    (
                        crate::binder::declaration::DeclarationKind::Class,
                        occurrence.subject_span(),
                    )
                }
                crate::frontend::DefaultExportOccurrenceKind::DirectFunction
                    if occurrence.declaration_anonymous() == Some(true) =>
                {
                    (
                        crate::binder::declaration::DeclarationKind::Function,
                        occurrence.subject_span(),
                    )
                }
                crate::frontend::DefaultExportOccurrenceKind::DefaultExpression => (
                    crate::binder::declaration::DeclarationKind::DefaultExportExpression,
                    occurrence.declaration_span(),
                ),
                _ => continue,
            };
            combined_lexical_events
                .attach_candidate_declaration_source(
                    SourceOrdinal::User(unit.module_ordinal),
                    kind,
                    span,
                )
                .map_err(|_| {
                    InjectedProfileError::Reservation(
                        "validated default has no exact lexical owner".to_owned(),
                    )
                })?;
        }
    }
    let mut external_effects = BTreeMap::new();
    let bound = super::bind_authoritative_project_core_with_module_admission(
        builder,
        units,
        &user_declaration_manifests,
        super::ProjectModuleAdmission {
            source_reexports: source_reexport_plan.as_ref(),
            default_module,
        },
        source_offset,
        &combined_lexical_events,
        &mut external_effects,
        super::AuthoritativeProjectBinderFinish::Continuation,
    )
    .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
    validate_parser_export_claims(
        &bound.binder,
        parser_export_claims,
        canonical[0].file_ordinal,
    )?;
    let super::BoundProjectBinder {
        binder,
        module_scopes: user_module_scopes,
        module_placeholders,
        ..
    } = bound;
    #[cfg(any(test, feature = "test-utils"))]
    record_complete_source_bind_for_test(canonical.len(), units.len());

    let library_count = canonical.len();
    for unit in units {
        let ordinal = library_count
            .checked_add(unit.module_ordinal.index())
            .ok_or(InjectedProfileError::SourceKeyOverflow)?;
        let source_key = source_offset
            .checked_add(unit.compilation_unit.source.0)
            .map(exact_key)
            .ok_or(InjectedProfileError::SourceKeyOverflow)?;
        canonical.push(CanonicalInput {
            file_ordinal: LibraryFileOrdinal::new(ordinal),
            source: "",
            kind: source_file_kind(&unit.normalized_path),
            source_key,
            origin: unit.compilation_unit.origin,
        });
        parsed.push(unit.program);
    }
    declaration_manifests.extend(user_declaration_manifests);
    module_scopes.extend(user_module_scopes);
    let semantic_scopes = canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .map(|((input, program), module)| {
            if matches!(input.origin, CompilationOrigin::User(_))
                || ModuleBindingContext::for_program(program, input.kind).external_module
            {
                module
            } else {
                binder.compilation_global
            }
        })
        .collect();
    let prelude_source = binder
        .source_for_module(binder.prelude_module)
        .ok_or_else(|| {
            InjectedProfileError::Binder("prelude module has no source identity".to_owned())
        })?;
    let _prelude_inventory = super::decls::inventory::BoundTypeDeclarationInventory::build(
        &binder,
        binder.prelude_module,
        prelude_source,
        prelude_manifest,
        None,
    )
    .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
    #[cfg(any(test, feature = "test-utils"))]
    assert_eq!(
        manifest_build_scope.finish(),
        u64::try_from(
            library_programs
                .len()
                .saturating_add(units.len())
                .saturating_add(1)
        )
        .unwrap_or(u64::MAX),
        "complete-source frontend builds one manifest per source and prelude"
    );
    Ok(CanonicalLibraryFrontend {
        canonical,
        parsed,
        declaration_manifests,
        binder,
        module_scopes,
        semantic_scopes,
        user_start: Some(library_count),
        user_units: units
            .iter()
            .map(|unit| (unit.module_ordinal, unit.unit_slot))
            .collect(),
        user_events,
        combined_lexical_events: Some(combined_lexical_events),
        external_effects,
        module_placeholders,
        collision_root_provenance,
        #[cfg(any(test, feature = "test-utils"))]
        parse_elapsed: Duration::ZERO,
        #[cfg(any(test, feature = "test-utils"))]
        bind_elapsed: Duration::ZERO,
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub struct CompleteCombinedOracleForTest {
    pub normalized_records: Vec<(LibraryFileOrdinal, String)>,
    pub normalized_root_projection: Vec<String>,
    pub normalized_semantic_identities: Vec<String>,
    pub initial_store_rows: usize,
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_complete_combined_oracle_for_test(
    sources: &[InjectedLibrarySource<'_>],
    library_count: usize,
    root_names: &[String],
) -> Result<CompleteCombinedOracleForTest, InjectedProfileError> {
    let (run, runtime) = compile_complete_combined_profile_for_test(sources, library_count)?;
    if run.initial_store_rows == 0
        || run.initial_published_type_rows != 0
        || run.initial_replacement_type_rows != 0
        || run.initial_type_param_id != 0
        || run.initial_class_id != 0
    {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete combined oracle did not start from fixed intrinsics and empty semantic state"
                .to_owned(),
        ));
    }
    let mut normalized_records = run
        .library_records
        .into_iter()
        .map(|(key, record)| {
            let normalized = match record {
                CheckerRecord::Diagnostic(diagnostic) => {
                    format!("{} {}", diagnostic.code.as_str(), diagnostic.message)
                }
                CheckerRecord::Incomplete(incomplete) => format!("incomplete {incomplete:?}"),
            };
            (key.file_ordinal, normalized)
        })
        .collect::<Vec<_>>();
    for (file_ordinal, record) in run.ordered_user_records {
        let normalized = match record {
            CheckerRecord::Diagnostic(diagnostic) => {
                format!("{} {}", diagnostic.code.as_str(), diagnostic.message)
            }
            CheckerRecord::Incomplete(incomplete) => format!("incomplete {incomplete:?}"),
        };
        normalized_records.push((file_ordinal, normalized));
    }
    Ok(CompleteCombinedOracleForTest {
        normalized_records,
        normalized_root_projection: runtime.normalized_root_projection_for_test(root_names),
        normalized_semantic_identities: runtime.normalized_semantic_identities_for_test(),
        initial_store_rows: run.initial_store_rows,
    })
}

pub(super) fn with_canonical_library_frontend<'source, Output>(
    sources: &[InjectedLibrarySource<'source>],
    consume: impl for<'ast> FnOnce(
        CanonicalLibraryFrontend<'source, 'ast>,
    ) -> Result<Output, InjectedProfileError>,
) -> Result<Output, InjectedProfileError> {
    with_canonical_frontend(sources, None, consume)
}

fn with_canonical_frontend<'source, Output>(
    sources: &[InjectedLibrarySource<'source>],
    user_start: Option<usize>,
    consume: impl for<'ast> FnOnce(
        CanonicalLibraryFrontend<'source, 'ast>,
    ) -> Result<Output, InjectedProfileError>,
) -> Result<Output, InjectedProfileError> {
    #[cfg(any(test, feature = "test-utils"))]
    let manifest_build_scope =
        crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest::start();
    #[cfg(any(test, feature = "test-utils"))]
    let parse_started = Instant::now();
    let mut canonical = canonical_inputs(sources)?;
    if let Some(user_start) = user_start {
        if user_start == 0 || user_start > canonical.len() {
            return Err(InjectedProfileError::EmptyProfile);
        }
        for (index, input) in canonical.iter_mut().enumerate().skip(user_start) {
            input.origin = CompilationOrigin::User(crate::source::OriginalModuleOrdinal::new(
                index - user_start,
            ));
        }
    }
    let allocators = (0..canonical.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed_and_claims = allocators
        .iter()
        .zip(&canonical)
        .map(|(allocator, input)| {
            let source_type = if input.kind.is_declaration() {
                SourceType::d_ts()
            } else {
                SourceType::ts()
            };
            let parsed = Parser::new(allocator, input.source, source_type).parse();
            if parsed.panicked {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: if parsed.diagnostics.is_empty() {
                        vec!["parser panicked without a diagnostic".to_owned()]
                    } else {
                        parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect()
                    },
                });
            }
            let mut claims = Vec::with_capacity(parsed.diagnostics.len());
            for diagnostic in &parsed.diagnostics {
                let code_is_ts1319 = diagnostic.code.scope.as_deref() == Some("TS")
                    && diagnostic.code.number.as_deref() == Some("1319");
                let [label] = diagnostic.labels.as_slice() else {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                };
                let start = label.offset();
                let Some(end) = start.checked_add(label.len()) else {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                };
                if !code_is_ts1319 {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                }
                claims.push(ParserExportClaim {
                    file_ordinal: input.file_ordinal,
                    span: Span::new(start, end),
                });
            }
            Ok((parsed, claims))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut parser_returns = Vec::with_capacity(parsed_and_claims.len());
    let mut claims = Vec::with_capacity(parsed_and_claims.len());
    for (parsed_unit, claims_unit) in parsed_and_claims {
        parser_returns.push(parsed_unit);
        claims.push(claims_unit);
    }
    let parser_export_claims = claims.into_iter().flatten().collect::<Vec<_>>();
    let parsed = parser_returns
        .iter()
        .map(|parsed| &parsed.program)
        .collect::<Vec<_>>();
    let declaration_manifests = parsed
        .iter()
        .map(|program| SourceDeclarationManifest::build(program))
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    let parse_elapsed = parse_started.elapsed();

    #[cfg(any(test, feature = "test-utils"))]
    let bind_started = Instant::now();
    let units = parsed
        .iter()
        .zip(&canonical)
        .map(|(program, input)| {
            (
                *program,
                CompilationUnit {
                    source: input.source_key,
                    origin: input.origin,
                    binding: ModuleBindingContext::for_program(program, input.kind),
                },
            )
        })
        .collect::<Vec<_>>();
    let (user_units, user_events, combined_lexical_events) = if let Some(user_start) = user_start {
        let mut user_units = Vec::with_capacity(units.len().saturating_sub(user_start));
        let mut user_events = EventStore::default();
        let mut reservations: LexicalReservations<PrivateCombinedRecordTicket> =
            LexicalReservations::default();
        for (user_index, ((program, unit), manifest)) in units
            .iter()
            .zip(&declaration_manifests)
            .skip(user_start)
            .enumerate()
        {
            let module_ordinal = ModuleOrdinal::new(user_index);
            let unit_slot = UnitSlot::new(user_index);
            reservations
                .reserve_private_continuation_program_with_declaration_manifest(
                    module_ordinal,
                    unit_slot,
                    program,
                    manifest,
                    unit.binding,
                    &mut user_events,
                )
                .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;
            user_units.push((module_ordinal, unit_slot));
        }
        (user_units, user_events, Some(reservations))
    } else {
        (Vec::new(), EventStore::default(), None)
    };
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
    let mut builder =
        ProjectBinderBuilder::new_with_declaration_manifest(&prelude.program, &prelude_manifest);
    let library_count = user_start.unwrap_or(units.len());
    let library_units_with_manifests = units[..library_count]
        .iter()
        .zip(&declaration_manifests[..library_count])
        .map(|((program, unit), manifest)| (*program, manifest, *unit))
        .collect::<Vec<_>>();
    let mut module_scopes = builder
        .try_add_library_modules_with_declaration_manifests(&library_units_with_manifests)
        .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
    let mut library_binder = builder.finish(module_scopes.last().copied().unwrap_or(ScopeId(0)));
    let collision_root_provenance =
        library_binder
            .take_library_root_projection()
            .ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "library binder did not retain its one-pass root projection".to_owned(),
                )
            })?;
    let library_units = canonical[..library_count]
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|(input, module)| LibraryBinderUnit {
            ordinal: input.file_ordinal,
            source: input.source_key,
            module,
        })
        .collect();
    let checkpoint = build_library_binder_checkpoint(library_binder, library_units);
    let (mut builder, _) = checkpoint.into_continuation();
    let mut appended_module = None;
    let mut module_placeholders = Vec::with_capacity(units.len().saturating_sub(library_count));
    if library_count < units.len() {
        builder.reserve_script_namespace_roots(units[library_count..].iter().copied());
        for ((program, unit), manifest) in units[library_count..]
            .iter()
            .zip(&declaration_manifests[library_count..])
        {
            let (module, placeholders) =
                builder.add_module_with_declaration_manifest(program, manifest, &[], *unit);
            module_scopes.push(module);
            module_placeholders.push(placeholders);
            appended_module = Some(module);
        }
    }
    let binder = builder
        .finish_frozen_library_continuation(appended_module)
        .map_err(|error| InjectedProfileError::Binder(error.to_owned()))?;
    validate_parser_export_claims(&binder, parser_export_claims, canonical[0].file_ordinal)?;
    let semantic_scopes = units
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|((_, unit), module)| {
            if matches!(unit.origin, CompilationOrigin::User(_)) || unit.binding.external_module {
                module
            } else {
                binder.compilation_global
            }
        })
        .collect::<Vec<_>>();
    let prelude_source = binder
        .source_for_module(binder.prelude_module)
        .ok_or_else(|| {
            InjectedProfileError::Binder("prelude module has no source identity".to_owned())
        })?;
    let _prelude_inventory = super::decls::inventory::BoundTypeDeclarationInventory::build(
        &binder,
        binder.prelude_module,
        prelude_source,
        prelude_manifest,
        None,
    )
    .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
    #[cfg(any(test, feature = "test-utils"))]
    let bind_elapsed = bind_started.elapsed();
    #[cfg(any(test, feature = "test-utils"))]
    {
        CANONICAL_FRONTEND_ENTRIES.set(CANONICAL_FRONTEND_ENTRIES.get().saturating_add(1));
        CANONICAL_FRONTEND_PARSE_UNITS.set(
            CANONICAL_FRONTEND_PARSE_UNITS
                .get()
                .saturating_add(u64::try_from(parsed.len()).unwrap_or(u64::MAX)),
        );
        CANONICAL_FRONTEND_BIND_BATCHES
            .set(CANONICAL_FRONTEND_BIND_BATCHES.get().saturating_add(1));
        CANONICAL_FRONTEND_BIND_UNITS.set(
            CANONICAL_FRONTEND_BIND_UNITS
                .get()
                .saturating_add(u64::try_from(module_scopes.len()).unwrap_or(u64::MAX)),
        );
    }
    #[cfg(any(test, feature = "test-utils"))]
    assert_eq!(
        manifest_build_scope.finish(),
        u64::try_from(canonical.len().saturating_add(1)).unwrap_or(u64::MAX),
        "canonical frontend builds one manifest per source and prelude"
    );
    consume(CanonicalLibraryFrontend {
        canonical,
        parsed,
        declaration_manifests,
        binder,
        module_scopes,
        semantic_scopes,
        user_start,
        user_units,
        user_events,
        combined_lexical_events,
        external_effects: BTreeMap::new(),
        module_placeholders,
        collision_root_provenance,
        #[cfg(any(test, feature = "test-utils"))]
        parse_elapsed,
        #[cfg(any(test, feature = "test-utils"))]
        bind_elapsed,
    })
}
