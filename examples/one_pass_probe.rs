//! Disposable CLI for measuring the complete-combined test path.

use std::io::Write;
use std::process::ExitCode;

use typokat_check::check::checker::library_compiler::{
    compile_complete_combined_profile_for_test, InjectedLibrarySource, InjectedProfileError,
};
use typokat_check::check::checker::reporting_record::CheckerRecord;
use typokat_diagnostics::diagnostics::{self, Diagnostic, DiagnosticFormat, IncompleteSurface};
use typokat_frontend::frontend::{run_project_parse_only, FileInput};
use typokat_library::profile::ExactLibraryProfile;
use typokat_library::LibraryFileOrdinal;

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

const LIBRARY_COUNT: usize = 82;
const CHECK_STACK_SIZE: usize = 256 * 1024 * 1024;
const EXIT_ERRORS: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_INCOMPLETE: u8 = 3;
const USAGE: &str = "usage: one_pass_probe probe-info --format json\n       one_pass_probe check --format compact <file.ts>...";

struct OwnedSource {
    file_ordinal: LibraryFileOrdinal,
    name: String,
    source: String,
}

#[derive(Default)]
struct UserRecords {
    diagnostics: Vec<Diagnostic>,
    incomplete: Vec<IncompleteSurface>,
}

enum CheckWorkerOutput {
    Checked {
        sources: Vec<OwnedSource>,
        records: Vec<UserRecords>,
    },
    ParseRejected {
        parse_errors: Vec<Vec<String>>,
    },
}

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();
    let code = match run(&args, &mut stdout, &mut stderr) {
        Ok(code) => code,
        Err(error) => {
            let _write_failure = writeln!(stderr, "error: {error}");
            EXIT_USAGE
        }
    };
    ExitCode::from(code)
}

fn run(args: &[String], stdout: &mut impl Write, stderr: &mut impl Write) -> Result<u8, String> {
    match args.get(1).map(String::as_str) {
        Some("probe-info") => {
            if args.get(2).map(String::as_str) != Some("--format")
                || args.get(3).map(String::as_str) != Some("json")
                || args.len() != 4
            {
                return Err(format!("invalid probe-info arguments\n{USAGE}"));
            }
            write_probe_info(stdout)?;
            Ok(0)
        }
        Some("check") => {
            if args.get(2).map(String::as_str) != Some("--format")
                || args.get(3).map(String::as_str) != Some("compact")
                || args.len() < 5
            {
                return Err(format!("invalid check arguments\n{USAGE}"));
            }
            check_paths(&args[4..], stderr)
        }
        Some(other) => Err(format!("unknown command '{other}'\n{USAGE}")),
        None => Err(USAGE.to_owned()),
    }
}

