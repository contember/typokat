//! typokat — a from-scratch TypeScript type checker in Rust.
//!
//! Library crate for the CLI and conformance harness; module layout mirrors the
//! architecture layers.

pub mod binder;
pub mod check;
mod class_semantics;
pub mod diagnostics;
pub mod driver;
pub mod frontend;
pub mod library;
pub mod relate;
mod source;
pub mod span;
pub mod surface;
pub mod types;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod build_reproducibility_tests {
    #[test]
    fn libtest_sources_do_not_capture_the_compile_time_repository_root() {
        let root = crate::test_support::repository_root();
        let forbidden = concat!("env!", "(\"CARGO_MANIFEST_DIR\")");
        let offenders = crate::test_support::rust_sources(&root.join("src"))
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
            let offenders = crate::test_support::rust_sources(&root.join(directory))
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
            "src/types",
            &["crate::diagnostics", "crate::relate", "crate::check"],
        );
        assert_absent("src/relate", &["crate::diagnostics", "crate::check"]);
        assert_absent("src/binder", &["crate::check", "../check/"]);
        assert_absent(
            "src/frontend.rs",
            &["crate::check", "crate::library", "crate::driver"],
        );
        assert_absent("src/library", &["crate::driver"]);
        assert_absent("src/check", &["crate::library", "crate::driver"]);
    }
}
