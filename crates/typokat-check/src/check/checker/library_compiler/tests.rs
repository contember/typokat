use super::*;
use crate::check::checker::type_groups::PublishedTypeGroup;
use typokat_types::alias_semantics::PublishedAliasTerminal;

const DECLARATION_RECEIPT_LIBRARY: [InjectedLibrarySource<'static>; 2] = [
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "declaration-receipt-a.d.ts",
            source: "interface ReceiptMerge { first: string } type ReceiptAlias = { value: string }; declare const ReceiptValue: string; declare function receiptFunction(): void;",
        },
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(1),
            name: "declaration-receipt-b.d.ts",
            source: "interface ReceiptMerge { second: number } declare class ReceiptClass {}",
        },
    ];

fn assert_owned_terminal<T: Send + Sync + 'static>() {}

fn install_explicit_injected_sparse_selection_for_test(
    mut state: OwnedLibraryRuntimeState,
    plan: std::sync::Arc<CollisionReplayPlan>,
    selected: BTreeSet<ReplayOwner>,
    sources: &[InjectedLibrarySource<'_>],
) -> (
    OwnedLibraryRuntimeState,
    Vec<crate::frontend::AuxiliarySourceInput>,
) {
    state
        .freeze_as_library_base()
        .expect("injected sparse fixture freezes");
    let permit = acquire_private_collision_replay_permit().expect("sparse fixture permit");
    let mut state = state
        .fork_sparse_collision_epoch()
        .expect("sparse fixture forks")
        .install_complete_source_replay_plan_with_permit(plan.clone(), &[], permit)
        .expect("sparse fixture installs its retained plan");
    let epoch = state
        .private_collision_epoch
        .as_mut()
        .expect("sparse fixture has an installed epoch");
    epoch.affected_owners = selected;
    epoch.mutation_owners.clear();
    epoch.value_winners.clear();
    epoch.owner_sites = PrivateCollisionEpoch::owner_sites_for(&plan, &epoch.affected_owners);
    epoch.library_record_baselines =
        PrivateCollisionEpoch::baselines_for(&plan, &epoch.affected_owners);
    epoch.complete_source_replay = false;

    let selected_ordinals = state
        .private_collision_source_ordinals()
        .expect("explicit sparse selection has source sites");
    let auxiliary = selected_ordinals
        .into_iter()
        .map(|ordinal| {
            let source = sources
                .iter()
                .find(|source| source.file_ordinal == ordinal)
                .expect("selected ordinal belongs to the injected profile");
            crate::frontend::AuxiliarySourceInput {
                source_ordinal: ordinal.index(),
                name: source.name.to_owned(),
                source: source.source.to_owned(),
            }
        })
        .collect();
    (state, auxiliary)
}

fn run_explicit_injected_sparse_selection_for_test(
    state: OwnedLibraryRuntimeState,
    auxiliary: Vec<crate::frontend::AuxiliarySourceInput>,
    root_names: Vec<String>,
) -> Result<super::super::PrivateProjectSemanticEvidenceForTest, &'static str> {
    let run = crate::frontend::run_project_frontend_with_auxiliary(
        vec![crate::frontend::FileInput {
            name: "/project/sparse-selection.ts".to_owned(),
            source: "export {}; const sparseSelectionControl = 1;".to_owned(),
        }],
        auxiliary,
        move |_, library_programs, units| {
            super::super::check_private_project_programs_with_library_evidence(
                state,
                library_programs,
                units,
                &root_names,
            )
        },
    );
    assert!(run.parse_errors.iter().all(Vec::is_empty));
    run.product
}

#[test]
fn selected_library_terminal_evidence_is_exact_for_a_ready_interface() {
    let sources = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "selected-library-terminal-evidence.d.ts",
        source: "interface SelectedEvidenceInterface { value: string }",
    }];
    let (_, state, plan) = compile_owned_injected_base_profile_with_plan(&sources)
        .expect("selected-library evidence prefix compiles");
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SelectedEvidenceInterface")
        .expect("selected-library evidence group")
        .id;
    let selected = BTreeSet::from([ReplayOwner::TypeGroup(group)]);
    let _selection_scope = ExplicitSparseSelectionScopeForTest::start(selected.clone());
    let (state, auxiliary) =
        install_explicit_injected_sparse_selection_for_test(state, plan, selected, &sources);

    let evidence = run_explicit_injected_sparse_selection_for_test(
        state,
        auxiliary,
        vec!["SelectedEvidenceInterface".to_owned()],
    )
    .expect("selected-library evidence route completes");
    let [row] = evidence.selected_library_terminals.as_slice() else {
        panic!("one exact selected-library terminal row")
    };
    assert_eq!(row.owner, ReplayOwner::TypeGroup(group));
    assert_eq!(row.group, group);
    assert_eq!(row.declarations.len(), 1);
    assert!(row.owner_attached);
    assert_eq!(row.terminal, "ready");
    assert_eq!(row.final_class, None);
}

#[test]
fn sparse_selected_prefix_enum_is_filled_while_unselected_peer_stays_frozen() {
    let sources = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "sparse-enum-selection.d.ts",
        source: r#"
declare enum SelectedPrefixEnum { First, Second }
declare enum UnselectedLocalEnum { Untouched }
"#,
    }];
    let (_, state, plan) = compile_owned_injected_base_profile_with_plan(&sources)
        .expect("sparse enum prefix compiles");
    let selected_group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SelectedPrefixEnum")
        .expect("selected enum group")
        .id;
    let unselected_group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "UnselectedLocalEnum")
        .expect("unselected enum group")
        .id;
    assert_ne!(selected_group, unselected_group);
    let selected = BTreeSet::from([ReplayOwner::TypeGroup(selected_group)]);
    let _selection_scope = ExplicitSparseSelectionScopeForTest::start(selected.clone());
    let (state, auxiliary) =
        install_explicit_injected_sparse_selection_for_test(state, plan, selected, &sources);

    let replay_scope =
        PrivateReplayProductionScopeForTest::start(PrivateReplayProductionFaultForTest::None)
            .expect("sparse enum route trace starts");
    super::super::start_enum_publication_trace_for_test();
    let evidence = run_explicit_injected_sparse_selection_for_test(
        state,
        auxiliary,
        vec![
            "SelectedPrefixEnum".to_owned(),
            "UnselectedLocalEnum".to_owned(),
        ],
    )
    .expect("selected enum sparse route completes");
    let trace = super::super::finish_enum_publication_trace_for_test();
    let replay_trace = replay_scope
        .finish()
        .expect("sparse enum route trace finishes");

    assert!(replay_trace
        .scheduled_owners
        .contains(&ReplayOwner::TypeGroup(selected_group)));
    assert_eq!(trace.reservations, 1);
    assert_eq!(trace.fills, 1);
    assert_eq!(
        trace.enum_group,
        Some(ReplayOwner::TypeGroup(selected_group))
    );
    assert_eq!(trace.ordered_member_names, ["First", "Second"]);
    assert!(trace.readonly_value_properties);
    assert!(!trace.terminal_visible_before_freeze);
    assert!(evidence
        .results
        .iter()
        .all(|result| { result.diagnostics.is_empty() && result.incomplete.is_empty() }));
    assert!(evidence
        .affected_terminals_from_frozen_prefix
        .contains("type:UnselectedLocalEnum"));
    assert!(!evidence
        .affected_terminals_unavailable
        .contains("type:UnselectedLocalEnum"));
    assert!(evidence
        .selected_library_terminals
        .iter()
        .any(|row| row.owner == ReplayOwner::TypeGroup(selected_group)
            && row.group == selected_group
            && row.owner_attached
            && row.terminal == "ready"));
    assert!(evidence
        .selected_library_terminals
        .iter()
        .all(|row| row.group != unselected_group));
}

#[test]
fn sparse_reparsed_source_does_not_replace_an_unselected_enum_prefix() {
    let sources = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "sparse-unselected-enum.d.ts",
        source: r#"
interface SelectedInterface { value: string }
declare enum UnselectedLocalEnum { Untouched }
"#,
    }];
    let (_, state, plan) = compile_owned_injected_base_profile_with_plan(&sources)
        .expect("sparse unselected enum prefix compiles");
    let selected_group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SelectedInterface")
        .expect("selected interface group")
        .id;
    let unselected_group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "UnselectedLocalEnum")
        .expect("unselected enum group")
        .id;
    let selected = BTreeSet::from([ReplayOwner::TypeGroup(selected_group)]);
    let _selection_scope = ExplicitSparseSelectionScopeForTest::start(selected.clone());
    let (state, auxiliary) =
        install_explicit_injected_sparse_selection_for_test(state, plan, selected, &sources);

    super::super::start_enum_publication_trace_for_test();
    let evidence = run_explicit_injected_sparse_selection_for_test(
        state,
        auxiliary,
        vec![
            "SelectedInterface".to_owned(),
            "UnselectedLocalEnum".to_owned(),
        ],
    )
    .expect("unselected enum sparse control completes");
    let trace = super::super::finish_enum_publication_trace_for_test();

    assert_eq!(trace.reservations, 0);
    assert_eq!(trace.fills, 0);
    assert!(evidence
        .affected_terminals_from_frozen_prefix
        .contains("type:UnselectedLocalEnum"));
    assert!(!evidence
        .affected_terminals_unavailable
        .contains("type:UnselectedLocalEnum"));
    assert!(evidence
        .selected_library_terminals
        .iter()
        .any(|row| row.owner == ReplayOwner::TypeGroup(selected_group)
            && row.group == selected_group
            && row.owner_attached
            && row.terminal == "ready"));
    assert!(evidence
        .selected_library_terminals
        .iter()
        .all(|row| row.group != unselected_group));
}

#[test]
fn sparse_class_owner_only_selection_preserves_frozen_class_terminal() {
    let sources = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "sparse-class-selection.d.ts",
        source: r#"
declare class SparseSelectedClass { value: string }
declare class SparseUnselectedClass { value: number }
"#,
    }];
    let (_, state, plan) = compile_owned_injected_base_profile_with_plan(&sources)
        .expect("sparse class prefix compiles");
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SparseSelectedClass")
        .expect("sparse class group")
        .id;
    let class = match state.published_types.groups().get(group) {
        Some(super::super::type_groups::PublishedTypeGroupTerminal::Ready(group)) => {
            match &group.surface {
                super::super::type_groups::PublishedTypeGroupSurface::Class(class) => *class,
                surface => panic!("sparse class has the wrong surface: {surface:?}"),
            }
        }
        terminal => panic!("sparse class prefix is not ready: {terminal:?}"),
    };
    let unselected_group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SparseUnselectedClass")
        .expect("unselected sparse class group")
        .id;
    let unselected_class = match state.published_types.groups().get(unselected_group) {
        Some(super::super::type_groups::PublishedTypeGroupTerminal::Ready(group)) => {
            match &group.surface {
                super::super::type_groups::PublishedTypeGroupSurface::Class(class) => *class,
                surface => panic!("unselected sparse class has the wrong surface: {surface:?}"),
            }
        }
        terminal => panic!("unselected sparse class prefix is not ready: {terminal:?}"),
    };
    let base_next_class_id = state.next_class_id;
    let selected = BTreeSet::from([ReplayOwner::Class(class)]);
    let _selection_scope = ExplicitSparseSelectionScopeForTest::start(selected.clone());
    let (state, auxiliary) =
        install_explicit_injected_sparse_selection_for_test(state, plan, selected, &sources);
    let replay_scope =
        PrivateReplayProductionScopeForTest::start(PrivateReplayProductionFaultForTest::None)
            .expect("class-only sparse route trace starts");
    super::super::decls::reset_class_allocation_events_for_test();

    let evidence = run_explicit_injected_sparse_selection_for_test(
        state,
        auxiliary,
        vec![
            "SparseSelectedClass".to_owned(),
            "SparseUnselectedClass".to_owned(),
        ],
    )
    .expect("class-owner-only sparse route completes");
    let replay_trace = replay_scope
        .finish()
        .expect("class-only route trace finishes");
    let selected_type_owners = replay_trace
        .scheduled_owners
        .iter()
        .filter(|owner| matches!(owner, ReplayOwner::Class(_) | ReplayOwner::TypeGroup(_)))
        .copied()
        .collect::<BTreeSet<_>>();

    assert_eq!(
        selected_type_owners,
        BTreeSet::from([ReplayOwner::Class(class)])
    );
    assert_eq!(
        super::super::decls::class_allocation_events_for_test(),
        [ClassId(base_next_class_id)]
    );
    assert_eq!(
        evidence.final_identity_ends[2],
        usize::try_from(base_next_class_id).expect("class identity fits usize") + 1
    );
    assert!(evidence
        .selected_library_terminals
        .iter()
        .any(|row| row.owner == ReplayOwner::Class(class)
            && row.group == group
            && row.owner_attached
            && row.terminal == "ready"
            && row.final_class == Some(class)));
    assert!(evidence.selected_library_terminals.iter().all(|row| {
        row.group != unselected_group && row.owner != ReplayOwner::Class(unselected_class)
    }));
    assert_eq!(evidence.class_attachments.rows.len(), 2);
    assert!(evidence.class_attachments.rows[1].declaration.is_none());
    assert!(evidence.class_attachments.rows[1].group.is_none());
    assert!(evidence.class_attachments.rows[1].class.is_none());
}

#[test]
fn sparse_type_group_only_class_selection_rebuilds_with_a_fresh_class() {
    let sources = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "sparse-type-group-class-selection.d.ts",
        source: "declare class SparseGroupSelectedClass { value: string }",
    }];
    let (_, state, plan) = compile_owned_injected_base_profile_with_plan(&sources)
        .expect("type-group-only class prefix compiles");
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SparseGroupSelectedClass")
        .expect("type-group-only class group")
        .id;
    let original_class = match state.published_types.groups().get(group) {
        Some(super::super::type_groups::PublishedTypeGroupTerminal::Ready(group)) => {
            match &group.surface {
                super::super::type_groups::PublishedTypeGroupSurface::Class(class) => *class,
                surface => panic!("type-group-only class has the wrong surface: {surface:?}"),
            }
        }
        terminal => panic!("type-group-only class prefix is not ready: {terminal:?}"),
    };
    let base_next_class_id = state.next_class_id;
    let selected = BTreeSet::from([ReplayOwner::TypeGroup(group)]);
    let _selection_scope = ExplicitSparseSelectionScopeForTest::start(selected.clone());
    let (state, auxiliary) =
        install_explicit_injected_sparse_selection_for_test(state, plan, selected, &sources);
    super::super::decls::reset_class_allocation_events_for_test();

    let evidence = run_explicit_injected_sparse_selection_for_test(
        state,
        auxiliary,
        vec!["SparseGroupSelectedClass".to_owned()],
    )
    .expect("type-group-only class replay completes");
    let rebuilt_class = ClassId(base_next_class_id);

    assert_ne!(rebuilt_class, original_class);
    assert_eq!(
        super::super::decls::class_allocation_events_for_test(),
        [rebuilt_class]
    );
    assert!(evidence.selected_library_terminals.iter().any(|row| {
        row.owner == ReplayOwner::TypeGroup(group)
            && row.group == group
            && row.terminal == "ready"
            && row.final_class == Some(rebuilt_class)
    }));
    let [attachment] = evidence.class_attachments.rows.as_slice() else {
        panic!("one type-group-selected class reservation")
    };
    assert_eq!(attachment.group, Some(group));
    assert_eq!(attachment.class, Some(rebuilt_class));
    assert!(attachment.declaration.is_some());
}

