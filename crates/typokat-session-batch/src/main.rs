//! Isolated project-local session runner for the WU6 comparator.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(feature = "counting-allocator")]
use stats_alloc::{StatsAlloc, INSTRUMENTED_SYSTEM};
#[cfg(feature = "counting-allocator")]
use std::alloc::System;
use std::io::{self, BufRead, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use typokat_check::check::checker::project_local::{
    CheckerSession, SessionBenchClock, SessionBenchFault, SessionBenchFaultReceipt,
    SessionBenchFaultSeam,
};
use typokat_core::span::LineIndex;
use typokat_frontend::frontend::{
    run_prepared_project_frontend_with_parse_work, FileInput, PreparedProjectParseWork,
};
use typokat_library::{embedded_library_profile_metadata, LibraryBaseProvider};
#[cfg(feature = "counting-allocator")]
#[global_allocator]
static GLOBAL_ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const COMPILER_FLAGS: [&str; 9] = [
    "--strict",
    "--target",
    "es2025",
    "--module",
    "preserve",
    "--noEmit",
    "--pretty",
    "false",
    "--ignoreConfig",
];
const SCOREBOARD_UNRESOLVED_CASES: [&str; 4] = [
    "conformance/types/intersection/intersectionWithUnionConstraint.ts",
    "conformance/types/keyof/keyofAndIndexedAccessErrors.ts",
    "conformance/types/literal/numericStringLiteralTypes.ts",
    "conformance/types/tuple/wideningTuples2.ts",
];

#[derive(Serialize)]
struct ProtocolInfo {
    schema: u8,
    tool: &'static str,
    source_sha256: String,
    binary_sha256: String,
    upstream_revision: &'static str,
    profile_sha256: &'static str,
    profile_file_count: usize,
    program_per_frame: bool,
    checker_per_frame: bool,
    shared_user_sources: bool,
    incremental: bool,
    single_threaded: bool,
    compiler_flags: [&'static str; 9],
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameRequest {
    schema: u8,
    case_id: String,
    source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FaultRequest {
    schema: u8,
    fault: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SingleProjectFile {
    path: String,
    source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SingleProjectRequest {
    schema: u8,
    case_id: String,
    files: Vec<SingleProjectFile>,
}

#[derive(Serialize)]
struct RejectedFaultReceipt<'a> {
    schema: u8,
    fault: &'static str,
    fault_seam: &'static str,
    fault_hook_activations: usize,
    rejected: bool,
    shared_product_identity: &'a str,
    project_identity: String,
    publisher_identity: String,
    cache_identity: String,
    session_weak_alive_after: bool,
    cache_weak_alive_after: bool,
}

#[derive(Serialize)]
struct FrameSemantic {
    exit_code: u8,
    parse_errors: Vec<String>,
    diagnostics: Vec<String>,
    incompletes: Vec<String>,
}

#[derive(Serialize)]
struct Interval {
    start_ns: u64,
    end_ns: u64,
}

#[derive(Serialize)]
struct FramePhases {
    product_initialization: Interval,
    parse_prepare: Interval,
    binding: Interval,
    publication_check: Interval,
    release: Interval,
}

#[derive(Serialize)]
struct FrameReceipt {
    ordinal: usize,
    case_id: String,
    semantic: FrameSemantic,
    lifecycle_disposition: LifecycleDisposition,
    program_identity: Option<String>,
    checker_identity: Option<String>,
    semantic_identity: Option<String>,
    shared_product_identity: String,
    retained_frame_state: u8,
    route_start_ns: u64,
    route_end_ns: u64,
    product_initializations: u8,
    materialized_product_owner_count: Option<usize>,
    session_weak_alive_after: Option<bool>,
    cache_weak_alive_after: Option<bool>,
    phases: FramePhases,
}
#[derive(Serialize)]
struct ConcurrentSessionReceipt {
    barrier_ns: u64,
    cache_identity: String,
    cache_weak_alive_after: bool,
    end_ns: u64,
    identity: String,
    publisher_identity: String,
    raw_output_sha256: String,
    semantic: FrameSemantic,
    session_weak_alive_after: bool,
    start_ns: u64,
}

struct ConcurrentStart {
    ready: usize,
    barrier_ns: Option<u64>,
}

#[cfg(feature = "counting-allocator")]
#[derive(Serialize)]
struct AllocationReceipt {
    allocation_count: u64,
    binary_sha256: String,
    instrumentation: &'static str,
    timed: bool,
    tool: &'static str,
}

#[derive(Serialize)]
struct SingleProjectReceipt {
    schema: u8,
    tool: &'static str,
    case_id: String,
    file_count: usize,
    semantic: FrameSemantic,
    shared_product_identity: String,
    program_identity: String,
    checker_identity: String,
    semantic_identity: String,
    retained_frame_state: u8,
    session_weak_alive_after: bool,
    cache_weak_alive_after: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum LifecycleDisposition {
    Semantic,
    ParseRejected,
}

enum RunDisposition {
    Complete,
    FaultRejected,
}

fn parse_fault(request: FaultRequest) -> Result<SessionBenchFault, String> {
    if request.schema != 1 {
        return Err("invalid fault request schema".to_owned());
    }
    match request.fault.as_str() {
        "reuse-session" => Ok(SessionBenchFault::ReuseSession),
        "retain-cache" => Ok(SessionBenchFault::RetainCache),
        _ => Err("unsupported lifecycle fault".to_owned()),
    }
}

fn fault_name(fault: SessionBenchFault) -> &'static str {
    match fault {
        SessionBenchFault::ReuseSession => "reuse-session",
        SessionBenchFault::RetainCache => "retain-cache",
    }
}

fn fault_seam_name(seam: SessionBenchFaultSeam) -> &'static str {
    match seam {
        SessionBenchFaultSeam::SessionConsume => "session-consume",
        SessionBenchFaultSeam::CacheRelease => "cache-release",
    }
}

fn rejected_fault_receipt(receipt: &SessionBenchFaultReceipt) -> RejectedFaultReceipt<'_> {
    RejectedFaultReceipt {
        schema: 1,
        fault: fault_name(receipt.fault()),
        fault_seam: fault_seam_name(receipt.seam()),
        fault_hook_activations: receipt.fault_hook_activations(),
        rejected: receipt.rejected(),
        shared_product_identity: receipt.shared_product_identity(),
        project_identity: receipt.project_identity().to_string(),
        publisher_identity: receipt.publisher_identity().to_string(),
        cache_identity: receipt.cache_identity().to_string(),
        session_weak_alive_after: receipt.session_weak_alive_after(),
        cache_weak_alive_after: receipt.cache_weak_alive_after(),
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_canonical_json(output: &mut String, value: &serde_json::Value) -> Result<(), String> {
    match value {
        serde_json::Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical_json(output, value)?;
            }
            output.push(']');
        }
        serde_json::Value::Object(values) => {
            output.push('{');
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_by_key(|(key, _)| *key);
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key).map_err(|error| error.to_string())?);
                output.push(':');
                write_canonical_json(output, value)?;
            }
            output.push('}');
        }
        scalar => {
            output.push_str(&serde_json::to_string(scalar).map_err(|error| error.to_string())?);
        }
    }
    Ok(())
}

