use super::*;

pub(super) fn validate_parser_export_claims(
    binder: &Binder,
    parser_claims: Vec<ParserExportClaim>,
    fallback_file_ordinal: LibraryFileOrdinal,
) -> Result<(), InjectedProfileError> {
    let mut binder_claims = Vec::new();
    for context in binder.namespaces.export_contexts() {
        if context.syntax != ExportSyntaxDisposition::FutureTk1319 {
            continue;
        }
        let CompilationOrigin::Library(file_ordinal) = context.origin else {
            return Err(InjectedProfileError::Parse {
                file_ordinal: fallback_file_ordinal,
                messages: vec!["binder produced an unowned TK1319 export context".to_owned()],
            });
        };
        if context.kind != ExportContextKind::ExportDefault {
            return Err(InjectedProfileError::Parse {
                file_ordinal,
                messages: vec!["binder produced a non-default TK1319 export context".to_owned()],
            });
        }
        binder_claims.push(ParserExportClaim {
            file_ordinal,
            span: context.span,
        });
    }
    match_parser_export_claims(parser_claims, binder_claims)
}

pub(super) fn match_parser_export_claims(
    mut parser_claims: Vec<ParserExportClaim>,
    binder_claims: Vec<ParserExportClaim>,
) -> Result<(), InjectedProfileError> {
    for binder_claim in binder_claims {
        let Some(index) = parser_claims
            .iter()
            .position(|parser_claim| *parser_claim == binder_claim)
        else {
            return Err(InjectedProfileError::Parse {
                file_ordinal: binder_claim.file_ordinal,
                messages: vec![format!(
                    "binder TK1319 claim has no parser owner at {}..{}",
                    binder_claim.span.start, binder_claim.span.end
                )],
            });
        };
        parser_claims.remove(index);
    }
    if let Some(parser_claim) = parser_claims.first() {
        return Err(InjectedProfileError::Parse {
            file_ordinal: parser_claim.file_ordinal,
            messages: vec![format!(
                "parser TS1319 claim has no binder owner at {}..{}",
                parser_claim.span.start, parser_claim.span.end
            )],
        });
    }
    Ok(())
}

