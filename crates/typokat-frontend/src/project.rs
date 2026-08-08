//! Deterministic, fail-closed project configuration discovery.

use jsonc_parser::ast::{Object, Value};
use jsonc_parser::{parse_to_ast, CollectOptions, ParseOptions};
use oxc_resolver::{ResolveError, Resolver, TsConfig};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

const CONFIG_NAME: &str = "tsconfig.json";

/// One configured root after resolver-owned decoding and frontend-owned normalization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRoot {
    pub identity: String,
    pub path: PathBuf,
    pub exists: bool,
}

/// Internal discovery product. Production dispatch does not consume it before WU3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredProject {
    pub config_path: PathBuf,
    pub project_directory: PathBuf,
    pub roots: Vec<ProjectRoot>,
    pub notices: Vec<ProjectNotice>,
}

/// A deterministic fail-closed project/config identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectNotice {
    UnsupportedConfigFiles {
        reason: String,
    },
    UnsupportedConfigRoot {
        reason: String,
        root: String,
    },
    UnsupportedConfigRootExtension {
        root: String,
    },
    UnsupportedConfigRootSelection {
        field: String,
    },
    UnsupportedConfigField {
        field: String,
    },
    UnsupportedCompilerOption {
        option: String,
        value: Option<String>,
    },
    MissingConfiguredRoot {
        root: String,
    },
}

impl ProjectNotice {
    /// Stable summary identity pinned by the B72 contract.
    #[must_use]
    pub fn identity(&self) -> String {
        match self {
            Self::UnsupportedConfigFiles { reason } => {
                if reason.starts_with("expected-string ") {
                    format!("unsupported-config-files {reason}")
                } else {
                    format!("unsupported-config-files {reason} {CONFIG_NAME}")
                }
            }
            Self::UnsupportedConfigRoot { reason, root } => {
                format!("unsupported-config-root {reason} {root}")
            }
            Self::UnsupportedConfigRootExtension { root } => {
                format!("unsupported-config-root-extension {root}")
            }
            Self::UnsupportedConfigRootSelection { field } => {
                format!("unsupported-config-root-selection {field} {CONFIG_NAME}")
            }
            Self::UnsupportedConfigField { field } => {
                format!("unsupported-config-field {field} {CONFIG_NAME}")
            }
            Self::UnsupportedCompilerOption { option, value } => match value {
                Some(value) => {
                    format!("unsupported-compiler-option {option} {value} {CONFIG_NAME}")
                }
                None => format!("unsupported-compiler-option {option} {CONFIG_NAME}"),
            },
            Self::MissingConfiguredRoot { root } => {
                format!("missing-configured-root {root}")
            }
        }
    }
}

/// Typed internal discovery failure. WU3 owns its public rendering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectDiscoveryError {
    UnsupportedInput {
        path: PathBuf,
    },
    MissingConfig {
        path: PathBuf,
    },
    ConfigIo {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    MalformedConfig {
        path: PathBuf,
        line: usize,
        column: usize,
    },
    AuditResolverDisagreement {
        path: PathBuf,
        detail: String,
    },
    RootConfigMismatch {
        requested: PathBuf,
        resolved: PathBuf,
    },
    Resolver {
        path: PathBuf,
        detail: String,
    },
}

