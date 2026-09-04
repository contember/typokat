use serde_json::Value;
use std::process::{Command, Output};
use typokat_library::project_binder_probe::{
    with_project_binder_probe, ProjectBinderPhaseCountersV1, ProjectBinderProbeSetupCountersV1,
};

const PRODUCT_DIGEST: &str = "a3e543680440ed88755d4d43f5a5a02c071052a061705e378d29d03a313d6c82";
const USER_SOURCE_PATH: &str = "/candidate/reflect_packaged.ts";
const USER_SOURCE_BYTES: u64 = 202;
const USER_SOURCE_DIGEST: &str = "a5e86f4497dc86ba58452644c3baa4ba5626d2bb52f1b98166854fae88a61d82";
const BINDER_DIGEST: &str = "10cee1d655a8e48e07d76bd27c003a7f77b1da68418e907587cf3e2cec7b9554";
#[cfg(feature = "dhat-heap")]
const PHASE_CONTROL_BYTES: u64 = 1024 * 1024;

fn invoke_probe(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typokat-project-binder-probe"))
        .args(arguments)
        .output()
        .expect("run project binder probe")
}

fn successful_receipt(arguments: &[&str]) -> Value {
    let output = invoke_probe(arguments);
    assert!(
        output.status.success(),
        "probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("probe emits one JSON receipt")
}

fn setup_counters_json(counters: ProjectBinderProbeSetupCountersV1) -> Value {
    serde_json::json!({
        "declaration_product_initializations": counters.declaration_product_initializations,
        "user_parses": counters.user_parses,
    })
}

fn phase_counters_json(counters: ProjectBinderPhaseCountersV1) -> Value {
    serde_json::json!({
        "constructions": counters.constructions,
        "declaration_product_initializations": counters.declaration_product_initializations,
        "product_consumptions": counters.product_consumptions,
        "fresh_binder_entries": counters.fresh_binder_entries,
        "bound_library_units": counters.bound_library_units,
        "bound_user_units": counters.bound_user_units,
        "typed_binding_products": counters.typed_binding_products,
        "heavy_binding_projections": counters.heavy_binding_projections,
        "source_manifest_builds": counters.source_manifest_builds,
        "event_reservations": counters.event_reservations,
        "query_cache_writes": {
            "projection": counters.query_cache_writes.projection,
            "evaluator": counters.query_cache_writes.evaluator,
        },
        "relation_cache_writes": counters.relation_cache_writes,
        "binder_layered_rows": counters.binder_layered_rows,
    })
}

fn typed_one_sample() -> Value {
    with_project_binder_probe(|probe| {
        let setup = setup_counters_json(probe.setup_receipt().counters());
        let sample = probe.bind_one().expect("one typed phase sample");
        let measured = phase_counters_json(sample.phase_receipt().counters());
        let result = sample.finish_result().expect("normalize retained binder");
        serde_json::json!({
            "setup_work": setup,
            "measured_work": measured,
            "result": {
                "product_arc_reused": result.product_arc_reused,
                "binder_digest": result.binder_digest,
            },
        })
    })
    .expect("prepare typed project binder probe")
}

fn rust_function_body<'source>(source: &'source str, signature: &str) -> &'source str {
    let start = source.find(signature).expect("probe function exists");
    let tail = &source[start + signature.len()..];
    let end = tail
        .find("\nfn ")
        .map_or(source.len(), |offset| start + signature.len() + offset);
    &source[start..end]
}

fn assert_markers_in_order(body: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = body[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing ordered probe marker: {marker}"));
        cursor += offset + marker.len();
    }
}

fn assert_binary_uses_typed_receipts() {
    let binary = include_str!("../src/main.rs");
    assert!(binary.contains("with_project_binder_probe"));
    assert!(binary.contains("phase_receipt().counters()"));
    assert!(binary.contains("finish_result()"));
    assert!(!binary.contains("ProjectBinderPhaseReceiptV1 {"));
    assert!(!binary.contains("const PRODUCT_DIGEST"));
    #[cfg(feature = "jemalloc-wall")]
    assert_markers_in_order(
        rust_function_body(binary, "fn run_wall"),
        &[
            "Instant::now()",
            "bind_one()",
            ".elapsed()",
            "finish_result()",
            "serde_json::to_string",
        ],
    );
    #[cfg(feature = "dhat-heap")]
    assert_markers_in_order(
        rust_function_body(binary, "fn run_heap"),
        &[
            "Profiler::new_heap()",
            "bind_one()",
            "HeapStats::get()",
            "drop(profiler)",
            "finish_result()",
            "serde_json::to_string",
        ],
    );
}