#[test]
fn sparse_type_group_selection_precedes_selected_class_preservation() {
    let sources = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "sparse-type-group-and-class-selection.d.ts",
        source: "declare class SparseGroupAndClassSelected { value: string }",
    }];
    let (_, state, plan) = compile_owned_injected_base_profile_with_plan(&sources)
        .expect("type-group-and-class prefix compiles");
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SparseGroupAndClassSelected")
        .expect("type-group-and-class group")
        .id;
    let original_class = match state.published_types.groups().get(group) {
        Some(super::super::type_groups::PublishedTypeGroupTerminal::Ready(group)) => {
            match &group.surface {
                super::super::type_groups::PublishedTypeGroupSurface::Class(class) => *class,
                surface => {
                    panic!("type-group-and-class has the wrong surface: {surface:?}")
                }
            }
        }
        terminal => panic!("type-group-and-class prefix is not ready: {terminal:?}"),
    };
    let base_next_class_id = state.next_class_id;
    let selected = BTreeSet::from([
        ReplayOwner::TypeGroup(group),
        ReplayOwner::Class(original_class),
    ]);
    let _selection_scope = ExplicitSparseSelectionScopeForTest::start(selected.clone());
    let (state, auxiliary) =
        install_explicit_injected_sparse_selection_for_test(state, plan, selected, &sources);
    super::super::decls::reset_class_allocation_events_for_test();

    let evidence = run_explicit_injected_sparse_selection_for_test(
        state,
        auxiliary,
        vec!["SparseGroupAndClassSelected".to_owned()],
    )
    .expect("type-group-and-class replay completes");
    let rebuilt_class = ClassId(base_next_class_id);

    assert_ne!(rebuilt_class, original_class);
    assert_eq!(
        super::super::decls::class_allocation_events_for_test(),
        [rebuilt_class]
    );
    assert!(evidence.selected_library_terminals.iter().any(|row| {
        row.owner == ReplayOwner::TypeGroup(group)
            && row.group == group
            && row.terminal == "ready"
            && row.final_class == Some(rebuilt_class)
    }));
    let [attachment] = evidence.class_attachments.rows.as_slice() else {
        panic!("one type-group-and-class-selected reservation")
    };
    assert_eq!(attachment.group, Some(group));
    assert_eq!(attachment.class, Some(rebuilt_class));
    assert!(attachment.declaration.is_some());
}

#[test]
fn sparse_class_only_merged_group_preserves_all_fragments_and_frozen_class() {
    let sources = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "sparse-class-interface-merge-selection.d.ts",
        source: r#"
interface SparseMergedClass<T> { interfaceValue: T }
declare class SparseMergedClass<T> { classValue: T }
"#,
    }];
    let (_, state, plan) = compile_owned_injected_base_profile_with_plan(&sources)
        .expect("class-interface merged prefix compiles");
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SparseMergedClass")
        .expect("class-interface merged group")
        .id;
    let original_class = match state.published_types.groups().get(group) {
        Some(super::super::type_groups::PublishedTypeGroupTerminal::Ready(group)) => {
            match &group.surface {
                super::super::type_groups::PublishedTypeGroupSurface::Class(class) => *class,
                surface => panic!("merged class has the wrong surface: {surface:?}"),
            }
        }
        terminal => panic!("merged class prefix is not ready: {terminal:?}"),
    };
    let base_next_type_param = state.next_type_param;
    let base_next_class_id = state.next_class_id;
    let selected = BTreeSet::from([ReplayOwner::Class(original_class)]);
    let _selection_scope = ExplicitSparseSelectionScopeForTest::start(selected.clone());
    let (state, auxiliary) =
        install_explicit_injected_sparse_selection_for_test(state, plan, selected, &sources);
    super::super::decls::reset_class_allocation_events_for_test();

    let evidence = run_explicit_injected_sparse_selection_for_test(
        state,
        auxiliary,
        vec!["SparseMergedClass".to_owned()],
    )
    .expect("class-only merged replay completes");

    assert_eq!(
        super::super::decls::class_allocation_events_for_test(),
        [ClassId(base_next_class_id)]
    );
    assert_eq!(
        evidence.final_identity_ends[1],
        usize::try_from(base_next_type_param).expect("type parameter identity fits usize") + 1
    );
    assert!(evidence.selected_library_terminals.iter().any(|row| {
        row.owner == ReplayOwner::Class(original_class)
            && row.group == group
            && row.declarations.len() == 2
            && row.terminal == "ready"
            && row.final_class == Some(original_class)
    }));
}

#[test]
fn sparse_class_only_attachment_keeps_original_identity_and_fresh_header_params() {
    let sources = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "sparse-class-attachment-red.d.ts",
        source: r#"
declare class SparseAttachedClass<T> { value: T }
declare class SparseUnselectedPeer { value: number }
"#,
    }];
    let (_, state, plan) = compile_owned_injected_base_profile_with_plan(&sources)
        .expect("class-only attachment prefix compiles");
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SparseAttachedClass")
        .expect("selected attachment group")
        .id;
    let declaration = state
        .binder
        .type_groups
        .get(group)
        .and_then(|group| {
            group
                .fragments
                .iter()
                .find(|fragment| fragment.kind == TypeFragmentKind::Class)
        })
        .expect("selected attachment class fragment")
        .declaration;
    let original_class = match state.published_types.groups().get(group) {
        Some(PublishedTypeGroupTerminal::Ready(group)) => match &group.surface {
            PublishedTypeGroupSurface::Class(class) => *class,
            surface => panic!("selected attachment surface changed: {surface:?}"),
        },
        terminal => panic!("selected attachment prefix is not ready: {terminal:?}"),
    };
    let fresh_header_param = TypeParamId(state.next_type_param);
    let selected = BTreeSet::from([ReplayOwner::Class(original_class)]);
    let _selection_scope = ExplicitSparseSelectionScopeForTest::start(selected.clone());
    let (state, auxiliary) =
        install_explicit_injected_sparse_selection_for_test(state, plan, selected, &sources);

    let evidence = run_explicit_injected_sparse_selection_for_test(
        state,
        auxiliary,
        vec![
            "SparseAttachedClass".to_owned(),
            "SparseUnselectedPeer".to_owned(),
        ],
    )
    .expect("class-only attachment replay completes");
    assert_eq!(evidence.class_attachments.rows.len(), 2);
    let attached = &evidence.class_attachments.rows[0];
    assert_eq!(attached.declaration, Some(declaration));
    assert_eq!(attached.group, Some(group));
    assert_eq!(attached.class, Some(original_class));
    assert_eq!(attached.header_type_params, [fresh_header_param]);
    let peer = &evidence.class_attachments.rows[1];
    assert!(peer.declaration.is_none());
    assert!(peer.group.is_none());
    assert!(peer.class.is_none());
}

#[test]
fn sparse_class_only_merged_group_attaches_once_to_the_class_decl_id() {
    let sources = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "sparse-class-merge-attachment-red.d.ts",
        source: r#"
interface SparseAttachedMerge<T> { interfaceValue: T }
declare class SparseAttachedMerge<T> { classValue: T }
"#,
    }];
    let (_, state, plan) = compile_owned_injected_base_profile_with_plan(&sources)
        .expect("class-only merge attachment prefix compiles");
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "SparseAttachedMerge")
        .expect("class-only merge attachment group")
        .id;
    let class_declaration = state
        .binder
        .type_groups
        .get(group)
        .into_iter()
        .flat_map(|group| &group.fragments)
        .find(|fragment| fragment.kind == TypeFragmentKind::Class)
        .expect("merged group has one class fragment")
        .declaration;
    let original_class = match state.published_types.groups().get(group) {
        Some(PublishedTypeGroupTerminal::Ready(group)) => match &group.surface {
            PublishedTypeGroupSurface::Class(class) => *class,
            surface => panic!("merged attachment surface changed: {surface:?}"),
        },
        terminal => panic!("merged attachment prefix is not ready: {terminal:?}"),
    };
    let selected = BTreeSet::from([ReplayOwner::Class(original_class)]);
    let _selection_scope = ExplicitSparseSelectionScopeForTest::start(selected.clone());
    let (state, auxiliary) =
        install_explicit_injected_sparse_selection_for_test(state, plan, selected, &sources);

    let evidence = run_explicit_injected_sparse_selection_for_test(
        state,
        auxiliary,
        vec!["SparseAttachedMerge".to_owned()],
    )
    .expect("class-only merged attachment replay completes");
    let [attachment] = evidence.class_attachments.rows.as_slice() else {
        panic!("one class reservation for a class-interface merged group")
    };
    assert_eq!(attachment.declaration, Some(class_declaration));
    assert_eq!(attachment.group, Some(group));
    assert_eq!(attachment.class, Some(original_class));
    assert_eq!(attachment.header_type_params.len(), 1);
}

#[test]
fn frozen_base_receipt_tracks_the_successful_central_freeze() {
    let (_, mut state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "freeze-receipt.d.ts",
        source: "interface FreezeReceipt { value: string }",
    }])
    .expect("freeze receipt fixture compiles");
    let scope = CompleteSourceRouteWorkScopeForTest::start();

    state
        .freeze_as_library_base()
        .expect("runtime state freezes through the central operation");

    let work = scope.finish();
    assert_eq!(work.frozen_base_seals, 1);
    assert_eq!(work.semantic_publications, 0);
    assert_eq!(work.replay_trace_constructions, 0);
    assert_eq!(work.replay_plan_constructions, 0);
}

#[test]
fn semantic_publication_receipt_tracks_the_actual_publish_entry() {
    let scope = CompleteSourceRouteWorkScopeForTest::start();

    compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "publication-receipt.d.ts",
        source: "interface PublicationReceipt { value: string }",
    }])
    .expect("publication receipt fixture compiles");

    let work = scope.finish();
    assert_eq!(work.semantic_publications, 1);
    assert_eq!(work.replay_plan_constructions, 0);
    assert_eq!(work.frozen_base_seals, 0);
}

#[test]
fn legacy_library_only_declaration_receipt_is_exact() {
    let manifest_scope =
        crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest::start();
    with_canonical_library_frontend(&DECLARATION_RECEIPT_LIBRARY, |frontend| {
        let groups = frontend
            .binder
            .type_groups
            .local_groups()
            .collect::<Vec<_>>();
        assert_eq!(
            groups
                .iter()
                .map(|group| (group.name.as_str(), group.fragments.len()))
                .collect::<Vec<_>>(),
            [
                ("ReceiptMerge", 2),
                ("ReceiptAlias", 1),
                ("ReceiptClass", 1),
            ]
        );
        assert_eq!(frontend.module_scopes.len(), 2);
        assert_eq!(
            groups[0].fragments[0].site.module,
            frontend.module_scopes[0]
        );
        assert_eq!(
            groups[0].fragments[1].site.module,
            frontend.module_scopes[1]
        );
        assert!(groups[0].fragments[0].declaration.0 < groups[0].fragments[1].declaration.0);
        Ok(())
    })
    .expect("canonical library frontend receipt");
    assert_eq!(manifest_scope.finish(), 3);

    let manifest_scope =
        crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest::start();
    let inventory_scope = crate::check::checker::DeclarationInventoryWorkScopeForTest::start();
    let (run, state) = compile_owned_injected_profile(&DECLARATION_RECEIPT_LIBRARY)
        .expect("legacy library-only declaration receipt");
    let manifests = manifest_scope.finish();
    let inventory_work = inventory_scope.finish();
    assert_eq!(manifests, 3);
    assert_eq!(inventory_work.builds, manifests);
    assert_eq!(
        inventory_work,
        crate::check::checker::DeclarationInventoryWorkForTest {
            builds: 3,
            source_type_rows: 4,
            admitted: 4,
            rejected: 0,
            exact_join_attempts: 4,
            raw_probe_attempts: 0,
            fragment_checks: 4,
            completeness_checks: 0,
            dense_slots: 4,
        }
    );
    assert_eq!(run.phase_counts.parse_units, 2);
    assert_eq!(run.phase_counts.bind_units, 2);
    assert_eq!(state.binder.type_groups.len(), 3);
    assert_eq!(state.published_types.groups().len(), 3);
    assert_eq!(
        state
            .binder
            .declarations
            .iter()
            .filter(|declaration| declaration.type_group.is_some())
            .count(),
        4
    );
    assert!((0..3).all(|index| state
        .published_types
        .groups()
        .get(TypeGroupId(index))
        .is_some()));
}

#[test]
fn complete_combined_declaration_receipt_preserves_library_prefix_and_user_suffix() {
    let sources = [
            DECLARATION_RECEIPT_LIBRARY[0],
            DECLARATION_RECEIPT_LIBRARY[1],
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(2),
                name: "declaration-receipt-user.ts",
                source: "export {}; interface UserReceiptInterface { value: string } type UserReceiptAlias = { value: string }; class UserReceiptClass {} const invalid: UserReceiptInterface = { value: 1 };",
            },
        ];
    let scope = CompleteSourceRouteWorkScopeForTest::start();
    let manifest_scope =
        crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest::start();
    let inventory_scope = crate::check::checker::DeclarationInventoryWorkScopeForTest::start();
    let (run, state) = compile_complete_combined_profile_for_test(&sources, 2)
        .expect("complete combined declaration receipt");
    let work = scope.finish();
    let manifests = manifest_scope.finish();
    let inventory_work = inventory_scope.finish();
    assert_eq!(manifests, 4);
    assert_eq!(inventory_work.builds, manifests);
    assert_eq!(
        inventory_work,
        crate::check::checker::DeclarationInventoryWorkForTest {
            builds: 4,
            source_type_rows: 7,
            admitted: 7,
            rejected: 0,
            exact_join_attempts: 7,
            raw_probe_attempts: 0,
            fragment_checks: 7,
            completeness_checks: 0,
            dense_slots: 7,
        }
    );

    assert_eq!(run.phase_counts.parse_units, 3);
    assert_eq!(run.phase_counts.bind_units, 3);
    assert_eq!(work.semantic_publications, 1);
    assert_eq!(work.replay_plan_constructions, 0);
    assert_eq!(state.binder.type_groups.len(), 6);
    assert_eq!(state.published_types.groups().len(), 6);
    assert_eq!(
        state
            .binder
            .type_groups
            .iter()
            .map(|group| (group.name.as_str(), group.fragments.len()))
            .collect::<Vec<_>>(),
        [
            ("ReceiptMerge", 2),
            ("ReceiptAlias", 1),
            ("ReceiptClass", 1),
            ("UserReceiptInterface", 1),
            ("UserReceiptAlias", 1),
            ("UserReceiptClass", 1),
        ]
    );
    let user_results = run.into_complete_source_user_results_for_test();
    assert_eq!(user_results.len(), 1);
    assert_eq!(user_results[0].diagnostics.len(), 1);
    assert_eq!(user_results[0].diagnostics[0].code.as_str(), "TK2322");
    assert!(user_results[0].incomplete.is_empty());
}

#[test]
fn complete_combined_probe_does_not_publish_product_census_evidence() {
    let sources = [
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "probe-evidence-lib.d.ts",
            source: "interface ProbeEvidence { value: string }",
        },
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(1),
            name: "probe-evidence-user.ts",
            source: "const value: ProbeEvidence = { value: 'ok' };",
        },
    ];
    let scope = CompleteSourceRouteWorkScopeForTest::start();

    let (run, _runtime) = compile_complete_combined_profile_for_test(&sources, 1)
        .expect("complete-combined probe fixture compiles");
    let work = scope.finish();

    assert_eq!(run.phase_counts.parse_units, 2);
    assert_eq!(run.phase_counts.statement_check_units, 2);
    assert_eq!(work.semantic_publications, 1);
    assert_eq!(
        work.library_evidence, None,
        "product census serialization is an explicit suite witness, not probe compile work"
    );
}

#[test]
fn replay_receipts_track_the_central_trace_and_plan_entries() {
    let scope = CompleteSourceRouteWorkScopeForTest::start();

    ReplayDependencyTrace::default()
        .finish_compact_plan(
            Vec::new(),
            Vec::new(),
            [0; 9],
            CollisionReplayConstructionEvidence::default(),
        )
        .expect("empty replay plan is structurally valid");

    let work = scope.finish();
    assert_eq!(work.replay_trace_constructions, 1);
    assert_eq!(work.replay_plan_constructions, 1);
    assert_eq!(work.semantic_publications, 0);
    assert_eq!(work.frozen_base_seals, 0);
}

