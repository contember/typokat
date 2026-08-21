//! Source contract for the private module-accounting scan boundary.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

const FRONTEND_SOURCE: &str = include_str!("../crates/typokat-frontend/src/frontend.rs");

#[derive(Clone, Debug)]
struct Token {
    text: String,
}

#[derive(Clone, Debug)]
struct Function {
    name: String,
    declaration: usize,
    opening: usize,
    closing: usize,
}

fn production_source(source: &str) -> &str {
    source
        .split_once("\n#[cfg(test)]\nmod ")
        .map_or(source, |(production, _tests)| production)
}

fn sanitized(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"//") {
            let start = index;
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            output[start..index].fill(b' ');
        } else if bytes[index..].starts_with(b"/*") {
            let start = index;
            index += 2;
            let mut depth = 1usize;
            while index < bytes.len() && depth > 0 {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            for byte in &mut output[start..index] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        } else if bytes[index] == b'"' {
            let mut prefix = index;
            while prefix > 0 && bytes[prefix - 1] == b'#' {
                prefix -= 1;
            }
            let raw_hashes = index - prefix;
            let is_raw = prefix > 0 && bytes[prefix - 1] == b'r';
            let start = index;
            index += 1;
            while index < bytes.len() {
                if !is_raw && bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == b'"'
                    && (!is_raw
                        || bytes
                            .get(index + 1..index + 1 + raw_hashes)
                            .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#')))
                {
                    index += 1 + raw_hashes;
                    break;
                } else {
                    index += 1;
                }
            }
            for byte in &mut output[start..index] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        } else if bytes[index] == b'\''
            && (bytes.get(index + 2) == Some(&b'\'')
                || (bytes.get(index + 1) == Some(&b'\\') && bytes.get(index + 3) == Some(&b'\'')))
        {
            let start = index;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == b'\'' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            for byte in &mut output[start..index] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        } else {
            index += 1;
        }
    }
    String::from_utf8(output).expect("sanitized Rust source stays UTF-8")
}

fn tokens(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(Token {
                text: source[start..index].to_owned(),
            });
        } else if bytes[index..].starts_with(b"::") {
            tokens.push(Token {
                text: "::".to_owned(),
            });
            index += 2;
        } else {
            tokens.push(Token {
                text: source[index..index + 1].to_owned(),
            });
            index += 1;
        }
    }
    tokens
}

fn matching_token(tokens: &[Token], opening: usize, open: &str, close: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(opening) {
        if token.text == open {
            depth += 1;
        } else if token.text == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn functions(tokens: &[Token]) -> Vec<Function> {
    let mut functions = Vec::new();
    for declaration in 0..tokens.len().saturating_sub(1) {
        if tokens[declaration].text != "fn" {
            continue;
        }
        let name = &tokens[declaration + 1].text;
        if !name
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        {
            continue;
        }
        let Some(opening) = (declaration + 2..tokens.len())
            .take_while(|index| tokens[*index].text != ";")
            .find(|index| tokens[*index].text == "{")
        else {
            continue;
        };
        let Some(closing) = matching_token(tokens, opening, "{", "}") else {
            continue;
        };
        functions.push(Function {
            name: name.clone(),
            declaration,
            opening,
            closing,
        });
    }
    functions
}

fn module_accounting_impl(tokens: &[Token]) -> Option<(usize, usize)> {
    let declaration = tokens
        .windows(2)
        .position(|window| window[0].text == "impl" && window[1].text == "ModuleAccounting")?;
    let opening = (declaration + 2..tokens.len()).find(|index| tokens[*index].text == "{")?;
    let closing = matching_token(tokens, opening, "{", "}")?;
    Some((opening, closing))
}

fn direct_calls<'a>(tokens: &'a [Token], function: &Function) -> Vec<&'a str> {
    let body = &tokens[function.opening + 1..function.closing];
    body.iter()
        .enumerate()
        .filter_map(|(index, token)| {
            (body.get(index + 1).is_some_and(|next| next.text == "(")
                && body
                    .get(index.wrapping_sub(1))
                    .is_none_or(|previous| previous.text != "." && previous.text != "::"))
            .then_some(token.text.as_str())
        })
        .collect()
}

fn statement_dispatches(tokens: &[Token], function: &Function) -> usize {
    let body = &tokens[function.opening + 1..function.closing];
    body.windows(4)
        .filter(|window| {
            window[0].text == "."
                && window[1].text == "body"
                && window[2].text == "."
                && matches!(window[3].text.as_str(), "iter" | "iter_mut" | "into_iter")
        })
        .count()
        + body
            .iter()
            .enumerate()
            .filter(|(index, token)| {
                token.text == "body"
                    && body
                        .get(index.wrapping_sub(1))
                        .is_some_and(|token| token.text == ".")
                    && body.get(index + 1).is_none_or(|token| token.text != ".")
                    && body[..*index]
                        .iter()
                        .rev()
                        .take_while(|token| !matches!(token.text.as_str(), ";" | "{" | "}"))
                        .any(|token| token.text == "for")
            })
            .count()
}

