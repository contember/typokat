//! Disabled RED acceptance spec for the measurement-only WU0C attribution spike.
//!
//! Activate only with `#[cfg(test)] mod wu0c_attribution;` and this sibling module in
//! `checker/mod.rs`. WU0C is finite, test-only, and unreachable from driver defaults/public APIs.
//! No environment means `Off`, so the ordinary ignored WU0B probe is byte-for-byte unaffected.
//! `ReporterControl` opens `TYPOKAT_WU0C_PROGRESS_PATH` and runs the reporter without semantic
//! capture. `Progress` adds low-overhead attribution. `Exact` is a separate expensive run.
//!
//! The scope is installed on the same libtest thread immediately before `run_injected_profile`;
//! it is not inherited by `check_source`'s worker. A thread-local `Rc` owner is captured once by
//! each `Pass` and `Substitution`. Nested/unwound scopes restore the prior owner; an already-captured
//! handle never retargets to an ambient nested scope. Dropped-session handles become inert.
//!
//! ## Collection and progress file
//!
//! Each `Substitution` owns plain local counters. Every 4,096 visits it attempts one cumulative
//! checkpoint to a bounded channel. The externally visible checkpoint cadence is tested here;
//! absence of locks, allocation, TLS/atomic/clock work, formatting, and exact-key construction on
//! the visit path is a mandatory implementation-review and compiler-inspection gate, not something
//! the instrumented path may self-report. A failed checkpoint sets a coverage-loss flag. The
//! reporter writes one buffered batch and one flush per
//! 250-ms heartbeat/checkpoint/terminal action—not one flush per line. The heartbeat distinguishes
//! reporter elapsed time from last accepted semantic-checkpoint elapsed time and carries the
//! active exclusive family/time at that checkpoint. A killed process retains prior flushed batches;
//! no claim is made that the semantic thread can flush an invalid line before the kill.
//!
//! A Ready application is the sole family-time frame. Substitution runs inherit that family and
//! own counters/trace, but never add another family elapsed-time charge; nested substitutions do
//! not double count. Eager Ready applications cannot nest on the real path. Arithmetic is exact:
//! `calls = hits + misses + active`, `misses = clean + tainted`, and
//! `completed = hits + misses`. Completed exclusive elapsed excludes active work. Each run has
//! `started = completed + active` with `active` in `0..=1`.
//!
//! Low mode reports only an opaque stable family token. After bind, WU0B pre-registers
//! `TypeGroupId -> token`; hot paths never derive it. The token is SHA-256 over
//! `u32be(domain_len) || domain || u32be(participant_count) ||` each sorted participant as
//! `u32be(9) || u32be(LibraryFileOrdinal) || u32be(declaration_span.start) || kind_u8`, where the
//! domain is `typokat-wu0c-family-v1` and stable codes are Interface=1, TypeAlias=2, Class=3. The
//! full merged participant tuple is included. Names, paths, source text, and runtime type/group ids
//! are forbidden.
//!
//! Exact mode adds a process-local universe id, bounded state dictionary, and per-run ordered
//! `enter -> outcome -> exit` events. Each event carries an event ordinal, visit id, parent visit
//! id, dictionary id, timestamp, and outcome disposition (`clean`, `completed_memo_hit`,
//! `raw_cycle_reentry`, or `tainted`). The validator checks stack nesting, parent links, dictionary
//! completeness, one outcome/exit per enter, and monotonic timestamps. This is replay input for the
//! 75% exact-repeat and 80% predicted-repeat-removal gates only; timing improvement comes from
//! actual matched A/B release runs.
//!
//! All limits are explicit: lines, eager keys, runs, dictionary entries, trace events, queued
//! checkpoint messages/bytes, rendered line bytes, total file bytes, map/context/application entry
//! counts, and live exact bytes. Reporter-owned capacity reserves one invalid and one finish line.
//! Saturation, coverage loss, or any exceeded limit rejects evidence. Enforcing collector limits
//! before allocation, checkpoint limits before enqueue, and sending bounded delta/coalesced payloads
//! rather than cumulative clones remain independent implementation/compiler-review gates; raw
//! runtime counts below exercise the paths but do not self-certify those properties.
//!
//! ## Evidence
//!
//! `validate_session_evidence` derives the sample; callers cannot supply trust booleans. It requires
//! a strict v1 parse, contiguous sequences, one capability/mode/process/universe, zero invalid /
//! saturation / coverage loss, bounded heartbeat and checkpoint age, and complete replay structure.
//! The validated evidence retains opaque session, prebuilt-binary, host, and workload/profile
//! identities from the session header. Normal termination requires a clean finish. A five-second
//! kill is accepted only with explicit external `Termination::Deadline` and a valid contiguous
//! prefix through the last clean heartbeat; its timing must satisfy
//! `0 < reserve_fill_us <= checkpoint_elapsed_us <= elapsed_us`. Valid activity after that
//! heartbeat is outside the evidence window. Missing/malformed fields anywhere still reject.
//!
//! Five validated exact samples must name the same cycle-tainted family and each attribute at least
//! 50% of reserve/fill exclusive time and visits to it, with at least 75% repeated exact visits.
//! Replay must predict at least 80% repeat removal. Five matched A/B pairs must have a median of the
//! five per-pair improvement ratios of at least 20%; non-cycle and progress/reporter-control pairs
//! use the same paired-ratio method with at most 2% median regression. Matching pins process,
//! variant/mode, phase, opaque binary/host identities, termination, and reserve/fill evidence. A
//! deadline baseline contributes only its last-clean-checkpoint lower bound; the candidate must
//! complete and prove the 20% gain against that bound. Authorization binds each A/B baseline to the
//! corresponding validated exact-process lower bound and requires all three matched sets to retain
//! the same prebuilt-binary and host identities. Runs are uncontended with warm filesystem cache.
//! Otherwise WU0 remains NO-GO and WU1 PENDING. Unit tests assert arithmetic and record binding
//! only; they neither measure live wall-clock performance nor establish that Candidate B exists.

use super::wu0c_attribution::{
    canonical_family_token, parse_attribution_line, replay_exact_trace,
    resolve_mode_from_values_for_test, resolve_release_config_from_values_for_test,
    start_attribution_for_test, validate_session_evidence, AttributionConfig, AttributionLimits,
    AttributionLine, AttributionMode, AttributionPhase, AttributionTestClock, AttributionTestSink,
    FamilyParticipant, LimitKind, Termination, ValidatedSessionEvidence,
};
use crate::binder::declaration::TypeFragmentKind;
use crate::check::checker::wu0b_library::{run_injected_profile, InjectedLibrarySource};
use crate::driver::check_source;
use crate::source::LibraryFileOrdinal;
use crate::types::repr::TypeParamId;
use crate::types::store::TypeId;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

const PREFIX: &str = "typokat-wu0c-attribution-v1";
const FAMILY: &str = "d2fc0cfa7ac50102b2e63833c7796b799df9624b43d5ddb7ac007f1b7972ee54";
const MAP: &str = "7:11,9:13";
const MAP_SHA: &str = "ae219c29ce76dd5c300ec075eb78b9b16ab6bb46a043be21ea1e5333d11387aa";
const APPLICATION: &str = "41|7:11,9:13";
const APPLICATION_SHA: &str = "cc5c98b4f99d962cb822d64ad6ac266f08d3dae6c3551c98b351c69d16195e2d";
const MERGED_INTERFACE_FAMILY: &str =
    "9c3d9c271d267440c318fb152ddd4b3124e613279057cb2c5bac978a8f8608bd";
const BINARY_IDENTITY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const HOST_IDENTITY: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const WORKLOAD_PROFILE_IDENTITY: &str =
    "5555555555555555555555555555555555555555555555555555555555555555";
const RELEASE_SESSION_IDENTITY: &str =
    "8888888888888888888888888888888888888888888888888888888888888888";

fn exact_session_identity(process: u8) -> String {
    format!("{:064x}", 10_000_u64 + u64::from(process))
}

fn protocol_fragment_kind_code(kind: TypeFragmentKind) -> u8 {
    match kind {
        TypeFragmentKind::Interface => 1,
        TypeFragmentKind::TypeAlias => 2,
        TypeFragmentKind::Class => 3,
    }
}

fn limits() -> AttributionLimits {
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

fn tiny_limits(limit: LimitKind) -> AttributionLimits {
    let mut tiny = AttributionLimits {
        lines: 64,
        eager_keys: 32,
        runs: 32,
        dictionary_entries: 32,
        trace_events: 128,
        checkpoint_messages: 8,
        checkpoint_bytes: 32_768,
        rendered_line_bytes: 2_048,
        file_bytes: 65_536,
        map_entries: 32,
        context_entries: 512,
        application_entries: 32,
        live_exact_bytes: 65_536,
        terminal_reserve_lines: 2,
        terminal_reserve_bytes: 4_096,
    };
    match limit {
        LimitKind::Lines => tiny.lines = 5,
        LimitKind::EagerKeys => tiny.eager_keys = 1,
        LimitKind::Runs => tiny.runs = 1,
        LimitKind::DictionaryEntries => tiny.dictionary_entries = 1,
        LimitKind::TraceEvents => tiny.trace_events = 3,
        LimitKind::CheckpointMessages => tiny.checkpoint_messages = 1,
        LimitKind::CheckpointBytes => tiny.checkpoint_bytes = 1,
        LimitKind::RenderedLineBytes => tiny.rendered_line_bytes = 1_024,
        LimitKind::FileBytes => tiny.file_bytes = 8_192,
        LimitKind::MapEntries => tiny.map_entries = 1,
        LimitKind::ContextEntries => tiny.context_entries = 1,
        LimitKind::ApplicationEntries => tiny.application_entries = 1,
        LimitKind::LiveExactBytes => tiny.live_exact_bytes = 512,
    }
    tiny.terminal_reserve_bytes = 2 * tiny.rendered_line_bytes;
    tiny
}

fn config(mode: AttributionMode, process: u8) -> AttributionConfig {
    AttributionConfig {
        process,
        universe: (mode == AttributionMode::Exact).then_some(1),
        mode,
        interval_ms: 250,
        checkpoint_visits: 4_096,
        evidence_window_ms: 5_000,
        limits: limits(),
    }
}

fn parse(line: &str) -> AttributionLine {
    let parsed = parse_attribution_line(line).expect("pinned v1 line parses");
    assert_eq!(parsed.render(), line);
    parsed
}

#[test]
fn concrete_family_token_canonicalizes_every_merged_participant() {
    assert_eq!(protocol_fragment_kind_code(TypeFragmentKind::Interface), 1);
    assert_eq!(protocol_fragment_kind_code(TypeFragmentKind::TypeAlias), 2);
    assert_eq!(protocol_fragment_kind_code(TypeFragmentKind::Class), 3);
    let participants = [
        FamilyParticipant::new(LibraryFileOrdinal::new(2), 40, TypeFragmentKind::Interface),
        FamilyParticipant::new(LibraryFileOrdinal::new(1), 20, TypeFragmentKind::TypeAlias),
        FamilyParticipant::new(LibraryFileOrdinal::new(2), 10, TypeFragmentKind::Class),
    ];
    let token = canonical_family_token(&participants);
    assert_eq!(token.as_str(), FAMILY);

    let reordered = [participants[2], participants[0], participants[1]];
    assert_eq!(canonical_family_token(&reordered), token);
    assert_ne!(canonical_family_token(&participants[..2]), token);
    let mut changed = participants;
    changed[0] =
        FamilyParticipant::new(LibraryFileOrdinal::new(2), 41, TypeFragmentKind::Interface);
    assert_ne!(canonical_family_token(&changed), token);
    changed = participants;
    changed[0] = FamilyParticipant::new(LibraryFileOrdinal::new(2), 40, TypeFragmentKind::Class);
    assert_ne!(canonical_family_token(&changed), token);
}

#[test]
fn exact_state_digest_fixtures_match_canonical_protocol_bytes() {
    let mut entries = [(9_u32, 13_u32), (7, 11)];
    entries.sort_unstable_by_key(|entry| entry.0);
    let canonical_map = entries
        .iter()
        .map(|(source, target)| format!("{source}:{target}"))
        .collect::<Vec<_>>()
        .join(",");
    let canonical_application = format!("41|{canonical_map}");

    assert_eq!(canonical_map, MAP);
    assert_eq!(canonical_application, APPLICATION);
    assert_eq!(
        format!("{:x}", Sha256::digest(canonical_map.as_bytes())),
        MAP_SHA,
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(canonical_application.as_bytes())),
        APPLICATION_SHA,
    );
}

