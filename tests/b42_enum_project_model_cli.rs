//! Public Bundler acceptance for enum project publication.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn from_fixture(case: &str) -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("typokat-b42-{case}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create B42 temporary project");
        let source = corpus_root().join(case);
        for file in ["source.ts", "consumer.ts"] {
            std::fs::copy(source.join(file), root.join(file)).expect("copy B42 source");
        }
        Self { root }
    }

    fn install_config(&self, case: &str, order: &str) {
        std::fs::copy(
            corpus_root()
                .join(case)
                .join(format!("tsconfig.{order}.json")),
            self.root.join("tsconfig.json"),
        )
        .expect("install B42 config");
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases/b42_enum_project_model")
}

fn run(project: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typokat"))
        .args(["check", "--format", "compact", "--project-summary", "json"])
        .arg(project)
        .output()
        .expect("run typokat B42 project")
}

#[test]
fn enum_project_routes_are_cleanly_published_and_root_order_identical() {
    for (case, expected_code) in [
        ("default_identifier", "TK2322"),
        ("named_import", "TK2322"),
        ("namespace_payload", "TK2322"),
        ("type_only", "TK2304"),
    ] {
        let project = TempProject::from_fixture(case);
        let mut outputs = Vec::new();
        for order in ["normal", "reverse"] {
            project.install_config(case, order);
            let output = run(&project.root);
            assert_eq!(output.status.code(), Some(1), "{case}/{order}");
            let summary: Value = serde_json::from_slice(&output.stdout)
                .unwrap_or_else(|error| panic!("{case}/{order}: {error}"));
            assert_eq!(
                summary["roots"],
                serde_json::json!(["consumer.ts", "source.ts"])
            );
            assert_eq!(summary["project_notices"], serde_json::json!([]));
            assert_eq!(summary["parse_errors"], serde_json::json!([]));
            assert_eq!(summary["incomplete"], serde_json::json!([]));
            let diagnostics = summary["diagnostics"].as_array().expect("diagnostics");
            assert_eq!(diagnostics.len(), 1, "{case}/{order}");
            assert!(
                diagnostics[0]
                    .as_str()
                    .expect("diagnostic line")
                    .ends_with(expected_code),
                "{case}/{order}: {diagnostics:?}"
            );
            outputs.push((output.stdout, output.stderr));
        }
        assert_eq!(outputs[0], outputs[1], "{case} root-order bytes");
    }
}