impl fmt::Display for ProjectDiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedInput { path } => write!(
                formatter,
                "project input must be a directory or an explicit {CONFIG_NAME}: {}",
                path.display()
            ),
            Self::MissingConfig { path } => write!(formatter, "missing config {}", path.display()),
            Self::ConfigIo { path, kind } => {
                write!(
                    formatter,
                    "config IO failure ({kind:?}) at {}",
                    path.display()
                )
            }
            Self::MalformedConfig { path, line, column } => write!(
                formatter,
                "malformed config {} at {line}:{column}",
                path.display()
            ),
            Self::AuditResolverDisagreement { path, detail } => write!(
                formatter,
                "config audit/resolver disagreement at {}: {detail}",
                path.display()
            ),
            Self::RootConfigMismatch {
                requested,
                resolved,
            } => write!(
                formatter,
                "resolver returned {} for requested root config {}",
                resolved.display(),
                requested.display()
            ),
            Self::Resolver { path, detail } => {
                write!(
                    formatter,
                    "resolver failure at {}: {detail}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ProjectDiscoveryError {}

#[derive(Debug)]
struct ConfigAudit {
    notices: Vec<ProjectNotice>,
    invalid_files_schema: bool,
    admitted_scalars: Option<AdmittedScalars>,
}

#[derive(Debug)]
struct AdmittedScalars {
    strict: bool,
    module: String,
}

/// Discover one exact `files`-only project config without exposing it to production dispatch.
pub fn discover_project(input: &Path) -> Result<DiscoveredProject, ProjectDiscoveryError> {
    discover_project_with_resolver(input, |config_path| {
        Resolver::default().resolve_tsconfig(config_path)
    })
}

fn discover_project_with_resolver(
    input: &Path,
    resolve_clean: impl FnOnce(&Path) -> Result<std::sync::Arc<TsConfig>, ResolveError>,
) -> Result<DiscoveredProject, ProjectDiscoveryError> {
    let config_path = locate_root_config(input)?;
    let source = read_root_config(&config_path)?;
    let audit = audit_root_config(&config_path, &source)?;
    let project_directory = config_path.parent().map(Path::to_path_buf).ok_or_else(|| {
        ProjectDiscoveryError::AuditResolverDisagreement {
            path: config_path.clone(),
            detail: "root config has no project directory".to_owned(),
        }
    })?;
    let canonical_path =
        fs::canonicalize(&config_path).map_err(|error| ProjectDiscoveryError::ConfigIo {
            path: config_path.clone(),
            kind: error.kind(),
        })?;
    let parsed = match TsConfig::parse(true, &config_path, &canonical_path, source.clone()) {
        Ok(parsed) => Some(parsed),
        Err(_) if audit.invalid_files_schema => None,
        Err(error) => {
            return Err(ProjectDiscoveryError::AuditResolverDisagreement {
                path: config_path,
                detail: error.to_string(),
            });
        }
    };

    if let (Some(scalars), Some(parsed)) = (&audit.admitted_scalars, &parsed) {
        if parsed.compiler_options.strict != Some(scalars.strict)
            || parsed.compiler_options.module.as_deref() != Some(scalars.module.as_str())
        {
            return Err(ProjectDiscoveryError::AuditResolverDisagreement {
                path: config_path,
                detail: "decoded admitted compiler option changed".to_owned(),
            });
        }
        let resolved = resolve_clean(&config_path)
            .map_err(|error| map_clean_resolver_error(&config_path, error))?;
        verify_same_root_config(&config_path, resolved.path())?;
        if parsed.files != resolved.files {
            let normalized_raw = normalized_root_values(
                parsed.files.as_deref().map_or(&[], |files| files),
                &project_directory,
            );
            let normalized_resolved = normalized_root_values(
                resolved.files.as_deref().map_or(&[], |files| files),
                Path::new(""),
            );
            if normalized_raw != normalized_resolved {
                return Err(ProjectDiscoveryError::AuditResolverDisagreement {
                    path: config_path,
                    detail: "resolver changed configured roots".to_owned(),
                });
            }
        }
    }

    let mut notices = audit.notices;
    let roots = match parsed.and_then(|parsed| parsed.files) {
        Some(files) => normalize_roots(&project_directory, files, &mut notices)?,
        None => Vec::new(),
    };
    Ok(DiscoveredProject {
        config_path,
        project_directory,
        roots,
        notices,
    })
}

fn locate_root_config(input: &Path) -> Result<PathBuf, ProjectDiscoveryError> {
    let absolute = if input.is_absolute() {
        input.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| ProjectDiscoveryError::ConfigIo {
                path: input.to_path_buf(),
                kind: error.kind(),
            })?
            .join(input)
    };
    match fs::metadata(&absolute) {
        Ok(metadata) if metadata.is_dir() => Ok(absolute.join(CONFIG_NAME)),
        Ok(metadata)
            if metadata.is_file()
                && absolute.file_name().is_some_and(|name| name == CONFIG_NAME) =>
        {
            Ok(absolute)
        }
        Ok(metadata) if metadata.is_file() => {
            Err(ProjectDiscoveryError::UnsupportedInput { path: absolute })
        }
        Ok(_) => Err(ProjectDiscoveryError::ConfigIo {
            path: absolute,
            kind: io::ErrorKind::InvalidInput,
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(ProjectDiscoveryError::MissingConfig { path: absolute })
        }
        Err(error) => Err(ProjectDiscoveryError::ConfigIo {
            path: absolute,
            kind: error.kind(),
        }),
    }
}

fn read_root_config(path: &Path) -> Result<String, ProjectDiscoveryError> {
    fs::read_to_string(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            ProjectDiscoveryError::MissingConfig {
                path: path.to_path_buf(),
            }
        } else {
            ProjectDiscoveryError::ConfigIo {
                path: path.to_path_buf(),
                kind: error.kind(),
            }
        }
    })
}

