//! Source guard for `TypeTag` matches that can hide new variants behind a wildcard.

use std::path::{Path, PathBuf};

#[derive(Debug)]
struct MatchSite {
    key: String,
    body: String,
}

fn rust_sources(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).expect("read source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "tests") {
                continue;
            }
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_none_or(|name| name != "tests.rs")
        {
            files.push(path);
        }
    }
}

fn mentions_variant(body: &str, variant: &str) -> bool {
    let needle = format!("TypeTag::{variant}");
    body.match_indices(&needle).any(|(index, _)| {
        body.as_bytes()
            .get(index + needle.len())
            .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
    })
}

fn declared_type_tag_variants(root: &Path) -> Vec<String> {
    let path = root.join("crates/typokat-types/src/types/repr.rs");
    let source = sanitized(&std::fs::read_to_string(path).expect("read TypeTag declaration"));
    let declaration = source
        .find("pub enum TypeTag")
        .expect("TypeTag declaration");
    let open = source[declaration..]
        .find('{')
        .map(|offset| declaration + offset)
        .expect("TypeTag body");
    let close = matching_brace(&source, open).expect("balanced TypeTag body");
    source[open + 1..close]
        .split(',')
        .filter_map(|field| {
            let name = field
                .split('=')
                .next()
                .expect("variant before discriminant")
                .trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

#[test]
fn type_tag_variant_list_is_frozen_for_exhaustiveness_audit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(
        declared_type_tag_variants(root),
        [
            "Intrinsic",
            "Literal",
            "Object",
            "Union",
            "Intersection",
            "Function",
            "TypeParam",
            "Array",
            "Tuple",
            "Readonly",
            "Conditional",
            "Instantiation",
            "Infer",
            "Mapped",
            "MappedValue",
            "Template",
            "Keyof",
            "ClassInstance",
            "DeferredIndexedAccess",
            "Declared",
            "Enum",
            "EnumMember",
        ],
        "TypeTag changed; adjudicate the new variant at every guarded match site"
    );
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
            let quote = bytes[index];
            let start = index;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == quote {
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

fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn match_sites(root: &Path, path: &Path) -> Vec<MatchSite> {
    let source = std::fs::read_to_string(path).expect("read Rust source");
    let clean = sanitized(&source);
    let mut sites = Vec::new();
    let mut cursor = 0usize;
    let mut ordinal = 0usize;
    while let Some(relative) = clean[cursor..].find("match") {
        let start = cursor + relative;
        cursor = start + "match".len();
        let before = clean.as_bytes()[..start].last().copied();
        let after = clean.as_bytes().get(cursor).copied();
        if before.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || after.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            continue;
        }
        let Some(relative_open) = clean[cursor..].find('{') else {
            break;
        };
        let open = cursor + relative_open;
        let header = clean[cursor..open]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let Some(close) = matching_brace(&clean, open) else {
            panic!("unbalanced match in {}", path.display());
        };
        let body = clean[open + 1..close].to_owned();
        if !top_level_patterns(&body).iter().any(|pattern| {
            pattern
                .split(" if ")
                .next()
                .is_some_and(|head| head.contains("TypeTag::"))
        }) {
            continue;
        }
        ordinal += 1;
        sites.push(MatchSite {
            key: format!(
                "{}#{ordinal}:match {header}",
                path.strip_prefix(root)
                    .expect("source under root")
                    .display()
            ),
            body,
        });
    }
    sites
}

fn top_level_patterns(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut patterns = Vec::new();
    let mut arm_start = 0usize;
    let mut brace_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => brace_depth += 1,
            b'}' => brace_depth = brace_depth.saturating_sub(1),
            b'(' => paren_depth += 1,
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = bracket_depth.saturating_sub(1),
            b'=' if bytes.get(index + 1) == Some(&b'>')
                && brace_depth == 0
                && paren_depth == 0
                && bracket_depth == 0 =>
            {
                patterns.push(body[arm_start..index].trim());
                index += 1;
            }
            b',' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                arm_start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    patterns
}

#[test]
fn production_type_tag_wildcards_adjudicate_enum_variants() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&root.join("crates"), &mut files);
    files.sort();

    let sites = files
        .iter()
        .flat_map(|path| match_sites(root, path))
        .collect::<Vec<_>>();
    assert!(!sites.is_empty(), "no production TypeTag match sites found");

    // Each allowlisted wildcard is a selective classifier whose fallback was reviewed as
    // conservative for both enum tags. Keys are exact so adding or moving a wildcard forces review.
    const ADJUDICATED_WILDCARDS: &[(&str, &str)] = &[
        (
            "crates/typokat-check/src/check/checker/annotations/type_operators.rs#1:match self.interner.store().tag(operand)",
            "keyof keeps non-object enum operands on the existing conservative error path",
        ),
        (
            "crates/typokat-check/src/check/checker/calls.rs#1:match self.interner.store().tag(callee_ty)",
            "the guarded fallback classifies enum values as provably non-callable",
        ),
        (
            "crates/typokat-check/src/check/checker/classes/publication.rs#1:match factory.store().tag(existing.ty)",
            "enum properties do not contribute call-signature overload rows",
        ),
        (
            "crates/typokat-check/src/check/checker/classes/publication.rs#2:match factory.store().tag(property.ty)",
            "enum properties do not contribute call-signature overload rows",
        ),
        (
            "crates/typokat-check/src/check/checker/eval/demand.rs#1:match store.tag(object)",
            "terminal enum nodes require no evaluator demand",
        ),
        (
            "crates/typokat-check/src/check/checker/eval/demand.rs#2:match store.tag(index)",
            "terminal enum nodes require no evaluator demand",
        ),
        (
            "crates/typokat-check/src/check/checker/eval/extends.rs#2:match store.tag(ty)",
            "enum nodes are terminal leaves in conditional concreteness traversal",
        ),
        (
            "crates/typokat-check/src/check/checker/eval/instantiation.rs#2:match self.interner.store().tag(ty)",
            "enum nodes expose no callable receiver signature",
        ),
        (
            "crates/typokat-check/src/check/checker/eval/mod.rs#1:match self.interner.store().tag(ty)",
            "enum nodes are already evaluated values",
        ),
        (
            "crates/typokat-check/src/check/checker/library_compiler/probes.rs#1:match store.tag(ty)",
            "the test probe returns no call signatures for enum nodes",
        ),
        (
            "crates/typokat-check/src/check/checker/namespace_values.rs#1:match self.interner.store().tag(property.ty)",
            "enum namespace properties do not contribute call-signature overload rows",
        ),
        (
            "crates/typokat-check/src/check/flow.rs#3:match store.tag(ty)",
            "whole and member enum typeof overlap is handled explicitly before the fallback",
        ),
        (
            "crates/typokat-check/src/check/flow.rs#4:match store.tag(left)",
            "same-id whole enums can overlap while exact members have their explicit arm",
        ),
        (
            "crates/typokat-check/src/check/query/mod.rs#1:match tag",
            "terminal enum identity is handled before structural recursive identity",
        ),
        (
            "crates/typokat-types/src/types/intern/mod.rs#1:match self.store.tag(ty)",
            "terminal enum nodes have no derivation children",
        ),
        (
            "crates/typokat-types/src/types/substitute/derivation.rs#1:match store.tag(template)",
            "nominal enum nodes do not derive substitutions structurally",
        ),
        (
            "crates/typokat-types/src/types/substitute/derivation.rs#2:match store.tag(template)",
            "nominal enum nodes have no structural object derivation",
        ),
    ];

    let mut unadjudicated = Vec::new();
    let mut observed_allowlist = Vec::new();
    for site in &sites {
        let patterns = top_level_patterns(&site.body);
        let names_both = patterns
            .iter()
            .filter_map(|pattern| pattern.split(" if ").next())
            .any(|pattern| mentions_variant(pattern, "Enum"))
            && patterns
                .iter()
                .filter_map(|pattern| pattern.split(" if ").next())
                .any(|pattern| mentions_variant(pattern, "EnumMember"));
        let has_wildcard = patterns.iter().any(|pattern| {
            let pattern = pattern.trim();
            pattern == "_" || pattern.starts_with("_ if ")
        });
        if has_wildcard && !names_both {
            if ADJUDICATED_WILDCARDS
                .iter()
                .any(|(key, _reason)| *key == site.key)
            {
                observed_allowlist.push(site.key.as_str());
            } else {
                unadjudicated.push(site.key.as_str());
            }
        }
    }
    assert!(
        unadjudicated.is_empty(),
        "TypeTag wildcard sites require Enum/EnumMember adjudication:\n{}",
        unadjudicated.join("\n")
    );
    let stale = ADJUDICATED_WILDCARDS
        .iter()
        .map(|(key, _reason)| *key)
        .filter(|key| !observed_allowlist.contains(key))
        .collect::<Vec<_>>();
    assert!(
        stale.is_empty(),
        "stale TypeTag wildcard adjudications require census update:\n{}",
        stale.join("\n")
    );
}
