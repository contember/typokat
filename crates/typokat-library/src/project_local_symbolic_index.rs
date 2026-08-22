use crate::binder::declaration::{
    DeclarationKind, SourceDeclarationManifest, SourceInterfaceOccurrenceKind, TypeFragmentKind,
};
use crate::profile::ExactLibraryProfile;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const LOCKED_SLICE: &str = include_str!("project_local_index_feasibility_cases/locked-slice.toml");
const INDEX_DOMAIN: &[u8] = b"typokat-project-local-symbolic-index-v1";
const GROUP_DOMAIN: &[u8] = b"typokat-project-local-symbolic-group-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SymbolicSlot {
    Value,
    Type,
    Namespace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SymbolicTypeForm {
    Alias,
    Interface,
    Class,
    Enum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SymbolicDeclarationForm {
    Variable,
    Namespace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SymbolicLexicalEventKind {
    Header,
    Heritage,
    Member,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SymbolicLexicalEvent {
    pub(crate) kind: SymbolicLexicalEventKind,
    pub(crate) start: u32,
    pub(crate) end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SymbolicLoweringRecipe {
    TypeFragment {
        form: SymbolicTypeForm,
        declaration_start: u32,
        declaration_end: u32,
        binding_start: u32,
        binding_end: u32,
        lexical_events: Vec<SymbolicLexicalEvent>,
    },
    Declaration {
        form: SymbolicDeclarationForm,
        statement_start: u32,
        declaration_start: u32,
        declaration_end: u32,
        binding_start: u32,
        binding_end: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SymbolicSource {
    pub(crate) ordinal: usize,
    pub(crate) name: String,
    pub(crate) sha256: String,
    pub(crate) lexical_statement_starts: Vec<u32>,
    pub(crate) lexical_member_starts: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SymbolicFragment {
    pub(crate) source_ordinal: usize,
    pub(crate) statement_start: u32,
    pub(crate) source_sha256: String,
    pub(crate) recipe: SymbolicLoweringRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SymbolicGroup {
    pub(crate) identity: String,
    pub(crate) name: String,
    pub(crate) slots: Vec<SymbolicSlot>,
    pub(crate) fragments: Vec<SymbolicFragment>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SymbolicDependency {
    pub(crate) from: String,
    pub(crate) to: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectLocalSymbolicIndex {
    pub(crate) profile_identity: String,
    pub(crate) sources: Vec<SymbolicSource>,
    pub(crate) groups: Vec<SymbolicGroup>,
    pub(crate) dependencies: Vec<SymbolicDependency>,
    pub(crate) owner_sccs: Vec<Vec<String>>,
    pub(crate) publication_sccs: Vec<Vec<String>>,
    pub(crate) required_demands: Vec<String>,
    pub(crate) global_keys: Vec<String>,
}

#[derive(Clone, Debug)]
struct LockedSource {
    ordinal: usize,
    name: String,
    sha256: String,
}

#[derive(Clone, Debug)]
struct LockedGroup {
    name: String,
    slots: Vec<SymbolicSlot>,
    statement_sites: Vec<(usize, u32)>,
}

#[derive(Clone, Debug)]
struct LockedSpec {
    profile_identity: String,
    sources: Vec<LockedSource>,
    groups: Vec<LockedGroup>,
    dependencies: Vec<SymbolicDependency>,
    owner_sccs: Vec<Vec<String>>,
    publication_sccs: Vec<Vec<String>>,
    required_demands: Vec<String>,
    global_keys: Vec<String>,
    event_order: BTreeMap<usize, (Vec<u32>, Vec<u32>)>,
}

impl ProjectLocalSymbolicIndex {
    pub(crate) fn build(library_source_order: &[usize]) -> Result<Self, String> {
        let locked = parse_locked_spec()?;
        validate_requested_source_order(&locked, library_source_order)?;
        let profile = ExactLibraryProfile::load_packaged().map_err(|error| error.to_string())?;
        if profile.profile_identity() != locked.profile_identity {
            return Err("locked profile identity differs from the packaged profile".to_owned());
        }

        let locked_sources = locked
            .sources
            .iter()
            .map(|source| (source.ordinal, source))
            .collect::<BTreeMap<_, _>>();
        let profile_sources = profile
            .sources()
            .iter()
            .map(|source| (source.ordinal().index(), source))
            .collect::<BTreeMap<_, _>>();
        let groups_by_source = locked
            .groups
            .iter()
            .flat_map(|group| {
                group
                    .statement_sites
                    .iter()
                    .map(move |(ordinal, start)| (*ordinal, (group, *start)))
            })
            .fold(
                BTreeMap::<usize, Vec<(&LockedGroup, u32)>>::new(),
                |mut rows, item| {
                    rows.entry(item.0).or_default().push(item.1);
                    rows
                },
            );

        let mut sources = Vec::new();
        let mut fragments_by_group = BTreeMap::<String, Vec<SymbolicFragment>>::new();
        for ordinal in library_source_order {
            let locked_source = locked_sources
                .get(ordinal)
                .ok_or_else(|| format!("source ordinal {ordinal} is outside the locked slice"))?;
            let profile_source = profile_sources
                .get(ordinal)
                .ok_or_else(|| format!("packaged source ordinal {ordinal} is missing"))?;
            if profile_source.name() != locked_source.name {
                return Err(format!(
                    "source ordinal {ordinal} name differs from the locked slice"
                ));
            }
            let actual_sha256 = sha256_hex(profile_source.bytes());
            if actual_sha256 != locked_source.sha256 {
                return Err(format!(
                    "source ordinal {ordinal} bytes differ from the locked slice"
                ));
            }
            let source_text = std::str::from_utf8(profile_source.bytes())
                .map_err(|_| format!("source ordinal {ordinal} is not UTF-8"))?;
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, source_text, SourceType::ts()).parse();
            if parsed.panicked || !parsed.diagnostics.is_empty() {
                return Err(format!("source ordinal {ordinal} did not parse cleanly"));
            }
            let manifest = SourceDeclarationManifest::build(&parsed.program);
            let selected_groups = groups_by_source
                .get(ordinal)
                .ok_or_else(|| format!("source ordinal {ordinal} has no locked groups"))?;
            for (group, statement_start) in selected_groups {
                let recipe = project_recipe(&manifest, source_text, group, *statement_start)?;
                fragments_by_group
                    .entry(group.name.clone())
                    .or_default()
                    .push(SymbolicFragment {
                        source_ordinal: *ordinal,
                        statement_start: *statement_start,
                        source_sha256: actual_sha256.clone(),
                        recipe,
                    });
            }
            let (statement_starts, member_starts) =
                locked.event_order.get(ordinal).cloned().ok_or_else(|| {
                    format!("source ordinal {ordinal} has no lexical event order")
                })?;
            validate_lexical_order(
                &manifest,
                selected_groups,
                &statement_starts,
                &member_starts,
                *ordinal,
            )?;
            sources.push(SymbolicSource {
                ordinal: *ordinal,
                name: profile_source.name().to_owned(),
                sha256: actual_sha256,
                lexical_statement_starts: statement_starts,
                lexical_member_starts: member_starts,
            });
        }
        sources.sort_by_key(|source| source.ordinal);

        let mut groups = Vec::with_capacity(locked.groups.len());
        for group in &locked.groups {
            let mut fragments = fragments_by_group
                .remove(&group.name)
                .ok_or_else(|| format!("group {:?} has no projected fragments", group.name))?;
            fragments.sort_by_key(|fragment| (fragment.source_ordinal, fragment.statement_start));
            if fragments.len() != group.statement_sites.len() {
                return Err(format!(
                    "group {:?} fragment count differs from the lock",
                    group.name
                ));
            }
            validate_slots(group, &fragments)?;
            let identity = group_identity(
                &locked.profile_identity,
                &group.name,
                &group.slots,
                &fragments,
            )?;
            groups.push(SymbolicGroup {
                identity,
                name: group.name.clone(),
                slots: group.slots.clone(),
                fragments,
            });
        }
        groups.sort_by(|left, right| left.name.cmp(&right.name));
        validate_graph(&groups, &locked)?;

        Ok(Self {
            profile_identity: locked.profile_identity,
            sources,
            groups,
            dependencies: locked.dependencies,
            owner_sccs: locked.owner_sccs,
            publication_sccs: locked.publication_sccs,
            required_demands: locked.required_demands,
            global_keys: locked.global_keys,
        })
    }

    pub(crate) fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        put_bytes(&mut bytes, INDEX_DOMAIN)?;
        put_string(&mut bytes, &self.profile_identity)?;
        put_len(&mut bytes, self.sources.len())?;
        for source in &self.sources {
            put_usize(&mut bytes, source.ordinal)?;
            put_string(&mut bytes, &source.name)?;
            put_string(&mut bytes, &source.sha256)?;
            put_u32s(&mut bytes, &source.lexical_statement_starts)?;
            put_u32s(&mut bytes, &source.lexical_member_starts)?;
        }
        put_len(&mut bytes, self.groups.len())?;
        for group in &self.groups {
            put_string(&mut bytes, &group.identity)?;
            put_string(&mut bytes, &group.name)?;
            put_len(&mut bytes, group.slots.len())?;
            for slot in &group.slots {
                bytes.push(slot_tag(*slot));
            }
            put_len(&mut bytes, group.fragments.len())?;
            for fragment in &group.fragments {
                put_usize(&mut bytes, fragment.source_ordinal)?;
                bytes.extend_from_slice(&fragment.statement_start.to_be_bytes());
                put_string(&mut bytes, &fragment.source_sha256)?;
                put_recipe(&mut bytes, &fragment.recipe)?;
            }
        }
        put_len(&mut bytes, self.dependencies.len())?;
        for dependency in &self.dependencies {
            put_string(&mut bytes, &dependency.from)?;
            put_string(&mut bytes, &dependency.to)?;
        }
        put_nested_strings(&mut bytes, &self.owner_sccs)?;
        put_nested_strings(&mut bytes, &self.publication_sccs)?;
        put_strings(&mut bytes, &self.required_demands)?;
        put_strings(&mut bytes, &self.global_keys)?;
        Ok(bytes)
    }

    pub(crate) fn digest(&self) -> Result<String, String> {
        Ok(sha256_hex(&self.canonical_bytes()?))
    }
}

fn validate_lexical_order(
    manifest: &SourceDeclarationManifest<'_>,
    selected_groups: &[(&LockedGroup, u32)],
    statement_starts: &[u32],
    member_starts: &[u32],
    source_ordinal: usize,
) -> Result<(), String> {
    let mut expected_statements = selected_groups
        .iter()
        .map(|(_, start)| *start)
        .collect::<Vec<_>>();
    expected_statements.sort_unstable();
    if statement_starts != expected_statements {
        return Err(format!(
            "source ordinal {source_ordinal} statement order differs from selected group sites"
        ));
    }
    if !member_starts.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(format!(
            "source ordinal {source_ordinal} member order is not strictly lexical"
        ));
    }
    let canonical_function_starts = manifest
        .declarations()
        .iter()
        .filter(|declaration| declaration.kind == DeclarationKind::Function)
        .map(|declaration| declaration.declaration_span.start)
        .collect::<BTreeSet<_>>();
    if member_starts
        .iter()
        .any(|start| !canonical_function_starts.contains(start))
    {
        return Err(format!(
            "source ordinal {source_ordinal} member order contains a non-canonical function site"
        ));
    }
    Ok(())
}

fn validate_slots(group: &LockedGroup, fragments: &[SymbolicFragment]) -> Result<(), String> {
    let mut projected = BTreeSet::new();
    for fragment in fragments {
        match fragment.recipe {
            SymbolicLoweringRecipe::TypeFragment { .. } => {
                projected.insert(SymbolicSlot::Type);
            }
            SymbolicLoweringRecipe::Declaration {
                form: SymbolicDeclarationForm::Variable,
                ..
            } => {
                projected.insert(SymbolicSlot::Value);
            }
            SymbolicLoweringRecipe::Declaration {
                form: SymbolicDeclarationForm::Namespace,
                ..
            } => {
                projected.insert(SymbolicSlot::Value);
                projected.insert(SymbolicSlot::Namespace);
            }
        }
    }
    let locked = group.slots.iter().copied().collect::<BTreeSet<_>>();
    if projected != locked || group.slots.len() != locked.len() {
        return Err(format!(
            "group {:?} slots differ from its canonical declaration forms",
            group.name
        ));
    }
    Ok(())
}

fn validate_graph(groups: &[SymbolicGroup], locked: &LockedSpec) -> Result<(), String> {
    let names = groups
        .iter()
        .map(|group| group.name.as_str())
        .collect::<BTreeSet<_>>();
    for dependency in &locked.dependencies {
        if !names.contains(dependency.from.as_str()) || !names.contains(dependency.to.as_str()) {
            return Err(format!(
                "dependency {:?} -> {:?} leaves the locked group inventory",
                dependency.from, dependency.to
            ));
        }
    }
    for members in locked.owner_sccs.iter().chain(&locked.publication_sccs) {
        if members
            .iter()
            .any(|member| !names.contains(member.as_str()))
        {
            return Err("locked SCC contains an unknown group".to_owned());
        }
    }
    if locked
        .required_demands
        .iter()
        .chain(&locked.global_keys)
        .any(|name| !names.contains(name.as_str()))
    {
        return Err("locked demand/global key contains an unknown group".to_owned());
    }
    Ok(())
}

fn project_recipe(
    manifest: &SourceDeclarationManifest<'_>,
    source: &str,
    group: &LockedGroup,
    statement_start: u32,
) -> Result<SymbolicLoweringRecipe, String> {
    let type_fragments = manifest
        .type_fragments()
        .iter()
        .filter(|fragment| {
            fragment.declaration_span.start == statement_start
                && span_text(
                    source,
                    fragment.binding_span.start,
                    fragment.binding_span.end,
                ) == Some(group.name.as_str())
        })
        .collect::<Vec<_>>();
    if type_fragments.len() == 1 {
        let fragment = type_fragments[0];
        let lexical_events = manifest
            .interface_occurrences(fragment.interface_occurrences.clone())
            .iter()
            .map(|event| SymbolicLexicalEvent {
                kind: match event.kind {
                    SourceInterfaceOccurrenceKind::Header => SymbolicLexicalEventKind::Header,
                    SourceInterfaceOccurrenceKind::Heritage => SymbolicLexicalEventKind::Heritage,
                    SourceInterfaceOccurrenceKind::Member => SymbolicLexicalEventKind::Member,
                },
                start: event.span.start,
                end: event.span.end,
            })
            .collect();
        return Ok(SymbolicLoweringRecipe::TypeFragment {
            form: match fragment.payload.kind() {
                TypeFragmentKind::TypeAlias => SymbolicTypeForm::Alias,
                TypeFragmentKind::Interface => SymbolicTypeForm::Interface,
                TypeFragmentKind::Class => SymbolicTypeForm::Class,
                TypeFragmentKind::Enum => SymbolicTypeForm::Enum,
            },
            declaration_start: fragment.declaration_span.start,
            declaration_end: fragment.declaration_span.end,
            binding_start: fragment.binding_span.start,
            binding_end: fragment.binding_span.end,
            lexical_events,
        });
    }
    if !type_fragments.is_empty() {
        return Err(format!(
            "group {:?} site {statement_start} projects multiple canonical type fragments",
            group.name
        ));
    }

    let declarations = manifest
        .declarations()
        .iter()
        .filter(|declaration| {
            matches!(
                declaration.kind,
                DeclarationKind::Variable | DeclarationKind::Namespace
            ) && declaration.declaration_span.start >= statement_start
                && span_text(
                    source,
                    declaration.binding_span.start,
                    declaration.binding_span.end,
                ) == Some(group.name.as_str())
        })
        .collect::<Vec<_>>();
    if declarations.len() != 1 {
        return Err(format!(
            "group {:?} site {statement_start} projects {} canonical declarations",
            group.name,
            declarations.len()
        ));
    }
    let declaration = declarations[0];
    Ok(SymbolicLoweringRecipe::Declaration {
        form: match declaration.kind {
            DeclarationKind::Variable => SymbolicDeclarationForm::Variable,
            DeclarationKind::Namespace => SymbolicDeclarationForm::Namespace,
            _ => return Err("unsupported canonical declaration form".to_owned()),
        },
        statement_start,
        declaration_start: declaration.declaration_span.start,
        declaration_end: declaration.declaration_span.end,
        binding_start: declaration.binding_span.start,
        binding_end: declaration.binding_span.end,
    })
}

fn span_text(source: &str, start: u32, end: u32) -> Option<&str> {
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?;
    source.get(start..end)
}

pub(crate) fn group_identity(
    profile_identity: &str,
    name: &str,
    slots: &[SymbolicSlot],
    fragments: &[SymbolicFragment],
) -> Result<String, String> {
    let mut framed = Vec::new();
    put_bytes(&mut framed, GROUP_DOMAIN)?;
    put_string(&mut framed, profile_identity)?;
    put_string(&mut framed, name)?;
    put_len(&mut framed, slots.len())?;
    for slot in slots {
        framed.push(slot_tag(*slot));
    }
    put_len(&mut framed, fragments.len())?;
    for fragment in fragments {
        put_usize(&mut framed, fragment.source_ordinal)?;
        framed.extend_from_slice(&fragment.statement_start.to_be_bytes());
        put_string(&mut framed, &fragment.source_sha256)?;
        put_recipe(&mut framed, &fragment.recipe)?;
    }
    Ok(sha256_hex(&framed))
}

fn validate_requested_source_order(locked: &LockedSpec, requested: &[usize]) -> Result<(), String> {
    let expected = locked
        .sources
        .iter()
        .map(|source| source.ordinal)
        .collect::<BTreeSet<_>>();
    let actual = requested.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected || requested.len() != expected.len() {
        return Err(
            "library source enumeration must contain each locked source exactly once".to_owned(),
        );
    }
    Ok(())
}

fn parse_locked_spec() -> Result<LockedSpec, String> {
    let root = LOCKED_SLICE
        .parse::<toml::Value>()
        .map_err(|error| format!("locked slice TOML is invalid: {error}"))?;
    let profile_identity = required_str(&root, "profile_identity")?.to_owned();
    let required_demands = required_string_array(&root, "required_demands")?;
    let global_keys = required_string_array(&root, "global_keys")?;
    let sources = required_table_array(&root, "sources")?
        .iter()
        .map(|row| {
            Ok(LockedSource {
                ordinal: required_usize(row, "ordinal")?,
                name: required_str(row, "name")?.to_owned(),
                sha256: required_str(row, "sha256")?.to_owned(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let groups = required_table_array(&root, "groups")?
        .iter()
        .map(|row| {
            let slots = required_string_array(row, "slots")?
                .iter()
                .map(|slot| match slot.as_str() {
                    "value" => Ok(SymbolicSlot::Value),
                    "type" => Ok(SymbolicSlot::Type),
                    "namespace" => Ok(SymbolicSlot::Namespace),
                    _ => Err(format!("unknown symbolic slot {slot:?}")),
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(LockedGroup {
                name: required_str(row, "name")?.to_owned(),
                slots,
                statement_sites: required_site_array(row, "statement_sites")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut dependencies = required_table_array(&root, "edges")?
        .iter()
        .map(|row| {
            Ok(SymbolicDependency {
                from: required_str(row, "from")?.to_owned(),
                to: required_str(row, "to")?.to_owned(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    dependencies.sort();
    let owner_sccs = parse_sccs(&root, "owner_sccs")?;
    let publication_sccs = parse_sccs(&root, "publication_sccs")?;
    let event_order = required_table_array(&root, "event_order")?
        .iter()
        .map(|row| {
            Ok((
                required_usize(row, "source_ordinal")?,
                (
                    required_u32_array(row, "statement_starts")?,
                    optional_u32_array(row, "member_starts")?,
                ),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    Ok(LockedSpec {
        profile_identity,
        sources,
        groups,
        dependencies,
        owner_sccs,
        publication_sccs,
        required_demands,
        global_keys,
        event_order,
    })
}

fn parse_sccs(root: &toml::Value, key: &str) -> Result<Vec<Vec<String>>, String> {
    let mut sccs = required_table_array(root, key)?
        .iter()
        .map(|row| required_string_array(row, "members"))
        .collect::<Result<Vec<_>, String>>()?;
    for members in &mut sccs {
        members.sort();
    }
    sccs.sort();
    Ok(sccs)
}

fn required_table_array<'a>(
    value: &'a toml::Value,
    key: &str,
) -> Result<&'a [toml::Value], String> {
    value
        .get(key)
        .and_then(toml::Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("locked slice field {key:?} must be an array"))
}

fn required_str<'a>(value: &'a toml::Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("locked slice field {key:?} must be a string"))
}

fn required_usize(value: &toml::Value, key: &str) -> Result<usize, String> {
    let raw = value
        .get(key)
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| format!("locked slice field {key:?} must be an integer"))?;
    usize::try_from(raw).map_err(|_| format!("locked slice field {key:?} is outside usize"))
}

fn required_string_array(value: &toml::Value, key: &str) -> Result<Vec<String>, String> {
    required_table_array(value, key)?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("locked slice field {key:?} contains a non-string"))
        })
        .collect()
}

fn required_site_array(value: &toml::Value, key: &str) -> Result<Vec<(usize, u32)>, String> {
    required_table_array(value, key)?
        .iter()
        .map(|site| {
            let values = site
                .as_array()
                .ok_or_else(|| format!("locked slice field {key:?} contains a non-array site"))?;
            if values.len() != 2 {
                return Err(format!(
                    "locked slice field {key:?} contains a malformed site"
                ));
            }
            let ordinal = values[0]
                .as_integer()
                .ok_or_else(|| format!("locked slice field {key:?} has a non-integer ordinal"))?;
            let start = values[1]
                .as_integer()
                .ok_or_else(|| format!("locked slice field {key:?} has a non-integer start"))?;
            Ok((
                usize::try_from(ordinal)
                    .map_err(|_| format!("locked slice field {key:?} ordinal is outside usize"))?,
                u32::try_from(start)
                    .map_err(|_| format!("locked slice field {key:?} start is outside u32"))?,
            ))
        })
        .collect()
}

fn required_u32_array(value: &toml::Value, key: &str) -> Result<Vec<u32>, String> {
    required_table_array(value, key)?
        .iter()
        .map(|item| {
            let raw = item
                .as_integer()
                .ok_or_else(|| format!("locked slice field {key:?} contains a non-integer"))?;
            u32::try_from(raw)
                .map_err(|_| format!("locked slice field {key:?} contains an integer outside u32"))
        })
        .collect()
}

fn optional_u32_array(value: &toml::Value, key: &str) -> Result<Vec<u32>, String> {
    if value.get(key).is_none() {
        return Ok(Vec::new());
    }
    required_u32_array(value, key)
}

fn put_recipe(bytes: &mut Vec<u8>, recipe: &SymbolicLoweringRecipe) -> Result<(), String> {
    match recipe {
        SymbolicLoweringRecipe::TypeFragment {
            form,
            declaration_start,
            declaration_end,
            binding_start,
            binding_end,
            lexical_events,
        } => {
            bytes.push(0);
            bytes.push(type_form_tag(*form));
            bytes.extend_from_slice(&declaration_start.to_be_bytes());
            bytes.extend_from_slice(&declaration_end.to_be_bytes());
            bytes.extend_from_slice(&binding_start.to_be_bytes());
            bytes.extend_from_slice(&binding_end.to_be_bytes());
            put_len(bytes, lexical_events.len())?;
            for event in lexical_events {
                bytes.push(event_kind_tag(event.kind));
                bytes.extend_from_slice(&event.start.to_be_bytes());
                bytes.extend_from_slice(&event.end.to_be_bytes());
            }
        }
        SymbolicLoweringRecipe::Declaration {
            form,
            statement_start,
            declaration_start,
            declaration_end,
            binding_start,
            binding_end,
        } => {
            bytes.push(1);
            bytes.push(declaration_form_tag(*form));
            bytes.extend_from_slice(&statement_start.to_be_bytes());
            bytes.extend_from_slice(&declaration_start.to_be_bytes());
            bytes.extend_from_slice(&declaration_end.to_be_bytes());
            bytes.extend_from_slice(&binding_start.to_be_bytes());
            bytes.extend_from_slice(&binding_end.to_be_bytes());
        }
    }
    Ok(())
}

fn put_nested_strings(bytes: &mut Vec<u8>, rows: &[Vec<String>]) -> Result<(), String> {
    put_len(bytes, rows.len())?;
    for row in rows {
        put_strings(bytes, row)?;
    }
    Ok(())
}

fn put_strings(bytes: &mut Vec<u8>, values: &[String]) -> Result<(), String> {
    put_len(bytes, values.len())?;
    for value in values {
        put_string(bytes, value)?;
    }
    Ok(())
}

fn put_u32s(bytes: &mut Vec<u8>, values: &[u32]) -> Result<(), String> {
    put_len(bytes, values.len())?;
    for value in values {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    Ok(())
}

fn put_string(bytes: &mut Vec<u8>, value: &str) -> Result<(), String> {
    put_bytes(bytes, value.as_bytes())
}

fn put_bytes(bytes: &mut Vec<u8>, value: &[u8]) -> Result<(), String> {
    put_len(bytes, value.len())?;
    bytes.extend_from_slice(value);
    Ok(())
}

fn put_len(bytes: &mut Vec<u8>, value: usize) -> Result<(), String> {
    put_usize(bytes, value)
}

fn put_usize(bytes: &mut Vec<u8>, value: usize) -> Result<(), String> {
    let value = u64::try_from(value).map_err(|_| "symbolic index length exceeds u64".to_owned())?;
    bytes.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

const fn slot_tag(slot: SymbolicSlot) -> u8 {
    match slot {
        SymbolicSlot::Value => 0,
        SymbolicSlot::Type => 1,
        SymbolicSlot::Namespace => 2,
    }
}

const fn type_form_tag(form: SymbolicTypeForm) -> u8 {
    match form {
        SymbolicTypeForm::Alias => 0,
        SymbolicTypeForm::Interface => 1,
        SymbolicTypeForm::Class => 2,
        SymbolicTypeForm::Enum => 3,
    }
}

const fn declaration_form_tag(form: SymbolicDeclarationForm) -> u8 {
    match form {
        SymbolicDeclarationForm::Variable => 0,
        SymbolicDeclarationForm::Namespace => 1,
    }
}

const fn event_kind_tag(kind: SymbolicLexicalEventKind) -> u8 {
    match kind {
        SymbolicLexicalEventKind::Header => 0,
        SymbolicLexicalEventKind::Heritage => 1,
        SymbolicLexicalEventKind::Member => 2,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