#[test]
fn complete_source_user_unit_validation_rejects_duplicate_module_ordinals() {
    let duplicate = ModuleOrdinal::new(4);
    let error = validate_complete_source_user_units(
        3,
        Some(1),
        &[(duplicate, UnitSlot::new(0)), (duplicate, UnitSlot::new(1))],
    )
    .expect_err("duplicate user module ordinal must fail closed");

    assert_eq!(
        error,
        InjectedProfileError::CanonicalProjection(
            "complete-source user module ordinal 4 is duplicated".to_owned()
        )
    );
}

#[test]
fn complete_source_result_assembly_rejects_unexpected_module_records() {
    let unexpected = ModuleOrdinal::new(9);
    let error = assemble_complete_source_user_results(
        &[(ModuleOrdinal::new(2), UnitSlot::new(0))],
        BTreeMap::from([(unexpected, Vec::new())]),
    )
    .expect_err("unexpected user module records must fail closed");

    assert_eq!(
        error,
        InjectedProfileError::CanonicalProjection(
            "complete-source retained records for unexpected user module ordinal 9".to_owned()
        )
    );
}

#[test]
fn canonical_property_key_bytes_pin_symbol_discriminants() {
    let cases = [
        (WellKnownSymbol::Iterator, 0),
        (WellKnownSymbol::ToStringTag, 1),
        (WellKnownSymbol::AsyncIterator, 2),
        (WellKnownSymbol::Species, 3),
        (WellKnownSymbol::ToPrimitive, 4),
        (WellKnownSymbol::Replace, 5),
        (WellKnownSymbol::Unscopables, 6),
        (WellKnownSymbol::Split, 7),
        (WellKnownSymbol::Search, 8),
        (WellKnownSymbol::Match, 9),
        (WellKnownSymbol::MatchAll, 10),
        (WellKnownSymbol::HasInstance, 11),
    ];

    for (symbol, discriminant) in cases {
        let mut bytes = CanonicalBytes::domain(b"");
        bytes
            .property_key(&PropertyKey::WellKnownSymbol(symbol))
            .expect("well-known symbol key encodes");
        assert_eq!(bytes.finish(), vec![1, discriminant], "{symbol}");
    }

    let mut string = CanonicalBytes::domain(b"");
    string
        .property_key(&PropertyKey::String("iterator".to_owned()))
        .expect("string property key encodes");
    assert_eq!(
        string.finish(),
        vec![0, 0, 0, 0, 0, 0, 0, 0, 8, b'i', b't', b'e', b'r', b'a', b't', b'o', b'r']
    );
}

fn compile_symbol_product_parts() -> OwnedLibraryRuntimeProductParts {
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "certified-symbol.d.ts",
        source: r#"
                declare const Other: {};
                declare const Symbol: {};
                interface CertifiedSymbolSurface {
                    [Symbol.iterator](): string;
                }
            "#,
    }])
    .expect("certified Symbol library compiles");
    state.into_product_parts().expect("extract Symbol product")
}

#[test]
fn authenticated_symbol_root_is_installed_before_interface_fill() {
    let records = compile_owned_injected_records(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "early-certified-symbol.d.ts",
        source: r#"
                declare const Symbol: {};
                interface CertifiedSymbolSurface {
                    [Symbol.iterator](): string;
                    [Symbol.asyncIterator](): number;
                }
            "#,
    }])
    .expect("certified Symbol library compiles");
    assert!(records.iter().all(|(_, record)| {
        !matches!(
            record,
            CheckerRecord::Incomplete(incomplete)
                if incomplete.id == "signature/method-signature/computed-key"
        )
    }));
}

#[test]
fn symbol_certificate_round_trips_and_rejects_corruption() {
    let parts = compile_symbol_product_parts();
    let certified = parts
        .runtime
        .certified_library_values
        .symbol
        .expect("Symbol root is certified");
    let restored = OwnedLibraryRuntimeState::from_product_parts(parts)
        .expect("certified Symbol product restores");
    assert_eq!(
        restored.runtime.certified_library_values.symbol,
        Some(certified)
    );

    let mut mismatched = compile_symbol_product_parts();
    let other = collect_root_rows(&mismatched.binder)
        .expect("product binder roots project")
        .into_iter()
        .find(|row| row.name == "Other")
        .and_then(|row| row.value)
        .expect("other in-range value root");
    mismatched.runtime.certified_library_values.symbol = Some(other);
    assert_product_restore_error(
        mismatched,
        "product certified Symbol value does not match the authenticated root",
    );

    let mut out_of_range = compile_symbol_product_parts();
    out_of_range.runtime.certified_library_values.symbol =
        Some(ValueStorageId(out_of_range.binder.decl_count));
    assert_product_restore_error(
        out_of_range,
        "product certified Symbol value is out of range",
    );
}

#[test]
fn symbol_certificate_survives_dense_and_sparse_forks() {
    let (_, mut base) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "forked-certified-symbol.d.ts",
        source: "declare const Symbol: {};",
    }])
    .expect("certified Symbol library compiles");
    let certified = base
        .runtime
        .certified_library_values
        .symbol
        .expect("Symbol root is certified");
    base.freeze_as_library_base().expect("Symbol base freezes");
    let dense = base.fork_user_delta_for_test().expect("dense fork");
    let sparse = base.fork_sparse_collision_epoch().expect("sparse fork");
    assert_eq!(
        dense.runtime.certified_library_values.symbol,
        Some(certified)
    );
    assert_eq!(
        sparse.runtime.certified_library_values.symbol,
        Some(certified)
    );
}

const PRODUCT_SEMANTIC_LIBRARY: &str = r#"
        interface Array<T> { item: T; }
        interface ReadonlyArray<T> { item: T; }
        interface String { stringMarker: string; }
        interface Number { numberMarker: number; }
        interface Boolean { booleanMarker: boolean; }
        interface RegExp { regexpMarker: boolean; }
        interface Object { objectMarker: string; }
        interface Function { functionMarker: string; }
        interface CallableFunction extends Function { callableMarker: number; }
        interface SeamWitness { value: number; }
        type SeamAlias<T> = { value: T; };
        declare class SeamBase { base: string; }
        declare class SeamCtorBase { protected constructor(); }
        declare class SeamInheritedCtor extends SeamCtorBase {}
        declare class SeamClass<T> extends SeamBase {
            constructor(value: T);
            value: T;
        }
        declare namespace SeamClass { export const tag: string; }
        declare namespace SeamSpace { export const enabled: boolean; }
        declare function seamNamed(value: number): string;
        declare namespace seamNamed { export const version: number; }
    "#;

fn compile_semantic_product_parts() -> OwnedLibraryRuntimeProductParts {
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "product-seam.d.ts",
        source: PRODUCT_SEMANTIC_LIBRARY,
    }])
    .expect("focused owned library profile");
    state.into_product_parts().expect("extract product parts")
}

fn compile_reservation_fixture(file_name: &str, source: &str) -> OwnedLibraryRuntimeState {
    compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: file_name,
        source,
    }])
    .expect("reservation lifecycle fixture compiles")
    .1
}

#[test]
fn exact_library_roots_install_only_the_six_trusted_marker_shapes() {
    let state = compile_reservation_fixture(
        "trusted-markers.d.ts",
        r#"
                type ThisParameterType<T> = T extends
                    (this: infer Receiver, ...args: never) => any ? Receiver : unknown;
                type OmitThisParameter<T> = unknown extends ThisParameterType<T>
                    ? T
                    : T extends (...args: infer A) => infer R
                        ? (...args: A) => R
                        : T;
                type Uppercase<S extends string> = intrinsic;
                type Lowercase<S extends string> = intrinsic;
                type Capitalize<S extends string> = intrinsic;
                type Uncapitalize<S extends string> = intrinsic;
                interface ThisType<T> {}
            "#,
    );
    let well_known = state.interner.well_known();
    for (name, marker) in [
        ("Uppercase", well_known.uppercase),
        ("Lowercase", well_known.lowercase),
        ("Capitalize", well_known.capitalize),
        ("Uncapitalize", well_known.uncapitalize),
        ("ThisType", well_known.this_type),
        ("OmitThisParameter", well_known.omit_this_parameter),
    ] {
        assert_eq!(published_template_type(&state, name), marker, "{name}");
    }
}

#[test]
fn early_omit_this_parameter_demand_does_not_freeze_its_library_root_twice() {
    let state = compile_reservation_fixture(
        "early-omit-demand.d.ts",
        r#"
                interface EarlyDemand<
                    T extends OmitThisParameter<
                        (this: { prefix: string }, value: string) => number
                    >
                > {}
                type ThisParameterType<T> = T extends
                    (this: infer Receiver, ...args: never) => any ? Receiver : unknown;
                type OmitThisParameter<T> = unknown extends ThisParameterType<T>
                    ? T
                    : T extends (...args: infer A) => infer R
                        ? (...args: A) => R
                        : T;
            "#,
    );
    assert_eq!(
        published_template_type(&state, "OmitThisParameter"),
        state.interner.well_known().omit_this_parameter
    );
}

#[test]
fn same_named_wrong_shapes_and_unlisted_intrinsics_are_not_trusted_markers() {
    let state = compile_reservation_fixture(
        "untrusted-markers.d.ts",
        r#"
                type Uppercase<S extends string> = S;
                type Lowercase<S extends string> = S;
                type Capitalize<S extends string> = S;
                type Uncapitalize<S extends string> = S;
                type OmitThisParameter<T> = T;
                interface ThisType<T> { marker: T; }
                type NoInfer<T> = intrinsic;
                type IntrinsicAlias<T> = intrinsic;
            "#,
    );
    let well_known = state.interner.well_known();
    for (name, marker) in [
        ("Uppercase", well_known.uppercase),
        ("Lowercase", well_known.lowercase),
        ("Capitalize", well_known.capitalize),
        ("Uncapitalize", well_known.uncapitalize),
        ("ThisType", well_known.this_type),
        ("OmitThisParameter", well_known.omit_this_parameter),
    ] {
        assert_ne!(published_template_type(&state, name), marker, "{name}");
    }
    assert_type_group_is_error_or_unavailable(&state, "NoInfer");
    assert_type_group_is_error_or_unavailable(&state, "IntrinsicAlias");
}

#[test]
fn trusted_string_marker_rejects_each_near_shape_dimension() {
    for source in [
        "type Uppercase<S> = intrinsic;",
        "type Uppercase<S extends number> = intrinsic;",
        "type Uppercase<S extends string = string> = intrinsic;",
        "type Uppercase<S extends string, T> = intrinsic;",
        "type Uppercase<S extends string> = S;",
    ] {
        let state = compile_reservation_fixture("string-marker-near-shape.d.ts", source);
        assert_ne!(
            published_template_type(&state, "Uppercase"),
            state.interner.well_known().uppercase,
            "{source}"
        );
    }
}

#[test]
fn trusted_this_type_marker_rejects_each_near_shape_dimension() {
    for source in [
        "interface ThisType {}",
        "interface ThisType<T, U> {}",
        "interface ThisType<T extends string> {}",
        "interface ThisType<T = unknown> {}",
        "interface ThisType<T> { value: T; }",
    ] {
        let state = compile_reservation_fixture("this-type-near-shape.d.ts", source);
        assert_ne!(
            published_template_type(&state, "ThisType"),
            state.interner.well_known().this_type,
            "{source}"
        );
    }
}

#[test]
fn trusted_omit_this_parameter_rejects_parameter_near_shapes() {
    let body = "unknown extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (...args: A) => R : T";
    for source in [
        format!("type OmitThisParameter = {body};"),
        format!("type OmitThisParameter<T, U> = {body};"),
        format!("type OmitThisParameter<T extends unknown> = {body};"),
        format!("type OmitThisParameter<T = unknown> = {body};"),
    ] {
        let state = compile_reservation_fixture("omit-parameter-near-shape.d.ts", &source);
        assert_ne!(
            published_template_type(&state, "OmitThisParameter"),
            state.interner.well_known().omit_this_parameter,
            "{source}"
        );
    }
}

#[test]
fn trusted_omit_this_parameter_rejects_conditional_near_shapes() {
    for body in [
            "any extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends T ? T : T extends (...args: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends ThisParameterType<T> ? unknown : T extends (...args: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends ThisParameterType<T> ? T : unknown extends (...args: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends ThisParameterType<T> ? T : T extends (value: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (value: A) => R : T",
            "unknown extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (...args: A) => unknown : T",
            "unknown extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (...args: A) => R : unknown",
        ] {
            let source = format!("type OmitThisParameter<T> = {body};");
            let state = compile_reservation_fixture("omit-body-near-shape.d.ts", &source);
            assert_ne!(
                published_template_type(&state, "OmitThisParameter"),
                state.interner.well_known().omit_this_parameter,
                "{source}"
            );
        }
}

fn published_template_type(state: &OwnedLibraryRuntimeState, name: &str) -> TypeId {
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == name)
        .unwrap_or_else(|| panic!("missing type group {name}"));
    match state.published_types.groups().get(group.id) {
        Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
            surface: PublishedTypeGroupSurface::Template(ty),
            ..
        })) => *ty,
        terminal => panic!("{name} did not publish a template: {terminal:?}"),
    }
}

fn published_object_property_type(
    state: &OwnedLibraryRuntimeState,
    owner: &str,
    property: &str,
) -> TypeId {
    let owner = published_template_type(state, owner);
    state
        .interner
        .store()
        .object_type(owner)
        .unwrap_or_else(|| panic!("{owner:?} is not an object template"))
        .property(property)
        .unwrap_or_else(|| panic!("{owner:?} has no {property} property"))
        .ty
}

#[rustfmt::skip]
fn store_type_reaches(
        state: &OwnedLibraryRuntimeState,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        const TYPE_DOMAIN: u8 = 1;
        let (references, _) = state.interner.reference_records_for_test();
        let mut edges = rustc_hash::FxHashMap::<u32, Vec<u32>>::default();
        for (owner_domain, target_domain, _, owner, referenced) in references {
            if owner_domain == TYPE_DOMAIN && target_domain == TYPE_DOMAIN {
                edges.entry(owner).or_default().push(referenced);
            }
        }
        let mut visited = rustc_hash::FxHashSet::default();
        let mut pending = edges.get(&source.0).cloned().unwrap_or_default();
        while let Some(current) = pending.pop() {
            if current == target.0 {
                return true;
            }
            if visited.insert(current) {
                pending.extend(edges.get(&current).into_iter().flatten().copied());
            }
        }
        false
    }

fn assert_strict_interner_state(state: &OwnedLibraryRuntimeState, label: &str) {
    state
        .interner
        .strict_terminal_state_for_test()
        .unwrap_or_else(|error| panic!("{label}: interner is not in terminal state: {error}"));
}

fn assert_type_group_is_error_or_unavailable(state: &OwnedLibraryRuntimeState, name: &str) {
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == name)
        .expect("fixture type group");
    match state.published_types.groups().get(group.id) {
        Some(PublishedTypeGroupTerminal::Unavailable(_)) => {}
        Some(PublishedTypeGroupTerminal::Ready(group)) => match group.surface {
            PublishedTypeGroupSurface::Template(ty) if ty == state.interner.well_known().error => {}
            PublishedTypeGroupSurface::Template(ty) => panic!(
                "{name} accidentally published TypeId {} ({:?}) instead of error/unavailable",
                ty.0,
                state.interner.store().tag(ty)
            ),
            PublishedTypeGroupSurface::Class(class) => {
                panic!("{name} accidentally published class {class:?}")
            }
            PublishedTypeGroupSurface::Enum(ty) => {
                panic!("{name} accidentally published enum TypeId {}", ty.0)
            }
        },
        None => panic!("{name} has no published terminal"),
    }
}