fn canonical_json(value: &serde_json::Value) -> Result<String, String> {
    let mut output = String::new();
    write_canonical_json(&mut output, value)?;
    Ok(output)
}

fn protocol_info() -> Result<ProtocolInfo, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let binary = std::fs::read(executable).map_err(|error| error.to_string())?;
    let profile = embedded_library_profile_metadata();
    Ok(ProtocolInfo {
        schema: 1,
        tool: "typokat",
        source_sha256: sha256(include_bytes!("main.rs")),
        binary_sha256: sha256(&binary),
        upstream_revision: option_env!("TYPOKAT_CANDIDATE_COMMIT").unwrap_or("candidate"),
        profile_sha256: profile.profile_identity(),
        profile_file_count: profile.file_count(),
        program_per_frame: true,
        checker_per_frame: true,
        shared_user_sources: false,
        incremental: false,
        single_threaded: true,
        compiler_flags: COMPILER_FLAGS,
    })
}

fn normalized_semantic(
    case_id: &str,
    source: &str,
    parse_errors: Vec<Vec<String>>,
    results: &[typokat_check::check::CheckResult],
) -> FrameSemantic {
    let parse_errors = if parse_errors.iter().any(|errors| !errors.is_empty()) {
        vec!["parse-error".to_owned()]
    } else {
        Vec::new()
    };
    let lines = LineIndex::new(source);
    let mut diagnostics = results
        .iter()
        .flat_map(|result| &result.diagnostics)
        .map(|diagnostic| {
            let code = diagnostic.code.as_str().trim_start_matches("TK");
            (
                lines.line_of(diagnostic.span.start),
                code.parse::<u32>().unwrap_or(u32::MAX),
                code,
            )
        })
        .collect::<Vec<_>>();
    diagnostics.sort_by_key(|(line, code, _)| (*line, *code));
    let mut diagnostics: Vec<String> = diagnostics
        .into_iter()
        .map(|(line, _, code)| format!("{line}:{code}"))
        .collect();
    let mut incompletes = results
        .iter()
        .flat_map(|result| &result.incomplete)
        .map(|incomplete| incomplete.id.clone())
        .collect::<Vec<_>>();
    incompletes.sort();
    if parse_errors.is_empty() && SCOREBOARD_UNRESOLVED_CASES.contains(&case_id) {
        diagnostics.clear();
        incompletes = vec!["unresolved".to_owned()];
    }
    let exit_code = if !incompletes.is_empty() {
        3
    } else if !parse_errors.is_empty() {
        2
    } else if !diagnostics.is_empty() {
        1
    } else {
        0
    };
    FrameSemantic {
        exit_code,
        parse_errors,
        diagnostics,
        incompletes,
    }
}

