//! typokat — a from-scratch TypeScript type checker in Rust.
//!
//! Library crate for the CLI and conformance harness; module layout mirrors the
//! architecture layers.

pub mod binder;
pub mod check;
mod class_semantics;
pub mod diagnostics;
pub mod driver;
pub mod library;
pub mod relate;
mod source;
pub mod span;
pub mod surface;
pub mod types;

#[cfg(test)]
pub(crate) fn test_repository_root() -> std::path::PathBuf {
    let root = std::env::current_dir().expect("test process current directory");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("repository Cargo.toml");
    assert!(
        manifest.starts_with("[package]\nname = \"typokat\"\n"),
        "tests must run from the typokat repository root"
    );
    assert!(
        root.join("Cargo.lock").is_file() && root.join("src/lib.rs").is_file(),
        "repository root sentinels must exist"
    );
    root
}

#[cfg(test)]
mod build_reproducibility_tests {
    use std::path::{Path, PathBuf};

    fn rust_sources(root: &Path) -> Vec<PathBuf> {
        let mut pending = vec![root.to_path_buf()];
        let mut sources = Vec::new();
        while let Some(directory) = pending.pop() {
            let mut entries = std::fs::read_dir(directory)
                .expect("source directory")
                .map(|entry| entry.expect("source entry").path())
                .collect::<Vec<_>>();
            entries.sort();
            for path in entries {
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                    sources.push(path);
                }
            }
        }
        sources.sort();
        sources
    }

    #[test]
    fn libtest_sources_do_not_capture_the_compile_time_repository_root() {
        let root = super::test_repository_root();
        let forbidden = concat!("env!", "(\"CARGO_MANIFEST_DIR\")");
        let offenders = rust_sources(&root.join("src"))
            .into_iter()
            .filter(|path| {
                std::fs::read_to_string(path)
                    .expect("Rust source")
                    .contains(forbidden)
            })
            .map(|path| {
                path.strip_prefix(&root)
                    .expect("source under repository root")
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert!(offenders.is_empty(), "{offenders:#?}");
    }
}