#[rustfmt::skip]
fn strict_interner_state_failure(
        label: &str,
        source: &'static str,
        root: &str,
    ) -> Option<String> {
        let state = compile_reservation_fixture("reservation-lifecycle.d.ts", source);
        assert_type_group_is_error_or_unavailable(&state, root);
        state
            .interner
            .strict_terminal_state_for_test()
            .err()
            .map(|error| format!("{label}: {error}"))
    }

fn assert_valid_class_interface_merge(label: &str, source: &'static str) {
    let state = compile_reservation_fixture("class-interface-merge.d.ts", source);
    let group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "MergedClassInterface")
        .expect("merged class/interface type group");
    let probe = type_probe(
        &state.binder,
        &state.published_types,
        state.interner.store(),
        group.id,
    );
    assert_eq!(probe.declaration_count, 2, "{label}: {probe:?}");
    assert_eq!(
        probe.member_names,
        ["classMember", "interfaceMember"],
        "{label}: {probe:?}"
    );
    assert!(
        matches!(
            state.published_types.groups().get(group.id),
            Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                surface: PublishedTypeGroupSurface::Class(_),
                ..
            }))
        ),
        "{label}: merged type group must publish a class surface"
    );
    state
        .interner
        .strict_terminal_state_for_test()
        .unwrap_or_else(|error| panic!("{label}: interner is not in terminal state: {error}"));
}

fn replace_module_sources(
    parts: &mut OwnedLibraryRuntimeProductParts,
    module_sources: rustc_hash::FxHashMap<ScopeId, crate::binder::namespace::SourceUnitKey>,
) {
    let placeholder = Binder::from_product_parts(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        ScopeId(0),
        ScopeId(0),
        ScopeId(0),
        ScopeId(0),
        0,
        0,
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    let binder = std::mem::replace(&mut parts.binder, placeholder);
    parts.binder = Binder::from_product_parts(
        binder.graph,
        binder.symbols,
        binder.declarations,
        binder.type_groups,
        binder.namespaces,
        binder.module,
        binder.prelude_module,
        binder.compilation_global,
        binder.script_namespace_root,
        binder.decl_count,
        binder.prelude_type_group_count,
        module_sources,
        binder.fn_scopes,
        binder.fn_decl_ids,
        binder.block_scopes,
    );
}

#[rustfmt::skip]
fn assert_product_restore_error(
        parts: OwnedLibraryRuntimeProductParts,
        expected: &'static str,
    ) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            OwnedLibraryRuntimeState::from_product_parts(parts)
        }));
        let Ok(result) = outcome else {
            panic!("product-parts corruption must return an error, not panic")
        };
        match result {
            Err(actual) => assert_eq!(actual, expected),
            Ok(_) => panic!("product-parts corruption unexpectedly restored"),
        }
    }

#[test]
fn injected_results_are_ast_free_owned_terminals() {
    assert_owned_terminal::<InjectedProfileRun>();
    assert_owned_terminal::<InjectedProfileError>();
    assert_owned_terminal::<LibraryPhaseTimings>();
    assert_owned_terminal::<OwnedLibraryRuntimeState>();
}

#[test]
fn failed_conditional_alias_recovery_leaves_no_pending_reservation() {
    let source = r#"
            type FailedAwaited<T> =
                T extends typeof unsupportedValue ? FailedAwaited<T> : T;
        "#;
    let state = compile_reservation_fixture("FailedAwaited.d.ts", source);
    let terminals = state.published_types.aliases().owned_terminals();
    let [(_, PublishedAliasTerminal::Ready(surface))] = terminals.as_slice() else {
        panic!("failed library alias retains one ready compatibility terminal")
    };
    assert_eq!(surface.body(), state.interner.well_known().error);
    assert!(state.interner.strict_terminal_state_for_test().is_ok());
}

#[test]
fn failed_mapped_alias_recovery_leaves_no_pending_reservation() {
    let source = r#"
            type FailedMapped<T> = { [K in FailedMapped<T>]: T };
        "#;
    let failure = strict_interner_state_failure("FailedMapped", source, "FailedMapped");
    assert!(failure.is_none(), "{failure:#?}");
}