#[test]
fn v1_schema_round_trips_active_arithmetic_and_replay_events() {
    let lines = [
        format!("{PREFIX} kind=heartbeat seq=1 process=3 mode=exact universe=1 phase=reserve_fill reporter_elapsed_us=4900000 checkpoint_elapsed_us=4800000 reserve_fill_us=4000000 active_family_sha256={FAMILY} active_elapsed_us=2600000 coverage_lost=0"),
        format!("{PREFIX} kind=eager seq=2 process=3 mode=exact universe=1 family_sha256={FAMILY} calls=9 hits=3 misses=5 clean=1 tainted=4 active=1 completed=8 completed_us=170000 active_us=30000"),
        format!("{PREFIX} kind=run seq=3 process=3 mode=exact universe=1 run=19 family_sha256={FAMILY} checkpoint=1 started=1 completed=0 active=1 visits=2 memo_hits=0 cycle_reentries=1 tainted_ancestors=1"),
        format!("{PREFIX} kind=state seq=4 process=3 mode=exact universe=1 run=19 state=1 type_id=73 context=7 map={MAP} map_sha256={MAP_SHA} application={APPLICATION} application_sha256={APPLICATION_SHA} saturated=0"),
        format!("{PREFIX} kind=event seq=5 process=3 mode=exact universe=1 run=19 event=1 action=enter visit=1 parent=none state=1 at_us=10"),
        format!("{PREFIX} kind=event seq=6 process=3 mode=exact universe=1 run=19 event=2 action=enter visit=2 parent=1 state=1 at_us=11"),
        format!("{PREFIX} kind=event seq=7 process=3 mode=exact universe=1 run=19 event=3 action=outcome visit=2 parent=1 state=1 disposition=raw_cycle_reentry at_us=12"),
        format!("{PREFIX} kind=event seq=8 process=3 mode=exact universe=1 run=19 event=4 action=exit visit=2 parent=1 state=1 at_us=13"),
        format!("{PREFIX} kind=event seq=9 process=3 mode=exact universe=1 run=19 event=5 action=outcome visit=1 parent=none state=1 disposition=tainted at_us=14"),
        format!("{PREFIX} kind=event seq=10 process=3 mode=exact universe=1 run=19 event=6 action=exit visit=1 parent=none state=1 at_us=15"),
    ];
    let parsed = lines.iter().map(|line| parse(line)).collect::<Vec<_>>();
    assert!(
        matches!(&parsed[0], AttributionLine::Heartbeat(line) if line.active_elapsed_us == 2_600_000)
    );
    assert!(matches!(&parsed[1], AttributionLine::Eager(line) if line.arithmetic_is_exact()));
    assert!(matches!(&parsed[2], AttributionLine::Run(line) if line.arithmetic_is_exact()));
    assert!(matches!(&parsed[3], AttributionLine::State(line) if !line.saturated));
    assert!(matches!(&parsed[4], AttributionLine::Event(line) if line.is_enter()));
    assert!(matches!(&parsed[5], AttributionLine::Event(line) if line.is_enter()));
    assert!(matches!(&parsed[6], AttributionLine::Event(line) if line.is_raw_cycle_reentry()));
    assert!(matches!(&parsed[7], AttributionLine::Event(line) if line.is_exit()));
    assert!(matches!(&parsed[8], AttributionLine::Event(line) if line.is_tainted()));
    assert!(matches!(&parsed[9], AttributionLine::Event(line) if line.is_exit()));
    for disposition in [
        "clean",
        "completed_memo_hit",
        "raw_cycle_reentry",
        "tainted",
    ] {
        let line = lines[6].replace("raw_cycle_reentry", disposition);
        assert!(matches!(parse(&line), AttributionLine::Event(_)));
    }

    let bad = lines[1].replace("calls=9", "calls=10");
    assert!(parse_attribution_line(&bad).is_err());
    let bad = lines[1].replace("misses=5", "misses=6");
    assert!(parse_attribution_line(&bad).is_err());
    let bad = lines[2].replace("started=1", "started=2");
    assert!(parse_attribution_line(&bad).is_err());
    let bad = lines[0].replace(" coverage_lost=0", " path=lib.dom.d.ts coverage_lost=0");
    assert!(parse_attribution_line(&bad).is_err());
    let bad = lines[3].replace("context=7", "context=9,7");
    assert!(parse_attribution_line(&bad).is_err());
}

fn push_line(lines: &mut Vec<String>, seq: &mut u64, process: u8, kind: &str, fields: String) {
    lines.push(format!(
        "{PREFIX} kind={kind} seq={} process={process} mode=exact universe=1 {fields}",
        *seq
    ));
    *seq += 1;
}

fn exact_session_lines(process: u8, normal: bool) -> Vec<String> {
    let limits = limits();
    let mut lines = Vec::new();
    let mut seq = 0_u64;
    push_line(
        &mut lines,
        &mut seq,
        process,
        "session",
        format!(
            "session_sha256={} binary_sha256={BINARY_IDENTITY} host_sha256={HOST_IDENTITY} workload_profile_sha256={WORKLOAD_PROFILE_IDENTITY} capabilities=progress,eager,substitution,exact_trace interval_ms=250 checkpoint_visits=4096 evidence_window_ms=5000 max_lines={} max_eager_keys={} max_runs={} max_dictionary_entries={} max_trace_events={} max_checkpoint_messages={} max_checkpoint_bytes={} max_line_bytes={} max_file_bytes={} max_map_entries={} max_context_entries={} max_application_entries={} max_live_exact_bytes={} terminal_reserve_lines={} terminal_reserve_bytes={}",
            exact_session_identity(process),
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
        ),
    );
    push_line(
        &mut lines,
        &mut seq,
        process,
        "state",
        format!("run=19 state=1 type_id=73 context=7 map={MAP} map_sha256={MAP_SHA} application={APPLICATION} application_sha256={APPLICATION_SHA} saturated=0"),
    );

    let mut event = 1_u64;
    push_line(
        &mut lines,
        &mut seq,
        process,
        "event",
        format!("run=19 event={event} action=enter visit=1 parent=none state=1 at_us=3"),
    );
    event += 1;
    for visit in 2_u64..=4_096 {
        let entered = 4 + (visit - 2) * 3;
        push_line(
            &mut lines,
            &mut seq,
            process,
            "event",
            format!(
                "run=19 event={event} action=enter visit={visit} parent=1 state=1 at_us={entered}"
            ),
        );
        event += 1;
        push_line(
            &mut lines,
            &mut seq,
            process,
            "event",
            format!("run=19 event={event} action=outcome visit={visit} parent=1 state=1 disposition=raw_cycle_reentry at_us={}", entered + 1),
        );
        event += 1;
        push_line(
            &mut lines,
            &mut seq,
            process,
            "event",
            format!(
                "run=19 event={event} action=exit visit={visit} parent=1 state=1 at_us={}",
                entered + 2
            ),
        );
        event += 1;
    }
    let root_outcome_at = 4 + 4_095 * 3;
    push_line(
        &mut lines,
        &mut seq,
        process,
        "event",
        format!("run=19 event={event} action=outcome visit=1 parent=none state=1 disposition=tainted at_us={root_outcome_at}"),
    );
    event += 1;
    push_line(
        &mut lines,
        &mut seq,
        process,
        "event",
        format!(
            "run=19 event={event} action=exit visit=1 parent=none state=1 at_us={}",
            root_outcome_at + 1
        ),
    );

    let (eager, run, active_family, active_elapsed) = if normal {
        (
            format!("family_sha256={FAMILY} calls=1 hits=0 misses=1 clean=0 tainted=1 active=0 completed=1 completed_us=2600000 active_us=0"),
            format!("run=19 family_sha256={FAMILY} checkpoint=1 started=1 completed=1 active=0 visits=4096 memo_hits=0 cycle_reentries=4095 tainted_ancestors=1"),
            "-",
            0,
        )
    } else {
        (
            format!("family_sha256={FAMILY} calls=1 hits=0 misses=0 clean=0 tainted=0 active=1 completed=0 completed_us=0 active_us=2600000"),
            format!("run=19 family_sha256={FAMILY} checkpoint=1 started=1 completed=0 active=1 visits=4096 memo_hits=0 cycle_reentries=4095 tainted_ancestors=1"),
            FAMILY,
            2_600_000,
        )
    };
    push_line(&mut lines, &mut seq, process, "eager", eager);
    push_line(&mut lines, &mut seq, process, "run", run);
    push_line(
        &mut lines,
        &mut seq,
        process,
        "heartbeat",
        format!("phase=reserve_fill reporter_elapsed_us=4900000 checkpoint_elapsed_us=4800000 reserve_fill_us=4000000 active_family_sha256={active_family} active_elapsed_us={active_elapsed} coverage_lost=0"),
    );
    if normal {
        let line_count = seq + 1;
        push_line(
            &mut lines,
            &mut seq,
            process,
            "finish",
            format!("status=complete reporter_elapsed_us=4950000 checkpoint_elapsed_us=4800000 coverage_lost=0 lines={line_count}"),
        );
    }
    lines
}

fn unrelated_repeated_roots_lines(process: u8) -> Vec<String> {
    let template = exact_session_lines(process, false);
    let mut lines = template[..2].to_vec();
    let mut seq = 2_u64;
    let mut event = 1_u64;
    for root_index in 0_u64..4_096 {
        let root = root_index * 2 + 1;
        let child = root + 1;
        let entered = root_index * 6 + 3;
        for fields in [
            format!("run=19 event={event} action=enter visit={root} parent=none state=1 at_us={entered}"),
            format!("run=19 event={} action=enter visit={child} parent={root} state=1 at_us={}", event + 1, entered + 1),
            format!("run=19 event={} action=outcome visit={child} parent={root} state=1 disposition=raw_cycle_reentry at_us={}", event + 2, entered + 2),
            format!("run=19 event={} action=exit visit={child} parent={root} state=1 at_us={}", event + 3, entered + 3),
            format!("run=19 event={} action=outcome visit={root} parent=none state=1 disposition=tainted at_us={}", event + 4, entered + 4),
            format!("run=19 event={} action=exit visit={root} parent=none state=1 at_us={}", event + 5, entered + 5),
        ] {
            push_line(&mut lines, &mut seq, process, "event", fields);
        }
        event += 6;
    }
    push_line(
        &mut lines,
        &mut seq,
        process,
        "eager",
        format!("family_sha256={FAMILY} calls=1 hits=0 misses=0 clean=0 tainted=0 active=1 completed=0 completed_us=0 active_us=2600000"),
    );
    push_line(
        &mut lines,
        &mut seq,
        process,
        "run",
        format!("run=19 family_sha256={FAMILY} checkpoint=1 started=1 completed=0 active=1 visits=8192 memo_hits=0 cycle_reentries=4096 tainted_ancestors=4096"),
    );
    push_line(
        &mut lines,
        &mut seq,
        process,
        "heartbeat",
        format!("phase=reserve_fill reporter_elapsed_us=4900000 checkpoint_elapsed_us=4800000 reserve_fill_us=4000000 active_family_sha256={FAMILY} active_elapsed_us=2600000 coverage_lost=0"),
    );
    lines
}