fn audit_root_config(path: &Path, source: &str) -> Result<ConfigAudit, ProjectDiscoveryError> {
    let options = ParseOptions {
        allow_comments: true,
        allow_loose_object_property_names: false,
        allow_trailing_commas: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    };
    let parsed = parse_to_ast(source, &CollectOptions::default(), &options)
        .map_err(|audit_error| classify_audit_syntax_error(path, source, &audit_error))?;
    let Some(Value::Object(root)) = parsed.value else {
        return Err(ProjectDiscoveryError::MalformedConfig {
            path: path.to_path_buf(),
            line: 1,
            column: 1,
        });
    };
    Ok(audit_root_object(source, &root))
}

fn classify_audit_syntax_error(
    path: &Path,
    source: &str,
    audit_error: &jsonc_parser::errors::ParseError,
) -> ProjectDiscoveryError {
    let canonical_path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) => {
            return ProjectDiscoveryError::ConfigIo {
                path: path.to_path_buf(),
                kind: error.kind(),
            };
        }
    };
    match TsConfig::parse(true, path, &canonical_path, source.to_owned()) {
        Err(error) => ProjectDiscoveryError::MalformedConfig {
            path: path.to_path_buf(),
            line: error.line(),
            column: error.column().max(1),
        },
        Ok(_) => ProjectDiscoveryError::AuditResolverDisagreement {
            path: path.to_path_buf(),
            detail: format!(
                "audit rejected JSONC at {}:{} but resolver parser accepted it",
                audit_error.line_display(),
                audit_error.column_display()
            ),
        },
    }
}

fn audit_root_object(source: &str, root: &Object<'_>) -> ConfigAudit {
    let mut notices = Vec::new();
    let mut invalid_files_schema = false;
    let mut admitted_scalars = None;
    let mut files_seen = false;
    let mut seen = BTreeSet::new();
    for property in &root.properties {
        let name = property.name.as_str();
        if !seen.insert(name.to_owned()) {
            notices.push(ProjectNotice::UnsupportedConfigField {
                field: format!("duplicate-{name}"),
            });
            continue;
        }
        match name {
            "compilerOptions" => match property.value.as_object() {
                Some(options) => {
                    admitted_scalars = Some(audit_compiler_options(Some(options), &mut notices));
                }
                None => notices.push(ProjectNotice::UnsupportedConfigField {
                    field: "compilerOptions expected-object".to_owned(),
                }),
            },
            "files" => {
                files_seen = true;
                audit_files(
                    source,
                    &property.value,
                    &mut notices,
                    &mut invalid_files_schema,
                );
            }
            "include" | "exclude" => {
                notices.push(ProjectNotice::UnsupportedConfigRootSelection {
                    field: name.to_owned(),
                });
            }
            "extends" | "references" => notices.push(ProjectNotice::UnsupportedConfigField {
                field: name.to_owned(),
            }),
            other => notices.push(ProjectNotice::UnsupportedConfigField {
                field: other.to_owned(),
            }),
        }
    }
    if !files_seen {
        notices.push(ProjectNotice::UnsupportedConfigFiles {
            reason: "missing".to_owned(),
        });
    }
    let admitted_scalars = match admitted_scalars {
        Some(scalars) => scalars,
        None => audit_compiler_options(None, &mut notices),
    };
    ConfigAudit {
        admitted_scalars: notices.is_empty().then_some(admitted_scalars),
        notices,
        invalid_files_schema,
    }
}