fn assert_common_receipt(receipt: &Value, mode: &str) {
    assert_eq!(receipt["schema"], 3);
    assert_eq!(
        receipt["route"],
        "immutable-library-product-to-fresh-project-binder"
    );
    assert_eq!(receipt["mode"], mode);
    assert_eq!(receipt["build"]["profile"], "release");
    assert_eq!(receipt["build"]["visible_parallelism"], 1);
    assert_eq!(
        receipt["authentication"]["phase_receipt"],
        "typokat-library::ProjectBinderPhaseReceiptV1"
    );
    assert_eq!(receipt["library"]["source_count"], 82);
    assert_eq!(receipt["library"]["product_digest"], PRODUCT_DIGEST);
    assert_eq!(receipt["user_project"]["source_count"], 1);
    assert_eq!(receipt["user_project"]["path"], USER_SOURCE_PATH);
    assert_eq!(receipt["user_project"]["source_bytes"], USER_SOURCE_BYTES);
    assert_eq!(receipt["user_project"]["source_sha256"], USER_SOURCE_DIGEST);
    assert_eq!(receipt["user_project"]["prepared_before_measurement"], true);
    assert_eq!(
        receipt["setup_work"]["declaration_product_initializations"],
        1
    );
    assert_eq!(receipt["setup_work"]["user_parses"], 1);
    assert_eq!(receipt["result"]["product_arc_reused"], true);
    assert_eq!(receipt["result"]["binder_digest"], BINDER_DIGEST);
}

fn assert_exact_phase_work(work: &Value, invocations: u64) {
    assert_eq!(work["constructions"], invocations);
    assert_eq!(work["declaration_product_initializations"], 0);
    assert_eq!(work["product_consumptions"], invocations);
    assert_eq!(work["fresh_binder_entries"], invocations);
    assert_eq!(work["bound_library_units"], invocations * 82);
    assert_eq!(work["bound_user_units"], invocations);
    assert_eq!(work["typed_binding_products"], invocations);
    assert_eq!(work["heavy_binding_projections"], 0);
    assert_eq!(work["source_manifest_builds"], invocations);
    assert_eq!(work["event_reservations"], 0);
    assert_eq!(work["query_cache_writes"]["projection"], 0);
    assert_eq!(work["query_cache_writes"]["evaluator"], 0);
    assert_eq!(work["relation_cache_writes"], 0);
    let binder_rows = work["binder_layered_rows"]
        .as_u64()
        .expect("binder row count is a u64");
    if invocations == 0 {
        assert_eq!(binder_rows, 0);
    } else {
        assert!(binder_rows > 0);
        assert_eq!(binder_rows % invocations, 0);
    }
}

#[test]
fn probe_feature_excludes_heavy_test_normalization() {
    let library_manifest = include_str!("../../typokat-library/Cargo.toml");
    let feature = library_manifest
        .split_once("project-binder-probe = [")
        .and_then(|(_, tail)| tail.split_once(']'))
        .map(|(feature, _)| feature)
        .expect("probe feature declaration");
    assert!(feature.contains("typokat-check/measurement-counters"));
    assert!(!feature.contains("test-utils"));

    let checker = include_str!("../../typokat-check/src/check/checker/mod.rs");
    assert!(!checker.contains("ProjectBindingProductForTest"));
    assert!(!checker.contains("normalized_project_binding_product"));

    let probe = include_str!("../../typokat-library/src/project_binder_probe.rs");
    assert!(!probe.contains(".normalized"));
    assert!(!probe.contains("ProjectBindingProductForTest"));
}

#[cfg(feature = "dhat-heap")]
fn heap_stat(receipt: &Value, field: &str) -> u64 {
    receipt["heap"][field]
        .as_u64()
        .unwrap_or_else(|| panic!("heap field {field} is a u64"))
}

#[cfg(feature = "jemalloc-wall")]
#[test]
fn cli_receipt_uses_the_opaque_library_phase_receipt() {
    let typed = typed_one_sample();
    let cli = successful_receipt(&["wall", "--warmups", "0", "--samples", "1"]);

    assert_eq!(cli["setup_work"], typed["setup_work"]);
    assert_eq!(cli["measured_work"], typed["measured_work"]);
    assert_eq!(
        cli["result"]["binder_digest"],
        typed["result"]["binder_digest"]
    );
    assert_eq!(
        cli["result"]["product_arc_reused"],
        typed["result"]["product_arc_reused"]
    );

    assert_binary_uses_typed_receipts();
}