fn validate_deadline(lines: &[String]) -> Result<ValidatedSessionEvidence, String> {
    validate_session_evidence(
        lines,
        Termination::Deadline {
            elapsed_us: 5_000_000,
        },
    )
    .map_err(|error| error.to_string())
}

fn resequence(lines: &mut [String]) {
    for (sequence, line) in lines.iter_mut().enumerate() {
        let marker = " seq=";
        let value_start = line.find(marker).expect("fixture has sequence") + marker.len();
        let value_end = line[value_start..]
            .find(' ')
            .map(|offset| value_start + offset)
            .expect("sequence is followed by fields");
        line.replace_range(value_start..value_end, &sequence.to_string());
    }
}

#[test]
fn whole_session_validator_accepts_only_a_clean_contiguous_evidence_prefix() {
    let deadline = exact_session_lines(1, false);
    let validated = validate_deadline(&deadline).expect("clean deadline prefix is evidence");
    assert_eq!(
        validated.session_identity_sha256(),
        exact_session_identity(1)
    );
    assert_eq!(validated.binary_identity_sha256(), BINARY_IDENTITY);
    assert_eq!(validated.host_identity_sha256(), HOST_IDENTITY);
    assert_eq!(
        validated.workload_profile_identity_sha256(),
        WORKLOAD_PROFILE_IDENTITY
    );
    assert!(matches!(
        validated.termination(),
        Termination::Deadline {
            elapsed_us: 5_000_000
        }
    ));
    assert_eq!(validated.checkpoint_elapsed_us(), 4_800_000);
    let sample = validated.sample();
    assert_eq!(sample.family_sha256, FAMILY);
    assert_eq!(sample.reserve_fill_us, 4_000_000);
    assert_eq!(sample.family_exclusive_us, 2_600_000);
    assert_eq!(sample.visits, 4_096);
    assert_eq!(sample.family_visits, 4_096);
    assert!(sample.family_cycle_tainted);
    assert_eq!(sample.exact_repeats, 4_095);
    let replay = replay_exact_trace(validated.replay_input()).expect("trace is replayable");
    assert_eq!(replay.repeated_visits, 4_095);
    assert_eq!(replay.removable_repeated_visits, 4_095);

    let unrelated = validate_deadline(&unrelated_repeated_roots_lines(1))
        .expect("unrelated repeated roots remain valid trace evidence");
    let unrelated_replay =
        replay_exact_trace(unrelated.replay_input()).expect("unrelated roots are replayable");
    assert_eq!(unrelated.sample().exact_repeats, 8_191);
    assert_eq!(unrelated_replay.repeated_visits, 8_191);
    assert_eq!(unrelated_replay.removable_repeated_visits, 4_096);
    assert_eq!(
        unrelated_replay.repeated_visits - unrelated_replay.removable_repeated_visits,
        4_095,
        "repeated parent=none roots are not removable cycle edges"
    );

    let normal = exact_session_lines(1, true);
    assert!(validate_session_evidence(&normal, Termination::Normal).is_ok());
    let mut missing_finish = normal.clone();
    missing_finish.pop();
    assert!(validate_session_evidence(&missing_finish, Termination::Normal).is_err());

    let mut missing_seq = deadline.clone();
    missing_seq.remove(2);
    assert!(validate_deadline(&missing_seq).is_err());
    let mut dropped = deadline.clone();
    let heartbeat = dropped.last_mut().expect("heartbeat");
    *heartbeat = heartbeat.replace("coverage_lost=0", "coverage_lost=1");
    assert!(validate_deadline(&dropped).is_err());
    let mut malformed = deadline.clone();
    malformed[1] = malformed[1].replace(" state=1", "");
    assert!(validate_deadline(&malformed).is_err());
    let mut wrong_universe = deadline.clone();
    wrong_universe[2] = wrong_universe[2].replace("universe=1", "universe=2");
    assert!(validate_deadline(&wrong_universe).is_err());
    let mut wrong_process = deadline.clone();
    wrong_process[2] = wrong_process[2].replace("process=1", "process=2");
    assert!(validate_deadline(&wrong_process).is_err());
    let mut wrong_mode = deadline.clone();
    wrong_mode[2] = wrong_mode[2].replace("mode=exact", "mode=progress");
    assert!(validate_deadline(&wrong_mode).is_err());
    let mut missing_capability = deadline.clone();
    missing_capability[0] = missing_capability[0].replace(",exact_trace", "");
    assert!(validate_deadline(&missing_capability).is_err());
    let mut malformed_session_identity = deadline.clone();
    malformed_session_identity[0] =
        malformed_session_identity[0].replace(&exact_session_identity(1), "not-opaque");
    assert!(validate_deadline(&malformed_session_identity).is_err());
    let mut saturated = deadline.clone();
    saturated[1] = saturated[1].replace("saturated=0", "saturated=1");
    assert!(validate_deadline(&saturated).is_err());
    let mut stale = deadline.clone();
    let heartbeat = stale.last_mut().expect("heartbeat");
    *heartbeat = heartbeat.replace(
        "checkpoint_elapsed_us=4800000",
        "checkpoint_elapsed_us=4500000",
    );
    assert!(validate_deadline(&stale).is_err());
    let mut zero_reserve_fill = deadline.clone();
    let heartbeat = zero_reserve_fill.last_mut().expect("heartbeat");
    *heartbeat = heartbeat.replace("reserve_fill_us=4000000", "reserve_fill_us=0");
    assert!(validate_deadline(&zero_reserve_fill).is_err());
    let mut reserve_fill_after_checkpoint = deadline.clone();
    let heartbeat = reserve_fill_after_checkpoint.last_mut().expect("heartbeat");
    *heartbeat = heartbeat.replace("reserve_fill_us=4000000", "reserve_fill_us=4850000");
    assert!(validate_deadline(&reserve_fill_after_checkpoint).is_err());
    let mut checkpoint_after_deadline = deadline.clone();
    let heartbeat = checkpoint_after_deadline.last_mut().expect("heartbeat");
    *heartbeat = heartbeat.replace(
        "checkpoint_elapsed_us=4800000",
        "checkpoint_elapsed_us=5000001",
    );
    assert!(validate_deadline(&checkpoint_after_deadline).is_err());
    let mut bad_parent = deadline.clone();
    let second_enter = bad_parent
        .iter_mut()
        .find(|line| line.contains("action=enter visit=2 "))
        .expect("second enter event");
    *second_enter = second_enter.replace("parent=1", "parent=999");
    assert!(validate_deadline(&bad_parent).is_err());
    let mut missing_dictionary = deadline.clone();
    let first_enter = missing_dictionary
        .iter_mut()
        .find(|line| line.contains("action=enter visit=1 "))
        .expect("first enter event");
    *first_enter = first_enter.replace("state=1", "state=2");
    assert!(validate_deadline(&missing_dictionary).is_err());
    let mut wrong_map_digest = deadline.clone();
    let state = wrong_map_digest
        .iter_mut()
        .find(|line| line.contains(" kind=state "))
        .expect("state line");
    *state = state.replace(MAP_SHA, FAMILY);
    assert!(validate_deadline(&wrong_map_digest).is_err());
    let mut wrong_application_digest = deadline.clone();
    let state = wrong_application_digest
        .iter_mut()
        .find(|line| line.contains(" kind=state "))
        .expect("state line");
    *state = state.replace(APPLICATION_SHA, FAMILY);
    assert!(validate_deadline(&wrong_application_digest).is_err());
    let mut wrong_cycle_counter = deadline.clone();
    let run = wrong_cycle_counter
        .iter_mut()
        .find(|line| line.contains(" kind=run "))
        .expect("run line");
    *run = run.replace("cycle_reentries=4095", "cycle_reentries=4094");
    assert!(validate_deadline(&wrong_cycle_counter).is_err());
    let mut wrong_tainted_counter = deadline.clone();
    let run = wrong_tainted_counter
        .iter_mut()
        .find(|line| line.contains(" kind=run "))
        .expect("run line");
    *run = run.replace("tainted_ancestors=1", "tainted_ancestors=2");
    assert!(validate_deadline(&wrong_tainted_counter).is_err());
    let mut wrong_visit_counter = deadline.clone();
    let run = wrong_visit_counter
        .iter_mut()
        .find(|line| line.contains(" kind=run "))
        .expect("run line");
    *run = run.replace("visits=4096", "visits=4095");
    assert!(validate_deadline(&wrong_visit_counter).is_err());
    let mut wrong_memo_counter = deadline.clone();
    let run = wrong_memo_counter
        .iter_mut()
        .find(|line| line.contains(" kind=run "))
        .expect("run line");
    *run = run.replace("memo_hits=0", "memo_hits=1");
    assert!(validate_deadline(&wrong_memo_counter).is_err());
    let mut raw_reentry_without_matching_ancestor = deadline.clone();
    let mut second_state = raw_reentry_without_matching_ancestor[1]
        .replace(" state=1 ", " state=2 ")
        .replace("type_id=73", "type_id=74");
    second_state = second_state.replace(" seq=1 ", " seq=2 ");
    raw_reentry_without_matching_ancestor.insert(2, second_state);
    for line in &mut raw_reentry_without_matching_ancestor {
        if line.contains("visit=2 ") {
            *line = line.replace(" state=1 ", " state=2 ");
        }
    }
    resequence(&mut raw_reentry_without_matching_ancestor);
    assert!(validate_deadline(&raw_reentry_without_matching_ancestor).is_err());

    let baseline = validated.sample().clone();
    let mut raced = deadline.clone();
    let seq = u64::try_from(raced.len()).expect("fixture line count fits u64");
    raced.push(format!("{PREFIX} kind=eager seq={seq} process=1 mode=exact universe=1 family_sha256={FAMILY} calls=1 hits=0 misses=1 clean=0 tainted=1 active=0 completed=1 completed_us=2700000 active_us=0"));
    let raced =
        validate_deadline(&raced).expect("valid post-heartbeat activity is outside evidence");
    assert_eq!(raced.sample(), &baseline);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimingPair {
    baseline: u64,
    candidate: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SignedRatio {
    negative: bool,
    magnitude: u64,
    denominator: u64,
}

fn paired_ratio(pair: TimingPair) -> Option<SignedRatio> {
    if pair.baseline == 0 {
        return None;
    }
    Some(if pair.candidate <= pair.baseline {
        SignedRatio {
            negative: false,
            magnitude: pair.baseline - pair.candidate,
            denominator: pair.baseline,
        }
    } else {
        SignedRatio {
            negative: true,
            magnitude: pair.candidate - pair.baseline,
            denominator: pair.baseline,
        }
    })
}

fn ratio_cmp(left: &SignedRatio, right: &SignedRatio) -> Ordering {
    match (left.negative, right.negative) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => {
            let lhs = u128::from(left.magnitude) * u128::from(right.denominator);
            let rhs = u128::from(right.magnitude) * u128::from(left.denominator);
            if left.negative {
                rhs.cmp(&lhs)
            } else {
                lhs.cmp(&rhs)
            }
        }
    }
}

fn paired_median(pairs: &[TimingPair]) -> Option<SignedRatio> {
    if pairs.len() != 5 {
        return None;
    }
    let mut ratios = pairs
        .iter()
        .copied()
        .map(paired_ratio)
        .collect::<Option<Vec<_>>>()?;
    ratios.sort_by(ratio_cmp);
    Some(ratios[2])
}

fn median_improvement_at_least(pairs: &[TimingPair], percent: u64) -> bool {
    paired_median(pairs).is_some_and(|median| {
        !median.negative
            && u128::from(median.magnitude) * 100
                >= u128::from(median.denominator) * u128::from(percent)
    })
}

fn median_regression_at_most(pairs: &[TimingPair], percent: u64) -> bool {
    paired_median(pairs).is_some_and(|median| {
        !median.negative
            || u128::from(median.magnitude) * 100
                <= u128::from(median.denominator) * u128::from(percent)
    })
}

#[test]
fn paired_ratio_gates_use_the_median_of_five_matched_ratios() {
    let one_bad_improvement = [
        TimingPair {
            baseline: 100,
            candidate: 80,
        },
        TimingPair {
            baseline: 1_000,
            candidate: 800,
        },
        TimingPair {
            baseline: 10_000,
            candidate: 8_000,
        },
        TimingPair {
            baseline: 100,
            candidate: 80,
        },
        TimingPair {
            baseline: 1,
            candidate: 1,
        },
    ];
    assert!(median_improvement_at_least(&one_bad_improvement, 20));
    let three_bad_improvements = [
        TimingPair {
            baseline: 100,
            candidate: 81,
        },
        TimingPair {
            baseline: 1_000,
            candidate: 810,
        },
        TimingPair {
            baseline: 10_000,
            candidate: 8_100,
        },
        TimingPair {
            baseline: 100,
            candidate: 80,
        },
        TimingPair {
            baseline: 1_000,
            candidate: 800,
        },
    ];
    assert!(!median_improvement_at_least(&three_bad_improvements, 20));

    let one_bad_overhead = [
        TimingPair {
            baseline: 100,
            candidate: 102,
        },
        TimingPair {
            baseline: 1_000,
            candidate: 1_020,
        },
        TimingPair {
            baseline: 10_000,
            candidate: 10_200,
        },
        TimingPair {
            baseline: 100,
            candidate: 102,
        },
        TimingPair {
            baseline: 1,
            candidate: 2,
        },
    ];
    assert!(median_regression_at_most(&one_bad_overhead, 2));
    let three_bad_overheads = [
        TimingPair {
            baseline: 100,
            candidate: 103,
        },
        TimingPair {
            baseline: 1_000,
            candidate: 1_030,
        },
        TimingPair {
            baseline: 10_000,
            candidate: 10_300,
        },
        TimingPair {
            baseline: 100,
            candidate: 102,
        },
        TimingPair {
            baseline: 1_000,
            candidate: 1_020,
        },
    ];
    assert!(!median_regression_at_most(&three_bad_overheads, 2));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimingVariant {
    BaselineA,
    CandidateB,
    NonCycleA,
    NonCycleB,
    ReporterControl,
    Progress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimingTermination {
    Normal,
    Deadline { elapsed_us: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReserveFillEvidence {
    Complete(u64),
    LastCleanCheckpointLowerBound {
        reserve_fill_us: u64,
        checkpoint_elapsed_us: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawTimingRun {
    process: usize,
    session_identity: String,
    variant: TimingVariant,
    mode: AttributionMode,
    phase: AttributionPhase,
    binary_identity: String,
    host_identity: String,
    workload_profile_identity: String,
    termination: TimingTermination,
    reserve_fill: ReserveFillEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawMatchedTiming {
    baseline: RawTimingRun,
    candidate: RawTimingRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchedEvidenceKind {
    CandidateAb,
    NonCycle,
    ReporterOverhead,
}

#[derive(Debug)]
struct ValidatedExactBaselineBinding {
    process: usize,
    session_identity: String,
    termination: TimingTermination,
    checkpoint_elapsed_us: u64,
    reserve_fill_us: u64,
}

#[derive(Debug)]
struct ValidatedMatchedEvidence {
    kind: MatchedEvidenceKind,
    binary_identity: String,
    host_identity: String,
    workload_profile_identity: String,
    exact_baselines: Option<[ValidatedExactBaselineBinding; 5]>,
    pairs: [TimingPair; 5],
}

fn is_opaque_identity(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn expected_timing_shape(
    kind: MatchedEvidenceKind,
) -> (
    TimingVariant,
    TimingVariant,
    AttributionMode,
    AttributionMode,
) {
    match kind {
        MatchedEvidenceKind::CandidateAb => (
            TimingVariant::BaselineA,
            TimingVariant::CandidateB,
            AttributionMode::Exact,
            AttributionMode::Exact,
        ),
        MatchedEvidenceKind::NonCycle => (
            TimingVariant::NonCycleA,
            TimingVariant::NonCycleB,
            AttributionMode::Exact,
            AttributionMode::Exact,
        ),
        MatchedEvidenceKind::ReporterOverhead => (
            TimingVariant::ReporterControl,
            TimingVariant::Progress,
            AttributionMode::ReporterControl,
            AttributionMode::Progress,
        ),
    }
}

fn elapsed_value(run: &RawTimingRun, permit_deadline_lower_bound: bool) -> Option<u64> {
    match (run.termination, run.reserve_fill) {
        (TimingTermination::Normal, ReserveFillEvidence::Complete(elapsed)) => Some(elapsed),
        (
            TimingTermination::Deadline { elapsed_us },
            ReserveFillEvidence::LastCleanCheckpointLowerBound {
                reserve_fill_us,
                checkpoint_elapsed_us,
            },
        ) if permit_deadline_lower_bound
            && elapsed_us == 5_000_000
            && reserve_fill_us > 0
            && reserve_fill_us <= checkpoint_elapsed_us
            && checkpoint_elapsed_us <= elapsed_us
            && elapsed_us - checkpoint_elapsed_us <= 250_000 =>
        {
            Some(reserve_fill_us)
        }
        _ => None,
    }
}

fn validate_matched_evidence(
    kind: MatchedEvidenceKind,
    raw: &[RawMatchedTiming],
) -> Result<ValidatedMatchedEvidence, String> {
    if raw.len() != 5 {
        return Err("exactly five matched processes are required".to_owned());
    }
    let (baseline_variant, candidate_variant, baseline_mode, candidate_mode) =
        expected_timing_shape(kind);
    let first = &raw[0];
    let identities = (
        first.baseline.binary_identity.as_str(),
        first.candidate.binary_identity.as_str(),
        first.baseline.host_identity.as_str(),
        first.baseline.workload_profile_identity.as_str(),
    );
    if !is_opaque_identity(identities.0)
        || !is_opaque_identity(identities.1)
        || !is_opaque_identity(identities.2)
        || !is_opaque_identity(identities.3)
        || identities.0 != identities.1
        || first.candidate.host_identity != identities.2
        || first.candidate.workload_profile_identity != identities.3
    {
        return Err("binary and host identities must be exact opaque matches".to_owned());
    }

    let mut pairs = Vec::with_capacity(5);
    let mut exact_baselines = Vec::with_capacity(5);
    for (index, matched) in raw.iter().enumerate() {
        let baseline = &matched.baseline;
        let candidate = &matched.candidate;
        if baseline.process != index + 1
            || candidate.process != index + 1
            || baseline.variant != baseline_variant
            || candidate.variant != candidate_variant
            || baseline.mode != baseline_mode
            || candidate.mode != candidate_mode
            || baseline.phase != AttributionPhase::ReserveFill
            || candidate.phase != AttributionPhase::ReserveFill
            || !is_opaque_identity(&baseline.session_identity)
            || !is_opaque_identity(&candidate.session_identity)
            || baseline.binary_identity != identities.0
            || candidate.binary_identity != identities.1
            || baseline.host_identity != identities.2
            || candidate.host_identity != identities.2
            || baseline.workload_profile_identity != identities.3
            || candidate.workload_profile_identity != identities.3
        {
            return Err("matched-run metadata differs".to_owned());
        }
        let baseline_elapsed = elapsed_value(baseline, kind == MatchedEvidenceKind::CandidateAb)
            .ok_or_else(|| "invalid baseline termination/elapsed evidence".to_owned())?;
        if kind == MatchedEvidenceKind::CandidateAb
            && !matches!(
                (baseline.termination, baseline.reserve_fill),
                (
                    TimingTermination::Deadline {
                        elapsed_us: 5_000_000
                    },
                    ReserveFillEvidence::LastCleanCheckpointLowerBound { .. }
                )
            )
        {
            return Err("candidate A/B baseline must be a deadline lower bound".to_owned());
        }
        if let (
            MatchedEvidenceKind::CandidateAb,
            TimingTermination::Deadline { .. },
            ReserveFillEvidence::LastCleanCheckpointLowerBound {
                reserve_fill_us,
                checkpoint_elapsed_us,
            },
        ) = (kind, baseline.termination, baseline.reserve_fill)
        {
            exact_baselines.push(ValidatedExactBaselineBinding {
                process: baseline.process,
                session_identity: baseline.session_identity.clone(),
                termination: baseline.termination,
                checkpoint_elapsed_us,
                reserve_fill_us,
            });
        }
        let candidate_elapsed = elapsed_value(candidate, false)
            .ok_or_else(|| "candidate must have complete normal evidence".to_owned())?;
        if kind != MatchedEvidenceKind::CandidateAb
            && baseline.termination != TimingTermination::Normal
        {
            return Err("control baseline must terminate normally".to_owned());
        }
        pairs.push(TimingPair {
            baseline: baseline_elapsed,
            candidate: candidate_elapsed,
        });
    }
    let pairs = pairs
        .try_into()
        .map_err(|_| "validated timing set has wrong length".to_owned())?;
    let exact_baselines = if kind == MatchedEvidenceKind::CandidateAb {
        Some(
            exact_baselines
                .try_into()
                .map_err(|_| "candidate A/B lacks exact-session bindings".to_owned())?,
        )
    } else {
        None
    };
    Ok(ValidatedMatchedEvidence {
        kind,
        binary_identity: identities.0.to_owned(),
        host_identity: identities.2.to_owned(),
        workload_profile_identity: identities.3.to_owned(),
        exact_baselines,
        pairs,
    })
}

fn matched_timing_fixture(
    kind: MatchedEvidenceKind,
    baseline_us: u64,
    candidate_us: u64,
) -> Vec<RawMatchedTiming> {
    let (baseline_variant, candidate_variant, baseline_mode, candidate_mode) =
        expected_timing_shape(kind);
    (1..=5)
        .map(|process| {
            let process_u8 = u8::try_from(process).expect("five timing processes fit u8");
            let kind_offset = match kind {
                MatchedEvidenceKind::CandidateAb => 20_000_u64,
                MatchedEvidenceKind::NonCycle => 30_000,
                MatchedEvidenceKind::ReporterOverhead => 40_000,
            };
            let baseline_session_identity = if kind == MatchedEvidenceKind::CandidateAb {
                exact_session_identity(process_u8)
            } else {
                format!("{:064x}", kind_offset + u64::from(process_u8) * 2)
            };
            let candidate_session_identity =
                format!("{:064x}", kind_offset + u64::from(process_u8) * 2 + 1);
            let (termination, reserve_fill) = if kind == MatchedEvidenceKind::CandidateAb {
                (
                    TimingTermination::Deadline {
                        elapsed_us: 5_000_000,
                    },
                    ReserveFillEvidence::LastCleanCheckpointLowerBound {
                        reserve_fill_us: baseline_us,
                        checkpoint_elapsed_us: 4_800_000,
                    },
                )
            } else {
                (
                    TimingTermination::Normal,
                    ReserveFillEvidence::Complete(baseline_us),
                )
            };
            RawMatchedTiming {
                baseline: RawTimingRun {
                    process,
                    session_identity: baseline_session_identity,
                    variant: baseline_variant,
                    mode: baseline_mode,
                    phase: AttributionPhase::ReserveFill,
                    binary_identity: BINARY_IDENTITY.to_owned(),
                    host_identity: HOST_IDENTITY.to_owned(),
                    workload_profile_identity: WORKLOAD_PROFILE_IDENTITY.to_owned(),
                    termination,
                    reserve_fill,
                },
                candidate: RawTimingRun {
                    process,
                    session_identity: candidate_session_identity,
                    variant: candidate_variant,
                    mode: candidate_mode,
                    phase: AttributionPhase::ReserveFill,
                    binary_identity: BINARY_IDENTITY.to_owned(),
                    host_identity: HOST_IDENTITY.to_owned(),
                    workload_profile_identity: WORKLOAD_PROFILE_IDENTITY.to_owned(),
                    termination: TimingTermination::Normal,
                    reserve_fill: ReserveFillEvidence::Complete(candidate_us),
                },
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoordinatorDecision {
    Authorized,
    Rejected { wu0_no_go: bool, wu1_pending: bool },
}

fn at_least(part: u64, total: u64, percent: u64) -> bool {
    total > 0 && u128::from(part) * 100 >= u128::from(total) * u128::from(percent)
}

fn exact_baselines_match_sessions(
    sessions: &[ValidatedSessionEvidence],
    ab: &ValidatedMatchedEvidence,
) -> bool {
    let Some(bindings) = &ab.exact_baselines else {
        return false;
    };
    if sessions.len() != bindings.len() {
        return false;
    }
    sessions
        .iter()
        .zip(bindings)
        .zip(ab.pairs)
        .all(|((session, binding), pair)| {
            let termination_matches = matches!(
                (binding.termination, session.termination()),
                (
                    TimingTermination::Deadline {
                        elapsed_us: binding_elapsed
                    },
                    Termination::Deadline {
                        elapsed_us: session_elapsed
                    }
                ) if binding_elapsed == session_elapsed
            );
            binding.process == session.process()
                && binding.session_identity == session.session_identity_sha256()
                && ab.binary_identity == session.binary_identity_sha256()
                && ab.host_identity == session.host_identity_sha256()
                && ab.workload_profile_identity == session.workload_profile_identity_sha256()
                && termination_matches
                && binding.checkpoint_elapsed_us == session.checkpoint_elapsed_us()
                && binding.reserve_fill_us == session.sample().reserve_fill_us
                && pair.baseline == binding.reserve_fill_us
        })
}

fn authorize(
    sessions: &[ValidatedSessionEvidence],
    ab: &ValidatedMatchedEvidence,
    non_cycle: &ValidatedMatchedEvidence,
    reporter_overhead: &ValidatedMatchedEvidence,
) -> CoordinatorDecision {
    let rejected = CoordinatorDecision::Rejected {
        wu0_no_go: true,
        wu1_pending: true,
    };
    if sessions.len() != 5
        || sessions
            .iter()
            .enumerate()
            .any(|(index, session)| session.process() != index + 1)
        || sessions.iter().any(|session| {
            let sample = session.sample();
            sample.family_sha256 != sessions[0].sample().family_sha256
                || !sample.family_cycle_tainted
                || !at_least(sample.family_exclusive_us, sample.reserve_fill_us, 50)
                || !at_least(sample.family_visits, sample.visits, 50)
                || !at_least(sample.exact_repeats, sample.exact_visits, 75)
                || replay_exact_trace(session.replay_input()).map_or(true, |replay| {
                    !at_least(replay.removable_repeated_visits, replay.repeated_visits, 80)
                })
        })
        || ab.kind != MatchedEvidenceKind::CandidateAb
        || non_cycle.kind != MatchedEvidenceKind::NonCycle
        || reporter_overhead.kind != MatchedEvidenceKind::ReporterOverhead
        || ab.binary_identity != non_cycle.binary_identity
        || ab.binary_identity != reporter_overhead.binary_identity
        || ab.host_identity != non_cycle.host_identity
        || ab.host_identity != reporter_overhead.host_identity
        || ab.workload_profile_identity != non_cycle.workload_profile_identity
        || ab.workload_profile_identity != reporter_overhead.workload_profile_identity
        || !exact_baselines_match_sessions(sessions, ab)
        || !median_improvement_at_least(&ab.pairs, 20)
        || !median_regression_at_most(&non_cycle.pairs, 2)
        || !median_regression_at_most(&reporter_overhead.pairs, 2)
    {
        return rejected;
    }
    CoordinatorDecision::Authorized
}

#[test]
fn coordinator_consumes_three_validated_matched_sets_and_rejects_metadata_mismatches() {
    let sessions = (1_u8..=5)
        .map(|process| validate_deadline(&exact_session_lines(process, false)))
        .collect::<Result<Vec<_>, _>>()
        .expect("five validated exact sessions");
    let ab = validate_matched_evidence(
        MatchedEvidenceKind::CandidateAb,
        &matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 4_000_000, 3_200_000),
    )
    .expect("candidate is 20% faster than the last clean checkpoint lower bound");
    let non_cycle = validate_matched_evidence(
        MatchedEvidenceKind::NonCycle,
        &matched_timing_fixture(MatchedEvidenceKind::NonCycle, 1_000, 1_020),
    )
    .expect("non-cycle control is matched");
    let overhead = validate_matched_evidence(
        MatchedEvidenceKind::ReporterOverhead,
        &matched_timing_fixture(MatchedEvidenceKind::ReporterOverhead, 1_000, 1_020),
    )
    .expect("Progress is matched to ReporterControl");
    assert!(exact_baselines_match_sessions(&sessions, &ab));
    assert_eq!(
        authorize(&sessions, &ab, &non_cycle, &overhead),
        CoordinatorDecision::Authorized
    );

    let rejected = CoordinatorDecision::Rejected {
        wu0_no_go: true,
        wu1_pending: true,
    };
    let mut different_session_raw =
        matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 4_000_000, 3_200_000);
    different_session_raw[1].baseline.session_identity =
        "6666666666666666666666666666666666666666666666666666666666666666".to_owned();
    let different_session =
        validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &different_session_raw)
            .expect("same process and duration can still reference another opaque session");
    assert!(!exact_baselines_match_sessions(
        &sessions,
        &different_session
    ));
    assert_eq!(
        authorize(&sessions, &different_session, &non_cycle, &overhead),
        rejected
    );

    let mut different_binary_raw =
        matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 4_000_000, 3_200_000);
    for matched in &mut different_binary_raw {
        for run in [&mut matched.baseline, &mut matched.candidate] {
            run.binary_identity =
                "3333333333333333333333333333333333333333333333333333333333333333".to_owned();
        }
    }
    let different_binary =
        validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &different_binary_raw)
            .expect("the A/B set is internally binary-matched");
    assert!(!exact_baselines_match_sessions(
        &sessions,
        &different_binary
    ));
    assert_eq!(
        authorize(&sessions, &different_binary, &non_cycle, &overhead),
        rejected
    );

    let mut different_host_raw =
        matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 4_000_000, 3_200_000);
    for matched in &mut different_host_raw {
        for run in [&mut matched.baseline, &mut matched.candidate] {
            run.host_identity =
                "4444444444444444444444444444444444444444444444444444444444444444".to_owned();
        }
    }
    let different_host =
        validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &different_host_raw)
            .expect("the A/B set is internally host-matched");
    assert!(!exact_baselines_match_sessions(&sessions, &different_host));
    assert_eq!(
        authorize(&sessions, &different_host, &non_cycle, &overhead),
        rejected
    );

    let mut different_workload_raw =
        matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 4_000_000, 3_200_000);
    for matched in &mut different_workload_raw {
        for run in [&mut matched.baseline, &mut matched.candidate] {
            run.workload_profile_identity =
                "7777777777777777777777777777777777777777777777777777777777777777".to_owned();
        }
    }
    let different_workload =
        validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &different_workload_raw)
            .expect("the A/B set is internally workload/profile-matched");
    assert!(!exact_baselines_match_sessions(
        &sessions,
        &different_workload
    ));
    assert_eq!(
        authorize(&sessions, &different_workload, &non_cycle, &overhead),
        rejected
    );
    let unbound_ab = validate_matched_evidence(
        MatchedEvidenceKind::CandidateAb,
        &matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 3_999_999, 3_000_000),
    )
    .expect("internally valid timing still needs the exact-session baseline");
    assert_eq!(
        authorize(&sessions, &unbound_ab, &non_cycle, &overhead),
        rejected
    );
    let unrelated_sessions = (1_u8..=5)
        .map(|process| validate_deadline(&unrelated_repeated_roots_lines(process)))
        .collect::<Result<Vec<_>, _>>()
        .expect("semantically valid independent roots");
    assert_eq!(
        authorize(&unrelated_sessions, &ab, &non_cycle, &overhead),
        rejected,
        "repeats across unrelated roots cannot authorize removal"
    );
    let slow_ab = validate_matched_evidence(
        MatchedEvidenceKind::CandidateAb,
        &matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 4_000_000, 3_200_001),
    )
    .expect("matched but below the improvement gate");
    assert_eq!(
        authorize(&sessions, &slow_ab, &non_cycle, &overhead),
        rejected
    );
    let bad_overhead = validate_matched_evidence(
        MatchedEvidenceKind::ReporterOverhead,
        &matched_timing_fixture(MatchedEvidenceKind::ReporterOverhead, 1_000, 1_021),
    )
    .expect("matched but above the overhead gate");
    assert_eq!(
        authorize(&sessions, &ab, &non_cycle, &bad_overhead),
        rejected
    );

    let mut mismatch = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    mismatch[1].candidate.process = 9;
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &mismatch).is_err());
    let mut mismatch = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    mismatch[1].candidate.variant = TimingVariant::NonCycleB;
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &mismatch).is_err());
    let mut mismatch = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    mismatch[1].candidate.mode = AttributionMode::Progress;
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &mismatch).is_err());
    let mut mismatch = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    mismatch[1].candidate.phase = AttributionPhase::Bind;
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &mismatch).is_err());
    let mut mismatch = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    mismatch[1].candidate.binary_identity =
        "3333333333333333333333333333333333333333333333333333333333333333".to_owned();
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &mismatch).is_err());
    let mut mismatch = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    mismatch[1].candidate.host_identity =
        "4444444444444444444444444444444444444444444444444444444444444444".to_owned();
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &mismatch).is_err());
    let mut mismatch = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    mismatch[1].candidate.termination = TimingTermination::Deadline {
        elapsed_us: 5_000_000,
    };
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &mismatch).is_err());
    let mut mismatch = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    mismatch[1].baseline.termination = TimingTermination::Normal;
    mismatch[1].baseline.reserve_fill = ReserveFillEvidence::Complete(1_000);
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &mismatch).is_err());
    let mut impossible = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    impossible[1].baseline.reserve_fill = ReserveFillEvidence::LastCleanCheckpointLowerBound {
        reserve_fill_us: 4_800_001,
        checkpoint_elapsed_us: 4_800_000,
    };
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &impossible).is_err());
    let mut impossible = matched_timing_fixture(MatchedEvidenceKind::CandidateAb, 1_000, 800);
    impossible[1].baseline.reserve_fill = ReserveFillEvidence::LastCleanCheckpointLowerBound {
        reserve_fill_us: 1_000,
        checkpoint_elapsed_us: 5_000_001,
    };
    assert!(validate_matched_evidence(MatchedEvidenceKind::CandidateAb, &impossible).is_err());
}