#[test]
fn conflicting_conditional_alias_reservations_close_in_both_orders() {
    let alias_first = r#"
            type ConditionalCollision<T> = T extends string ? string : number;
            interface ConditionalCollision<T> { interfaceMember: T; }
        "#;
    let interface_first = r#"
            interface ConditionalCollision<T> { interfaceMember: T; }
            type ConditionalCollision<T> = T extends string ? string : number;
        "#;
    let failures = [
        strict_interner_state_failure(
            "conditional-alias-first",
            alias_first,
            "ConditionalCollision",
        ),
        strict_interner_state_failure(
            "conditional-interface-first",
            interface_first,
            "ConditionalCollision",
        ),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    assert!(failures.is_empty(), "{failures:#?}");
}

#[rustfmt::skip]
#[test]
    fn conflicting_mapped_alias_reservations_close_in_both_orders() {
        let alias_first = r#"
            type MappedCollision<T> = { [K in keyof T]: T[K] };
            interface MappedCollision<T> { interfaceMember: T; }
        "#;
        let interface_first = r#"
            interface MappedCollision<T> { interfaceMember: T; }
            type MappedCollision<T> = { [K in keyof T]: T[K] };
        "#;
        let failures = [
            strict_interner_state_failure("mapped-alias-first", alias_first, "MappedCollision"),
            strict_interner_state_failure(
                "mapped-interface-first",
                interface_first,
                "MappedCollision",
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert!(failures.is_empty(), "{failures:#?}");
    }

#[rustfmt::skip]
#[test]
    fn conflicting_object_alias_reservations_close_in_both_orders() {
        let alias_first = r#"
            type ObjectCollision = { aliasMember: string };
            interface ObjectCollision { interfaceMember: number; }
        "#;
        let interface_first = r#"
            interface ObjectCollision { interfaceMember: number; }
            type ObjectCollision = { aliasMember: string };
        "#;
        let failures = [
            strict_interner_state_failure("object-alias-first", alias_first, "ObjectCollision"),
            strict_interner_state_failure(
                "object-interface-first",
                interface_first,
                "ObjectCollision",
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert!(failures.is_empty(), "{failures:#?}");
    }

#[test]
fn valid_class_interface_merge_closes_reservations_in_both_orders() {
    let class_first = r#"
            declare class MergedClassInterface { classMember: string; }
            interface MergedClassInterface { interfaceMember: number; }
        "#;
    let interface_first = r#"
            interface MergedClassInterface { interfaceMember: number; }
            declare class MergedClassInterface { classMember: string; }
        "#;

    assert_valid_class_interface_merge("class-first control", class_first);
    assert_valid_class_interface_merge("interface-first", interface_first);
}

#[test]
fn acyclic_object_aliases_publish_one_structural_identity() {
    let state = compile_reservation_fixture(
        "acyclic-object-aliases.d.ts",
        r#"
                type FirstShape = {
                    value: number;
                    run(input: string): boolean;
                };
                type SecondShape = {
                    run(input: string): boolean;
                    value: number;
                };
            "#,
    );
    let first = published_template_type(&state, "FirstShape");
    let second = published_template_type(&state, "SecondShape");

    assert_eq!(first, second, "equal acyclic aliases must hash-cons");
    assert!(!store_type_reaches(&state, first, first));
    assert_strict_interner_state(&state, "equal acyclic aliases");
}

#[test]
fn acyclic_object_alias_reuses_an_existing_anonymous_shape() {
    let state = compile_reservation_fixture(
        "anonymous-object-collision.d.ts",
        r#"
                type Carrier = {
                    nested: {
                        value: number;
                        run(input: string): boolean;
                    };
                };
                type NamedShape = {
                    value: number;
                    run(input: string): boolean;
                };
            "#,
    );
    let anonymous = published_object_property_type(&state, "Carrier", "nested");
    let named = published_template_type(&state, "NamedShape");

    assert_eq!(named, anonymous, "named alias must reuse the anonymous row");
    assert_strict_interner_state(&state, "anonymous shape collision");
}

#[test]
fn recursive_object_aliases_retain_their_stable_reserved_roots() {
    let state = compile_reservation_fixture(
        "recursive-object-aliases.d.ts",
        r#"
                type SelfNode = { next: SelfNode | null };
                type MutualLeft = { right: MutualRight | null };
                type MutualRight = { left: MutualLeft | null };
                type NestedNode = {
                    next: (() => readonly NestedNode[]) | null;
                };
            "#,
    );
    let self_node = published_template_type(&state, "SelfNode");
    let mutual_left = published_template_type(&state, "MutualLeft");
    let mutual_right = published_template_type(&state, "MutualRight");
    let nested = published_template_type(&state, "NestedNode");

    assert!(store_type_reaches(&state, self_node, self_node));
    assert_ne!(mutual_left, mutual_right);
    assert!(store_type_reaches(&state, mutual_left, mutual_right));
    assert!(store_type_reaches(&state, mutual_right, mutual_left));
    assert!(store_type_reaches(&state, nested, nested));
    assert_strict_interner_state(&state, "recursive aliases");
}

#[test]
fn captured_object_alias_roots_are_never_remapped_behind_existing_edges() {
    let state = compile_reservation_fixture(
        "captured-object-aliases.d.ts",
        r#"
                type ForwardConsumer = { value: ForwardLater };
                type ForwardLater = { marker: number };

                interface InterfaceConsumer { value: InterfaceLater; }
                type InterfaceLater = { marker: string };

                type BackwardEarlier = { marker: boolean };
                type BackwardConsumer = { value: BackwardEarlier };
            "#,
    );

    for (consumer, dependency) in [
        ("ForwardConsumer", "ForwardLater"),
        ("InterfaceConsumer", "InterfaceLater"),
        ("BackwardConsumer", "BackwardEarlier"),
    ] {
        assert_eq!(
            published_object_property_type(&state, consumer, "value"),
            published_template_type(&state, dependency),
            "{consumer} must retain the published identity of {dependency}"
        );
    }
    assert_strict_interner_state(&state, "captured alias roots");
}

#[test]
fn type_parameter_defaults_follow_final_canonical_alias_identity() {
    let state = compile_reservation_fixture(
        "type-parameter-default-alias-capture.d.ts",
        r#"
                type FirstShape = { marker: number };
                type Box<T = NamedShape> = T;
                type NamedShape = { marker: number };
            "#,
    );
    let first = published_template_type(&state, "FirstShape");
    let named = published_template_type(&state, "NamedShape");
    let box_group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "Box")
        .expect("Box type group");
    let Some(PublishedTypeGroupTerminal::Ready(box_surface)) =
        state.published_types.groups().get(box_group.id)
    else {
        panic!("Box did not publish a ready type group");
    };

    assert_eq!(first, named, "equal acyclic aliases must hash-cons");
    assert_strict_interner_state(&state, "type parameter default alias capture");
    assert_eq!(
        box_surface.parameter_defaults,
        [PublishedTypeParameterDefault::Ready(named)],
        "published defaults must follow the final canonical alias identity"
    );
}

#[test]
fn replay_index_covers_class_references_in_interface_parameter_defaults() {
    let state = compile_reservation_fixture(
        "replay-interface-class-default.d.ts",
        r#"
                declare class DefaultClass {}
                interface Box<T = DefaultClass> { value: T; }
            "#,
    );
    let replay = state
        .replay_index()
        .expect("source compiler retains its replay index");

    assert_eq!(replay.unowned_demand_count, 0);
    assert_eq!(replay.invalid_owner_site_count, 0);
    assert_eq!(replay.noncanonical_edge_count, 0);
    assert_eq!(replay.typed_reference_coverage_misses, 0);
}

#[test]
fn replay_index_covers_class_references_copied_across_overload_rows() {
    let state = compile_reservation_fixture(
        "replay-overload-class-provenance.d.ts",
        r#"
                declare class DefaultClass {}
                declare function consume(value: DefaultClass): void;
                declare function consume(value: string): void;
            "#,
    );
    let replay = state
        .replay_index()
        .expect("source compiler retains its replay index");

    assert_eq!(replay.unowned_demand_count, 0);
    assert_eq!(replay.invalid_owner_site_count, 0);
    assert_eq!(replay.noncanonical_edge_count, 0);
    assert_eq!(replay.typed_reference_coverage_misses, 0);
}

#[test]
fn terminal_class_validator_follows_declared_recipe_template_dependencies() {
    const DECLARED_RECIPE_DOMAIN: u8 = 10;
    const TYPE_DOMAIN: u8 = 1;
    const CLASS_DOMAIN: u8 = 3;

    let state = compile_reservation_fixture(
        "replay-declared-recipe-class-template.d.ts",
        r#"
                declare class RecipeDependency {}
                interface RecipeTemplate<T> {
                    item: T;
                    dependency: RecipeDependency;
                }
                interface RecipeConsumer {
                    nested: RecipeTemplate<string>[];
                }
            "#,
    );
    let consumer_group = state
        .binder
        .type_groups
        .iter()
        .find(|group| group.name == "RecipeConsumer")
        .expect("consumer type group")
        .id;
    let consumer_template = published_template_type(&state, "RecipeConsumer");
    let (store_references, _) = state.interner.reference_records_for_test();
    let mut pending = vec![(TYPE_DOMAIN, consumer_template.0, false)];
    let mut visited = BTreeSet::new();
    let mut reaches_class_through_recipe = false;
    while let Some((domain, owner, crossed_recipe)) = pending.pop() {
        if !visited.insert((domain, owner, crossed_recipe)) {
            continue;
        }
        for &(owner_domain, target_domain, _, row, target) in &store_references {
            if owner_domain != domain || row != owner {
                continue;
            }
            if target_domain == CLASS_DOMAIN && crossed_recipe {
                reaches_class_through_recipe = true;
                break;
            }
            if matches!(target_domain, TYPE_DOMAIN | DECLARED_RECIPE_DOMAIN) {
                pending.push((
                    target_domain,
                    target,
                    crossed_recipe || target_domain == DECLARED_RECIPE_DOMAIN,
                ));
            }
        }
        if reaches_class_through_recipe {
            break;
        }
    }
    assert!(
        reaches_class_through_recipe,
        "fixture must expose Type -> Recipe -> Type/Class dependency reachability"
    );

    let trace = ReplayDependencyTrace::default();
    let replay = state
        .replay_index()
        .expect("source compiler retains its replay index");
    for edge in &replay.reverse_edges {
        if edge.consumer == ReplayOwner::TypeGroup(consumer_group) {
            continue;
        }
        trace.enter(edge.consumer);
        trace.demand(edge.dependency);
        trace.leave(edge.consumer);
    }
    let runtime = state
        .runtime
        .snapshot_parts()
        .expect("runtime snapshot parts");
    validate_terminal_class_dependencies(
        Some(&trace),
        &state.interner,
        ReplayTerminalValidationInputs {
            binder: &state.binder,
            published: &state.published_types,
            decl_types: &state.decl_types,
            namespace_terminals: &runtime.namespace_terminals,
            runtime: &runtime,
            semantic_identities: state.semantic_identities.as_ref(),
        },
    )
    .expect("terminal validation enumerates canonical references");
    let error = trace
        .finish(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0)
        .expect_err("omitting the consumer dependency must fail typed coverage");
    assert!(
        matches!(
            error,
            ReplayIndexGenerationError::TypedReferenceCoverage { .. }
        ),
        "declared recipe template dependency was invisible to terminal validation: {error:?}"
    );
}

#[rustfmt::skip]
#[test]
    fn terminal_class_validator_preserves_cycles_duplicates_and_multiple_classes() {
        const TYPE_DOMAIN: u8 = 1;
        const DECLARED_RECIPE_DOMAIN: u8 = 10;

        let owner = ReplayOwner::TypeGroup(TypeGroupId(0));
        let classes = [ClassId(7), ClassId(11)];
        let semantic_edges = BTreeMap::from([
            ((TYPE_DOMAIN, 0), vec![(DECLARED_RECIPE_DOMAIN, 10)]),
            (
                (DECLARED_RECIPE_DOMAIN, 10),
                vec![(TYPE_DOMAIN, 11), (TYPE_DOMAIN, 11)],
            ),
            ((TYPE_DOMAIN, 11), vec![(DECLARED_RECIPE_DOMAIN, 10)]),
        ]);
        let class_edges = BTreeMap::from([
            ((DECLARED_RECIPE_DOMAIN, 10), vec![classes[0], classes[0]]),
            ((TYPE_DOMAIN, 11), vec![classes[1]]),
        ]);
        let direct = BTreeMap::from([(owner, vec![TypeId(0)])]);

        let trace = ReplayDependencyTrace::default();
        trace.enter(owner);
        for class in classes {
            trace.demand(ReplayOwner::Class(class));
        }
        trace.leave(owner);
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            direct,
            None,
        );

        let partition = vec![
            owner,
            ReplayOwner::Class(classes[0]),
            ReplayOwner::Class(classes[1]),
        ];
        let sites = partition
            .iter()
            .copied()
            .map(|owner| ReplayOwnerSite {
                owner,
                file_ordinal: LibraryFileOrdinal::new(0),
                span: Span::new(0, 1),
            })
            .collect::<Vec<_>>();
        let baselines = partition
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
            .collect::<Vec<_>>();
        let replay = trace
            .finish(partition, Vec::new(), sites, baselines, 0)
            .expect("cycles and duplicate edges retain both terminal classes");
        assert_eq!(
            replay
                .reverse_edges
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                ReplayReverseEdge {
                    dependency: ReplayOwner::Class(classes[0]),
                    consumer: owner,
                },
                ReplayReverseEdge {
                    dependency: ReplayOwner::Class(classes[1]),
                    consumer: owner,
                },
            ])
        );
    }

#[test]
fn terminal_class_validator_processes_a_shared_semantic_tail_once() {
    const TYPE_DOMAIN: u8 = 1;
    const DECLARED_RECIPE_DOMAIN: u8 = 10;
    const OWNER_COUNT: usize = 64;
    const SHARED_DEPTH: usize = 128;

    let owners = (0..OWNER_COUNT)
        .map(|index| {
            ReplayOwner::TypeGroup(TypeGroupId(
                u32::try_from(index).expect("owner id fits u32"),
            ))
        })
        .collect::<Vec<_>>();
    let classes = [ClassId(7), ClassId(11), ClassId(13)];
    let shared_tail = (0..SHARED_DEPTH)
        .map(|index| {
            let domain = if index % 2 == 0 {
                DECLARED_RECIPE_DOMAIN
            } else {
                TYPE_DOMAIN
            };
            (
                domain,
                10_000 + u32::try_from(index).expect("semantic id fits u32"),
            )
        })
        .collect::<Vec<_>>();

    let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
    let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();
    for (index, owner) in owners.iter().copied().enumerate() {
        let root = TypeId(u32::try_from(index).expect("root type id fits u32"));
        direct.insert(owner, vec![root]);
        semantic_edges
            .entry((TYPE_DOMAIN, root.0))
            .or_default()
            .push(shared_tail[0]);
    }
    for pair in shared_tail.windows(2) {
        semantic_edges
            .entry(pair[0])
            .or_default()
            .extend([pair[1], pair[1]]);
    }
    semantic_edges
        .entry(*shared_tail.last().expect("non-empty shared tail"))
        .or_default()
        .push(shared_tail[SHARED_DEPTH / 2]);

    let mut class_edges = BTreeMap::<(u8, u32), Vec<ClassId>>::new();
    class_edges
        .entry(shared_tail[SHARED_DEPTH / 4])
        .or_default()
        .extend([classes[0], classes[0]]);
    class_edges
        .entry(shared_tail[SHARED_DEPTH / 2])
        .or_default()
        .push(classes[1]);
    class_edges
        .entry(*shared_tail.last().expect("non-empty shared tail"))
        .or_default()
        .push(classes[2]);

    let trace = ReplayDependencyTrace::default();
    for owner in &owners {
        trace.enter(*owner);
        for class in classes {
            trace.demand(ReplayOwner::Class(class));
        }
        trace.leave(*owner);
    }
    let mut work = TerminalClassDependencyValidationWork::default();
    require_terminal_class_dependency_closure(
        &trace,
        &semantic_edges,
        &class_edges,
        direct,
        Some(&mut work),
    );

    let mut partition = owners.clone();
    partition.extend(classes.into_iter().map(ReplayOwner::Class));
    let sites = partition
        .iter()
        .copied()
        .map(|owner| ReplayOwnerSite {
            owner,
            file_ordinal: LibraryFileOrdinal::new(0),
            span: Span::new(0, 1),
        })
        .collect::<Vec<_>>();
    let baselines = partition
        .iter()
        .copied()
        .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
        .collect::<Vec<_>>();
    let replay = trace
        .finish(partition, Vec::new(), sites, baselines, 0)
        .expect("cycles and duplicate semantic edges preserve the exact class oracle");
    let expected_edges = owners
        .iter()
        .flat_map(|owner| {
            classes.iter().map(move |class| ReplayReverseEdge {
                dependency: ReplayOwner::Class(*class),
                consumer: *owner,
            })
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        replay
            .reverse_edges
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        expected_edges,
        "shared-tail summarization must not change the owner-to-class oracle"
    );

    let unique_semantic_nodes = OWNER_COUNT + SHARED_DEPTH;
    assert_eq!(
        work.owner_root_summaries,
        u64::try_from(OWNER_COUNT).expect("owner count fits u64")
    );
    let linear_input_size = unique_semantic_nodes
        + semantic_edges.values().map(Vec::len).sum::<usize>()
        + class_edges.values().map(Vec::len).sum::<usize>()
        + OWNER_COUNT;
    let measured_work = work
        .owner_root_summaries
        .saturating_add(work.semantic_node_visits)
        .saturating_add(work.semantic_edge_probes)
        .saturating_add(work.class_edge_probes);
    assert!(
        measured_work <= 4 * u64::try_from(linear_input_size).expect("validation input fits u64"),
        "terminal dependency validation must stay O(V + E + owners), not owners * depth: \
             input={linear_input_size}, work={work:?}"
    );
}

#[test]
fn terminal_class_validator_deduplicates_repeated_class_across_shared_tail() {
    const TYPE_DOMAIN: u8 = 1;
    const OWNER_COUNT: usize = 64;
    const SHARED_DEPTH: usize = 128;

    let owners = (0..OWNER_COUNT)
        .map(|index| {
            ReplayOwner::TypeGroup(TypeGroupId(
                u32::try_from(index).expect("owner id fits u32"),
            ))
        })
        .collect::<Vec<_>>();
    let class = ClassId(7);
    let shared_tail = (0..SHARED_DEPTH)
        .map(|index| {
            (
                TYPE_DOMAIN,
                10_000 + u32::try_from(index).expect("semantic id fits u32"),
            )
        })
        .collect::<Vec<_>>();
    let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
    let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();
    for (index, owner) in owners.iter().copied().enumerate() {
        let root = TypeId(u32::try_from(index).expect("root type id fits u32"));
        direct.insert(owner, vec![root]);
        semantic_edges.insert((TYPE_DOMAIN, root.0), vec![shared_tail[0]]);
    }
    for pair in shared_tail.windows(2) {
        semantic_edges.insert(pair[0], vec![pair[1]]);
    }
    let class_edges = shared_tail
        .iter()
        .copied()
        .map(|node| (node, vec![class]))
        .collect::<BTreeMap<_, _>>();

    let trace = ReplayDependencyTrace::default();
    for owner in &owners {
        trace.enter(*owner);
        trace.demand(ReplayOwner::Class(class));
        trace.leave(*owner);
    }
    let mut work = TerminalClassDependencyValidationWork::default();
    require_terminal_class_dependency_closure(
        &trace,
        &semantic_edges,
        &class_edges,
        direct,
        Some(&mut work),
    );

    let mut partition = owners.clone();
    partition.push(ReplayOwner::Class(class));
    let sites = partition
        .iter()
        .copied()
        .map(|owner| ReplayOwnerSite {
            owner,
            file_ordinal: LibraryFileOrdinal::new(0),
            span: Span::new(0, 1),
        })
        .collect::<Vec<_>>();
    let baselines = partition
        .iter()
        .copied()
        .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
        .collect::<Vec<_>>();
    let replay = trace
        .finish(partition, Vec::new(), sites, baselines, 0)
        .expect("repeated class references retain their exact terminal-class oracle");
    let expected_edges = owners
        .iter()
        .map(|owner| ReplayReverseEdge {
            dependency: ReplayOwner::Class(class),
            consumer: *owner,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        replay
            .reverse_edges
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        expected_edges
    );

    let semantic_node_count = OWNER_COUNT + SHARED_DEPTH;
    let semantic_edge_count = OWNER_COUNT + SHARED_DEPTH - 1;
    let class_edge_count = SHARED_DEPTH;
    let output_edge_count = OWNER_COUNT;
    let linear_input_and_output = semantic_node_count
        + semantic_edge_count
        + class_edge_count
        + output_edge_count
        + OWNER_COUNT;
    assert!(
        work.class_summary_items
            <= 6 * u64::try_from(linear_input_and_output)
                .expect("validation input and output fit u64"),
        "repeated terminal classes must be deduplicated before owner expansion: \
             input+output={linear_input_and_output}, work={work:?}"
    );
}

#[test]
fn terminal_class_validator_summary_work_is_output_sensitive_for_unique_chain() {
    const TYPE_DOMAIN: u8 = 1;
    const ROOTED_DEPTH: usize = 1_024;
    const DISCONNECTED_DEPTH: usize = 2_048;

    let owner = ReplayOwner::TypeGroup(TypeGroupId(0));
    let rooted_classes = (0..ROOTED_DEPTH)
        .map(|index| ClassId(10_000 + u32::try_from(index).expect("class id fits u32")))
        .collect::<Vec<_>>();
    let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
    let mut class_edges = BTreeMap::<(u8, u32), Vec<ClassId>>::new();

    for (index, class) in rooted_classes.iter().copied().enumerate() {
        let node = (
            TYPE_DOMAIN,
            u32::try_from(index).expect("semantic id fits u32"),
        );
        class_edges.insert(node, vec![class]);
        if index + 1 < ROOTED_DEPTH {
            semantic_edges.insert(
                node,
                vec![(
                    TYPE_DOMAIN,
                    u32::try_from(index + 1).expect("semantic id fits u32"),
                )],
            );
        }
    }

    for index in 0..DISCONNECTED_DEPTH {
        let id = 100_000 + u32::try_from(index).expect("semantic id fits u32");
        let node = (TYPE_DOMAIN, id);
        class_edges.insert(
            node,
            vec![ClassId(
                200_000 + u32::try_from(index).expect("class id fits u32"),
            )],
        );
        if index + 1 < DISCONNECTED_DEPTH {
            semantic_edges.insert(node, vec![(TYPE_DOMAIN, id + 1)]);
        }
    }

    let trace = ReplayDependencyTrace::default();
    trace.enter(owner);
    for class in &rooted_classes {
        trace.demand(ReplayOwner::Class(*class));
    }
    trace.leave(owner);

    let mut work = TerminalClassDependencyValidationWork::default();
    require_terminal_class_dependency_closure(
        &trace,
        &semantic_edges,
        &class_edges,
        BTreeMap::from([(owner, vec![TypeId(0)])]),
        Some(&mut work),
    );

    let mut partition = vec![owner];
    partition.extend(rooted_classes.iter().copied().map(ReplayOwner::Class));
    let sites = partition
        .iter()
        .copied()
        .map(|owner| ReplayOwnerSite {
            owner,
            file_ordinal: LibraryFileOrdinal::new(0),
            span: Span::new(0, 1),
        })
        .collect::<Vec<_>>();
    let baselines = partition
        .iter()
        .copied()
        .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
        .collect::<Vec<_>>();
    let replay = trace
        .finish(partition, Vec::new(), sites, baselines, 0)
        .expect("the rooted chain retains its exact terminal-class oracle");
    let expected_edges = rooted_classes
        .iter()
        .map(|class| ReplayReverseEdge {
            dependency: ReplayOwner::Class(*class),
            consumer: owner,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        replay
            .reverse_edges
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        expected_edges,
        "the output oracle must contain exactly the rooted chain's classes"
    );

    let semantic_node_count = ROOTED_DEPTH + DISCONNECTED_DEPTH;
    let semantic_edge_count = (ROOTED_DEPTH - 1) + (DISCONNECTED_DEPTH - 1);
    let class_edge_count = semantic_node_count;
    let output_edge_count = ROOTED_DEPTH;
    let linear_input_and_output =
        semantic_node_count + semantic_edge_count + class_edge_count + output_edge_count + 1;
    assert!(
        work.class_summary_items
            <= 6 * u64::try_from(linear_input_and_output)
                .expect("validation input and output fit u64"),
        "terminal-class summaries must be root-relevant and output-sensitive, not copy every \
             suffix's growing class set: input+output={linear_input_and_output}, work={work:?}"
    );
}

/// Shared *owner-expression* spine: a chain of `L` components each merging the
/// same cross owner, so every spine component mints a fresh two-input
/// `TerminalOwnerExpression::Union` whose owner set stays tiny (`{seed, cross}`).
/// `M` sibling components hang off the tail, each with its own owner and its own
/// terminal class, so each becomes a *separate* query root that is not an
/// ancestor of any other. No spine union is ever a query root, so a
/// summarizer that memoises only at roots re-walks the whole spine once per
/// sibling — Θ(M · L) work for Θ(M) output.
fn shared_union_spine_probe(
    spine_len: usize,
    siblings: usize,
    demand_first_sibling: bool,
) -> (u64, TerminalClassDependencyValidationWork) {
    const TYPE_DOMAIN: u8 = 1;
    let spine = |index: usize| {
        (
            TYPE_DOMAIN,
            1_000_000 + u32::try_from(index).expect("spine id fits u32"),
        )
    };
    let cross = (TYPE_DOMAIN, 2_000_000_u32);
    let sibling = |index: usize| {
        (
            TYPE_DOMAIN,
            3_000_000 + u32::try_from(index).expect("sibling id fits u32"),
        )
    };
    let sibling_root = |index: usize| {
        (
            TYPE_DOMAIN,
            4_000_000 + u32::try_from(index).expect("sibling root id fits u32"),
        )
    };

    let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
    let mut class_edges = BTreeMap::<(u8, u32), Vec<ClassId>>::new();
    let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();

    let seed_owner = ReplayOwner::TypeGroup(TypeGroupId(0));
    let cross_owner = ReplayOwner::TypeGroup(TypeGroupId(1));
    for index in 0..spine_len - 1 {
        semantic_edges
            .entry(spine(index))
            .or_default()
            .push(spine(index + 1));
    }
    for index in 0..spine_len {
        semantic_edges.entry(cross).or_default().push(spine(index));
    }
    direct.insert(seed_owner, vec![TypeId(spine(0).1)]);
    direct.insert(cross_owner, vec![TypeId(cross.1)]);

    let classes = (0..siblings)
        .map(|index| ClassId(100 + u32::try_from(index).expect("class id fits u32")))
        .collect::<Vec<_>>();
    let sibling_owners = (0..siblings)
        .map(|index| {
            ReplayOwner::TypeGroup(TypeGroupId(
                2 + u32::try_from(index).expect("owner id fits u32"),
            ))
        })
        .collect::<Vec<_>>();
    for index in 0..siblings {
        semantic_edges
            .entry(spine(spine_len - 1))
            .or_default()
            .push(sibling(index));
        semantic_edges
            .entry(sibling_root(index))
            .or_default()
            .push(sibling(index));
        direct.insert(sibling_owners[index], vec![TypeId(sibling_root(index).1)]);
        class_edges
            .entry(sibling(index))
            .or_default()
            .push(classes[index]);
    }

    let trace = ReplayDependencyTrace::default();
    if demand_first_sibling {
        // Only the first sibling owner's edge is authenticated; every other
        // emitted pair stays a coverage miss, which the caller counts.
        trace.enter(sibling_owners[0]);
        trace.demand(ReplayOwner::Class(classes[0]));
        trace.leave(sibling_owners[0]);
    }

    let mut work = TerminalClassDependencyValidationWork::default();
    require_terminal_class_dependency_closure(
        &trace,
        &semantic_edges,
        &class_edges,
        direct,
        Some(&mut work),
    );

    let error = trace
        .finish(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0)
        .expect_err("the probe authenticates at most one emitted pair");
    let ReplayIndexGenerationError::TypedReferenceCoverage { count, .. } = error else {
        panic!("terminal closure must report its unauthenticated pairs: {error:?}");
    };
    (count, work)
}

/// Input + output size of the shared-union-spine shape: semantic nodes and
/// edges, class edges, owner roots, and the emitted owner × class pairs.
fn shared_union_spine_input_and_output(spine_len: usize, siblings: usize) -> usize {
    let semantic_nodes = spine_len + 1 + 2 * siblings;
    let semantic_edges = (spine_len - 1) + spine_len + 2 * siblings;
    let class_edges = siblings;
    let owner_roots = 2 + siblings;
    let output_pairs = 3 * siblings;
    semantic_nodes + semantic_edges + class_edges + owner_roots + output_pairs
}

#[test]
fn terminal_class_validator_shared_union_spine_stays_complete() {
    const BASE: usize = 256;

    // Completeness: each class must still summarize to exactly its three
    // owners {seed, cross, own}, and an authenticated pair must be seen.
    let (base_misses, base_work) = shared_union_spine_probe(BASE, BASE, false);
    let (authenticated_misses, _) = shared_union_spine_probe(BASE, BASE, true);
    assert_eq!(
        base_misses,
        u64::try_from(3 * BASE).expect("pair count fits u64"),
        "each sibling class must summarize to exactly its three owners"
    );
    assert_eq!(
        authenticated_misses,
        base_misses - 1,
        "authenticating one owner-class edge must retire exactly one pair"
    );

    // The graph-build / SCC / Kahn counters stay linear on this shape — none
    // of them observes the owner-expression traversal, which the companion
    // `..._summary_is_output_sensitive` spec pins separately.
    let input_and_output = shared_union_spine_input_and_output(BASE, BASE);
    let counted = base_work
        .owner_root_summaries
        .saturating_add(base_work.semantic_node_visits)
        .saturating_add(base_work.semantic_edge_probes)
        .saturating_add(base_work.class_edge_probes)
        .saturating_add(base_work.class_summary_items);
    assert!(
        counted <= 6 * u64::try_from(input_and_output).expect("input fits u64"),
        "the committed counters are blind to the owner-expression traversal: \
             input+output={input_and_output}, work={base_work:?}"
    );
}

#[test]
fn terminal_class_validator_shared_union_spine_summary_is_output_sensitive() {
    const BASE: usize = 256;
    const SCALED: usize = 1_024;

    let (base_misses, base_work) = shared_union_spine_probe(BASE, BASE, false);
    let (scaled_misses, scaled_work) = shared_union_spine_probe(SCALED, SCALED, false);
    assert_eq!(
        base_misses,
        u64::try_from(3 * BASE).expect("pair count fits u64"),
        "each sibling class must summarize to exactly its three owners"
    );
    assert_eq!(
        scaled_misses,
        u64::try_from(3 * SCALED).expect("pair count fits u64"),
        "each sibling class must summarize to exactly its three owners"
    );

    // Two sizes, so a super-linear summary is visible as a ratio and not
    // just as a budget overrun on one arbitrary shape.
    let base_budget = 6 * u64::try_from(shared_union_spine_input_and_output(BASE, BASE))
        .expect("validation input and output fit u64");
    let scaled_budget = 6 * u64::try_from(shared_union_spine_input_and_output(SCALED, SCALED))
        .expect("validation input and output fit u64");
    let base_visits = base_work.owner_expression_visits;
    let scaled_visits = scaled_work.owner_expression_visits;
    assert!(
        base_visits <= base_budget && scaled_visits <= scaled_budget,
        "owner-expression summarization must stay O(inputs + owners), not owners * spine \
             length: {BASE}x{BASE} visited {base_visits} (budget {base_budget}), \
             {SCALED}x{SCALED} visited {scaled_visits} (budget {scaled_budget})"
    );
}

#[test]
fn declared_recipe_domain_uses_independent_base_and_local_ends() {
    const DECLARED_RECIPE_DOMAIN: u8 = 10;
    const TYPE_DOMAIN: u8 = 1;

    let state = compile_reservation_fixture(
        "declared-recipe-domain-ends.d.ts",
        "interface RecipeOwner { values: string[]; }",
    );
    let base = owned_state_identity_ends(&state);
    let recipe_end = state.interner.store().all_declared_recipes().count();
    assert!(recipe_end > 0, "fixture must plan declaration recipes");

    let mut summary = OwnedBaseReferenceSummary::default();
    classify_interner_live_reference(
        &mut summary,
        &base,
        DECLARED_RECIPE_DOMAIN,
        0,
        TYPE_DOMAIN,
        u32::try_from(base.store).expect("store end fits u32"),
    );
    classify_interner_live_reference(
        &mut summary,
        &base,
        DECLARED_RECIPE_DOMAIN,
        u32::try_from(recipe_end).expect("recipe end fits u32"),
        TYPE_DOMAIN,
        0,
    );

    assert_eq!(
        summary,
        OwnedBaseReferenceSummary {
            base_to_delta: 1,
            delta_to_base: 1,
            delta_to_delta: 0,
        },
        "recipe rows must be classified against their own frozen prefix"
    );
}

#[test]
fn binder_source_unit_domain_cannot_alias_the_declared_recipe_domain() {
    const SOURCE_UNIT_DOMAIN: u8 = 10;
    const SCOPE_DOMAIN: u8 = 4;

    let state = compile_reservation_fixture(
        "source-unit-domain-origin.d.ts",
        "interface RecipeOwner { values: string[]; }",
    );
    let mut base = owned_state_identity_ends(&state);
    assert!(base.source_units > 0);
    assert!(base.scopes > 0);
    base.declared_recipes = base.source_units.saturating_add(100);

    let first_delta_source = u32::try_from(base.source_units).expect("source-unit end fits u32");
    let last_base_source = first_delta_source
        .checked_sub(1)
        .expect("fixture retains a base source unit");
    let first_delta_scope = u32::try_from(base.scopes).expect("scope end fits u32");

    let mut summary = OwnedBaseReferenceSummary::default();
    classify_binder_live_reference(
        &mut summary,
        &base,
        SOURCE_UNIT_DOMAIN,
        first_delta_source,
        SCOPE_DOMAIN,
        first_delta_scope,
    );
    classify_binder_live_reference(
        &mut summary,
        &base,
        SOURCE_UNIT_DOMAIN,
        last_base_source,
        SCOPE_DOMAIN,
        first_delta_scope,
    );
    classify_binder_live_reference(
        &mut summary,
        &base,
        SOURCE_UNIT_DOMAIN,
        first_delta_source,
        SCOPE_DOMAIN,
        0,
    );

    assert_eq!(
        summary,
        OwnedBaseReferenceSummary {
            base_to_delta: 1,
            delta_to_base: 1,
            delta_to_delta: 1,
        },
        "binder source units use their source prefix even when recipe ids overlap"
    );
}

#[test]
fn store_prefix_digest_authenticates_unreferenced_planned_recipes() {
    let mut interner = Interner::with_intrinsics();
    let prefix_len = interner.store().len();
    let recipe_prefix_len = interner.store().all_declared_recipes().count();
    let before = store_prefix_digest(interner.store(), prefix_len, recipe_prefix_len)
        .expect("initial prefix digest");
    let string = interner.well_known().string;
    interner.intern_declared_recipe(crate::types::repr::DeclaredRecipeNode::Type(string));
    let planned_recipe_prefix_len = interner.store().all_declared_recipes().count();
    let after = store_prefix_digest(interner.store(), prefix_len, planned_recipe_prefix_len)
        .expect("prefix digest after recipe planning");

    assert_ne!(
        after, before,
        "an unreferenced recipe row is still authenticated prefix state"
    );
}

#[test]
fn replay_index_records_empty_and_constructor_only_parent_edges() {
    let state = compile_reservation_fixture(
        "replay-class-parent-edges.d.ts",
        r#"
                declare class EmptyBase {}
                declare class EmptyChild extends EmptyBase {}
                declare class ConstructorBase { constructor(value: string); }
                declare class ConstructorChild extends ConstructorBase {
                    constructor(value: string);
                }
            "#,
    );
    let class = |name: &str| {
        let group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == name)
            .unwrap_or_else(|| panic!("missing class group {name}"));
        match state.published_types.groups().get(group.id) {
            Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                surface: PublishedTypeGroupSurface::Class(class),
                ..
            })) => *class,
            terminal => panic!("{name} did not publish a class: {terminal:?}"),
        }
    };
    let replay = state
        .replay_index()
        .expect("source compiler retains its replay index");

    for (parent, child) in [
        (class("EmptyBase"), class("EmptyChild")),
        (class("ConstructorBase"), class("ConstructorChild")),
    ] {
        assert!(replay.reverse_edges.contains(&ReplayReverseEdge {
            dependency: ReplayOwner::Class(parent),
            consumer: ReplayOwner::Class(child),
        }));
    }
}