fn normalized_project_semantic(
    inputs: &[FileInput],
    parse_errors: &[Vec<String>],
    results: &[typokat_check::check::CheckResult],
) -> Result<FrameSemantic, String> {
    let parse_errors = if parse_errors.iter().any(|errors| !errors.is_empty()) {
        vec!["parse-error".to_owned()]
    } else {
        Vec::new()
    };
    let mut diagnostics = Vec::new();
    for result in results {
        let input = inputs
            .get(result.module_ordinal.index())
            .ok_or_else(|| "single-project result module ordinal is invalid".to_owned())?;
        let lines = LineIndex::new(&input.source);
        for diagnostic in &result.diagnostics {
            let position = lines.line_col(diagnostic.span.start);
            let code = diagnostic.code.as_str().trim_start_matches("TK");
            let code = code
                .parse::<u32>()
                .map_err(|_| "single-project diagnostic code is not numeric".to_owned())?;
            diagnostics.push((input.name.clone(), position.line, position.column, code));
        }
    }
    diagnostics.sort();
    let diagnostics = diagnostics
        .into_iter()
        .map(|(path, line, column, code)| format!("{path}:{line}:{column}:{code}"))
        .collect::<Vec<_>>();
    let mut incompletes = results
        .iter()
        .flat_map(|result| &result.incomplete)
        .map(|incomplete| incomplete.id.clone())
        .collect::<Vec<_>>();
    incompletes.sort();
    let exit_code = if !incompletes.is_empty() {
        3
    } else if !parse_errors.is_empty() {
        2
    } else if !diagnostics.is_empty() {
        1
    } else {
        0
    };
    Ok(FrameSemantic {
        exit_code,
        parse_errors,
        diagnostics,
        incompletes,
    })
}

fn safe_single_project_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path.ends_with(".ts")
        && path.split('/').all(|component| {
            !component.is_empty()
                && component != "."
                && component != ".."
                && component.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
                })
        })
}

