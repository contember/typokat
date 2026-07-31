//! RED contract for WU7's production CLI dispatch/rendering infrastructure boundary.

use std::fmt;
use std::io::{self, Write};
use std::sync::Arc;
use typokat::driver::FileReport;
use typokat::frontend::FileInput;
use typokat::library::LibraryInitError;

type ProductionProjectResult = Result<Vec<FileReport>, Arc<LibraryInitError>>;
type ProductionProjectCheck = fn(Vec<FileInput>) -> ProductionProjectResult;

// The generic IO core must remain instantiated by this exact production callback shape.
const _: ProductionProjectCheck = typokat::driver::check_project;

#[derive(Debug)]
struct InjectedProviderError;

impl fmt::Display for InjectedProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("injected provider failure")
    }
}

#[derive(Default)]
struct FailingWriter {
    write_attempts: usize,
}

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        self.write_attempts += 1;
        Err(io::Error::other("injected write failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("injected flush failure"))
    }
}

fn unique_probe_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "typokat-wu7-{tag}-{}-{}.ts",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos()
    ))
}

fn check_args(path: &std::path::Path) -> Vec<String> {
    vec![
        "typokat".to_owned(),
        "check".to_owned(),
        "--format".to_owned(),
        "compact".to_owned(),
        path.to_string_lossy().into_owned(),
    ]
}

fn project_must_not_run(_inputs: Vec<FileInput>) -> Result<Vec<FileReport>, InjectedProviderError> {
    panic!("library-info must not dispatch a project check")
}

#[test]
fn actual_check_dispatch_maps_provider_failure_to_exit_two_without_partial_output() {
    let path = unique_probe_path("cli-fault");
    let source = "export const shouldNotRender: number = \"wrong\";\n";
    std::fs::write(&path, source).expect("write CLI fault probe");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut calls = 0;
    let exit = super::run_cli_core_with_io(
        &check_args(&path),
        &mut stdout,
        &mut stderr,
        |inputs: Vec<FileInput>| -> Result<Vec<FileReport>, InjectedProviderError> {
            calls += 1;
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].name, path.to_string_lossy());
            assert_eq!(inputs[0].source, source);
            Err(InjectedProviderError)
        },
    );
    let _ = std::fs::remove_file(path);

    assert_eq!(
        calls, 1,
        "the actual check dispatch must reach the driver boundary"
    );
    assert_eq!(exit, 2);
    assert!(stdout.is_empty());
    assert_eq!(
        String::from_utf8(stderr).expect("CLI stderr is UTF-8"),
        "error: failed to initialize embedded TypeScript 6.0.3 library: injected provider failure\n"
    );
}

#[test]
fn provider_failure_with_an_unwritable_stderr_returns_exit_two_without_panicking() {
    let path = unique_probe_path("stderr-fault");
    std::fs::write(&path, "export const value = 1;\n").expect("write stderr fault probe");

    let mut stdout = Vec::new();
    let mut stderr = FailingWriter::default();
    let exit = super::run_cli_core_with_io(
        &check_args(&path),
        &mut stdout,
        &mut stderr,
        |_inputs: Vec<FileInput>| -> Result<Vec<FileReport>, InjectedProviderError> {
            Err(InjectedProviderError)
        },
    );
    let _ = std::fs::remove_file(path);

    assert_eq!(exit, 2);
    assert!(stdout.is_empty());
    assert!(stderr.write_attempts > 0);
}

#[test]
fn library_info_with_an_unwritable_stdout_reports_exit_two_without_panicking() {
    let args = vec![
        "typokat".to_owned(),
        "library-info".to_owned(),
        "--format".to_owned(),
        "json".to_owned(),
    ];
    let mut stdout = FailingWriter::default();
    let mut stderr = Vec::new();
    let exit = super::run_cli_core_with_io(&args, &mut stdout, &mut stderr, project_must_not_run);

    assert_eq!(exit, 2);
    assert!(stdout.write_attempts > 0);
    assert_eq!(
        String::from_utf8(stderr).expect("CLI stderr is UTF-8"),
        "error: failed to write library info: injected write failure\n"
    );
}

#[test]
fn production_main_uses_the_same_io_core_as_the_unit_contract() {
    let main_source = include_str!("main.rs");
    assert!(
        main_source.contains("fn run_cli_core_with_io"),
        "the writer-injected core must be ordinary production code"
    );
    let main_start = main_source.find("fn main()").expect("production main");
    let main_end = main_start
        + main_source[main_start..]
            .find("\n}\n\n")
            .expect("end of production main")
        + 3;
    assert!(
        main_source[main_start..main_end].contains("run_cli_core_with_io"),
        "main must dispatch through the same writer-injected core"
    );
    assert!(
        !main_source.contains("match run(&args)"),
        "main must not retain a second dispatch/render path"
    );
}