fn root_candidate(
    slots: impl IntoIterator<Item = SourceBindingSlot>,
) -> crate::binder::declaration::SourceGlobalBindingCandidate {
    crate::binder::declaration::SourceGlobalBindingCandidate {
        slots: slots.into_iter().collect(),
        global_object_contributor: false,
    }
}

#[test]
fn replay_root_census_rejects_a_missing_binder_root() {
    let candidates = BTreeMap::from([(
        "PresentOnlyInSource".to_owned(),
        root_candidate([SourceBindingSlot::Value]),
    )]);
    let error = validate_root_census(&candidates, &[], false).unwrap_err();
    assert_eq!(
        error,
        ReplayIndexGenerationError::InvalidRootSlot("PresentOnlyInSource".to_owned())
    );
}

#[test]
fn replay_root_census_rejects_an_inexact_slot_set() {
    let candidates = BTreeMap::from([(
        "Merged".to_owned(),
        root_candidate([SourceBindingSlot::Value, SourceBindingSlot::Type]),
    )]);
    let rows = [RootNameRow {
        name: "Merged".to_owned(),
        value: Some(ValueStorageId(0)),
        ty: None,
        namespace: None,
    }];
    let error = validate_root_census(&candidates, &rows, false).unwrap_err();
    assert_eq!(
        error,
        ReplayIndexGenerationError::InvalidRootSlot("Merged".to_owned())
    );
}

#[test]
fn replay_root_census_requires_explicit_global_this_to_publish() {
    let error = validate_root_census(&BTreeMap::new(), &[], true).unwrap_err();
    assert_eq!(
        error,
        ReplayIndexGenerationError::InvalidRootSlot("globalThis".to_owned())
    );
}

#[test]
fn instantiated_namespace_expectation_does_not_depend_on_storage_or_root_row() {
    let mut candidate = root_candidate([SourceBindingSlot::Namespace]);
    apply_merge_root_semantics(
        &mut candidate,
        BTreeSet::from([SourceBindingSlot::Namespace]),
        false,
        true,
    );
    assert_eq!(
        candidate.slots,
        BTreeSet::from([SourceBindingSlot::Value, SourceBindingSlot::Namespace])
    );
    assert!(candidate.global_object_contributor);

    let candidates = BTreeMap::from([("RuntimeNamespace".to_owned(), candidate)]);
    let rows = [RootNameRow {
        name: "RuntimeNamespace".to_owned(),
        value: None,
        ty: None,
        namespace: Some(NamespaceId(0)),
    }];
    assert_eq!(
        validate_root_census(&candidates, &rows, false).unwrap_err(),
        ReplayIndexGenerationError::InvalidRootSlot("RuntimeNamespace".to_owned())
    );
}

#[test]
fn replay_root_normalization_covers_pure_and_mixed_namespace_semantics() {
    let source = r#"
            declare namespace PureType { interface Member {} }
            declare namespace PureRuntime { const value: number; }
            interface InterfaceMerge {}
            declare namespace InterfaceMerge { interface Member {} }
            declare class ClassMerge {}
            declare namespace ClassMerge { interface Member {} }
            declare function FunctionMerge(): void;
            declare namespace FunctionMerge { interface Member {} }
        "#;
    let state = compile_reservation_fixture("root-normalization.d.ts", source);
    let replay = state
        .replay_index()
        .expect("source compiler retains its replay index");
    let root = |name: &str| {
        replay
            .root_slots
            .iter()
            .find(|root| root.name == name)
            .unwrap_or_else(|| panic!("missing replay root {name}"))
    };
    let slots = |name: &str| {
        let root = root(name);
        (
            root.value.is_some(),
            root.ty.is_some(),
            root.namespace.is_some(),
            root.global_object_contributor,
        )
    };
    assert_eq!(slots("PureType"), (false, false, true, false));
    assert_eq!(slots("PureRuntime"), (true, false, true, true));
    assert_eq!(slots("InterfaceMerge"), (false, true, true, false));
    assert_eq!(slots("ClassMerge"), (true, true, true, false));
    assert_eq!(slots("FunctionMerge"), (true, false, true, true));

    let binding_span = |name: &str| {
        let start = u32::try_from(source.find(name).expect("fixture binding")).unwrap();
        Span::new(start, start + u32::try_from(name.len()).unwrap())
    };
    let has_global_site = |span: Span| {
        replay
            .owner_sites
            .iter()
            .any(|site| site.owner == ReplayOwner::GlobalObject && site.span == span)
    };
    assert!(!has_global_site(binding_span("PureType")));
    assert!(has_global_site(binding_span("PureRuntime")));
}

#[test]
fn object_alias_identity_ignores_member_source_order() {
    let state = compile_reservation_fixture(
        "object-alias-member-order.d.ts",
        r#"
                type OrderedFirst = { alpha: string; beta: number };
                type OrderedSecond = { beta: number; alpha: string };
            "#,
    );

    assert_eq!(
        published_template_type(&state, "OrderedFirst"),
        published_template_type(&state, "OrderedSecond"),
        "member source order is not structural identity"
    );
    assert_strict_interner_state(&state, "object alias member order");
}

#[test]
fn object_alias_identity_keeps_identity_bearing_metadata() {
    let state = compile_reservation_fixture(
        "object-alias-metadata.d.ts",
        r#"
                type OptionalValue = { value?: number };
                type RequiredValue = { value: number };
                type ReadonlyValue = { readonly value: number };
                type MutableValue = { value: number };
                type StringIndexed = { [key: string]: number };
                type NumberIndexed = { [key: number]: number };
                type Callable = { (input: string): boolean };
                type Constructable = { new (input: string): { value: boolean } };
            "#,
    );

    for (left, right) in [
        ("OptionalValue", "RequiredValue"),
        ("ReadonlyValue", "MutableValue"),
        ("StringIndexed", "NumberIndexed"),
        ("Callable", "Constructable"),
    ] {
        assert_ne!(
            published_template_type(&state, left),
            published_template_type(&state, right),
            "{left} and {right} differ in identity-bearing metadata"
        );
    }
    assert_strict_interner_state(&state, "object alias identity metadata");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "large object-alias scaling is a release-only gate"
)]
fn acyclic_object_alias_canonicalization_scales_linearly() {
    fn compile_scale(count: usize) -> (Duration, usize, usize) {
        let mut source = String::new();
        for index in 0..count {
            use std::fmt::Write;
            writeln!(
                source,
                "type Scale{index:05} = {{ value: number; run(input: string): boolean }};"
            )
            .expect("write scale fixture");
        }
        let (run, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "object-alias-scale.d.ts",
            source: &source,
        }])
        .expect("object-alias scale fixture compiles");
        let identities = state
            .binder
            .type_groups
            .iter()
            .filter(|group| group.name.starts_with("Scale"))
            .map(|group| match state.published_types.groups().get(group.id) {
                Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                    surface: PublishedTypeGroupSurface::Template(ty),
                    ..
                })) => *ty,
                terminal => panic!("scale alias did not publish: {terminal:?}"),
            })
            .collect::<rustc_hash::FxHashSet<_>>();
        assert_strict_interner_state(&state, "object alias scale");
        eprintln!(
                "typokat-object-alias-scale-v1 aliases={count} store_rows={} reserve_fill_us={} unique_published={}",
                state.interner.store().len(),
                run.phase_timings.reserve_fill.as_micros(),
                identities.len(),
            );
        (
            run.phase_timings.reserve_fill,
            state.interner.store().len(),
            identities.len(),
        )
    }

    let (small_time, small_rows, small_unique) = compile_scale(1_000);
    let (large_time, large_rows, large_unique) = compile_scale(10_000);
    assert_eq!((small_unique, large_unique), (1, 1));
    assert!(large_rows < small_rows.saturating_mul(12));
    assert!(
            large_time <= small_time.saturating_mul(20),
            "10x aliases must not approach quadratic reserve/fill time: {small_time:?} -> {large_time:?}"
        );
}

