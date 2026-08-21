//! RED acceptance contract for the CLI report transaction owner.

const MAIN_SOURCE: &str = include_str!("../src/main.rs");

const TRANSACTION: &str = "render_report_transaction";
const TRANSACTION_CALL: &str = "render_report_transaction(";
const PROJECT_NOTICES: &str = ".output.project_notices";
const PARSE_ERRORS: &str = ".output.parse_errors";
const DIAGNOSTICS_RENDERER: &str = "diagnostics::render_to_writer_with_format(";
const INCOMPLETE_RENDERER: &str = "diagnostics::render_incomplete_to_writer_with_format(";
const WRAPPERS: [&str; 3] = [
    "render_official_batch_reports",
    "render_project_reports",
    "check_paths_with_io",
];

fn production_source(source: &str) -> &str {
    source
        .split_once("\n#[cfg(test)]\nmod wu7_cli_fault_spec;")
        .map_or(source, |(production, _tests)| production)
}

fn count(source: &str, needle: &str) -> usize {
    source.match_indices(needle).count()
}

fn mask_range(cleaned: &mut [u8], original: &[u8], start: usize, end: usize) {
    for index in start..end {
        if original[index] != b'\n' && original[index] != b'\r' {
            cleaned[index] = b' ';
        }
    }
}

fn quoted_string_end(source: &[u8], quote: usize) -> usize {
    let mut cursor = quote + 1;
    while cursor < source.len() {
        match source[cursor] {
            b'\\' => cursor = (cursor + 2).min(source.len()),
            b'"' => return cursor + 1,
            _ => cursor += 1,
        }
    }
    source.len()
}

fn char_literal_end(source: &str, quote: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = quote + 1;
    if cursor >= bytes.len() {
        return None;
    }
    if bytes[cursor] == b'\\' {
        cursor += 1;
        match bytes.get(cursor).copied()? {
            b'x' => cursor = cursor.checked_add(3)?,
            b'u' if bytes.get(cursor + 1) == Some(&b'{') => {
                cursor += 2;
                while bytes.get(cursor) != Some(&b'}') {
                    cursor += 1;
                    if cursor >= bytes.len() {
                        return None;
                    }
                }
                cursor += 1;
            }
            _ => cursor += 1,
        }
    } else {
        cursor += source[cursor..].chars().next()?.len_utf8();
    }
    (bytes.get(cursor) == Some(&b'\'')).then_some(cursor + 1)
}

fn raw_string_end(source: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    if matches!(source.get(cursor), Some(b'b' | b'c')) {
        cursor += 1;
    }
    if source.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let hashes_start = cursor;
    while source.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    if source.get(cursor) != Some(&b'"') {
        return None;
    }
    let hash_count = cursor - hashes_start;
    cursor += 1;
    while cursor < source.len() {
        if source[cursor] == b'"'
            && source.get(cursor + 1..cursor + 1 + hash_count)
                == Some(&source[hashes_start..hashes_start + hash_count])
        {
            return Some(cursor + 1 + hash_count);
        }
        cursor += 1;
    }
    Some(source.len())
}

