//! Test-only attribution for the finite WU0C post-cache measurement.

use crate::binder::declaration::{TypeFragmentKind, TypeGroupId};
use crate::binder::Binder;
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use crate::types::repr::TypeParamId;
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{JoinHandle, ThreadId};
use std::time::{Duration, Instant};

const PREFIX: &str = "typokat-wu0c-attribution-v1";
const FAMILY_DOMAIN: &[u8] = b"typokat-wu0c-family-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AttributionMode {
    Off,
    ReporterControl,
    Progress,
    Exact,
}

impl AttributionMode {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "reporter-control" => Some(Self::ReporterControl),
            "progress" => Some(Self::Progress),
            "exact" => Some(Self::Exact),
            _ => None,
        }
    }

    fn render(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::ReporterControl => "reporter-control",
            Self::Progress => "progress",
            Self::Exact => "exact",
        }
    }

    fn captures_semantics(self) -> bool {
        matches!(self, Self::Progress | Self::Exact)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AttributionPhase {
    Bind,
    ReserveFill,
    PublicationValidation,
    StatementCheck,
}

impl AttributionPhase {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "bind" => Some(Self::Bind),
            "reserve_fill" => Some(Self::ReserveFill),
            "publication_validation" => Some(Self::PublicationValidation),
            "statement_check" => Some(Self::StatementCheck),
            _ => None,
        }
    }

    fn render(self) -> &'static str {
        match self {
            Self::Bind => "bind",
            Self::ReserveFill => "reserve_fill",
            Self::PublicationValidation => "publication_validation",
            Self::StatementCheck => "statement_check",
        }
    }
}