#[test]
fn focused_profile_selects_complete_native_bridge_identities() {
    let run = run_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "native-bridges.d.ts",
        source: r#"
                interface Array<T> { mapResult: T; }
                interface ReadonlyArray<T> { mapResult: T; }
                interface String { toUpperCaseResult: string; }
                interface Number { toFixedResult: string; }
                interface Boolean { valueOfResult: boolean; }
                interface RegExp { testResult: boolean; }
                interface Object { toStringResult: string; }
                interface Function { legacyCallResult: string; }
                interface CallableFunction extends Function { callResult: number; }
            "#,
    }])
    .expect("focused source-compiled library profile");
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
        let super::super::library_identities::LibraryIdentityTerminal::Ready(identity) = terminal
        else {
            panic!("focused native bridge identity must be ready")
        };
        assert_eq!(identity.parameters.len(), expected_arity);
    }
}

#[test]
fn owned_runtime_product_parts_restore_a_consumable_base() {
    let parts = compile_semantic_product_parts();
    assert_eq!(parts.source_file_count, 1);
    assert!(parts.semantic_identities.is_some());
    assert!(!parts.runtime.class_application_parameters.is_empty());
    assert!(!parts.runtime.class_new_metadata.is_empty());
    assert!(!parts.runtime.class_value_bindings.is_empty());
    assert!(!parts.runtime.namespace_terminals.is_empty());
    assert!(!parts.runtime.named_function_symbols.is_empty());
    let restored =
        OwnedLibraryRuntimeState::from_product_parts(parts).expect("restore product parts");
    let user = check_caller_certified_collision_free_source_with_owned_library(
        restored,
        r#"
                declare const witness: SeamWitness;
                const witnessValue: number = witness.value;
                declare const nativeValues: number[];
                const nativeArrayItem: number = nativeValues.item;
                const nativeStringMarker: string = "value".stringMarker;
                const instance = new SeamClass<number>(1);
                const classValue: number = instance.value;
                const inheritedValue: string = instance.base;
                const base = new SeamBase();
                const directBaseValue: string = base.base;
                const classTag: string = SeamClass.tag;
                const enabled: boolean = SeamSpace.enabled;
                const called: string = seamNamed(1);
                const functionVersion: number = seamNamed.version;
            "#,
    )
    .expect("consume restored base");

    assert!(user.result.diagnostics.is_empty());
    assert!(user.result.incomplete.is_empty());
    assert_eq!(user.witness.base_max_source_key, 1);
}

#[test]
fn owned_runtime_product_parts_restore_native_bridge_error_behavior() {
    let restored = OwnedLibraryRuntimeState::from_product_parts(compile_semantic_product_parts())
        .expect("restore product parts");
    let user = check_caller_certified_collision_free_source_with_owned_library(
        restored,
        r#"
                declare const nativeValues: number[];
                const wrongNativeItem: string = nativeValues.item;
            "#,
    )
    .expect("consume restored native bridge");

    assert_eq!(user.result.diagnostics.len(), 1);
    assert_eq!(
        user.result.diagnostics[0].code,
        crate::diagnostics::DiagnosticCode::TK2322
    );
    assert!(user.result.incomplete.is_empty());
}

#[test]
fn owned_runtime_product_parts_reject_empty_and_gapped_source_prefixes() {
    let mut empty = compile_semantic_product_parts();
    replace_module_sources(&mut empty, Default::default());
    assert_product_restore_error(empty, "product binder has no retained source ownership");

    let mut gapped = compile_semantic_product_parts();
    let mut sources = gapped
        .binder
        .module_sources()
        .iter()
        .map(|(&scope, &source)| (scope, source))
        .collect::<rustc_hash::FxHashMap<_, _>>();
    let library_scope = sources
        .iter()
        .find_map(|(scope, source)| (source.0 == 1).then_some(*scope))
        .expect("library source owner");
    sources.insert(library_scope, exact_key(2));
    replace_module_sources(&mut gapped, sources);
    assert_product_restore_error(
        gapped,
        "product binder source keys are not the contiguous prelude/library prefix",
    );
}

#[rustfmt::skip]
#[test]
fn owned_runtime_product_parts_reject_group_and_counter_collisions() {
        let mut groups = compile_semantic_product_parts();
        groups.published_types.groups.pop();
        assert_product_restore_error(
            groups,
            "product published type-group count does not match the binder",
        );

        let mut type_parameters = compile_semantic_product_parts();
        type_parameters.next_type_param = 0;
        assert_product_restore_error(
            type_parameters,
            "product type parameter id collides with the next parameter counter",
        );

        let mut classes = compile_semantic_product_parts();
        classes.next_class_id = 0;
        assert_product_restore_error(
            classes,
            "product class id collides with the next class counter",
        );

        let mut renamed = compile_semantic_product_parts();
        let template_group = renamed
            .published_types
            .groups
            .iter_mut()
            .find_map(|terminal| match terminal {
                PublishedTypeGroupTerminal::Ready(group)
                    if group.name == "SeamWitness"
                        && matches!(group.surface, PublishedTypeGroupSurface::Template(_)) =>
                {
                    Some(group)
                }
                PublishedTypeGroupTerminal::Ready(_)
                | PublishedTypeGroupTerminal::Unavailable(_) => None,
            })
            .expect("ready template group");
        template_group.name = "RenamedSeamWitness".to_owned();
        assert_product_restore_error(
            renamed,
            "product published type-group name does not match the binder",
        );
    }

#[test]
fn owned_runtime_product_parts_reject_malformed_alias_rows_and_ownership() {
    let mut invalid_body = compile_semantic_product_parts();
    let (_, terminal) = invalid_body
        .published_types
        .aliases
        .iter_mut()
        .find(|(_, terminal)| matches!(terminal, PublishedAliasTerminal::Ready(_)))
        .expect("ready alias publication");
    let PublishedAliasTerminal::Ready(surface) = terminal else {
        unreachable!("selected ready alias")
    };
    *surface = typokat_types::alias_semantics::PublishedAliasSurface::new(
        surface.alias(),
        surface.template(),
        surface.type_params().to_vec(),
        TypeId(u32::MAX),
    );
    assert_product_restore_error(
        invalid_body,
        "product published alias surface has an invalid reference",
    );

    let mut invalid_parameters = compile_semantic_product_parts();
    let (_, terminal) = invalid_parameters
        .published_types
        .aliases
        .iter_mut()
        .find(|(_, terminal)| matches!(terminal, PublishedAliasTerminal::Ready(_)))
        .expect("ready alias publication");
    let PublishedAliasTerminal::Ready(surface) = terminal else {
        unreachable!("selected ready alias")
    };
    *surface = typokat_types::alias_semantics::PublishedAliasSurface::new(
        surface.alias(),
        surface.template(),
        vec![TypeParamId(u32::MAX)],
        surface.body(),
    );
    assert_product_restore_error(
        invalid_parameters,
        "product published alias surface has an invalid reference",
    );

    let mut missing_owner = compile_semantic_product_parts();
    missing_owner
        .published_types
        .aliases
        .retain(|(_, terminal)| !matches!(terminal, PublishedAliasTerminal::Ready(_)));
    assert_product_restore_error(
        missing_owner,
        "product alias type group has no ready alias publication",
    );
}

#[rustfmt::skip]
#[test]
    fn owned_runtime_product_parts_reject_missing_required_class_rows() {
        let mut new_metadata = compile_semantic_product_parts();
        let removed_class = new_metadata
            .runtime
            .class_new_metadata
            .pop()
            .expect("new metadata row")
            .0;
        assert!(new_metadata
            .runtime
            .class_value_bindings
            .iter()
            .any(|(_, binding)| binding.class_id == removed_class));
        assert_product_restore_error(
            new_metadata,
            "product new metadata does not exactly cover published classes",
        );

        let mut applications = compile_semantic_product_parts();
        applications
            .runtime
            .class_application_parameters
            .pop()
            .expect("class application row");
        assert_product_restore_error(
            applications,
            "product class application metadata does not exactly cover published classes",
        );

        let mut names = compile_semantic_product_parts();
        names.runtime.class_names.pop().expect("class name row");
        assert_product_restore_error(
            names,
            "product class names do not exactly match published class groups",
        );

        let mut bindings = compile_semantic_product_parts();
        bindings
            .runtime
            .class_value_bindings
            .pop()
            .expect("class value binding row");
        assert_product_restore_error(
            bindings,
            "product class value bindings do not exactly cover published classes",
        );

        let mut parent_chain = compile_semantic_product_parts();
        let inherited = parent_chain
            .runtime
            .class_new_metadata
            .iter()
            .find_map(|(class, metadata)| {
                (*class != metadata.ctor_declaring_class).then_some(*class)
            })
            .expect("class with inherited constructor owner");
        let parent_row = parent_chain
            .runtime
            .class_parents
            .iter()
            .position(|(class, _)| *class == inherited)
            .expect("inherited class parent row");
        parent_chain.runtime.class_parents.remove(parent_row);
        assert_product_restore_error(
            parent_chain,
            "product constructor owner is not on the class parent chain",
        );

        let mut wrong_name = compile_semantic_product_parts();
        wrong_name.runtime.class_names[0].1 = "DifferentClass".to_owned();
        assert_product_restore_error(
            wrong_name,
            "product class names do not exactly match published class groups",
        );

        let mut generic_bit = compile_semantic_product_parts();
        let generic_class = generic_bit
            .runtime
            .class_application_parameters
            .iter()
            .find_map(|(class, parameters)| (!parameters.is_empty()).then_some(*class))
            .expect("generic class application");
        let binding = generic_bit
            .runtime
            .class_value_bindings
            .iter_mut()
            .find_map(|(_, binding)| (binding.class_id == generic_class).then_some(binding))
            .expect("generic class value binding");
        binding.has_header_type_params = !binding.has_header_type_params;
        assert_product_restore_error(
            generic_bit,
            "product class value binding generic bit does not match application metadata",
        );
    }

#[test]
fn owned_runtime_product_parts_reject_dangling_semantic_and_type_references() {
    let mut semantic = compile_semantic_product_parts();
    let LibraryIdentityTerminal::Ready(identity) = &mut semantic
        .semantic_identities
        .as_mut()
        .expect("semantic identities")[0]
    else {
        panic!("Array identity is ready")
    };
    identity.group = TypeGroupId(u32::MAX);
    assert_product_restore_error(
        semantic,
        "product semantic identity refers to an unpublished type group",
    );

    let mut types = compile_semantic_product_parts();
    let ready = types
        .runtime
        .namespace_terminals
        .iter_mut()
        .find_map(|row| match &mut row.terminal {
            FrozenNamespaceValueTerminalSnapshot::Ready { ty, .. } => Some(ty),
            FrozenNamespaceValueTerminalSnapshot::Unavailable(_) => None,
        })
        .expect("ready namespace terminal");
    *ready = TypeId(u32::MAX);
    assert_product_restore_error(
        types,
        "product namespace terminal has an invalid ready reference",
    );
}

#[test]
fn owned_runtime_product_parts_reject_dangling_runtime_binder_references() {
    let mut values = compile_semantic_product_parts();
    values
        .runtime
        .class_value_bindings
        .first_mut()
        .expect("class value binding")
        .0 = ValueStorageId(u32::MAX);
    assert_product_restore_error(
        values,
        "product class value binding has an invalid reference",
    );

    let mut symbols = compile_semantic_product_parts();
    symbols.runtime.named_function_symbols[0] = SymbolId(u32::MAX);
    assert_product_restore_error(
        symbols,
        "product named-function metadata refers to a non-function symbol",
    );

    let mut namespaces = compile_semantic_product_parts();
    namespaces.runtime.namespace_terminals[0].namespace =
        crate::binder::namespace::NamespaceId(u32::MAX);
    assert_product_restore_error(
        namespaces,
        "product namespace terminal refers to an unknown namespace",
    );
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
    assert_eq!(run.library_records[0].0.source_start, 23);
    assert_eq!(diagnostic.span.start, 6);
}

/// The route to a shared base drops the library's records; only an explicit inspection
/// caller collects them (ADR-0018). The ledger is still drained on both routes — that drain
/// is the completeness gate, not the retention.
#[test]
fn the_base_route_retains_no_record_while_the_census_route_collects_them() {
    let injected = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(7),
        name: "broken.ts",
        source: "const broken: number = 'wrong';",
    }];

    let (base_run, _base_state) =
        compile_owned_injected_base_profile(&injected).expect("base-route injected profile");
    assert!(base_run.library_records.is_empty());
    assert_eq!(
        base_run.phase_counts.reserved_records,
        base_run.phase_counts.filled_records
    );

    let collected =
        compile_owned_injected_records(&injected).expect("census-route injected profile");
    assert_eq!(collected.len(), 1);
    assert!(matches!(collected[0].1, CheckerRecord::Diagnostic(_)));
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
        "library export-context TK1319 reporting is deferred beyond snapshot feasibility"
    );
    assert_eq!(incomplete.span, Span::new(29, 63));
    assert_eq!(key.source_start, incomplete.span.start);
}

