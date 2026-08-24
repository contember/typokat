//! WU4 contract for project-local demand-driven default-library semantics.

use crate::base::UserDeltaProjectInputForTest;
use crate::provider::{
    CompleteSourceSemanticDemandOracleForTest, LibraryBaseProvider, ProjectLocalCacheWriteForTest,
    ProjectLocalDependencyEdgeForTest, ProjectLocalForceAllTraceForTest, ProjectLocalOrderForTest,
    ProjectLocalPublicationSccCommitForTest, ProjectLocalRecordKindForTest,
    ProjectLocalRepeatedDemandForTest, ProjectLocalRootConsumerForTest, ProjectLocalSccForTest,
    ProjectLocalSccPublicationForTest, ProjectLocalSemanticFaultForTest,
    ProjectLocalSemanticProbesForTest, ProjectLocalSemanticRequestForTest,
    ProjectLocalSemanticTraceForTest, ProjectLocalTerminalStateForTest,
    ProjectLocalUnavailableAuditForTest,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const ORACLE_BYTES: &[u8] = include_bytes!(
    "../../../tests/cases/sr_project_local_library_candidate/semantic-demand-oracle.json"
);
const ORACLE_SHA256: &str = "ca33cdd3312575990448e50f786cf03f009e04d4fe2d1a94308da66eb5f8d203";
const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const TYPESCRIPT_REVISION: &str = "050880ce59e30b356b686bd3144efe24f875ebc8";
const OFFICIAL_FRAME_PATH: &str = "conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatBetweenTupleAndArray.ts";
const OFFICIAL_FRAME_SHA256: &str =
    "8ea2fffa0e48d8727722bb2e52ba98a235d72dbe7ef9848ac355f8c5d82f6a68";

#[test]
fn official_semantic_demand_frame_is_exact_and_admitted() {
    let frame = include_bytes!(
        "../../../tests/cases/sr_project_local_library_candidate/official_assignment_compat_between_tuple_and_array.ts"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(frame)),
        OFFICIAL_FRAME_SHA256,
        "official demand frame bytes drifted from the pinned TypeScript source"
    );

    let manifest: Value = serde_json::from_str(include_str!(
        "../../../tooling/official-suite/corpus/manifest.json"
    ))
    .expect("valid official-suite manifest");
    assert_eq!(manifest["sha"], TYPESCRIPT_REVISION);
    assert_eq!(manifest["transport"]["revision"], TYPESCRIPT_REVISION);

    let scoreboard = include_str!("../../../tooling/official-suite/scoreboard.txt");
    let row = scoreboard
        .lines()
        .find(|row| row.ends_with(OFFICIAL_FRAME_PATH))
        .expect("official demand frame is present in the authenticated scoreboard");
    assert!(
        row.starts_with("IN\t"),
        "official demand frame left the admitted scoreboard: {row}"
    );
}

#[test]
fn publication_scc_oracle_preserves_the_locked_array_cycle() {
    let oracle = single_complete_source_oracle("array");
    assert_eq!(
        oracle.publication_sccs.len(),
        1,
        "typed publication oracle must contain the full authenticated SCC set"
    );
    let names = oracle.publication_sccs[0]
        .members
        .iter()
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["Array", "ArrayConstructor", "ConcatArray"],
        "the typed publication oracle lost the exact Array cycle"
    );
}

#[test]
fn final_global_contributor_oracle_includes_user_provenance() {
    let oracle = single_complete_source_oracle("global-this");
    assert_global_this_user_contributors(&oracle);
}

#[test]
fn complete_source_demand_oracle_matches_frozen_wu4_receipt() {
    let actual = complete_source_oracles();
    let force_all = force_all_value(&actual[0].1);
    for (case_id, oracle) in &actual {
        assert_eq!(
            force_all_value(oracle),
            force_all,
            "complete-source force_all drifted by user frame {case_id}"
        );
    }
    assert_complete_source_controls(&actual);
    let actual_value = json!({
        "schema": 2,
        "profile_identity": PROFILE_IDENTITY,
        "library_force_all": force_all,
        "cases": actual
            .iter()
            .map(|(case_id, oracle)| complete_source_case_value(case_id, oracle))
            .collect::<Vec<_>>()
    });
    if std::env::var_os("TYPOKAT_PRINT_WU4_ORACLE").is_some() {
        println!(
            "{}",
            serde_json::to_string_pretty(&actual_value).expect("serializable WU4 oracle")
        );
        return;
    }

    assert_eq!(format!("{:x}", Sha256::digest(ORACLE_BYTES)), ORACLE_SHA256);
    assert_eq!(
        actual_value,
        frozen_oracle(),
        "complete-source WU4 raw trace drifted"
    );
}