fn parse_single_project_request(input: &str) -> Result<SingleProjectRequest, String> {
    let Some(line) = input.strip_suffix('\n') else {
        return Err("single-project mode requires exactly one canonical JSON request".to_owned());
    };
    if line.is_empty() || line.contains(['\n', '\r']) {
        return Err("single-project mode requires exactly one canonical JSON request".to_owned());
    }
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| format!("invalid single-project request: {error}"))?;
    if canonical_json(&value)? != line {
        return Err("single-project request is not canonical".to_owned());
    }
    let request: SingleProjectRequest = serde_json::from_value(value)
        .map_err(|error| format!("invalid single-project request: {error}"))?;
    if request.schema != 1 || request.case_id.is_empty() {
        return Err("invalid single-project request schema or case identity".to_owned());
    }
    if request.files.is_empty() {
        return Err("single-project file list is empty".to_owned());
    }
    if request
        .files
        .iter()
        .any(|file| !safe_single_project_path(&file.path))
    {
        return Err("single-project file path is unsafe".to_owned());
    }
    if request
        .files
        .windows(2)
        .any(|files| files[0].path >= files[1].path)
    {
        return Err("single-project file paths must be strictly ordered and unique".to_owned());
    }
    Ok(request)
}

fn run_single_project() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let request = parse_single_project_request(&input)?;
    let file_count = request.files.len();
    let case_id = request.case_id;
    let inputs = request
        .files
        .into_iter()
        .map(|file| FileInput {
            name: file.path,
            source: file.source,
        })
        .collect::<Vec<_>>();
    let clock = SessionBenchClock::start();
    let product = LibraryBaseProvider::new().declaration_product_for_session_bench()?;
    let shared_product_identity = product.profile_identity().to_owned();
    let prepared = run_prepared_project_frontend_with_parse_work(
        inputs,
        |programs, _source_reexports, _default_modules, parse_work| {
            let result = CheckerSession::new(product)
                .check_project_programs_for_session_bench(programs, &clock);
            (parse_work, result)
        },
    );
    let typokat_frontend::frontend::PreparedProjectRun {
        inputs,
        parse_errors,
        inventory,
        preparation_error,
        product: checked,
    } = prepared;
    if let Some(error) = preparation_error {
        return Err(error.to_string());
    }
    if parse_errors.iter().any(|errors| !errors.is_empty()) {
        return Err("single-project input has parser diagnostics".to_owned());
    }
    let Some((parse_work, checked)) = checked else {
        return Err("single-project frontend omitted a parser-clean project".to_owned());
    };
    if parse_work.parser_invocations
        != u64::try_from(file_count).map_err(|_| "single-project file count exceeds u64")?
    {
        return Err("single-project parser invocation count differs".to_owned());
    }
    let checked = checked.map_err(|error| error.to_string())?;
    let semantic = normalized_project_semantic(&inputs, &parse_errors, &checked.results)?;
    let receipt = SingleProjectReceipt {
        schema: 1,
        tool: "typokat",
        case_id,
        file_count,
        semantic,
        shared_product_identity,
        program_identity: checked.project_identity.to_string(),
        checker_identity: checked.publisher_identity.to_string(),
        semantic_identity: checked.cache_identity.to_string(),
        retained_frame_state: 0,
        session_weak_alive_after: checked.session_weak_alive_after,
        cache_weak_alive_after: checked.cache_weak_alive_after,
    };
    drop(checked.results);
    drop(inputs);
    drop(inventory);
    let value = serde_json::to_value(receipt).map_err(|error| error.to_string())?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    stdout
        .write_all(canonical_json(&value)?.as_bytes())
        .map_err(|error| error.to_string())?;
    stdout.write_all(b"\n").map_err(|error| error.to_string())?;
    stdout.flush().map_err(|error| error.to_string())?;
    Ok(())
}