#[test]
fn modes_default_off_and_reporter_control_never_capture_semantics() {
    assert_eq!(
        resolve_mode_from_values_for_test(None, None),
        AttributionMode::Off
    );
    assert_eq!(
        resolve_mode_from_values_for_test(Some("reporter-control"), Some("progress.log")),
        AttributionMode::ReporterControl
    );
    assert!(super::wu0c_attribution::current_session_for_test().is_none());
    assert!(super::wu0c_attribution::capture_pass_for_test().is_none());
    assert!(super::wu0c_attribution::capture_substitution_for_test().is_none());

    let clock = AttributionTestClock::default();
    let sink = AttributionTestSink::default();
    let scope =
        start_attribution_for_test(config(AttributionMode::ReporterControl, 1), &clock, &sink)
            .expect("reporter-only control");
    assert!(scope.control_for_test().capture_pass_for_test().is_none());
    assert!(scope
        .control_for_test()
        .capture_substitution_for_test()
        .is_none());
    scope.control_for_test().report_now_and_wait_for_test();
    assert!(sink
        .parsed_lines()
        .iter()
        .all(|line| !matches!(line, AttributionLine::Eager(_) | AttributionLine::Run(_))));
    drop(scope);
}

#[test]
fn release_config_resolver_never_defaults_requested_exact_identity_or_universe() {
    let complete = vec![
        ("TYPOKAT_WU0C_PROGRESS_PATH", "progress.log"),
        ("TYPOKAT_WU0C_MODE", "exact"),
        ("TYPOKAT_WU0B_PROCESS", "1"),
        ("TYPOKAT_WU0C_UNIVERSE", "73"),
        ("TYPOKAT_WU0C_SESSION_SHA256", RELEASE_SESSION_IDENTITY),
        ("TYPOKAT_WU0C_BINARY_SHA256", BINARY_IDENTITY),
        ("TYPOKAT_WU0C_HOST_SHA256", HOST_IDENTITY),
        (
            "TYPOKAT_WU0C_WORKLOAD_PROFILE_SHA256",
            WORKLOAD_PROFILE_IDENTITY,
        ),
    ];
    assert!(resolve_release_config_from_values_for_test(&complete).is_ok());
    for required in [
        "TYPOKAT_WU0C_SESSION_SHA256",
        "TYPOKAT_WU0C_BINARY_SHA256",
        "TYPOKAT_WU0C_HOST_SHA256",
        "TYPOKAT_WU0C_WORKLOAD_PROFILE_SHA256",
        "TYPOKAT_WU0B_PROCESS",
        "TYPOKAT_WU0C_UNIVERSE",
    ] {
        let missing = complete
            .iter()
            .copied()
            .filter(|(name, _)| *name != required)
            .collect::<Vec<_>>();
        assert!(
            resolve_release_config_from_values_for_test(&missing).is_err(),
            "missing {required} must fail closed"
        );
    }
    for (name, malformed) in [
        ("TYPOKAT_WU0C_SESSION_SHA256", "not-opaque"),
        ("TYPOKAT_WU0C_BINARY_SHA256", "not-opaque"),
        ("TYPOKAT_WU0C_HOST_SHA256", "not-opaque"),
        ("TYPOKAT_WU0C_WORKLOAD_PROFILE_SHA256", "not-opaque"),
        ("TYPOKAT_WU0C_UNIVERSE", "not-a-universe"),
    ] {
        let mut invalid = complete.clone();
        invalid
            .iter_mut()
            .find(|(candidate, _)| *candidate == name)
            .expect("required value exists")
            .1 = malformed;
        assert!(
            resolve_release_config_from_values_for_test(&invalid).is_err(),
            "malformed {name} must fail closed"
        );
    }
    for process in ["0", "6", "255", "not-a-process"] {
        let mut invalid = complete.clone();
        invalid
            .iter_mut()
            .find(|(candidate, _)| *candidate == "TYPOKAT_WU0B_PROCESS")
            .expect("process exists")
            .1 = process;
        assert!(
            resolve_release_config_from_values_for_test(&invalid).is_err(),
            "release process {process} is outside 1..=5"
        );
    }
}