pub(super) fn canonical_inputs<'source>(
    sources: &[InjectedLibrarySource<'source>],
) -> Result<Vec<CanonicalInput<'source>>, InjectedProfileError> {
    if sources.is_empty() {
        return Err(InjectedProfileError::EmptyProfile);
    }
    let mut names = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    for source in sources {
        if source.name.is_empty() {
            return Err(InjectedProfileError::EmptyName {
                file_ordinal: source.file_ordinal,
            });
        }
        if !names.insert(source.name) {
            return Err(InjectedProfileError::DuplicateName(source.name.to_owned()));
        }
        if !ordinals.insert(source.file_ordinal) {
            return Err(InjectedProfileError::DuplicateFileOrdinal(
                source.file_ordinal,
            ));
        }
    }
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_by_key(|source| source.file_ordinal);
    sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            let source_key = u32::try_from(index + 1)
                .map(exact_key)
                .map_err(|_| InjectedProfileError::SourceKeyOverflow)?;
            Ok(CanonicalInput {
                file_ordinal: source.file_ordinal,
                source: source.source,
                kind: source_file_kind(source.name),
                source_key,
                origin: CompilationOrigin::Library(source.file_ordinal),
            })
        })
        .collect()
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn collect_type_probes(
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    store: &Store,
    canonical: &[CanonicalInput<'_>],
    module_scopes: &[ScopeId],
) -> (
    BTreeMap<String, TypeProbe>,
    BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
) {
    let mut globals = BTreeMap::new();
    let mut modules = BTreeMap::new();
    for group in binder.type_groups.iter() {
        let probe = type_probe(binder, published, store, group.id);
        let global_symbol = binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&group.name))
            .and_then(|symbol| binder.symbols.get(symbol));
        if global_symbol.is_some_and(|symbol| symbol.ty == Some(group.id)) {
            globals.insert(group.name.clone(), probe.clone());
        }
        for (input, scope) in canonical.iter().zip(module_scopes) {
            let local_symbol = binder
                .graph
                .get(*scope)
                .and_then(|scope| scope.lookup_local(&group.name))
                .and_then(|symbol| binder.symbols.get(symbol));
            if local_symbol.is_some_and(|symbol| symbol.ty == Some(group.id)) {
                modules.insert((input.file_ordinal, group.name.clone()), probe.clone());
            }
        }
    }
    (globals, modules)
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn type_probe(
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    store: &Store,
    identity: TypeGroupId,
) -> TypeProbe {
    let group = binder
        .type_groups
        .get(identity)
        .expect("published type probe has a binder group");
    let declaration_identities = group
        .fragments
        .iter()
        .filter_map(|fragment| {
            library_ordinal(
                binder
                    .namespaces
                    .compilation_origin_for_source(fragment.source)?,
            )
            .map(|file_ordinal| (file_ordinal, identity))
        })
        .collect::<Vec<_>>();
    let member_names = published
        .groups()
        .get(identity)
        .and_then(|terminal| match terminal {
            PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                PublishedTypeGroupSurface::Template(ty) | PublishedTypeGroupSurface::Enum(ty) => {
                    Some(ty)
                }
                PublishedTypeGroupSurface::Class(class) => {
                    match published.classes().published_class(class) {
                        DemandOutcome::Ready(surface) => Some(surface.instance_template()),
                        DemandOutcome::Exhausted(_) => None,
                    }
                }
            },
            PublishedTypeGroupTerminal::Unavailable(_) => None,
        })
        .and_then(|ty| store.object_type(ty))
        .map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.key.to_string())
                .collect()
        })
        .unwrap_or_default();
    TypeProbe {
        identity,
        declaration_identities,
        declaration_count: group.fragments.len(),
        member_names,
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn collect_value_probes<Ticket: Copy>(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<Ticket>,
) -> BTreeMap<String, ValueProbe> {
    let mut probes = BTreeMap::new();
    for (symbol_id, symbol) in binder.symbols.iter() {
        if binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&symbol.name))
            != Some(symbol_id)
        {
            continue;
        }
        if let Some(probe) = value_probe_for_symbol(
            binder,
            decl_types,
            store,
            namespace_values,
            binder.compilation_global,
            symbol_id,
        ) {
            probes.insert(symbol.name.clone(), probe);
        }
    }
    probes
}