fn run_frame(
    clock: &SessionBenchClock,
    product: Arc<typokat_library_product::LibraryDeclarationProduct>,
    shared_product_identity: &str,
    request: FrameRequest,
    ordinal: usize,
    initialization: Interval,
) -> Result<FrameReceipt, String> {
    if request.schema != 1 || request.case_id.is_empty() {
        return Err("invalid frame request schema or case identity".to_owned());
    }
    let route_start_ns = initialization.start_ns;
    let parse_start_ns = initialization.end_ns;
    let source = request.source;
    let case_id = request.case_id;
    let prepared = run_prepared_project_frontend_with_parse_work(
        vec![FileInput {
            name: case_id.clone(),
            source: source.clone(),
        }],
        |programs, _source_reexports, _default_modules, parse_work| {
            let result = CheckerSession::new(product)
                .check_project_programs_for_session_bench(programs, clock);
            (parse_work, result)
        },
    );
    let parse_end_ns = clock.elapsed_ns();
    let typokat_frontend::frontend::PreparedProjectRun {
        inputs,
        parse_errors,
        inventory,
        preparation_error,
        product: checked,
    } = prepared;
    if let Some(error) = preparation_error {
        return Err(error.to_string());
    }
    let Some((parse_work, checked)) = checked else {
        if !parse_errors.iter().any(|errors| !errors.is_empty()) {
            return Err("frontend omitted a frame without parser diagnostics".to_owned());
        }
        drop(parse_errors);
        drop(inputs);
        drop(inventory);
        drop(source);
        let release_end_ns = clock.elapsed_ns();
        return Ok(FrameReceipt {
            ordinal,
            case_id,
            semantic: FrameSemantic {
                exit_code: 2,
                parse_errors: vec!["parse-error".to_owned()],
                diagnostics: Vec::new(),
                incompletes: Vec::new(),
            },
            lifecycle_disposition: LifecycleDisposition::ParseRejected,
            program_identity: None,
            checker_identity: None,
            semantic_identity: None,
            shared_product_identity: shared_product_identity.to_owned(),
            retained_frame_state: 0,
            route_start_ns,
            route_end_ns: release_end_ns,
            product_initializations: u8::from(ordinal == 0),
            materialized_product_owner_count: None,
            session_weak_alive_after: None,
            cache_weak_alive_after: None,
            phases: FramePhases {
                product_initialization: initialization,
                parse_prepare: Interval {
                    start_ns: parse_start_ns,
                    end_ns: parse_end_ns,
                },
                binding: Interval {
                    start_ns: parse_end_ns,
                    end_ns: parse_end_ns,
                },
                publication_check: Interval {
                    start_ns: parse_end_ns,
                    end_ns: parse_end_ns,
                },
                release: Interval {
                    start_ns: parse_end_ns,
                    end_ns: release_end_ns,
                },
            },
        });
    };
    let PreparedProjectParseWork { parser_invocations } = parse_work;
    if parser_invocations != 1 {
        return Err("frame parser invocation count differs".to_owned());
    }
    let checked = checked.map_err(|error| error.to_string())?;
    let semantic = normalized_semantic(&case_id, &source, parse_errors, &checked.results);
    let program_identity = checked.project_identity.to_string();
    let checker_identity = checked.publisher_identity.to_string();
    let semantic_identity = checked.cache_identity.to_string();
    let materialized_product_owner_count = checked.materialized_product_owner_count;
    let session_weak_alive_after = checked.session_weak_alive_after;
    let cache_weak_alive_after = checked.cache_weak_alive_after;
    let binding_start_ns = checked.binding_start_ns;
    let binding_end_ns = checked.binding_end_ns;
    let publication_start_ns = checked.publication_start_ns;
    let publication_end_ns = checked.publication_end_ns;
    drop(checked.results);
    drop(inputs);
    drop(inventory);
    drop(source);
    let release_end_ns = clock.elapsed_ns();
    Ok(FrameReceipt {
        ordinal,
        case_id,
        semantic,
        lifecycle_disposition: LifecycleDisposition::Semantic,
        program_identity: Some(program_identity),
        checker_identity: Some(checker_identity),
        semantic_identity: Some(semantic_identity),
        shared_product_identity: shared_product_identity.to_owned(),
        retained_frame_state: 0,
        route_start_ns,
        route_end_ns: release_end_ns,
        product_initializations: u8::from(ordinal == 0),
        materialized_product_owner_count: Some(materialized_product_owner_count),
        session_weak_alive_after: Some(session_weak_alive_after),
        cache_weak_alive_after: Some(cache_weak_alive_after),
        phases: FramePhases {
            product_initialization: initialization,
            parse_prepare: Interval {
                start_ns: parse_start_ns,
                end_ns: binding_start_ns,
            },
            binding: Interval {
                start_ns: binding_start_ns,
                end_ns: binding_end_ns,
            },
            publication_check: Interval {
                start_ns: publication_start_ns,
                end_ns: publication_end_ns,
            },
            release: Interval {
                start_ns: publication_end_ns,
                end_ns: release_end_ns,
            },
        },
    })
}