#[test]
fn exact_checkpoint_at_4096_is_direct_reborrow_safe_and_emits_exact_shape() {
    let clock = AttributionTestClock::default();
    let sink = AttributionTestSink::default();
    let scope = start_attribution_for_test(config(AttributionMode::Exact, 1), &clock, &sink)
        .expect("exact scope");
    let control = scope.control_for_test();
    let pass = control.capture_pass_for_test().expect("exact Pass");
    let run = pass.capture_substitution_for_test(
        FAMILY,
        &[(TypeParamId(7), TypeId(11)), (TypeParamId(9), TypeId(13))],
        Some(TypeId(41)),
    );
    for _ in 0..4_096 {
        run.record_visit_for_test(TypeId(73), &[TypeParamId(7), TypeParamId(9)]);
    }
    control.reporter_barrier_for_test();
    let rendered = sink.rendered_lines();
    assert!(rendered
        .iter()
        .any(|line| line.contains(" kind=run ") && line.contains(" visits=4096 ")));
    assert!(rendered.iter().any(|line| {
        line.contains(" kind=state ")
            && line.contains(" map=7:11,9:13 ")
            && line.contains(" application=41|7:11,9:13 ")
    }));
    assert!(sink
        .parsed_lines()
        .iter()
        .any(|line| { matches!(line, AttributionLine::Event(event) if event.is_exit()) }));
    drop(scope);
}

