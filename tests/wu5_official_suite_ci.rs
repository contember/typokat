//! WU5 acceptance: the official-suite CI cache must restore every fetched input.

const CORPUS_PATH: &str = "tooling/official-suite/corpus";
const REPOSITORY_PATH: &str = "tooling/official-suite/.tools/typescript.git";
const CACHE_KEY: &str =
    "ts-official-full-v2-${{ hashFiles('tooling/official-suite/tsofficial.py') }}";

fn official_suite_job(workflow: &str) -> Result<String, String> {
    let job_start = workflow
        .find("\n  official-suite:\n")
        .ok_or("CI must define the official-suite job")?;
    let mut lines = workflow[job_start + 1..].lines();
    if lines.next() != Some("  official-suite:") {
        return Err("official-suite job header must be exact".to_owned());
    }
    Ok(lines
        .take_while(|line| line.is_empty() || line.starts_with("    "))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn steps(job: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut step_start = None;
    let mut offset = 0;

    for line in job.split_inclusive('\n') {
        if line.starts_with("      - ") {
            if let Some(start) = step_start {
                result.push(&job[start..offset]);
            }
            step_start = Some(offset);
        }
        offset += line.len();
    }

    if let Some(start) = step_start {
        result.push(&job[start..]);
    }
    result
}

fn step_containing<'a>(job: &'a str, needle: &str) -> Result<&'a str, String> {
    steps(job)
        .into_iter()
        .find(|step| step.contains(needle))
        .ok_or_else(|| format!("official-suite job must contain a step matching {needle:?}"))
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn with_path_entries(step: &str) -> Result<Vec<&str>, String> {
    let lines = step.lines().collect::<Vec<_>>();
    let with_index = lines
        .iter()
        .position(|line| leading_spaces(line) == 8 && line.trim() == "with:")
        .ok_or("cache step must contain a step-level with block")?;
    let path_index = lines
        .iter()
        .enumerate()
        .skip(with_index + 1)
        .take_while(|(_, line)| line.trim().is_empty() || leading_spaces(line) > 8)
        .find(|(_, line)| leading_spaces(line) == 10 && line.trim() == "path: |")
        .map(|(index, _)| index)
        .ok_or("cache with block must contain a literal path block")?;

    let entries = lines
        .iter()
        .skip(path_index + 1)
        .take_while(|line| line.trim().is_empty() || leading_spaces(line) > 10)
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return Err("cache path block must contain entries".to_owned());
    }
    Ok(entries)
}

fn step_level_value<'a>(step: &'a str, key: &str) -> Option<&'a str> {
    step.lines().find_map(|line| {
        if leading_spaces(line) != 8 {
            return None;
        }
        line.trim().strip_prefix(key).map(str::trim)
    })
}

fn with_value<'a>(step: &'a str, key: &str) -> Option<&'a str> {
    let lines = step.lines().collect::<Vec<_>>();
    let with_index = lines
        .iter()
        .position(|line| leading_spaces(line) == 8 && line.trim() == "with:")?;
    lines
        .iter()
        .skip(with_index + 1)
        .take_while(|line| line.trim().is_empty() || leading_spaces(line) > 8)
        .find_map(|line| {
            if leading_spaces(line) != 10 {
                return None;
            }
            line.trim().strip_prefix(key).map(str::trim)
        })
}

fn validate_official_suite_job(workflow: &str) -> Result<(), String> {
    let job = official_suite_job(workflow)?;
    let cache = step_containing(&job, "uses: actions/cache@v4")?;
    let paths = with_path_entries(cache)?;
    if !paths.contains(&CORPUS_PATH) {
        return Err(format!(
            "cache with.path must contain exact entry {CORPUS_PATH}"
        ));
    }
    if !paths.contains(&REPOSITORY_PATH) {
        return Err(format!(
            "cache with.path must contain exact entry {REPOSITORY_PATH}"
        ));
    }
    if with_value(cache, "key:") != Some(CACHE_KEY) {
        return Err(format!(
            "cache key must be the versioned full-fetch key {CACHE_KEY}"
        ));
    }

    let fetch = step_containing(&job, "python3 tsofficial.py fetch")?;
    if step_level_value(fetch, "if:").is_some() {
        return Err("the pinned fetch step must not have any step-level if key".to_owned());
    }
    if step_level_value(fetch, "run:") != Some("python3 tsofficial.py fetch") {
        return Err("the pinned fetch step must use exact run command".to_owned());
    }
    if step_level_value(fetch, "working-directory:") != Some("tooling/official-suite") {
        return Err("the pinned fetch step must run from the official-suite directory".to_owned());
    }

    let ratchet = step_containing(&job, "python3 tsofficial.py run --check")?;
    if step_level_value(ratchet, "run:") != Some("python3 tsofficial.py run --check") {
        return Err("the identity ratchet step must use exact run --check command".to_owned());
    }
    if step_level_value(ratchet, "working-directory:") != Some("tooling/official-suite") {
        return Err("run --check must execute from the official-suite directory".to_owned());
    }
    Ok(())
}

fn checked_in_workflow() -> String {
    std::fs::read_to_string(".github/workflows/ci.yml").expect("read the checked-in CI workflow")
}

fn replace_once(source: &str, from: &str, to: &str) -> String {
    assert_eq!(
        source.matches(from).count(),
        1,
        "negative control mutation must have exactly one target"
    );
    source.replacen(from, to, 1)
}

#[test]
fn official_suite_ci_restores_and_validates_the_full_pinned_fetch() {
    validate_official_suite_job(&checked_in_workflow())
        .expect("official-suite CI must preserve its complete fetched state and identity ratchet");
}

#[test]
fn negative_control_rejects_any_conditional_fetch() {
    let workflow = checked_in_workflow();
    let broken = replace_once(
        &workflow,
        "      - name: Fetch corpus (pinned TS SHA)\n",
        "      - name: Fetch corpus (pinned TS SHA)\n        if: ${{ false }}\n",
    );
    let error = validate_official_suite_job(&broken)
        .expect_err("a differently-spelled step condition must make the gate fire");
    assert!(
        error.contains("must not have any step-level if key"),
        "negative control must fail for the intended reason: {error}"
    );
}

#[test]
fn negative_control_rejects_repository_path_outside_with_path() {
    let workflow = checked_in_workflow();
    let broken = replace_once(
        &workflow,
        "            tooling/official-suite/.tools/typescript.git\n",
        "          # tooling/official-suite/.tools/typescript.git\n",
    );
    let error = validate_official_suite_job(&broken)
        .expect_err("mentioning the repository outside with.path must make the gate fire");
    assert!(
        error.contains("cache with.path must contain exact entry"),
        "negative control must fail for the intended reason: {error}"
    );
}
