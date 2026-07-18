//! Measurement-only compiler for injected declaration-library profiles.

use super::context::DeclTypes;
use super::events_library::{
    LibraryEventKey, LibraryEventLedger, LibraryEventLedgerError, LibraryRecordTicket,
    LibrarySemanticReportingAdapter,
};
use super::lexical_events::LexicalReservations;
use super::lexical_events_library::{library_unit, ExactUnit};
#[cfg(test)]
use super::library_reporting::LibraryReportingFamily;
use super::library_reporting::{LibraryReportingConsumer, LibraryReportingReceipt};
use super::namespace_values::NamespaceValueRegistry;
use super::reporting_record::CheckerRecord;
use super::type_groups::{
    PublishedTypeEnvironment, PublishedTypeGroupSurface, PublishedTypeGroupTerminal,
};
use super::{
    build_pass_with_tickets, finish_semantic_effects, reserve_type_decls, PassReporting,
    PassReportingPlan,
};
use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::{TypeGroupId, ValueStorageId};
use crate::binder::namespace::{
    exact_key, CompilationUnit, ExactKey, ExportContextKind, ExportSyntaxDisposition,
    MergeDeclarationKind, ModuleBindingContext, SourceFileKind,
};
use crate::binder::scope::ScopeId;
use crate::binder::Binder;
use crate::class_semantics::DemandOutcome;
use crate::diagnostics::render_type;
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use crate::span::Span;
use crate::types::repr::TypeTag;
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