#[cfg(any(test, feature = "test-utils"))]
fn value_probe_for_symbol<Ticket: Copy>(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<Ticket>,
    owner_scope: ScopeId,
    symbol_id: SymbolId,
) -> Option<ValueProbe> {
    let symbol = binder.symbols.get(symbol_id)?;
    let identity = symbol.value?;
    let participant_identities = symbol
        .declarations
        .iter()
        .filter_map(|declaration| {
            let declaration = binder.declarations.get(*declaration)?;
            origin_for_module(binder, declaration.site.module)
                .map(|file_ordinal| (file_ordinal, identity))
        })
        .collect::<Vec<_>>();
    let visible = decl_types.get(identity);
    let call_signature_count = visible
        .map(|ty| signature_ids(store, ty).len())
        .unwrap_or_default();
    let signatures = visible
        .map(|ty| {
            signature_ids(store, ty)
                .into_iter()
                .filter_map(|signature| signature_probe(store, signature))
                .collect()
        })
        .unwrap_or_default();
    let member_names = visible
        .and_then(|ty| store.object_type(ty))
        .map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.key.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let callable_members = binder
        .namespace_value_attachment(owner_scope, &symbol.name)
        .map(|attachment| {
            attachment
                .members
                .into_iter()
                .filter(|member| member.kind == MergeDeclarationKind::Function)
                .filter_map(|member| {
                    let member_identity = member.value_storage?;
                    let reservation =
                        namespace_values.namespace_function_reservation(member.declaration)?;
                    let source = match member.origin {
                        CompilationOrigin::Library(file_ordinal) => library_unit(file_ordinal),
                        CompilationOrigin::User(_) => reservation.unit,
                    };
                    let property_ty = visible
                        .and_then(|ty| store.object_type(ty))
                        .and_then(|object| object.property(member.name))
                        .map(|property| property.ty)?;
                    let signature_ids = signature_ids(store, property_ty);
                    let signatures = signature_ids
                        .iter()
                        .filter_map(|signature| signature_probe(store, *signature))
                        .collect::<Vec<_>>();
                    Some(CallableMemberProbe {
                        name: member.name.to_owned(),
                        identity: member_identity,
                        source,
                        reservation_source: reservation.unit,
                        source_start: member.site.declaration_span.start,
                        call_signature_count: signature_ids.len(),
                        signatures,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(ValueProbe {
        identity,
        visible_type: visible,
        participant_identities,
        declaration_count: symbol.declarations.len(),
        call_signature_count,
        signatures,
        member_names,
        callable_members,
    })
}

#[cfg(any(test, feature = "test-utils"))]
fn signature_ids(store: &Store, ty: TypeId) -> Vec<TypeId> {
    match store.tag(ty) {
        TypeTag::Function => vec![ty],
        TypeTag::Object => store
            .object_type(ty)
            .map(|object| object.call_signatures.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn signature_probe(store: &Store, ty: TypeId) -> Option<SignatureProbe> {
    let signature = store.function_type(ty)?;
    Some(SignatureProbe {
        parameter_types: signature
            .params
            .iter()
            .map(|parameter| render_type(store, parameter.ty, false))
            .collect(),
        return_type: render_type(store, signature.ret, false),
    })
}

#[cfg(any(test, feature = "test-utils"))]
fn origin_for_module(binder: &Binder, module: ScopeId) -> Option<LibraryFileOrdinal> {
    binder
        .namespaces
        .source_units()
        .find(|unit| unit.module == module)
        .and_then(|unit| library_ordinal(unit.origin))
}

#[cfg(any(test, feature = "test-utils"))]
fn library_ordinal(origin: CompilationOrigin) -> Option<LibraryFileOrdinal> {
    match origin {
        CompilationOrigin::Library(file_ordinal) => Some(file_ordinal),
        CompilationOrigin::User(_) => None,
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn release_phase_line(
    process: usize,
    registry_validation: Duration,
    timings: &LibraryPhaseTimings,
    total: Duration,
) -> String {
    format!(
        "typokat-wu0b-phase-v1 process={process} registry_validation_us={} parse_us={} bind_us={} reserve_fill_us={} publication_validation_us={} statement_check_us={} total_us={}",
        registry_validation.as_micros(),
        timings.parse.as_micros(),
        timings.bind.as_micros(),
        timings.reserve_fill.as_micros(),
        timings.publication_validation.as_micros(),
        timings.statement_check.as_micros(),
        total.as_micros(),
    )
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) struct ReleaseOutcomeLine {
    pub(super) process: usize,
    pub(super) file_count: usize,
    pub(super) reserved_records: usize,
    pub(super) filled_records: usize,
    pub(super) publication_validations: usize,
    pub(super) library_diagnostics: usize,
    pub(super) library_incompletes: usize,
    pub(super) tiny_parse_errors: usize,
    pub(super) tiny_diagnostics: usize,
    pub(super) tiny_incompletes: usize,
}

#[cfg(any(test, feature = "test-utils"))]
impl ReleaseOutcomeLine {
    pub(super) fn render(&self) -> String {
        format!(
            "typokat-wu0b-outcome-v1 process={} file_count={} reserved_records={} filled_records={} publication_validations={} library_diagnostics={} library_incompletes={} tiny_parse_errors={} tiny_diagnostics={} tiny_incompletes={}",
            self.process,
            self.file_count,
            self.reserved_records,
            self.filled_records,
            self.publication_validations,
            self.library_diagnostics,
            self.library_incompletes,
            self.tiny_parse_errors,
            self.tiny_diagnostics,
            self.tiny_incompletes,
        )
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn assert_exact_profile_interner_has_no_pending_reservations(
    injected: &[InjectedLibrarySource<'_>],
) {
    let (_, state) =
        compile_owned_injected_profile(injected).expect("source-compiled full-library profile");
    state
        .interner
        .strict_terminal_state_for_test()
        .expect("full profile must close every reserved type");
}

#[cfg(any(test, feature = "test-utils"))]
pub fn assert_exact_profile_replay_index_is_complete_and_deterministic(
    injected: &[InjectedLibrarySource<'_>],
) {
    let started = Instant::now();
    let (_, first_state) =
        compile_owned_injected_profile(injected).expect("first exact replay index generation");
    let first_elapsed = started.elapsed();
    let started = Instant::now();
    let (_, second_state) =
        compile_owned_injected_profile(injected).expect("second exact replay index generation");
    let second_elapsed = started.elapsed();
    let first = first_state
        .replay_index()
        .expect("source compiler retains its replay index");
    let second = second_state
        .replay_index()
        .expect("source compiler retains its replay index");
    assert_eq!(
        first.canonical_manifest_len(),
        second.canonical_manifest_len()
    );
    assert_eq!(
        first.canonical_manifest_sha256,
        second.canonical_manifest_sha256
    );
    assert_eq!(first.unowned_demand_count, 0);
    assert_eq!(first.invalid_owner_site_count, 0);
    assert_eq!(first.noncanonical_edge_count, 0);
    assert_eq!(first.typed_reference_coverage_misses, 0);
    let digest = first
        .canonical_manifest_sha256
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    eprintln!(
        "replay-index owners={} roots={} sites={} edges={} root_edges={} sccs={} statements={} baselines={} bytes={} sha256={} first={:?} second={:?}",
        first.owner_partition.len(),
        first.root_slots.len(),
        first.owner_sites.len(),
        first.reverse_edges.len(),
        first.root_slot_consumers.len(),
        first.scc_membership.len(),
        first.statement_owners.len(),
        first.baseline_records.len(),
        first.canonical_manifest_len(),
        digest,
        first_elapsed,
        second_elapsed,
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub fn assert_exact_profile_selects_complete_native_bridge_identities(
    injected: &[InjectedLibrarySource<'_>],
) {
    let run = run_injected_profile(injected).expect("source-compiled full-library profile");
    let identities = run.semantic_identities();
    assert!(identities.all_ready());
    assert_eq!(
        identities.callable_function_group(),
        run.global_type_probe("CallableFunction")
            .map(|probe| probe.identity)
    );
    assert_ne!(
        identities.callable_function_group(),
        run.global_type_probe("Function")
            .map(|probe| probe.identity)
    );
    let expected_arities = [1, 1, 0, 0, 0, 0, 0, 0];
    for (terminal, expected_arity) in identities.terminals().into_iter().zip(expected_arities) {
        let super::library_identities::LibraryIdentityTerminal::Ready(identity) = terminal else {
            panic!("exact profile native bridge identity must be ready")
        };
        assert_eq!(identity.parameters.len(), expected_arity);
        assert_ne!(identity.template, TypeId(0));
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn run_library_release_probe(
    injected: &[InjectedLibrarySource<'_>],
    registry_validation: Duration,
    total_started: Instant,
) {
    const TINY_SOURCE: &str = "export const typokatLibraryProbe: number = 1;\n";

    let process = std::env::var("TYPOKAT_WU0B_PROCESS")
        .expect("TYPOKAT_WU0B_PROCESS must identify release process 1..5")
        .parse::<usize>()
        .expect("TYPOKAT_WU0B_PROCESS must be an integer in 1..5");
    assert!(
        (1..=5).contains(&process),
        "TYPOKAT_WU0B_PROCESS must be in 1..5"
    );

    let run = run_injected_profile(injected).expect("exact library profile execution");
    assert_eq!(run.phase_counts.parse_units, 82);
    assert_eq!(run.phase_counts.bind_units, 82);
    assert_eq!(run.phase_counts.statement_check_units, 82);
    assert!(run.phase_counts.reserved_records > 0);
    assert_eq!(
        run.phase_counts.reserved_records,
        run.phase_counts.filled_records
    );
    assert!(run.phase_counts.publication_validations > 0);
    let expected = (0..82)
        .map(LibraryFileOrdinal::new)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        run.reserved_file_ordinals
            .iter()
            .copied()
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert!(run
        .library_records
        .iter()
        .all(|(key, _)| expected.contains(&key.file_ordinal)));
    assert!(run
        .reporting_receipts
        .iter()
        .all(|receipt| expected.contains(&receipt.file_ordinal)));

    let tiny = crate::check::test_support::check_source(TINY_SOURCE);
    assert!(tiny.parse_errors.is_empty(), "{:?}", tiny.parse_errors);
    assert!(tiny.diagnostics.is_empty(), "{:?}", tiny.diagnostics);
    assert!(tiny.incomplete.is_empty(), "{:?}", tiny.incomplete);
    let library_diagnostics = run
        .library_records
        .iter()
        .filter(|(_, record)| matches!(record, CheckerRecord::Diagnostic(_)))
        .count();
    let library_incompletes = run.library_records.len() - library_diagnostics;
    let total = total_started.elapsed();
    assert!(
        registry_validation + run.phase_timings.measured_total() <= total,
        "external total must cover every measured phase"
    );

    println!(
        "{}",
        release_phase_line(process, registry_validation, &run.phase_timings, total)
    );
    println!(
        "{}",
        ReleaseOutcomeLine {
            process,
            file_count: injected.len(),
            reserved_records: run.phase_counts.reserved_records,
            filled_records: run.phase_counts.filled_records,
            publication_validations: run.phase_counts.publication_validations,
            library_diagnostics,
            library_incompletes,
            tiny_parse_errors: tiny.parse_errors.len(),
            tiny_diagnostics: tiny.diagnostics.len(),
            tiny_incompletes: tiny.incomplete.len(),
        }
        .render()
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub fn assert_exact_full_profile_owned_base_checks_caller_certified_suffix(
    injected: &[InjectedLibrarySource<'_>],
) {
    let (compiled, state) =
        compile_owned_injected_profile(injected).expect("exact source-compiled owned library");
    eprintln!(
        "owned-base compile timings: parse={:?} bind={:?} reserve_fill={:?} publication={:?} statements={:?} total={:?}",
        compiled.phase_timings.parse,
        compiled.phase_timings.bind,
        compiled.phase_timings.reserve_fill,
        compiled.phase_timings.publication_validation,
        compiled.phase_timings.statement_check,
        compiled.phase_timings.measured_total(),
    );
    let source = concat!(
        include_str!("../../../../../../tooling/full-lib-bench/workloads/fast-clean/main.ts"),
        "\nconst directDomProbe: HTMLDivElement = document.createElement(\"div\");\n",
    );
    let run = check_caller_certified_collision_free_source_with_owned_library(state, source)
        .expect("exact owned base accepts the WU0A suffix");
    eprintln!(
        "owned-base user timings: parse={:?} bind={:?} check={:?}",
        run.timings.parse, run.timings.bind, run.timings.check
    );

    let (_, state) = compile_owned_injected_profile(injected)
        .expect("second exact source-compiled owned library");
    let focused = check_caller_certified_collision_free_source_with_owned_library(
        state,
        r#"
            const mutableBad: string[] = [1].map(value => value);
            const readonlyValues: readonly number[] = [1, 2];
            const readonlyBad: string[] = readonlyValues.map(value => value);
            readonlyValues.push(3);
            const upperBad: number = "x".toUpperCase();
            const fixedBad: number = (1).toFixed();
            const booleanBad: string = true.valueOf();
            const calledBad: number = ((value: string) => value).call(undefined, "x");
            const objectBad: number = ({ value: 1 }).toString();
            const regexpBad: string = /x/.test("x");
            const domBad: number = document.createElement("div");
        "#,
    )
    .expect("exact owned base accepts the focused semantic suffix");
    eprintln!(
        "owned-base focused timings: parse={:?} bind={:?} check={:?}",
        focused.timings.parse, focused.timings.bind, focused.timings.check
    );
    assert_eq!(
        focused
            .result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2339,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
        ]
    );
    assert!(focused.result.incomplete.is_empty());
    assert_eq!(focused.witness.store_prefix_stable, None);
    assert_eq!(run.witness.store_prefix_stable, None);
    assert!(
        run.result.incomplete.is_empty(),
        "{:?}",
        run.result.incomplete
    );
    assert!(
        run.result.diagnostics.is_empty(),
        "{:?}",
        run.result.diagnostics
    );
}