#[test]
fn recursive_crossing_of_exact_checkpoint_threshold_cannot_skip_4096() {
    let clock = AttributionTestClock::default();
    let sink = AttributionTestSink::default();
    let scope = start_attribution_for_test(config(AttributionMode::Exact, 1), &clock, &sink)
        .expect("exact scope");
    let control = scope.control_for_test();
    let pass = control.capture_pass_for_test().expect("exact Pass");
    let run = pass.capture_substitution_for_test(FAMILY, &[], None);
    for _ in 0..4_095 {
        run.record_visit_for_test(TypeId(73), &[]);
    }
    let outer = run
        .enter_visit_for_test(TypeId(73), &[])
        .expect("visit 4096 enters");
    let child = run
        .enter_visit_for_test(TypeId(73), &[])
        .expect("recursive visit 4097 enters before 4096 exits");
    run.finish_cycle_visit_for_test(child);
    run.finish_tainted_visit_for_test(outer);
    control.reporter_barrier_for_test();
    let checkpoint = sink
        .rendered_lines()
        .into_iter()
        .find(|line| line.contains(" kind=run ") && line.contains(" checkpoint=1 "))
        .expect("crossing 4096 emits a checkpoint");
    assert!(checkpoint.contains(" visits=4097 "));
    drop(scope);
}

#[test]
fn progress_hot_path_is_local_and_batches_exactly_every_4096_visits() {
    let clock = AttributionTestClock::default();
    let sink = AttributionTestSink::default();
    let scope = start_attribution_for_test(config(AttributionMode::Progress, 1), &clock, &sink)
        .expect("progress scope");
    let control = scope.control_for_test();
    let pass = control
        .capture_pass_for_test()
        .expect("progress captures Pass");
    let run = pass.capture_substitution_for_test(
        FAMILY,
        &[(TypeParamId(7), TypeId(11))],
        Some(TypeId(41)),
    );
    for _ in 0..8_193 {
        run.record_visit_for_test(TypeId(73), &[TypeParamId(7)]);
    }
    control.report_now_and_wait_for_test();
    let rendered = sink.rendered_lines();
    let checkpoint_runs = rendered
        .iter()
        .filter(|line| line.contains(" kind=run ") && line.contains(" checkpoint=1 "))
        .collect::<Vec<_>>();
    assert_eq!(checkpoint_runs.len(), 2);
    assert!(checkpoint_runs
        .iter()
        .any(|line| line.contains(" visits=4096 ")));
    assert!(checkpoint_runs
        .iter()
        .any(|line| line.contains(" visits=8192 ")));
    assert!(sink
        .parsed_lines()
        .iter()
        .all(|line| !matches!(line, AttributionLine::State(_) | AttributionLine::Event(_))));
    assert_eq!(sink.batch_count(), sink.flush_count());
    for line in rendered {
        assert!(!line.contains(" type_id="));
        assert!(!line.contains(" map="));
        assert!(!line.contains(" application="));
        assert!(!line.contains(" disposition="));
        assert!(!line.contains(" action="));
        assert!(!line.contains(" parent="));
        assert!(!line.contains(" state="));
        assert!(!line.contains(" event="));
        assert!(!line.contains(" universe="));
    }
    drop(scope);
}