fn assert_complete_source_controls(actual: &[(&str, CompleteSourceSemanticDemandOracleForTest)]) {
    let mixed_forward = complete_source_case(actual, "mixed-forward");
    let mixed_reverse = complete_source_case(actual, "mixed-reverse");
    assert!(
        !mixed_forward.mixed_semantic_owner_rows.is_empty(),
        "the mixed Array group must have a positive mixed-origin witness"
    );
    assert_eq!(
        semantic_graph_projection(mixed_forward),
        semantic_graph_projection(mixed_reverse),
        "mixed-group source order changed the normalized semantic graph"
    );
    assert_ne!(
        rows_digest(&complete_source_case(actual, "isolation-a").user_semantic_owner_rows),
        rows_digest(&complete_source_case(actual, "isolation-b").user_semantic_owner_rows),
        "the two isolated projects must retain distinct augmentations"
    );
    for case_id in ["isolation-a", "isolation-b"] {
        let isolation = complete_source_case(actual, case_id);
        let mixed_array = isolation
            .mixed_semantic_owner_rows
            .iter()
            .find(|row| row.starts_with("type:\"Array\"@"))
            .expect("isolation project has one mixed project-local Array owner");
        assert!(
            isolation.semantic_closure_rows.contains(mixed_array),
            "{case_id} member resolution did not demand its mixed Array owner"
        );
    }
    let isolation_a_probe_rows =
        semantic_probe_rows(&complete_source_case(actual, "isolation-a").semantic_probes);
    let isolation_b_probe_rows =
        semantic_probe_rows(&complete_source_case(actual, "isolation-b").semantic_probes);
    assert!(
        isolation_a_probe_rows
            .iter()
            .any(|row| row.contains("projectAOnly"))
            && isolation_a_probe_rows
                .iter()
                .all(|row| !row.contains("projectBOnly")),
        "isolation A semantic probes contain a foreign augmentation"
    );
    assert!(
        isolation_b_probe_rows
            .iter()
            .any(|row| row.contains("projectBOnly"))
            && isolation_b_probe_rows
                .iter()
                .all(|row| !row.contains("projectAOnly")),
        "isolation B semantic probes contain a foreign augmentation"
    );
    let official =
        complete_source_case(actual, "official-assignment-compat-between-tuple-and-array");
    assert!(
        official
            .semantic_closure_rows
            .iter()
            .any(|row| row.starts_with("type:\"Array\"@")),
        "the authenticated upstream frame did not demand the packaged Array surface"
    );
    assert_eq!(
        official
            .user_check_results
            .iter()
            .map(|result| result.diagnostics.len())
            .sum::<usize>(),
        2,
        "the authenticated IN frame drifted from its exact tsc diagnostic count"
    );
    assert!(
        official
            .user_check_results
            .iter()
            .all(|result| result.incomplete.is_empty()),
        "the authenticated IN frame acquired a typokat incomplete surface"
    );

    let reflect = complete_source_case(actual, "reflect");
    assert!(
        reflect
            .root_rows
            .iter()
            .all(|row| !row.starts_with("root:\"keys\"|")),
        "the user-local keys binding entered the canonical library root set"
    );
    let reflect_value = root_slot_owner(&reflect.root_rows, "Reflect", "value=")
        .expect("the packaged Reflect value root exists");
    let reflect_consumers = reflect
        .root_consumers
        .iter()
        .filter(|consumer| {
            consumer.name == "Reflect"
                && consumer.slot == "value"
                && consumer.consumer.contains("keys")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reflect_consumers.len(),
        1,
        "the local keys owner has one exact packaged Reflect consumer"
    );
    assert!(
        reflect.reverse_edges.iter().any(|edge| {
            edge.dependency == reflect_value && edge.consumer == reflect_consumers[0].consumer
        }),
        "the packaged Reflect value dependency did not reach the local keys owner"
    );

    assert_global_this_user_contributors(complete_source_case(actual, "global-this"));
}

fn assert_global_this_user_contributors(global_this: &CompleteSourceSemanticDemandOracleForTest) {
    for name in ["B103GlobalThisValue", "B103GlobalThisCall"] {
        let contributor = global_this
            .global_contributors
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("final global object omitted user contributor {name}"));
        assert!(
            contributor.sites.iter().any(|site| {
                site.origin == crate::provider::ProjectLocalContributorOriginForTest::User
            }),
            "global contributor {name} lost user provenance"
        );
    }
}

