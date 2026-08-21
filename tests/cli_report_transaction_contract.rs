//! RED acceptance contract for the CLI report transaction owner.

const MAIN_SOURCE: &str = include_str!("../src/main.rs");

const PROJECT_NOTICES: &str = "report.output.project_notices";
const PARSE_ERRORS: &str = "report.output.parse_errors";
const DIAGNOSTICS_RENDERER: &str = "diagnostics::render_to_writer_with_format(";
const INCOMPLETE_RENDERER: &str = "diagnostics::render_incomplete_to_writer_with_format(";

fn production_source(source: &str) -> &str {
    source
        .split_once("\n#[cfg(test)]\nmod wu7_cli_fault_spec;")
        .map_or(source, |(production, _tests)| production)
}

fn count(source: &str, needle: &str) -> usize {
    source.match_indices(needle).count()
}

fn matching_brace(source: &str, opening: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in source.as_bytes()[opening..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(opening + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn contains_in_order(source: &str, needles: &[&str]) -> bool {
    let mut remainder = source;
    for needle in needles {
        let Some(position) = remainder.find(needle) else {
            return false;
        };
        remainder = &remainder[position + needle.len()..];
    }
    true
}

fn ordered_report_loop_count(source: &str) -> usize {
    source
        .match_indices("for report in ")
        .filter_map(|(loop_start, _)| {
            let opening = source[loop_start..].find('{')? + loop_start;
            let closing = matching_brace(source, opening)?;
            Some(&source[opening + 1..closing])
        })
        .filter(|body| {
            contains_in_order(
                body,
                &[
                    PROJECT_NOTICES,
                    PARSE_ERRORS,
                    DIAGNOSTICS_RENDERER,
                    INCOMPLETE_RENDERER,
                ],
            )
        })
        .count()
}

fn compact_whitespace(source: &str) -> String {
    source.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn status_precedence_count(source: &str) -> usize {
    const PRECEDENCE: &str = "if had_incomplete { CheckStatus::Incomplete } else if had_errors { CheckStatus::Diagnostics } else { CheckStatus::Clean }";
    count(&compact_whitespace(source), PRECEDENCE)
}

fn contract_failures(source: &str) -> Vec<String> {
    let source = production_source(source);
    let mut failures = Vec::new();

    let ordered_loops = ordered_report_loop_count(source);
    if ordered_loops != 1 {
        failures.push(format!(
            "expected exactly one project notices -> parse errors -> diagnostics -> incomplete report loop, found {ordered_loops}"
        ));
    }

    for (channel, needle) in [
        ("project notices", PROJECT_NOTICES),
        ("parse errors", PARSE_ERRORS),
        ("diagnostics renderer", DIAGNOSTICS_RENDERER),
        ("incomplete renderer", INCOMPLETE_RENDERER),
    ] {
        let actual = count(source, needle);
        if actual != 1 {
            failures.push(format!(
                "expected exactly one {channel} channel in production, found {actual}"
            ));
        }
    }

    for flag in ["had_errors", "had_incomplete"] {
        let actual = count(source, flag);
        if actual != 3 {
            failures.push(format!(
                "expected exactly three occurrences of {flag} in production, found {actual}"
            ));
        }
    }

    let precedence_decisions = status_precedence_count(source);
    if precedence_decisions != 1 {
        failures.push(format!(
            "expected exactly one incomplete > errors > clean status decision, found {precedence_decisions}"
        ));
    }

    failures
}

fn owned_transaction_fixture() -> String {
    r#"
fn render_report_transaction(reports: &[FileReport]) -> CheckStatus {
    let mut had_errors = false;
    let mut had_incomplete = false;
    for report in reports {
        for notice in &report.output.project_notices {
            render_notice(notice);
        }
        for parse_error in &report.output.parse_errors {
            render_parse_error(parse_error);
        }
        diagnostics::render_to_writer_with_format();
        diagnostics::render_incomplete_to_writer_with_format();
        had_errors |= report.output.has_errors();
        had_incomplete |= report.output.is_incomplete();
    }
    if had_incomplete {
        CheckStatus::Incomplete
    } else if had_errors {
        CheckStatus::Diagnostics
    } else {
        CheckStatus::Clean
    }
}
"#
    .to_owned()
}

#[test]
fn cli_report_transaction_has_one_owner() {
    let failures = contract_failures(MAIN_SOURCE);
    assert!(
        failures.is_empty(),
        "CLI report transaction contract failed:\n{}",
        failures.join("\n")
    );
}

#[test]
fn contract_accepts_complete_owned_transaction_fixture() {
    assert!(contract_failures(&owned_transaction_fixture()).is_empty());
}

#[test]
fn contract_rejects_each_omitted_report_channel() {
    for channel in [
        PROJECT_NOTICES,
        PARSE_ERRORS,
        DIAGNOSTICS_RENDERER,
        INCOMPLETE_RENDERER,
    ] {
        let mutated = owned_transaction_fixture().replacen(channel, "omitted_channel", 1);
        assert!(
            !contract_failures(&mutated).is_empty(),
            "contract accepted a transaction without {channel}"
        );
    }
}

#[test]
fn contract_rejects_errors_before_incomplete_status() {
    let reversed = owned_transaction_fixture().replacen(
        "if had_incomplete {\n        CheckStatus::Incomplete\n    } else if had_errors {\n        CheckStatus::Diagnostics",
        "if had_errors {\n        CheckStatus::Diagnostics\n    } else if had_incomplete {\n        CheckStatus::Incomplete",
        1,
    );
    assert!(
        !contract_failures(&reversed).is_empty(),
        "contract accepted errors > incomplete > clean precedence"
    );
}
