//! Test-only binder outcome reporting for the injected library prototype.

use super::events_library::{LibraryEventLedger, LibraryEventLedgerError};
use super::reporting_record::CheckerRecord;
use crate::binder::namespace::{
    ExportResolutionDisposition, ExportSyntaxDisposition, GlobalIssue,
    LocalAmbientExportAliasFailureKind, PlacementIssueKind, UmdContext,
};
use crate::binder::Binder;
use crate::diagnostics::{Diagnostic, IncompleteSurface};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use std::collections::BTreeMap;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LibraryReportingFamily {
    LocalAmbientExportAliasFailure,
    PlacementIssue,
    GlobalAugmentation,
    UmdExportContext,
    ExportContext,
    NamespaceMember,
    StandaloneNamespaceValueMember,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LibraryReportingReceipt {
    pub(crate) family: LibraryReportingFamily,
    pub(crate) file_ordinal: LibraryFileOrdinal,
    pub(crate) observed_outcomes: usize,
    pub(crate) emitted_records: usize,
}

pub(crate) struct LibraryReportingConsumer<'ledger> {
    ledger: &'ledger mut LibraryEventLedger,
}

impl<'ledger> LibraryReportingConsumer<'ledger> {
    pub(crate) fn new(ledger: &'ledger mut LibraryEventLedger) -> Self {
        Self { ledger }
    }

    pub(crate) fn consume_binder_outcomes(
        &mut self,
        binder: &Binder,
    ) -> Result<Vec<LibraryReportingReceipt>, LibraryEventLedgerError> {
        let mut receipts = BTreeMap::new();

        for failure in binder.local_ambient_export_alias_failures() {
            let diagnostic = match failure.kind {
                LocalAmbientExportAliasFailureKind::Missing => {
                    Diagnostic::cannot_find_name(failure.local_span, &failure.local_name)
                }
                LocalAmbientExportAliasFailureKind::NonLocal => {
                    Diagnostic::cannot_export_non_local(failure.local_span, &failure.local_name)
                }
            };
            self.complete_outcome(
                &mut receipts,
                LibraryReportingFamily::LocalAmbientExportAliasFailure,
                failure.origin,
                failure.local_span.start,
                vec![CheckerRecord::Diagnostic(diagnostic)],
            )?;
        }

        for issue in binder.namespaces.placement_issues() {
            let diagnostic = match issue.kind {
                PlacementIssueKind::FutureTk2434 => {
                    Diagnostic::namespace_precedes_class_or_function(issue.span)
                }
            };
            self.complete_outcome(
                &mut receipts,
                LibraryReportingFamily::PlacementIssue,
                issue.origin,
                issue.span.start,
                vec![CheckerRecord::Diagnostic(diagnostic)],
            )?;
        }

        for global in binder.namespaces.globals() {
            let records = global
                .issues
                .iter()
                .map(|issue| {
                    CheckerRecord::Diagnostic(match issue {
                        GlobalIssue::FutureTk2669 => {
                            Diagnostic::global_augmentation_requires_module(global.diagnostic_span)
                        }
                        GlobalIssue::FutureTk2670 => {
                            Diagnostic::global_augmentation_requires_declare(global.diagnostic_span)
                        }
                    })
                })
                .collect();
            self.complete_outcome(
                &mut receipts,
                LibraryReportingFamily::GlobalAugmentation,
                global.origin,
                global.diagnostic_span.start,
                records,
            )?;
        }

        for export in binder.namespaces.umd_exports() {
            let records = match export.context {
                UmdContext::FutureTk1314NonExternal => vec![CheckerRecord::Diagnostic(
                    Diagnostic::global_module_export_requires_module(export.span),
                )],
                UmdContext::FutureTk1315Implementation => vec![CheckerRecord::Diagnostic(
                    Diagnostic::global_module_export_requires_declaration_file(export.span),
                )],
                UmdContext::FutureTk1316Nested | UmdContext::DeferredValidBacklog15 => Vec::new(),
            };
            self.complete_outcome(
                &mut receipts,
                LibraryReportingFamily::UmdExportContext,
                export.origin,
                export.span.start,
                records,
            )?;
        }

        for context in binder.namespaces.export_contexts() {
            let records = match context.syntax {
                ExportSyntaxDisposition::FutureTk1319 => {
                    vec![CheckerRecord::Incomplete(IncompleteSurface::new(
                        "library/export-context/future-tk1319",
                        context.span,
                        "library export-context TK1319 reporting is deferred beyond WU0B",
                    ))]
                }
                ExportSyntaxDisposition::Valid
                | ExportSyntaxDisposition::FutureTk1194
                | ExportSyntaxDisposition::FutureTk1063
                | ExportSyntaxDisposition::FutureTk2666 => Vec::new(),
            };
            match context.resolution {
                ExportResolutionDisposition::NotRequired
                | ExportResolutionDisposition::DeferredBacklog15 => {}
            }
            self.complete_outcome(
                &mut receipts,
                LibraryReportingFamily::ExportContext,
                context.origin,
                context.span.start,
                records,
            )?;
        }

        let mut members = binder.namespaces.members().collect::<Vec<_>>();
        members.sort_by_key(|member| (member.origin, member.declaration_span.start, member.id));
        for member in members {
            self.complete_outcome(
                &mut receipts,
                LibraryReportingFamily::NamespaceMember,
                member.origin,
                member.declaration_span.start,
                Vec::new(),
            )?;
        }

        let mut standalone_members = binder
            .standalone_namespace_value_attachments()
            .into_iter()
            .flat_map(|attachment| attachment.members)
            .collect::<Vec<_>>();
        standalone_members
            .sort_by_key(|member| (member.origin, member.declaration_span.start, member.member));
        for member in standalone_members {
            self.complete_outcome(
                &mut receipts,
                LibraryReportingFamily::StandaloneNamespaceValueMember,
                member.origin,
                member.declaration_span.start,
                Vec::new(),
            )?;
        }

        Ok(receipts.into_values().collect())
    }

