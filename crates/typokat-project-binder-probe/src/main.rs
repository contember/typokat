#[cfg(all(feature = "jemalloc-wall", feature = "dhat-heap"))]
compile_error!("jemalloc-wall and dhat-heap are mutually exclusive");

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

#[cfg(feature = "jemalloc-wall")]
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use serde_json::{json, Value};
#[cfg(feature = "jemalloc-wall")]
use std::time::Instant;
use typokat_library::project_binder_probe::{
    with_project_binder_probe, ProjectBinderPhaseCountersV1, ProjectBinderProbeResult,
    ProjectBinderProbeSetupCountersV1,
};

const USER_SOURCE_PATH: &str = "/candidate/reflect_packaged.ts";
const USER_SOURCE_BYTES: u64 = 202;
const USER_SOURCE_DIGEST: &str = "a5e86f4497dc86ba58452644c3baa4ba5626d2bb52f1b98166854fae88a61d82";
#[cfg(feature = "dhat-heap")]
const PHASE_CONTROL_BYTES: usize = 1024 * 1024;

#[cfg(feature = "dhat-heap")]
struct DhatArtifactGuard(&'static str);

#[cfg(feature = "dhat-heap")]
impl Drop for DhatArtifactGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0);
    }
}

fn setup_json(counters: ProjectBinderProbeSetupCountersV1) -> Value {
    json!({
        "declaration_product_initializations": counters.declaration_product_initializations,
        "user_parses": counters.user_parses,
    })
}