#[cfg(feature = "jemalloc-wall")]
#[test]
fn wall_probe_emits_authenticated_production_receipt() {
    let receipt = successful_receipt(&["wall"]);
    assert_common_receipt(&receipt, "wall");
    assert_eq!(receipt["allocator"], "tikv-jemallocator");
    assert_eq!(receipt["run"]["warmups"], 10);
    assert_eq!(receipt["run"]["samples"], 31);
    assert_exact_phase_work(&receipt["warmup_work"], 10);
    assert_exact_phase_work(&receipt["measured_work"], 31);

    let samples = receipt["wall"]["samples_ns"]
        .as_array()
        .expect("wall samples array");
    assert_eq!(samples.len(), 31);
    let mut durations = samples
        .iter()
        .map(|sample| sample.as_u64().expect("positive nanosecond sample"))
        .collect::<Vec<_>>();
    assert!(durations.iter().all(|duration| *duration > 0));
    durations.sort_unstable();
    assert_eq!(receipt["wall"]["median_ns"], durations[15]);
    assert!(receipt["heap"].is_null());
}

#[cfg(feature = "jemalloc-wall")]
#[test]
fn every_wall_sample_constructs_one_fresh_binder() {
    let receipt = successful_receipt(&["wall", "--warmups", "0", "--samples", "2"]);
    assert_common_receipt(&receipt, "wall");
    assert_exact_phase_work(&receipt["warmup_work"], 0);
    assert_exact_phase_work(&receipt["measured_work"], 2);
    assert_eq!(
        receipt["result"]["binder_digests"],
        serde_json::json!([BINDER_DIGEST, BINDER_DIGEST])
    );
    assert_eq!(
        receipt["wall"]["samples_ns"].as_array().map(Vec::len),
        Some(2)
    );
}

#[cfg(feature = "jemalloc-wall")]
#[test]
fn wall_cli_rejects_unknown_duplicate_missing_and_zero_arguments() {
    for arguments in [
        &["wall", "--unknown"] as &[&str],
        &["wall", "--warmups", "1", "--warmups", "2"],
        &["wall", "--samples"],
        &["wall", "--samples", "0"],
    ] {
        assert!(
            !invoke_probe(arguments).status.success(),
            "accepted {arguments:?}"
        );
    }
}

#[cfg(feature = "dhat-heap")]
#[test]
fn heap_probe_excludes_setup_and_includes_measured_control_allocation() {
    let typed = typed_one_sample();
    let baseline = successful_receipt(&["heap"]);
    let phase = successful_receipt(&["heap", "--control", "phase-retained-1m"]);
    for receipt in [&baseline, &phase] {
        assert_common_receipt(receipt, "heap");
        assert_eq!(receipt["allocator"], "dhat");
        assert_eq!(receipt["run"]["warmups"], 0);
        assert_eq!(receipt["run"]["samples"], 1);
        assert_exact_phase_work(&receipt["warmup_work"], 0);
        assert_exact_phase_work(&receipt["measured_work"], 1);
        assert!(receipt["wall"].is_null());
    }
    assert_eq!(baseline["heap"]["control"], "none");
    assert_eq!(phase["heap"]["control"], "phase-retained-1m");
    assert_eq!(baseline["setup_work"], typed["setup_work"]);
    assert_eq!(baseline["measured_work"], typed["measured_work"]);
    assert_eq!(
        baseline["result"]["binder_digest"],
        typed["result"]["binder_digest"]
    );
    assert_eq!(
        baseline["result"]["product_arc_reused"],
        typed["result"]["product_arc_reused"]
    );
    assert_binary_uses_typed_receipts();

    for field in [
        "allocation_calls",
        "allocated_bytes",
        "live_bytes_at_snapshot",
        "peak_live_bytes",
    ] {
        assert!(heap_stat(&baseline, field) > 0);
    }
    assert_eq!(
        heap_stat(&phase, "allocation_calls"),
        heap_stat(&baseline, "allocation_calls") + 1
    );
    for field in [
        "allocated_bytes",
        "live_bytes_at_snapshot",
        "peak_live_bytes",
    ] {
        assert_eq!(
            heap_stat(&phase, field),
            heap_stat(&baseline, field) + PHASE_CONTROL_BYTES
        );
    }
    assert!(heap_stat(&baseline, "allocated_bytes") >= heap_stat(&baseline, "peak_live_bytes"));
    assert!(
        heap_stat(&baseline, "peak_live_bytes") >= heap_stat(&baseline, "live_bytes_at_snapshot")
    );
}

#[cfg(feature = "dhat-heap")]
#[test]
fn heap_cli_rejects_unknown_duplicate_and_missing_arguments() {
    for arguments in [
        &["heap", "--unknown"] as &[&str],
        &["heap", "--control"],
        &[
            "heap",
            "--control",
            "none",
            "--control",
            "phase-retained-1m",
        ],
    ] {
        assert!(
            !invoke_probe(arguments).status.success(),
            "accepted {arguments:?}"
        );
    }
}
