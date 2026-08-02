//! typokat — a from-scratch TypeScript type checker in Rust.
//!
//! Library crate for the CLI and conformance harness; module layout mirrors the
//! architecture layers.

pub use typokat_binder::binder;
pub use typokat_core::span;
pub use typokat_diagnostics::diagnostics;
pub use typokat_driver::driver;
pub use typokat_frontend::frontend;
pub use typokat_library as library;
pub use typokat_relate::relate;
pub use typokat_surface::surface;
pub use typokat_types::types;

#[cfg(test)]
pub(crate) use typokat_core::test_support;

#[cfg(test)]
mod build_reproducibility_tests {
    #[test]
    fn libtest_sources_do_not_capture_the_compile_time_repository_root() {
        let root = crate::test_support::repository_root();
        let forbidden = concat!("env!", "(\"CARGO_MANIFEST_DIR\")");
        let sources = crate::test_support::workspace_rust_sources();
        assert!(
            sources.iter().any(|path| path == &root.join("src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-core/src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-types/src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-binder/src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-relate/src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-diagnostics/src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-surface/src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-frontend/src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-check/src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-library/src/lib.rs"))
                && sources
                    .iter()
                    .any(|path| path == &root.join("crates/typokat-driver/src/lib.rs")),
            "reproducibility scan must cover root and workspace-member sources"
        );
        let offenders = sources
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

    #[test]
    fn source_layers_have_no_known_upward_edges() {
        let root = crate::test_support::repository_root();
        let assert_absent = |directory: &str, forbidden: &[&str]| {
            let sources = crate::test_support::rust_sources(&root.join(directory));
            assert!(
                !sources.is_empty(),
                "{directory}: layer tripwire must inspect at least one Rust source"
            );
            let offenders = sources
                .into_iter()
                .filter_map(|path| {
                    let source = std::fs::read_to_string(&path).expect("Rust source");
                    let hits = forbidden
                        .iter()
                        .filter(|needle| source.contains(**needle))
                        .copied()
                        .collect::<Vec<_>>();
                    (!hits.is_empty()).then(|| {
                        (
                            path.strip_prefix(&root)
                                .expect("source under repository root")
                                .display()
                                .to_string(),
                            hits,
                        )
                    })
                })
                .collect::<Vec<_>>();
            assert!(offenders.is_empty(), "{directory}: {offenders:#?}");
        };

        assert_absent(
            "crates/typokat-types/src/types",
            &[
                "typokat_binder",
                "typokat_relate",
                "typokat_diagnostics",
                "typokat_frontend",
                "typokat_check",
                "typokat_library",
                "typokat_driver",
            ],
        );
        assert_absent(
            "crates/typokat-relate/src/relate",
            &[
                "typokat_binder",
                "typokat_diagnostics",
                "typokat_frontend",
                "typokat_check",
                "typokat_library",
                "typokat_driver",
            ],
        );
        assert_absent(
            "crates/typokat-binder/src/binder",
            &[
                "typokat_relate",
                "typokat_diagnostics",
                "typokat_frontend",
                "typokat_check",
                "typokat_library",
                "typokat_driver",
            ],
        );
        assert_absent(
            "crates/typokat-diagnostics/src/diagnostics",
            &[
                "typokat_frontend",
                "typokat_check",
                "typokat_library",
                "typokat_driver",
            ],
        );
        assert_absent(
            "crates/typokat-surface/src",
            &[
                "typokat_core",
                "typokat_types",
                "typokat_binder",
                "typokat_relate",
                "typokat_diagnostics",
                "typokat_frontend",
                "typokat_check",
                "typokat_library",
                "typokat_driver",
            ],
        );
        assert_absent(
            "crates/typokat-frontend/src/frontend.rs",
            &[
                "typokat_diagnostics",
                "typokat_check",
                "typokat_library",
                "typokat_driver",
            ],
        );
        assert_absent(
            "crates/typokat-core/src",
            &[
                "typokat_types",
                "typokat_binder",
                "typokat_relate",
                "typokat_diagnostics",
                "typokat_surface",
                "typokat_frontend",
                "typokat_check",
                "typokat_library",
                "typokat_driver",
            ],
        );
        assert_absent(
            "crates/typokat-library/src",
            &["typokat_driver", "crate::driver"],
        );
        assert_absent(
            "crates/typokat-check/src/check",
            &[
                "typokat_library",
                "typokat_driver",
                "crate::library",
                "crate::driver",
            ],
        );
        assert_absent(
            "crates/typokat-driver/src",
            &["typokat::", "crate::main", "../src/"],
        );
    }
}
