//! RED process-boundary contract for WU7's isolated official-suite worker.

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_typokat");
const BATCH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BATCH_FRAME_BYTES: usize = 2 * 1024 * 1024;
const PROFILE_SHA256: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";

#[test]
fn official_batch_cases_share_the_worker_process_but_not_user_state() {
    let requests = [
        serde_json::json!({
            "schema": 1,
            "case_id": "defines-global",
            "name": "defines-global.ts",
            "source": "export {}; declare global { var wu7BatchLeak: number; }\n"
        }),
        serde_json::json!({
            "schema": 1,
            "case_id": "must-not-see-global",
            "name": "must-not-see-global.ts",
            "source": "export {}; const value: number = wu7BatchLeak;\n"
        }),
    ];
    let input = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let (worker_pid, output) = run_official_batch(&input);

    assert_eq!(
        output.status.code(),
        Some(0),
        "batch stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "{output:#?}");

    let responses = String::from_utf8(output.stdout)
        .expect("batch output is UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("batch response is JSONL"))
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), requests.len());
    for response in &responses {
        let keys = response
            .as_object()
            .expect("batch response is an object")
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            keys,
            BTreeSet::from([
                "case_id",
                "exit_code",
                "profile_sha256",
                "provider_route",
                "schema",
                "stderr",
                "stdout",
                "worker_pid",
            ])
        );
        assert_eq!(response["schema"], 1);
        assert_eq!(response["worker_pid"].as_u64(), Some(u64::from(worker_pid)));
        assert_eq!(response["provider_route"], "production-default-library");
        assert_eq!(response["profile_sha256"], PROFILE_SHA256);
    }
    assert_eq!(responses[0]["case_id"], "defines-global");
    assert_eq!(responses[0]["exit_code"], 0);
    assert_eq!(responses[1]["case_id"], "must-not-see-global");
    assert_eq!(responses[1]["exit_code"], 1);
    assert!(
        responses[1]["stderr"]
            .as_str()
            .is_some_and(|stderr| stderr.contains("TK2304")),
        "the second isolated case must not inherit the first case's global: {:?}",
        responses[1]
    );
}

#[test]
fn official_batch_rejects_malformed_requests_without_partial_case_output() {
    let malformed = [
        "not json".to_owned(),
        serde_json::json!({
            "schema": 2,
            "case_id": "wrong-schema",
            "name": "wrong-schema.ts",
            "source": "const value = 1;\n"
        })
        .to_string(),
        serde_json::json!({
            "schema": true,
            "case_id": "bool-schema",
            "name": "bool-schema.ts",
            "source": "const value = 1;\n"
        })
        .to_string(),
        serde_json::json!({
            "schema": 1,
            "case_id": "missing-source",
            "name": "missing-source.ts"
        })
        .to_string(),
        serde_json::json!({
            "schema": 1,
            "case_id": "empty-name",
            "name": "",
            "source": "const value = 1;\n"
        })
        .to_string(),
        serde_json::json!({
            "schema": 1,
            "case_id": "non-string-name",
            "name": 7,
            "source": "const value = 1;\n"
        })
        .to_string(),
        serde_json::json!({
            "schema": 1,
            "case_id": "non-string-source",
            "name": "non-string-source.ts",
            "source": false
        })
        .to_string(),
        serde_json::json!({
            "schema": 1,
            "case_id": 7,
            "name": "wrong-type.ts",
            "source": "const value = 1;\n"
        })
        .to_string(),
        serde_json::json!({
            "schema": 1,
            "case_id": "",
            "name": "empty-id.ts",
            "source": "const value = 1;\n"
        })
        .to_string(),
        serde_json::json!({
            "schema": 1,
            "case_id": "extra-key",
            "name": "extra-key.ts",
            "source": "const value = 1;\n",
            "unexpected": true
        })
        .to_string(),
        serde_json::json!({
            "schema": 1,
            "case_id": "oversized-frame",
            "name": "oversized-frame.ts",
            "source": "x".repeat(MAX_BATCH_FRAME_BYTES)
        })
        .to_string(),
    ];

    for request in malformed {
        let (_, output) = run_official_batch(&(request + "\n"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{output:#?}");
        assert!(output.stdout.is_empty(), "{output:#?}");
        assert!(
            stderr.contains("error: malformed official-batch request:"),
            "{output:#?}"
        );
        assert!(
            !stderr.contains("TK"),
            "no partial case output: {output:#?}"
        );
    }

    let prefix = r#"{"schema":1,"case_id":"boundary","name":"boundary.ts","source":"//"#;
    let suffix = "\"}\n";
    let padding = MAX_BATCH_FRAME_BYTES - prefix.len() - suffix.len();
    let exact = format!("{prefix}{}{suffix}", "x".repeat(padding));
    assert_eq!(exact.len(), MAX_BATCH_FRAME_BYTES);
    let (_, exact_output) = run_official_batch(&exact);
    assert_eq!(
        exact_output.status.code(),
        Some(0),
        "exact-cap frame must be accepted: {exact_output:#?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&exact_output.stdout)
            .lines()
            .count(),
        1
    );

    let oversized = format!("{prefix}{}{suffix}", "x".repeat(padding + 1));
    assert_eq!(oversized.len(), MAX_BATCH_FRAME_BYTES + 1);
    let (_, oversized_output) = run_official_batch(&oversized);
    assert_eq!(oversized_output.status.code(), Some(2));
    assert!(oversized_output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&oversized_output.stderr)
        .contains("error: malformed official-batch request:"));
}

#[test]
fn production_batch_worker_cannot_delegate_cases_to_child_processes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    for source in production_rust_sources(&root) {
        let text = fs::read_to_string(&source).expect("read production Rust source");
        if ["std::process::Command", "process::Command", "Command::new("]
            .iter()
            .any(|needle| text.contains(needle))
        {
            offenders.push(
                source
                    .strip_prefix(&root)
                    .expect("source under repository root")
                    .display()
                    .to_string(),
            );
        }
    }
    assert!(
        offenders.is_empty(),
        "the same-process worker must call the driver directly, not proxy cases: {offenders:?}"
    );
}

fn run_official_batch(input: &str) -> (u32, Output) {
    let mut child = Command::new(BIN)
        .arg("official-batch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn official batch worker");
    let worker_pid = child.id();
    let mut stdin = child.stdin.take().expect("batch stdin");
    // An oversized-frame rejection may close the reader before the writer drains.
    let _ = stdin.write_all(input.as_bytes());
    drop(stdin);
    (worker_pid, wait_with_timeout(child))
}

fn wait_with_timeout(mut child: Child) -> Output {
    let deadline = Instant::now() + BATCH_TIMEOUT;
    loop {
        if child.try_wait().expect("poll batch worker").is_some() {
            return child.wait_with_output().expect("collect batch worker");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .expect("collect timed-out batch worker");
            panic!(
                "official-batch worker exceeded {BATCH_TIMEOUT:?}: stderr={}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn production_rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.join("src"), root.join("crates")];
    let mut sources = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("read production source directory") {
            let path = entry.expect("source directory entry").path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) != Some("tests") {
                    pending.push(path);
                }
                continue;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if path.extension().and_then(|extension| extension.to_str()) == Some("rs")
                && !name.ends_with("_spec.rs")
                && name != "tests.rs"
                && name != "test_support.rs"
            {
                sources.push(path);
            }
        }
    }
    sources.sort();
    sources
}