#[test]
fn phase_updates_and_fixed_heartbeat_deadline_survive_continuous_traffic() {
    let mut test_config = config(AttributionMode::Progress, 1);
    test_config.checkpoint_visits = 1;
    let clock = AttributionTestClock::default();
    let sink = AttributionTestSink::default();
    let scope = start_attribution_for_test(test_config, &clock, &sink).expect("progress scope");
    let control = scope.control_for_test();
    let pass = control.capture_pass_for_test().expect("progress Pass");
    let run = pass.capture_substitution_for_test(FAMILY, &[], None);

    control.enter_phase(AttributionPhase::ReserveFill);
    for _ in 0..100 {
        clock.advance_us(48_000);
        run.record_visit_for_test(TypeId(73), &[]);
        control.enqueue_control_traffic_for_test();
    }
    control.reporter_barrier_for_test();

    clock.advance_us(20_000);
    control.enter_phase(AttributionPhase::PublicationValidation);
    control.enter_phase(AttributionPhase::StatementCheck);
    control.reporter_barrier_for_test();
    assert_eq!(control.coalesced_phase_updates_for_test(), 1);
    let phase_update = sink
        .parsed_lines()
        .into_iter()
        .rev()
        .find_map(|line| match line {
            AttributionLine::Heartbeat(heartbeat) => Some(heartbeat),
            _ => None,
        })
        .expect("coalesced phase update reaches reporter immediately");
    assert_eq!(phase_update.phase(), AttributionPhase::StatementCheck);
    assert_eq!(phase_update.reporter_elapsed_us, 4_820_000);
    assert_eq!(phase_update.checkpoint_elapsed_us, 4_800_000);

    for _ in 0..180 {
        clock.advance_us(1_000);
        control.enqueue_control_traffic_for_test();
    }
    assert!(control.fire_due_heartbeat_for_test());
    control.reporter_barrier_for_test();
    let fixed_deadlines = sink
        .parsed_lines()
        .into_iter()
        .filter_map(|line| match line {
            AttributionLine::Heartbeat(heartbeat)
                if heartbeat.reporter_elapsed_us.is_multiple_of(250_000) =>
            {
                Some(heartbeat.reporter_elapsed_us)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        fixed_deadlines,
        (1_u64..=20).map(|tick| tick * 250_000).collect::<Vec<_>>()
    );
    let heartbeat = sink
        .parsed_lines()
        .into_iter()
        .rev()
        .find_map(|line| match line {
            AttributionLine::Heartbeat(heartbeat) => Some(heartbeat),
            _ => None,
        })
        .expect("fixed 250ms heartbeat is not starved");
    assert_eq!(heartbeat.phase(), AttributionPhase::StatementCheck);
    assert_eq!(heartbeat.reporter_elapsed_us, 5_000_000);
    assert_eq!(heartbeat.checkpoint_elapsed_us, 4_800_000);
    assert_ne!(
        heartbeat.reporter_elapsed_us,
        heartbeat.checkpoint_elapsed_us
    );
    drop(scope);
}

#[test]
fn one_miss_application_owns_family_time_while_nested_substitutions_own_counters() {
    let clock = AttributionTestClock::default();
    let sink = AttributionTestSink::default();
    let scope = start_attribution_for_test(config(AttributionMode::Progress, 1), &clock, &sink)
        .expect("progress scope");
    let control = scope.control_for_test();
    control.enter_phase(AttributionPhase::ReserveFill);
    let pass = control
        .capture_pass_for_test()
        .expect("progress captures Pass");
    let application = pass.start_ready_application_for_test(FAMILY);
    clock.advance_us(100);
    let outer_run =
        application.capture_substitution_for_test(&[(TypeParamId(7), TypeId(11))], None);
    outer_run.record_visit_for_test(TypeId(73), &[TypeParamId(7)]);
    clock.advance_us(100);
    let nested_run =
        outer_run.capture_nested_substitution_for_test(&[(TypeParamId(9), TypeId(13))], None);
    nested_run.record_visit_for_test(TypeId(73), &[TypeParamId(9)]);
    nested_run.record_visit_for_test(TypeId(73), &[TypeParamId(9)]);
    clock.advance_us(200);
    nested_run.finish_clean_for_test();
    clock.advance_us(50);
    outer_run.finish_tainted_for_test();
    clock.advance_us(50);
    application.finish_miss_tainted_for_test();
    let batches_before = sink.batch_count();
    let flushes_before = sink.flush_count();
    control.accept_checkpoint_and_report_for_test();
    let eager = sink
        .parsed_lines()
        .into_iter()
        .filter_map(|line| match line {
            AttributionLine::Eager(line) => Some(line),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(eager.len(), 1, "one Ready application owns family time");
    assert!(eager[0].arithmetic_is_exact());
    assert_eq!(eager[0].calls, 1);
    assert_eq!(eager[0].hits, 0);
    assert_eq!(eager[0].misses, 1);
    assert_eq!(eager[0].tainted, 1);
    assert_eq!(eager[0].completed_us, 500);
    assert_eq!(eager[0].active_us, 0);

    let runs = sink
        .parsed_lines()
        .into_iter()
        .filter_map(|line| match line {
            AttributionLine::Run(line) => Some(line),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|line| line.family_sha256 == FAMILY));
    let mut visits = runs.iter().map(|line| line.visits).collect::<Vec<_>>();
    visits.sort_unstable();
    assert_eq!(visits, [1, 2]);
    assert_eq!(sink.batch_count(), batches_before + 1);
    assert_eq!(sink.flush_count(), flushes_before + 1);
    drop(scope);
}

#[test]
fn captured_handles_survive_nested_ambient_scope_then_become_stale() {
    let outer_clock = AttributionTestClock::default();
    let outer_sink = AttributionTestSink::default();
    let outer = start_attribution_for_test(
        config(AttributionMode::Progress, 1),
        &outer_clock,
        &outer_sink,
    )
    .expect("outer scope");
    let pass = outer
        .control_for_test()
        .capture_pass_for_test()
        .expect("outer Pass owner");

    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let inner_clock = AttributionTestClock::default();
        let inner_sink = AttributionTestSink::default();
        let _inner = start_attribution_for_test(
            config(AttributionMode::Exact, 2),
            &inner_clock,
            &inner_sink,
        )
        .expect("nested exact scope");
        let run = pass.capture_substitution_for_test(FAMILY, &[(TypeParamId(7), TypeId(11))], None);
        run.record_visit_for_test(TypeId(73), &[TypeParamId(7)]);
        assert_eq!(run.session_process_for_test(), 1);
        panic!("intentional nested unwind");
    }));
    assert!(unwind.is_err());
    assert_eq!(
        super::wu0c_attribution::current_session_for_test()
            .expect("outer restored")
            .process(),
        1
    );
    drop(outer);
    let before = outer_sink.rendered_lines().len();
    assert!(!pass.record_ready_hit_for_test(FAMILY));
    assert_eq!(outer_sink.rendered_lines().len(), before);
}

#[test]
fn merged_interface_registration_and_eager_application_emit_the_independent_token() {
    let expected = canonical_family_token(&[
        FamilyParticipant::new(LibraryFileOrdinal::new(0), 0, TypeFragmentKind::Interface),
        FamilyParticipant::new(LibraryFileOrdinal::new(1), 0, TypeFragmentKind::Interface),
    ]);
    assert_eq!(expected.as_str(), MERGED_INTERFACE_FAMILY);

    let clock = AttributionTestClock::default();
    let sink = AttributionTestSink::default();
    let scope = start_attribution_for_test(config(AttributionMode::Progress, 1), &clock, &sink)
        .expect("attribution is on");
    let control = scope.control_for_test();
    control.enter_phase(AttributionPhase::ReserveFill);
    let profile = run_injected_profile(&[
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "left.d.ts",
            source: "interface Shared<T> { left: T }\ndeclare let useShared: Shared<string>;",
        },
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(1),
            name: "right.d.ts",
            source: "interface Shared<T> { right: T }",
        },
    ])
    .expect("merged generic interface profile");
    let group = profile
        .global_type_probe("Shared")
        .expect("both files bind to one type group");
    assert_eq!(group.declaration_count, 2);
    assert_eq!(
        control.registered_family_token_for_test(group.identity),
        Some(expected.clone())
    );
    control.report_now_and_wait_for_test();
    assert!(sink
        .parsed_lines()
        .iter()
        .any(|line| matches!(line, AttributionLine::Eager(eager)
            if eager.family_sha256 == MERGED_INTERFACE_FAMILY && eager.calls > 0)));
    drop(scope);
}

#[test]
fn scope_and_profile_run_share_one_thread_while_tiny_worker_is_unattributed() {
    let observed = std::thread::spawn(|| {
        let owner = std::thread::current().id();
        let clock = AttributionTestClock::default();
        let sink = AttributionTestSink::default();
        let scope = start_attribution_for_test(config(AttributionMode::Progress, 3), &clock, &sink)
            .expect("scope installed on execution thread");
        let control = scope.control_for_test();
        control.enter_phase(AttributionPhase::ReserveFill);
        run_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "focused.d.ts",
            source: "interface Focused<T> { value: T; }",
        }])
        .expect("focused profile");
        assert_eq!(control.last_semantic_thread_for_test(), Some(owner));
        assert!(control.registered_family_count_for_test() > 0);
        assert_eq!(control.unregistered_family_lookups_for_test(), 0);
        let captured = control.captured_passes_for_test();
        let tiny = check_source("export const tiny: number = 1;");
        assert!(tiny.parse_errors.is_empty());
        assert_eq!(control.captured_passes_for_test(), captured);
        drop(scope);
        (owner, control.reporter_thread_for_test())
    })
    .join()
    .expect("execution thread finishes");
    assert_ne!(Some(observed.0), observed.1);
}