fn code_only(source: &str) -> String {
    let original = source.as_bytes();
    let mut cleaned = original.to_vec();
    let mut cursor = 0usize;
    while cursor < original.len() {
        if original.get(cursor..cursor + 2) == Some(b"//") {
            let end = original[cursor + 2..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(original.len(), |offset| cursor + 2 + offset);
            mask_range(&mut cleaned, original, cursor, end);
            cursor = end;
            continue;
        }
        if original.get(cursor..cursor + 2) == Some(b"/*") {
            let mut end = cursor + 2;
            let mut depth = 1usize;
            while end < original.len() && depth > 0 {
                if original.get(end..end + 2) == Some(b"/*") {
                    depth += 1;
                    end += 2;
                } else if original.get(end..end + 2) == Some(b"*/") {
                    depth -= 1;
                    end += 2;
                } else {
                    end += 1;
                }
            }
            mask_range(&mut cleaned, original, cursor, end);
            cursor = end;
            continue;
        }
        if let Some(end) = raw_string_end(original, cursor) {
            mask_range(&mut cleaned, original, cursor, end);
            cursor = end;
            continue;
        }
        let quote = if original[cursor] == b'"' {
            Some(cursor)
        } else if matches!(original[cursor], b'b' | b'c') && original.get(cursor + 1) == Some(&b'"')
        {
            Some(cursor + 1)
        } else {
            None
        };
        if let Some(quote) = quote {
            let end = quoted_string_end(original, quote);
            mask_range(&mut cleaned, original, cursor, end);
            cursor = end;
            continue;
        }
        let quote = if original[cursor] == b'\'' {
            Some(cursor)
        } else if original[cursor] == b'b' && original.get(cursor + 1) == Some(&b'\'') {
            Some(cursor + 1)
        } else {
            None
        };
        if let Some(quote) = quote {
            if let Some(end) = char_literal_end(source, quote) {
                mask_range(&mut cleaned, original, cursor, end);
                cursor = end;
                continue;
            }
        }
        cursor += 1;
    }
    String::from_utf8(cleaned).expect("masking valid Rust source preserves UTF-8")
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

fn function_spans(source: &str, name: &str) -> Vec<(usize, usize)> {
    let needle = format!("fn {name}");
    source
        .match_indices(&needle)
        .filter_map(|(start, _)| {
            let after_name = start + needle.len();
            matches!(source.as_bytes().get(after_name), Some(b'(' | b'<')).then_some(())?;
            let opening = source[after_name..].find('{')? + after_name;
            let closing = matching_brace(source, opening)?;
            Some((start, closing + 1))
        })
        .collect()
}

fn body(source: &str, span: (usize, usize)) -> &str {
    let opening = source[span.0..span.1]
        .find('{')
        .expect("function span has an opening brace")
        + span.0;
    &source[opening + 1..span.1 - 1]
}

fn source_without_span(source: &str, span: (usize, usize)) -> String {
    let mut outside = source.as_bytes().to_vec();
    mask_range(&mut outside, source.as_bytes(), span.0, span.1);
    String::from_utf8(outside).expect("masking valid Rust source preserves UTF-8")
}

fn has_public_visibility(source: &str, definition_start: usize) -> bool {
    let item_start = ['}', ';', ']']
        .iter()
        .filter_map(|delimiter| source[..definition_start].rfind(*delimiter))
        .max()
        .map_or(0, |boundary| boundary + 1);
    source[item_start..definition_start]
        .split_whitespace()
        .any(|token| token == "pub" || token.starts_with("pub("))
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
        .match_indices("for ")
        .filter_map(|(loop_start, _)| {
            let opening = source[loop_start..].find('{')? + loop_start;
            let closing = matching_brace(source, opening)?;
            Some(&source[opening + 1..closing])
        })
        .filter(|loop_body| {
            contains_in_order(
                loop_body,
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
    let source = code_only(production_source(source));
    let mut failures = Vec::new();

    let transaction_spans = function_spans(&source, TRANSACTION);
    if transaction_spans.len() != 1 {
        failures.push(format!(
            "expected exactly one {TRANSACTION} definition, found {}",
            transaction_spans.len()
        ));
        return failures;
    }
    let transaction_span = transaction_spans[0];
    if has_public_visibility(&source, transaction_span.0) {
        failures.push(format!("expected {TRANSACTION} to remain private"));
    }
    let transaction_body = body(&source, transaction_span);
    let outside_transaction = source_without_span(&source, transaction_span);

    let ordered_loops = ordered_report_loop_count(transaction_body);
    if ordered_loops != 1 {
        failures.push(format!(
            "expected exactly one project notices -> parse errors -> diagnostics -> incomplete report loop in {TRANSACTION}, found {ordered_loops}"
        ));
    }

    for (channel, needle) in [
        ("project notices", PROJECT_NOTICES),
        ("parse errors", PARSE_ERRORS),
        ("diagnostics renderer", DIAGNOSTICS_RENDERER),
        ("incomplete renderer", INCOMPLETE_RENDERER),
    ] {
        let inside = count(transaction_body, needle);
        let outside = count(&outside_transaction, needle);
        if inside != 1 || outside != 0 {
            failures.push(format!(
                "expected {channel} authority once inside {TRANSACTION} and never outside, found {inside} inside and {outside} outside"
            ));
        }
    }

    for flag in ["had_errors", "had_incomplete"] {
        let inside = count(transaction_body, flag);
        let outside = count(&outside_transaction, flag);
        if inside != 3 || outside != 0 {
            failures.push(format!(
                "expected three {flag} occurrences inside {TRANSACTION} and none outside, found {inside} inside and {outside} outside"
            ));
        }
    }

    let inside_precedence = status_precedence_count(transaction_body);
    let outside_precedence = status_precedence_count(&outside_transaction);
    if inside_precedence != 1 || outside_precedence != 0 {
        failures.push(format!(
            "expected one incomplete > errors > clean decision inside {TRANSACTION} and none outside, found {inside_precedence} inside and {outside_precedence} outside"
        ));
    }

    let expected_total_calls = WRAPPERS.len() + 1;
    let total_calls = count(&source, TRANSACTION_CALL);
    if total_calls != expected_total_calls {
        failures.push(format!(
            "expected one {TRANSACTION} definition plus one call from each wrapper, found {total_calls} total occurrences"
        ));
    }
    for wrapper in WRAPPERS {
        let spans = function_spans(&source, wrapper);
        if spans.len() != 1 {
            failures.push(format!(
                "expected exactly one {wrapper} definition, found {}",
                spans.len()
            ));
            continue;
        }
        let calls = count(body(&source, spans[0]), TRANSACTION_CALL);
        if calls != 1 {
            failures.push(format!(
                "expected {wrapper} to call {TRANSACTION} exactly once, found {calls} calls"
            ));
        }
    }

    failures
}

fn owned_transaction_fixture() -> String {
    r#"
fn render_official_batch_reports(reports: &[FileReport]) -> CheckStatus {
    render_report_transaction(reports)
}

fn render_project_reports(reports: &[FileReport]) -> CheckStatus {
    render_report_transaction(reports)
}

fn check_paths_with_io(reports: &[FileReport]) -> CheckStatus {
    render_report_transaction(reports)
}

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
fn cli_report_transaction_has_one_private_owner_and_three_callers() {
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
fn contract_ignores_channel_and_status_decoys_in_non_code_text() {
    let mut fixture = owned_transaction_fixture();
    fixture.push_str(
        r####"
// for report in reports { report.output.project_notices; had_errors; }
/* report.output.parse_errors diagnostics::render_to_writer_with_format(
   diagnostics::render_incomplete_to_writer_with_format( had_incomplete */
const DECOY: &str = r#"fn render_report_transaction(reports: &[FileReport]) {
    if had_incomplete { CheckStatus::Incomplete } else if had_errors {
        CheckStatus::Diagnostics
    } else { CheckStatus::Clean }
}"#;
const BRACES: char = '{';
"####,
    );
    assert!(contract_failures(&fixture).is_empty());
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

#[test]
fn contract_rejects_disconnected_transaction_owner() {
    let disconnected = owned_transaction_fixture().replace(
        "render_report_transaction(reports)",
        "disconnected_report_transaction(reports)",
    );
    assert!(
        !contract_failures(&disconnected).is_empty(),
        "contract accepted an owner that no route calls"
    );
}

#[test]
fn contract_rejects_public_transaction_owner() {
    let public = owned_transaction_fixture().replacen(
        "fn render_report_transaction",
        "pub(in crate) fn render_report_transaction",
        1,
    );
    assert!(
        !contract_failures(&public).is_empty(),
        "contract accepted a public report transaction"
    );
}

#[test]
fn contract_rejects_renamed_duplicate_report_loop() {
    let mut duplicated = owned_transaction_fixture();
    duplicated.push_str(
        r#"
fn duplicate_route(reports: &[FileReport]) -> CheckStatus {
    let mut had_errors = false;
    let mut had_incomplete = false;
    for file_report in reports {
        for notice in &file_report.output.project_notices { render_notice(notice); }
        for parse_error in &file_report.output.parse_errors { render_parse_error(parse_error); }
        diagnostics::render_to_writer_with_format();
        diagnostics::render_incomplete_to_writer_with_format();
        had_errors |= file_report.output.has_errors();
        had_incomplete |= file_report.output.is_incomplete();
    }
    if had_incomplete {
        CheckStatus::Incomplete
    } else if had_errors {
        CheckStatus::Diagnostics
    } else {
        CheckStatus::Clean
    }
}
"#,
    );
    assert!(
        !contract_failures(&duplicated).is_empty(),
        "contract accepted a renamed second report transaction"
    );
}