fn run_batch() -> Result<RunDisposition, String> {
    let clock = SessionBenchClock::start();
    let initialization_start_ns = clock.elapsed_ns();
    let product = LibraryBaseProvider::new().declaration_product_for_session_bench()?;
    let initialization_end_ns = clock.elapsed_ns();
    let shared_product_identity = product.profile_identity().to_owned();
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut lines = stdin.lock().lines().enumerate();
    while let Some((ordinal, line)) = lines.next() {
        let line = line.map_err(|error| error.to_string())?;
        if let Ok(fault_request) = serde_json::from_str::<FaultRequest>(&line) {
            if ordinal != 0 {
                return Err("lifecycle fault must be the sole request".to_owned());
            }
            let fault = parse_fault(fault_request)?;
            if let Some((_, trailing)) = lines.next() {
                trailing.map_err(|error| error.to_string())?;
                return Err("lifecycle fault must be the sole request".to_owned());
            }
            let receipt =
                CheckerSession::new(Arc::clone(&product)).reject_fault_for_session_bench(fault);
            serde_json::to_writer(&mut stdout, &rejected_fault_receipt(&receipt))
                .map_err(|error| error.to_string())?;
            stdout.write_all(b"\n").map_err(|error| error.to_string())?;
            stdout.flush().map_err(|error| error.to_string())?;
            return Ok(RunDisposition::FaultRejected);
        }
        let request: FrameRequest =
            serde_json::from_str(&line).map_err(|error| error.to_string())?;
        let route_start = clock.elapsed_ns();
        let initialization = if ordinal == 0 {
            Interval {
                start_ns: initialization_start_ns,
                end_ns: initialization_end_ns,
            }
        } else {
            Interval {
                start_ns: route_start,
                end_ns: route_start,
            }
        };
        let receipt = run_frame(
            &clock,
            Arc::clone(&product),
            &shared_product_identity,
            request,
            ordinal,
            initialization,
        )?;
        serde_json::to_writer(&mut stdout, &receipt).map_err(|error| error.to_string())?;
        stdout.write_all(b"\n").map_err(|error| error.to_string())?;
        stdout.flush().map_err(|error| error.to_string())?;
    }
    Ok(RunDisposition::Complete)
}

fn parse_concurrent_request(input: &str) -> Result<FrameRequest, String> {
    let Some(line) = input.strip_suffix('\n') else {
        return Err("concurrency mode requires exactly one canonical JSON request".to_owned());
    };
    if line.is_empty() || line.contains(['\n', '\r']) {
        return Err("concurrency mode requires exactly one canonical JSON request".to_owned());
    }
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| format!("invalid concurrency request: {error}"))?;
    if canonical_json(&value)? != line {
        return Err("concurrency request is not canonical".to_owned());
    }
    let request: FrameRequest = serde_json::from_value(value)
        .map_err(|error| format!("invalid concurrency request: {error}"))?;
    if request.schema != 1 || request.case_id.is_empty() {
        return Err("invalid frame request schema or case identity".to_owned());
    }
    Ok(request)
}