#[test]
fn mixed_ts1319_and_ts1063_diagnostics_fail_closed() {
    let file_ordinal = LibraryFileOrdinal::new(15);
    let source = "declare namespace Mixed { export default function f(): void; export = Mixed; }";
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

const OWNED_MINI_LIBRARY: &str = r#"
        interface Object { toString(): string; }
        interface Function {}
        interface CallableFunction extends Function {
            call<T, A extends unknown[], R>(this: (this: T, ...args: A) => R, thisArg: T, ...args: A): R;
        }
        interface String { toUpperCase(): string; }
        interface Number { toFixed(fractionDigits?: number): string; }
        interface Boolean { valueOf(): boolean; }
        interface Array<T> {
            map<U>(callbackfn: (value: T) => U): U[];
            push(...items: T[]): number;
        }
        interface ReadonlyArray<T> { map<U>(callbackfn: (value: T) => U): U[]; }
        interface RegExp { test(value: string): boolean; }
        interface HTMLElement {}
        interface HTMLDivElement extends HTMLElement { align: string; }
        interface HTMLElementTagNameMap { div: HTMLDivElement; }
        interface ElementCreationOptions {}
        interface Document {
            createElement<K extends keyof HTMLElementTagNameMap>(
                tagName: K,
                options?: ElementCreationOptions,
            ): HTMLElementTagNameMap[K];
        }
        declare var document: Document;
        declare function increment(value: number): number;
        declare namespace increment { const identity: string; }
        declare class LibraryBase<T = string> {
            constructor(value: T);
            value: T;
        }
        declare namespace LibraryBase { const kind: string; }
        declare class PrivateLibraryClass { private constructor(); }
        declare namespace RuntimeBag { const answer: number; }
    "#;

const OWNED_MINI_USER: &str = r#"
        class UserSuffix {}
        const mappedOk: string[] = [1].map(value => value.toFixed());
        const mappedBad: number[] = [1].map(value => value.toFixed());
        const readonlyValues: readonly number[] = [1, 2];
        const readonlyBad: number[] = readonlyValues.map(value => value.toFixed());
        readonlyValues.push(3);
        const upperBad: number = "x".toUpperCase();
        const fixedBad: number = (1).toFixed();
        const booleanBad: string = true.valueOf();
        const calledBad: number = ((value: string) => value).call(undefined, "x");
        const objectBad: number = ({ value: 1 }).toString();
        const regexpBad: string = /x/.test("x");
        const domOk: HTMLDivElement = document.createElement("div");
        const domBad: number = document.createElement("div");
        const libraryClass = new LibraryBase<string>("x");
        const libraryValue: string = libraryClass.value;
        const libraryKind: string = LibraryBase.kind;
        const runtimeAnswer: number = RuntimeBag.answer;
        const RuntimeAlias = RuntimeBag;
        const aliasAnswer: number = RuntimeAlias.answer;
        increment.notAFunctionMember;
    "#;

fn object_overlap_scale_source(width: usize) -> String {
    let source_properties = (0..width)
        .map(|index| format!("  p{index:03}: [{index}];"))
        .collect::<Vec<_>>()
        .join("\n");
    let target_properties = (0..width)
        .map(|index| format!("  p{index:03}: Object;"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "interface ScaleSource {{\n{source_properties}\n}}\n\
             interface ScaleTarget {{\n{target_properties}\n}}\n\
             declare const source: ScaleSource;\n\
             const target: ScaleTarget = source;\n"
    )
}

fn object_overlap_relation_frames(width: usize) -> Result<u64, String> {
    let library = format!(
        "{OWNED_MINI_LIBRARY}\n\
             interface Array<T> {{ readonly length: number; }}\n\
             interface ReadonlyArray<T> {{ readonly length: number; }}\n"
    );
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "object-overlap-scale.d.ts",
        source: &library,
    }])
    .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        crate::relate::relation::relation_source_cold_measure(),
        None
    );
    let guard = crate::relate::relation::start_relation_source_cold_measure();
    let run = check_caller_certified_collision_free_source_with_owned_library(
        state,
        &object_overlap_scale_source(width),
    )?;
    let work = crate::relate::relation::relation_source_cold_measure().unwrap_or_default();
    drop(guard);
    assert!(
        run.result.diagnostics.is_empty(),
        "{:?}",
        run.result.diagnostics
    );
    assert!(
        run.result.incomplete.is_empty(),
        "{:?}",
        run.result.incomplete
    );
    Ok(work.uncached_relation_frames)
}

#[test]
fn object_overlap_demand_retries_do_not_rescan_named_property_prefixes() -> Result<(), String> {
    const SMALL: usize = 16;
    const LARGE: usize = 32;

    let small = object_overlap_relation_frames(SMALL)?;
    let large = object_overlap_relation_frames(LARGE)?;
    assert!(small > 0, "the scale guard must observe relation work");
    assert!(
        large <= small.saturating_mul(3),
        "doubling Object-overlap width must stay sub-quadratic: small={small}, large={large}"
    );
    Ok(())
}

#[test]
fn owned_generic_promise_identity_preserves_resolve_argument() {
    let library = r#"
            type SeamAwaited<T> = T;
            interface SeamPromise<T> {
                then<Result = T>(
                    onfulfilled?: ((value: T) => Result) | null,
                ): SeamPromise<Result>;
            }
            interface SeamPromiseConstructor {
                resolve<T>(value: T): SeamPromise<SeamAwaited<T>>;
            }
            declare var SeamPromise: SeamPromiseConstructor;
        "#;
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "promise-identity.d.ts",
        source: library,
    }])
    .expect("focused Promise identity library compiles");
    let run = check_caller_certified_collision_free_source_with_owned_library(
        state,
        r#"
                declare const awaited: SeamAwaited<number>;
                const wrongAwaited: string = awaited;
                const resolved = SeamPromise.resolve(1);
                const numberControl: SeamPromise<number> = resolved;
                resolved.then(value => {
                    const valueControl: number = value;
                    const wrongValue: string = value;
                    return value;
                });
                const wrongPromise: SeamPromise<string> = resolved;
            "#,
    )
    .expect("focused Promise suffix checks");
    assert!(
        run.result.incomplete.is_empty(),
        "{:?}",
        run.result.incomplete
    );
    assert_eq!(
        run.result.diagnostics.len(),
        3,
        "{:?}",
        run.result.diagnostics
    );
    assert_eq!(
        run.result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
        ],
    );
}

#[test]
fn owned_conditional_awaited_preserves_non_thenable_argument() {
    let library = r#"
            type SeamAwaited<T> = T extends null | undefined ? T :
                T extends object & { then(onfulfilled: infer F, ...args: infer _): any; } ?
                    F extends ((value: infer V, ...args: infer _) => any) ?
                        SeamAwaited<V> :
                    never :
                T;
            interface SeamPromise<T> {
                then<Result = T>(
                    onfulfilled?: ((value: T) => Result) | null,
                ): SeamPromise<Result>;
            }
            interface SeamPromiseConstructor {
                resolve<T>(value: T): SeamPromise<SeamAwaited<T>>;
            }
            declare var SeamPromise: SeamPromiseConstructor;
        "#;
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "promise-awaited.d.ts",
        source: library,
    }])
    .expect("focused conditional Awaited library compiles");
    let run = check_caller_certified_collision_free_source_with_owned_library(
        state,
        r#"
                declare const awaited: SeamAwaited<number>;
                const wrongAwaited: string = awaited;
                const resolved = SeamPromise.resolve(1);
                const numberControl: SeamPromise<number> = resolved;
                resolved.then(value => {
                    const valueControl: number = value;
                    const wrongValue: string = value;
                    return value;
                });
                const wrongPromise: SeamPromise<string> = resolved;
            "#,
    )
    .expect("focused conditional Awaited suffix checks");
    assert!(
        run.result.incomplete.is_empty(),
        "{:?}",
        run.result.incomplete
    );
    assert_eq!(
        run.result.diagnostics.len(),
        3,
        "{:?}",
        run.result.diagnostics
    );
    assert_eq!(
        run.result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
        ],
    );
}

fn owned_state_identity_ends(state: &OwnedLibraryRuntimeState) -> OwnedBaseFinalIdentityEnds {
    OwnedBaseFinalIdentityEnds {
        store: state.interner.store().len(),
        declared_recipes: state.interner.store().all_declared_recipes().count(),
        type_params: usize::try_from(state.next_type_param)
            .expect("base type parameter end fits usize"),
        classes: usize::try_from(state.next_class_id).expect("base class end fits usize"),
        scopes: state.binder.graph.len(),
        symbols: state.binder.symbols.len(),
        declarations: state.binder.declarations.len(),
        type_groups: state.binder.type_groups.len(),
        namespaces: state.binder.namespaces.len(),
        value_storages: state.decl_types.len(),
        source_units: state.binder.checkpoint_ends().next_source,
    }
}

#[test]
fn owned_runtime_forks_share_checker_prefixes_and_keep_empty_private_suffixes() {
    let (_, mut base) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "owned-sharing.d.ts",
        source: OWNED_MINI_LIBRARY,
    }])
    .expect("owned sharing library compiles");
    base.freeze_as_library_base().expect("owned base seals");
    let first = base
        .fork_user_delta_for_test()
        .expect("first owned checker suffix");
    let second = base
        .fork_user_delta_for_test()
        .expect("second owned checker suffix");

    assert!(first.shares_checker_base_with(&second));
    assert_eq!(first.decl_types.local_len(), 0);
    assert_eq!(second.decl_types.local_len(), 0);
    assert_eq!(first.decl_types.len(), base.decl_types.len());
    assert_eq!(
        second.published_types.groups().len(),
        base.published_types.groups().len()
    );
}

#[test]
fn owned_base_final_identity_witness_uses_real_aliases_dense_suffixes_and_base_shapes() {
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "owned-final-identities.d.ts",
        source: OWNED_MINI_LIBRARY,
    }])
    .expect("owned final-identity library compiles");
    let base = owned_state_identity_ends(&state);
    let run = check_caller_certified_collision_free_source_with_owned_library(
        state,
        r#"
                export type FirstLibraryAlias = LibraryBase<string>;
                export type SecondLibraryAlias = LibraryBase<string>;
                export declare namespace HonestLocalSpace {
                    export interface HonestLocalInterface<T> { value: T; }
                    export class HonestLocalClass<U> {
                        value: U;
                    }
                    export const localValue: HonestLocalClass<number>;
                }
                export const honestReusedFunction = increment;
            "#,
    )
    .expect("owned base captures final identity state");
    assert!(
        run.result.diagnostics.is_empty(),
        "{:?}",
        run.result.diagnostics
    );
    assert!(
        run.result.incomplete.is_empty(),
        "{:?}",
        run.result.incomplete
    );

    let aliases = &run.final_identity.named_alias_types;
    assert_eq!(
        aliases.get("FirstLibraryAlias"),
        aliases.get("SecondLibraryAlias")
    );
    assert!(aliases.get("FirstLibraryAlias").is_some());
    for (base_end, final_end) in [
        (base.store, run.final_identity.ends.store),
        (base.type_params, run.final_identity.ends.type_params),
        (base.classes, run.final_identity.ends.classes),
        (base.scopes, run.final_identity.ends.scopes),
        (base.symbols, run.final_identity.ends.symbols),
        (base.declarations, run.final_identity.ends.declarations),
        (base.type_groups, run.final_identity.ends.type_groups),
        (base.namespaces, run.final_identity.ends.namespaces),
        (base.value_storages, run.final_identity.ends.value_storages),
    ] {
        assert!(final_end > base_end, "{base_end}..{final_end}");
    }
    let reused = run
        .final_identity
        .reused_base_shape
        .expect("user lowering reuses a non-intrinsic base shape");
    assert!(reused.type_id.index() < base.store);
    assert!(matches!(
        reused.tag,
        TypeTag::Object | TypeTag::ClassInstance | TypeTag::Function
    ));
}

#[test]
fn production_check_program_does_not_enter_final_identity_inspection_route() {
    let allocator = Allocator::default();
    let parsed = Parser::new(
        &allocator,
        "type ProductionAlias = { value: number }; const value: ProductionAlias = { value: 1 };",
        SourceType::ts(),
    )
    .parse();
    assert!(!parsed.panicked);
    assert!(parsed.diagnostics.is_empty());
    let inspections_before = super::super::final_identity_inspector_calls_for_test();
    let mut interner = Interner::with_intrinsics();
    let result = super::super::check_program(&mut interner, &parsed.program);
    assert!(result.diagnostics.is_empty());
    assert!(result.incomplete.is_empty());
    assert_eq!(
        super::super::final_identity_inspector_calls_for_test(),
        inspections_before
    );
}

#[test]
fn owned_library_state_checks_caller_certified_collision_free_suffix_without_prefix_rebinding() {
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "owned-mini.d.ts",
        source: OWNED_MINI_LIBRARY,
    }])
    .expect("owned mini library compiles");
    let run = check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix(
        state,
        OWNED_MINI_USER,
    )
    .expect("owned mini base accepts a caller-certified collision-free suffix");
    let codes = run
        .result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
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
            crate::diagnostics::DiagnosticCode::TK2339,
        ]
    );
    assert_eq!(
        run.result
            .diagnostics
            .last()
            .map(|diagnostic| diagnostic.message.as_str()),
        Some("Property 'notAFunctionMember' does not exist on type 'typeof increment'")
    );
    assert!(run.result.incomplete.is_empty());
    assert!(run.witness.array_group_stable);
    assert!(run.witness.document_value_stable);
    assert_eq!(run.witness.store_prefix_stable, Some(true));
    assert!(run.witness.final_store_len >= run.witness.base_store_len);
    assert!(run.witness.final_type_group_count > run.witness.base_type_group_count);
    assert!(run.witness.final_decl_count > run.witness.base_decl_count);
    assert!(run.witness.source_key > run.witness.base_max_source_key);
}

#[test]
fn incomplete_collision_identity_selection_emits_no_user_diagnostics() {
    let (_, mut base) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "owned-mini.d.ts",
        source: OWNED_MINI_LIBRARY,
    }])
    .expect("owned mini library compiles");
    base.freeze_as_library_base().expect("owned base seals");
    let collision = base
        .fork_sparse_collision_epoch()
        .expect("sparse collision epoch forks");
    let run = super::super::library_identities::
            with_forced_collision_identity_selection_pending(|| {
                check_caller_certified_collision_free_source_with_owned_library(
                    collision,
                    "interface Array<T> { replacementMarker(): T }\nconst wrong: string = [1].replacementMarker();\n",
                )
            })
            .expect("collision publication returns a typed incomplete outcome");

    assert!(
        run.result.diagnostics.is_empty(),
        "{:?}",
        run.result.diagnostics
    );
    assert_eq!(
        run.result.incomplete.len(),
        1,
        "{:?}",
        run.result.incomplete
    );
    assert_eq!(
        run.result.incomplete[0].id,
        "library/publication/semantic-identity-selection-pending"
    );
}

#[test]
fn owned_frozen_interface_heritage_projects_into_local_interface() {
    let (_, state) = compile_owned_injected_profile(&[
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-mini.d.ts",
            source: OWNED_MINI_LIBRARY,
        },
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(1),
            name: "html-element.d.ts",
            source: "interface HTMLElement { elementMarker: string; }",
        },
    ])
    .expect("owned HTMLElement library compiles");
    let run = check_caller_certified_collision_free_source_with_owned_library(
        state,
        r#"interface Local extends HTMLElement { localMarker: number; }
declare const local: Local;
const inheritedGood: string = local.elementMarker;
const inheritedBad: number = local.elementMarker;
"#,
    )
    .expect("local interface composes its frozen heritage endpoint");

    assert_eq!(run.result.diagnostics.len(), 1);
    assert_eq!(
        run.result.diagnostics[0].code,
        crate::diagnostics::DiagnosticCode::TK2322,
    );
    assert!(
        run.result.incomplete.is_empty(),
        "{:#?}",
        run.result.incomplete
    );
}

#[test]
fn owned_library_preserves_class_and_namespace_runtime_metadata_for_caller_certified_collision_free_suffix(
) {
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "owned-mini.d.ts",
        source: OWNED_MINI_LIBRARY,
    }])
    .expect("owned mini library compiles");
    let run = check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix(
        state,
        r#"
                class UserFirst extends LibraryBase<string> { constructor() { super("x"); } }
                const explicit = new LibraryBase<string>("x");
                const value: string = explicit.value;
                const kind: string = LibraryBase.kind;
                const answer: number = RuntimeBag.answer;
                const RuntimeAlias = RuntimeBag;
                const aliasAnswer: number = RuntimeAlias.answer;
                new PrivateLibraryClass();
            "#,
    )
    .expect("owned class and namespace metadata installs");
    assert_eq!(run.result.diagnostics.len(), 1);
    assert_eq!(
        run.result.diagnostics[0].code,
        crate::diagnostics::DiagnosticCode::TK2673
    );
    assert!(run.result.incomplete.is_empty());
    assert_eq!(run.witness.store_prefix_stable, Some(true));
}

#[test]
fn owned_library_continuation_admits_declare_global_syntax() {
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "owned-mini.d.ts",
        source: OWNED_MINI_LIBRARY,
    }])
    .expect("owned mini library compiles");
    let checks_before = super::super::bound_user_check_calls_for_test();
    let run = check_caller_certified_collision_free_source_with_owned_library(
        state,
        "export {}; declare global { interface UserOwnedGlobal { value: string; } }",
    )
    .expect("declare-global continuation binds and checks");
    assert!(run.result.diagnostics.is_empty());
    assert!(run.result.incomplete.is_empty());
    assert_eq!(
        super::super::bound_user_check_calls_for_test(),
        checks_before + 1
    );
}