fn phase_json(counters: ProjectBinderPhaseCountersV1) -> Value {
    json!({
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

struct ReceiptInputs<'a> {
    mode: &'a str,
    allocator: &'a str,
    setup: ProjectBinderProbeSetupCountersV1,
    product_digest: &'a str,
    warmups: usize,
    samples: usize,
    warmup_work: ProjectBinderPhaseCountersV1,
    measured_work: ProjectBinderPhaseCountersV1,
    result: &'a ProjectBinderProbeResult,
    binder_digests: &'a [String],
}

fn common_receipt(inputs: ReceiptInputs<'_>) -> Value {
    let ReceiptInputs {
        mode,
        allocator,
        setup,
        product_digest,
        warmups,
        samples,
        warmup_work,
        measured_work,
        result,
        binder_digests,
    } = inputs;
    json!({
        "schema": 3,
        "route": "immutable-library-product-to-fresh-project-binder",
        "mode": mode,
        "allocator": allocator,
        "build": {
            "profile": "release",
            "visible_parallelism": std::thread::available_parallelism().map_or(0, usize::from),
        },
        "authentication": {
            "phase_receipt": "typokat-library::ProjectBinderPhaseReceiptV1",
        },
        "library": {
            "source_count": 82,
            "product_digest": product_digest,
        },
        "user_project": {
            "source_count": 1,
            "path": USER_SOURCE_PATH,
            "source_bytes": USER_SOURCE_BYTES,
            "source_sha256": USER_SOURCE_DIGEST,
            "prepared_before_measurement": true,
        },
        "run": {
            "warmups": warmups,
            "samples": samples,
        },
        "setup_work": setup_json(setup),
        "warmup_work": phase_json(warmup_work),
        "measured_work": phase_json(measured_work),
        "result": {
            "product_arc_reused": result.product_arc_reused,
            "binder_digest": result.binder_digest,
            "binder_digests": binder_digests,
        },
    })
}

#[cfg(feature = "jemalloc-wall")]
fn parse_wall_arguments(arguments: &[String]) -> Result<(usize, usize), String> {
    let mut warmups = None;
    let mut samples = None;
    let mut index = 0;
    while index < arguments.len() {
        let flag = &arguments[index];
        let target = match flag.as_str() {
            "--warmups" => &mut warmups,
            "--samples" => &mut samples,
            _ => return Err(format!("unknown wall argument {flag:?}")),
        };
        if target.is_some() {
            return Err(format!("duplicate wall argument {flag:?}"));
        }
        let value = arguments
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        *target = Some(
            value
                .parse::<usize>()
                .map_err(|_| format!("invalid value for {flag}"))?,
        );
        index += 2;
    }
    let samples = samples.unwrap_or(31);
    if samples == 0 {
        return Err("wall samples must be positive".to_owned());
    }
    Ok((warmups.unwrap_or(10), samples))
}

#[cfg(feature = "jemalloc-wall")]
fn run_wall(arguments: &[String]) -> Result<String, String> {
    let (warmups, samples) = parse_wall_arguments(arguments)?;
    with_project_binder_probe(|probe| {
        let setup = probe.setup_receipt().counters();
        let product_digest = probe.product_digest().to_owned();
        let mut warmup_work = ProjectBinderPhaseCountersV1::default();
        for _ in 0..warmups {
            let sample = probe.bind_one().map_err(|error| error.to_string())?;
            warmup_work = warmup_work.saturating_add(sample.phase_receipt().counters());
            sample.finish_result().map_err(|error| error.to_string())?;
        }
        let mut measured_work = ProjectBinderPhaseCountersV1::default();
        let mut durations = Vec::with_capacity(samples);
        let mut binder_digests = Vec::with_capacity(samples);
        let mut last_result = None;
        for _ in 0..samples {
            let started = Instant::now();
            let sample = probe.bind_one().map_err(|error| error.to_string())?;
            let elapsed = started.elapsed();
            measured_work = measured_work.saturating_add(sample.phase_receipt().counters());
            durations.push(u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX).max(1));
            let result = sample.finish_result().map_err(|error| error.to_string())?;
            binder_digests.push(result.binder_digest.clone());
            last_result = Some(result);
        }
        let result =
            last_result.ok_or_else(|| "wall probe requires at least one sample".to_owned())?;
        let mut ordered = durations.clone();
        ordered.sort_unstable();
        let median = ordered[ordered.len() / 2];
        let mut receipt = common_receipt(ReceiptInputs {
            mode: "wall",
            allocator: "tikv-jemallocator",
            setup,
            product_digest: &product_digest,
            warmups,
            samples,
            warmup_work,
            measured_work,
            result: &result,
            binder_digests: &binder_digests,
        });
        receipt["wall"] = json!({
            "samples_ns": durations,
            "median_ns": median,
        });
        receipt["heap"] = Value::Null;
        serde_json::to_string(&receipt).map_err(|error| error.to_string())
    })
    .map_err(|error| error.to_string())?
}

#[cfg(feature = "dhat-heap")]
fn run_heap(arguments: &[String]) -> Result<String, String> {
    let control = match arguments {
        [] => "none",
        [flag] if flag == "--control" => return Err("missing value for --control".to_owned()),
        [flag, value] if flag == "--control" => value.as_str(),
        [flag, ..] if flag == "--control" => {
            return Err("duplicate or trailing heap arguments".to_owned());
        }
        [argument, ..] => return Err(format!("unknown heap argument {argument:?}")),
    };
    if !matches!(control, "none" | "phase-retained-1m") {
        return Err(format!("unknown heap control {control:?}"));
    }
    with_project_binder_probe(|probe| {
        let setup = probe.setup_receipt().counters();
        let product_digest = probe.product_digest().to_owned();
        let _artifact = DhatArtifactGuard("dhat-heap.json");
        let profiler = dhat::Profiler::new_heap();
        let phase_control =
            (control == "phase-retained-1m").then(|| vec![0_u8; PHASE_CONTROL_BYTES]);
        let sample = probe.bind_one().map_err(|error| error.to_string())?;
        let measured_work = sample.phase_receipt().counters();
        let stats = dhat::HeapStats::get();
        drop(profiler);
        let result = sample.finish_result().map_err(|error| error.to_string())?;
        let binder_digests = [result.binder_digest.clone()];
        let mut receipt = common_receipt(ReceiptInputs {
            mode: "heap",
            allocator: "dhat",
            setup,
            product_digest: &product_digest,
            warmups: 0,
            samples: 1,
            warmup_work: ProjectBinderPhaseCountersV1::default(),
            measured_work,
            result: &result,
            binder_digests: &binder_digests,
        });
        receipt["wall"] = Value::Null;
        receipt["heap"] = json!({
            "control": control,
            "allocation_calls": stats.total_blocks,
            "allocated_bytes": stats.total_bytes,
            "live_bytes_at_snapshot": u64::try_from(stats.curr_bytes).unwrap_or(u64::MAX),
            "peak_live_bytes": u64::try_from(stats.max_bytes).unwrap_or(u64::MAX),
        });
        let serialized = serde_json::to_string(&receipt).map_err(|error| error.to_string());
        drop(phase_control);
        serialized
    })
    .map_err(|error| error.to_string())?
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("project binder probe requires a release build");
        std::process::exit(2);
    }
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let result = match arguments.first().map(String::as_str) {
        #[cfg(feature = "jemalloc-wall")]
        Some("wall") => run_wall(&arguments[1..]),
        #[cfg(feature = "dhat-heap")]
        Some("heap") => run_heap(&arguments[1..]),
        Some(mode) => Err(format!("unsupported probe mode {mode:?}")),
        None => Err("missing probe mode".to_owned()),
    };
    match result {
        Ok(receipt) => println!("{receipt}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