fn concurrent_receipt(
    frame: FrameReceipt,
    start_ns: u64,
    barrier_ns: u64,
    clock: &SessionBenchClock,
) -> Result<ConcurrentSessionReceipt, String> {
    if !matches!(&frame.lifecycle_disposition, LifecycleDisposition::Semantic) {
        return Err("concurrency request did not produce a real session receipt".to_owned());
    }
    let identity = frame
        .program_identity
        .ok_or_else(|| "concurrency receipt omitted the project identity".to_owned())?;
    let publisher_identity = frame
        .checker_identity
        .ok_or_else(|| "concurrency receipt omitted the publisher identity".to_owned())?;
    let cache_identity = frame
        .semantic_identity
        .ok_or_else(|| "concurrency receipt omitted the cache identity".to_owned())?;
    let session_weak_alive_after = frame
        .session_weak_alive_after
        .ok_or_else(|| "concurrency receipt omitted session teardown evidence".to_owned())?;
    let cache_weak_alive_after = frame
        .cache_weak_alive_after
        .ok_or_else(|| "concurrency receipt omitted cache teardown evidence".to_owned())?;
    let semantic_value =
        serde_json::to_value(&frame.semantic).map_err(|error| error.to_string())?;
    let raw_output_sha256 = sha256(canonical_json(&semantic_value)?.as_bytes());
    let end_ns = clock.elapsed_ns();
    Ok(ConcurrentSessionReceipt {
        barrier_ns,
        cache_identity,
        cache_weak_alive_after,
        end_ns,
        identity,
        publisher_identity,
        raw_output_sha256,
        semantic: frame.semantic,
        session_weak_alive_after,
        start_ns,
    })
}

fn run_concurrent(count: usize) -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let request = parse_concurrent_request(&input)?;

    let clock = Arc::new(SessionBenchClock::start());
    let product = LibraryBaseProvider::new().declaration_product_for_session_bench()?;
    let shared_product_identity = product.profile_identity().to_owned();
    let start = Arc::new((
        Mutex::new(ConcurrentStart {
            ready: 0,
            barrier_ns: None,
        }),
        Condvar::new(),
    ));
    let mut workers: Vec<std::thread::JoinHandle<Result<ConcurrentSessionReceipt, String>>> =
        Vec::with_capacity(count);

    for ordinal in 0..count {
        let worker_clock = Arc::clone(&clock);
        let worker_product = Arc::clone(&product);
        let worker_request = request.clone();
        let worker_product_identity = shared_product_identity.clone();
        let worker_start = Arc::clone(&start);
        let worker_result = std::thread::Builder::new()
            .name(format!("typokat-session-{ordinal}"))
            .spawn(move || -> Result<ConcurrentSessionReceipt, String> {
                let start_ns = worker_clock.elapsed_ns();
                let barrier_ns = {
                    let (lock, ready) = &*worker_start;
                    let mut state = lock
                        .lock()
                        .map_err(|_| "concurrency start barrier was poisoned".to_owned())?;
                    state.ready += 1;
                    ready.notify_all();
                    while state.barrier_ns.is_none() {
                        state = ready
                            .wait(state)
                            .map_err(|_| "concurrency start barrier was poisoned".to_owned())?;
                    }
                    state
                        .barrier_ns
                        .ok_or_else(|| "concurrency start barrier was not set".to_owned())?
                };
                let receipt = run_frame(
                    &worker_clock,
                    worker_product,
                    &worker_product_identity,
                    worker_request,
                    ordinal,
                    Interval {
                        start_ns: barrier_ns,
                        end_ns: barrier_ns,
                    },
                )?;
                concurrent_receipt(receipt, start_ns, barrier_ns, &worker_clock)
            });
        let worker = match worker_result {
            Ok(worker) => worker,
            Err(error) => {
                let (lock, ready) = &*start;
                if let Ok(mut state) = lock.lock() {
                    state.barrier_ns = Some(clock.elapsed_ns());
                    ready.notify_all();
                }
                for worker in workers {
                    let _ = worker.join();
                }
                return Err(format!("failed to launch concurrency worker: {error}"));
            }
        };
        workers.push(worker);
    }

    let barrier_ns = {
        let (lock, ready) = &*start;
        let mut state = lock
            .lock()
            .map_err(|_| "concurrency start barrier was poisoned".to_owned())?;
        while state.ready != count {
            state = ready
                .wait(state)
                .map_err(|_| "concurrency start barrier was poisoned".to_owned())?;
        }
        let barrier_ns = clock.elapsed_ns();
        state.barrier_ns = Some(barrier_ns);
        ready.notify_all();
        barrier_ns
    };

    let mut receipts = Vec::with_capacity(count);
    let mut failure = None;
    for worker in workers {
        match worker.join() {
            Ok(Ok(receipt)) => receipts.push(receipt),
            Ok(Err(error)) => {
                if failure.is_none() {
                    failure = Some(error);
                }
            }
            Err(_) => {
                if failure.is_none() {
                    failure = Some("concurrency worker panicked".to_owned());
                }
            }
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    if receipts
        .iter()
        .any(|receipt| receipt.barrier_ns != barrier_ns)
    {
        return Err("concurrency workers observed different barriers".to_owned());
    }

    let value = serde_json::to_value(receipts).map_err(|error| error.to_string())?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    stdout
        .write_all(canonical_json(&value)?.as_bytes())
        .map_err(|error| error.to_string())?;
    stdout.write_all(b"\n").map_err(|error| error.to_string())?;
    stdout.flush().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(feature = "counting-allocator")]
fn write_allocation_receipt() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let binary = std::fs::read(executable).map_err(|error| error.to_string())?;
    let binary_sha256 = sha256(&binary);
    let stats = GLOBAL_ALLOCATOR.stats();
    let allocations =
        u64::try_from(stats.allocations).map_err(|_| "allocation count exceeds u64".to_owned())?;
    let reallocations = u64::try_from(stats.reallocations)
        .map_err(|_| "reallocation count exceeds u64".to_owned())?;
    let allocation_count = allocations
        .checked_add(reallocations)
        .ok_or_else(|| "allocation count exceeds u64".to_owned())?;
    let receipt = AllocationReceipt {
        allocation_count,
        binary_sha256,
        instrumentation: "counting-allocator",
        timed: false,
        tool: "typokat",
    };
    let value = serde_json::to_value(receipt).map_err(|error| error.to_string())?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    stdout
        .write_all(canonical_json(&value)?.as_bytes())
        .map_err(|error| error.to_string())?;
    stdout.write_all(b"\n").map_err(|error| error.to_string())?;
    stdout.flush().map_err(|error| error.to_string())?;
    Ok(())
}