fn write_probe_info(stdout: &mut impl Write) -> Result<(), String> {
    let profile = ExactLibraryProfile::load_packaged()
        .map_err(|error| format!("failed to load packaged library: {error}"))?;
    if profile.sources().len() != LIBRARY_COUNT {
        return Err(format!(
            "packaged library has {} files; expected {LIBRARY_COUNT}",
            profile.sources().len()
        ));
    }
    writeln!(
        stdout,
        "{{\"schema\":1,\"profile_sha256\":\"{}\",\"file_count\":{},\"probe_route\":\"test-only-complete-combined\",\"source_backed\":true,\"replay_index\":false}}",
        profile.profile_identity(),
        profile.sources().len()
    )
    .map_err(|error| format!("failed to write probe info: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("failed to flush probe info: {error}"))
}

fn check_paths(paths: &[String], stderr: &mut impl Write) -> Result<u8, String> {
    let profile = ExactLibraryProfile::load_packaged()
        .map_err(|error| format!("failed to load packaged library: {error}"))?;
    if profile.sources().len() != LIBRARY_COUNT {
        return Err(format!(
            "packaged library has {} files; expected {LIBRARY_COUNT}",
            profile.sources().len()
        ));
    }

    let mut user_sources = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        let ordinal = LIBRARY_COUNT
            .checked_add(index)
            .ok_or_else(|| "user source ordinal overflow".to_owned())?;
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read '{path}': {error}"))?;
        user_sources.push(OwnedSource {
            file_ordinal: LibraryFileOrdinal::new(ordinal),
            name: path.clone(),
            source,
        });
    }

    let output = on_check_worker(move || check_sources(profile, user_sources))??;
    match output {
        CheckWorkerOutput::ParseRejected { parse_errors } => {
            for file_errors in parse_errors {
                for error in file_errors {
                    writeln!(stderr, "error: {error}").map_err(|write_error| {
                        format!("failed to render parse error: {write_error}")
                    })?;
                }
            }
            stderr
                .flush()
                .map_err(|error| format!("failed to flush parse errors: {error}"))?;
            Ok(EXIT_ERRORS)
        }
        CheckWorkerOutput::Checked { sources, records } => {
            render_records(paths, &sources, &records, stderr)
        }
    }
}

fn check_sources(
    profile: ExactLibraryProfile,
    mut user_sources: Vec<OwnedSource>,
) -> Result<CheckWorkerOutput, String> {
    let user_count = user_sources.len();
    let mut sources = Vec::with_capacity(LIBRARY_COUNT.saturating_add(user_count));
    for library in profile.sources() {
        let source = std::str::from_utf8(library.bytes()).map_err(|error| {
            format!("library source '{}' is not UTF-8: {error}", library.name())
        })?;
        sources.push(OwnedSource {
            file_ordinal: library.ordinal(),
            name: library.name().to_owned(),
            source: source.to_owned(),
        });
    }
    sources.append(&mut user_sources);

    let compile_result = {
        let injected = sources
            .iter()
            .map(|source| InjectedLibrarySource {
                file_ordinal: source.file_ordinal,
                name: &source.name,
                source: &source.source,
            })
            .collect::<Vec<_>>();
        compile_complete_combined_profile_for_test(&injected, LIBRARY_COUNT)
    };
    let run = match compile_result {
        Ok((run, _runtime)) => run,
        Err(InjectedProfileError::Parse { file_ordinal, .. })
            if user_index(file_ordinal, user_count).is_some() =>
        {
            let user_sources = sources.split_off(LIBRARY_COUNT);
            let parsed = run_project_parse_only(
                user_sources
                    .into_iter()
                    .map(|source| FileInput {
                        name: source.name,
                        source: source.source,
                    })
                    .collect(),
            );
            if parsed.parse_errors.iter().all(Vec::is_empty) {
                return Err(
                    "one-pass compilation rejected a user parse without parser diagnostics"
                        .to_owned(),
                );
            }
            return Ok(CheckWorkerOutput::ParseRejected {
                parse_errors: parsed.parse_errors,
            });
        }
        Err(error) => return Err(format!("one-pass compilation failed: {error:?}")),
    };

    let mut records = (0..user_count)
        .map(|_| UserRecords::default())
        .collect::<Vec<_>>();
    for (key, record) in run.library_records {
        if key.file_ordinal.index() < LIBRARY_COUNT {
            continue;
        }
        let index = user_index(key.file_ordinal, user_count).ok_or_else(|| {
            format!(
                "one-pass compilation returned unknown user ordinal {}",
                key.file_ordinal.index()
            )
        })?;
        match record {
            CheckerRecord::Diagnostic(diagnostic) => {
                records[index].diagnostics.push(diagnostic);
            }
            CheckerRecord::Incomplete(incomplete) => {
                records[index].incomplete.push(incomplete);
            }
        }
    }

    Ok(CheckWorkerOutput::Checked {
        sources: sources.split_off(LIBRARY_COUNT),
        records,
    })
}

fn render_records(
    paths: &[String],
    sources: &[OwnedSource],
    records: &[UserRecords],
    stderr: &mut impl Write,
) -> Result<u8, String> {
    let mut had_errors = false;
    let mut had_incomplete = false;
    for ((path, source), records) in paths.iter().zip(sources).zip(records) {
        diagnostics::render_to_writer_with_format(
            stderr,
            path,
            &source.source,
            &records.diagnostics,
            DiagnosticFormat::Compact,
        )
        .map_err(|error| format!("failed to render diagnostics: {error}"))?;
        diagnostics::render_incomplete_to_writer_with_format(
            stderr,
            path,
            &source.source,
            &records.incomplete,
            DiagnosticFormat::Compact,
        )
        .map_err(|error| format!("failed to render incomplete surfaces: {error}"))?;
        had_errors |= records.diagnostics.iter().any(Diagnostic::is_error);
        had_incomplete |= !records.incomplete.is_empty();
    }
    stderr
        .flush()
        .map_err(|error| format!("failed to flush diagnostics: {error}"))?;

    Ok(if had_incomplete {
        EXIT_INCOMPLETE
    } else if had_errors {
        EXIT_ERRORS
    } else {
        0
    })
}

fn on_check_worker<T, Work>(work: Work) -> Result<T, String>
where
    T: Send,
    Work: FnOnce() -> T + Send,
{
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .stack_size(CHECK_STACK_SIZE)
            .spawn_scoped(scope, work)
            .map_err(|error| format!("cannot spawn the check worker: {error}"))?;
        worker
            .join()
            .map_err(|_| "the check worker terminated unexpectedly".to_owned())
    })
}

fn user_index(file_ordinal: LibraryFileOrdinal, user_count: usize) -> Option<usize> {
    file_ordinal
        .index()
        .checked_sub(LIBRARY_COUNT)
        .filter(|index| *index < user_count)
}
