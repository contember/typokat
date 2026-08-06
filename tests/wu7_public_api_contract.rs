//! Compile-time WU7 contract for the intentional pre-1.0 driver API break.

use std::sync::Arc;
use typokat::driver::{
    check_files, check_project, check_project_once, check_source, production_cli_route,
    production_library_route, CheckOutput, DriverError, FileReport,
};
use typokat::frontend::FileInput;
use typokat::library::LibraryInitError;

type ProjectCheckResult = Result<Vec<FileReport>, DriverError>;
type ProjectCheck = fn(Vec<FileInput>) -> ProjectCheckResult;

const _: fn(&str) -> Result<CheckOutput, DriverError> = check_source;
const _: ProjectCheck = check_files;
const _: ProjectCheck = check_project;
const _: ProjectCheck = check_project_once;
const _: fn() -> &'static str = production_cli_route;
const _: fn() -> Result<&'static str, Arc<LibraryInitError>> = production_library_route;

#[test]
fn public_driver_signatures_are_result_bearing() {}