fn audit_files(
    source: &str,
    value: &Value<'_>,
    notices: &mut Vec<ProjectNotice>,
    invalid_schema: &mut bool,
) {
    let Some(array) = value.as_array() else {
        notices.push(ProjectNotice::UnsupportedConfigFiles {
            reason: "expected-array".to_owned(),
        });
        *invalid_schema = true;
        return;
    };
    if array.elements.is_empty() {
        notices.push(ProjectNotice::UnsupportedConfigFiles {
            reason: "empty".to_owned(),
        });
        return;
    }
    for element in &array.elements {
        let Some(root) = element.as_string_lit() else {
            let line = source[..element_range_start(element)]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                .saturating_add(1);
            notices.push(ProjectNotice::UnsupportedConfigFiles {
                reason: format!("expected-string {CONFIG_NAME}:{line}"),
            });
            *invalid_schema = true;
            continue;
        };
        audit_root_name(root.value.as_ref(), notices);
    }
}

fn element_range_start(value: &Value<'_>) -> usize {
    use jsonc_parser::common::Ranged;
    value.start()
}

fn audit_root_name(root: &str, notices: &mut Vec<ProjectNotice>) {
    let path = Path::new(root);
    if path.is_absolute() {
        notices.push(ProjectNotice::UnsupportedConfigRoot {
            reason: "absolute".to_owned(),
            root: root.to_owned(),
        });
        return;
    }
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        notices.push(ProjectNotice::UnsupportedConfigRoot {
            reason: "parent-traversal".to_owned(),
            root: root.to_owned(),
        });
        return;
    }
    let is_ts =
        path.extension().is_some_and(|extension| extension == "ts") && !root.ends_with(".d.ts");
    if !is_ts {
        notices.push(ProjectNotice::UnsupportedConfigRootExtension {
            root: root.to_owned(),
        });
    }
}

fn audit_compiler_options(
    options: Option<&Object<'_>>,
    notices: &mut Vec<ProjectNotice>,
) -> AdmittedScalars {
    let mut strict = None;
    let mut no_emit = None;
    let mut module = None;
    let mut module_resolution = None;
    let mut seen = BTreeSet::new();
    if let Some(options) = options {
        for property in &options.properties {
            let name = property.name.as_str();
            if !seen.insert(name.to_owned()) {
                notices.push(ProjectNotice::UnsupportedCompilerOption {
                    option: format!("duplicate-{name}"),
                    value: None,
                });
                continue;
            }
            match name {
                "strict" => strict = boolean_option(name, &property.value, notices),
                "noEmit" => no_emit = boolean_option(name, &property.value, notices),
                "module" => module = string_option(name, "ESNext", &property.value, notices),
                "moduleResolution" => {
                    module_resolution = string_option(name, "Bundler", &property.value, notices);
                }
                other => notices.push(ProjectNotice::UnsupportedCompilerOption {
                    option: other.to_owned(),
                    value: None,
                }),
            }
        }
    }
    missing_boolean_option("strict", strict, notices);
    missing_boolean_option("noEmit", no_emit, notices);
    missing_string_option("module", module.as_deref(), notices);
    missing_string_option("moduleResolution", module_resolution.as_deref(), notices);
    AdmittedScalars {
        strict: strict == Some(true),
        module: match module {
            Some(module) => module,
            None => "ESNext".to_owned(),
        },
    }
}

fn boolean_option(name: &str, value: &Value<'_>, notices: &mut Vec<ProjectNotice>) -> Option<bool> {
    match value.as_boolean_lit().map(|literal| literal.value) {
        Some(true) => Some(true),
        Some(false) => {
            notices.push(ProjectNotice::UnsupportedCompilerOption {
                option: name.to_owned(),
                value: Some("false".to_owned()),
            });
            Some(false)
        }
        None => {
            notices.push(ProjectNotice::UnsupportedCompilerOption {
                option: name.to_owned(),
                value: Some("expected-boolean".to_owned()),
            });
            Some(false)
        }
    }
}