#[test]
fn every_finite_limit_reserves_reporter_terminal_capacity_and_rejects_evidence() {
    for limit in LimitKind::ALL {
        let configured = tiny_limits(limit);
        assert_eq!(configured.terminal_reserve_lines, 2);
        assert!(configured.terminal_reserve_bytes >= 2 * configured.rendered_line_bytes);
        let mode = match limit {
            LimitKind::EagerKeys
            | LimitKind::Runs
            | LimitKind::CheckpointMessages
            | LimitKind::CheckpointBytes
            | LimitKind::Lines
            | LimitKind::FileBytes => AttributionMode::Progress,
            LimitKind::DictionaryEntries
            | LimitKind::TraceEvents
            | LimitKind::RenderedLineBytes
            | LimitKind::MapEntries
            | LimitKind::ContextEntries
            | LimitKind::ApplicationEntries
            | LimitKind::LiveExactBytes => AttributionMode::Exact,
        };
        let mut test_config = config(mode, 1);
        test_config.limits = configured;
        test_config.checkpoint_visits = 1;
        let clock = AttributionTestClock::default();
        let sink = AttributionTestSink::default();
        let scope = start_attribution_for_test(test_config, &clock, &sink)
            .expect("tiny real collector/reporter scope");
        let control = scope.control_for_test();
        let pass = control.capture_pass_for_test().expect("semantic Pass");

        match limit {
            LimitKind::EagerKeys => {
                let first = pass.start_ready_application_for_test(&format!("{:064x}", 1));
                first.finish_miss_tainted_for_test();
                assert_eq!(control.collector_counts_for_test().eager_keys, 1);
                let second = pass.start_ready_application_for_test(&format!("{:064x}", 2));
                second.finish_miss_tainted_for_test();
                let crossed = control.collector_counts_for_test();
                assert_eq!(crossed.eager_keys, 1);
                assert!(crossed.coverage_lost);
                let third = pass.start_ready_application_for_test(&format!("{:064x}", 3));
                third.finish_miss_tainted_for_test();
                assert_eq!(control.collector_counts_for_test().eager_keys, 1);
            }
            LimitKind::Runs => {
                let _first = pass.capture_substitution_for_test(&format!("{:064x}", 1), &[], None);
                assert_eq!(control.collector_counts_for_test().runs, 1);
                let _second = pass.capture_substitution_for_test(&format!("{:064x}", 2), &[], None);
                let crossed = control.collector_counts_for_test();
                assert_eq!(crossed.runs, 1);
                assert!(crossed.coverage_lost);
                let _third = pass.capture_substitution_for_test(&format!("{:064x}", 3), &[], None);
                assert_eq!(control.collector_counts_for_test().runs, 1);
            }
            LimitKind::DictionaryEntries | LimitKind::TraceEvents => {
                let first = pass.capture_substitution_for_test(&format!("{:064x}", 1), &[], None);
                first.record_visit_for_test(TypeId(73), &[]);
                let before = control.collector_counts_for_test();
                assert_eq!(before.dictionary_entries, 1);
                assert_eq!(before.trace_events, 3);
                let second = pass.capture_substitution_for_test(&format!("{:064x}", 2), &[], None);
                second.record_visit_for_test(TypeId(74), &[]);
                let crossed = control.collector_counts_for_test();
                assert!(crossed.coverage_lost);
                if limit == LimitKind::DictionaryEntries {
                    assert_eq!(crossed.dictionary_entries, 1);
                } else {
                    assert_eq!(crossed.trace_events, 3);
                }
                let third = pass.capture_substitution_for_test(&format!("{:064x}", 3), &[], None);
                third.record_visit_for_test(TypeId(75), &[]);
                let sticky = control.collector_counts_for_test();
                assert_eq!(sticky.dictionary_entries, crossed.dictionary_entries);
                assert_eq!(sticky.trace_events, crossed.trace_events);
            }
            LimitKind::MapEntries => {
                let first = pass.capture_substitution_for_test(
                    FAMILY,
                    &[(TypeParamId(7), TypeId(11))],
                    None,
                );
                first.record_visit_for_test(TypeId(73), &[]);
                assert_eq!(control.collector_counts_for_test().map_entries, 1);
                let second = pass.capture_substitution_for_test(
                    FAMILY,
                    &[(TypeParamId(7), TypeId(11)), (TypeParamId(9), TypeId(13))],
                    None,
                );
                second.record_visit_for_test(TypeId(74), &[]);
                let crossed = control.collector_counts_for_test();
                assert_eq!(crossed.map_entries, 1);
                assert!(crossed.coverage_lost);
                second.record_visit_for_test(TypeId(75), &[]);
                assert_eq!(control.collector_counts_for_test().map_entries, 1);
            }
            LimitKind::ContextEntries => {
                let run = pass.capture_substitution_for_test(FAMILY, &[], None);
                run.record_visit_for_test(TypeId(73), &[TypeParamId(7)]);
                assert_eq!(control.collector_counts_for_test().context_entries, 1);
                run.record_visit_for_test(TypeId(74), &[TypeParamId(7), TypeParamId(9)]);
                let crossed = control.collector_counts_for_test();
                assert_eq!(crossed.context_entries, 1);
                assert!(crossed.coverage_lost);
                run.record_visit_for_test(TypeId(75), &[TypeParamId(7), TypeParamId(9)]);
                assert_eq!(control.collector_counts_for_test().context_entries, 1);
            }
            LimitKind::ApplicationEntries => {
                let first = pass.capture_substitution_for_test(
                    &format!("{:064x}", 1),
                    &[],
                    Some(TypeId(41)),
                );
                first.record_visit_for_test(TypeId(73), &[]);
                assert_eq!(control.collector_counts_for_test().application_entries, 1);
                let second = pass.capture_substitution_for_test(
                    &format!("{:064x}", 2),
                    &[],
                    Some(TypeId(42)),
                );
                second.record_visit_for_test(TypeId(74), &[]);
                let crossed = control.collector_counts_for_test();
                assert_eq!(crossed.application_entries, 1);
                assert!(crossed.coverage_lost);
                let third = pass.capture_substitution_for_test(
                    &format!("{:064x}", 3),
                    &[],
                    Some(TypeId(43)),
                );
                third.record_visit_for_test(TypeId(75), &[]);
                assert_eq!(control.collector_counts_for_test().application_entries, 1);
            }
            LimitKind::LiveExactBytes => {
                let mut retained = Vec::new();
                for index in 1_u32..=16 {
                    let run = pass.capture_substitution_for_test(
                        &format!("{index:064x}"),
                        &[],
                        Some(TypeId(40 + index)),
                    );
                    run.record_visit_for_test(TypeId(70 + index), &[]);
                    retained.push(run);
                    if control.collector_counts_for_test().coverage_lost {
                        break;
                    }
                }
                let crossed = control.collector_counts_for_test();
                assert!(
                    retained.len() >= 2,
                    "global bytes cross across distinct runs"
                );
                assert!(crossed.live_exact_bytes <= configured.live_exact_bytes);
                assert!(crossed.coverage_lost);
                let post = pass.capture_substitution_for_test(&format!("{:064x}", 99), &[], None);
                post.record_visit_for_test(TypeId(99), &[]);
                assert_eq!(
                    control.collector_counts_for_test().live_exact_bytes,
                    crossed.live_exact_bytes
                );
            }
            LimitKind::CheckpointMessages | LimitKind::CheckpointBytes => {
                control.pause_reporter_for_test();
                let before = control.checkpoint_queue_counts_for_test();
                let first = pass.capture_substitution_for_test(&format!("{:064x}", 1), &[], None);
                first.record_visit_for_test(TypeId(73), &[]);
                let after_first = control.checkpoint_queue_counts_for_test();
                assert!(after_first.messages <= configured.checkpoint_messages);
                assert!(after_first.bytes <= configured.checkpoint_bytes);
                assert_eq!(
                    after_first.messages - before.messages,
                    usize::from(limit == LimitKind::CheckpointMessages)
                );
                let second = pass.capture_substitution_for_test(&format!("{:064x}", 2), &[], None);
                second.record_visit_for_test(TypeId(74), &[]);
                let crossed = control.checkpoint_queue_counts_for_test();
                assert_eq!(crossed.messages, after_first.messages);
                assert_eq!(crossed.bytes, after_first.bytes);
                assert!(control.collector_counts_for_test().coverage_lost);
                let third = pass.capture_substitution_for_test(&format!("{:064x}", 3), &[], None);
                third.record_visit_for_test(TypeId(75), &[]);
                let sticky = control.checkpoint_queue_counts_for_test();
                assert_eq!(sticky.messages, crossed.messages);
                assert_eq!(sticky.bytes, crossed.bytes);
                control.resume_reporter_for_test();
            }
            LimitKind::RenderedLineBytes => {
                let run = pass.capture_substitution_for_test(FAMILY, &[], Some(TypeId(41)));
                let context = (0_u32..256).map(TypeParamId).collect::<Vec<_>>();
                run.record_visit_for_test(TypeId(73), &context);
            }
            LimitKind::Lines | LimitKind::FileBytes => {
                let count = if limit == LimitKind::Lines { 4 } else { 16 };
                for index in 1..=count {
                    let application =
                        pass.start_ready_application_for_test(&format!("{index:064x}"));
                    application.finish_miss_tainted_for_test();
                }
            }
        }

        control.report_now_and_wait_for_test();
        control.reporter_barrier_for_test();
        let after_invalid_lines = sink.rendered_lines();
        let after_invalid_bytes = after_invalid_lines
            .iter()
            .map(|line| line.len() + 1)
            .sum::<usize>();
        assert!(after_invalid_lines
            .iter()
            .filter_map(|line| parse_attribution_line(line).ok())
            .any(
                |line| matches!(line, AttributionLine::Invalid(invalid) if invalid.limit == limit)
            ));
        control.report_now_and_wait_for_test();
        control.reporter_barrier_for_test();
        assert_eq!(
            sink.rendered_lines(),
            after_invalid_lines,
            "invalidation is sticky"
        );
        assert_eq!(
            sink.rendered_lines()
                .iter()
                .map(|line| line.len() + 1)
                .sum::<usize>(),
            after_invalid_bytes
        );
        drop(scope);

        let lines = sink.rendered_lines();
        let bytes = lines.iter().map(|line| line.len() + 1).sum::<usize>();
        assert!(lines.len() <= configured.lines);
        assert!(bytes <= configured.file_bytes);
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains(" kind=invalid "))
                .count(),
            1
        );
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains(" kind=finish "))
                .count(),
            1
        );
        assert!(lines
            .iter()
            .any(|line| line.contains(" kind=finish ") && line.contains(" coverage_lost=1")));
        assert!(validate_session_evidence(&lines, Termination::Normal).is_err());
    }
}

#[test]
fn one_shot_sink_failure_persists_invalidation_and_never_looks_like_clean_deadline() {
    let clock = AttributionTestClock::default();
    let sink = AttributionTestSink::default();
    let scope = start_attribution_for_test(config(AttributionMode::Exact, 1), &clock, &sink)
        .expect("exact scope");
    let control = scope.control_for_test();
    control.enter_phase(AttributionPhase::ReserveFill);
    let pass = control.capture_pass_for_test().expect("exact Pass");
    let application = pass.start_ready_application_for_test(FAMILY);
    let run = application.capture_substitution_for_test(&[], Some(TypeId(41)));
    let root = run
        .enter_visit_for_test(TypeId(73), &[])
        .expect("cycle root enters");
    for _ in 0..4_095 {
        let child = run
            .enter_visit_for_test(TypeId(73), &[])
            .expect("raw reentry child enters");
        run.finish_cycle_visit_for_test(child);
    }
    clock.advance_us(4_800_000);
    run.finish_tainted_visit_for_test(root);
    control.reporter_barrier_for_test();
    clock.advance_us(200_000);
    assert!(control.fire_due_heartbeat_for_test());
    control.reporter_barrier_for_test();
    let clean_prefix = sink.rendered_lines();
    assert!(validate_session_evidence(
        &clean_prefix,
        Termination::Deadline {
            elapsed_us: 5_000_000
        }
    )
    .is_ok());

    sink.fail_next_write_for_test();
    control.report_now_and_wait_for_test();
    control.reporter_barrier_for_test();
    assert_eq!(sink.write_failures_for_test(), 1);
    let lines = sink.rendered_lines();
    assert!(lines.iter().filter_map(|line| parse_attribution_line(line).ok()).any(
        |line| matches!(line, AttributionLine::Invalid(invalid) if invalid.is_sink_write_failure())
    ));
    assert!(lines.starts_with(&clean_prefix));
    assert!(validate_session_evidence(
        &lines,
        Termination::Deadline {
            elapsed_us: 5_000_000
        }
    )
    .is_err());
    drop(application);
    drop(scope);
}

#[test]
fn activation_has_exact_module_adjacency_named_hooks_and_direct_probe_install_order() {
    let checker_mod = include_str!("mod.rs");
    let checker_context = include_str!("context.rs");
    let substitute = include_str!("../../types/substitute/mod.rs");
    let resolve = include_str!("decls/resolve.rs");
    let wu0b = include_str!("wu0b_library.rs");
    let attribution = include_str!("wu0c_attribution.rs");
    const ACTIVATION: &str =
        "#[cfg(test)]\nmod wu0c_attribution;\n#[cfg(test)]\nmod wu0c_post_cache_attribution_spec;";
    assert_eq!(checker_mod.matches(ACTIVATION).count(), 1);
    assert_eq!(
        checker_mod.matches("capture_wu0c_pass_attribution").count()
            + checker_context
                .matches("capture_wu0c_pass_attribution")
                .count(),
        1
    );
    assert_eq!(
        substitute
            .matches("capture_wu0c_substitution_attribution")
            .count(),
        1
    );
    assert_eq!(
        resolve
            .matches("start_wu0c_ready_application_attribution")
            .count(),
        1
    );
    assert_eq!(wu0b.matches("register_wu0c_family_tokens").count(), 1);

    assert!(!checker_context.contains("wu0c_ready_group"));
    assert!(!resolve.contains("wu0c_ready_group"));
    let ready_signature = resolve
        .find("fn substitute_ready_type_group_application")
        .map(|start| &resolve[start..])
        .and_then(|tail| tail.split_once('{').map(|(signature, _)| signature))
        .expect("Ready substitution has an explicit signature");
    assert!(ready_signature.contains("group: TypeGroupId"));
    let ready_call = resolve
        .rfind("substitute_ready_type_group_application(")
        .map(|start| &resolve[start..])
        .expect("Ready path consumes its group at the callsite");
    assert!(ready_call[..ready_call.find(')').expect("call closes")].contains("decl_id"));

    let release_start = attribution
        .find("pub(super) fn start_wu0c_attribution_from_env()")
        .expect("real release entrypoint");
    let release_end = attribution[release_start..]
        .find("pub(in crate::check::checker) fn capture_wu0c_pass_attribution")
        .map(|offset| release_start + offset)
        .expect("release entrypoint has a bounded source slice");
    let release_entrypoint = &attribution[release_start..release_end];
    assert_eq!(
        release_entrypoint
            .matches("resolve_release_config_from_values(")
            .count(),
        1
    );
    let delegated = release_entrypoint
        .find("resolve_release_config_from_values(")
        .expect("real entrypoint delegates to pure resolver");
    let opens_sink = release_entrypoint
        .find("OpenOptions::new()")
        .expect("sink opens only after validation");
    assert!(delegated < opens_sink);
    assert!(!release_entrypoint.contains("unwrap_or(1)"));
    assert!(!release_entrypoint.contains("unwrap_or_else"));

    // Source occurrence checks are deliberately small; hot-path cost still requires review and
    // compiler inspection before activation. The explicit Ready argument is the one-shot ownership
    // boundary: partial, nongeneric, and lazy bypasses cannot leave an ambient family for a later
    // Ready application.

    let bind_complete = wu0b
        .find("let bind_elapsed")
        .expect("binding completes before registration");
    let register = wu0b
        .find("register_wu0c_family_tokens")
        .expect("family tokens pre-register once");
    let reserve_fill = wu0b
        .find("let reserve_fill_started")
        .expect("hot semantic phases follow registration");
    assert!(bind_complete < register && register < reserve_fill);

    let probe_start = wu0b
        .find("fn wu0b_release_probe_once()")
        .expect("ignored release probe");
    let probe = &wu0b[probe_start..];
    let install = probe
        .find("start_wu0c_attribution_from_env")
        .expect("opt-in scope install");
    let execute = probe
        .find("run_injected_profile(")
        .expect("direct profile execution");
    assert!(install < execute);
    assert!(!probe[..execute].contains("std::thread::Builder"));
}