    fn complete_outcome(
        &mut self,
        receipts: &mut BTreeMap<
            (LibraryFileOrdinal, LibraryReportingFamily),
            LibraryReportingReceipt,
        >,
        family: LibraryReportingFamily,
        origin: CompilationOrigin,
        source_start: u32,
        records: Vec<CheckerRecord>,
    ) -> Result<(), LibraryEventLedgerError> {
        let file_ordinal = match origin {
            CompilationOrigin::Library(file_ordinal) => file_ordinal,
            CompilationOrigin::User(_) => return Ok(()),
        };
        let emitted_records = records.len();
        let event = self.ledger.reserve_event(file_ordinal, source_start);
        self.ledger.complete(event.primary, records)?;
        let receipt = receipts
            .entry((file_ordinal, family))
            .or_insert(LibraryReportingReceipt {
                family,
                file_ordinal,
                observed_outcomes: 0,
                emitted_records: 0,
            });
        receipt.observed_outcomes += 1;
        receipt.emitted_records += emitted_records;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::bind::ProjectBinderBuilder;
    use crate::binder::namespace::{
        CompilationUnit, ModuleBindingContext, SourceFileKind, SourceUnitKey,
    };
    use crate::diagnostics::DiagnosticCode;
    use crate::source::OriginalModuleOrdinal;
    use crate::span::Span;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    struct LibrarySource<'a> {
        file_ordinal: LibraryFileOrdinal,
        source: &'a str,
        kind: SourceFileKind,
    }

    type LibraryRun = (
        Vec<LibraryReportingReceipt>,
        Vec<(super::super::events_library::LibraryEventKey, CheckerRecord)>,
    );