fn string_option(
    name: &str,
    expected: &str,
    value: &Value<'_>,
    notices: &mut Vec<ProjectNotice>,
) -> Option<String> {
    match value.as_string_lit() {
        Some(actual) if actual.value == expected => Some(actual.value.to_string()),
        Some(actual) => {
            notices.push(ProjectNotice::UnsupportedCompilerOption {
                option: name.to_owned(),
                value: Some(actual.value.to_lowercase()),
            });
            Some(actual.value.to_string())
        }
        None => {
            notices.push(ProjectNotice::UnsupportedCompilerOption {
                option: name.to_owned(),
                value: Some("expected-string".to_owned()),
            });
            Some(String::new())
        }
    }
}

fn missing_boolean_option(name: &str, value: Option<bool>, notices: &mut Vec<ProjectNotice>) {
    if value.is_none() {
        notices.push(ProjectNotice::UnsupportedCompilerOption {
            option: name.to_owned(),
            value: Some("missing".to_owned()),
        });
    }
}

fn missing_string_option(name: &str, value: Option<&str>, notices: &mut Vec<ProjectNotice>) {
    if value.is_none() {
        notices.push(ProjectNotice::UnsupportedCompilerOption {
            option: name.to_owned(),
            value: Some("missing".to_owned()),
        });
    }
}

fn normalize_roots(
    project_directory: &Path,
    configured: Vec<PathBuf>,
    notices: &mut Vec<ProjectNotice>,
) -> Result<Vec<ProjectRoot>, ProjectDiscoveryError> {
    let canonical_project =
        fs::canonicalize(project_directory).map_err(|error| ProjectDiscoveryError::ConfigIo {
            path: project_directory.to_path_buf(),
            kind: error.kind(),
        })?;
    let mut roots = BTreeMap::new();
    for configured_root in configured {
        let Some(identity) = normalize_root_identity(&configured_root) else {
            continue;
        };
        let path = project_directory.join(&identity);
        let exists = fs::metadata(&path).is_ok_and(|metadata| metadata.is_file());
        if !exists {
            notices.push(ProjectNotice::MissingConfiguredRoot {
                root: identity.clone(),
            });
        } else {
            let canonical_root =
                fs::canonicalize(&path).map_err(|error| ProjectDiscoveryError::ConfigIo {
                    path: path.clone(),
                    kind: error.kind(),
                })?;
            if !canonical_root.starts_with(&canonical_project) {
                notices.push(ProjectNotice::UnsupportedConfigRoot {
                    reason: "symlink-escape".to_owned(),
                    root: identity.clone(),
                });
            }
        }
        roots.entry(identity.clone()).or_insert(ProjectRoot {
            identity,
            path,
            exists,
        });
    }
    Ok(roots.into_values().collect())
}