fn scan_consumes_owner(tokens: &[Token], function: &Function) -> bool {
    let Some(opening) =
        (function.declaration..function.opening).find(|index| tokens[*index].text == "(")
    else {
        return false;
    };
    let Some(closing) = matching_token(tokens, opening, "(", ")") else {
        return false;
    };
    let receiver = tokens[opening + 1..closing]
        .split(|token| token.text == ",")
        .next()
        .unwrap_or_default();
    receiver.iter().any(|token| token.text == "self")
        && receiver.iter().all(|token| token.text != "&")
}

fn contract_failures(source: &str) -> Vec<String> {
    let clean = sanitized(production_source(source));
    let tokens = tokens(&clean);
    let functions = functions(&tokens);
    let by_name = functions.iter().enumerate().fold(
        BTreeMap::<&str, Vec<usize>>::new(),
        |mut by_name, (index, function)| {
            by_name
                .entry(function.name.as_str())
                .or_default()
                .push(index);
            by_name
        },
    );
    let mut failures = Vec::new();
    let mut seeds = by_name
        .get("account_project_modules")
        .cloned()
        .unwrap_or_default();
    let module_impl = module_accounting_impl(&tokens);
    if let Some((opening, closing)) = module_impl {
        seeds.extend(
            functions
                .iter()
                .enumerate()
                .filter(|(_, function)| {
                    function.declaration > opening && function.declaration < closing
                })
                .map(|(index, _)| index),
        );
    } else {
        failures.push("missing private ModuleAccounting owner".to_owned());
    }
    if !by_name.contains_key("account_project_modules") {
        failures.push("missing account_project_modules facade".to_owned());
    }

    let mut reachable = BTreeSet::new();
    let mut pending = VecDeque::from(seeds);
    while let Some(index) = pending.pop_front() {
        if !reachable.insert(index) {
            continue;
        }
        for called in direct_calls(&tokens, &functions[index]) {
            if let Some(targets) = by_name.get(called) {
                pending.extend(targets.iter().copied());
            }
        }
    }

    let dispatches = reachable
        .iter()
        .map(|index| statement_dispatches(&tokens, &functions[*index]))
        .sum::<usize>();
    if dispatches != 1 {
        failures.push(format!(
            "expected exactly one reachable program.body statement dispatch, found {dispatches}"
        ));
    }

    let scan_methods = module_impl.map_or_else(Vec::new, |(opening, closing)| {
        functions
            .iter()
            .filter(|function| {
                function.name == "scan"
                    && function.declaration > opening
                    && function.declaration < closing
            })
            .collect::<Vec<_>>()
    });
    if scan_methods.len() == 1 && statement_dispatches(&tokens, scan_methods[0]) != 1 {
        failures.push(
            "ModuleAccounting::scan must contain the sole program.body statement dispatch"
                .to_owned(),
        );
    }
    if scan_methods.len() != 1 || !scan_consumes_owner(&tokens, scan_methods[0]) {
        failures.push("ModuleAccounting::scan must consume its owner exactly once".to_owned());
    }

    failures
}

fn owned_scan_fixture(extra: &str) -> String {
    format!(
        r#"
struct ModuleAccounting;

impl ModuleAccounting {{
    fn scan(self, program: &Program<'_>) -> Self {{
        for statement in &program.body {{
            visit(statement);
        }}
        {extra}
        self
    }}

    fn finalize(self) {{}}
}}

fn account_project_modules() {{
    build_owner();
}}
"#
    )
}

#[test]
fn module_accounting_has_one_reachable_statement_dispatch() {
    let failures = contract_failures(FRONTEND_SOURCE);
    assert!(
        failures.is_empty(),
        "module accounting structure contract failed:\n{}",
        failures.join("\n")
    );
}

#[test]
fn contract_rejects_a_renamed_reachable_duplicate_loop() {
    let fixture = format!(
        "{}\nfn renamed_evidence_collector(program: &Program<'_>) {{\n    for item in &program.body {{ visit(item); }}\n}}\n",
        owned_scan_fixture("renamed_evidence_collector(program);")
    );
    assert_eq!(
        contract_failures(&fixture),
        ["expected exactly one reachable program.body statement dispatch, found 2"]
    );
}

#[test]
fn contract_ignores_comment_and_string_decoys() {
    let fixture = owned_scan_fixture(
        r#"
// renamed_evidence_collector(program); for item in &program.body {}
let decoy = "renamed_evidence_collector(program); program.body.iter()";
"#,
    );
    assert!(contract_failures(&fixture).is_empty());
}

#[test]
fn contract_rejects_a_borrowed_reusable_scan_owner() {
    let fixture = owned_scan_fixture("").replacen("fn scan(self,", "fn scan(&mut self,", 1);
    assert_eq!(
        contract_failures(&fixture),
        ["ModuleAccounting::scan must consume its owner exactly once"]
    );
}