#[derive(Copy, Clone, Debug)]
pub(crate) struct InjectedLibrarySource<'source> {
    pub(crate) file_ordinal: LibraryFileOrdinal,
    pub(crate) name: &'source str,
    pub(crate) source: &'source str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InjectedProfileError {
    EmptyProfile,
    EmptyName {
        file_ordinal: LibraryFileOrdinal,
    },
    DuplicateName(String),
    DuplicateFileOrdinal(LibraryFileOrdinal),
    SourceKeyOverflow,
    Parse {
        file_ordinal: LibraryFileOrdinal,
        messages: Vec<String>,
    },
    Reservation(String),
    Reporting(LibraryEventLedgerError),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryPhaseCounts {
    pub(crate) parse_units: usize,
    pub(crate) bind_units: usize,
    pub(crate) reserved_records: usize,
    pub(crate) filled_records: usize,
    pub(crate) publication_validations: usize,
    pub(crate) statement_check_units: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryPhaseTimings {
    pub(crate) parse: Duration,
    pub(crate) bind: Duration,
    pub(crate) reserve_fill: Duration,
    pub(crate) publication_validation: Duration,
    pub(crate) statement_check: Duration,
}

impl LibraryPhaseTimings {
    fn measured_total(&self) -> Duration {
        self.parse
            + self.bind
            + self.reserve_fill
            + self.publication_validation
            + self.statement_check
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeProbe {
    pub(crate) identity: TypeGroupId,
    pub(crate) declaration_identities: Vec<(LibraryFileOrdinal, TypeGroupId)>,
    pub(crate) declaration_count: usize,
    pub(crate) member_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SignatureProbe {
    pub(crate) parameter_types: Vec<String>,
    pub(crate) return_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallableMemberProbe {
    pub(crate) name: String,
    pub(crate) identity: ValueStorageId,
    pub(crate) source: ExactUnit,
    pub(crate) reservation_source: ExactUnit,
    pub(crate) source_start: u32,
    pub(crate) call_signature_count: usize,
    pub(crate) signatures: Vec<SignatureProbe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValueProbe {
    pub(crate) identity: ValueStorageId,
    pub(crate) participant_identities: Vec<(LibraryFileOrdinal, ValueStorageId)>,
    pub(crate) declaration_count: usize,
    pub(crate) call_signature_count: usize,
    pub(crate) member_names: Vec<String>,
    pub(crate) callable_members: Vec<CallableMemberProbe>,
}

#[derive(Debug)]
pub(crate) struct InjectedProfileRun {
    pub(crate) phase_counts: LibraryPhaseCounts,
    pub(crate) phase_timings: LibraryPhaseTimings,
    pub(crate) reserved_file_ordinals: Vec<LibraryFileOrdinal>,
    pub(crate) reporting_receipts: Vec<LibraryReportingReceipt>,
    pub(crate) library_records: Vec<(LibraryEventKey, CheckerRecord)>,
    pub(crate) pass_source_units: Vec<ExactUnit>,
    pub(crate) lexical_source_units: Vec<ExactUnit>,
    global_types: BTreeMap<String, TypeProbe>,
    module_types: BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
    global_values: BTreeMap<String, ValueProbe>,
}

impl InjectedProfileRun {
    pub(crate) fn global_type_probe(&self, name: &str) -> Option<&TypeProbe> {
        self.global_types.get(name)
    }

    pub(crate) fn module_type_probe(
        &self,
        file_ordinal: LibraryFileOrdinal,
        name: &str,
    ) -> Option<&TypeProbe> {
        self.module_types.get(&(file_ordinal, name.to_owned()))
    }

    pub(crate) fn global_value_probe(&self, name: &str) -> Option<&ValueProbe> {
        self.global_values.get(name)
    }
}

struct CanonicalInput<'source> {
    file_ordinal: LibraryFileOrdinal,
    source: &'source str,
    kind: SourceFileKind,
    source_key: ExactKey,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ParserExportClaim {
    file_ordinal: LibraryFileOrdinal,
    span: Span,
}

pub(crate) fn run_injected_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<InjectedProfileRun, InjectedProfileError> {
    let parse_started = Instant::now();
    let canonical = canonical_inputs(sources)?;
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
    let (parsed, claims): (Vec<_>, Vec<_>) = parsed_and_claims.into_iter().unzip();
    let parser_export_claims = claims.into_iter().flatten().collect::<Vec<_>>();
    let parse_elapsed = parse_started.elapsed();

    let bind_started = Instant::now();
    let units = parsed
        .iter()
        .zip(&canonical)
        .map(|(parsed, input)| {
            (
                &parsed.program,
                CompilationUnit {
                    source: input.source_key,
                    origin: CompilationOrigin::Library(input.file_ordinal),
                    binding: ModuleBindingContext::for_program(&parsed.program, input.kind),
                },
            )
        })
        .collect::<Vec<_>>();
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let module_scopes = builder.add_library_modules(&units);
    let binder = builder.finish(module_scopes.last().copied().unwrap_or(ScopeId(0)));
    validate_parser_export_claims(&binder, parser_export_claims, canonical[0].file_ordinal)?;
    let semantic_scopes = units
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|((_, unit), module)| {
            if unit.binding.external_module {
                module
            } else {
                binder.compilation_global
            }
        })
        .collect::<Vec<_>>();
    let bind_elapsed = bind_started.elapsed();

    super::wu0c_attribution::register_wu0c_family_tokens(&binder);
    let reserve_fill_started = Instant::now();
    let mut ledger = LibraryEventLedger::default();
    let mut lexical_events: LexicalReservations<LibraryRecordTicket> =
        LexicalReservations::default();
    for (input, parsed) in canonical.iter().zip(&parsed) {
        lexical_events
            .reserve_library_program(input.file_ordinal, &parsed.program, &mut ledger)
            .map_err(InjectedProfileError::Reporting)?;
    }

    let mut interner = Interner::with_intrinsics();
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    let mut type_decls = Vec::new();
    let mut type_resolved = vec![None; binder.type_groups.len()];
    for ((input, parsed), scope) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
    {
        reserve_type_decls(
            &mut interner,
            &binder,
            scope,
            &parsed.program,
            &mut next_type_param,
            &mut next_class_id,
            &mut type_decls,
            &mut type_resolved,
        );
        lexical_events.attach_library_declaration_owners(
            input.file_ordinal,
            &binder,
            scope,
            &parsed.program,
        );
        lexical_events.attach_library_class_bindings(
            input.file_ordinal,
            &binder,
            scope,
            &parsed.program,
            &type_decls,
        );
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;

    let pending_tickets = lexical_events.library_semantic_tickets();
    let mut pass = build_pass_with_tickets(
        &mut interner,
        &binder,
        type_decls,
        type_resolved,
        DeclTypes::new(binder.decl_count),
        next_type_param,
        PassReportingPlan {
            reporting: PassReporting {
                source: library_unit(canonical[0].file_ordinal),
                lexical_events,
                suppress_effects: false,
            },
            pending_tickets,
        },
    );

    let declaration_count = pass.type_decls.len();
    pass.fill_type_decls_range(binder.module, 0, declaration_count);
    let reserve_fill_elapsed = reserve_fill_started.elapsed();

    super::wu0c_attribution::enter_wu0c_phase(
        super::wu0c_attribution::AttributionPhase::PublicationValidation,
    );
    let publication_validation_started = Instant::now();
    let module_programs = module_scopes
        .iter()
        .copied()
        .zip(parsed.iter())
        .map(|(scope, parsed)| (scope, parsed.program.body.as_slice()))
        .collect::<Vec<_>>();
    pass.prepare_project_attached_namespace_values(&module_programs);
    pass.prepare_project_standalone_namespace_values(&module_programs);
    pass.publish_class_surfaces();
    pass.finalize_standalone_namespace_values();
    pass.precompute_standalone_namespace_value_aliases(&module_programs);
    pass.fill_pending_interfaces_range(binder.module, 0, declaration_count);
    let publication_validations = pass.publish_type_groups();
    pass.validate_published_class_surfaces();
    let (global_types, module_types) = collect_type_probes(
        &binder,
        pass.type_environment.published(),
        pass.interner.store(),
        &canonical,
        &module_scopes,
    );
    let lexical_source_units = pass
        .lexical_events
        .library_lexical_evidence()
        .iter()
        .copied()
        .collect::<Vec<_>>();
    let publication_validation_elapsed = publication_validation_started.elapsed();

    super::wu0c_attribution::enter_wu0c_phase(
        super::wu0c_attribution::AttributionPhase::StatementCheck,
    );
    let statement_check_started = Instant::now();
    let mut pass_source_units = Vec::with_capacity(canonical.len());
    for (((input, parsed), module), semantic_scope) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(semantic_scopes.iter().copied())
    {
        pass.current_module = module;
        pass.current_source = library_unit(input.file_ordinal);
        pass_source_units.push(pass.current_source);
        pass.build_flow_graph(semantic_scope, &parsed.program.body);
        pass.check_statements(semantic_scope, &parsed.program.body);
    }
    let global_values = collect_value_probes(
        &binder,
        &pass.decl_types,
        pass.interner.store(),
        &pass.namespace_values,
    );

    let batches = finish_semantic_effects(&mut pass);
    LibrarySemanticReportingAdapter::new(&mut ledger)
        .complete_semantic_batches(batches)
        .map_err(InjectedProfileError::Reporting)?;
    let reporting_receipts = LibraryReportingConsumer::new(&mut ledger)
        .consume_binder_outcomes(&binder)
        .map_err(InjectedProfileError::Reporting)?;
    let snapshot = ledger.snapshot();
    let library_records = ledger.finish().map_err(InjectedProfileError::Reporting)?;
    let statement_check_elapsed = statement_check_started.elapsed();

    Ok(InjectedProfileRun {
        phase_counts: LibraryPhaseCounts {
            parse_units: parsed.len(),
            bind_units: module_scopes.len(),
            reserved_records: snapshot.reserved_records,
            filled_records: snapshot.filled_records,
            publication_validations,
            statement_check_units: pass_source_units.len(),
        },
        phase_timings: LibraryPhaseTimings {
            parse: parse_elapsed,
            bind: bind_elapsed,
            reserve_fill: reserve_fill_elapsed,
            publication_validation: publication_validation_elapsed,
            statement_check: statement_check_elapsed,
        },
        reserved_file_ordinals: snapshot.reserved_file_ordinals,
        reporting_receipts,
        library_records,
        pass_source_units,
        lexical_source_units,
        global_types,
        module_types,
        global_values,
    })
}

fn validate_parser_export_claims(
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

fn match_parser_export_claims(
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

fn canonical_inputs<'source>(
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
            })
        })
        .collect()
}

fn source_file_kind(name: &str) -> SourceFileKind {
    if name.ends_with(".d.mts") {
        SourceFileKind::DeclarationMts
    } else if name.ends_with(".d.cts") {
        SourceFileKind::DeclarationCts
    } else if name.ends_with(".d.ts") {
        SourceFileKind::DeclarationTs
    } else if name.ends_with(".mts") {
        SourceFileKind::ImplementationMts
    } else if name.ends_with(".cts") {
        SourceFileKind::ImplementationCts
    } else {
        SourceFileKind::ImplementationTs
    }
}

fn collect_type_probes(
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

fn type_probe(
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
                PublishedTypeGroupSurface::Template(ty) => Some(ty),
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
                .map(|property| property.name.clone())
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

fn collect_value_probes(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<LibraryRecordTicket>,
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
        let Some(identity) = symbol.value else {
            continue;
        };
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
        let member_names = visible
            .and_then(|ty| store.object_type(ty))
            .map(|object| {
                object
                    .properties
                    .iter()
                    .map(|property| property.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let callable_members = binder
            .namespace_value_attachment(binder.compilation_global, &symbol.name)
            .map(|attachment| {
                attachment
                    .members
                    .into_iter()
                    .filter(|member| member.kind == MergeDeclarationKind::Function)
                    .filter_map(|member| {
                        let member_identity = member.value_storage?;
                        let file_ordinal = library_ordinal(member.origin)?;
                        let reservation =
                            namespace_values.namespace_function_reservation(member.declaration)?;
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
                            source: library_unit(file_ordinal),
                            reservation_source: reservation.unit,
                            source_start: member.site.declaration_span.start,
                            call_signature_count: signature_ids.len(),
                            signatures,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        probes.insert(
            symbol.name.clone(),
            ValueProbe {
                identity,
                participant_identities,
                declaration_count: symbol.declarations.len(),
                call_signature_count,
                member_names,
                callable_members,
            },
        );
    }
    probes
}

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

fn origin_for_module(binder: &Binder, module: ScopeId) -> Option<LibraryFileOrdinal> {
    binder
        .namespaces
        .source_units()
        .find(|unit| unit.module == module)
        .and_then(|unit| library_ordinal(unit.origin))
}

fn library_ordinal(origin: CompilationOrigin) -> Option<LibraryFileOrdinal> {
    match origin {
        CompilationOrigin::Library(file_ordinal) => Some(file_ordinal),
        CompilationOrigin::User(_) => None,
    }
}

fn release_phase_line(
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

struct ReleaseOutcomeLine {
    process: usize,
    file_count: usize,
    reserved_records: usize,
    filled_records: usize,
    publication_validations: usize,
    library_diagnostics: usize,
    library_incompletes: usize,
    tiny_parse_errors: usize,
    tiny_diagnostics: usize,
    tiny_incompletes: usize,
}

impl ReleaseOutcomeLine {
    fn render(&self) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::checker::wu0b_profile::load_strict_profile;
    use crate::driver::check_source;

    fn assert_owned_terminal<T: Send + Sync + 'static>() {}

    const TINY_SOURCE: &str = "export const typokatWu0bProbe: number = 1;\n";

    #[test]
    fn injected_results_are_ast_free_owned_terminals() {
        assert_owned_terminal::<InjectedProfileRun>();
        assert_owned_terminal::<InjectedProfileError>();
        assert_owned_terminal::<LibraryPhaseTimings>();
    }

    #[test]
    fn phase_timings_are_real_nonoverlapping_measurements() {
        let started = Instant::now();
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "timing.d.ts",
            source: "interface TimingWitness { value: number; }",
        }])
        .expect("focused timing profile");
        let external = started.elapsed();
        assert!(run.phase_timings.measured_total() <= external);
        assert_eq!(run.phase_counts.parse_units, 1);
        assert_eq!(run.phase_counts.bind_units, 1);
        assert_eq!(run.phase_counts.statement_check_units, 1);
    }

    #[test]
    fn release_probe_lines_have_the_exact_v1_contract() {
        let timings = LibraryPhaseTimings {
            parse: Duration::from_micros(2),
            bind: Duration::from_micros(3),
            reserve_fill: Duration::from_micros(5),
            publication_validation: Duration::from_micros(7),
            statement_check: Duration::from_micros(11),
        };
        assert_eq!(
            release_phase_line(
                4,
                Duration::from_micros(1),
                &timings,
                Duration::from_micros(31),
            ),
            "typokat-wu0b-phase-v1 process=4 registry_validation_us=1 parse_us=2 bind_us=3 reserve_fill_us=5 publication_validation_us=7 statement_check_us=11 total_us=31"
        );
        assert_eq!(
            ReleaseOutcomeLine {
                process: 4,
                file_count: 82,
                reserved_records: 13,
                filled_records: 13,
                publication_validations: 9,
                library_diagnostics: 2,
                library_incompletes: 3,
                tiny_parse_errors: 0,
                tiny_diagnostics: 0,
                tiny_incompletes: 0,
            }
            .render(),
            "typokat-wu0b-outcome-v1 process=4 file_count=82 reserved_records=13 filled_records=13 publication_validations=9 library_diagnostics=2 library_incompletes=3 tiny_parse_errors=0 tiny_diagnostics=0 tiny_incompletes=0"
        );
    }

    #[test]
    #[ignore = "release-only WU0B cold-process measurement"]
    fn wu0b_release_probe_once() {
        let process = std::env::var("TYPOKAT_WU0B_PROCESS")
            .expect("TYPOKAT_WU0B_PROCESS must identify release process 1..5")
            .parse::<usize>()
            .expect("TYPOKAT_WU0B_PROCESS must be an integer in 1..5");
        assert!(
            (1..=5).contains(&process),
            "TYPOKAT_WU0B_PROCESS must be in 1..5"
        );

        let total_started = Instant::now();
        let registry_started = Instant::now();
        let profile = load_strict_profile().expect("strict WU0B registry validation");
        let registry_validation = registry_started.elapsed();
        let injected = profile.injected_sources();
        let _wu0c_attribution = super::super::wu0c_attribution::start_wu0c_attribution_from_env();
        let run = run_injected_profile(&injected).expect("exact WU0B profile execution");

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

        let tiny = check_source(TINY_SOURCE);
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

    #[test]
    fn recoverable_parser_diagnostic_fails_closed_before_profile_execution() {
        let file_ordinal = LibraryFileOrdinal::new(3);
        let source = "declare namespace Broken { export = Broken; }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "witness must be recoverable");
        assert_eq!(parsed.diagnostics.len(), 1, "witness must diagnose once");
        assert_eq!(parsed.diagnostics[0].code.scope.as_deref(), Some("TS"));
        assert_eq!(parsed.diagnostics[0].code.number.as_deref(), Some("1063"));
        let result = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "recoverable.ts",
            source,
        }]);
        let Err(InjectedProfileError::Parse {
            file_ordinal: actual,
            messages,
        }) = result
        else {
            panic!("recoverable parser diagnostics must abort the injected run");
        };
        assert_eq!(actual, file_ordinal);
        assert!(!messages.is_empty());
    }

    #[test]
    fn focused_shared_interface_identity_and_surface() {
        let run = run_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(1),
                name: "first.d.ts",
                source: "interface Shared { first: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(2),
                name: "second.d.ts",
                source: "interface Shared { second: string; }",
            },
        ])
        .expect("focused injected profile");
        let shared = run.global_type_probe("Shared").expect("shared type probe");
        assert_eq!(shared.declaration_count, 2);
        assert_eq!(shared.member_names, ["first", "second"]);
        assert_eq!(run.phase_counts.parse_units, 2);
        assert_eq!(run.phase_counts.bind_units, 2);
        assert_eq!(run.phase_counts.statement_check_units, 2);
        assert_eq!(
            run.phase_counts.reserved_records,
            run.phase_counts.filled_records
        );
        assert_eq!(run.phase_counts.publication_validations, 1);
        assert_eq!(
            run.reserved_file_ordinals,
            [LibraryFileOrdinal::new(1), LibraryFileOrdinal::new(2)]
        );
        assert!(run.reporting_receipts.is_empty());
        assert!(run.library_records.is_empty());
    }

    #[test]
    fn focused_implementation_diagnostic_keeps_exact_library_key() {
        let file_ordinal = LibraryFileOrdinal::new(7);
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "broken.ts",
            source: "const broken: number = 'wrong';",
        }])
        .expect("focused injected profile");
        assert_eq!(run.pass_source_units, [library_unit(file_ordinal)]);
        assert!(!run.lexical_source_units.is_empty());
        assert_eq!(run.library_records.len(), 1);
        assert_eq!(run.library_records[0].0.file_ordinal, file_ordinal);
        let CheckerRecord::Diagnostic(diagnostic) = &run.library_records[0].1 else {
            panic!("implementation mismatch must be a diagnostic");
        };
        assert_eq!(run.library_records[0].0.source_start, diagnostic.span.start);
    }

    #[test]
    fn focused_function_namespace_and_module_private_probes() {
        let script = LibraryFileOrdinal::new(10);
        let module = LibraryFileOrdinal::new(11);
        let run = run_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: script,
                name: "function.d.ts",
                source: "declare function Merged(value: number): string; declare namespace Merged { export function member(value: string): number; } interface Shared { script: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: module,
                name: "module.d.ts",
                source: "export {}; interface Private { local: boolean; } declare global { interface Shared { module: string; } }",
            },
        ])
        .expect("focused injected profile");

        let merged = run.global_value_probe("Merged").expect("merged value");
        assert_eq!(merged.declaration_count, 2);
        assert_eq!(merged.call_signature_count, 1, "{merged:?}");
        assert_eq!(merged.member_names, ["member"], "{merged:?}");
        assert_eq!(merged.callable_members.len(), 1);
        assert_eq!(merged.callable_members[0].signatures.len(), 1);
        assert!(run.global_type_probe("Private").is_none());
        assert!(run.module_type_probe(module, "Private").is_some());
        assert_eq!(
            run.global_type_probe("Shared")
                .expect("augmented global")
                .declaration_count,
            2
        );
    }

    #[test]
    fn focused_typed_ts1319_claim_is_reported_once_by_binder_consumer() {
        let file_ordinal = LibraryFileOrdinal::new(14);
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "export-context.d.ts",
            source: "declare namespace Exported { export default function f(): void; }",
        }])
        .expect("typed TS1319 claim must transfer to binder reporting");
        let receipt = run
            .reporting_receipts
            .iter()
            .find(|receipt| receipt.family == LibraryReportingFamily::ExportContext)
            .expect("export-context receipt");
        assert_eq!(receipt.file_ordinal, file_ordinal);
        assert_eq!(receipt.observed_outcomes, 1);
        assert_eq!(receipt.emitted_records, 1);
        assert_eq!(run.library_records.len(), 1);
        let (key, record) = &run.library_records[0];
        assert_eq!(key.file_ordinal, file_ordinal);
        assert_eq!(key.source_start, 29);
        let CheckerRecord::Incomplete(incomplete) = record else {
            panic!("TK1319 reporting must remain an incomplete record");
        };
        assert_eq!(incomplete.id, "library/export-context/future-tk1319");
        assert_eq!(
            incomplete.context,
            "library export-context TK1319 reporting is deferred beyond WU0B"
        );
        assert_eq!(incomplete.span, Span::new(29, 63));
        assert_eq!(key.source_start, incomplete.span.start);
    }

    #[test]
    fn mixed_ts1319_and_ts1063_diagnostics_fail_closed() {
        let file_ordinal = LibraryFileOrdinal::new(15);
        let source =
            "declare namespace Mixed { export default function f(): void; export = Mixed; }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "witness must be recoverable");
        let codes = parsed
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code.scope.as_deref(),
                    diagnostic.code.number.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert!(codes.contains(&(Some("TS"), Some("1319"))), "{codes:?}");
        assert!(codes.contains(&(Some("TS"), Some("1063"))), "{codes:?}");
        let result = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "mixed.ts",
            source,
        }]);
        assert!(matches!(
            result,
            Err(InjectedProfileError::Parse {
                file_ordinal: actual,
                ..
            }) if actual == file_ordinal
        ));
    }

    #[test]
    fn parser_export_claim_inventory_rejects_duplicates_and_unmatched_binder_claims() {
        let claim = ParserExportClaim {
            file_ordinal: LibraryFileOrdinal::new(16),
            span: Span::new(4, 12),
        };
        assert!(matches!(
            match_parser_export_claims(vec![claim, claim], vec![claim]),
            Err(InjectedProfileError::Parse { .. })
        ));
        assert!(matches!(
            match_parser_export_claims(Vec::new(), vec![claim]),
            Err(InjectedProfileError::Parse { .. })
        ));
    }

    #[test]
    fn focused_identical_callable_offsets_keep_exact_owners_and_signatures() {
        let first = LibraryFileOrdinal::new(34);
        let second = LibraryFileOrdinal::new(35);
        let sources = [
            InjectedLibrarySource {
                file_ordinal: first,
                name: "first.d.ts",
                source: "declare namespace OffsetCallable { export function alpha(value: number): string; }\ndeclare function OffsetCallable(): void;",
            },
            InjectedLibrarySource {
                file_ordinal: second,
                name: "second.d.ts",
                source: "declare namespace OffsetCallable { export function bravo(value: string): number; }",
            },
        ];
        let reversed = [sources[1], sources[0]];

        for run in [
            run_injected_profile(&sources).expect("forward injected profile"),
            run_injected_profile(&reversed).expect("reverse injected profile"),
        ] {
            let merged = run
                .global_value_probe("OffsetCallable")
                .expect("merged callable");
            let mut members = merged.callable_members.clone();
            members.sort_by(|left, right| left.name.cmp(&right.name));
            assert_eq!(members.len(), 2);
            assert_ne!(members[0].identity, members[1].identity);
            for (member, name, file, parameter, result) in [
                (&members[0], "alpha", first, "number", "string"),
                (&members[1], "bravo", second, "string", "number"),
            ] {
                assert_eq!(member.name, name);
                assert_eq!(member.source, library_unit(file));
                assert_eq!(member.reservation_source, library_unit(file));
                assert_eq!(member.source_start, 42);
                assert_eq!(member.call_signature_count, 1);
                assert_eq!(member.signatures.len(), 1);
                assert_eq!(member.signatures[0].parameter_types, [parameter]);
                assert_eq!(member.signatures[0].return_type, result);
            }
        }
    }

    #[test]
    fn focused_function_namespace_merges_are_input_order_independent() {
        let sources = [
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(30),
                name: "function.d.ts",
                source: "declare function FunctionFirst(value: number): string;",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(31),
                name: "function-namespace.d.ts",
                source: "declare namespace FunctionFirst { export const tag: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(32),
                name: "namespace.d.ts",
                source: "declare namespace NamespaceFirst { export const tag: string; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(33),
                name: "namespace-function.d.ts",
                source: "declare function NamespaceFirst(value: string): number;",
            },
        ];
        let reversed = [sources[3], sources[2], sources[1], sources[0]];
        for run in [
            run_injected_profile(&sources).expect("forward injected profile"),
            run_injected_profile(&reversed).expect("reverse injected profile"),
        ] {
            for name in ["FunctionFirst", "NamespaceFirst"] {
                let merged = run.global_value_probe(name).expect("merged value");
                assert_eq!(merged.declaration_count, 2);
                assert_eq!(merged.call_signature_count, 1);
                assert_eq!(merged.member_names, ["tag"]);
            }
            assert!(run.library_records.is_empty());
        }
    }

    #[test]
    fn focused_parse_clean_binder_reporting_families_have_real_receipts() {
        for (index, name, source, family) in [
            (
                10,
                "alias.d.ts",
                "declare namespace AliasOutput { interface Local {} export { A as B }; export { Local as A }; }",
                LibraryReportingFamily::LocalAmbientExportAliasFailure,
            ),
            (
                11,
                "placement.ts",
                "namespace Late { export const value = 1; } function Late(): void {}",
                LibraryReportingFamily::PlacementIssue,
            ),
            (
                12,
                "global.d.ts",
                "declare global { interface InvalidScriptGlobal {} }",
                LibraryReportingFamily::GlobalAugmentation,
            ),
            (
                13,
                "umd.d.ts",
                "export as namespace ScriptUmd;",
                LibraryReportingFamily::UmdExportContext,
            ),
            (
                15,
                "member.d.ts",
                "declare namespace MemberRoot { const value: number; }",
                LibraryReportingFamily::NamespaceMember,
            ),
            (
                16,
                "standalone.d.ts",
                "declare namespace StandaloneRoot { const value: number; }",
                LibraryReportingFamily::StandaloneNamespaceValueMember,
            ),
        ] {
            let file_ordinal = LibraryFileOrdinal::new(index);
            let run = run_injected_profile(&[InjectedLibrarySource {
                file_ordinal,
                name,
                source,
            }])
            .expect("binder reporting profile");
            let receipt = run
                .reporting_receipts
                .iter()
                .find(|receipt| receipt.family == family)
                .expect("family receipt");
            assert_eq!(receipt.file_ordinal, file_ordinal);
            assert_eq!(receipt.observed_outcomes, 1);
        }
    }
}