#[test]
fn raw_trace_validator_rejects_each_known_broken_control() {
    let oracle = single_complete_source_oracle("array");
    let valid = valid_control_trace("array", &oracle, true);
    validate_candidate_trace(
        &valid,
        "array",
        &oracle,
        true,
        ProjectLocalOrderForTest::Forward,
        ProjectLocalOrderForTest::Forward,
    )
    .expect("raw control trace is valid");

    let mut omitted_edge = valid.clone();
    omitted_edge.reverse_edges.pop();
    assert_eq!(
        validate_candidate_trace(
            &omitted_edge,
            "array",
            &oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("candidate omitted or added a semantic dependency edge")
    );

    let mut premature = valid.clone();
    premature.publication_scc_commits[0].published_members.pop();
    assert_eq!(
        validate_candidate_trace(
            &premature,
            "array",
            &oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("terminal became observable before its whole demanded SCC was published")
    );

    let mut provisional_cache = valid.clone();
    provisional_cache.durable_cache_writes[0].state = ProjectLocalTerminalStateForTest::Resolving;
    assert_eq!(
        validate_candidate_trace(
            &provisional_cache,
            "array",
            &oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("Resolving state wrote a durable cache entry")
    );

    let mut stale_global = valid.clone();
    stale_global.published_global_contributors.pop();
    assert_eq!(
        validate_candidate_trace(
            &stale_global,
            "array",
            &oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("global object was not built from the final contributor set")
    );

    let mut wrong_source = valid.clone();
    wrong_source.source_inputs[0].sha256.push('0');
    assert_eq!(
        validate_candidate_trace(
            &wrong_source,
            "array",
            &oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("candidate did not execute the exact requested source inputs")
    );

    let mut ordinary_force_all = valid;
    ordinary_force_all
        .ordinary_force_all_invocations
        .push("ordinary-demand".to_owned());
    assert_eq!(
        validate_candidate_trace(
            &ordinary_force_all,
            "array",
            &oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("ordinary session invoked force_all")
    );

    let multi_source_oracle = single_complete_source_oracle("isolation-a");
    assert!(multi_source_oracle.source_visit_rows.len() > 1);
    let forward_only = valid_control_trace("isolation-a", &multi_source_oracle, true);
    assert_eq!(
        validate_candidate_trace(
            &forward_only,
            "isolation-a",
            &multi_source_oracle,
            true,
            ProjectLocalOrderForTest::Reverse,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("candidate did not execute the requested source and demand orders")
    );

    let mut shared_publisher = forward_only;
    shared_publisher.publisher_session_identity = 2;
    assert_eq!(
        validate_candidate_trace(
            &shared_publisher,
            "isolation-a",
            &multi_source_oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("candidate trace has no session or publisher identity")
    );

    let mut missing_semantic_probe = valid_control_trace("array", &oracle, true);
    missing_semantic_probe.semantic_probes.global_types.pop();
    assert_eq!(
        validate_candidate_trace(
            &missing_semantic_probe,
            "array",
            &oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("candidate semantic probes differ from complete-source semantics")
    );

    let mut missing_user_event = valid_control_trace("array", &oracle, true);
    missing_user_event.user_event_ledger.pop();
    assert_eq!(
        validate_candidate_trace(
            &missing_user_event,
            "array",
            &oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("candidate user EventStore ledger differs from complete-source semantics")
    );

    let mut missing_result = valid_control_trace("array", &oracle, true);
    missing_result.user_check_results.pop();
    assert_eq!(
        validate_candidate_trace(
            &missing_result,
            "array",
            &oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("candidate check results differ from complete-source semantics")
    );

    let unavailable_oracle = single_complete_source_oracle("unavailable");
    let mut leaked_provisional = valid_control_trace("unavailable", &unavailable_oracle, false);
    validate_candidate_trace(
        &leaked_provisional,
        "unavailable",
        &unavailable_oracle,
        false,
        ProjectLocalOrderForTest::Forward,
        ProjectLocalOrderForTest::Forward,
    )
    .expect("typed unavailable control trace is valid");
    leaked_provisional.unavailable_audits[0]
        .published_provisional_writes
        .push("leaked-provisional".to_owned());
    assert_eq!(
        validate_candidate_trace(
            &leaked_provisional,
            "unavailable",
            &unavailable_oracle,
            false,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        ),
        Err("unavailable terminal did not discard every provisional write")
    );
}

#[test]
#[ignore = "RED until the WU4 project-local semantic coordinator exists"]
fn project_local_demands_match_frozen_graphs_in_both_orders() {
    let actual = complete_source_oracles();
    let mut session_identities = BTreeSet::new();
    for (case_id, oracle) in &actual {
        for source_order in [
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Reverse,
        ] {
            for demand_order in [
                ProjectLocalOrderForTest::Forward,
                ProjectLocalOrderForTest::Reverse,
            ] {
                let trace = candidate_trace(ProjectLocalSemanticRequestForTest {
                    case_id,
                    inputs: case_inputs(case_id),
                    source_order,
                    demand_order,
                    force_all: false,
                    fault: None,
                });
                validate_candidate_trace(
                    &trace,
                    case_id,
                    oracle,
                    false,
                    source_order,
                    demand_order,
                )
                .unwrap_or_else(|error| {
                    panic!("{case_id}/{source_order:?}/{demand_order:?}: {error}")
                });
                assert!(
                    session_identities.insert(trace.project_session_identity),
                    "candidate reused a semantic session across project/order runs"
                );
            }
        }
    }
}

#[test]
#[ignore = "RED until the WU4 project-local semantic coordinator exists"]
fn project_local_sccs_publish_atomically_and_repeat_identities() {
    let actual = complete_source_oracles();
    for case_id in ["array", "unavailable"] {
        let oracle = complete_source_case(&actual, case_id);
        let trace = candidate_trace(candidate_request(case_id, false, None));
        validate_candidate_trace(
            &trace,
            case_id,
            oracle,
            false,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        )
        .expect("valid candidate trace");
        assert!(trace.scc_publications.iter().all(|publication| {
            publication.final_state != ProjectLocalTerminalStateForTest::Resolving
        }));
        assert!(trace.publication_scc_commits.iter().all(|publication| {
            publication.final_state != ProjectLocalTerminalStateForTest::Resolving
        }));
        assert!(trace
            .repeated_demands
            .iter()
            .all(|demand| demand.first_identity == demand.repeated_identity));
    }
}

#[test]
#[ignore = "RED until the WU4 project-local semantic coordinator exists"]
fn project_local_force_all_matches_raw_complete_source_evidence() {
    let actual = complete_source_oracles();
    for (case_id, oracle) in &actual {
        let trace = candidate_trace(candidate_request(case_id, true, None));
        validate_candidate_trace(
            &trace,
            case_id,
            oracle,
            true,
            ProjectLocalOrderForTest::Forward,
            ProjectLocalOrderForTest::Forward,
        )
        .expect("valid force_all candidate trace");
    }
}

#[test]
#[ignore = "RED until the WU4 project-local semantic coordinator exists"]
fn injected_candidate_faults_reach_the_raw_trace_guards() {
    let oracle = single_complete_source_oracle("array");
    let controls = [
        (
            ProjectLocalSemanticFaultForTest::OmitDependencyEdge,
            "candidate omitted or added a semantic dependency edge",
        ),
        (
            ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete,
            "terminal became observable before its whole demanded SCC was published",
        ),
        (
            ProjectLocalSemanticFaultForTest::WriteResolvingToDurableCache,
            "Resolving state wrote a durable cache entry",
        ),
        (
            ProjectLocalSemanticFaultForTest::PublishStaleGlobalObject,
            "global object was not built from the final contributor set",
        ),
        (
            ProjectLocalSemanticFaultForTest::InvokeForceAllFromOrdinaryDemand,
            "ordinary session invoked force_all",
        ),
    ];
    for (fault, expected) in controls {
        let trace = candidate_trace(candidate_request("array", false, Some(fault)));
        assert_eq!(
            validate_candidate_trace(
                &trace,
                "array",
                &oracle,
                false,
                ProjectLocalOrderForTest::Forward,
                ProjectLocalOrderForTest::Forward,
            ),
            Err(expected),
            "injected fault reached the wrong guard: {fault:?}"
        );
    }
}

fn candidate_request(
    case_id: &'static str,
    force_all: bool,
    fault: Option<ProjectLocalSemanticFaultForTest>,
) -> ProjectLocalSemanticRequestForTest {
    ProjectLocalSemanticRequestForTest {
        case_id,
        inputs: case_inputs(case_id),
        source_order: ProjectLocalOrderForTest::Forward,
        demand_order: ProjectLocalOrderForTest::Forward,
        force_all,
        fault,
    }
}

fn candidate_trace(
    request: ProjectLocalSemanticRequestForTest,
) -> ProjectLocalSemanticTraceForTest {
    LibraryBaseProvider::new()
        .project_local_semantic_candidate_trace_for_test(request)
        .unwrap_or_else(|error| panic!("candidate unavailable: {error}"))
}

fn validate_candidate_trace(
    trace: &ProjectLocalSemanticTraceForTest,
    expected_case_id: &str,
    oracle: &CompleteSourceSemanticDemandOracleForTest,
    expect_force_all: bool,
    source_order: ProjectLocalOrderForTest,
    demand_order: ProjectLocalOrderForTest,
) -> Result<(), &'static str> {
    if trace.case_id != expected_case_id
        || trace.project_session_identity == 0
        || trace.publisher_session_identity != trace.project_session_identity
    {
        return Err("candidate trace has no session or publisher identity");
    }
    if trace.source_inputs != oracle.source_inputs {
        return Err("candidate did not execute the exact requested source inputs");
    }
    if trace.source_visit_rows != ordered_rows(&oracle.source_visit_rows, source_order)
        || trace.demand_visit_rows != ordered_rows(&oracle.semantic_closure_rows, demand_order)
    {
        return Err("candidate did not execute the requested source and demand orders");
    }
    require_canonical_rows(&trace.graph_owner_rows)?;
    require_canonical_rows(&trace.root_rows)?;
    require_canonical_rows(&trace.ordinary_semantic_owner_rows)?;
    require_canonical(&trace.reverse_edges)?;
    require_canonical(&trace.root_consumers)?;
    require_canonical(&trace.sccs)?;
    require_canonical(&trace.publication_sccs)?;
    if trace.graph_owner_rows != oracle.owner_rows {
        return Err("candidate owner graph differs from the complete-source graph");
    }
    if trace.root_rows != oracle.root_rows {
        return Err("candidate root graph differs from the complete-source graph");
    }
    if trace.reverse_edges != oracle.reverse_edges {
        return Err("candidate omitted or added a semantic dependency edge");
    }
    if trace.root_consumers != oracle.root_consumers || trace.sccs != oracle.sccs {
        return Err("candidate dependency partition differs from the complete-source graph");
    }
    if trace.publication_sccs != oracle.publication_sccs {
        return Err("candidate publication SCC graph differs from the locked semantic oracle");
    }
    if trace.unavailable_terminals != oracle.unavailable_terminals {
        return Err("candidate unavailable terminals differ from complete-source causes");
    }
    let audited_terminals = trace
        .unavailable_audits
        .iter()
        .map(|audit| audit.terminal.clone())
        .collect::<Vec<_>>();
    if audited_terminals != oracle.unavailable_terminals
        || trace.unavailable_audits.iter().any(|audit| {
            audit.discarded_provisional_writes.is_empty()
                || !audit.published_provisional_writes.is_empty()
        })
    {
        return Err("unavailable terminal did not discard every provisional write");
    }
    if trace.ordinary_semantic_owner_rows != oracle.semantic_closure_rows {
        return Err("ordinary semantic owner work differs from the frozen demand closure");
    }
    if trace.semantic_probes != oracle.semantic_probes {
        return Err("candidate semantic probes differ from complete-source semantics");
    }
    if trace.user_event_ledger != oracle.user_event_ledger {
        return Err("candidate user EventStore ledger differs from complete-source semantics");
    }
    if trace.user_check_results != oracle.user_check_results {
        return Err("candidate check results differ from complete-source semantics");
    }
    if trace.ordinary_semantic_owner_rows.len() >= oracle.library_semantic_owner_rows.len() {
        return Err("ordinary semantic work is not strictly smaller than force_all");
    }

    let demanded = trace
        .ordinary_semantic_owner_rows
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let demanded_names = semantic_owner_names(&trace.ordinary_semantic_owner_rows);
    let demanded_publication_sccs = oracle
        .publication_sccs
        .iter()
        .filter(|scc| {
            scc.members
                .iter()
                .any(|member| demanded_names.contains(&member.name))
        })
        .cloned()
        .collect::<Vec<_>>();
    if trace.publication_scc_commits.len() != demanded_publication_sccs.len() {
        return Err("candidate did not publish every demanded semantic SCC exactly once");
    }
    for expected_scc in demanded_publication_sccs {
        let mut matching = trace
            .publication_scc_commits
            .iter()
            .filter(|publication| publication.scc == expected_scc);
        let Some(publication) = matching.next() else {
            return Err("candidate did not publish every demanded semantic SCC exactly once");
        };
        if matching.next().is_some()
            || publication.reserved_members != expected_scc.members
            || publication.published_members != expected_scc.members
            || !publication.observed_before_complete.is_empty()
            || publication.state_before_publish != ProjectLocalTerminalStateForTest::Resolving
            || publication.final_state != ProjectLocalTerminalStateForTest::Ready
        {
            return Err("terminal became observable before its whole demanded SCC was published");
        }
    }
    let demanded_sccs = oracle
        .sccs
        .iter()
        .filter(|scc| scc.members.iter().any(|member| demanded.contains(member)))
        .cloned()
        .collect::<Vec<_>>();
    if trace.scc_publications.len() != demanded_sccs.len() {
        return Err("candidate did not publish every demanded SCC exactly once");
    }
    for expected_scc in demanded_sccs {
        let mut matching = trace
            .scc_publications
            .iter()
            .filter(|publication| publication.scc == expected_scc);
        let Some(publication) = matching.next() else {
            return Err("candidate did not publish every demanded SCC exactly once");
        };
        if matching.next().is_some()
            || publication.reserved_members != expected_scc.members
            || !publication.observed_before_complete.is_empty()
            || publication.state_before_publish != ProjectLocalTerminalStateForTest::Resolving
            || publication.final_state == ProjectLocalTerminalStateForTest::Resolving
        {
            return Err("terminal became observable before its whole demanded SCC was published");
        }
        match publication.final_state {
            ProjectLocalTerminalStateForTest::Ready => {
                if publication.published_members != expected_scc.members
                    || !publication.discarded_provisional_writes.is_empty()
                {
                    return Err(
                        "terminal became observable before its whole demanded SCC was published",
                    );
                }
            }
            ProjectLocalTerminalStateForTest::Unavailable(_) => {
                if !publication.published_members.is_empty()
                    || publication.discarded_provisional_writes != expected_scc.members
                {
                    return Err("unavailable terminal did not discard every provisional write");
                }
            }
            ProjectLocalTerminalStateForTest::Resolving => unreachable!(),
        }
    }
    for terminal in &oracle.unavailable_terminals {
        let namespace_prefix = format!("namespace:{:?}@", terminal.name);
        let matches_terminal = trace.scc_publications.iter().any(|publication| {
            publication
                .scc
                .members
                .iter()
                .any(|member| member.starts_with(&namespace_prefix))
                && publication.final_state
                    == ProjectLocalTerminalStateForTest::Unavailable(terminal.reason)
        });
        if !matches_terminal {
            return Err("candidate unavailable terminal did not close its demanded SCC");
        }
    }

    if trace
        .durable_cache_writes
        .iter()
        .any(|write| write.state == ProjectLocalTerminalStateForTest::Resolving)
    {
        return Err("Resolving state wrote a durable cache entry");
    }
    let cache_owners = trace
        .durable_cache_writes
        .iter()
        .map(|write| write.owner.clone())
        .collect::<Vec<_>>();
    if sorted_unique(cache_owners) != trace.ordinary_semantic_owner_rows {
        return Err("durable cache does not contain exactly the final demanded owners");
    }

    if trace.final_global_contributors != oracle.global_contributors
        || trace.published_global_contributors != oracle.global_contributors
    {
        return Err("global object was not built from the final contributor set");
    }
    if !trace.ordinary_force_all_invocations.is_empty() {
        return Err("ordinary session invoked force_all");
    }

    let repeated_owners = trace
        .repeated_demands
        .iter()
        .map(|demand| demand.owner.clone())
        .collect::<Vec<_>>();
    if sorted_unique(repeated_owners) != trace.ordinary_semantic_owner_rows
        || trace
            .repeated_demands
            .iter()
            .any(|demand| demand.first_identity != demand.repeated_identity)
    {
        return Err("repeated demand changed a project-local semantic identity");
    }

    match (&trace.force_all, expect_force_all) {
        (None, false) => {}
        (Some(force_all), true) => validate_force_all(force_all, oracle)?,
        _ => return Err("force_all trace does not match the explicit test request"),
    }
    Ok(())
}

fn validate_force_all(
    force_all: &ProjectLocalForceAllTraceForTest,
    oracle: &CompleteSourceSemanticDemandOracleForTest,
) -> Result<(), &'static str> {
    require_canonical_rows(&force_all.semantic_owner_rows)?;
    if force_all.semantic_owner_rows != oracle.library_semantic_owner_rows
        || force_all.semantic_probes != oracle.force_all_semantic_probes
        || force_all.record_kinds != oracle.library_record_kinds
        || force_all.record_ledger != oracle.library_record_ledger
    {
        return Err("force_all differs from raw complete-source library evidence");
    }
    Ok(())
}

fn valid_control_trace(
    case_id: &str,
    oracle: &CompleteSourceSemanticDemandOracleForTest,
    force_all: bool,
) -> ProjectLocalSemanticTraceForTest {
    let demanded_names = semantic_owner_names(&oracle.semantic_closure_rows);
    let publication_scc_commits = oracle
        .publication_sccs
        .iter()
        .filter(|scc| {
            scc.members
                .iter()
                .any(|member| demanded_names.contains(&member.name))
        })
        .map(|scc| ProjectLocalPublicationSccCommitForTest {
            scc: scc.clone(),
            reserved_members: scc.members.clone(),
            published_members: scc.members.clone(),
            observed_before_complete: Vec::new(),
            state_before_publish: ProjectLocalTerminalStateForTest::Resolving,
            final_state: ProjectLocalTerminalStateForTest::Ready,
        })
        .collect();
    let demanded = oracle
        .semantic_closure_rows
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let scc_publications = oracle
        .sccs
        .iter()
        .filter(|scc| scc.members.iter().any(|member| demanded.contains(member)))
        .map(|scc| {
            let unavailable = oracle.unavailable_terminals.iter().find(|terminal| {
                let namespace_prefix = format!("namespace:{:?}@", terminal.name);
                scc.members
                    .iter()
                    .any(|member| member.starts_with(&namespace_prefix))
            });
            let final_state = unavailable.map_or(ProjectLocalTerminalStateForTest::Ready, |row| {
                ProjectLocalTerminalStateForTest::Unavailable(row.reason)
            });
            ProjectLocalSccPublicationForTest {
                scc: scc.clone(),
                reserved_members: scc.members.clone(),
                published_members: unavailable.map_or_else(|| scc.members.clone(), |_| Vec::new()),
                observed_before_complete: Vec::new(),
                state_before_publish: ProjectLocalTerminalStateForTest::Resolving,
                final_state,
                discarded_provisional_writes: unavailable
                    .map_or_else(Vec::new, |_| scc.members.clone()),
            }
        })
        .collect();
    let durable_cache_writes = oracle
        .semantic_closure_rows
        .iter()
        .map(|owner| ProjectLocalCacheWriteForTest {
            owner: owner.clone(),
            state: oracle
                .unavailable_terminals
                .iter()
                .find(|terminal| owner.starts_with(&format!("namespace:{:?}@", terminal.name)))
                .map_or(ProjectLocalTerminalStateForTest::Ready, |terminal| {
                    ProjectLocalTerminalStateForTest::Unavailable(terminal.reason)
                }),
        })
        .collect();
    let repeated_demands = oracle
        .semantic_closure_rows
        .iter()
        .enumerate()
        .map(|(index, owner)| ProjectLocalRepeatedDemandForTest {
            owner: owner.clone(),
            first_identity: format!("test-id-{index}"),
            repeated_identity: format!("test-id-{index}"),
        })
        .collect();
    ProjectLocalSemanticTraceForTest {
        case_id: case_id.to_owned(),
        project_session_identity: 1,
        publisher_session_identity: 1,
        source_inputs: oracle.source_inputs.clone(),
        source_visit_rows: oracle.source_visit_rows.clone(),
        demand_visit_rows: oracle.semantic_closure_rows.clone(),
        graph_owner_rows: oracle.owner_rows.clone(),
        root_rows: oracle.root_rows.clone(),
        reverse_edges: oracle.reverse_edges.clone(),
        root_consumers: oracle.root_consumers.clone(),
        sccs: oracle.sccs.clone(),
        publication_sccs: oracle.publication_sccs.clone(),
        unavailable_terminals: oracle.unavailable_terminals.clone(),
        ordinary_semantic_owner_rows: oracle.semantic_closure_rows.clone(),
        semantic_probes: oracle.semantic_probes.clone(),
        user_event_ledger: oracle.user_event_ledger.clone(),
        user_check_results: oracle.user_check_results.clone(),
        unavailable_audits: oracle
            .unavailable_terminals
            .iter()
            .map(|terminal| ProjectLocalUnavailableAuditForTest {
                terminal: terminal.clone(),
                discarded_provisional_writes: vec![format!("namespace:{:?}", terminal.name)],
                published_provisional_writes: Vec::new(),
            })
            .collect(),
        scc_publications,
        publication_scc_commits,
        durable_cache_writes,
        final_global_contributors: oracle.global_contributors.clone(),
        published_global_contributors: oracle.global_contributors.clone(),
        repeated_demands,
        ordinary_force_all_invocations: Vec::new(),
        force_all: force_all.then(|| ProjectLocalForceAllTraceForTest {
            semantic_owner_rows: oracle.library_semantic_owner_rows.clone(),
            semantic_probes: oracle.force_all_semantic_probes.clone(),
            record_kinds: oracle.library_record_kinds.clone(),
            record_ledger: oracle.library_record_ledger.clone(),
        }),
    }
}

fn complete_source_oracles() -> Vec<(&'static str, CompleteSourceSemanticDemandOracleForTest)> {
    let provider = LibraryBaseProvider::new();
    oracle_inputs()
        .into_iter()
        .map(|(case_id, inputs)| {
            let oracle = provider
                .complete_source_semantic_demand_oracle_for_test(&inputs)
                .unwrap_or_else(|error| panic!("{case_id}: {error}"));
            (case_id, oracle)
        })
        .collect()
}

fn single_complete_source_oracle(case_id: &str) -> CompleteSourceSemanticDemandOracleForTest {
    let inputs = case_inputs(case_id);
    LibraryBaseProvider::new()
        .complete_source_semantic_demand_oracle_for_test(&inputs)
        .unwrap_or_else(|error| panic!("{case_id}: {error}"))
}

fn case_inputs(case_id: &str) -> Vec<UserDeltaProjectInputForTest<'static>> {
    oracle_inputs()
        .into_iter()
        .find(|(id, _)| *id == case_id)
        .map(|(_, inputs)| inputs)
        .unwrap_or_else(|| panic!("missing complete-source oracle input {case_id}"))
}

fn complete_source_case<'a>(
    actual: &'a [(&str, CompleteSourceSemanticDemandOracleForTest)],
    case_id: &str,
) -> &'a CompleteSourceSemanticDemandOracleForTest {
    actual
        .iter()
        .find_map(|(id, oracle)| (*id == case_id).then_some(oracle))
        .unwrap_or_else(|| panic!("missing complete-source oracle {case_id}"))
}

fn force_all_value(oracle: &CompleteSourceSemanticDemandOracleForTest) -> Value {
    let diagnostics = oracle
        .library_record_kinds
        .iter()
        .filter(|kind| **kind == ProjectLocalRecordKindForTest::Diagnostic)
        .count();
    let incompletes = oracle.library_record_kinds.len() - diagnostics;
    let counts = owner_counts(&oracle.library_semantic_owner_rows);
    json!({
        "type_groups": counts[0],
        "values": counts[1],
        "namespaces": counts[2],
        "classes": counts[3],
        "global_objects": counts[4],
        "semantic_total": oracle.library_semantic_owner_rows.len(),
        "owner_digest": rows_digest(&oracle.library_semantic_owner_rows),
        "diagnostics": diagnostics,
        "incompletes": incompletes,
        "records": oracle.library_record_kinds.len(),
        "record_ledger_digest": bytes_digest(&oracle.library_record_ledger)
    })
}

fn complete_source_case_value(
    case_id: &str,
    oracle: &CompleteSourceSemanticDemandOracleForTest,
) -> Value {
    let combined = owner_counts(&oracle.owner_rows);
    let mut value = json!({
        "id": case_id,
        "source_count": oracle.phase_counts.parse_units.saturating_sub(82),
        "combined": {
            "type_groups": combined[0],
            "values": combined[1],
            "namespaces": combined[2],
            "classes": combined[3],
            "global_objects": combined[4],
            "statements": combined[5],
            "semantic_total": combined[..5].iter().sum::<usize>(),
            "owner_digest": rows_digest(&oracle.owner_rows)
        },
        "user_semantic_total": oracle.user_semantic_owner_rows.len(),
        "user_semantic_digest": rows_digest(&oracle.user_semantic_owner_rows),
        "mixed_semantic_total": oracle.mixed_semantic_owner_rows.len(),
        "mixed_semantic_digest": rows_digest(&oracle.mixed_semantic_owner_rows),
        "roots": oracle.root_rows.len(),
        "root_digest": rows_digest(&oracle.root_rows),
        "global_contributors": oracle.global_contributors.len(),
        "global_contributor_digest": rows_digest(&global_contributor_rows(oracle)),
        "reverse_edges": oracle.reverse_edges.len(),
        "reverse_edge_digest": rows_digest(&dependency_rows(&oracle.reverse_edges)),
        "root_consumers": oracle.root_consumers.len(),
        "root_consumer_digest": rows_digest(&root_consumer_rows(&oracle.root_consumers)),
        "sccs": oracle.sccs.len(),
        "scc_digest": rows_digest(&scc_rows(&oracle.sccs)),
        "user_statements": oracle.user_statement_rows.len(),
        "user_statement_digest": rows_digest(&oracle.user_statement_rows),
        "ordinary_semantic_closure": oracle.semantic_closure_rows.len(),
        "ordinary_semantic_closure_digest": rows_digest(&oracle.semantic_closure_rows),
        "ordinary_semantic_probes": semantic_probe_rows(&oracle.semantic_probes).len(),
        "ordinary_semantic_probe_digest": rows_digest(&semantic_probe_rows(&oracle.semantic_probes)),
        "force_all_semantic_probes": semantic_probe_rows(&oracle.force_all_semantic_probes).len(),
        "force_all_semantic_probe_digest": rows_digest(&semantic_probe_rows(&oracle.force_all_semantic_probes)),
        "user_event_records": user_event_rows(oracle).len(),
        "user_event_record_digest": rows_digest(&user_event_rows(oracle)),
        "user_check_results": user_result_rows(oracle).len(),
        "user_check_result_digest": rows_digest(&user_result_rows(oracle)),
        "source_visits": oracle.source_visit_rows.len(),
        "source_visit_digest": rows_digest(&oracle.source_visit_rows),
        "publication_sccs": oracle.publication_sccs.len(),
        "publication_scc_digest": rows_digest(&publication_scc_rows(&oracle.publication_sccs)),
        "unavailable_terminals": oracle.unavailable_terminals.len(),
        "unavailable_terminal_digest": rows_digest(&unavailable_terminal_rows(oracle)),
        "parse_units": oracle.phase_counts.parse_units,
        "bind_units": oracle.phase_counts.bind_units,
        "reserved_records": oracle.phase_counts.reserved_records,
        "filled_records": oracle.phase_counts.filled_records,
        "publication_validations": oracle.phase_counts.publication_validations,
        "statement_check_units": oracle.phase_counts.statement_check_units
    });
    value
        .as_object_mut()
        .expect("complete-source case receipt is an object")
        .insert(
            "source_input_digest".to_owned(),
            Value::String(rows_digest(&source_input_rows(oracle))),
        );
    value
}

fn semantic_graph_projection(oracle: &CompleteSourceSemanticDemandOracleForTest) -> Value {
    json!({
        "owners": rows_digest(&oracle.owner_rows),
        "roots": rows_digest(&oracle.root_rows),
        "edges": rows_digest(&dependency_rows(&oracle.reverse_edges)),
        "consumers": rows_digest(&root_consumer_rows(&oracle.root_consumers)),
        "sccs": rows_digest(&scc_rows(&oracle.sccs)),
        "closure": rows_digest(&oracle.semantic_closure_rows),
    })
}

fn owner_counts(rows: &[String]) -> [usize; 6] {
    let mut counts = [0; 6];
    for row in rows {
        let index = if row.starts_with("type:") {
            0
        } else if row.starts_with("value:") {
            1
        } else if row.starts_with("namespace:") {
            2
        } else if row.starts_with("class:") {
            3
        } else if row.starts_with("global-object:") {
            4
        } else if row.starts_with("statement:") {
            5
        } else {
            panic!("unknown normalized owner row: {row}");
        };
        counts[index] += 1;
    }
    counts
}

fn dependency_rows(edges: &[ProjectLocalDependencyEdgeForTest]) -> Vec<String> {
    edges
        .iter()
        .map(|edge| {
            format!(
                "dependency={:?}|consumer={:?}",
                edge.dependency, edge.consumer
            )
        })
        .collect()
}

fn root_consumer_rows(consumers: &[ProjectLocalRootConsumerForTest]) -> Vec<String> {
    consumers
        .iter()
        .map(|consumer| {
            format!(
                "root={:?}|slot={}|consumer={:?}",
                consumer.name, consumer.slot, consumer.consumer
            )
        })
        .collect()
}

fn root_slot_owner(rows: &[String], name: &str, slot: &str) -> Option<String> {
    let prefix = format!("root:{name:?}|");
    rows.iter().find_map(|row| {
        let remainder = row.strip_prefix(&prefix)?;
        let start = remainder.find(slot)?.checked_add(slot.len())?;
        let owner = remainder.get(start..)?.split('|').next()?;
        (owner != "-").then(|| owner.to_owned())
    })
}

fn semantic_owner_names(rows: &[String]) -> BTreeSet<String> {
    rows.iter()
        .filter_map(|row| {
            ["type:\"", "value:\""]
                .into_iter()
                .find_map(|prefix| row.strip_prefix(prefix))
                .and_then(|row| row.split_once("\"@").map(|(name, _)| name.to_owned()))
        })
        .collect()
}

fn source_input_rows(oracle: &CompleteSourceSemanticDemandOracleForTest) -> Vec<String> {
    oracle
        .source_inputs
        .iter()
        .map(|input| {
            format!(
                "path={:?}|bytes={}|sha256={}",
                input.path, input.byte_len, input.sha256
            )
        })
        .collect()
}

fn scc_rows(sccs: &[ProjectLocalSccForTest]) -> Vec<String> {
    sccs.iter()
        .map(|scc| {
            scc.members
                .iter()
                .map(|member| format!("{}:{member}", member.len()))
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

fn publication_scc_rows(
    sccs: &[crate::provider::ProjectLocalPublicationSccForTest],
) -> Vec<String> {
    sccs.iter()
        .map(|scc| {
            scc.members
                .iter()
                .map(|member| format!("{}:{}", member.name.len(), member.name))
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

fn unavailable_terminal_rows(oracle: &CompleteSourceSemanticDemandOracleForTest) -> Vec<String> {
    oracle
        .unavailable_terminals
        .iter()
        .map(|terminal| {
            let fragments = terminal
                .fragments
                .iter()
                .map(|fragment| {
                    format!(
                        "source={:?}|origin={:?}|start={}",
                        fragment.source_path, fragment.origin, fragment.source_start,
                    )
                })
                .collect::<Vec<_>>();
            format!(
                "name={:?}|reason={:?}|fragments={}",
                terminal.name,
                terminal.reason,
                encode_rows(&fragments),
            )
        })
        .collect()
}

fn user_event_rows(oracle: &CompleteSourceSemanticDemandOracleForTest) -> Vec<String> {
    oracle
        .user_event_ledger
        .iter()
        .map(|row| {
            format!(
                "source={:?}|start={}|event={}|record={}|payload={}",
                row.source_path,
                row.source_start,
                row.event_ordinal,
                row.record_ordinal,
                checker_record_row(&row.record),
            )
        })
        .collect()
}

fn user_result_rows(oracle: &CompleteSourceSemanticDemandOracleForTest) -> Vec<String> {
    oracle
        .user_check_results
        .iter()
        .map(|row| {
            let diagnostics = row
                .diagnostics
                .iter()
                .map(diagnostic_row)
                .collect::<Vec<_>>();
            let incomplete = row
                .incomplete
                .iter()
                .map(incomplete_row)
                .collect::<Vec<_>>();
            format!(
                "source={:?}|diagnostics={}|incomplete={}",
                row.source_path,
                encode_rows(&diagnostics),
                encode_rows(&incomplete),
            )
        })
        .collect()
}

fn checker_record_row(
    record: &crate::check::checker::library_compiler::NormalizedCheckerRecordForTest,
) -> String {
    match record {
        crate::check::checker::library_compiler::NormalizedCheckerRecordForTest::Diagnostic(
            diagnostic,
        ) => format!("diagnostic:({})", diagnostic_row(diagnostic)),
        crate::check::checker::library_compiler::NormalizedCheckerRecordForTest::Incomplete(
            incomplete,
        ) => format!("incomplete:({})", incomplete_row(incomplete)),
    }
}

fn diagnostic_row(
    diagnostic: &crate::check::checker::library_compiler::NormalizedDiagnosticForTest,
) -> String {
    format!(
        "span={}-{}|code={:?}|severity={:?}|message={:?}|elaboration={}",
        diagnostic.span.start,
        diagnostic.span.end,
        diagnostic.code,
        diagnostic.severity,
        diagnostic.message,
        encode_rows(&diagnostic.elaboration),
    )
}

fn incomplete_row(
    incomplete: &crate::check::checker::library_compiler::NormalizedIncompleteForTest,
) -> String {
    format!(
        "span={}-{}|id={:?}|context={:?}",
        incomplete.span.start, incomplete.span.end, incomplete.id, incomplete.context,
    )
}

fn semantic_probe_rows(probes: &ProjectLocalSemanticProbesForTest) -> Vec<String> {
    let mut rows = Vec::new();
    rows.extend(probes.global_types.iter().map(|probe| {
        format!(
            "global-type={:?}|declarations={}|members={}",
            probe.name,
            probe.declaration_count,
            encode_rows(&probe.member_names),
        )
    }));
    rows.extend(probes.module_types.iter().map(|row| {
        format!(
            "module-type={:?}:{:?}|declarations={}|members={}",
            row.source_path,
            row.probe.name,
            row.probe.declaration_count,
            encode_rows(&row.probe.member_names),
        )
    }));
    rows.extend(probes.global_values.iter().map(|probe| {
        let signatures = probe
            .signatures
            .iter()
            .map(signature_row)
            .collect::<Vec<_>>();
        let callable_members = probe
            .callable_members
            .iter()
            .map(|member| {
                let signatures = member
                    .signatures
                    .iter()
                    .map(signature_row)
                    .collect::<Vec<_>>();
                format!(
                    "name={:?}|source={:?}|reservation={:?}|start={}|calls={}|signatures={}",
                    member.name,
                    member.source_path,
                    member.reservation_source_path,
                    member.source_start,
                    member.call_signature_count,
                    encode_rows(&signatures),
                )
            })
            .collect::<Vec<_>>();
        format!(
            "global-value={:?}|declarations={}|calls={}|signatures={}|members={}|callable-members={}",
            probe.name,
            probe.declaration_count,
            probe.call_signature_count,
            encode_rows(&signatures),
            encode_rows(&probe.member_names),
            encode_rows(&callable_members),
        )
    }));
    rows.sort();
    rows
}

fn signature_row(signature: &crate::check::checker::library_compiler::SignatureProbe) -> String {
    format!(
        "parameters={}|return={:?}",
        encode_rows(&signature.parameter_types),
        signature.return_type,
    )
}

fn encode_rows(rows: &[String]) -> String {
    rows.iter()
        .map(|row| format!("{}:{row}", row.len()))
        .collect::<Vec<_>>()
        .join("|")
}

fn global_contributor_rows(oracle: &CompleteSourceSemanticDemandOracleForTest) -> Vec<String> {
    oracle
        .global_contributors
        .iter()
        .map(|row| {
            let sites = row
                .sites
                .iter()
                .map(|site| {
                    format!(
                        "source={:?}|origin={:?}|span={}-{}|kind={:?}",
                        site.source_path, site.origin, site.span_start, site.span_end, site.kind,
                    )
                })
                .collect::<Vec<_>>();
            format!(
                "name={:?}|value={}|type={}|namespace={}|sites={}",
                row.name,
                row.has_value,
                row.has_type,
                row.has_namespace,
                encode_rows(&sites),
            )
        })
        .collect()
}

fn rows_digest(rows: &[String]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"typokat/wu4/normalized-rows/v1");
    for row in rows {
        digest.update(u64::try_from(row.len()).unwrap_or(u64::MAX).to_be_bytes());
        digest.update(row.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn bytes_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn require_canonical_rows(rows: &[String]) -> Result<(), &'static str> {
    if sorted_unique(rows.to_vec()) != rows {
        return Err("candidate raw rows are not sorted and unique");
    }
    Ok(())
}

fn require_canonical<T: Clone + Ord>(rows: &[T]) -> Result<(), &'static str> {
    if sorted_unique(rows.to_vec()) != rows {
        return Err("candidate raw graph rows are not sorted and unique");
    }
    Ok(())
}

fn sorted_unique<T: Ord>(mut rows: Vec<T>) -> Vec<T> {
    rows.sort();
    rows.dedup();
    rows
}

fn ordered_rows(rows: &[String], order: ProjectLocalOrderForTest) -> Vec<String> {
    let mut ordered = rows.to_vec();
    if order == ProjectLocalOrderForTest::Reverse {
        ordered.reverse();
    }
    ordered
}

fn frozen_oracle() -> Value {
    serde_json::from_slice(ORACLE_BYTES).expect("valid WU4 semantic demand oracle")
}

fn oracle_inputs() -> Vec<(&'static str, Vec<UserDeltaProjectInputForTest<'static>>)> {
    vec![
        (
            "reflect",
            vec![input(
                "/candidate/reflect_packaged.ts",
                include_str!(
                    "../../../tests/cases/sr_project_local_library_candidate/reflect_packaged.ts"
                ),
            )],
        ),
        (
            "array",
            vec![input(
                "/candidate/array_library_scc.ts",
                include_str!(
                    "../../../tests/cases/sr_project_local_library_candidate/array_library_scc.ts"
                ),
            )],
        ),
        (
            "global-this",
            vec![
                input(
                    "/candidate/global-this/00_global.ts",
                    include_str!(
                        "../../../tests/cases/b103_library_merge_correctness_project/global_this_contributor/00_global.ts"
                    ),
                ),
                input(
                    "/candidate/global-this/99_consume.ts",
                    include_str!(
                        "../../../tests/cases/b103_library_merge_correctness_project/global_this_contributor/99_consume.ts"
                    ),
                ),
            ],
        ),
        (
            "unavailable",
            vec![input(
                "/candidate/unavailable.ts",
                include_str!(
                    "../../../tests/cases/b43_namespaces_declaration_merging/wu6a_unavailable_ledger.ts"
                ),
            )],
        ),
        ("mixed-forward", mixed_inputs(false)),
        ("mixed-reverse", mixed_inputs(true)),
        (
            "isolation-a",
            vec![
                input(
                    "/candidate/isolation/00_augment.ts",
                    include_str!(
                        "../../../tests/cases/sr_project_local_library_candidate/isolation_a/00_augment.ts"
                    ),
                ),
                input(
                    "/candidate/isolation/99_consume.ts",
                    include_str!(
                        "../../../tests/cases/sr_project_local_library_candidate/isolation_a/99_consume.ts"
                    ),
                ),
            ],
        ),
        (
            "isolation-b",
            vec![
                input(
                    "/candidate/isolation/00_augment.ts",
                    include_str!(
                        "../../../tests/cases/sr_project_local_library_candidate/isolation_b/00_augment.ts"
                    ),
                ),
                input(
                    "/candidate/isolation/99_consume.ts",
                    include_str!(
                        "../../../tests/cases/sr_project_local_library_candidate/isolation_b/99_consume.ts"
                    ),
                ),
            ],
        ),
        (
            "official-assignment-compat-between-tuple-and-array",
            vec![input(
                "/candidate/official/types/typeRelationships/assignmentCompatibility/assignmentCompatBetweenTupleAndArray.ts",
                include_str!(
                    "../../../tests/cases/sr_project_local_library_candidate/official_assignment_compat_between_tuple_and_array.ts"
                ),
            )],
        ),
    ]
}

fn mixed_inputs(reverse: bool) -> Vec<UserDeltaProjectInputForTest<'static>> {
    let script = input(
        "/candidate/mixed/00_script.ts",
        include_str!(
            "../../../tests/cases/sr_project_local_library_candidate/mixed_group_forward/00_script.ts"
        ),
    );
    let module = input(
        "/candidate/mixed/50_module.ts",
        include_str!(
            "../../../tests/cases/sr_project_local_library_candidate/mixed_group_forward/50_module.ts"
        ),
    );
    let consume = input(
        "/candidate/mixed/99_consume.ts",
        include_str!(
            "../../../tests/cases/sr_project_local_library_candidate/mixed_group_forward/99_consume.ts"
        ),
    );
    if reverse {
        vec![consume, module, script]
    } else {
        vec![script, module, consume]
    }
}

const fn input(path: &'static str, source: &'static str) -> UserDeltaProjectInputForTest<'static> {
    UserDeltaProjectInputForTest { path, source }
}