fn normalize_root_identity(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn normalized_root_values(paths: &[PathBuf], base: &Path) -> Vec<PathBuf> {
    let mut values = paths
        .iter()
        .filter_map(|path| {
            if path.is_absolute() {
                Some(path.clone())
            } else {
                normalize_root_identity(path).map(|identity| base.join(identity))
            }
        })
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn verify_same_root_config(requested: &Path, resolved: &Path) -> Result<(), ProjectDiscoveryError> {
    let requested_canonical =
        fs::canonicalize(requested).map_err(|error| ProjectDiscoveryError::ConfigIo {
            path: requested.to_path_buf(),
            kind: error.kind(),
        })?;
    let resolved_canonical =
        fs::canonicalize(resolved).map_err(|error| ProjectDiscoveryError::ConfigIo {
            path: resolved.to_path_buf(),
            kind: error.kind(),
        })?;
    if requested_canonical == resolved_canonical {
        Ok(())
    } else {
        Err(ProjectDiscoveryError::RootConfigMismatch {
            requested: requested.to_path_buf(),
            resolved: resolved.to_path_buf(),
        })
    }
}

fn map_clean_resolver_error(path: &Path, error: ResolveError) -> ProjectDiscoveryError {
    match error {
        ResolveError::TsconfigNotFound(missing) => {
            ProjectDiscoveryError::AuditResolverDisagreement {
                path: path.to_path_buf(),
                detail: format!(
                    "resolver reported an already-read root config missing: {}",
                    missing.display()
                ),
            }
        }
        ResolveError::TsconfigLoadFailed { source, .. } => {
            ProjectDiscoveryError::AuditResolverDisagreement {
                path: path.to_path_buf(),
                detail: source.to_string(),
            }
        }
        other => ProjectDiscoveryError::Resolver {
            path: path.to_path_buf(),
            detail: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value as JsonValue;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_ID: AtomicU64 = AtomicU64::new(0);

    fn repository_root() -> PathBuf {
        let current = std::env::current_dir().expect("read test working directory");
        current
            .ancestors()
            .find(|candidate| {
                candidate.join("Cargo.toml").is_file()
                    && candidate
                        .join("tests/cases/b72_bundler_project_tracer/contract.json")
                        .is_file()
            })
            .map(Path::to_path_buf)
            .expect("find typokat repository root from test working directory")
    }

    fn corpus_root() -> PathBuf {
        repository_root().join("tests/cases/b72_bundler_project_tracer")
    }

    fn contract() -> JsonValue {
        let source =
            fs::read_to_string(corpus_root().join("contract.json")).expect("read B72 contract");
        serde_json::from_str(&source).expect("parse B72 contract")
    }

    fn notice_identities(project: &DiscoveredProject) -> Vec<String> {
        project
            .notices
            .iter()
            .map(ProjectNotice::identity)
            .collect()
    }

    #[test]
    fn all_config_boundary_notices_and_published_roots_match_the_contract() {
        let contract = contract();
        let cases = contract["config_boundary_cases"]
            .as_array()
            .expect("config boundary cases");
        for case in cases {
            let id = case["id"].as_str().expect("case id");
            let project = discover_project(&corpus_root().join(id))
                .unwrap_or_else(|error| panic!("discover {id}: {error}"));
            let expected_notices = case["project_notices"]
                .as_array()
                .expect("project notices")
                .iter()
                .map(|notice| notice.as_str().expect("notice string").to_owned())
                .collect::<Vec<_>>();
            let expected_roots = case["roots"]
                .as_array()
                .expect("roots")
                .iter()
                .map(|root| root.as_str().expect("root string").to_owned())
                .collect::<Vec<_>>();
            assert_eq!(
                notice_identities(&project),
                expected_notices,
                "{id} notices"
            );
            assert_eq!(
                project
                    .roots
                    .iter()
                    .map(|root| root.identity.clone())
                    .collect::<Vec<_>>(),
                expected_roots,
                "{id} roots"
            );
        }
    }

    #[test]
    fn directory_and_config_inputs_are_identical_and_roots_are_normalized() {
        let directory = corpus_root().join("admitted_files_js_substitution");
        let from_directory = discover_project(&directory).expect("directory discovery");
        let from_config = discover_project(&directory.join(CONFIG_NAME)).expect("config discovery");
        assert_eq!(from_directory, from_config);
        assert_eq!(
            from_directory
                .roots
                .iter()
                .map(|root| root.identity.as_str())
                .collect::<Vec<_>>(),
            vec!["main.ts", "value.ts"]
        );
        assert!(from_directory.notices.is_empty());
    }

    #[test]
    fn missing_and_malformed_configs_remain_typed_errors() {
        let missing_directory = corpus_root().join("missing_directory_config");
        assert_eq!(
            discover_project(&missing_directory),
            Err(ProjectDiscoveryError::MissingConfig {
                path: missing_directory.join(CONFIG_NAME)
            })
        );

        let malformed = corpus_root().join("malformed_config/tsconfig.json");
        assert_eq!(
            discover_project(&malformed),
            Err(ProjectDiscoveryError::MalformedConfig {
                path: malformed,
                line: 5,
                column: 1
            })
        );
    }

    #[test]
    fn generic_unknown_keys_and_lib_fail_closed_in_source_order() {
        let directory = temp_project("unknown-keys");
        write_config(
            &directory,
            r#"{
  "unknownTop": true,
  "compilerOptions": {
    "strict": true,
    "noEmit": true,
    "module": "ESNext",
    "lib": ["ES2025"],
    "unknownOption": true,
    "moduleResolution": "Bundler"
  },
  "files": ["main.ts"]
}"#,
        );
        fs::write(directory.join("main.ts"), "export {};\n").expect("write source");
        let project = discover_project(&directory).expect("unknown config discovery");
        assert_eq!(
            notice_identities(&project),
            vec![
                "unsupported-config-field unknownTop tsconfig.json",
                "unsupported-compiler-option lib tsconfig.json",
                "unsupported-compiler-option unknownOption tsconfig.json",
            ]
        );
        assert_eq!(project.roots[0].identity, "main.ts");
        fs::remove_dir_all(directory).expect("remove temp project");
    }

    #[test]
    fn repeated_discovery_is_deterministic() {
        let directory = corpus_root().join("unsupported_root_globs");
        let first = discover_project(&directory).expect("first discovery");
        for _ in 0..8 {
            assert_eq!(discover_project(&directory), Ok(first.clone()));
        }
    }

    #[test]
    fn invalid_files_shapes_are_not_parser_fallbacks() {
        for id in ["config_non_array_files", "config_non_string_file"] {
            let project = discover_project(&corpus_root().join(id)).expect("typed notice");
            assert!(project.roots.is_empty());
            assert!(notice_identities(&project)[0].starts_with("unsupported-config-files"));
        }
    }

    #[test]
    fn audit_clean_oxc_rejection_cannot_fall_back() {
        let directory = corpus_root().join("admitted_files_extensionless");
        let error = discover_project_with_resolver(&directory, |_| {
            Err(ResolveError::TsconfigNotFound(PathBuf::from("forced")))
        });
        assert_eq!(
            error,
            Err(ProjectDiscoveryError::AuditResolverDisagreement {
                path: directory.join(CONFIG_NAME),
                detail: "resolver reported an already-read root config missing: forced".to_owned()
            })
        );
    }

    #[test]
    fn only_directories_and_explicit_tsconfig_files_are_admitted_inputs() {
        let directory = temp_project("input-kind");
        for name in ["foo.json", "source.ts"] {
            let path = directory.join(name);
            fs::write(&path, "{}").expect("write inadmissible input");
            assert_eq!(
                discover_project(&path),
                Err(ProjectDiscoveryError::UnsupportedInput { path })
            );
        }
        fs::remove_dir_all(directory).expect("remove temp project");
    }

    #[test]
    fn extends_and_references_never_invoke_resolver_composition() {
        use std::cell::Cell;

        for id in ["config_extends", "config_references"] {
            let invoked = Cell::new(false);
            let project = discover_project_with_resolver(&corpus_root().join(id), |_| {
                invoked.set(true);
                Err(ResolveError::TsconfigNotFound(PathBuf::from(
                    "must-not-run",
                )))
            })
            .unwrap_or_else(|error| panic!("discover {id}: {error}"));
            assert!(!invoked.get(), "{id} must not compose config");
            assert_eq!(
                project
                    .roots
                    .iter()
                    .map(|root| root.identity.as_str())
                    .collect::<Vec<_>>(),
                vec!["main.ts"]
            );
        }
    }

    #[test]
    fn production_dispatch_reaches_discovery_only_at_the_cli_boundary() {
        let repository = repository_root();
        let main = fs::read_to_string(repository.join("src/main.rs")).expect("read CLI source");
        let driver = fs::read_to_string(repository.join("crates/typokat-driver/src/driver.rs"))
            .expect("read driver source");
        assert!(main.contains("discover_project"));
        assert!(!driver.contains("discover_project"));
    }

    fn temp_project(label: &str) -> PathBuf {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "typokat-project-discovery-{}-{label}-{id}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create temp project");
        directory
    }

    fn write_config(directory: &Path, source: &str) {
        fs::write(directory.join(CONFIG_NAME), source).expect("write config");
    }
}