impl HeartbeatLine {
    pub(super) fn phase(&self) -> AttributionPhase {
        self.phase
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AttributionLimits {
    pub lines: usize,
    pub eager_keys: usize,
    pub runs: usize,
    pub dictionary_entries: usize,
    pub trace_events: usize,
    pub checkpoint_messages: usize,
    pub checkpoint_bytes: usize,
    pub rendered_line_bytes: usize,
    pub file_bytes: usize,
    pub map_entries: usize,
    pub context_entries: usize,
    pub application_entries: usize,
    pub live_exact_bytes: usize,
    pub terminal_reserve_lines: usize,
    pub terminal_reserve_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AttributionConfig {
    pub process: u8,
    pub universe: Option<u64>,
    pub mode: AttributionMode,
    pub interval_ms: u64,
    pub checkpoint_visits: u64,
    pub evidence_window_ms: u64,
    pub limits: AttributionLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FamilyParticipant {
    file_ordinal: LibraryFileOrdinal,
    declaration_start: u32,
    kind: TypeFragmentKind,
}

impl FamilyParticipant {
    pub(super) const fn new(
        file_ordinal: LibraryFileOrdinal,
        declaration_start: u32,
        kind: TypeFragmentKind,
    ) -> Self {
        Self {
            file_ordinal,
            declaration_start,
            kind,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FamilyToken(String);

impl FamilyToken {
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(super) fn canonical_family_token(participants: &[FamilyParticipant]) -> FamilyToken {
    let mut participants = participants.to_vec();
    participants.sort_by_key(|participant| {
        (
            participant.file_ordinal,
            participant.declaration_start,
            fragment_kind_code(participant.kind),
        )
    });
    let mut input = Vec::with_capacity(8 + FAMILY_DOMAIN.len() + participants.len() * 13);
    input.extend_from_slice(
        &u32::try_from(FAMILY_DOMAIN.len())
            .expect("WU0C family domain length fits u32")
            .to_be_bytes(),
    );
    input.extend_from_slice(FAMILY_DOMAIN);
    input.extend_from_slice(
        &u32::try_from(participants.len())
            .expect("WU0C family participant count fits u32")
            .to_be_bytes(),
    );
    for participant in participants {
        input.extend_from_slice(&9_u32.to_be_bytes());
        input.extend_from_slice(
            &u32::try_from(participant.file_ordinal.index())
                .expect("library file ordinal fits WU0C protocol u32")
                .to_be_bytes(),
        );
        input.extend_from_slice(&participant.declaration_start.to_be_bytes());
        input.push(fragment_kind_code(participant.kind));
    }
    FamilyToken(hex_sha256(&input))
}

fn fragment_kind_code(kind: TypeFragmentKind) -> u8 {
    match kind {
        TypeFragmentKind::Interface => 1,
        TypeFragmentKind::TypeAlias => 2,
        TypeFragmentKind::Class => 3,
    }
}

fn hex_sha256(input: &[u8]) -> String {
    let digest = sha256(input);
    let mut rendered = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut rendered, "{byte:02x}").expect("writing to String cannot fail");
    }
    rendered
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_len = u64::try_from(input.len())
        .expect("WU0C SHA-256 input length fits u64")
        .checked_mul(8)
        .expect("WU0C SHA-256 bit length fits u64");
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = INITIAL;
    let mut words = [0_u32; 64];
    for chunk in padded.chunks_exact(64) {
        for (index, bytes) in chunk.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let first = h
                .wrapping_add(sigma1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let second = sigma0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(first);
            d = c;
            c = b;
            b = a;
            a = first.wrapping_add(second);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut digest = [0_u8; 32];
    for (index, word) in state.into_iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LimitKind {
    Lines,
    EagerKeys,
    Runs,
    DictionaryEntries,
    TraceEvents,
    CheckpointMessages,
    CheckpointBytes,
    RenderedLineBytes,
    FileBytes,
    MapEntries,
    ContextEntries,
    ApplicationEntries,
    LiveExactBytes,
}

impl LimitKind {
    pub(super) const ALL: [Self; 13] = [
        Self::Lines,
        Self::EagerKeys,
        Self::Runs,
        Self::DictionaryEntries,
        Self::TraceEvents,
        Self::CheckpointMessages,
        Self::CheckpointBytes,
        Self::RenderedLineBytes,
        Self::FileBytes,
        Self::MapEntries,
        Self::ContextEntries,
        Self::ApplicationEntries,
        Self::LiveExactBytes,
    ];

    fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|limit| limit.render() == value)
    }

    fn render(self) -> &'static str {
        match self {
            Self::Lines => "lines",
            Self::EagerKeys => "eager_keys",
            Self::Runs => "runs",
            Self::DictionaryEntries => "dictionary_entries",
            Self::TraceEvents => "trace_events",
            Self::CheckpointMessages => "checkpoint_messages",
            Self::CheckpointBytes => "checkpoint_bytes",
            Self::RenderedLineBytes => "rendered_line_bytes",
            Self::FileBytes => "file_bytes",
            Self::MapEntries => "map_entries",
            Self::ContextEntries => "context_entries",
            Self::ApplicationEntries => "application_entries",
            Self::LiveExactBytes => "live_exact_bytes",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SessionLine {
    raw: String,
    process: usize,
    mode: AttributionMode,
    universe: Option<u64>,
    session_sha256: String,
    binary_sha256: String,
    host_sha256: String,
    workload_profile_sha256: String,
    capabilities: String,
    interval_ms: u64,
    checkpoint_visits: u64,
    evidence_window_ms: u64,
    limits: AttributionLimits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HeartbeatLine {
    raw: String,
    phase: AttributionPhase,
    pub reporter_elapsed_us: u64,
    pub checkpoint_elapsed_us: u64,
    pub reserve_fill_us: u64,
    active_family_sha256: Option<String>,
    pub active_elapsed_us: u64,
    coverage_lost: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EagerLine {
    raw: String,
    pub family_sha256: String,
    pub calls: u64,
    pub hits: u64,
    pub misses: u64,
    pub clean: u64,
    pub tainted: u64,
    pub active: u64,
    pub completed: u64,
    pub completed_us: u64,
    pub active_us: u64,
}

impl EagerLine {
    pub(super) fn arithmetic_is_exact(&self) -> bool {
        self.calls == self.hits + self.misses + self.active
            && self.misses == self.clean + self.tainted
            && self.completed == self.hits + self.misses
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RunLine {
    raw: String,
    run: u64,
    pub family_sha256: String,
    checkpoint: bool,
    pub started: u64,
    pub completed: u64,
    pub active: u64,
    pub visits: u64,
    pub memo_hits: u64,
    pub cycle_reentries: u64,
    pub tainted_ancestors: u64,
}

impl RunLine {
    pub(super) fn arithmetic_is_exact(&self) -> bool {
        self.started == self.completed + self.active && self.active <= 1
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StateLine {
    raw: String,
    run: u64,
    state: u64,
    type_id: u32,
    context: Vec<u32>,
    map: Vec<(u32, u32)>,
    map_sha256: String,
    application: String,
    application_sha256: String,
    pub saturated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventAction {
    Enter,
    Outcome,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventDisposition {
    Clean,
    CompletedMemoHit,
    RawCycleReentry,
    Tainted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EventLine {
    raw: String,
    run: u64,
    event: u64,
    action: EventAction,
    visit: u64,
    parent: Option<u64>,
    state: u64,
    disposition: Option<EventDisposition>,
    at_us: u64,
}

impl EventLine {
    pub(super) fn is_enter(&self) -> bool {
        self.action == EventAction::Enter
    }

    pub(super) fn is_exit(&self) -> bool {
        self.action == EventAction::Exit
    }

    pub(super) fn is_raw_cycle_reentry(&self) -> bool {
        self.disposition == Some(EventDisposition::RawCycleReentry)
    }

    pub(super) fn is_tainted(&self) -> bool {
        self.disposition == Some(EventDisposition::Tainted)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InvalidLine {
    raw: String,
    pub limit: LimitKind,
    sink_write_failure: bool,
}

impl InvalidLine {
    pub(super) fn is_sink_write_failure(&self) -> bool {
        self.sink_write_failure
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FinishLine {
    raw: String,
    status_complete: bool,
    reporter_elapsed_us: u64,
    checkpoint_elapsed_us: u64,
    coverage_lost: bool,
    lines: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AttributionLine {
    Session(SessionLine),
    Heartbeat(HeartbeatLine),
    Eager(EagerLine),
    Run(RunLine),
    State(StateLine),
    Event(EventLine),
    Invalid(InvalidLine),
    Finish(FinishLine),
}

impl AttributionLine {
    pub(super) fn render(&self) -> String {
        match self {
            Self::Session(line) => line.raw.clone(),
            Self::Heartbeat(line) => line.raw.clone(),
            Self::Eager(line) => line.raw.clone(),
            Self::Run(line) => line.raw.clone(),
            Self::State(line) => line.raw.clone(),
            Self::Event(line) => line.raw.clone(),
            Self::Invalid(line) => line.raw.clone(),
            Self::Finish(line) => line.raw.clone(),
        }
    }
}

#[derive(Clone, Copy)]
struct CommonLine {
    seq: u64,
    process: usize,
    mode: AttributionMode,
    universe: Option<u64>,
}

fn fields(line: &str) -> Result<Vec<(&str, &str)>, String> {
    let mut words = line.split_ascii_whitespace();
    if words.next() != Some(PREFIX) {
        return Err("wrong attribution prefix".to_owned());
    }
    words
        .map(|word| {
            let mut parts = word.split('=');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default();
            if key.is_empty() || value.is_empty() || parts.next().is_some() {
                return Err("malformed attribution field".to_owned());
            }
            Ok((key, value))
        })
        .collect()
}

fn expect_keys(fields: &[(&str, &str)], keys: &[&str]) -> Result<(), String> {
    if fields.len() != keys.len()
        || fields
            .iter()
            .zip(keys)
            .any(|((actual, _), expected)| actual != expected)
    {
        return Err("attribution fields do not match the strict v1 schema".to_owned());
    }
    Ok(())
}

fn number<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, String> {
    value.parse().map_err(|_| format!("invalid numeric {name}"))
}

fn bit(value: &str, name: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!("invalid boolean {name}")),
    }
}

fn opaque(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn common(fields: &[(&str, &str)]) -> Result<CommonLine, String> {
    if fields.len() < 5 || fields[0].0 != "kind" || fields[1].0 != "seq" {
        return Err("missing common attribution fields".to_owned());
    }
    if fields[2].0 != "process" || fields[3].0 != "mode" {
        return Err("missing common attribution identity".to_owned());
    }
    let mode =
        AttributionMode::parse(fields[3].1).ok_or_else(|| "invalid attribution mode".to_owned())?;
    let (universe, next) = if fields.get(4).is_some_and(|field| field.0 == "universe") {
        (Some(number(fields[4].1, "universe")?), 5)
    } else {
        (None, 4)
    };
    if (mode == AttributionMode::Exact) != universe.is_some() || next != fields.len().min(next) {
        return Err("mode/universe shape mismatch".to_owned());
    }
    Ok(CommonLine {
        seq: number(fields[1].1, "sequence")?,
        process: number(fields[2].1, "process")?,
        mode,
        universe,
    })
}

fn parse_csv_numbers(value: &str) -> Result<Vec<u32>, String> {
    if value == "-" {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|part| number(part, "list entry"))
        .collect()
}

fn parse_map(value: &str) -> Result<Vec<(u32, u32)>, String> {
    if value == "-" {
        return Ok(Vec::new());
    }
    let parsed = value
        .split(',')
        .map(|part| {
            let Some((left, right)) = part.split_once(':') else {
                return Err("invalid map entry".to_owned());
            };
            if right.contains(':') {
                return Err("invalid map entry".to_owned());
            }
            Ok((number(left, "map parameter")?, number(right, "map type")?))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parsed.windows(2).any(|window| window[0].0 >= window[1].0) {
        return Err("map entries are not canonical".to_owned());
    }
    Ok(parsed)
}

pub(super) fn parse_attribution_line(line: &str) -> Result<AttributionLine, String> {
    let fields = fields(line)?;
    let common = common(&fields)?;
    let prefix_len = if common.universe.is_some() { 5 } else { 4 };
    let rest = &fields[prefix_len..];
    match fields[0].1 {
        "session" => parse_session(line, common, rest),
        "heartbeat" => parse_heartbeat(line, common, rest),
        "eager" => parse_eager(line, common, rest),
        "run" => parse_run(line, common, rest),
        "state" => parse_state(line, common, rest),
        "event" => parse_event(line, common, rest),
        "invalid" => parse_invalid(line, common, rest),
        "finish" => parse_finish(line, common, rest),
        _ => Err("unknown attribution line kind".to_owned()),
    }
}

fn parse_session(
    raw: &str,
    common: CommonLine,
    rest: &[(&str, &str)],
) -> Result<AttributionLine, String> {
    const KEYS: [&str; 22] = [
        "session_sha256",
        "binary_sha256",
        "host_sha256",
        "workload_profile_sha256",
        "capabilities",
        "interval_ms",
        "checkpoint_visits",
        "evidence_window_ms",
        "max_lines",
        "max_eager_keys",
        "max_runs",
        "max_dictionary_entries",
        "max_trace_events",
        "max_checkpoint_messages",
        "max_checkpoint_bytes",
        "max_line_bytes",
        "max_file_bytes",
        "max_map_entries",
        "max_context_entries",
        "max_application_entries",
        "max_live_exact_bytes",
        "terminal_reserve_lines",
    ];
    let mut expected = KEYS.to_vec();
    expected.push("terminal_reserve_bytes");
    expect_keys(rest, &expected)?;
    if common.seq != 0 || rest[..4].iter().any(|(_, value)| !opaque(value)) {
        return Err("invalid session identity".to_owned());
    }
    let limits = AttributionLimits {
        lines: number(rest[8].1, "max lines")?,
        eager_keys: number(rest[9].1, "max eager keys")?,
        runs: number(rest[10].1, "max runs")?,
        dictionary_entries: number(rest[11].1, "max dictionary entries")?,
        trace_events: number(rest[12].1, "max trace events")?,
        checkpoint_messages: number(rest[13].1, "max checkpoint messages")?,
        checkpoint_bytes: number(rest[14].1, "max checkpoint bytes")?,
        rendered_line_bytes: number(rest[15].1, "max line bytes")?,
        file_bytes: number(rest[16].1, "max file bytes")?,
        map_entries: number(rest[17].1, "max map entries")?,
        context_entries: number(rest[18].1, "max context entries")?,
        application_entries: number(rest[19].1, "max application entries")?,
        live_exact_bytes: number(rest[20].1, "max live exact bytes")?,
        terminal_reserve_lines: number(rest[21].1, "terminal reserve lines")?,
        terminal_reserve_bytes: number(rest[22].1, "terminal reserve bytes")?,
    };
    if limits.terminal_reserve_lines < 2
        || limits.terminal_reserve_bytes < 2 * limits.rendered_line_bytes
    {
        return Err("terminal capacity is not reserved".to_owned());
    }
    Ok(AttributionLine::Session(SessionLine {
        raw: raw.to_owned(),
        process: common.process,
        mode: common.mode,
        universe: common.universe,
        session_sha256: rest[0].1.to_owned(),
        binary_sha256: rest[1].1.to_owned(),
        host_sha256: rest[2].1.to_owned(),
        workload_profile_sha256: rest[3].1.to_owned(),
        capabilities: rest[4].1.to_owned(),
        interval_ms: number(rest[5].1, "interval")?,
        checkpoint_visits: number(rest[6].1, "checkpoint visits")?,
        evidence_window_ms: number(rest[7].1, "evidence window")?,
        limits,
    }))
}

fn parse_heartbeat(
    raw: &str,
    _common: CommonLine,
    rest: &[(&str, &str)],
) -> Result<AttributionLine, String> {
    expect_keys(
        rest,
        &[
            "phase",
            "reporter_elapsed_us",
            "checkpoint_elapsed_us",
            "reserve_fill_us",
            "active_family_sha256",
            "active_elapsed_us",
            "coverage_lost",
        ],
    )?;
    let active_family_sha256 = match rest[4].1 {
        "-" => None,
        value if opaque(value) => Some(value.to_owned()),
        _ => return Err("invalid active family token".to_owned()),
    };
    let active_elapsed_us = number(rest[5].1, "active elapsed")?;
    if active_family_sha256.is_none() && active_elapsed_us != 0 {
        return Err("inactive heartbeat has active elapsed".to_owned());
    }
    Ok(AttributionLine::Heartbeat(HeartbeatLine {
        raw: raw.to_owned(),
        phase: AttributionPhase::parse(rest[0].1)
            .ok_or_else(|| "invalid attribution phase".to_owned())?,
        reporter_elapsed_us: number(rest[1].1, "reporter elapsed")?,
        checkpoint_elapsed_us: number(rest[2].1, "checkpoint elapsed")?,
        reserve_fill_us: number(rest[3].1, "reserve/fill elapsed")?,
        active_family_sha256,
        active_elapsed_us,
        coverage_lost: bit(rest[6].1, "coverage lost")?,
    }))
}

fn parse_eager(
    raw: &str,
    _common: CommonLine,
    rest: &[(&str, &str)],
) -> Result<AttributionLine, String> {
    expect_keys(
        rest,
        &[
            "family_sha256",
            "calls",
            "hits",
            "misses",
            "clean",
            "tainted",
            "active",
            "completed",
            "completed_us",
            "active_us",
        ],
    )?;
    if !opaque(rest[0].1) {
        return Err("invalid family token".to_owned());
    }
    let parsed = EagerLine {
        raw: raw.to_owned(),
        family_sha256: rest[0].1.to_owned(),
        calls: number(rest[1].1, "calls")?,
        hits: number(rest[2].1, "hits")?,
        misses: number(rest[3].1, "misses")?,
        clean: number(rest[4].1, "clean misses")?,
        tainted: number(rest[5].1, "tainted misses")?,
        active: number(rest[6].1, "active calls")?,
        completed: number(rest[7].1, "completed calls")?,
        completed_us: number(rest[8].1, "completed elapsed")?,
        active_us: number(rest[9].1, "active elapsed")?,
    };
    if !parsed.arithmetic_is_exact() || parsed.active > 1 {
        return Err("invalid eager arithmetic".to_owned());
    }
    Ok(AttributionLine::Eager(parsed))
}

fn parse_run(
    raw: &str,
    _common: CommonLine,
    rest: &[(&str, &str)],
) -> Result<AttributionLine, String> {
    expect_keys(
        rest,
        &[
            "run",
            "family_sha256",
            "checkpoint",
            "started",
            "completed",
            "active",
            "visits",
            "memo_hits",
            "cycle_reentries",
            "tainted_ancestors",
        ],
    )?;
    if !opaque(rest[1].1) {
        return Err("invalid run family token".to_owned());
    }
    let parsed = RunLine {
        raw: raw.to_owned(),
        run: number(rest[0].1, "run")?,
        family_sha256: rest[1].1.to_owned(),
        checkpoint: bit(rest[2].1, "checkpoint")?,
        started: number(rest[3].1, "started")?,
        completed: number(rest[4].1, "completed")?,
        active: number(rest[5].1, "active")?,
        visits: number(rest[6].1, "visits")?,
        memo_hits: number(rest[7].1, "memo hits")?,
        cycle_reentries: number(rest[8].1, "cycle reentries")?,
        tainted_ancestors: number(rest[9].1, "tainted ancestors")?,
    };
    if !parsed.arithmetic_is_exact()
        || parsed.memo_hits > parsed.visits
        || parsed.cycle_reentries > parsed.visits
        || parsed.tainted_ancestors > parsed.visits
    {
        return Err("invalid run arithmetic".to_owned());
    }
    Ok(AttributionLine::Run(parsed))
}

fn parse_state(
    raw: &str,
    common: CommonLine,
    rest: &[(&str, &str)],
) -> Result<AttributionLine, String> {
    if common.mode != AttributionMode::Exact {
        return Err("state line requires exact mode".to_owned());
    }
    expect_keys(
        rest,
        &[
            "run",
            "state",
            "type_id",
            "context",
            "map",
            "map_sha256",
            "application",
            "application_sha256",
            "saturated",
        ],
    )?;
    let context = parse_csv_numbers(rest[3].1)?;
    if context.windows(2).any(|window| window[0] >= window[1]) {
        return Err("context is not canonical".to_owned());
    }
    let map = parse_map(rest[4].1)?;
    if !opaque(rest[5].1) || !opaque(rest[7].1) {
        return Err("invalid exact-state digest".to_owned());
    }
    Ok(AttributionLine::State(StateLine {
        raw: raw.to_owned(),
        run: number(rest[0].1, "run")?,
        state: number(rest[1].1, "state")?,
        type_id: number(rest[2].1, "type id")?,
        context,
        map,
        map_sha256: rest[5].1.to_owned(),
        application: rest[6].1.to_owned(),
        application_sha256: rest[7].1.to_owned(),
        saturated: bit(rest[8].1, "saturated")?,
    }))
}

fn parse_event(
    raw: &str,
    common: CommonLine,
    rest: &[(&str, &str)],
) -> Result<AttributionLine, String> {
    if common.mode != AttributionMode::Exact {
        return Err("event line requires exact mode".to_owned());
    }
    let action_value = rest.get(2).map(|field| field.1).unwrap_or_default();
    let action = match action_value {
        "enter" => EventAction::Enter,
        "outcome" => EventAction::Outcome,
        "exit" => EventAction::Exit,
        _ => return Err("invalid event action".to_owned()),
    };
    let expected = if action == EventAction::Outcome {
        vec![
            "run",
            "event",
            "action",
            "visit",
            "parent",
            "state",
            "disposition",
            "at_us",
        ]
    } else {
        vec![
            "run", "event", "action", "visit", "parent", "state", "at_us",
        ]
    };
    expect_keys(rest, &expected)?;
    let parent = match rest[4].1 {
        "none" => None,
        value => Some(number(value, "parent visit")?),
    };
    let disposition = if action == EventAction::Outcome {
        Some(match rest[6].1 {
            "clean" => EventDisposition::Clean,
            "completed_memo_hit" => EventDisposition::CompletedMemoHit,
            "raw_cycle_reentry" => EventDisposition::RawCycleReentry,
            "tainted" => EventDisposition::Tainted,
            _ => return Err("invalid event disposition".to_owned()),
        })
    } else {
        None
    };
    let at_index = if action == EventAction::Outcome { 7 } else { 6 };
    Ok(AttributionLine::Event(EventLine {
        raw: raw.to_owned(),
        run: number(rest[0].1, "run")?,
        event: number(rest[1].1, "event")?,
        action,
        visit: number(rest[3].1, "visit")?,
        parent,
        state: number(rest[5].1, "state")?,
        disposition,
        at_us: number(rest[at_index].1, "event timestamp")?,
    }))
}

fn parse_invalid(
    raw: &str,
    _common: CommonLine,
    rest: &[(&str, &str)],
) -> Result<AttributionLine, String> {
    if rest.len() == 1 {
        expect_keys(rest, &["limit"])?;
    } else {
        expect_keys(rest, &["limit", "reason"])?;
        if rest[1].1 != "sink_write_failure" {
            return Err("invalid attribution failure reason".to_owned());
        }
    }
    let limit = LimitKind::parse(rest[0].1).ok_or_else(|| "invalid limit kind".to_owned())?;
    Ok(AttributionLine::Invalid(InvalidLine {
        raw: raw.to_owned(),
        limit,
        sink_write_failure: rest.len() == 2,
    }))
}

fn parse_finish(
    raw: &str,
    _common: CommonLine,
    rest: &[(&str, &str)],
) -> Result<AttributionLine, String> {
    expect_keys(
        rest,
        &[
            "status",
            "reporter_elapsed_us",
            "checkpoint_elapsed_us",
            "coverage_lost",
            "lines",
        ],
    )?;
    if rest[0].1 != "complete" {
        return Err("invalid finish status".to_owned());
    }
    Ok(AttributionLine::Finish(FinishLine {
        raw: raw.to_owned(),
        status_complete: true,
        reporter_elapsed_us: number(rest[1].1, "reporter elapsed")?,
        checkpoint_elapsed_us: number(rest[2].1, "checkpoint elapsed")?,
        coverage_lost: bit(rest[3].1, "coverage lost")?,
        lines: number(rest[4].1, "line count")?,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Termination {
    Normal,
    Deadline { elapsed_us: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AttributionSample {
    pub family_sha256: String,
    pub reserve_fill_us: u64,
    pub family_exclusive_us: u64,
    pub visits: u64,
    pub family_visits: u64,
    pub family_cycle_tainted: bool,
    pub exact_repeats: u64,
    pub exact_visits: u64,
}

#[derive(Clone, Debug)]
pub(super) struct ReplayInput {
    states: Vec<StateLine>,
    events: Vec<EventLine>,
}

#[derive(Clone, Debug)]
pub(super) struct ValidatedSessionEvidence {
    process: usize,
    session_identity_sha256: String,
    binary_identity_sha256: String,
    host_identity_sha256: String,
    workload_profile_identity_sha256: String,
    termination: Termination,
    checkpoint_elapsed_us: u64,
    sample: AttributionSample,
    replay: ReplayInput,
}

impl ValidatedSessionEvidence {
    pub(super) fn process(&self) -> usize {
        self.process
    }

    pub(super) fn session_identity_sha256(&self) -> &str {
        &self.session_identity_sha256
    }

    pub(super) fn binary_identity_sha256(&self) -> &str {
        &self.binary_identity_sha256
    }

    pub(super) fn host_identity_sha256(&self) -> &str {
        &self.host_identity_sha256
    }

    pub(super) fn workload_profile_identity_sha256(&self) -> &str {
        &self.workload_profile_identity_sha256
    }

    pub(super) fn termination(&self) -> Termination {
        self.termination
    }

    pub(super) fn checkpoint_elapsed_us(&self) -> u64 {
        self.checkpoint_elapsed_us
    }

    pub(super) fn sample(&self) -> &AttributionSample {
        &self.sample
    }

    pub(super) fn replay_input(&self) -> &ReplayInput {
        &self.replay
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReplayResult {
    pub repeated_visits: u64,
    pub removable_repeated_visits: u64,
}

pub(super) fn replay_exact_trace(input: &ReplayInput) -> Result<ReplayResult, String> {
    validate_replay(input)
}

fn validate_replay(input: &ReplayInput) -> Result<ReplayResult, String> {
    let mut next_state_by_run = BTreeMap::new();
    let mut dictionary_keys = BTreeSet::new();
    for state in &input.states {
        let next = next_state_by_run.entry(state.run).or_insert(1_u64);
        if state.state != *next
            || !dictionary_keys.insert((
                state.run,
                state.type_id,
                state.context.clone(),
                state.map.clone(),
                state.application.clone(),
            ))
        {
            return Err("non-canonical exact dictionary".to_owned());
        }
        *next += 1;
    }
    let states = input
        .states
        .iter()
        .map(|state| ((state.run, state.state), state))
        .collect::<BTreeMap<_, _>>();
    if states.len() != input.states.len() {
        return Err("duplicate exact dictionary state".to_owned());
    }
    let mut events_by_run: BTreeMap<u64, Vec<&EventLine>> = BTreeMap::new();
    for event in &input.events {
        if !states.contains_key(&(event.run, event.state)) {
            return Err("trace references a missing dictionary state".to_owned());
        }
        events_by_run.entry(event.run).or_default().push(event);
    }

    let mut seen_states = BTreeSet::new();
    let mut repeated_visits = 0_u64;
    let mut removable_repeated_visits = 0_u64;
    for (run, events) in events_by_run {
        let mut stack: Vec<(u64, Option<u64>, u64, bool, u32)> = Vec::new();
        let mut seen_visits = BTreeSet::new();
        let mut last_at = None;
        for (index, event) in events.into_iter().enumerate() {
            let expected_event = u64::try_from(index + 1).expect("trace event index fits u64");
            if event.run != run || event.event != expected_event {
                return Err("non-contiguous per-run event ordinal".to_owned());
            }
            if last_at.is_some_and(|last| event.at_us < last) {
                return Err("non-monotonic exact timestamp".to_owned());
            }
            last_at = Some(event.at_us);
            match event.action {
                EventAction::Enter => {
                    let expected_parent = stack.last().map(|frame| frame.0);
                    if event.parent != expected_parent || !seen_visits.insert(event.visit) {
                        return Err("invalid exact enter nesting".to_owned());
                    }
                    let state = states
                        .get(&(run, event.state))
                        .expect("dictionary presence checked above");
                    let state_key = (
                        state.type_id,
                        state.context.clone(),
                        state.map.clone(),
                        state.application.clone(),
                    );
                    if !seen_states.insert(state_key) {
                        repeated_visits += 1;
                    }
                    stack.push((event.visit, event.parent, event.state, false, state.type_id));
                }
                EventAction::Outcome => {
                    let Some(frame_index) = stack.len().checked_sub(1) else {
                        return Err("outcome without enter".to_owned());
                    };
                    let frame = stack[frame_index];
                    if frame.0 != event.visit
                        || frame.1 != event.parent
                        || frame.2 != event.state
                        || frame.3
                        || event.disposition.is_none()
                    {
                        return Err("invalid exact outcome".to_owned());
                    }
                    if event.disposition == Some(EventDisposition::RawCycleReentry) {
                        if event.parent.is_none()
                            || !stack[..frame_index]
                                .iter()
                                .any(|ancestor| ancestor.4 == frame.4)
                        {
                            return Err(
                                "raw cycle re-entry has no matching active ancestor".to_owned()
                            );
                        }
                        removable_repeated_visits += 1;
                    }
                    stack[frame_index].3 = true;
                }
                EventAction::Exit => {
                    let Some(frame) = stack.pop() else {
                        return Err("exit without enter".to_owned());
                    };
                    if frame.0 != event.visit
                        || frame.1 != event.parent
                        || frame.2 != event.state
                        || !frame.3
                    {
                        return Err("invalid exact exit".to_owned());
                    }
                }
            }
        }
        if !stack.is_empty() {
            return Err("unfinished exact trace stack".to_owned());
        }
    }
    Ok(ReplayResult {
        repeated_visits,
        removable_repeated_visits,
    })
}

pub(super) fn validate_session_evidence(
    lines: &[String],
    termination: Termination,
) -> Result<ValidatedSessionEvidence, String> {
    if lines.is_empty() {
        return Err("empty attribution session".to_owned());
    }
    let mut parsed = Vec::with_capacity(lines.len());
    let mut common_rows = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let raw_fields = fields(line)?;
        let row = common(&raw_fields)?;
        if row.seq != u64::try_from(index).expect("session line index fits u64") {
            return Err("non-contiguous session sequence".to_owned());
        }
        common_rows.push(row);
        parsed.push(parse_attribution_line(line)?);
    }
    let AttributionLine::Session(session) = &parsed[0] else {
        return Err("session header must be first".to_owned());
    };
    if session.mode != AttributionMode::Exact
        || session.universe.is_none()
        || session.interval_ms != 250
        || session.checkpoint_visits != 4_096
        || session.evidence_window_ms != 5_000
        || session.capabilities != "progress,eager,substitution,exact_trace"
        || lines.len() > session.limits.lines
        || lines
            .iter()
            .any(|line| line.len() > session.limits.rendered_line_bytes)
        || lines.iter().map(|line| line.len() + 1).sum::<usize>() > session.limits.file_bytes
    {
        return Err("session header is not admissible exact evidence".to_owned());
    }
    if common_rows.iter().any(|row| {
        row.process != session.process
            || row.mode != session.mode
            || row.universe != session.universe
    }) {
        return Err("session identity changes between lines".to_owned());
    }
    if parsed
        .iter()
        .any(|line| matches!(line, AttributionLine::Invalid(_)))
    {
        return Err("session contains an invalid/saturation line".to_owned());
    }

    let evidence_end = match termination {
        Termination::Normal => {
            let Some(AttributionLine::Finish(finish)) = parsed.last() else {
                return Err("normal session lacks a clean finish".to_owned());
            };
            if !finish.status_complete
                || finish.coverage_lost
                || finish.lines != u64::try_from(lines.len()).expect("line count fits u64")
                || finish.checkpoint_elapsed_us > finish.reporter_elapsed_us
            {
                return Err("normal finish is not clean".to_owned());
            }
            parsed.len() - 1
        }
        Termination::Deadline { elapsed_us } => {
            if elapsed_us != 5_000_000
                || parsed
                    .iter()
                    .any(|line| matches!(line, AttributionLine::Finish(_)))
            {
                return Err("deadline termination is not the pinned five-second kill".to_owned());
            }
            parsed
                .iter()
                .rposition(|line| {
                    matches!(line, AttributionLine::Heartbeat(heartbeat)
                        if !heartbeat.coverage_lost
                            && heartbeat.reporter_elapsed_us <= elapsed_us
                            && heartbeat.checkpoint_elapsed_us <= elapsed_us
                            && elapsed_us - heartbeat.checkpoint_elapsed_us <= 250_000
                            && heartbeat.reserve_fill_us > 0
                            && heartbeat.reserve_fill_us <= heartbeat.checkpoint_elapsed_us)
                })
                .ok_or_else(|| "deadline session lacks a recent clean heartbeat".to_owned())?
                + 1
        }
    };

    let evidence = &parsed[..evidence_end];
    let heartbeat = evidence
        .iter()
        .rev()
        .find_map(|line| match line {
            AttributionLine::Heartbeat(heartbeat) => Some(heartbeat),
            _ => None,
        })
        .ok_or_else(|| "session lacks heartbeat evidence".to_owned())?;
    if heartbeat.phase != AttributionPhase::ReserveFill
        || heartbeat.coverage_lost
        || heartbeat.reserve_fill_us == 0
        || heartbeat.reserve_fill_us > heartbeat.checkpoint_elapsed_us
    {
        return Err("heartbeat is outside reserve/fill evidence".to_owned());
    }

    let states = evidence
        .iter()
        .filter_map(|line| match line {
            AttributionLine::State(state) => Some(state.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let events = evidence
        .iter()
        .filter_map(|line| match line {
            AttributionLine::Event(event) => Some(event.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if states.len() > session.limits.dictionary_entries
        || events.len() > session.limits.trace_events
        || states.iter().any(|state| {
            state.saturated
                || state.map.len() > session.limits.map_entries
                || state.context.len() > session.limits.context_entries
                || usize::from(state.application != "-") > session.limits.application_entries
                || state.map_sha256 != hex_sha256(render_map(&state.map).as_bytes())
                || state.application_sha256 != hex_sha256(state.application.as_bytes())
        })
    {
        return Err("exact state exceeded a finite limit".to_owned());
    }
    let live_exact_bytes = states.iter().map(|state| state.raw.len()).sum::<usize>()
        + events.iter().map(|event| event.raw.len()).sum::<usize>();
    if live_exact_bytes > session.limits.live_exact_bytes {
        return Err("live exact bytes exceeded".to_owned());
    }
    let replay = ReplayInput { states, events };
    let replay_result = validate_replay(&replay)?;

    let mut latest_runs = BTreeMap::new();
    for run in evidence.iter().filter_map(|line| match line {
        AttributionLine::Run(run) => Some(run),
        _ => None,
    }) {
        if run.run == 0 {
            return Err("zero substitution run identity".to_owned());
        }
        if let Some(previous) = latest_runs.insert(run.run, run) {
            if previous.family_sha256 != run.family_sha256
                || previous.visits > run.visits
                || previous.memo_hits > run.memo_hits
                || previous.cycle_reentries > run.cycle_reentries
                || previous.tainted_ancestors > run.tainted_ancestors
                || previous.completed > run.completed
            {
                return Err("non-cumulative substitution checkpoint".to_owned());
            }
        }
    }
    if latest_runs.is_empty() || latest_runs.len() > session.limits.runs {
        return Err("session lacks bounded substitution runs".to_owned());
    }
    if replay
        .states
        .iter()
        .any(|state| !latest_runs.contains_key(&state.run))
        || replay
            .events
            .iter()
            .any(|event| !latest_runs.contains_key(&event.run))
    {
        return Err("exact trace has no corresponding run checkpoint".to_owned());
    }
    for run in latest_runs.values() {
        let run_events = replay
            .events
            .iter()
            .filter(|event| event.run == run.run)
            .collect::<Vec<_>>();
        let visits = run_events
            .iter()
            .filter(|event| event.action == EventAction::Enter)
            .count();
        let memo_hits = run_events
            .iter()
            .filter(|event| event.disposition == Some(EventDisposition::CompletedMemoHit))
            .count();
        let cycle_reentries = run_events
            .iter()
            .filter(|event| event.disposition == Some(EventDisposition::RawCycleReentry))
            .count();
        let tainted_ancestors = run_events
            .iter()
            .filter(|event| event.disposition == Some(EventDisposition::Tainted))
            .count();
        if run.visits != u64::try_from(visits).expect("bounded visit count fits u64")
            || run.memo_hits != u64::try_from(memo_hits).expect("bounded memo count fits u64")
            || run.cycle_reentries
                != u64::try_from(cycle_reentries).expect("bounded cycle count fits u64")
            || run.tainted_ancestors
                != u64::try_from(tainted_ancestors).expect("bounded taint count fits u64")
        {
            return Err("run counters differ from the exact event trace".to_owned());
        }
    }
    let runs = latest_runs.values().copied().collect::<Vec<_>>();
    let family = heartbeat
        .active_family_sha256
        .clone()
        .or_else(|| runs.first().map(|run| run.family_sha256.clone()))
        .ok_or_else(|| "session lacks a family attribution".to_owned())?;
    let family_runs = runs
        .iter()
        .copied()
        .filter(|run| run.family_sha256 == family)
        .collect::<Vec<_>>();
    if family_runs.is_empty() || runs.iter().any(|run| !run.checkpoint) {
        return Err("session lacks cumulative run checkpoints".to_owned());
    }
    let visits = runs.iter().map(|run| run.visits).sum();
    let family_visits = family_runs.iter().map(|run| run.visits).sum();
    let family_cycle_tainted = family_runs
        .iter()
        .any(|run| run.cycle_reentries > 0 && run.tainted_ancestors > 0);

    let eager = evidence
        .iter()
        .filter_map(|line| match line {
            AttributionLine::Eager(eager) if eager.family_sha256 == family => Some(eager),
            _ => None,
        })
        .next_back()
        .ok_or_else(|| "session lacks eager family timing".to_owned())?;
    let family_exclusive_us = eager
        .completed_us
        .checked_add(eager.active_us)
        .ok_or_else(|| "family elapsed overflow".to_owned())?;
    if eager.active == 0 && eager.active_us != 0
        || eager.active == 1 && heartbeat.active_family_sha256.as_deref() != Some(&family)
        || heartbeat.active_elapsed_us != eager.active_us
    {
        return Err("active eager/heartbeat evidence differs".to_owned());
    }

    Ok(ValidatedSessionEvidence {
        process: session.process,
        session_identity_sha256: session.session_sha256.clone(),
        binary_identity_sha256: session.binary_sha256.clone(),
        host_identity_sha256: session.host_sha256.clone(),
        workload_profile_identity_sha256: session.workload_profile_sha256.clone(),
        termination,
        checkpoint_elapsed_us: heartbeat.checkpoint_elapsed_us,
        sample: AttributionSample {
            family_sha256: family,
            reserve_fill_us: heartbeat.reserve_fill_us,
            family_exclusive_us,
            visits,
            family_visits,
            family_cycle_tainted,
            exact_repeats: replay_result.repeated_visits,
            exact_visits: u64::try_from(
                replay
                    .events
                    .iter()
                    .filter(|event| event.is_enter())
                    .count(),
            )
            .expect("exact visit count fits u64"),
        },
        replay,
    })
}

#[derive(Clone, Default)]
pub(super) struct AttributionTestClock {
    now_us: Arc<AtomicU64>,
}

impl AttributionTestClock {
    pub(super) fn advance_us(&self, elapsed: u64) {
        self.now_us.fetch_add(elapsed, Ordering::Relaxed);
    }
}

#[derive(Clone, Default)]
pub(super) struct AttributionTestSink {
    inner: Arc<Mutex<TestSinkState>>,
}

#[derive(Default)]
struct TestSinkState {
    batches: Vec<Vec<String>>,
    flushes: usize,
    fail_next_write: bool,
    write_failures: usize,
}

impl AttributionTestSink {
    pub(super) fn rendered_lines(&self) -> Vec<String> {
        self.inner
            .lock()
            .expect("WU0C test sink mutex is not poisoned")
            .batches
            .iter()
            .flatten()
            .cloned()
            .collect()
    }

    pub(super) fn parsed_lines(&self) -> Vec<AttributionLine> {
        self.rendered_lines()
            .into_iter()
            .map(|line| parse_attribution_line(&line).expect("reporter emits strict v1 lines"))
            .collect()
    }

    pub(super) fn batch_count(&self) -> usize {
        self.inner
            .lock()
            .expect("WU0C test sink mutex is not poisoned")
            .batches
            .len()
    }

    pub(super) fn flush_count(&self) -> usize {
        self.inner
            .lock()
            .expect("WU0C test sink mutex is not poisoned")
            .flushes
    }

    pub(super) fn fail_next_write_for_test(&self) {
        self.inner
            .lock()
            .expect("WU0C test sink mutex is not poisoned")
            .fail_next_write = true;
    }

    pub(super) fn write_failures_for_test(&self) -> usize {
        self.inner
            .lock()
            .expect("WU0C test sink mutex is not poisoned")
            .write_failures
    }
}

#[derive(Clone)]
enum AttributionClock {
    Test(AttributionTestClock),
    Real(Instant),
}

impl AttributionClock {
    fn now_us(&self) -> u64 {
        match self {
            Self::Test(clock) => clock.now_us.load(Ordering::Relaxed),
            Self::Real(started) => u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        }
    }
}

enum ReporterSink {
    Test(AttributionTestSink),
    File(BufWriter<File>),
}

impl ReporterSink {
    fn write_batch(&mut self, lines: Vec<String>) -> std::io::Result<()> {
        match self {
            Self::Test(sink) => {
                let mut state = sink
                    .inner
                    .lock()
                    .expect("WU0C test sink mutex is not poisoned");
                if state.fail_next_write {
                    state.fail_next_write = false;
                    state.write_failures += 1;
                    return Err(std::io::Error::other("injected WU0C sink failure"));
                }
                state.batches.push(lines);
                state.flushes += 1;
                Ok(())
            }
            Self::File(writer) => {
                for line in lines {
                    writer.write_all(line.as_bytes())?;
                    writer.write_all(b"\n")?;
                }
                writer.flush()
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
struct EagerStats {
    calls: u64,
    hits: u64,
    misses: u64,
    clean: u64,
    tainted: u64,
    active: u64,
    completed: u64,
    completed_us: u64,
    active_started_us: Option<u64>,
}

#[derive(Clone, Debug)]
struct ExactStateSnapshot {
    id: u64,
    type_id: u32,
    context: Arc<[u32]>,
    map: Arc<[(u32, u32)]>,
    application: Option<TypeId>,
    saturated: bool,
}

#[derive(Clone, Debug)]
struct ExactEventSnapshot {
    event: u64,
    action: EventAction,
    visit: u64,
    parent: Option<u64>,
    state: u64,
    disposition: Option<EventDisposition>,
    at_us: u64,
}

#[derive(Clone, Debug)]
struct RunSnapshot {
    id: u64,
    family: String,
    completed: bool,
    visits: u64,
    memo_hits: u64,
    cycle_reentries: u64,
    tainted_ancestors: u64,
    states: Vec<ExactStateSnapshot>,
    events: Vec<ExactEventSnapshot>,
}

#[derive(Clone, Debug)]
struct SemanticSnapshot {
    phase: AttributionPhase,
    semantic_elapsed_us: u64,
    reserve_fill_us: u64,
    coverage_lost: bool,
    invalid_limit: Option<LimitKind>,
    eager: Vec<(String, EagerStats)>,
    runs: Vec<RunSnapshot>,
}

impl SemanticSnapshot {
    fn active_family(&self) -> Option<(&str, u64)> {
        self.eager.iter().find_map(|(family, stats)| {
            (stats.active == 1).then(|| {
                let started = stats.active_started_us.unwrap_or(self.semantic_elapsed_us);
                (
                    family.as_str(),
                    self.semantic_elapsed_us.saturating_sub(started),
                )
            })
        })
    }
}

enum ReporterMessage {
    Checkpoint(SemanticSnapshot, usize),
    Report(SemanticSnapshot, mpsc::Sender<()>),
    Finish(SemanticSnapshot, mpsc::Sender<()>),
    Phase,
    FireDue(mpsc::Sender<bool>),
    Barrier(mpsc::Sender<()>),
    Pause(mpsc::Sender<()>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CollectorCounts {
    pub eager_keys: usize,
    pub runs: usize,
    pub dictionary_entries: usize,
    pub trace_events: usize,
    pub map_entries: usize,
    pub context_entries: usize,
    pub application_entries: usize,
    pub live_exact_bytes: usize,
    pub coverage_lost: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CheckpointQueueCounts {
    pub messages: usize,
    pub bytes: usize,
}

#[derive(Default)]
struct CheckpointQueueState {
    counts: CheckpointQueueCounts,
}

struct ReporterWriter {
    config: AttributionConfig,
    identities: SessionIdentities,
    sink: ReporterSink,
    seq: u64,
    file_bytes: usize,
    invalid: bool,
    emitted_states: BTreeSet<(u64, u64)>,
    emitted_events: BTreeSet<(u64, u64)>,
    emitted_runs: BTreeMap<u64, (u64, bool, u64, u64, u64)>,
}

#[derive(Clone)]
struct SessionIdentities {
    session: String,
    binary: String,
    host: String,
    workload_profile: String,
}

impl ReporterWriter {
    fn new(config: AttributionConfig, identities: SessionIdentities, sink: ReporterSink) -> Self {
        Self {
            config,
            identities,
            sink,
            seq: 0,
            file_bytes: 0,
            invalid: false,
            emitted_states: BTreeSet::new(),
            emitted_events: BTreeSet::new(),
            emitted_runs: BTreeMap::new(),
        }
    }

    fn common(&self, kind: &str) -> String {
        let universe = self
            .config
            .universe
            .map(|universe| format!(" universe={universe}"))
            .unwrap_or_default();
        format!(
            "{PREFIX} kind={kind} seq={} process={} mode={}{}",
            self.seq,
            self.config.process,
            self.config.mode.render(),
            universe
        )
    }

    fn session_line(&self) -> String {
        let capabilities = match self.config.mode {
            AttributionMode::Off => "-",
            AttributionMode::ReporterControl => "progress",
            AttributionMode::Progress => "progress,eager,substitution",
            AttributionMode::Exact => "progress,eager,substitution,exact_trace",
        };
        let limits = self.config.limits;
        format!(
            "{} session_sha256={} binary_sha256={} host_sha256={} workload_profile_sha256={} capabilities={capabilities} interval_ms={} checkpoint_visits={} evidence_window_ms={} max_lines={} max_eager_keys={} max_runs={} max_dictionary_entries={} max_trace_events={} max_checkpoint_messages={} max_checkpoint_bytes={} max_line_bytes={} max_file_bytes={} max_map_entries={} max_context_entries={} max_application_entries={} max_live_exact_bytes={} terminal_reserve_lines={} terminal_reserve_bytes={}",
            self.common("session"),
            self.identities.session,
            self.identities.binary,
            self.identities.host,
            self.identities.workload_profile,
            self.config.interval_ms,
            self.config.checkpoint_visits,
            self.config.evidence_window_ms,
            limits.lines,
            limits.eager_keys,
            limits.runs,
            limits.dictionary_entries,
            limits.trace_events,
            limits.checkpoint_messages,
            limits.checkpoint_bytes,
            limits.rendered_line_bytes,
            limits.file_bytes,
            limits.map_entries,
            limits.context_entries,
            limits.application_entries,
            limits.live_exact_bytes,
            limits.terminal_reserve_lines,
            limits.terminal_reserve_bytes,
        )
    }

    fn write_initial(&mut self) {
        let line = self.session_line();
        self.write_lines(vec![line]);
    }

    fn render_snapshot(&mut self, snapshot: &SemanticSnapshot) -> Vec<String> {
        let mut lines = Vec::new();
        for (family, stats) in &snapshot.eager {
            let active_us = stats.active_started_us.map_or(0, |started| {
                snapshot.semantic_elapsed_us.saturating_sub(started)
            });
            lines.push(format!(
                "{} family_sha256={family} calls={} hits={} misses={} clean={} tainted={} active={} completed={} completed_us={} active_us={active_us}",
                self.common_with_offset("eager", lines.len()),
                stats.calls,
                stats.hits,
                stats.misses,
                stats.clean,
                stats.tainted,
                stats.active,
                stats.completed,
                stats.completed_us,
            ));
        }
        for run in &snapshot.runs {
            let last = self.emitted_runs.get(&run.id).copied();
            let has_exact_delta = self.config.mode == AttributionMode::Exact
                && (!run.states.is_empty() || !run.events.is_empty());
            let due_checkpoint = has_exact_delta
                || (run.visits >= self.config.checkpoint_visits
                    && last.is_none_or(
                        |(visits, _, memo_hits, cycle_reentries, tainted_ancestors)| {
                            run.visits >= visits.saturating_add(self.config.checkpoint_visits)
                                || run.memo_hits != memo_hits
                                || run.cycle_reentries != cycle_reentries
                                || run.tainted_ancestors != tainted_ancestors
                        },
                    ));
            let due_completion = run.completed
                && last
                    .is_none_or(|(visits, completed, _, _, _)| visits != run.visits || !completed);
            if !due_checkpoint && !due_completion {
                continue;
            }
            if self.config.mode == AttributionMode::Exact {
                for state in &run.states {
                    if !self.emitted_states.insert((run.id, state.id)) {
                        continue;
                    }
                    let map = render_map(&state.map);
                    let context = render_context(&state.context);
                    let application = render_application(
                        state.application,
                        &state.map,
                        application_len(state.application, &state.map),
                    );
                    lines.push(format!(
                        "{} run={} state={} type_id={} context={context} map={map} map_sha256={} application={} application_sha256={} saturated={}",
                        self.common_with_offset("state", lines.len()),
                        run.id,
                        state.id,
                        state.type_id,
                        hex_sha256(map.as_bytes()),
                        application,
                        hex_sha256(application.as_bytes()),
                        u8::from(state.saturated),
                    ));
                }
                for event in &run.events {
                    if !self.emitted_events.insert((run.id, event.event)) {
                        continue;
                    }
                    let parent = event
                        .parent
                        .map(|parent| parent.to_string())
                        .unwrap_or_else(|| "none".to_owned());
                    let action = match event.action {
                        EventAction::Enter => "enter",
                        EventAction::Outcome => "outcome",
                        EventAction::Exit => "exit",
                    };
                    let disposition = event.disposition.map(|disposition| {
                        format!(" disposition={}", render_disposition(disposition))
                    });
                    lines.push(format!(
                        "{} run={} event={} action={action} visit={} parent={parent} state={}{} at_us={}",
                        self.common_with_offset("event", lines.len()),
                        run.id,
                        event.event,
                        event.visit,
                        event.state,
                        disposition.unwrap_or_default(),
                        event.at_us,
                    ));
                }
            }
            lines.push(format!(
                "{} run={} family_sha256={} checkpoint=1 started=1 completed={} active={} visits={} memo_hits={} cycle_reentries={} tainted_ancestors={}",
                self.common_with_offset("run", lines.len()),
                run.id,
                run.family,
                u8::from(run.completed),
                u8::from(!run.completed),
                run.visits,
                run.memo_hits,
                run.cycle_reentries,
                run.tainted_ancestors,
            ));
            self.emitted_runs.insert(
                run.id,
                (
                    run.visits,
                    run.completed,
                    run.memo_hits,
                    run.cycle_reentries,
                    run.tainted_ancestors,
                ),
            );
        }
        lines
    }

    fn common_with_offset(&self, kind: &str, offset: usize) -> String {
        let universe = self
            .config
            .universe
            .map(|universe| format!(" universe={universe}"))
            .unwrap_or_default();
        format!(
            "{PREFIX} kind={kind} seq={} process={} mode={}{}",
            self.seq + u64::try_from(offset).expect("reporter batch offset fits u64"),
            self.config.process,
            self.config.mode.render(),
            universe
        )
    }

    fn write_snapshot(&mut self, snapshot: &SemanticSnapshot) {
        if let Some(limit) = snapshot.invalid_limit {
            self.invalidate(limit, false);
            return;
        }
        if self.invalid {
            return;
        }
        let lines = self.render_snapshot(snapshot);
        if !lines.is_empty() {
            self.write_lines(lines);
        }
    }

    fn write_heartbeat(&mut self, snapshot: &SemanticSnapshot, reporter_elapsed_us: u64) {
        if let Some(limit) = snapshot.invalid_limit {
            self.invalidate(limit, false);
            return;
        }
        if self.invalid {
            return;
        }
        let line = self.heartbeat_line(snapshot, reporter_elapsed_us, 0);
        self.write_lines(vec![line]);
    }

    fn heartbeat_line(
        &self,
        snapshot: &SemanticSnapshot,
        reporter_elapsed_us: u64,
        offset: usize,
    ) -> String {
        let (family, active_us) = snapshot.active_family().unwrap_or(("-", 0));
        format!(
            "{} phase={} reporter_elapsed_us={reporter_elapsed_us} checkpoint_elapsed_us={} reserve_fill_us={} active_family_sha256={family} active_elapsed_us={active_us} coverage_lost={}",
            self.common_with_offset("heartbeat", offset),
            snapshot.phase.render(),
            snapshot.semantic_elapsed_us,
            snapshot.reserve_fill_us,
            u8::from(snapshot.coverage_lost || self.invalid),
        )
    }

    fn write_finish(&mut self, snapshot: &SemanticSnapshot, reporter_elapsed_us: u64) {
        if let Some(limit) = snapshot.invalid_limit {
            self.invalidate(limit, false);
        }
        if !self.invalid {
            self.write_snapshot(snapshot);
            self.write_heartbeat(snapshot, reporter_elapsed_us);
        }
        let finish_seq = self.seq;
        let total_lines = finish_seq + 1;
        let universe = self
            .config
            .universe
            .map(|universe| format!(" universe={universe}"))
            .unwrap_or_default();
        let finish = format!(
            "{PREFIX} kind=finish seq={finish_seq} process={} mode={}{} status=complete reporter_elapsed_us={reporter_elapsed_us} checkpoint_elapsed_us={} coverage_lost={} lines={total_lines}",
            self.config.process,
            self.config.mode.render(),
            universe,
            snapshot.semantic_elapsed_us,
            u8::from(snapshot.coverage_lost || self.invalid),
        );
        self.write_terminal(vec![finish]);
    }

    fn write_lines(&mut self, lines: Vec<String>) {
        if self.invalid || lines.is_empty() {
            return;
        }
        let limits = self.config.limits;
        let exceeds_line = lines
            .iter()
            .any(|line| line.len() > limits.rendered_line_bytes);
        let batch_bytes = lines.iter().map(|line| line.len() + 1).sum::<usize>();
        let ordinary_line_limit = limits.lines.saturating_sub(limits.terminal_reserve_lines);
        let ordinary_byte_limit = limits
            .file_bytes
            .saturating_sub(limits.terminal_reserve_bytes);
        let line_overflow = usize::try_from(self.seq)
            .ok()
            .and_then(|written| written.checked_add(lines.len()))
            .is_none_or(|total| total > ordinary_line_limit);
        let byte_overflow = self
            .file_bytes
            .checked_add(batch_bytes)
            .is_none_or(|total| total > ordinary_byte_limit);
        if exceeds_line || line_overflow || byte_overflow {
            let limit = if exceeds_line {
                LimitKind::RenderedLineBytes
            } else if line_overflow {
                LimitKind::Lines
            } else {
                LimitKind::FileBytes
            };
            self.invalidate(limit, false);
            return;
        }
        if self.sink.write_batch(lines.clone()).is_err() {
            self.invalidate(LimitKind::FileBytes, true);
            return;
        }
        let written_bytes = lines.iter().map(|line| line.len() + 1).sum::<usize>();
        self.seq += u64::try_from(lines.len()).expect("reporter batch length fits u64");
        self.file_bytes = self.file_bytes.saturating_add(written_bytes);
    }

    fn invalidate(&mut self, limit: LimitKind, sink_write_failure: bool) {
        if self.invalid {
            return;
        }
        self.invalid = true;
        let reason = if sink_write_failure {
            " reason=sink_write_failure"
        } else {
            ""
        };
        let line = format!(
            "{} limit={}{}",
            self.common("invalid"),
            limit.render(),
            reason
        );
        self.write_terminal(vec![line]);
    }

    fn write_terminal(&mut self, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        let bytes = lines.iter().map(|line| line.len() + 1).sum::<usize>();
        let next_lines = usize::try_from(self.seq)
            .ok()
            .and_then(|written| written.checked_add(lines.len()));
        let next_bytes = self.file_bytes.checked_add(bytes);
        if next_lines.is_none_or(|total| total > self.config.limits.lines)
            || next_bytes.is_none_or(|total| total > self.config.limits.file_bytes)
            || lines
                .iter()
                .any(|line| line.len() > self.config.limits.rendered_line_bytes)
        {
            return;
        }
        if self.sink.write_batch(lines.clone()).is_ok() {
            self.seq += u64::try_from(lines.len()).expect("terminal batch length fits u64");
            self.file_bytes = self.file_bytes.saturating_add(bytes);
        }
    }
}

fn render_map(map: &[(u32, u32)]) -> String {
    if map.is_empty() {
        return "-".to_owned();
    }
    map.iter()
        .map(|(parameter, ty)| format!("{parameter}:{ty}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn decimal_len(value: u32) -> usize {
    if value == 0 {
        1
    } else {
        usize::try_from(value.ilog10()).expect("u32 digit count fits usize") + 1
    }
}

fn rendered_map_len(map: &[(u32, u32)]) -> usize {
    if map.is_empty() {
        return 1;
    }
    map.iter()
        .enumerate()
        .map(|(index, (parameter, ty))| {
            usize::from(index != 0) + decimal_len(*parameter) + 1 + decimal_len(*ty)
        })
        .sum()
}

fn application_len(application: Option<TypeId>, map: &[(u32, u32)]) -> usize {
    application.map_or(1, |application| {
        decimal_len(application.0) + 1 + rendered_map_len(map)
    })
}

fn render_application(application: Option<TypeId>, map: &[(u32, u32)], len: usize) -> String {
    let Some(application) = application else {
        return "-".to_owned();
    };
    let mut rendered = String::with_capacity(len);
    std::fmt::Write::write_fmt(&mut rendered, format_args!("{}|", application.0))
        .expect("writing to String is infallible");
    if map.is_empty() {
        rendered.push('-');
    } else {
        for (index, (parameter, ty)) in map.iter().enumerate() {
            if index != 0 {
                rendered.push(',');
            }
            std::fmt::Write::write_fmt(&mut rendered, format_args!("{parameter}:{ty}"))
                .expect("writing to String is infallible");
        }
    }
    rendered
}

fn render_context(context: &[u32]) -> String {
    if context.is_empty() {
        return "-".to_owned();
    }
    context
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn render_disposition(disposition: EventDisposition) -> &'static str {
    match disposition {
        EventDisposition::Clean => "clean",
        EventDisposition::CompletedMemoHit => "completed_memo_hit",
        EventDisposition::RawCycleReentry => "raw_cycle_reentry",
        EventDisposition::Tainted => "tainted",
    }
}

struct ReporterSharedState {
    checkpoint_queue: Arc<Mutex<CheckpointQueueState>>,
    pause: Arc<(Mutex<bool>, Condvar)>,
    phase_slot: Arc<Mutex<Option<AttributionPhase>>>,
    thread: Arc<Mutex<Option<ThreadId>>>,
}

fn reporter_loop(
    config: AttributionConfig,
    identities: SessionIdentities,
    clock: AttributionClock,
    sink: ReporterSink,
    receiver: Receiver<ReporterMessage>,
    shared: ReporterSharedState,
    ready: mpsc::Sender<()>,
) {
    *shared
        .thread
        .lock()
        .expect("WU0C reporter thread-id mutex is not poisoned") =
        Some(std::thread::current().id());
    let mut writer = ReporterWriter::new(config, identities, sink);
    writer.write_initial();
    let _ = ready.send(());
    let mut last_snapshot = SemanticSnapshot {
        phase: AttributionPhase::Bind,
        semantic_elapsed_us: 0,
        reserve_fill_us: 0,
        coverage_lost: false,
        invalid_limit: None,
        eager: Vec::new(),
        runs: Vec::new(),
    };
    let interval = Duration::from_millis(config.interval_ms);
    let interval_us = config.interval_ms.saturating_mul(1_000);
    let mut next_heartbeat_us = interval_us;
    let mut phase_dirty = false;
    loop {
        {
            let (paused, resumed) = &*shared.pause;
            let mut paused = paused
                .lock()
                .expect("WU0C reporter pause mutex is not poisoned");
            while *paused {
                paused = resumed
                    .wait(paused)
                    .expect("WU0C reporter pause mutex is not poisoned");
            }
        }
        match receiver.recv_timeout(interval) {
            Ok(message) => match message {
                ReporterMessage::Checkpoint(snapshot, bytes) => {
                    emit_due_heartbeats(
                        &mut writer,
                        &last_snapshot,
                        clock.now_us(),
                        interval_us,
                        &mut next_heartbeat_us,
                    );
                    let mut queue = shared
                        .checkpoint_queue
                        .lock()
                        .expect("WU0C checkpoint queue mutex is not poisoned");
                    queue.counts.messages = queue.counts.messages.saturating_sub(1);
                    queue.counts.bytes = queue.counts.bytes.saturating_sub(bytes);
                    drop(queue);
                    writer.write_snapshot(&snapshot);
                    last_snapshot = snapshot;
                    phase_dirty = false;
                }
                ReporterMessage::Report(snapshot, done) => {
                    emit_due_heartbeats(
                        &mut writer,
                        &last_snapshot,
                        clock.now_us(),
                        interval_us,
                        &mut next_heartbeat_us,
                    );
                    writer.write_snapshot(&snapshot);
                    last_snapshot = snapshot;
                    phase_dirty = false;
                    let _ = done.send(());
                }
                ReporterMessage::Finish(snapshot, done) => {
                    emit_due_heartbeats(
                        &mut writer,
                        &last_snapshot,
                        clock.now_us(),
                        interval_us,
                        &mut next_heartbeat_us,
                    );
                    let elapsed = clock.now_us();
                    writer.write_finish(&snapshot, elapsed);
                    let _ = done.send(());
                    break;
                }
                ReporterMessage::Phase => {
                    emit_due_heartbeats(
                        &mut writer,
                        &last_snapshot,
                        clock.now_us(),
                        interval_us,
                        &mut next_heartbeat_us,
                    );
                    if let Some(phase) = shared
                        .phase_slot
                        .lock()
                        .expect("WU0C phase slot mutex is not poisoned")
                        .take()
                    {
                        last_snapshot.phase = phase;
                        phase_dirty = true;
                    }
                }
                ReporterMessage::FireDue(done) => {
                    let emitted = emit_due_heartbeats(
                        &mut writer,
                        &last_snapshot,
                        clock.now_us(),
                        interval_us,
                        &mut next_heartbeat_us,
                    );
                    let _ = done.send(emitted);
                }
                ReporterMessage::Barrier(done) => {
                    if let Some(phase) = shared
                        .phase_slot
                        .lock()
                        .expect("WU0C phase slot mutex is not poisoned")
                        .take()
                    {
                        last_snapshot.phase = phase;
                        phase_dirty = true;
                    }
                    if phase_dirty {
                        let now = clock.now_us();
                        if now > 0 {
                            writer.write_heartbeat(&last_snapshot, now);
                        }
                        phase_dirty = false;
                    }
                    let _ = done.send(());
                }
                ReporterMessage::Pause(done) => {
                    *shared
                        .pause
                        .0
                        .lock()
                        .expect("WU0C reporter pause mutex is not poisoned") = true;
                    let _ = done.send(());
                }
            },
            Err(RecvTimeoutError::Timeout) => {
                emit_due_heartbeats(
                    &mut writer,
                    &last_snapshot,
                    clock.now_us(),
                    interval_us,
                    &mut next_heartbeat_us,
                );
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn emit_due_heartbeats(
    writer: &mut ReporterWriter,
    snapshot: &SemanticSnapshot,
    now_us: u64,
    interval_us: u64,
    next_heartbeat_us: &mut u64,
) -> bool {
    let mut emitted = false;
    while *next_heartbeat_us <= now_us {
        writer.write_heartbeat(snapshot, *next_heartbeat_us);
        emitted = true;
        *next_heartbeat_us = next_heartbeat_us.saturating_add(interval_us);
        if interval_us == 0 {
            break;
        }
    }
    emitted
}

struct SemanticState {
    phase: AttributionPhase,
    reserve_fill_started_us: Option<u64>,
    reserve_fill_completed_us: u64,
    families: BTreeMap<TypeGroupId, FamilyToken>,
    eager: BTreeMap<String, EagerStats>,
    runs: Vec<Weak<RunCapture>>,
    completed_runs: Vec<RunSnapshot>,
    next_run: u64,
    coverage_lost: bool,
    invalid_limit: Option<LimitKind>,
    counts: CollectorCounts,
    captured_passes: usize,
    last_semantic_thread: Option<ThreadId>,
    unregistered_family_lookups: usize,
}

impl Default for SemanticState {
    fn default() -> Self {
        Self {
            phase: AttributionPhase::Bind,
            reserve_fill_started_us: None,
            reserve_fill_completed_us: 0,
            families: BTreeMap::new(),
            eager: BTreeMap::new(),
            runs: Vec::new(),
            completed_runs: Vec::new(),
            next_run: 1,
            coverage_lost: false,
            invalid_limit: None,
            counts: CollectorCounts::default(),
            captured_passes: 0,
            last_semantic_thread: None,
            unregistered_family_lookups: 0,
        }
    }
}

struct AttributionSession {
    config: AttributionConfig,
    clock: AttributionClock,
    active: Cell<bool>,
    coverage_lost: Cell<bool>,
    state: RefCell<SemanticState>,
    sender: SyncSender<ReporterMessage>,
    checkpoint_queue: Arc<Mutex<CheckpointQueueState>>,
    reporter_pause: Arc<(Mutex<bool>, Condvar)>,
    phase_slot: Arc<Mutex<Option<AttributionPhase>>>,
    coalesced_phase_updates: Cell<usize>,
    last_coalesced_phase_updates: Cell<usize>,
    reporter_thread: Arc<Mutex<Option<ThreadId>>>,
}

impl AttributionSession {
    fn invalidate_state(state: &mut SemanticState, limit: LimitKind) {
        if state.invalid_limit.is_none() {
            state.invalid_limit = Some(limit);
        }
        state.coverage_lost = true;
        state.counts.coverage_lost = true;
    }

    fn invalidate(&self, limit: LimitKind) {
        self.coverage_lost.set(true);
        let mut state = self.state.borrow_mut();
        Self::invalidate_state(&mut state, limit);
    }

    fn lose_coverage(&self) {
        self.coverage_lost.set(true);
        let mut state = self.state.borrow_mut();
        state.coverage_lost = true;
        state.counts.coverage_lost = true;
    }

    fn ensure_eager_family(&self, family: &str) -> bool {
        let mut state = self.state.borrow_mut();
        if state.coverage_lost {
            return false;
        }
        if state.eager.contains_key(family) {
            return true;
        }
        if state.counts.eager_keys >= self.config.limits.eager_keys {
            self.coverage_lost.set(true);
            Self::invalidate_state(&mut state, LimitKind::EagerKeys);
            return false;
        }
        state.eager.insert(family.to_owned(), EagerStats::default());
        state.counts.eager_keys += 1;
        true
    }

    fn reserve_exact_state(&self, context_entries: usize, bytes: usize) -> bool {
        let mut state = self.state.borrow_mut();
        if state.coverage_lost {
            return false;
        }
        let limits = self.config.limits;
        let next_dictionary = state.counts.dictionary_entries.checked_add(1);
        let next_context = state.counts.context_entries.checked_add(context_entries);
        let next_bytes = state.counts.live_exact_bytes.checked_add(bytes);
        let limit = if next_dictionary.is_none_or(|count| count > limits.dictionary_entries) {
            Some(LimitKind::DictionaryEntries)
        } else if next_context.is_none_or(|count| count > limits.context_entries) {
            Some(LimitKind::ContextEntries)
        } else if next_bytes.is_none_or(|count| count > limits.live_exact_bytes) {
            Some(LimitKind::LiveExactBytes)
        } else {
            None
        };
        if let Some(limit) = limit {
            self.coverage_lost.set(true);
            Self::invalidate_state(&mut state, limit);
            return false;
        }
        state.counts.dictionary_entries = next_dictionary.expect("checked dictionary count");
        state.counts.context_entries = next_context.expect("checked context count");
        state.counts.live_exact_bytes = next_bytes.expect("checked exact byte count");
        true
    }

    fn reserve_exact_event(&self, bytes: usize) -> bool {
        let mut state = self.state.borrow_mut();
        if state.coverage_lost {
            return false;
        }
        let limits = self.config.limits;
        let next_events = state.counts.trace_events.checked_add(1);
        let next_bytes = state.counts.live_exact_bytes.checked_add(bytes);
        let limit = if next_events.is_none_or(|count| count > limits.trace_events) {
            Some(LimitKind::TraceEvents)
        } else if next_bytes.is_none_or(|count| count > limits.live_exact_bytes) {
            Some(LimitKind::LiveExactBytes)
        } else {
            None
        };
        if let Some(limit) = limit {
            self.coverage_lost.set(true);
            Self::invalidate_state(&mut state, limit);
            return false;
        }
        state.counts.trace_events = next_events.expect("checked event count");
        state.counts.live_exact_bytes = next_bytes.expect("checked exact byte count");
        true
    }

    fn preflight_exact_visit(
        &self,
        context_entries: usize,
        state_bytes: usize,
        event_bytes: usize,
    ) -> bool {
        let mut state = self.state.borrow_mut();
        if state.coverage_lost {
            return false;
        }
        let limits = self.config.limits;
        let limit = if state
            .counts
            .dictionary_entries
            .checked_add(1)
            .is_none_or(|count| count > limits.dictionary_entries)
        {
            Some(LimitKind::DictionaryEntries)
        } else if state
            .counts
            .context_entries
            .checked_add(context_entries)
            .is_none_or(|count| count > limits.context_entries)
        {
            Some(LimitKind::ContextEntries)
        } else if state
            .counts
            .trace_events
            .checked_add(1)
            .is_none_or(|count| count > limits.trace_events)
        {
            Some(LimitKind::TraceEvents)
        } else if state
            .counts
            .live_exact_bytes
            .checked_add(state_bytes)
            .and_then(|bytes| bytes.checked_add(event_bytes))
            .is_none_or(|count| count > limits.live_exact_bytes)
        {
            Some(LimitKind::LiveExactBytes)
        } else {
            None
        };
        if let Some(limit) = limit {
            self.coverage_lost.set(true);
            Self::invalidate_state(&mut state, limit);
            return false;
        }
        true
    }

    fn snapshot(&self) -> SemanticSnapshot {
        let now = self.clock.now_us();
        let mut state = self.state.borrow_mut();
        state.runs.retain(|run| run.strong_count() > 0);
        let reserve_fill_us = state.reserve_fill_completed_us.saturating_add(
            state
                .reserve_fill_started_us
                .map_or(0, |started| now.saturating_sub(started)),
        );
        let mut runs = state.completed_runs.clone();
        runs.extend(
            state
                .runs
                .iter()
                .filter_map(Weak::upgrade)
                .map(|run| run.snapshot()),
        );
        SemanticSnapshot {
            phase: state.phase,
            semantic_elapsed_us: now,
            reserve_fill_us,
            coverage_lost: state.coverage_lost,
            invalid_limit: state.invalid_limit,
            eager: state
                .eager
                .iter()
                .map(|(family, stats)| (family.clone(), stats.clone()))
                .collect(),
            runs,
        }
    }

    fn enter_phase(&self, phase: AttributionPhase) {
        if !self.active.get() {
            return;
        }
        let now = self.clock.now_us();
        let mut state = self.state.borrow_mut();
        if state.phase == AttributionPhase::ReserveFill {
            if let Some(started) = state.reserve_fill_started_us.take() {
                state.reserve_fill_completed_us = state
                    .reserve_fill_completed_us
                    .saturating_add(now.saturating_sub(started));
            }
        }
        if phase == AttributionPhase::ReserveFill && state.phase != AttributionPhase::ReserveFill {
            state.reserve_fill_started_us = Some(now);
        }
        state.phase = phase;
        state.last_semantic_thread = Some(std::thread::current().id());
        drop(state);
        let replaced_pending = self
            .phase_slot
            .lock()
            .expect("WU0C phase slot mutex is not poisoned")
            .replace(phase)
            .is_some();
        if replaced_pending {
            self.coalesced_phase_updates
                .set(self.coalesced_phase_updates.get().saturating_add(1));
        }
        let should_notify = !replaced_pending;
        if should_notify && self.sender.send(ReporterMessage::Phase).is_err() {
            self.invalidate(LimitKind::CheckpointMessages);
        }
    }

    fn checkpoint_nonblocking(&self, run: &RunCapture) {
        if !self.active.get() || self.coverage_lost.get() {
            return;
        }
        let eager_bytes = self
            .state
            .borrow()
            .eager
            .values()
            .filter(|stats| stats.active == 1)
            .count()
            .saturating_mul(128);
        let bytes = 64_usize
            .saturating_add(eager_bytes)
            .saturating_add(run.checkpoint_delta_estimated_bytes());
        {
            let mut queue = self
                .checkpoint_queue
                .lock()
                .expect("WU0C checkpoint queue mutex is not poisoned");
            let next_messages = queue.counts.messages.saturating_add(1);
            if next_messages > self.config.limits.checkpoint_messages {
                drop(queue);
                self.invalidate(LimitKind::CheckpointMessages);
                return;
            }
            let Some(next_bytes) = queue.counts.bytes.checked_add(bytes) else {
                drop(queue);
                self.invalidate(LimitKind::CheckpointBytes);
                return;
            };
            if next_bytes > self.config.limits.checkpoint_bytes {
                drop(queue);
                self.invalidate(LimitKind::CheckpointBytes);
                return;
            }
            queue.counts.messages = next_messages;
            queue.counts.bytes = next_bytes;
        }
        let snapshot = self.checkpoint_snapshot(run);
        match self
            .sender
            .try_send(ReporterMessage::Checkpoint(snapshot, bytes))
        {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                let mut queue = self
                    .checkpoint_queue
                    .lock()
                    .expect("WU0C checkpoint queue mutex is not poisoned");
                queue.counts.messages = queue.counts.messages.saturating_sub(1);
                queue.counts.bytes = queue.counts.bytes.saturating_sub(bytes);
                drop(queue);
                self.invalidate(LimitKind::CheckpointMessages);
            }
        }
    }

    fn checkpoint_snapshot(&self, run: &RunCapture) -> SemanticSnapshot {
        let now = self.clock.now_us();
        let state = self.state.borrow();
        let reserve_fill_us = state.reserve_fill_completed_us.saturating_add(
            state
                .reserve_fill_started_us
                .map_or(0, |started| now.saturating_sub(started)),
        );
        SemanticSnapshot {
            phase: state.phase,
            semantic_elapsed_us: now,
            reserve_fill_us,
            coverage_lost: state.coverage_lost,
            invalid_limit: state.invalid_limit,
            eager: state
                .eager
                .iter()
                .filter(|(_, stats)| stats.active == 1)
                .map(|(family, stats)| (family.clone(), stats.clone()))
                .collect(),
            runs: vec![run.checkpoint_delta()],
        }
    }

    fn report_and_wait(&self) {
        if !self.active.get() {
            return;
        }
        let (done, wait) = mpsc::channel();
        if self
            .sender
            .send(ReporterMessage::Report(self.snapshot(), done))
            .is_ok()
        {
            let _ = wait.recv();
        } else {
            self.lose_coverage();
        }
    }

    fn reserve_run(&self, map_entries: usize, application: Option<TypeId>) -> Option<u64> {
        let mode = self.config.mode;
        let application_entries =
            usize::from(mode == AttributionMode::Exact && application.is_some());
        let map_entries = if mode == AttributionMode::Exact {
            map_entries
        } else {
            0
        };
        let exact_base_bytes = if mode == AttributionMode::Exact {
            64_usize
                .saturating_add(map_entries.saturating_mul(8))
                .saturating_add(application_entries.saturating_mul(8))
        } else {
            0
        };
        let mut state = self.state.borrow_mut();
        if state.coverage_lost {
            return None;
        }
        let limits = self.config.limits;
        let next_map_entries = state.counts.map_entries.checked_add(map_entries);
        let next_application_entries = state
            .counts
            .application_entries
            .checked_add(application_entries);
        let next_live_bytes = state.counts.live_exact_bytes.checked_add(exact_base_bytes);
        let limit = if state.counts.runs >= limits.runs {
            Some(LimitKind::Runs)
        } else if next_map_entries.is_none_or(|count| count > limits.map_entries) {
            Some(LimitKind::MapEntries)
        } else if next_application_entries.is_none_or(|count| count > limits.application_entries) {
            Some(LimitKind::ApplicationEntries)
        } else if next_live_bytes.is_none_or(|count| count > limits.live_exact_bytes) {
            Some(LimitKind::LiveExactBytes)
        } else {
            None
        };
        if let Some(limit) = limit {
            self.coverage_lost.set(true);
            Self::invalidate_state(&mut state, limit);
            return None;
        }
        let id = state.next_run;
        state.next_run += 1;
        state.counts.runs += 1;
        state.counts.map_entries = next_map_entries.expect("checked exact map count");
        state.counts.application_entries =
            next_application_entries.expect("checked application count");
        state.counts.live_exact_bytes = next_live_bytes.expect("checked exact byte count");
        Some(id)
    }

    fn finish_new_run(
        self: &Rc<Self>,
        id: u64,
        family: &str,
        canonical_map: Vec<(u32, u32)>,
        application: Option<TypeId>,
    ) -> Rc<RunCapture> {
        let mode = self.config.mode;
        let detail = if mode == AttributionMode::Exact {
            RunDetail::Exact(RefCell::new(ExactRunState {
                map: canonical_map.into(),
                application,
                ..ExactRunState::default()
            }))
        } else {
            RunDetail::Progress
        };
        let run = Rc::new(RunCapture {
            session: Rc::clone(self),
            id,
            family: family.to_owned(),
            mode,
            checkpoint_visits: self.config.checkpoint_visits,
            next_checkpoint: Cell::new(self.config.checkpoint_visits),
            checkpoint_due: Cell::new(false),
            pending_open_trace: Cell::new(false),
            visits: Cell::new(0),
            memo_hits: Cell::new(0),
            cycle_reentries: Cell::new(0),
            tainted_ancestors: Cell::new(0),
            completed: Cell::new(false),
            detail,
        });
        self.state.borrow_mut().runs.push(Rc::downgrade(&run));
        run
    }

    fn new_run(
        self: &Rc<Self>,
        family: &str,
        map: &[(TypeParamId, TypeId)],
        application: Option<TypeId>,
    ) -> Option<Rc<RunCapture>> {
        let id = self.reserve_run(map.len(), application)?;
        let canonical_map = if self.config.mode == AttributionMode::Exact {
            map.iter()
                .map(|(parameter, ty)| (parameter.0, ty.0))
                .collect()
        } else {
            Vec::new()
        };
        Some(self.finish_new_run(id, family, canonical_map, application))
    }

    fn new_run_from_hash_map(
        self: &Rc<Self>,
        family: &str,
        map: &FxHashMap<TypeParamId, TypeId>,
        application: Option<TypeId>,
    ) -> Option<Rc<RunCapture>> {
        let id = self.reserve_run(map.len(), application)?;
        let mut canonical_map = map
            .iter()
            .map(|(parameter, ty)| (parameter.0, ty.0))
            .collect::<Vec<_>>();
        canonical_map.sort_unstable_by_key(|entry| entry.0);
        Some(self.finish_new_run(id, family, canonical_map, application))
    }
}

thread_local! {
    static CURRENT_SESSION: RefCell<Option<Rc<AttributionSession>>> = const { RefCell::new(None) };
    static ACTIVE_APPLICATION: RefCell<Option<ActiveApplicationContext>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct ActiveApplicationContext {
    session: Weak<AttributionSession>,
    family: String,
    application: Option<TypeId>,
}

pub(super) struct AttributionScope {
    session: Rc<AttributionSession>,
    previous: Option<Rc<AttributionSession>>,
    reporter: Option<JoinHandle<()>>,
    _thread_affine: std::marker::PhantomData<Rc<()>>,
}

impl AttributionScope {
    pub(super) fn control_for_test(&self) -> AttributionControl {
        AttributionControl {
            session: Rc::clone(&self.session),
        }
    }
}

impl Drop for AttributionScope {
    fn drop(&mut self) {
        self.session.active.set(false);
        CURRENT_SESSION.with(|current| {
            current.replace(self.previous.take());
        });
        let snapshot = self.session.snapshot();
        let (done, wait) = mpsc::channel();
        if self
            .session
            .sender
            .send(ReporterMessage::Finish(snapshot, done))
            .is_ok()
        {
            let _ = wait.recv();
        }
        if let Some(reporter) = self.reporter.take() {
            let _ = reporter.join();
        }
    }
}

#[derive(Clone)]
pub(super) struct AttributionControl {
    session: Rc<AttributionSession>,
}

impl AttributionControl {
    pub(super) fn process(&self) -> usize {
        usize::from(self.session.config.process)
    }

    pub(super) fn enter_phase(&self, phase: AttributionPhase) {
        self.session.enter_phase(phase);
    }

    pub(super) fn capture_pass_for_test(&self) -> Option<PassAttribution> {
        capture_pass_from_session(&self.session)
    }

    pub(super) fn capture_substitution_for_test(&self) -> Option<SubstitutionAttribution> {
        if !self.session.active.get() || !self.session.config.mode.captures_semantics() {
            return None;
        }
        self.session
            .new_run("-", &[], None)
            .map(SubstitutionAttribution::enabled)
    }

    pub(super) fn report_now_and_wait_for_test(&self) {
        self.session.report_and_wait();
    }

    pub(super) fn accept_checkpoint_and_report_for_test(&self) {
        self.session.report_and_wait();
    }

    pub(super) fn reporter_barrier_for_test(&self) {
        let (done, wait) = mpsc::channel();
        if self
            .session
            .sender
            .send(ReporterMessage::Barrier(done))
            .is_ok()
        {
            let _ = wait.recv();
        }
        self.session
            .last_coalesced_phase_updates
            .set(self.session.coalesced_phase_updates.replace(0));
    }

    pub(super) fn enqueue_control_traffic_for_test(&self) {
        let (done, wait) = mpsc::channel();
        if self
            .session
            .sender
            .send(ReporterMessage::Barrier(done))
            .is_err()
        {
            self.session.invalidate(LimitKind::CheckpointMessages);
        } else {
            let _ = wait.recv();
        }
    }

    pub(super) fn fire_due_heartbeat_for_test(&self) -> bool {
        let (done, wait) = mpsc::channel();
        if self
            .session
            .sender
            .send(ReporterMessage::FireDue(done))
            .is_err()
        {
            self.session.invalidate(LimitKind::CheckpointMessages);
            return false;
        }
        wait.recv().unwrap_or(false)
    }

    pub(super) fn coalesced_phase_updates_for_test(&self) -> usize {
        self.session.last_coalesced_phase_updates.get()
    }

    pub(super) fn collector_counts_for_test(&self) -> CollectorCounts {
        self.session.state.borrow().counts
    }

    pub(super) fn checkpoint_queue_counts_for_test(&self) -> CheckpointQueueCounts {
        self.session
            .checkpoint_queue
            .lock()
            .expect("WU0C checkpoint queue mutex is not poisoned")
            .counts
    }

    pub(super) fn pause_reporter_for_test(&self) {
        let (done, wait) = mpsc::channel();
        if self
            .session
            .sender
            .send(ReporterMessage::Pause(done))
            .is_ok()
        {
            let _ = wait.recv();
        }
    }

    pub(super) fn resume_reporter_for_test(&self) {
        let (paused, resumed) = &*self.session.reporter_pause;
        *paused
            .lock()
            .expect("WU0C reporter pause mutex is not poisoned") = false;
        resumed.notify_all();
    }

    pub(super) fn registered_family_token_for_test(
        &self,
        group: TypeGroupId,
    ) -> Option<FamilyToken> {
        self.session.state.borrow().families.get(&group).cloned()
    }

    pub(super) fn registered_family_count_for_test(&self) -> usize {
        self.session.state.borrow().families.len()
    }

    pub(super) fn unregistered_family_lookups_for_test(&self) -> usize {
        self.session.state.borrow().unregistered_family_lookups
    }

    pub(super) fn captured_passes_for_test(&self) -> usize {
        self.session.state.borrow().captured_passes
    }

    pub(super) fn last_semantic_thread_for_test(&self) -> Option<ThreadId> {
        self.session.state.borrow().last_semantic_thread
    }

    pub(super) fn reporter_thread_for_test(&self) -> Option<ThreadId> {
        *self
            .session
            .reporter_thread
            .lock()
            .expect("WU0C reporter thread-id mutex is not poisoned")
    }
}

#[derive(Clone)]
pub(in crate::check::checker) struct PassAttribution {
    session: Rc<AttributionSession>,
}

fn capture_pass_from_session(session: &Rc<AttributionSession>) -> Option<PassAttribution> {
    if !session.active.get() || !session.config.mode.captures_semantics() {
        return None;
    }
    let mut state = session.state.borrow_mut();
    state.captured_passes += 1;
    state.last_semantic_thread = Some(std::thread::current().id());
    drop(state);
    Some(PassAttribution {
        session: Rc::clone(session),
    })
}

impl PassAttribution {
    pub(super) fn capture_substitution_for_test(
        &self,
        family: &str,
        map: &[(TypeParamId, TypeId)],
        application: Option<TypeId>,
    ) -> SubstitutionAttribution {
        SubstitutionAttribution {
            run: self.session.new_run(family, map, application),
        }
    }

    pub(super) fn start_ready_application_for_test(
        &self,
        family: &str,
    ) -> ReadyApplicationAttribution {
        self.start_ready_application(family.to_owned(), None)
    }

    pub(super) fn record_ready_hit_for_test(&self, family: &str) -> bool {
        self.record_ready_hit(family)
    }

    pub(in crate::check::checker) fn record_ready_group_hit(&self, group: TypeGroupId) {
        let family = self.session.state.borrow().families.get(&group).cloned();
        if let Some(family) = family {
            self.record_ready_hit(family.as_str());
        } else {
            self.session.state.borrow_mut().unregistered_family_lookups += 1;
        }
    }

    fn record_ready_hit(&self, family: &str) -> bool {
        if !self.session.active.get() || !self.session.ensure_eager_family(family) {
            return false;
        }
        let mut state = self.session.state.borrow_mut();
        let eager = state
            .eager
            .get_mut(family)
            .expect("bounded eager family was registered");
        eager.calls += 1;
        eager.hits += 1;
        eager.completed += 1;
        true
    }

    pub(in crate::check::checker) fn start_ready_group_application(
        &self,
        group: TypeGroupId,
        application: TypeId,
    ) -> Option<ReadyApplicationAttribution> {
        let family = self.session.state.borrow().families.get(&group).cloned();
        if let Some(family) = family {
            Some(self.start_ready_application(family.0, Some(application)))
        } else {
            self.session.state.borrow_mut().unregistered_family_lookups += 1;
            None
        }
    }

    fn start_ready_application(
        &self,
        family: String,
        application: Option<TypeId>,
    ) -> ReadyApplicationAttribution {
        let started = self.session.clock.now_us();
        let enabled = self.session.ensure_eager_family(&family);
        let application = (self.session.config.mode == AttributionMode::Exact)
            .then_some(application)
            .flatten();
        let previous = if enabled {
            let active = ActiveApplicationContext {
                session: Rc::downgrade(&self.session),
                family: family.clone(),
                application,
            };
            let previous = ACTIVE_APPLICATION.with(|current| current.replace(Some(active)));
            let mut state = self.session.state.borrow_mut();
            let eager = state
                .eager
                .get_mut(&family)
                .expect("bounded eager family was registered");
            eager.calls += 1;
            eager.active += 1;
            eager.active_started_us = Some(started);
            previous
        } else {
            None
        };
        ReadyApplicationAttribution {
            session: Rc::downgrade(&self.session),
            family,
            application,
            started_us: started,
            previous: RefCell::new(previous),
            finished: Cell::new(false),
            enabled,
            _thread_affine: std::marker::PhantomData,
        }
    }
}

pub(in crate::check::checker) struct ReadyApplicationAttribution {
    session: Weak<AttributionSession>,
    family: String,
    application: Option<TypeId>,
    started_us: u64,
    previous: RefCell<Option<ActiveApplicationContext>>,
    finished: Cell<bool>,
    enabled: bool,
    _thread_affine: std::marker::PhantomData<Rc<()>>,
}

impl ReadyApplicationAttribution {
    pub(super) fn capture_substitution_for_test(
        &self,
        map: &[(TypeParamId, TypeId)],
        application: Option<TypeId>,
    ) -> SubstitutionAttribution {
        let session = self
            .session
            .upgrade()
            .expect("live test application retains its attribution session");
        SubstitutionAttribution {
            run: session.new_run(&self.family, map, application.or(self.application)),
        }
    }

    pub(super) fn finish_miss_tainted_for_test(&self) {
        self.finish(false);
    }

    pub(in crate::check::checker) fn finish_clean(&self) {
        self.finish(true);
    }

    pub(in crate::check::checker) fn finish_tainted(&self) {
        self.finish(false);
    }

    fn finish(&self, clean: bool) {
        if self.finished.replace(true) {
            return;
        }
        if !self.enabled {
            return;
        }
        ACTIVE_APPLICATION.with(|current| {
            current.replace(self.previous.borrow_mut().take());
        });
        let Some(session) = self
            .session
            .upgrade()
            .filter(|session| session.active.get())
        else {
            return;
        };
        let elapsed = session.clock.now_us().saturating_sub(self.started_us);
        let mut state = session.state.borrow_mut();
        let Some(eager) = state.eager.get_mut(&self.family) else {
            return;
        };
        eager.active = eager.active.saturating_sub(1);
        eager.active_started_us = None;
        eager.misses += 1;
        eager.completed += 1;
        eager.completed_us = eager.completed_us.saturating_add(elapsed);
        if clean {
            eager.clean += 1;
        } else {
            eager.tainted += 1;
        }
    }
}

impl Drop for ReadyApplicationAttribution {
    fn drop(&mut self) {
        self.finish(false);
    }
}

#[derive(Default)]
struct ExactRunState {
    map: Arc<[(u32, u32)]>,
    application: Option<TypeId>,
    states: Vec<ExactStateSnapshot>,
    state_ids: BTreeMap<u32, BTreeMap<Arc<[u32]>, u64>>,
    events: Vec<ExactEventSnapshot>,
    checkpoint_state_cursor: usize,
    checkpoint_event_cursor: usize,
    stack: Vec<(u64, Option<u64>, u64)>,
    next_visit: u64,
    saturated: bool,
}

enum RunDetail {
    Progress,
    Exact(RefCell<ExactRunState>),
}

struct RunCapture {
    session: Rc<AttributionSession>,
    id: u64,
    family: String,
    mode: AttributionMode,
    checkpoint_visits: u64,
    next_checkpoint: Cell<u64>,
    checkpoint_due: Cell<bool>,
    pending_open_trace: Cell<bool>,
    visits: Cell<u64>,
    memo_hits: Cell<u64>,
    cycle_reentries: Cell<u64>,
    tainted_ancestors: Cell<u64>,
    completed: Cell<bool>,
    detail: RunDetail,
}

impl RunCapture {
    fn application(&self) -> Option<TypeId> {
        match &self.detail {
            RunDetail::Progress => None,
            RunDetail::Exact(exact) => exact.borrow().application,
        }
    }

    fn checkpoint_delta_estimated_bytes(&self) -> usize {
        let exact_bytes = match &self.detail {
            RunDetail::Progress => 0,
            RunDetail::Exact(exact) => {
                let exact = exact.borrow();
                let states = exact.states[exact.checkpoint_state_cursor..].iter().fold(
                    0_usize,
                    |total, state| {
                        total
                            .saturating_add(96)
                            .saturating_add(state.context.len().saturating_mul(4))
                            .saturating_add(state.map.len().saturating_mul(8))
                            .saturating_add(application_len(state.application, &state.map))
                    },
                );
                states.saturating_add(
                    exact
                        .events
                        .len()
                        .saturating_sub(exact.checkpoint_event_cursor)
                        .saturating_mul(64),
                )
            }
        };
        96_usize
            .saturating_add(self.family.len())
            .saturating_add(exact_bytes)
    }

    fn checkpoint_delta(&self) -> RunSnapshot {
        let (states, events) = match &self.detail {
            RunDetail::Progress => (Vec::new(), Vec::new()),
            RunDetail::Exact(exact) => {
                let mut exact = exact.borrow_mut();
                let states = exact.states[exact.checkpoint_state_cursor..].to_vec();
                let events = exact.events[exact.checkpoint_event_cursor..].to_vec();
                exact.checkpoint_state_cursor = exact.states.len();
                exact.checkpoint_event_cursor = exact.events.len();
                (states, events)
            }
        };
        RunSnapshot {
            id: self.id,
            family: self.family.clone(),
            completed: self.completed.get(),
            visits: self.visits.get(),
            memo_hits: self.memo_hits.get(),
            cycle_reentries: self.cycle_reentries.get(),
            tainted_ancestors: self.tainted_ancestors.get(),
            states,
            events,
        }
    }

    fn record_progress_visit(&self) -> bool {
        if !self.session.active.get() || self.session.coverage_lost.get() {
            return false;
        }
        let visits = self.visits.get().saturating_add(1);
        self.visits.set(visits);
        if visits >= self.next_checkpoint.get() {
            self.checkpoint_due.set(true);
            self.next_checkpoint.set(
                self.next_checkpoint
                    .get()
                    .saturating_add(self.checkpoint_visits),
            );
        }
        true
    }

    fn emit_due_checkpoint(&self, has_open_stack: bool) {
        if self.checkpoint_due.replace(false) {
            self.session.checkpoint_nonblocking(self);
            self.pending_open_trace.set(has_open_stack);
        }
    }

    fn enter(
        &self,
        ty: TypeId,
        blocked: impl ExactSizeIterator<Item = TypeParamId>,
    ) -> Option<AttributionVisit> {
        if !self.record_progress_visit() {
            return None;
        }
        if self.mode != AttributionMode::Exact {
            return Some(AttributionVisit {
                visit: 0,
                parent: None,
                state: 0,
            });
        }
        let RunDetail::Exact(exact) = &self.detail else {
            return None;
        };
        let session = &self.session;
        let context_entries = blocked.len();
        let state_bytes = {
            let exact = exact.borrow();
            96_usize
                .saturating_add(context_entries.saturating_mul(4))
                .saturating_add(exact.map.len().saturating_mul(8))
                .saturating_add(application_len(exact.application, &exact.map))
        };
        if !session.preflight_exact_visit(context_entries, state_bytes, 64) {
            exact.borrow_mut().saturated = true;
            return None;
        }
        let mut context = blocked.map(|parameter| parameter.0).collect::<Vec<_>>();
        context.sort_unstable();
        context.dedup();
        let mut exact = exact.borrow_mut();
        let state = if let Some(state) = exact
            .state_ids
            .get(&ty.0)
            .and_then(|contexts| contexts.get(context.as_slice()))
            .copied()
        {
            state
        } else {
            let application_len = application_len(exact.application, &exact.map);
            let state_bytes = 96_usize
                .saturating_add(context.len().saturating_mul(4))
                .saturating_add(exact.map.len().saturating_mul(8))
                .saturating_add(application_len);
            if !session.reserve_exact_state(context.len(), state_bytes) {
                exact.saturated = true;
                return None;
            }
            let state = u64::try_from(exact.states.len() + 1)
                .expect("bounded exact dictionary length fits u64");
            let state_map = Arc::clone(&exact.map);
            let application = exact.application;
            let context: Arc<[u32]> = context.into();
            exact
                .state_ids
                .entry(ty.0)
                .or_default()
                .insert(Arc::clone(&context), state);
            exact.states.push(ExactStateSnapshot {
                id: state,
                type_id: ty.0,
                context,
                map: state_map,
                application,
                saturated: false,
            });
            state
        };
        exact.next_visit += 1;
        let visit = exact.next_visit;
        let parent = exact.stack.last().map(|frame| frame.0);
        exact.stack.push((visit, parent, state));
        if !push_exact_event(
            session,
            &mut exact,
            EventAction::Enter,
            visit,
            parent,
            state,
            None,
        ) {
            exact.stack.pop();
            return None;
        }
        Some(AttributionVisit {
            visit,
            parent,
            state,
        })
    }

    fn finish_visit(&self, visit: AttributionVisit, disposition: EventDisposition) {
        match disposition {
            EventDisposition::CompletedMemoHit => {
                self.memo_hits.set(self.memo_hits.get().saturating_add(1));
            }
            EventDisposition::RawCycleReentry => {
                self.cycle_reentries
                    .set(self.cycle_reentries.get().saturating_add(1));
            }
            EventDisposition::Tainted => {
                self.tainted_ancestors
                    .set(self.tainted_ancestors.get().saturating_add(1));
            }
            EventDisposition::Clean => {}
        }
        if self.mode != AttributionMode::Exact {
            self.emit_due_checkpoint(false);
            return;
        }
        if !self.session.active.get() {
            return;
        }
        let RunDetail::Exact(exact) = &self.detail else {
            return;
        };
        let session = &self.session;
        let mut exact = exact.borrow_mut();
        if !push_exact_event(
            session,
            &mut exact,
            EventAction::Outcome,
            visit.visit,
            visit.parent,
            visit.state,
            Some(disposition),
        ) {
            return;
        }
        if !push_exact_event(
            session,
            &mut exact,
            EventAction::Exit,
            visit.visit,
            visit.parent,
            visit.state,
            None,
        ) {
            return;
        }
        let popped = exact.stack.pop();
        if popped != Some((visit.visit, visit.parent, visit.state)) {
            exact.saturated = true;
            self.session.lose_coverage();
        }
        let has_open_stack = !exact.stack.is_empty();
        drop(exact);
        if self.checkpoint_due.get() {
            self.emit_due_checkpoint(has_open_stack);
        } else if !has_open_stack && self.pending_open_trace.replace(false) {
            self.session.checkpoint_nonblocking(self);
        }
    }

    fn snapshot(&self) -> RunSnapshot {
        let (states, events) = match &self.detail {
            RunDetail::Progress => (Vec::new(), Vec::new()),
            RunDetail::Exact(exact) => {
                let exact = exact.borrow();
                (exact.states.clone(), exact.events.clone())
            }
        };
        RunSnapshot {
            id: self.id,
            family: self.family.clone(),
            completed: self.completed.get(),
            visits: self.visits.get(),
            memo_hits: self.memo_hits.get(),
            cycle_reentries: self.cycle_reentries.get(),
            tainted_ancestors: self.tainted_ancestors.get(),
            states,
            events,
        }
    }
}

impl Drop for RunCapture {
    fn drop(&mut self) {
        if self.session.active.get() {
            self.completed.set(true);
            self.session
                .state
                .borrow_mut()
                .completed_runs
                .push(self.snapshot());
        }
    }
}

fn push_exact_event(
    session: &AttributionSession,
    exact: &mut ExactRunState,
    action: EventAction,
    visit: u64,
    parent: Option<u64>,
    state: u64,
    disposition: Option<EventDisposition>,
) -> bool {
    if !session.reserve_exact_event(64) {
        exact.saturated = true;
        return false;
    }
    exact.events.push(ExactEventSnapshot {
        event: u64::try_from(exact.events.len() + 1).expect("bounded event length fits u64"),
        action,
        visit,
        parent,
        state,
        disposition,
        at_us: session.clock.now_us(),
    });
    true
}

#[derive(Clone, Copy)]
pub(crate) struct AttributionVisit {
    visit: u64,
    parent: Option<u64>,
    state: u64,
}

pub(crate) struct SubstitutionAttribution {
    run: Option<Rc<RunCapture>>,
}

impl Drop for SubstitutionAttribution {
    fn drop(&mut self) {
        if let Some(run) = &self.run {
            run.completed.set(true);
        }
    }
}

impl SubstitutionAttribution {
    fn enabled(run: Rc<RunCapture>) -> Self {
        Self { run: Some(run) }
    }

    pub(super) fn record_visit_for_test(&self, ty: TypeId, blocked: &[TypeParamId]) {
        let Some(run) = &self.run else {
            return;
        };
        if let Some(visit) = run.enter(ty, blocked.iter().copied()) {
            run.finish_visit(visit, EventDisposition::Clean);
        }
    }

    pub(super) fn enter_visit_for_test(
        &self,
        ty: TypeId,
        blocked: &[TypeParamId],
    ) -> Option<AttributionVisit> {
        self.run.as_ref()?.enter(ty, blocked.iter().copied())
    }

    pub(super) fn finish_cycle_visit_for_test(&self, visit: AttributionVisit) {
        if let Some(run) = &self.run {
            run.finish_visit(visit, EventDisposition::RawCycleReentry);
        }
    }

    pub(super) fn finish_clean_visit_for_test(&self, visit: AttributionVisit) {
        if let Some(run) = &self.run {
            run.finish_visit(visit, EventDisposition::Clean);
        }
    }

    pub(super) fn finish_tainted_visit_for_test(&self, visit: AttributionVisit) {
        if let Some(run) = &self.run {
            run.finish_visit(visit, EventDisposition::Tainted);
        }
    }

    pub(super) fn capture_nested_substitution_for_test(
        &self,
        map: &[(TypeParamId, TypeId)],
        application: Option<TypeId>,
    ) -> Self {
        let Some(run) = &self.run else {
            return Self { run: None };
        };
        let session = run.session.clone();
        Self {
            run: session.new_run(&run.family, map, application.or(run.application())),
        }
    }

    pub(super) fn finish_clean_for_test(&self) {
        if let Some(run) = &self.run {
            run.completed.set(true);
        }
    }

    pub(super) fn finish_tainted_for_test(&self) {
        if let Some(run) = &self.run {
            run.completed.set(true);
        }
    }

    pub(super) fn session_process_for_test(&self) -> u8 {
        self.run
            .as_ref()
            .expect("test substitution attribution is enabled")
            .session
            .config
            .process
    }

    pub(crate) fn enter_visit(
        &self,
        ty: TypeId,
        blocked: &rustc_hash::FxHashSet<TypeParamId>,
    ) -> Option<AttributionVisit> {
        self.run.as_ref()?.enter(ty, blocked.iter().copied())
    }

    pub(crate) fn finish_clean_visit(&self, visit: AttributionVisit) {
        if let Some(run) = &self.run {
            run.finish_visit(visit, EventDisposition::Clean);
        }
    }

    pub(crate) fn finish_memo_visit(&self, visit: AttributionVisit) {
        if let Some(run) = &self.run {
            run.finish_visit(visit, EventDisposition::CompletedMemoHit);
        }
    }

    pub(crate) fn finish_cycle_visit(&self, visit: AttributionVisit) {
        if let Some(run) = &self.run {
            run.finish_visit(visit, EventDisposition::RawCycleReentry);
        }
    }

    pub(crate) fn finish_tainted_visit(&self, visit: AttributionVisit) {
        if let Some(run) = &self.run {
            run.finish_visit(visit, EventDisposition::Tainted);
        }
    }

    pub(crate) fn finish_run(&self) {
        if let Some(run) = &self.run {
            run.completed.set(true);
        }
    }
}

pub(super) fn resolve_mode_from_values_for_test(
    mode: Option<&str>,
    progress_path: Option<&str>,
) -> AttributionMode {
    if progress_path.is_none() {
        return AttributionMode::Off;
    }
    mode.and_then(AttributionMode::parse)
        .unwrap_or(AttributionMode::Off)
}

pub(super) fn current_session_for_test() -> Option<AttributionControl> {
    CURRENT_SESSION.with(|current| {
        current
            .borrow()
            .as_ref()
            .filter(|session| session.active.get())
            .map(|session| AttributionControl {
                session: Rc::clone(session),
            })
    })
}

pub(super) fn capture_pass_for_test() -> Option<PassAttribution> {
    CURRENT_SESSION.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(capture_pass_from_session)
    })
}

pub(super) fn capture_substitution_for_test() -> Option<SubstitutionAttribution> {
    capture_substitution_from_active(&FxHashMap::default())
}

pub(super) fn start_attribution_for_test(
    config: AttributionConfig,
    clock: &AttributionTestClock,
    sink: &AttributionTestSink,
) -> Result<AttributionScope, String> {
    start_attribution(
        config,
        AttributionClock::Test(clock.clone()),
        ReporterSink::Test(sink.clone()),
        test_identities(config),
    )
}

fn start_attribution(
    config: AttributionConfig,
    clock: AttributionClock,
    sink: ReporterSink,
    identities: SessionIdentities,
) -> Result<AttributionScope, String> {
    if config.mode == AttributionMode::Off {
        return Err("Off mode has no attribution scope".to_owned());
    }
    if config.process == 0
        || config.interval_ms == 0
        || config.checkpoint_visits == 0
        || config.evidence_window_ms == 0
        || (config.mode == AttributionMode::Exact) != config.universe.is_some()
        || config.limits.checkpoint_messages == 0
        || config.limits.terminal_reserve_lines < 2
        || config.limits.terminal_reserve_bytes < 2 * config.limits.rendered_line_bytes
        || [
            &identities.session,
            &identities.binary,
            &identities.host,
            &identities.workload_profile,
        ]
        .into_iter()
        .any(|identity| !opaque(identity))
    {
        return Err("invalid WU0C attribution configuration".to_owned());
    }
    let (sender, receiver) =
        mpsc::sync_channel(config.limits.checkpoint_messages.saturating_add(8));
    let (ready, wait_until_ready) = mpsc::channel();
    let reporter_thread = Arc::new(Mutex::new(None));
    let reporter_thread_owner = Arc::clone(&reporter_thread);
    let checkpoint_queue = Arc::new(Mutex::new(CheckpointQueueState::default()));
    let reporter_checkpoint_queue = Arc::clone(&checkpoint_queue);
    let reporter_pause = Arc::new((Mutex::new(false), Condvar::new()));
    let reporter_pause_owner = Arc::clone(&reporter_pause);
    let phase_slot = Arc::new(Mutex::new(None));
    let reporter_phase_slot = Arc::clone(&phase_slot);
    let reporter_clock = clock.clone();
    let reporter = std::thread::Builder::new()
        .name("typokat-wu0c-reporter".to_owned())
        .spawn(move || {
            reporter_loop(
                config,
                identities,
                reporter_clock,
                sink,
                receiver,
                ReporterSharedState {
                    checkpoint_queue: reporter_checkpoint_queue,
                    pause: reporter_pause_owner,
                    phase_slot: reporter_phase_slot,
                    thread: reporter_thread_owner,
                },
                ready,
            )
        })
        .map_err(|error| format!("cannot start WU0C reporter: {error}"))?;
    wait_until_ready
        .recv()
        .map_err(|_| "WU0C reporter exited before initialization".to_owned())?;
    let session = Rc::new(AttributionSession {
        config,
        clock,
        active: Cell::new(true),
        coverage_lost: Cell::new(false),
        state: RefCell::new(SemanticState::default()),
        sender,
        checkpoint_queue,
        reporter_pause,
        phase_slot,
        coalesced_phase_updates: Cell::new(0),
        last_coalesced_phase_updates: Cell::new(0),
        reporter_thread,
    });
    let previous = CURRENT_SESSION.with(|current| current.replace(Some(Rc::clone(&session))));
    Ok(AttributionScope {
        session,
        previous,
        reporter: Some(reporter),
        _thread_affine: std::marker::PhantomData,
    })
}

fn test_identities(config: AttributionConfig) -> SessionIdentities {
    let seed = format!(
        "test-session:{}:{}:{:?}",
        config.process,
        config.mode.render(),
        std::thread::current().id()
    );
    SessionIdentities {
        session: hex_sha256(seed.as_bytes()),
        binary: hex_sha256(b"typokat-wu0c-test-binary"),
        host: hex_sha256(b"typokat-wu0c-test-host"),
        workload_profile: hex_sha256(b"typokat-wu0c-test-workload-profile"),
    }
}

fn default_limits() -> AttributionLimits {
    AttributionLimits {
        lines: 65_536,
        eager_keys: 4_096,
        runs: 262_144,
        dictionary_entries: 1_048_576,
        trace_events: 4_194_304,
        checkpoint_messages: 64,
        checkpoint_bytes: 8_388_608,
        rendered_line_bytes: 65_536,
        file_bytes: 134_217_728,
        map_entries: 4_096,
        context_entries: 4_096,
        application_entries: 4_096,
        live_exact_bytes: 67_108_864,
        terminal_reserve_lines: 2,
        terminal_reserve_bytes: 131_072,
    }
}

struct ReleaseAttributionConfig {
    path: PathBuf,
    config: AttributionConfig,
    identities: SessionIdentities,
}

fn release_value<'a>(values: &'a [(&str, &str)], name: &str) -> Result<&'a str, String> {
    let mut matches = values
        .iter()
        .filter_map(|(candidate, value)| (*candidate == name).then_some(*value));
    let value = matches
        .next()
        .ok_or_else(|| format!("missing required WU0C release value {name}"))?;
    if matches.next().is_some() {
        return Err(format!("duplicate WU0C release value {name}"));
    }
    Ok(value)
}

fn resolve_release_config_from_values(
    values: &[(&str, &str)],
) -> Result<ReleaseAttributionConfig, String> {
    let path = release_value(values, "TYPOKAT_WU0C_PROGRESS_PATH")?;
    if path.is_empty() {
        return Err("WU0C progress path is empty".to_owned());
    }
    let mode = AttributionMode::parse(release_value(values, "TYPOKAT_WU0C_MODE")?)
        .filter(|mode| *mode != AttributionMode::Off)
        .ok_or_else(|| "invalid WU0C release mode".to_owned())?;
    let process = release_value(values, "TYPOKAT_WU0B_PROCESS")?
        .parse::<u8>()
        .map_err(|_| "invalid WU0C release process".to_owned())?;
    if !(1..=5).contains(&process) {
        return Err("WU0C release process is outside 1..=5".to_owned());
    }
    let universe = if mode == AttributionMode::Exact {
        let universe = release_value(values, "TYPOKAT_WU0C_UNIVERSE")?
            .parse::<u64>()
            .map_err(|_| "invalid WU0C exact universe".to_owned())?;
        if universe == 0 {
            return Err("WU0C exact universe must be nonzero".to_owned());
        }
        Some(universe)
    } else {
        None
    };
    let identities = SessionIdentities {
        session: release_value(values, "TYPOKAT_WU0C_SESSION_SHA256")?.to_owned(),
        binary: release_value(values, "TYPOKAT_WU0C_BINARY_SHA256")?.to_owned(),
        host: release_value(values, "TYPOKAT_WU0C_HOST_SHA256")?.to_owned(),
        workload_profile: release_value(values, "TYPOKAT_WU0C_WORKLOAD_PROFILE_SHA256")?.to_owned(),
    };
    if [
        &identities.session,
        &identities.binary,
        &identities.host,
        &identities.workload_profile,
    ]
    .into_iter()
    .any(|identity| !opaque(identity))
    {
        return Err("invalid WU0C release identity".to_owned());
    }
    Ok(ReleaseAttributionConfig {
        path: PathBuf::from(path),
        config: AttributionConfig {
            process,
            universe,
            mode,
            interval_ms: 250,
            checkpoint_visits: 4_096,
            evidence_window_ms: 5_000,
            limits: default_limits(),
        },
        identities,
    })
}

pub(super) fn resolve_release_config_from_values_for_test(
    values: &[(&str, &str)],
) -> Result<(), String> {
    resolve_release_config_from_values(values).map(|_| ())
}

pub(super) fn start_wu0c_attribution_from_env() -> Option<AttributionScope> {
    std::env::var_os("TYPOKAT_WU0C_PROGRESS_PATH")?;
    const NAMES: [&str; 8] = [
        "TYPOKAT_WU0C_PROGRESS_PATH",
        "TYPOKAT_WU0C_MODE",
        "TYPOKAT_WU0B_PROCESS",
        "TYPOKAT_WU0C_UNIVERSE",
        "TYPOKAT_WU0C_SESSION_SHA256",
        "TYPOKAT_WU0C_BINARY_SHA256",
        "TYPOKAT_WU0C_HOST_SHA256",
        "TYPOKAT_WU0C_WORKLOAD_PROFILE_SHA256",
    ];
    let owned = NAMES
        .into_iter()
        .filter_map(|name| std::env::var(name).ok().map(|value| (name, value)))
        .collect::<Vec<_>>();
    let borrowed = owned
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect::<Vec<_>>();
    let resolved = resolve_release_config_from_values(&borrowed).ok()?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(resolved.path)
        .ok()?;
    start_attribution(
        resolved.config,
        AttributionClock::Real(Instant::now()),
        ReporterSink::File(BufWriter::new(file)),
        resolved.identities,
    )
    .ok()
}

pub(in crate::check::checker) fn capture_wu0c_pass_attribution() -> Option<PassAttribution> {
    CURRENT_SESSION.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(capture_pass_from_session)
    })
}

pub(crate) fn capture_wu0c_substitution_attribution(
    map: &FxHashMap<TypeParamId, TypeId>,
) -> Option<SubstitutionAttribution> {
    capture_substitution_from_active(map)
}

fn capture_substitution_from_active(
    map: &FxHashMap<TypeParamId, TypeId>,
) -> Option<SubstitutionAttribution> {
    ACTIVE_APPLICATION.with(|active| {
        let active = active.borrow();
        let active = active.as_ref()?;
        let session = active
            .session
            .upgrade()
            .filter(|session| session.active.get())?;
        let run = if session.config.mode == AttributionMode::Exact {
            session.new_run_from_hash_map(&active.family, map, active.application)?
        } else {
            session.new_run(&active.family, &[], None)?
        };
        Some(SubstitutionAttribution::enabled(run))
    })
}

pub(in crate::check::checker) fn start_wu0c_ready_application_attribution(
    pass: &Option<PassAttribution>,
    group: TypeGroupId,
    application: TypeId,
) -> Option<ReadyApplicationAttribution> {
    pass.as_ref()?
        .start_ready_group_application(group, application)
}

pub(in crate::check::checker) fn register_wu0c_family_tokens(binder: &Binder) {
    CURRENT_SESSION.with(|current| {
        let current = current.borrow();
        let Some(session) = current
            .as_ref()
            .filter(|session| session.active.get() && session.config.mode.captures_semantics())
        else {
            return;
        };
        let mut registrations = Vec::new();
        for group in binder.type_groups.iter() {
            let participants = group
                .fragments
                .iter()
                .filter_map(|fragment| {
                    let CompilationOrigin::Library(file_ordinal) = binder
                        .namespaces
                        .compilation_origin_for_source(fragment.source)?
                    else {
                        return None;
                    };
                    Some(FamilyParticipant::new(
                        file_ordinal,
                        fragment.site.declaration_span.start,
                        fragment.kind,
                    ))
                })
                .collect::<Vec<_>>();
            if !participants.is_empty() {
                registrations.push((group.id, canonical_family_token(&participants)));
            }
        }
        let mut state = session.state.borrow_mut();
        for (group, token) in registrations {
            state.families.insert(group, token);
        }
        state.last_semantic_thread = Some(std::thread::current().id());
        drop(state);
        session.enter_phase(AttributionPhase::ReserveFill);
    });
}

pub(in crate::check::checker) fn enter_wu0c_phase(phase: AttributionPhase) {
    CURRENT_SESSION.with(|current| {
        if let Some(session) = current.borrow().as_ref() {
            session.enter_phase(phase);
        }
    });
}
