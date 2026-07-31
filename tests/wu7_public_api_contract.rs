//! Compile-time WU7 contract for the intentional pre-1.0 driver API break.

use std::sync::Arc;
use typokat::driver::{
    check_files, check_project, check_source, production_library_route, CheckOutput, FileReport,
};
use typokat::frontend::FileInput;
use typokat::library::LibraryInitError;

const _: fn(&str) -> Result<CheckOutput, Arc<LibraryInitError>> = check_source;
const _: fn(Vec<FileInput>) -> Result<Vec<FileReport>, Arc<LibraryInitError>> = check_files;
const _: fn(Vec<FileInput>) -> Result<Vec<FileReport>, Arc<LibraryInitError>> = check_project;
const _: fn() -> Result<&'static str, Arc<LibraryInitError>> = production_library_route;

#[test]
fn public_driver_signatures_are_result_bearing() {}