    fn bind_library_sources(
        sources: &[LibrarySource<'_>],
    ) -> Result<LibraryRun, LibraryEventLedgerError> {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        assert!(prelude.diagnostics.is_empty());
        let allocators = (0..sources.len())
            .map(|_| Allocator::default())
            .collect::<Vec<_>>();
        let parsed = allocators
            .iter()
            .zip(sources)
            .map(|(allocator, source)| {
                let source_type = if source.kind.is_declaration() {
                    SourceType::d_ts()
                } else {
                    SourceType::ts()
                };
                let parsed = Parser::new(allocator, source.source, source_type).parse();
                assert!(!parsed.panicked);
                parsed
            })
            .collect::<Vec<_>>();
        let units = parsed
            .iter()
            .zip(sources)
            .enumerate()
            .map(|(index, (parsed, source))| {
                let unit = CompilationUnit {
                    source: SourceUnitKey(
                        u32::try_from(index + 1).expect("library source count fits u32"),
                    ),
                    origin: CompilationOrigin::Library(source.file_ordinal),
                    binding: ModuleBindingContext::for_program(&parsed.program, source.kind),
                };
                (&parsed.program, unit)
            })
            .collect::<Vec<_>>();
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let modules = builder.add_library_modules(&units);
        let module = *modules.last().expect("library source batch is non-empty");
        let binder = builder.finish(module);
        let mut ledger = LibraryEventLedger::default();
        let receipts =
            LibraryReportingConsumer::new(&mut ledger).consume_binder_outcomes(&binder)?;
        Ok((receipts, ledger.finish()?))
    }

    fn receipt(
        receipts: &[LibraryReportingReceipt],
        family: LibraryReportingFamily,
    ) -> &LibraryReportingReceipt {
        receipts
            .iter()
            .find(|receipt| receipt.family == family)
            .expect("expected reporting receipt")
    }

    fn record_span(record: &CheckerRecord) -> Span {
        match record {
            CheckerRecord::Diagnostic(diagnostic) => diagnostic.span,
            CheckerRecord::Incomplete(incomplete) => incomplete.span,
        }
    }

    #[test]
    fn seven_committed_representatives_report_exact_family_outcomes() {
        enum ExpectedEmission {
            Diagnostic(DiagnosticCode),
            Incomplete {
                id: &'static str,
                context: &'static str,
            },
            None,
        }

        struct Case {
            file_ordinal: LibraryFileOrdinal,
            source: &'static str,
            kind: SourceFileKind,
            family: LibraryReportingFamily,
            expected: ExpectedEmission,
        }

        let cases = [
            Case {
                file_ordinal: LibraryFileOrdinal::new(10),
                source: "declare namespace AliasOutput { interface Local {} export { A as B }; export { Local as A }; }",
                kind: SourceFileKind::DeclarationTs,
                family: LibraryReportingFamily::LocalAmbientExportAliasFailure,
                expected: ExpectedEmission::Diagnostic(DiagnosticCode::TK2661),
            },
            Case {
                file_ordinal: LibraryFileOrdinal::new(11),
                source: "namespace Late { export const value = 1; } function Late(): void {}",
                kind: SourceFileKind::ImplementationTs,
                family: LibraryReportingFamily::PlacementIssue,
                expected: ExpectedEmission::Diagnostic(DiagnosticCode::TK2434),
            },
            Case {
                file_ordinal: LibraryFileOrdinal::new(12),
                source: "declare global { interface InvalidScriptGlobal {} }",
                kind: SourceFileKind::DeclarationTs,
                family: LibraryReportingFamily::GlobalAugmentation,
                expected: ExpectedEmission::Diagnostic(DiagnosticCode::TK2669),
            },
            Case {
                file_ordinal: LibraryFileOrdinal::new(13),
                source: "export as namespace ScriptUmd;",
                kind: SourceFileKind::DeclarationTs,
                family: LibraryReportingFamily::UmdExportContext,
                expected: ExpectedEmission::Diagnostic(DiagnosticCode::TK1314),
            },
            Case {
                file_ordinal: LibraryFileOrdinal::new(14),
                source: "declare namespace Exported { export default function f(): void; }",
                kind: SourceFileKind::DeclarationTs,
                family: LibraryReportingFamily::ExportContext,
                expected: ExpectedEmission::Incomplete {
                    id: "library/export-context/future-tk1319",
                    context: "library export-context TK1319 reporting is deferred beyond WU0B",
                },
            },
            Case {
                file_ordinal: LibraryFileOrdinal::new(15),
                source: "declare namespace MemberRoot { const value: number; }",
                kind: SourceFileKind::DeclarationTs,
                family: LibraryReportingFamily::NamespaceMember,
                expected: ExpectedEmission::None,
            },
            Case {
                file_ordinal: LibraryFileOrdinal::new(16),
                source: "declare namespace StandaloneRoot { const value: number; }",
                kind: SourceFileKind::DeclarationTs,
                family: LibraryReportingFamily::StandaloneNamespaceValueMember,
                expected: ExpectedEmission::None,
            },
        ];

        for case in cases {
            let (receipts, records) = bind_library_sources(&[LibrarySource {
                file_ordinal: case.file_ordinal,
                source: case.source,
                kind: case.kind,
            }])
            .unwrap();
            let target = receipt(&receipts, case.family);
            assert_eq!(target.file_ordinal, case.file_ordinal);
            assert_eq!(target.observed_outcomes, 1);
            assert!(records.iter().all(|(key, record)| {
                key.file_ordinal == case.file_ordinal
                    && key.source_start == record_span(record).start
            }));

            match case.expected {
                ExpectedEmission::Diagnostic(code) => {
                    assert_eq!(target.emitted_records, 1);
                    assert_eq!(records.len(), 1);
                    assert!(records.iter().any(|(_, record)| matches!(
                        record,
                        CheckerRecord::Diagnostic(diagnostic) if diagnostic.code == code
                    )));
                }
                ExpectedEmission::Incomplete { id, context } => {
                    assert_eq!(target.emitted_records, 1);
                    assert_eq!(records.len(), 1);
                    assert!(records.iter().any(|(_, record)| matches!(
                        record,
                        CheckerRecord::Incomplete(incomplete)
                            if incomplete.id == id && incomplete.context == context
                    )));
                }
                ExpectedEmission::None => {
                    assert_eq!(target.emitted_records, 0);
                    assert!(records.is_empty());
                }
            }
        }
    }

    #[test]
    fn reverse_library_input_keeps_receipts_and_replay_order_stable() {
        let source = "declare namespace AliasOutput { export { Missing as Value }; }";
        let forward = [
            LibrarySource {
                file_ordinal: LibraryFileOrdinal::new(70),
                source,
                kind: SourceFileKind::DeclarationTs,
            },
            LibrarySource {
                file_ordinal: LibraryFileOrdinal::new(71),
                source,
                kind: SourceFileKind::DeclarationTs,
            },
        ];
        let reverse = [
            LibrarySource {
                file_ordinal: LibraryFileOrdinal::new(71),
                source,
                kind: SourceFileKind::DeclarationTs,
            },
            LibrarySource {
                file_ordinal: LibraryFileOrdinal::new(70),
                source,
                kind: SourceFileKind::DeclarationTs,
            },
        ];
        let (forward_receipts, forward_records) = bind_library_sources(&forward).unwrap();
        let (reverse_receipts, reverse_records) = bind_library_sources(&reverse).unwrap();
        assert_eq!(forward_receipts, reverse_receipts);

        let replay =
            |records: &[(super::super::events_library::LibraryEventKey, CheckerRecord)]| {
                records
                    .iter()
                    .map(|(key, record)| {
                        let CheckerRecord::Diagnostic(diagnostic) = record else {
                            panic!("expected diagnostic record")
                        };
                        (
                            key.file_ordinal,
                            key.source_start,
                            key.event_ordinal,
                            key.record_ordinal,
                            diagnostic.code,
                        )
                    })
                    .collect::<Vec<_>>()
            };
        assert_eq!(replay(&forward_records), replay(&reverse_records));
        let missing_start = u32::try_from(source.find("Missing").expect("source contains Missing"))
            .expect("source position fits u32");
        assert_eq!(
            replay(&forward_records),
            [
                (
                    LibraryFileOrdinal::new(70),
                    missing_start,
                    0,
                    0,
                    DiagnosticCode::TK2304,
                ),
                (
                    LibraryFileOrdinal::new(71),
                    missing_start,
                    0,
                    0,
                    DiagnosticCode::TK2304,
                ),
            ]
        );
    }

    #[test]
    fn one_global_row_preserves_both_ordered_diagnostics() {
        let (receipts, records) = bind_library_sources(&[LibrarySource {
            file_ordinal: LibraryFileOrdinal::new(20),
            source: "global { interface ScriptGlobal {} }",
            kind: SourceFileKind::ImplementationTs,
        }])
        .unwrap();
        let target = receipt(&receipts, LibraryReportingFamily::GlobalAugmentation);
        assert_eq!(target.observed_outcomes, 1);
        assert_eq!(target.emitted_records, 2);
        assert!(matches!(
            records.as_slice(),
            [
                (first_key, CheckerRecord::Diagnostic(first)),
                (second_key, CheckerRecord::Diagnostic(second))
            ] if first.code == DiagnosticCode::TK2669
                && second.code == DiagnosticCode::TK2670
                && first_key.file_ordinal == second_key.file_ordinal
                && first_key.source_start == second_key.source_start
                && first_key.event_ordinal == second_key.event_ordinal
        ));
    }

    #[test]
    fn user_origin_outcomes_are_skipped_without_retagging() {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let parsed = Parser::new(
            &source_allocator,
            "declare global { interface InvalidScriptGlobal {} }",
            SourceType::d_ts(),
        )
        .parse();
        let unit = CompilationUnit {
            source: SourceUnitKey::SINGLE_SOURCE,
            origin: CompilationOrigin::User(OriginalModuleOrdinal::new(0)),
            binding: ModuleBindingContext::for_program(
                &parsed.program,
                SourceFileKind::DeclarationTs,
            ),
        };
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let (module, placeholders) = builder.add_module(&parsed.program, &[], unit);
        assert!(placeholders.is_empty());
        let binder = builder.finish(module);
        let globals = binder.namespaces.globals().collect::<Vec<_>>();
        assert_eq!(globals.len(), 1);
        assert_eq!(
            globals[0].origin,
            CompilationOrigin::User(OriginalModuleOrdinal::new(0))
        );
        assert_eq!(globals[0].issues, [GlobalIssue::FutureTk2669]);
        let mut ledger = LibraryEventLedger::default();
        let receipts = LibraryReportingConsumer::new(&mut ledger)
            .consume_binder_outcomes(&binder)
            .unwrap();
        assert!(receipts.is_empty());
        assert!(ledger.finish().unwrap().is_empty());
    }
}