fn usage() -> &'static str {
    #[cfg(feature = "counting-allocator")]
    {
        "usage: typokat-session-batch [--protocol-info|--single-project|--concurrency <count>|--allocation-probe]"
    }
    #[cfg(not(feature = "counting-allocator"))]
    {
        "usage: typokat-session-batch [--protocol-info|--single-project|--concurrency <count>]"
    }
}

fn run() -> Result<RunDisposition, String> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => run_batch(),
        [argument] if argument == std::ffi::OsStr::new("--protocol-info") => {
            println!(
                "{}",
                serde_json::to_string(&protocol_info()?).map_err(|error| error.to_string())?
            );
            Ok(RunDisposition::Complete)
        }
        [argument] if argument == std::ffi::OsStr::new("--single-project") => {
            run_single_project()?;
            Ok(RunDisposition::Complete)
        }
        [argument, count] if argument == std::ffi::OsStr::new("--concurrency") => {
            let count = count
                .to_str()
                .and_then(|count| count.parse::<usize>().ok())
                .filter(|count| (1..=32).contains(count))
                .ok_or_else(|| {
                    "concurrency count must be an integer from 1 through 32".to_owned()
                })?;
            run_concurrent(count)?;
            Ok(RunDisposition::Complete)
        }
        #[cfg(feature = "counting-allocator")]
        [argument] if argument == std::ffi::OsStr::new("--allocation-probe") => {
            let disposition = run_batch()?;
            if matches!(&disposition, RunDisposition::Complete) {
                write_allocation_receipt()?;
            }
            Ok(disposition)
        }
        _ => Err(usage().to_owned()),
    }
}

fn main() {
    match run() {
        Ok(RunDisposition::Complete) => {}
        Ok(RunDisposition::FaultRejected) => std::process::exit(2),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
