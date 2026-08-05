//! WU5 acceptance: the official-suite CI cache must restore every fetched input.

fn official_suite_job(workflow: &str) -> String {
    let job_start = workflow
        .find("\n  official-suite:\n")
        .expect("CI must define the official-suite job");
    let mut lines = workflow[job_start + 1..].lines();
    assert_eq!(lines.next(), Some("  official-suite:"));
    lines
        .take_while(|line| line.is_empty() || line.starts_with("    "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn step_containing<'a>(job: &'a str, needle: &str) -> &'a str {
    let mut step_start = None;
    let mut offset = 0;

    for line in job.split_inclusive('\n') {
        if line.starts_with("      - ") {
            if let Some(start) = step_start {
                let step = &job[start..offset];
                if step.contains(needle) {
                    return step;
                }
            }
            step_start = Some(offset);
        }
        offset += line.len();
    }

    if let Some(start) = step_start {
        let step = &job[start..];
        if step.contains(needle) {
            return step;
        }
    }

    panic!("official-suite job must contain a step matching {needle:?}");
}

fn checked_in_official_suite_job() -> String {
    let workflow = std::fs::read_to_string(".github/workflows/ci.yml")
        .expect("read the checked-in CI workflow");
    official_suite_job(&workflow)
}

#[test]
fn official_suite_cache_restores_corpus_and_pinned_repository() {
    let job = checked_in_official_suite_job();
    let cache = step_containing(&job, "uses: actions/cache@v4");

    assert!(
        cache
            .lines()
            .any(|line| line.trim() == "tooling/official-suite/corpus"),
        "the official-suite cache must restore the fetched corpus"
    );
    assert!(
        cache
            .lines()
            .any(|line| line.trim() == "tooling/official-suite/.tools/typescript.git"),
        "the official-suite cache must also restore the pinned full-blob TypeScript repository"
    );
}

#[test]
fn official_suite_cache_key_rejects_the_incomplete_cache_generation() {
    let job = checked_in_official_suite_job();
    let cache = step_containing(&job, "uses: actions/cache@v4");
    let key = cache
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("key:"))
        .expect("official-suite cache must define a key");

    assert!(
        key.starts_with("key: ts-official-full-v2-"),
        "the cache key must use the v2 full-fetch generation, not the incomplete corpus-only entry: {key}"
    );
}

#[test]
fn official_suite_always_validates_the_pinned_fetch_state() {
    let job = checked_in_official_suite_job();
    let fetch = step_containing(&job, "python3 tsofficial.py fetch");

    assert!(
        fetch.lines().all(|line| line.trim() != "if: steps.corpus-cache.outputs.cache-hit != 'true'"),
        "the pinned fetch command must run on cache hits so it validates restored repository state"
    );
    assert!(
        fetch
            .lines()
            .any(|line| line.trim() == "working-directory: tooling/official-suite"),
        "the pinned fetch command must run from the official-suite directory"
    );
}

#[test]
fn official_suite_keeps_the_committed_identity_ratchet() {
    let job = checked_in_official_suite_job();
    let ratchet = step_containing(&job, "python3 tsofficial.py run --check");

    assert!(
        ratchet
            .lines()
            .any(|line| line.trim() == "working-directory: tooling/official-suite"),
        "run --check must execute against the official-suite corpus"
    );
}
