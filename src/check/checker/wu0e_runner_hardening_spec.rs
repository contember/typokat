//! Disabled RED acceptance delta for WU0E runner hardening.
//!
//! Do not wire this module in `checker/mod.rs` in the spec commit. After that commit, implement the
//! contract and activate it with exactly:
//!
//! ```text
//! #[cfg(test)]
//! mod wu0e_runner_hardening_spec;
//! ```
//!
//! The behavioral test remains ignored and must be invoked explicitly. It is diagnostic-only and
//! cannot produce, validate, or relax WU0D release evidence.
//!
//! ## Delegated cgroup-v2 containment
//!
//! The outer coordinator reexecutes itself exactly once with this absolute argv:
//!
//! ```text
//! /usr/bin/systemd-run --user --scope --quiet --no-ask-password
//!   --property=Delegate=yes --expand-environment=no --
//!   /usr/bin/perl ABS_SCRIPT ARGS
//! ```
//!
//! `--wait`, `--pipe`, `--pty`, and shell wrapping are forbidden. Stdout, stderr, and exit status
//! pass through unchanged. Forged or nested reexec markers reject, and exactly one `systemd-run`
//! launch occurs.
//!
//! Inside the scope, the coordinator derives the exact cgroup from `/proc/self/cgroup` and
//! cross-checks its unit `ControlGroup` and `Delegate` through absolute
//! `/usr/bin/systemctl --user`. It never falls back to a writable ancestor. To satisfy the
//! no-internal-process rule, it creates `supervisor/`, moves itself there, proves the delegated
//! root's `cgroup.procs` empty, and only then enables `+memory`. The supervisor remains outside
//! every exclusive workload/validator launch cgroup.
//!
//! Mandatory pre-fork preflight opens and verifies `cgroup.type=domain`, `cgroup.procs`,
//! `cgroup.events`, writable `cgroup.kill`, `memory.max`, `memory.swap.max`, `memory.oom.group`,
//! `memory.current`, `memory.peak`, and `memory.events.local`. It writes and reads back
//! `memory.max=1073741824`, `memory.swap.max=0`, and `memory.oom.group=1`. The child's first
//! post-fork action moves itself into the launch cgroup, before `setsid`, readiness, environment
//! setup, or direct stable-handle exec. The parent accepts readiness only after independently
//! verifying membership and the new process group. Descendants inherit membership.
//! The real diagnostic and all ordinary fixtures retain the 1 GiB value. Only the isolated
//! test-only kernel-OOM fixture uses a fixed 64 MiB cgroup, below the acceptance watchdog's 512 MiB
//! RSS threshold; its metadata is validated against 64 MiB rather than the production value.
//!
//! Exact sampled RSS enumerates a `cgroup.procs` snapshot with fixed retry count and deadline,
//! excludes zombie/dead members, treats lowercase Linux state `t` as live and `x` as dead, retries
//! members that vanished with the snapshot, and fails infrastructure on a stably unreadable member,
//! unresolved churn, or arithmetic uncertainty. The 10 ms sample interval is attribution telemetry,
//! not a safety bound: an intentionally delayed sample must remain `normal`.
//!
//! `memory.current`, `memory.peak`, and numeric baseline/final/delta values from
//! `memory.events.local` remain distinct from summed RSS. A real low-limit kernel fixture must
//! produce causal `max`, `oom`, `oom_kill`, or `oom_group_kill` deltas and a matching
//! `memory_source`; `max` alone means cap contact, not invented OOM. These cgroup metrics are the
//! hard memory backstop.
//!
//! Cleanup polls `cgroup.events:populated` and the confirmed PGID, retains the leader zombie while
//! descendants live, redundantly attempts direct-child, PGID, and `cgroup.kill`, reads final events,
//! reaps the leader, and removes exactly the launch cgroup. A monitor exception persists metadata
//! before reporting infrastructure failure and must finish this cleanup. The retention-policy test
//! uses an explicitly synthetic, Rust-owned cgroup/populated/drain-view input; it does not claim to
//! create a deterministic kernel task state. That injected drain expiry persists both bounded
//! emergency attempts, launches no validator, records `cgroup_retained=1`, requests scope abort,
//! and stops. Separate process fixtures exercise real cleanup and identity disappearance. Scope
//! disappearance is verified only by the Rust parent after the evidence process exits.
//!
//! A retained production failure durably persists its process metadata, re-verifies the delegated
//! scope identity, and then executes exactly
//! `/usr/bin/systemctl --user --no-block stop UNIT`. Evidence mode exercises the same command
//! construction and identity policy through a Rust-owned injected spy exactly once. The spy removes
//! no real scope; the injected callback removes and verifies only the synthetic retained launch
//! cgroup. Its runner-owned outcome leaves outer scope disappearance `pending`; a separate
//! Rust-parent outcome may record disappearance after independently observing the real scope path
//! absent. Normal evidence-scope completion is never attributed to the injected abort request.
//! This also applies when an exception escapes after fork: if exception cleanup retains the launch
//! cgroup, complete mandatory process metadata is fsynced before the verified abort request, and
//! only then may the original exception propagate.
//!
//! Delegated-root teardown disables only a controller enabled by this runner, moves the coordinator
//! from `supervisor/` back to the delegated root, proves `supervisor/` empty, and removes it. Any
//! teardown failure is infrastructure and stops the schedule.
//!
//! ## Concrete evidence, not self-attestation
//!
//! `--self-test-evidence` consumes Rust-owned fixture files and writes bounded artifacts. The Rust
//! acceptance below independently verifies exact dossier bytes and digest, exact mismatch error,
//! actual termination outputs for overlapping flags (including post-read discoveries), lowercase
//! state parser outputs, exact shared-scheduler callback journals, stable-exec output, unchanged
//! victim digest/inode, persisted process metadata, dead child identities, and removed cgroups.
//! Boolean `case=x passed=1` claims are not evidence and are not accepted.
//!
//! Every process metadata record and dossier row persists scope/control-group identities, launch
//! cgroup, configuration readbacks, sampled RSS peak, `memory.current`/`memory.peak`, event
//! baselines/finals/deltas/source, readiness/membership, containment attempts, and cleanup state.
//! The one termination adjudicator applies, including post-read flags:
//! `infrastructure > trace > stdout > stderr > rss > deadline > crash/normal`.
//!
//! The frozen executable is launched through a stable opened handle. Replacing its source path must
//! execute `trusted_marker=1`, never `replacement_marker=1`, and path drift must reject. Temporary
//! publication is exclusive, no-follow, and no-replace beneath verified real directories. Artifact
//! inode replacement rejects and the Rust-owned victim's inode and digest remain unchanged.
//! Path identity is reverified after every completed workload and validator launch. The acceptance
//! replaces the final candidate-b validator pathname only after that validator completes and proves
//! both stable-handle execution and rejection before the schedule can succeed.
//!
//! Self-test schedule, environment, identity, and preflight observations travel through the same
//! production descriptor and hardened-launch hooks as an ordinary run. A Rust-owned dynamic seed
//! and executable observation probe make those hook inputs externally visible. The preflight action
//! ordering is a real child/parent trace tied by digest to `preflight.meta`, not a string literal.
//! The former v1 workload/validator/dossier production route is absent. The independent containment
//! self-test and its supervisor remain a separate active diagnostic contract.
//!
//! The acceptance itself never uses unbounded `Command::output()`. A Rust-owned watchdog runs the
//! runner in a fresh session. Two bounded pipe readers accept at most limit+1 bytes and then close,
//! so stdout/stderr are OS-bounded without a shell, unsafe code, or resource-limit shim. The
//! watchdog polls summed descendant RSS, performs a final post-exit sample/check, enforces a
//! deadline, and kills the outer process tree on failure; it does not misdescribe polling as an
//! absolute hard RSS cap. Once reexeced, the launch fixtures additionally have their real cgroup
//! memory backstops. Scratch is removed only after success, so failure artifacts survive.
//!
//! ## WU0G single-child protocol
//!
//! `--wu0g-child-v1 REQUEST_FILE RESULT_DIR` admits exactly one canonical, sorted,
//! length/domain-framed request and one hardened launch. It accepts only causal or performance,
//! baseline or candidate, the kind-appropriate exact rung/pair/launch ordinal and identities, a
//! frozen libtest digest, unique request/sentinel/artifact identities, and conservative nonzero
//! deadline, memory, RSS, and nofile limits. Missing, duplicate, unknown, reordered, or inapplicable
//! fields reject. The runner derives the sole exact ignored libtest filter and its fixed WU0G
//! environment after scrubbing inherited WU0B through WU0G variables; the request cannot supply
//! argv, commands, shell text, or environment keys.
//!
//! The existing hardened supervisor owns termination, PGID/cgroup containment, bounded output,
//! reap, and cleanup. It reads back deadline, memory, RSS, and nofile; the child independently reads
//! `/proc/self/limits`. Libtest, `/usr/bin/prlimit`, `/usr/bin/perf`, and any setup helper are opened
//! and revalidated as stable executable handles. Performance alone uses authenticated perf in
//! command mode with the exact pinned `instructions:u` argv and default descendant inheritance.
//! Its exclusive no-follow <=4 KiB artifact must be one seven-semicolon-field row. The count is
//! unavailable unless the outer result is normal, perf itself exited zero without signal, the
//! checker sentinel/artifact authenticate, and OOM, containment, reap, and cleanup are clean.
//! Partial counts never authorize, and perf never selectively finalizes a killed workload.

#![cfg(target_os = "linux")]

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_CAPTURE_BYTES: u64 = 512 * 1_024;
const MAX_OUTER_RSS_BYTES: u64 = 512 * 1_024 * 1_024;
const OUTER_DEADLINE: Duration = Duration::from_secs(90);
const OUTER_DRAIN: Duration = Duration::from_secs(2);
const A_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BINARY_SHA: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const HOST_SHA: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const PROFILE_SHA: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const INVENTORY_SHA: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
const WU0G_CHILD_FILTER: &str =
    "check::checker::decls::wu0g_interface_fill_attribution_spec::wu0g_hardened_child_once";
const WU0G_REQUEST_FIELDS: &[&str] = &[
    "artifact_relative_path",
    "binary_identity",
    "candidate_identity",
    "cpu_affinity",
    "deadline_ms",
    "host_identity",
    "kind",
    "launch_identity",
    "launch_ordinal",
    "libtest_relative_path",
    "memory_limit_bytes",
    "mode",
    "nofile_hard",
    "nofile_soft",
    "nonce",
    "pair_identity",
    "pair_ordinal",
    "perf_event",
    "perf_identity",
    "perf_version",
    "plan_identity",
    "prlimit_identity",
    "request_relative_path",
    "result_identity",
    "result_relative_path",
    "rss_limit_bytes",
    "rung_identity",
    "rung_ordinal",
    "semantic_artifact_relative_path",
    "sentinel_relative_path",
    "workload_identity",
];
const WU0G_ENV_ALLOWLIST: &[&str] = &[
    "TYPOKAT_WU0G_CHILD_REQUEST_FD",
    "TYPOKAT_WU0G_CHILD_REQUEST_SHA256",
    "TYPOKAT_WU0G_CHILD_RESULT_DIR_FD",
];
const WU0G_SENTINEL_FIELDS: &[&str] = &[
    "argv",
    "environment",
    "fd_inventory",
    "nofile_hard",
    "nofile_soft",
    "request_content_identity",
    "semantic_artifact_identity",
    "semantic_artifact_size",
];
const WU0G_RESULT_FIELDS: &[&str] = &[
    "artifact_identity",
    "artifact_size",
    "binary_identity",
    "cgroup_identity",
    "cgroup_populated_zero",
    "cgroup_removed",
    "cgroup_retained",
    "child_argv",
    "child_env",
    "child_fd_inventory",
    "child_identity",
    "cleanup_succeeded",
    "containment_failures",
    "deadline_ms",
    "deadline_readback_ms",
    "drain_complete",
    "exit_code",
    "host_identity",
    "launch_identity",
    "leader_pid",
    "leader_reaped",
    "leader_start_ticks",
    "max_rss_bytes",
    "memory_limit_bytes",
    "memory_limit_readback_bytes",
    "membership_verified",
    "nofile_hard",
    "nofile_hard_readback",
    "nofile_soft",
    "nofile_soft_readback",
    "oom_delta",
    "oom_kill_delta",
    "outer_raw_wait_status",
    "perf_artifact_identity",
    "perf_artifact_size",
    "perf_event",
    "perf_exit_code",
    "perf_identity",
    "perf_invocation",
    "perf_raw_wait_status",
    "perf_term_signal",
    "perf_version",
    "pgid",
    "pgid_empty",
    "plan_identity",
    "prlimit_identity",
    "readiness_seen",
    "request_content_identity",
    "result_identity",
    "rss_limit_bytes",
    "rss_limit_readback_bytes",
    "scope_abort_observed",
    "scope_abort_requested",
    "scope_identity",
    "sentinel_identity",
    "sentinel_size",
    "stderr_identity",
    "stderr_size",
    "stdout_identity",
    "stdout_size",
    "term_signal",
    "termination",
];
const WU0G_REQUEST_CAP_BYTES: u64 = 64 * 1_024;
const WU0G_RESULT_CAP_BYTES: u64 = 64 * 1_024;
const WU0G_STDIO_CAP_BYTES: u64 = 128 * 1_024;
const WU0G_SENTINEL_CAP_BYTES: u64 = 4 * 1_024;
const WU0G_ARTIFACT_CAP_BYTES: u64 = 256 * 1_024;
const WU0G_PERF_ARTIFACT_CAP_BYTES: u64 = 4 * 1_024;
const WU0G_FORBIDDEN_ENV: &[(&str, &str)] = &[
    ("TYPOKAT_WU0B_FORBIDDEN_CANARY", "wu0b-must-be-scrubbed"),
    ("TYPOKAT_WU0C_FORBIDDEN_CANARY", "wu0c-must-be-scrubbed"),
    ("TYPOKAT_WU0D_FORBIDDEN_CANARY", "wu0d-must-be-scrubbed"),
    ("TYPOKAT_WU0E_FORBIDDEN_CANARY", "wu0e-must-be-scrubbed"),
    ("TYPOKAT_WU0F_FORBIDDEN_CANARY", "wu0f-must-be-scrubbed"),
    ("TYPOKAT_WU0G_FORBIDDEN_CANARY", "wu0g-must-be-scrubbed"),
];
const WU0G_CHILD_ARGV: &str = "--ignored|--exact|check::checker::decls::wu0g_interface_fill_attribution_spec::wu0g_hardened_child_once|--nocapture";
const WU0G_LEGACY_SELF_TEST_INVENTORY: &[u8] = b"setsid-containment\n\
pre-setsid-direct-kill\n\
zombie-leader-reservation\n\
leader-exit-descendant-kill\n\
summed-live-group-rss\n\
rss-sampling-interval\n\
stdout-flood\n\
stderr-flood\n\
trace-flood\n\
bounded-drain\n\
bounded-post-read\n\
rss-sampling-failure\n\
rss-arithmetic-overflow\n\
binary-swap\n\
environment-scrub\n\
workload-allowlist\n\
validator-allowlist\n\
validator-after-each-workload\n\
exact-primary-probe\n\
no-alternate-compiler\n\
same-binary-validator\n\
pre-post-binary-digest\n\
one-frozen-binary\n\
warm-inventory-before-every-launch\n\
same-binary-host-profile-inventory\n\
cross-mode-identity-parity\n";

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

struct AcceptanceScratch {
    root: PathBuf,
    fixtures: PathBuf,
    evidence: PathBuf,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl AcceptanceScratch {
    fn create() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("wu0e-runner-hardening-acceptance");
        std::fs::create_dir_all(&base).expect("create acceptance root");
        assert_real_directory(&base);
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let root = base.join(format!("{}-{serial}", std::process::id()));
        std::fs::create_dir(&root).expect("create exclusive acceptance scratch");
        let fixtures = root.join("fixtures");
        let evidence = root.join("evidence");
        std::fs::create_dir(&fixtures).expect("create fixture directory");
        std::fs::create_dir(&evidence).expect("create evidence directory");
        Self {
            stdout: root.join("runner.stdout"),
            stderr: root.join("runner.stderr"),
            root,
            fixtures,
            evidence,
        }
    }

    fn finish(self) {
        std::fs::remove_dir_all(&self.root).expect("remove successful acceptance scratch");
    }
}

#[derive(Clone, Debug)]
struct ProcFacts {
    parent: u32,
    start_ticks: u64,
    rss_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    digest: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wu0gRequestKind {
    Causal,
    Performance,
}

struct Wu0gFixture {
    root: PathBuf,
    request_path: PathBuf,
    result_dir: PathBuf,
    frozen_libtest: PathBuf,
    request_bytes: Vec<u8>,
    request_fields: BTreeMap<String, String>,
}

struct StableReplacement {
    target: PathBuf,
    replacement: PathBuf,
    opened_identity: (u64, u64),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ObservedLaunchFacts {
    pid: u32,
    start_ticks: u64,
    launch_cgroup: PathBuf,
    scope_cgroup: PathBuf,
}

struct BoundedRun {
    status: ExitStatus,
    raw_wait_status: i32,
    max_descendant_rss: u64,
    stdout_oversized: bool,
    stderr_oversized: bool,
    observed_identities: BTreeMap<u32, u64>,
    observed_launches: BTreeSet<(u32, u64)>,
    observed_launch_facts: BTreeSet<ObservedLaunchFacts>,
    observed_launch_cgroups: BTreeSet<PathBuf>,
    replacement_performed: bool,
}

struct CaptureThread {
    oversized: Arc<AtomicBool>,
    worker: JoinHandle<()>,
}

impl CaptureThread {
    fn finish(self) -> bool {
        self.worker
            .join()
            .expect("bounded capture thread completes");
        self.oversized.load(Ordering::SeqCst)
    }
}

fn assert_real_directory(path: &Path) {
    let metadata = std::fs::symlink_metadata(path).expect("inspect real directory");
    assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
}

fn create_exclusive(path: &Path, bytes: &[u8], executable: bool) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("create exclusive fixture");
    file.write_all(bytes).expect("write fixture");
    file.flush().expect("flush fixture");
    drop(file);
    if executable {
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(path, permissions).unwrap();
    }
}

fn create_append_probe(path: &Path) {
    create_exclusive(
        path,
        b"#!/usr/bin/perl\nuse strict; use warnings; use Fcntl qw(O_WRONLY O_APPEND O_NOFOLLOW); my $sink = shift @ARGV; if (defined $ENV{TYPOKAT_WU0E_REQUIRE_FILE}) { -f $ENV{TYPOKAT_WU0E_REQUIRE_FILE} or die \"required evidence missing\\n\"; } sysopen my $h, $sink, O_WRONLY | O_APPEND | O_NOFOLLOW or die $!; print {$h} join(' ', @ARGV), \"\\n\" or die $!; close $h or die $!; exit(($ENV{TYPOKAT_WU0E_CALLBACK_EXIT} // 0) + 0);\n",
        true,
    );
}

fn create_scope_abort_spy(path: &Path) {
    create_exclusive(
        path,
        b"#!/usr/bin/perl\nuse strict; use warnings; use Fcntl qw(O_RDONLY O_WRONLY O_APPEND O_NOFOLLOW); my ($sink, $meta, $unit, $control_group, @command) = @ARGV; defined $control_group && @command == 5 or die \"scope abort spy argv\n\"; $command[0] eq '/usr/bin/systemctl' && $command[1] eq '--user' && $command[2] eq '--no-block' && $command[3] eq 'stop' or die \"scope abort command prefix\n\"; $command[4] eq $unit or die \"scope abort unit mismatch\n\"; sysopen my $m, $meta, O_RDONLY | O_NOFOLLOW or die $!; local $/; my $bytes = <$m>; close $m or die $!; $bytes =~ /(?:\\A|\\n)cgroup_retained=1\\n/ or die \"retained metadata missing\n\"; sysopen my $h, $sink, O_WRONLY | O_APPEND | O_NOFOLLOW or die $!; print {$h} \"callback=1 unit=$unit control_group=$control_group argv=\", join('|', @command), \"\\n\" or die $!; close $h or die $!;\n",
        true,
    );
}

fn create_retained_exception_abort_spy(path: &Path) {
    let required = REQUIRED_PROCESS_META.join(" ");
    let source = format!(
        "#!/usr/bin/perl\nuse strict; use warnings; use Fcntl qw(O_RDONLY O_WRONLY O_APPEND O_NOFOLLOW); my ($sink, $meta, $unit, $control_group, @command) = @ARGV; defined $control_group && @command == 5 or die \"scope abort spy argv\\n\"; $command[0] eq '/usr/bin/systemctl' && $command[1] eq '--user' && $command[2] eq '--no-block' && $command[3] eq 'stop' && $command[4] eq $unit or die \"scope abort command mismatch\\n\"; sysopen my $m, $meta, O_RDONLY | O_NOFOLLOW or die $!; local $/; my $bytes = <$m>; close $m or die $!; my @lines = split /\\n/, $bytes; shift(@lines) eq 'typokat-wu0e-process-meta-v2' or die \"process meta header\\n\"; my %field; for my $line (@lines) {{ my ($key, $value) = split /=/, $line, 2; defined $value && !exists $field{{$key}} or die \"process meta field\\n\"; $field{{$key}} = $value; }} for my $key (qw({required})) {{ exists $field{{$key}} or die \"mandatory process meta missing\\n\"; }} $field{{cgroup_retained}} eq '1' && $field{{meta_fsync_completed}} eq '1' or die \"retained metadata not durable\\n\"; sysopen my $h, $sink, O_WRONLY | O_APPEND | O_NOFOLLOW or die $!; print {{$h}} \"callback=1 unit=$unit control_group=$control_group argv=\", join('|', @command), \"\\n\" or die $!; close $h or die $!;\n"
    );
    create_exclusive(path, source.as_bytes(), true);
}

fn create_production_hook_probe(path: &Path) {
    create_exclusive(
        path,
        b"#!/usr/bin/perl\nuse strict; use warnings; use Digest::SHA qw(sha256_hex); use Fcntl qw(O_RDONLY O_WRONLY O_APPEND O_NOFOLLOW); my ($sink, $seed, @fields) = @ARGV; @fields or die \"production hook fields missing\\n\"; sysopen my $s, $seed, O_RDONLY | O_NOFOLLOW or die $!; local $/; my $bytes = <$s>; close $s or die $!; sysopen my $h, $sink, O_WRONLY | O_APPEND | O_NOFOLLOW or die $!; print {$h} \"seed_sha256=\", sha256_hex($bytes), \" \", join(' ', @fields), \"\\n\" or die $!; close $h or die $!;\n",
        true,
    );
}

fn create_validator_probe(path: &Path) {
    create_exclusive(
        path,
        b"#!/usr/bin/perl\nuse strict; use warnings; my $mode = $ENV{TYPOKAT_WU0E_VALIDATE_MODE} // 'workload'; my $termination = $ENV{TYPOKAT_WU0E_VALIDATE_TERMINATION} // 'normal'; print \"trusted_validator_marker=1\\n\"; if ($mode ne 'workload') { print \"typokat-wu0e-validation-v1 mode=$mode termination=$termination status=complete semantic_sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\n\"; }\n",
        true,
    );
}

fn start_bounded_capture<R: Read + Send + 'static>(mut reader: R, path: PathBuf) -> CaptureThread {
    let oversized = Arc::new(AtomicBool::new(false));
    let worker_flag = Arc::clone(&oversized);
    let worker = std::thread::spawn(move || {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("create bounded capture artifact");
        let mut accepted = 0_u64;
        let mut buffer = [0_u8; 8 * 1_024];
        loop {
            let count = reader.read(&mut buffer).expect("read bounded child pipe");
            if count == 0 {
                break;
            }
            let count_u64 = u64::try_from(count).unwrap();
            let remaining = MAX_CAPTURE_BYTES + 1 - accepted;
            let retained = usize::try_from(remaining.min(count_u64)).unwrap();
            output
                .write_all(&buffer[..retained])
                .expect("write bounded capture artifact");
            accepted = accepted
                .checked_add(u64::try_from(retained).unwrap())
                .expect("bounded capture count");
            if accepted > MAX_CAPTURE_BYTES || count_u64 > remaining {
                worker_flag.store(true, Ordering::SeqCst);
                break;
            }
        }
        output.flush().expect("flush bounded capture artifact");
    });
    CaptureThread { oversized, worker }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_path(path: &Path) -> String {
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .expect("open digest input");
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        let count = file.read(&mut buffer).expect("read digest input");
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    format!("{:x}", digest.finalize())
}

fn exact_perf_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let mut child = Command::new("/usr/bin/perf")
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch bounded perf version probe");
        let stdout = child.stdout.take().expect("perf version stdout");
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout
                .take(4_097)
                .read_to_end(&mut bytes)
                .expect("read bounded perf version");
            bytes
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll perf version") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("perf version probe deadline expired");
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(status.success());
        let bytes = reader.join().expect("join perf version reader");
        assert!(bytes.len() <= 4_096 && bytes.ends_with(b"\n"));
        let version = std::str::from_utf8(&bytes[..bytes.len() - 1]).unwrap();
        assert!(!version.is_empty() && !version.contains(['\n', '\r']));
        version.to_owned()
    })
}

fn framed_identity(domain: &str, bytes: &[u8]) -> String {
    let mut framed = Vec::new();
    framed.extend_from_slice(&u64::try_from(domain.len()).unwrap().to_be_bytes());
    framed.extend_from_slice(domain.as_bytes());
    framed.extend_from_slice(&u64::try_from(bytes.len()).unwrap().to_be_bytes());
    framed.extend_from_slice(bytes);
    sha256_hex(&framed)
}

fn protocol_record_bytes(
    header: &str,
    schema: &[&str],
    fields: &BTreeMap<String, String>,
) -> Vec<u8> {
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        schema
    );
    let mut bytes = format!("{header}\n").into_bytes();
    for key in schema {
        let value = fields.get(*key).expect("protocol field");
        bytes.extend_from_slice(format!("{key}={}:{}\n", value.len(), value).as_bytes());
    }
    bytes
}

fn wu0g_request_fields(kind: Wu0gRequestKind, frozen_libtest: &Path) -> BTreeMap<String, String> {
    let mut fields = WU0G_REQUEST_FIELDS
        .iter()
        .map(|field| ((*field).to_owned(), "none".to_owned()))
        .collect::<BTreeMap<_, _>>();
    for (key, value) in [
        ("artifact_relative_path", "artifacts/child.bin".to_owned()),
        ("binary_identity", sha256_path(frozen_libtest)),
        ("candidate_identity", sha256_hex(b"candidate-b-v1")),
        ("cpu_affinity", "0".to_owned()),
        ("deadline_ms", "30000".to_owned()),
        ("host_identity", sha256_hex(b"fixture-host")),
        ("libtest_relative_path", "tools/frozen-libtest".to_owned()),
        ("memory_limit_bytes", "536870912".to_owned()),
        ("mode", "baseline".to_owned()),
        ("nofile_hard", "256".to_owned()),
        ("nofile_soft", "256".to_owned()),
        ("nonce", "0123456789abcdef0123456789abcdef".to_owned()),
        ("plan_identity", sha256_hex(b"fixture-plan")),
        (
            "prlimit_identity",
            sha256_path(Path::new("/usr/bin/prlimit")),
        ),
        (
            "request_relative_path",
            "requests/launch-0.request".to_owned(),
        ),
        ("result_identity", sha256_hex(b"fixture-result-0")),
        ("result_relative_path", "results/launch-0".to_owned()),
        ("rss_limit_bytes", "402653184".to_owned()),
        (
            "semantic_artifact_relative_path",
            "artifacts/semantic.bin".to_owned(),
        ),
        (
            "sentinel_relative_path",
            "artifacts/completion.sentinel".to_owned(),
        ),
        ("workload_identity", sha256_hex(b"fixture-workload")),
    ] {
        fields.insert(key.to_owned(), value);
    }
    match kind {
        Wu0gRequestKind::Causal => {
            fields.insert("kind".to_owned(), "causal".to_owned());
            fields.insert("rung_identity".to_owned(), sha256_hex(b"fixture-rung-0"));
            fields.insert("rung_ordinal".to_owned(), "0".to_owned());
        }
        Wu0gRequestKind::Performance => {
            fields.insert("kind".to_owned(), "performance".to_owned());
            fields.insert(
                "launch_identity".to_owned(),
                sha256_hex(b"fixture-launch-0"),
            );
            fields.insert("launch_ordinal".to_owned(), "0".to_owned());
            fields.insert("pair_identity".to_owned(), sha256_hex(b"fixture-pair-0"));
            fields.insert("pair_ordinal".to_owned(), "0".to_owned());
            fields.insert("perf_event".to_owned(), "instructions:u".to_owned());
            fields.insert(
                "perf_identity".to_owned(),
                sha256_path(Path::new("/usr/bin/perf")),
            );
            fields.insert("perf_version".to_owned(), exact_perf_version().to_owned());
        }
    }
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        WU0G_REQUEST_FIELDS
    );
    fields
}

fn spec_owned_wu0g_request_bytes(
    kind: Wu0gRequestKind,
    frozen_libtest: &Path,
) -> (BTreeMap<String, String>, Vec<u8>) {
    let fields = wu0g_request_fields(kind, frozen_libtest);
    let bytes = protocol_record_bytes(
        "typokat-wu0g-child-request-v1",
        WU0G_REQUEST_FIELDS,
        &fields,
    );
    assert!(u64::try_from(bytes.len()).unwrap() <= WU0G_REQUEST_CAP_BYTES);
    (fields, bytes)
}

fn replace_wu0g_request_field(bytes: &[u8], key: &str, value: &str) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).expect("request fixture UTF-8");
    let prefix = format!("{key}=");
    let mut found = false;
    let mut output = String::new();
    for line in text.lines() {
        if line.starts_with(&prefix) {
            assert!(!found);
            found = true;
            output.push_str(&format!("{key}={}:{}\n", value.len(), value));
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    assert!(found);
    output.into_bytes()
}

fn prepare_wu0g_fixture(parent: &Path, name: &str, kind: Wu0gRequestKind) -> Wu0gFixture {
    let root = parent.join(name);
    std::fs::create_dir(&root).expect("create WU0G fixture root");
    for relative in ["requests", "results", "tools"] {
        std::fs::create_dir(root.join(relative)).expect("create WU0G fixture directory");
    }
    let frozen_libtest = root.join("tools/frozen-libtest");
    let source = std::env::current_exe().expect("current libtest path");
    std::fs::copy(&source, &frozen_libtest).expect("freeze current libtest");
    let mut permissions = std::fs::metadata(&frozen_libtest).unwrap().permissions();
    permissions.set_mode(0o500);
    std::fs::set_permissions(&frozen_libtest, permissions).unwrap();
    let (request_fields, request_bytes) = spec_owned_wu0g_request_bytes(kind, &frozen_libtest);
    let request_path = root.join("requests/launch-0.request");
    create_exclusive(&request_path, &request_bytes, false);
    Wu0gFixture {
        result_dir: root.join("results/launch-0"),
        root,
        request_path,
        frozen_libtest,
        request_bytes,
        request_fields,
    }
}

fn parse_framed_record(
    path: &Path,
    cap: u64,
    expected_header: &str,
    expected_fields: &[&str],
) -> BTreeMap<String, String> {
    let bytes = read_bounded_regular(path, cap);
    assert!(bytes.is_ascii() && !bytes.contains(&b'\r'));
    let header_end = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .expect("framed record header newline");
    assert_eq!(&bytes[..header_end], expected_header.as_bytes());
    let mut cursor = header_end + 1;
    let mut fields = BTreeMap::new();
    for expected in expected_fields {
        let prefix = format!("{expected}=");
        assert_eq!(
            bytes.get(cursor..cursor + prefix.len()),
            Some(prefix.as_bytes()),
            "noncanonical field order for {expected}"
        );
        cursor += prefix.len();
        let colon = bytes[cursor..]
            .iter()
            .position(|byte| *byte == b':')
            .map(|offset| cursor + offset)
            .expect("framed value length delimiter");
        let length_text = std::str::from_utf8(&bytes[cursor..colon]).unwrap();
        assert!(!length_text.is_empty() && !length_text.starts_with('+'));
        assert!(length_text == "0" || !length_text.starts_with('0'));
        assert!(length_text.bytes().all(|byte| byte.is_ascii_digit()));
        let length = length_text.parse::<usize>().expect("framed value length");
        let value_start = colon + 1;
        let value_end = value_start.checked_add(length).expect("framed value end");
        assert_eq!(bytes.get(value_end), Some(&b'\n'));
        let value = std::str::from_utf8(
            bytes
                .get(value_start..value_end)
                .expect("complete framed value"),
        )
        .unwrap();
        assert!(!value
            .chars()
            .any(|character| matches!(character, '\n' | '\r' | '\0')));
        assert!(fields
            .insert((*expected).to_owned(), value.to_owned())
            .is_none());
        cursor = value_end + 1;
    }
    assert_eq!(cursor, bytes.len(), "trailing framed record bytes");
    fields
}

fn exact_decimal(fields: &BTreeMap<String, String>, key: &str) -> u64 {
    let value = fields.get(key).expect("numeric protocol field");
    assert!(value == "0" || !value.starts_with('0'));
    assert!(value.bytes().all(|byte| byte.is_ascii_digit()));
    value.parse().expect("bounded protocol integer")
}

fn file_identity(path: &Path) -> FileIdentity {
    let metadata = std::fs::symlink_metadata(path).expect("identity target exists");
    assert!(metadata.is_file() && !metadata.file_type().is_symlink());
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        digest: sha256_hex(&read_bounded_regular(path, 64 * 1_024)),
    }
}

fn read_bounded_regular(path: &Path, limit: u64) -> Vec<u8> {
    let before = std::fs::symlink_metadata(path).expect("bounded artifact exists");
    assert!(before.is_file() && !before.file_type().is_symlink());
    assert!(
        before.len() <= limit,
        "oversized artifact: {}",
        path.display()
    );
    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .expect("open bounded artifact");
    let opened = file.metadata().expect("inspect opened artifact");
    assert_eq!((before.dev(), before.ino()), (opened.dev(), opened.ino()));
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .expect("bounded artifact read");
    assert!(u64::try_from(bytes.len()).unwrap() <= limit);
    let after = std::fs::symlink_metadata(path).expect("artifact remains present");
    assert_eq!((opened.dev(), opened.ino()), (after.dev(), after.ino()));
    assert_eq!(opened.len(), after.len());
    bytes
}

fn parse_proc_stat(pid: u32) -> Option<(u32, u64)> {
    let bytes = std::fs::read(format!("/proc/{pid}/stat")).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let tail = text.rsplit_once(") ")?.1;
    let fields = tail.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() < 20 {
        return None;
    }
    Some((fields[1].parse().ok()?, fields[19].parse().ok()?))
}

fn proc_snapshot() -> BTreeMap<u32, ProcFacts> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir("/proc").expect("read /proc") {
        let entry = entry.expect("read /proc entry");
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let Some((parent, start_ticks)) = parse_proc_stat(pid) else {
            continue;
        };
        let Ok(statm) = std::fs::read_to_string(format!("/proc/{pid}/statm")) else {
            continue;
        };
        let Some(pages) = statm
            .split_ascii_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        let rss_bytes = pages.checked_mul(4096).expect("watchdog RSS fits u64");
        result.insert(
            pid,
            ProcFacts {
                parent,
                start_ticks,
                rss_bytes,
            },
        );
    }
    result
}

fn belongs_to_tree(pid: u32, root: u32, snapshot: &BTreeMap<u32, ProcFacts>) -> bool {
    let mut cursor = pid;
    for _ in 0..=snapshot.len() {
        if cursor == root {
            return true;
        }
        let Some(facts) = snapshot.get(&cursor) else {
            return false;
        };
        if facts.parent == cursor || facts.parent == 0 {
            return false;
        }
        cursor = facts.parent;
    }
    false
}

fn tree_members(root: u32, snapshot: &BTreeMap<u32, ProcFacts>) -> Vec<u32> {
    snapshot
        .keys()
        .copied()
        .filter(|pid| belongs_to_tree(*pid, root, snapshot))
        .collect()
}

fn tree_rss(root: u32, snapshot: &BTreeMap<u32, ProcFacts>) -> u64 {
    tree_members(root, snapshot)
        .into_iter()
        .try_fold(0_u64, |sum, pid| {
            sum.checked_add(snapshot.get(&pid)?.rss_bytes)
        })
        .expect("watchdog RSS sum is complete and bounded")
}

fn kill_pid_argument(argument: String) {
    let Ok(mut killer) = Command::new("/bin/kill")
        .args(["-KILL", "--", &argument])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    let deadline = Instant::now() + Duration::from_millis(100);
    loop {
        if killer.try_wait().ok().flatten().is_some() {
            return;
        }
        if Instant::now() >= deadline {
            let _ = killer.kill();
            let reap_deadline = Instant::now() + Duration::from_millis(100);
            while killer.try_wait().ok().flatten().is_none() {
                if Instant::now() >= reap_deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn terminate_outer_tree(child: &mut Child, root: u32) {
    let deadline = Instant::now() + OUTER_DRAIN;
    loop {
        let snapshot = proc_snapshot();
        let members = tree_members(root, &snapshot);
        kill_pid_argument(format!("-{root}"));
        for pid in &members {
            kill_pid_argument(pid.to_string());
        }
        let _ = child.kill();
        if child.try_wait().ok().flatten().is_some()
            && members
                .into_iter()
                .all(|pid| parse_proc_stat(pid).is_none())
        {
            return;
        }
        assert!(Instant::now() < deadline, "outer watchdog drain expired");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn live_known_identities(known: &BTreeMap<u32, u64>) -> Vec<u32> {
    known
        .iter()
        .filter_map(|(pid, expected_start)| {
            parse_proc_stat(*pid)
                .is_some_and(|(_, actual_start)| actual_start == *expected_start)
                .then_some(*pid)
        })
        .collect()
}

fn known_identity_rss(known: &BTreeMap<u32, u64>, snapshot: &BTreeMap<u32, ProcFacts>) -> u64 {
    known
        .iter()
        .filter_map(|(pid, expected_start)| {
            let facts = snapshot.get(pid)?;
            (facts.start_ticks == *expected_start).then_some(facts.rss_bytes)
        })
        .try_fold(0_u64, u64::checked_add)
        .expect("final watchdog RSS sum fits u64")
}

fn proc_executable_identity(pid: u32) -> Option<(u64, u64)> {
    let metadata = std::fs::metadata(format!("/proc/{pid}/exe")).ok()?;
    Some((metadata.dev(), metadata.ino()))
}

fn proc_cgroup_v2_path(pid: u32) -> Option<PathBuf> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    let relative = text
        .lines()
        .find_map(|line| line.strip_prefix("0::"))?
        .trim_start_matches('/');
    Some(Path::new("/sys/fs/cgroup").join(relative))
}

fn observed_scope_cgroup(launch_cgroup: &Path) -> Option<PathBuf> {
    launch_cgroup.ancestors().find_map(|ancestor| {
        ancestor
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".scope"))
            .then(|| ancestor.to_owned())
    })
}

fn canonical_observed_cgroup_path(path: &Path) -> String {
    let relative = path
        .strip_prefix("/sys/fs/cgroup")
        .expect("observed cgroup is below the cgroup-v2 mount");
    format!("/{}", relative.display())
}

fn observed_cgroup_identity(domain: &str, path: &Path) -> String {
    framed_identity(domain, canonical_observed_cgroup_path(path).as_bytes())
}

fn observed_child_identity(facts: &ObservedLaunchFacts) -> String {
    let canonical = format!(
        "typokat-wu0g-observed-child-v1\nleader_pid={}\nleader_start_ticks={}\nlaunch_cgroup={}\nscope_cgroup={}\n",
        facts.pid,
        facts.start_ticks,
        canonical_observed_cgroup_path(&facts.launch_cgroup),
        canonical_observed_cgroup_path(&facts.scope_cgroup),
    );
    framed_identity("wu0g-child-v1", canonical.as_bytes())
}

fn process_has_open_identity(pid: u32, identity: (u64, u64)) -> bool {
    let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        entry
            .metadata()
            .is_ok_and(|metadata| (metadata.dev(), metadata.ino()) == identity)
    })
}

fn run_bounded_command(
    mut command: Command,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    launch_executable: Option<(u64, u64)>,
    replacement: Option<&StableReplacement>,
    artifact_root: &Path,
) -> BoundedRun {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch bounded command");
    let stdout_capture =
        start_bounded_capture(child.stdout.take().expect("take child stdout"), stdout_path);
    let stderr_capture =
        start_bounded_capture(child.stderr.take().expect("take child stderr"), stderr_path);
    let root = child.id();
    let started = Instant::now();
    let mut max_descendant_rss = 0;
    let mut known_identities = BTreeMap::new();
    let mut observed_launches = BTreeSet::new();
    let mut observed_launch_facts = BTreeSet::new();
    let mut observed_launch_cgroups = BTreeSet::new();
    let mut replacement_performed = false;
    loop {
        if let Some(status) = child.try_wait().expect("poll bounded command") {
            let drain_deadline = Instant::now() + OUTER_DRAIN;
            loop {
                let final_snapshot = proc_snapshot();
                max_descendant_rss =
                    max_descendant_rss.max(known_identity_rss(&known_identities, &final_snapshot));
                let live = live_known_identities(&known_identities);
                if live.is_empty() {
                    break;
                }
                if Instant::now() >= drain_deadline {
                    for pid in live {
                        kill_pid_argument(pid.to_string());
                    }
                    panic!(
                        "outer watchdog observed surviving descendants; artifacts: {}",
                        artifact_root.display()
                    );
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let stdout_oversized = stdout_capture.finish();
            let stderr_oversized = stderr_capture.finish();
            return BoundedRun {
                raw_wait_status: status.into_raw(),
                status,
                max_descendant_rss,
                stdout_oversized,
                stderr_oversized,
                observed_identities: known_identities,
                observed_launches,
                observed_launch_facts,
                observed_launch_cgroups,
                replacement_performed,
            };
        }
        let snapshot = proc_snapshot();
        for pid in tree_members(root, &snapshot) {
            let start_ticks = snapshot.get(&pid).unwrap().start_ticks;
            known_identities.insert(pid, start_ticks);
            if launch_executable.is_some_and(|expected| {
                proc_executable_identity(pid).is_some_and(|actual| actual == expected)
            }) {
                observed_launches.insert((pid, start_ticks));
                if let Some(cgroup) = proc_cgroup_v2_path(pid) {
                    observed_launch_cgroups.insert(cgroup.clone());
                    if let Some(scope_cgroup) = observed_scope_cgroup(&cgroup) {
                        observed_launch_facts.insert(ObservedLaunchFacts {
                            pid,
                            start_ticks,
                            launch_cgroup: cgroup,
                            scope_cgroup,
                        });
                    }
                }
            }
            if !replacement_performed
                && replacement
                    .is_some_and(|attack| process_has_open_identity(pid, attack.opened_identity))
            {
                let attack = replacement.expect("replacement attack exists");
                std::fs::rename(&attack.replacement, &attack.target)
                    .expect("replace executable after stable open");
                replacement_performed = true;
            }
        }
        let rss = tree_rss(root, &snapshot);
        max_descendant_rss = max_descendant_rss.max(rss);
        let failure = if started.elapsed() >= OUTER_DEADLINE {
            Some("deadline")
        } else if stdout_capture.oversized.load(Ordering::SeqCst) {
            Some("stdout")
        } else if stderr_capture.oversized.load(Ordering::SeqCst) {
            Some("stderr")
        } else if rss > MAX_OUTER_RSS_BYTES {
            Some("rss")
        } else {
            None
        };
        if let Some(reason) = failure {
            terminate_outer_tree(&mut child, root);
            let _ = stdout_capture.finish();
            let _ = stderr_capture.finish();
            panic!(
                "outer watchdog hit {reason}; artifacts: {}",
                artifact_root.display()
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn runner_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tooling/wu0e-diagnostic/run.pl")
}

fn direct_wu0g_command(fixture: &Wu0gFixture) -> Command {
    let mut command = Command::new("/usr/bin/perl");
    command
        .arg(runner_path())
        .arg("--wu0g-child-v1")
        .arg(&fixture.request_path)
        .arg(&fixture.result_dir);
    for (key, value) in WU0G_FORBIDDEN_ENV {
        command.env(key, value);
    }
    command
}

fn run_direct_wu0g_with(
    fixture: &Wu0gFixture,
    label: &str,
    injected_tool: Option<(&str, &Path)>,
    replacement: Option<&StableReplacement>,
) -> BoundedRun {
    let canary_path = fixture.root.join(format!("{label}.inherited-fd-canary"));
    create_exclusive(&canary_path, b"must-not-reach-wu0g-child\n", false);
    let canary = OpenOptions::new()
        .read(true)
        .open(&canary_path)
        .expect("open inherited FD canary");
    let flags = rustix::io::fcntl_getfd(&canary).expect("read FD flags");
    rustix::io::fcntl_setfd(&canary, flags & !rustix::io::FdFlags::CLOEXEC)
        .expect("make FD canary inheritable");
    assert_eq!(
        rustix::io::fcntl_getfd(&canary).unwrap() & rustix::io::FdFlags::CLOEXEC,
        rustix::io::FdFlags::empty()
    );
    let frozen = std::fs::metadata(&fixture.frozen_libtest).expect("frozen libtest identity");
    let stdout = fixture.root.join(format!("{label}.runner.stdout"));
    let stderr = fixture.root.join(format!("{label}.runner.stderr"));
    let mut command = direct_wu0g_command(fixture);
    if let Some((key, path)) = injected_tool {
        command.env(key, path);
    }
    let bounded = run_bounded_command(
        command,
        stdout,
        stderr,
        Some((frozen.dev(), frozen.ino())),
        replacement,
        &fixture.root,
    );
    drop(canary);
    bounded
}

fn run_direct_wu0g(fixture: &Wu0gFixture, label: &str) -> BoundedRun {
    run_direct_wu0g_with(fixture, label, None, None)
}

fn run_bounded_self_test(scratch: &AcceptanceScratch, nonce: &str) -> BoundedRun {
    let mut command = Command::new("/usr/bin/setsid");
    command
        .arg("/usr/bin/perl")
        .arg(runner_path())
        .arg("--self-test-evidence")
        .arg(&scratch.evidence)
        .arg(&scratch.fixtures)
        .arg(nonce);
    run_bounded_command(
        command,
        scratch.stdout.clone(),
        scratch.stderr.clone(),
        None,
        None,
        &scratch.root,
    )
}

fn parse_record(path: &Path, prefix: &str) -> BTreeMap<String, String> {
    let bytes = read_bounded_regular(path, 64 * 1_024);
    assert!(bytes.is_ascii() && bytes.ends_with(b"\n") && !bytes.contains(&b'\r'));
    let text = std::str::from_utf8(&bytes).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(prefix));
    let mut fields = BTreeMap::new();
    for line in lines {
        let (key, value) = line.split_once('=').expect("canonical key=value record");
        assert!(!key.is_empty() && !value.is_empty());
        assert!(fields.insert(key.to_owned(), value.to_owned()).is_none());
    }
    fields
}

fn numeric(fields: &BTreeMap<String, String>, key: &str) -> u64 {
    let value = fields.get(key).unwrap();
    assert!(!value.starts_with('+'));
    assert!(value == "0" || !value.starts_with('0'));
    value.parse().unwrap()
}

fn exact_dossier() -> String {
    format!(
        "typokat-wu0e-diagnostic-dossier-v2\n\
binary_identity={BINARY_SHA}\n\
host_identity={HOST_SHA}\n\
profile_identity={PROFILE_SHA}\n\
inventory_identity={INVENTORY_SHA}\n\
mode_order=plain,measured-off,candidate-b\n\
workload mode=plain termination=normal semantic_sha256={A_SHA} scope_unit=fixture.scope scope_control_group=/fixture.scope launch_cgroup=/fixture.scope/plain memory_max=1073741824 memory_swap_max=0 memory_oom_group=1 rss_peak=4096 memory_current=4096 memory_peak=8192 events_max_baseline=0 events_max_final=0 events_max_delta=0 events_oom_baseline=0 events_oom_final=0 events_oom_delta=0 events_oom_kill_baseline=0 events_oom_kill_final=0 events_oom_kill_delta=0 events_oom_group_kill_baseline=0 events_oom_group_kill_final=0 events_oom_group_kill_delta=0 memory_source=none readiness=1 membership=1 setsid=1 direct_kill_attempted=0 pgid_kill_attempted=0 cgroup_kill_attempted=0 cleanup_populated_zero=1 cleanup_pgid_empty=1 leader_reaped=1 cgroup_removed=1 cgroup_retained=0 cleanup=removed\n\
workload mode=measured-off termination=normal semantic_sha256={A_SHA} scope_unit=fixture.scope scope_control_group=/fixture.scope launch_cgroup=/fixture.scope/measured-off memory_max=1073741824 memory_swap_max=0 memory_oom_group=1 rss_peak=4096 memory_current=4096 memory_peak=8192 events_max_baseline=0 events_max_final=0 events_max_delta=0 events_oom_baseline=0 events_oom_final=0 events_oom_delta=0 events_oom_kill_baseline=0 events_oom_kill_final=0 events_oom_kill_delta=0 events_oom_group_kill_baseline=0 events_oom_group_kill_final=0 events_oom_group_kill_delta=0 memory_source=none readiness=1 membership=1 setsid=1 direct_kill_attempted=0 pgid_kill_attempted=0 cgroup_kill_attempted=0 cleanup_populated_zero=1 cleanup_pgid_empty=1 leader_reaped=1 cgroup_removed=1 cgroup_retained=0 cleanup=removed\n\
workload mode=candidate-b termination=deadline semantic_sha256=unavailable scope_unit=fixture.scope scope_control_group=/fixture.scope launch_cgroup=/fixture.scope/candidate-b memory_max=1073741824 memory_swap_max=0 memory_oom_group=1 rss_peak=4096 memory_current=4096 memory_peak=8192 events_max_baseline=0 events_max_final=0 events_max_delta=0 events_oom_baseline=0 events_oom_final=0 events_oom_delta=0 events_oom_kill_baseline=0 events_oom_kill_final=0 events_oom_kill_delta=0 events_oom_group_kill_baseline=0 events_oom_group_kill_final=0 events_oom_group_kill_delta=0 memory_source=none readiness=1 membership=1 setsid=1 direct_kill_attempted=1 pgid_kill_attempted=1 cgroup_kill_attempted=1 cleanup_populated_zero=1 cleanup_pgid_empty=1 leader_reaped=1 cgroup_removed=1 cgroup_retained=0 cleanup=removed\n"
    )
}

fn exact_termination_cases() -> &'static str {
    "typokat-wu0e-termination-fixtures-v1\n\
case=all-loop flags=infrastructure,trace,stdout,stderr,rss,deadline,crash post=none actual=infrastructure\n\
case=post-infrastructure flags=trace,stdout,stderr,rss,deadline,crash post=infrastructure actual=infrastructure\n\
case=post-trace flags=stdout,stderr,rss,deadline,crash post=trace actual=trace\n\
case=post-stdout flags=stderr,rss,deadline,crash post=stdout actual=stdout\n\
case=post-stderr flags=rss,deadline,crash post=stderr actual=stderr\n\
case=post-rss flags=deadline,crash post=rss actual=rss\n\
case=deadline flags=deadline,crash post=none actual=deadline\n\
case=crash flags=crash post=none actual=crash\n\
case=normal flags=none post=none actual=normal\n\
case=delayed-rss-sample flags=none post=none sample_interval_us=15000 target_us=10000 actual=normal\n\
case=max-contact flags=memory-max-contact post=none actual=normal memory_source=max\n"
}

fn exact_preflight_failures() -> &'static str {
    "typokat-wu0e-preflight-failure-fixtures-v1\n\
case=cgroup-unavailable actual=infrastructure workload_exec=0 validator_exec=0\n\
case=delegate-false actual=infrastructure workload_exec=0 validator_exec=0\n\
case=memory-controller-missing actual=infrastructure workload_exec=0 validator_exec=0\n\
case=cgroup-type-missing actual=infrastructure workload_exec=0 validator_exec=0\n\
case=cgroup-type-threaded actual=infrastructure workload_exec=0 validator_exec=0\n\
case=cgroup-procs-inaccessible actual=infrastructure workload_exec=0 validator_exec=0\n\
case=cgroup-events-malformed actual=infrastructure workload_exec=0 validator_exec=0\n\
case=cgroup-kill-unwritable actual=infrastructure workload_exec=0 validator_exec=0\n\
case=memory-max-readback-mismatch actual=infrastructure workload_exec=0 validator_exec=0\n\
case=memory-swap-max-readback-mismatch actual=infrastructure workload_exec=0 validator_exec=0\n\
case=memory-oom-group-readback-mismatch actual=infrastructure workload_exec=0 validator_exec=0\n\
case=memory-current-missing actual=infrastructure workload_exec=0 validator_exec=0\n\
case=memory-peak-malformed actual=infrastructure workload_exec=0 validator_exec=0\n\
case=memory-events-local-unreadable actual=infrastructure workload_exec=0 validator_exec=0\n"
}

fn exact_rss_churn_cases() -> &'static str {
    "typokat-wu0e-rss-churn-fixtures-v1 retry_attempts=3 retry_deadline_us=10000\n\
case=vanished-member attempt=1 result=retry attempt=2 result=complete members=2\n\
case=stable-unreadable-member attempt=1 result=retry attempt=2 result=retry attempt=3 result=infrastructure\n\
case=unresolved-membership-churn attempt=1 result=retry attempt=2 result=retry attempt=3 result=infrastructure\n\
case=lowercase-t result=live\n\
case=lowercase-x result=dead\n"
}

fn exact_schedule_complete() -> String {
    format!(
        "typokat-wu0e-schedule-journal-v1\n\
build count=1 binary={BINARY_SHA}\n\
callback seq=0 kind=workload mode=plain argv=/fixture/frozen-libtest|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_primary_probe_once|--nocapture env=TYPOKAT_WU0E_MODE=plain|TYPOKAT_WU0E_TRACE_PATH=/fixture/plain.trace identities={BINARY_SHA}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} result=normal\n\
callback seq=1 kind=validator mode=plain argv=/fixture/frozen-libtest|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_validate_trace_once|--nocapture env=TYPOKAT_WU0E_VALIDATE_MODE=plain|TYPOKAT_WU0E_VALIDATE_TERMINATION=normal|TYPOKAT_WU0E_VALIDATE_TRACE_PATH=/fixture/plain.trace identities={BINARY_SHA}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} result=normal\n\
callback seq=2 kind=workload mode=measured-off argv=/fixture/frozen-libtest|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_primary_probe_once|--nocapture env=TYPOKAT_WU0E_MODE=measured-off|TYPOKAT_WU0E_TRACE_PATH=/fixture/measured-off.trace identities={BINARY_SHA}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} result=normal\n\
callback seq=3 kind=validator mode=measured-off argv=/fixture/frozen-libtest|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_validate_trace_once|--nocapture env=TYPOKAT_WU0E_VALIDATE_MODE=measured-off|TYPOKAT_WU0E_VALIDATE_TERMINATION=normal|TYPOKAT_WU0E_VALIDATE_TRACE_PATH=/fixture/measured-off.trace identities={BINARY_SHA}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} result=normal\n\
callback seq=4 kind=workload mode=candidate-b argv=/fixture/frozen-libtest|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_primary_probe_once|--nocapture env=TYPOKAT_WU0E_MODE=candidate-b|TYPOKAT_WU0E_TRACE_PATH=/fixture/candidate-b.trace identities={BINARY_SHA}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} result=normal\n\
callback seq=5 kind=validator mode=candidate-b argv=/fixture/frozen-libtest|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_validate_trace_once|--nocapture env=TYPOKAT_WU0E_VALIDATE_MODE=candidate-b|TYPOKAT_WU0E_VALIDATE_TERMINATION=normal|TYPOKAT_WU0E_VALIDATE_TRACE_PATH=/fixture/candidate-b.trace identities={BINARY_SHA}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} result=normal\n"
    )
}

fn exact_schedule_stop() -> String {
    format!(
        "typokat-wu0e-schedule-journal-v1\n\
build count=1 binary={BINARY_SHA}\n\
callback seq=0 kind=workload mode=plain argv=/fixture/frozen-libtest|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_primary_probe_once|--nocapture env=TYPOKAT_WU0E_MODE=plain|TYPOKAT_WU0E_TRACE_PATH=/fixture/plain.trace identities={BINARY_SHA}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} result=normal\n\
callback seq=1 kind=validator mode=plain argv=/fixture/frozen-libtest|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_validate_trace_once|--nocapture env=TYPOKAT_WU0E_VALIDATE_MODE=plain|TYPOKAT_WU0E_VALIDATE_TERMINATION=normal|TYPOKAT_WU0E_VALIDATE_TRACE_PATH=/fixture/plain.trace identities={BINARY_SHA}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} result=normal\n\
callback seq=2 kind=workload mode=measured-off argv=/fixture/frozen-libtest|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_primary_probe_once|--nocapture env=TYPOKAT_WU0E_MODE=measured-off|TYPOKAT_WU0E_TRACE_PATH=/fixture/measured-off.trace identities={BINARY_SHA}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} result=infrastructure\n\
stop after_seq=2 reason=infrastructure validator_launched=0\n"
    )
}

fn exact_failure_order(meta_path: &Path) -> String {
    let stderr_sha = sha256_hex(b"nested-failure-stderr\n");
    format!(
        "typokat-wu0e-failure-order-v1\n\
seq=0 event=nested-probe-start\n\
seq=1 event=nested-probe-exit status=73\n\
seq=2 event=stderr-captured sha256={stderr_sha}\n\
seq=3 event=process-meta-created path={}\n\
seq=4 event=process-meta-fsynced path={}\n\
seq=5 event=failure-stderr-published sha256={stderr_sha}\n\
seq=6 event=failure-status-published status=73\n",
        meta_path.display(),
        meta_path.display(),
    )
}

fn exact_scope_abort_spy(unit: &str, control_group: &str) -> String {
    format!(
        "callback=1 unit={unit} control_group={control_group} argv=/usr/bin/systemctl|--user|--no-block|stop|{unit}\n"
    )
}

fn exact_retained_exception_order(meta_path: &Path, unit: &str, control_group: &str) -> String {
    format!(
        "typokat-wu0e-retained-exception-order-v1\n\
seq=0 event=outer-exception phase=post-fork error=synthetic-retained-lifecycle-exception\n\
seq=1 event=process-meta-fsynced path={} mandatory_fields=complete cgroup_retained=1\n\
seq=2 event=scope-identity-reverified unit={unit} control_group={control_group} delegate=yes\n\
seq=3 event=scope-abort-requested argv=/usr/bin/systemctl|--user|--no-block|stop|{unit}\n\
seq=4 event=outer-exception-propagated error=synthetic-retained-lifecycle-exception\n",
        meta_path.display()
    )
}

fn exact_production_hook_routing(
    binary: &Path,
    trace_root: &Path,
    seed_sha: &str,
    binary_sha: &str,
) -> String {
    let mut lines = Vec::new();
    let mut seq = 0_u8;
    for mode in ["plain", "measured-off", "candidate-b"] {
        let trace = trace_root.join(format!("production-hook-{mode}.trace"));
        let workload_argv = format!(
            "{}|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_primary_probe_once|--nocapture",
            binary.display()
        );
        let workload_env = format!(
            "TYPOKAT_WU0E_MODE={mode}|TYPOKAT_WU0E_TRACE_PATH={}",
            trace.display()
        );
        lines.push(format!(
            "seed_sha256={seed_sha} seq={seq} hook=production-launch kind=workload mode={mode} argv={workload_argv} env={workload_env} identities={binary_sha}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} preflight=admitted launch=confirmed"
        ));
        seq += 1;

        let validator_argv = format!(
            "{}|--ignored|--exact|check::checker::wu0e_diagnostic::wu0e_validate_trace_once|--nocapture",
            binary.display()
        );
        let validator_env = format!(
            "TYPOKAT_WU0E_VALIDATE_MODE={mode}|TYPOKAT_WU0E_VALIDATE_TERMINATION=normal|TYPOKAT_WU0E_VALIDATE_TRACE_PATH={}",
            trace.display()
        );
        lines.push(format!(
            "seed_sha256={seed_sha} seq={seq} hook=production-launch kind=validator mode={mode} argv={validator_argv} env={validator_env} identities={binary_sha}|{HOST_SHA}|{PROFILE_SHA}|{INVENTORY_SHA} preflight=admitted launch=confirmed"
        ));
        seq += 1;
    }
    lines.join("\n") + "\n"
}

fn exact_preflight_action_trace() -> &'static str {
    "typokat-wu0e-preflight-action-trace-v1\n\
actor=child seq=0 action=self-move\n\
actor=child seq=1 action=setsid\n\
actor=child seq=2 action=readiness\n\
actor=child seq=3 action=environment\n\
actor=child seq=4 action=stable-exec\n\
actor=parent seq=0 action=configure-and-readback outcome=admitted\n\
actor=parent seq=1 action=readiness-observed\n\
actor=parent seq=2 action=membership-verify\n\
actor=parent seq=3 action=pgid-verify\n\
actor=parent seq=4 action=completion\n"
}

fn exact_candidate_b_validator_path_drift() -> &'static str {
    "typokat-wu0e-candidate-b-validator-path-v1\n\
seq=0 event=scheduler-dispatch kind=validator mode=candidate-b final_validator=1\n\
seq=1 event=launch-confirmed membership=1 setsid=1\n\
seq=2 event=trusted-handle-completed exit_code=0\n\
seq=3 event=pathname-replaced phase=post-completion\n\
seq=4 event=path-revalidation outcome=rejected error=frozen-executable-pathname-identity-drifted\n"
}

const REQUIRED_PROCESS_META: &[&str] = &[
    "kind",
    "mode",
    "termination",
    "scope_unit",
    "scope_control_group",
    "launch_cgroup",
    "cgroup_type",
    "memory_max",
    "memory_swap_max",
    "memory_oom_group",
    "rss_peak",
    "memory_current",
    "memory_peak",
    "events_max_baseline",
    "events_max_final",
    "events_max_delta",
    "events_oom_baseline",
    "events_oom_final",
    "events_oom_delta",
    "events_oom_kill_baseline",
    "events_oom_kill_final",
    "events_oom_kill_delta",
    "events_oom_group_kill_baseline",
    "events_oom_group_kill_final",
    "events_oom_group_kill_delta",
    "memory_source",
    "readiness_seen",
    "membership_verified",
    "setsid_verified",
    "direct_kill_attempted",
    "pgid_kill_attempted",
    "cgroup_kill_attempted",
    "emergency_attempts",
    "cleanup_populated_zero",
    "cleanup_pgid_empty",
    "leader_reaped",
    "cgroup_removed",
    "cgroup_retained",
    "validator_launched",
    "infrastructure_error",
];

fn assert_process_metadata(path: &Path, expected_memory_max: u64) -> BTreeMap<String, String> {
    let fields = parse_record(path, "typokat-wu0e-process-meta-v2");
    for key in REQUIRED_PROCESS_META {
        assert!(
            fields.contains_key(*key),
            "missing {key}: {}",
            path.display()
        );
    }
    assert_eq!(numeric(&fields, "memory_max"), expected_memory_max);
    assert_eq!(numeric(&fields, "memory_swap_max"), 0);
    assert_eq!(numeric(&fields, "memory_oom_group"), 1);
    for family in ["max", "oom", "oom_kill", "oom_group_kill"] {
        let baseline = numeric(&fields, &format!("events_{family}_baseline"));
        let final_value = numeric(&fields, &format!("events_{family}_final"));
        let delta = numeric(&fields, &format!("events_{family}_delta"));
        assert_eq!(baseline.checked_add(delta), Some(final_value));
    }
    fields
}

fn assert_pid_identity_gone(pid: u32, start_ticks: u64) {
    if let Some((_, current_start_ticks)) = parse_proc_stat(pid) {
        assert_ne!(
            current_start_ticks, start_ticks,
            "fixture child still exists"
        );
    }
}

fn assert_all_observed_identities_gone(bounded: &BoundedRun) {
    for (pid, start_ticks) in &bounded.observed_identities {
        assert_pid_identity_gone(*pid, *start_ticks);
    }
}

fn assert_observed_cgroups_eventually_absent(paths: &BTreeSet<PathBuf>) {
    assert!(
        !paths.is_empty(),
        "no launch cgroup was independently observed"
    );
    let deadline = Instant::now() + OUTER_DRAIN;
    loop {
        let remaining = paths
            .iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        if remaining.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "launch cgroup survived: {remaining:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn assert_no_forbidden_child_data(bytes: &[u8], canary_path: &Path) {
    for (key, value) in WU0G_FORBIDDEN_ENV {
        assert!(!bytes
            .windows(key.len())
            .any(|window| window == key.as_bytes()));
        assert!(!bytes
            .windows(value.len())
            .any(|window| window == value.as_bytes()));
    }
    let canary = canary_path.to_string_lossy();
    assert!(!bytes
        .windows(canary.len())
        .any(|window| window == canary.as_bytes()));
}

fn exact_child_fd_inventory(kind: Wu0gRequestKind) -> &'static str {
    match kind {
        Wu0gRequestKind::Causal => "stderr|stdin|stdout|request|result|libtest|prlimit",
        Wu0gRequestKind::Performance => {
            "stderr|stdin|stdout|request|result|libtest|prlimit|perf|perf-log"
        }
    }
}

fn parse_perf_row(bytes: &[u8]) -> (u64, u64) {
    assert!(u64::try_from(bytes.len()).unwrap() <= WU0G_PERF_ARTIFACT_CAP_BYTES);
    assert!(bytes.ends_with(b"\n") && !bytes[..bytes.len() - 1].contains(&b'\n'));
    assert_eq!(bytes.iter().filter(|byte| **byte == b';').count(), 6);
    let line = std::str::from_utf8(&bytes[..bytes.len() - 1]).unwrap();
    let fields = line.split(';').collect::<Vec<_>>();
    assert_eq!(fields.len(), 7);
    assert_eq!(
        &fields[1..],
        ["", "instructions:u", fields[3], "100.00", "", ""]
    );
    let instructions = fields[0]
        .parse::<u64>()
        .expect("canonical instruction count");
    let runtime = fields[3].parse::<u64>().expect("canonical perf runtime");
    assert!(instructions > 0 && runtime > 0);
    (instructions, runtime)
}

fn assert_valid_wu0g_run(fixture: &Wu0gFixture, kind: Wu0gRequestKind, bounded: &BoundedRun) {
    assert!(bounded.status.success());
    assert_eq!(bounded.raw_wait_status, 0);
    assert!(!bounded.stdout_oversized && !bounded.stderr_oversized);
    assert!(bounded.max_descendant_rss <= MAX_OUTER_RSS_BYTES);
    assert_eq!(
        bounded.observed_launches.len(),
        1,
        "exactly one libtest launch"
    );
    assert_all_observed_identities_gone(bounded);
    assert_observed_cgroups_eventually_absent(&bounded.observed_launch_cgroups);

    let outer_stdout = read_bounded_regular(
        &fixture.root.join("valid.runner.stdout"),
        WU0G_STDIO_CAP_BYTES,
    );
    let outer_stderr = read_bounded_regular(
        &fixture.root.join("valid.runner.stderr"),
        WU0G_STDIO_CAP_BYTES,
    );
    let canary_path = fixture.root.join("valid.inherited-fd-canary");
    assert_no_forbidden_child_data(&outer_stdout, &canary_path);
    assert_no_forbidden_child_data(&outer_stderr, &canary_path);

    let result_path = fixture.result_dir.join("result.v1");
    let result = parse_framed_record(
        &result_path,
        WU0G_RESULT_CAP_BYTES,
        "typokat-wu0g-child-result-v1",
        WU0G_RESULT_FIELDS,
    );
    assert_eq!(
        bounded.observed_launch_facts.len(),
        1,
        "watchdog must capture exact launch and scope facts"
    );
    let observed = bounded.observed_launch_facts.iter().next().unwrap();
    assert_eq!(
        bounded.observed_launches,
        [(observed.pid, observed.start_ticks)].into_iter().collect()
    );
    assert_ne!(observed.launch_cgroup, observed.scope_cgroup);
    assert!(observed.launch_cgroup.starts_with(&observed.scope_cgroup));
    assert_eq!(
        exact_decimal(&result, "leader_pid"),
        u64::from(observed.pid)
    );
    assert_eq!(
        exact_decimal(&result, "leader_start_ticks"),
        observed.start_ticks
    );
    assert_eq!(
        result.get("child_identity").unwrap(),
        &observed_child_identity(observed)
    );
    assert_eq!(
        result.get("cgroup_identity").unwrap(),
        &observed_cgroup_identity("wu0g-cgroup-v1", &observed.launch_cgroup)
    );
    assert_eq!(
        result.get("scope_identity").unwrap(),
        &observed_cgroup_identity("wu0g-scope-v1", &observed.scope_cgroup)
    );
    for key in [
        "binary_identity",
        "host_identity",
        "launch_identity",
        "perf_event",
        "perf_identity",
        "perf_version",
        "plan_identity",
        "prlimit_identity",
        "result_identity",
    ] {
        assert_eq!(result.get(key), fixture.request_fields.get(key));
    }
    assert_eq!(
        result.get("request_content_identity").unwrap(),
        &framed_identity("wu0g-request-content-v1", &fixture.request_bytes)
    );
    for key in [
        "deadline_ms",
        "memory_limit_bytes",
        "rss_limit_bytes",
        "nofile_soft",
        "nofile_hard",
    ] {
        assert_eq!(result.get(key), fixture.request_fields.get(key));
    }
    for (configured, readback) in [
        ("deadline_ms", "deadline_readback_ms"),
        ("memory_limit_bytes", "memory_limit_readback_bytes"),
        ("rss_limit_bytes", "rss_limit_readback_bytes"),
        ("nofile_soft", "nofile_soft_readback"),
        ("nofile_hard", "nofile_hard_readback"),
    ] {
        assert_eq!(result.get(configured), result.get(readback));
    }
    for key in [
        "cgroup_populated_zero",
        "cgroup_removed",
        "cleanup_succeeded",
        "drain_complete",
        "leader_reaped",
        "membership_verified",
        "pgid_empty",
        "readiness_seen",
    ] {
        assert_eq!(result.get(key).map(String::as_str), Some("1"));
    }
    for key in [
        "cgroup_retained",
        "containment_failures",
        "oom_delta",
        "oom_kill_delta",
        "scope_abort_observed",
        "scope_abort_requested",
    ] {
        assert_eq!(result.get(key).map(String::as_str), Some("0"));
    }
    assert_eq!(
        result.get("termination").map(String::as_str),
        Some("normal")
    );
    assert_eq!(result.get("exit_code").map(String::as_str), Some("0"));
    assert_eq!(result.get("term_signal").map(String::as_str), Some("none"));
    assert_eq!(
        result.get("outer_raw_wait_status").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        exact_decimal(&result, "leader_pid"),
        exact_decimal(&result, "pgid")
    );
    assert!(exact_decimal(&result, "max_rss_bytes") <= exact_decimal(&result, "rss_limit_bytes"));

    let sentinel_path = fixture.result_dir.join("artifacts/completion.sentinel");
    let sentinel = parse_framed_record(
        &sentinel_path,
        WU0G_SENTINEL_CAP_BYTES,
        "typokat-wu0g-child-completion-sentinel-v1",
        WU0G_SENTINEL_FIELDS,
    );
    let exact_env = WU0G_ENV_ALLOWLIST.join("|");
    assert_eq!(
        sentinel.get("argv").map(String::as_str),
        Some(WU0G_CHILD_ARGV)
    );
    assert!(sentinel
        .get("argv")
        .unwrap()
        .split('|')
        .any(|argument| argument == WU0G_CHILD_FILTER));
    assert_eq!(sentinel.get("environment"), Some(&exact_env));
    assert_eq!(
        sentinel.get("fd_inventory").map(String::as_str),
        Some(exact_child_fd_inventory(kind))
    );
    assert_eq!(
        sentinel.get("nofile_soft"),
        fixture.request_fields.get("nofile_soft")
    );
    assert_eq!(
        sentinel.get("nofile_hard"),
        fixture.request_fields.get("nofile_hard")
    );
    assert_eq!(
        sentinel.get("request_content_identity"),
        result.get("request_content_identity")
    );
    let semantic_path = fixture.result_dir.join("artifacts/semantic.bin");
    let semantic = read_bounded_regular(&semantic_path, WU0G_ARTIFACT_CAP_BYTES);
    assert_eq!(
        sentinel.get("semantic_artifact_identity").unwrap(),
        &framed_identity("wu0g-semantic-artifact-v1", &semantic)
    );
    assert_eq!(
        exact_decimal(&sentinel, "semantic_artifact_size"),
        u64::try_from(semantic.len()).unwrap()
    );
    let sentinel_bytes = read_bounded_regular(&sentinel_path, WU0G_SENTINEL_CAP_BYTES);
    assert_eq!(
        result.get("sentinel_identity").unwrap(),
        &framed_identity("wu0g-child-completion-sentinel-v1", &sentinel_bytes)
    );
    assert_eq!(
        exact_decimal(&result, "sentinel_size"),
        u64::try_from(sentinel_bytes.len()).unwrap()
    );
    assert_eq!(result.get("child_argv"), sentinel.get("argv"));
    assert_eq!(result.get("child_env"), sentinel.get("environment"));
    assert_eq!(
        result.get("child_fd_inventory"),
        sentinel.get("fd_inventory")
    );
    assert_no_forbidden_child_data(&sentinel_bytes, &canary_path);

    let child_artifact = read_bounded_regular(
        &fixture.result_dir.join("artifacts/child.bin"),
        WU0G_ARTIFACT_CAP_BYTES,
    );
    assert_eq!(
        result.get("artifact_identity").unwrap(),
        &framed_identity("wu0g-child-artifact-v1", &child_artifact)
    );
    assert_eq!(
        exact_decimal(&result, "artifact_size"),
        u64::try_from(child_artifact.len()).unwrap()
    );
    let child_stdout =
        read_bounded_regular(&fixture.result_dir.join("stdout.bin"), WU0G_STDIO_CAP_BYTES);
    let child_stderr =
        read_bounded_regular(&fixture.result_dir.join("stderr.bin"), WU0G_STDIO_CAP_BYTES);
    assert_eq!(
        exact_decimal(&result, "stdout_size"),
        u64::try_from(child_stdout.len()).unwrap()
    );
    assert_eq!(
        exact_decimal(&result, "stderr_size"),
        u64::try_from(child_stderr.len()).unwrap()
    );
    assert_no_forbidden_child_data(&child_stdout, &canary_path);
    assert_no_forbidden_child_data(&child_stderr, &canary_path);

    match kind {
        Wu0gRequestKind::Causal => {
            for key in [
                "perf_artifact_identity",
                "perf_artifact_size",
                "perf_event",
                "perf_exit_code",
                "perf_identity",
                "perf_invocation",
                "perf_raw_wait_status",
                "perf_term_signal",
                "perf_version",
            ] {
                assert_eq!(result.get(key).map(String::as_str), Some("none"));
            }
        }
        Wu0gRequestKind::Performance => {
            let exact_perf = format!(
                "/usr/bin/perf|stat|--no-big-num|--no-scale|-x|;|-e|instructions:u|--log-fd|198|--|/proc/self/fd/197|{WU0G_CHILD_ARGV}"
            );
            assert_eq!(result.get("perf_invocation"), Some(&exact_perf));
            assert_eq!(result.get("perf_exit_code").map(String::as_str), Some("0"));
            assert_eq!(
                result.get("perf_raw_wait_status").map(String::as_str),
                Some("0")
            );
            assert_eq!(
                result.get("perf_term_signal").map(String::as_str),
                Some("none")
            );
            let perf = read_bounded_regular(
                &fixture.result_dir.join("artifacts/perf.csv"),
                WU0G_PERF_ARTIFACT_CAP_BYTES,
            );
            parse_perf_row(&perf);
            assert_eq!(
                result.get("perf_artifact_identity").unwrap(),
                &framed_identity("wu0g-perf-artifact-v1", &perf)
            );
            assert_eq!(
                exact_decimal(&result, "perf_artifact_size"),
                u64::try_from(perf.len()).unwrap()
            );
        }
    }
}

fn assert_rejected_before_launch(fixture: &Wu0gFixture, label: &str, bounded: &BoundedRun) {
    assert_ne!(bounded.raw_wait_status, 0, "{label} was admitted");
    assert!(!bounded.stdout_oversized && !bounded.stderr_oversized);
    assert!(
        bounded.observed_launches.is_empty(),
        "{label} launched libtest"
    );
    assert_all_observed_identities_gone(bounded);
    assert!(!fixture.result_dir.join("result.v1").exists());
    assert!(!fixture
        .result_dir
        .join("artifacts/completion.sentinel")
        .exists());
}

fn run_direct_legacy(scratch: &AcceptanceScratch, label: &str, argument: &str) -> BoundedRun {
    let canary_path = scratch.root.join(format!("legacy-{label}.fd-canary"));
    create_exclusive(&canary_path, b"legacy-inherited-fd-canary\n", false);
    let canary = OpenOptions::new().read(true).open(&canary_path).unwrap();
    let flags = rustix::io::fcntl_getfd(&canary).unwrap();
    rustix::io::fcntl_setfd(&canary, flags & !rustix::io::FdFlags::CLOEXEC).unwrap();
    let mut command = Command::new("/usr/bin/perl");
    command.arg(runner_path()).arg(argument);
    for (key, value) in WU0G_FORBIDDEN_ENV {
        command.env(key, value);
    }
    command.env("TYPOKAT_WU0G_CHILD_REQUEST_FD", "forged-legacy-value");
    let bounded = run_bounded_command(
        command,
        scratch.root.join(format!("legacy-{label}.stdout")),
        scratch.root.join(format!("legacy-{label}.stderr")),
        None,
        None,
        &scratch.root,
    );
    drop(canary);
    bounded
}

fn legacy_self_test_inventory(bytes: &[u8]) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).expect("legacy self-test output UTF-8");
    let mut inventory = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("typokat-wu0e-self-test-observation-v1 case=") else {
            continue;
        };
        let case = rest.split_once(' ').map_or(rest, |(case, _)| case);
        inventory.extend_from_slice(case.as_bytes());
        inventory.push(b'\n');
    }
    inventory
}

#[test]
#[ignore = "WU0E legacy dry-run/self-test inventory; direct and hard-bounded"]
fn wu0g_route_leaves_legacy_diagnostics_byte_for_byte_unchanged() {
    let scratch = AcceptanceScratch::create();
    let dry = run_direct_legacy(&scratch, "dry", "--dry-run");
    assert!(dry.status.success() && dry.raw_wait_status == 0);
    assert!(!dry.stdout_oversized && !dry.stderr_oversized);
    let dry_stdout = read_bounded_regular(
        &scratch.root.join("legacy-dry.stdout"),
        WU0G_STDIO_CAP_BYTES,
    );
    let dry_stderr = read_bounded_regular(
        &scratch.root.join("legacy-dry.stderr"),
        WU0G_STDIO_CAP_BYTES,
    );
    assert_eq!(
        dry_stdout,
        b"typokat-wu0e-runner-dry-v1 mode_order=plain,measured-off,candidate-b build_count=1 workload_count=3 validator_count=3 profile_files=82 warm_regular_files=88 deadline_us=180000000 max_process_group_rss_bytes=1073741824 max_stdout_bytes=131072 max_stderr_bytes=131072 max_trace_bytes=262144\n"
    );
    assert!(dry_stderr.is_empty());

    let self_test = run_direct_legacy(&scratch, "self-test", "--self-test");
    assert!(self_test.status.success() && self_test.raw_wait_status == 0);
    assert!(!self_test.stdout_oversized && !self_test.stderr_oversized);
    let self_stdout = read_bounded_regular(
        &scratch.root.join("legacy-self-test.stdout"),
        WU0G_STDIO_CAP_BYTES,
    );
    let self_stderr = read_bounded_regular(
        &scratch.root.join("legacy-self-test.stderr"),
        WU0G_STDIO_CAP_BYTES,
    );
    assert_eq!(
        legacy_self_test_inventory(&self_stdout),
        WU0G_LEGACY_SELF_TEST_INVENTORY
    );
    assert!(self_stderr.is_empty());
    for output in [&dry_stdout, &dry_stderr, &self_stdout, &self_stderr] {
        assert!(!String::from_utf8_lossy(output)
            .to_ascii_lowercase()
            .contains("wu0g"));
    }
    assert_all_observed_identities_gone(&dry);
    assert_all_observed_identities_gone(&self_test);
    assert!(std::fs::read_dir(&scratch.evidence)
        .unwrap()
        .next()
        .is_none());
    scratch.finish();
}

#[test]
#[ignore = "WU0G actual direct CLI request/result/sentinel and cgroup acceptance"]
fn wu0g_actual_direct_cli_route_is_independently_observed() {
    let scratch = AcceptanceScratch::create();
    for (name, kind) in [
        ("causal-valid", Wu0gRequestKind::Causal),
        ("performance-valid", Wu0gRequestKind::Performance),
    ] {
        let fixture = prepare_wu0g_fixture(&scratch.root, name, kind);
        let bounded = run_direct_wu0g(&fixture, "valid");
        assert_valid_wu0g_run(&fixture, kind, &bounded);
    }
    scratch.finish();
}

#[test]
#[ignore = "WU0G actual invalid canonical request matrix; hard-bounded and zero launch"]
fn wu0g_actual_direct_cli_rejects_invalid_request_matrix() {
    let scratch = AcceptanceScratch::create();
    let causal_seed = prepare_wu0g_fixture(&scratch.root, "causal-seed", Wu0gRequestKind::Causal);
    let performance_seed = prepare_wu0g_fixture(
        &scratch.root,
        "performance-seed",
        Wu0gRequestKind::Performance,
    );
    let causal = causal_seed.request_bytes.clone();
    let performance = performance_seed.request_bytes.clone();
    std::fs::remove_dir_all(&causal_seed.root).unwrap();
    std::fs::remove_dir_all(&performance_seed.root).unwrap();
    let mut cases = vec![
        (
            Wu0gRequestKind::Causal,
            "invalid-kind",
            replace_wu0g_request_field(&causal, "kind", "control"),
        ),
        (
            Wu0gRequestKind::Causal,
            "invalid-mode",
            replace_wu0g_request_field(&causal, "mode", "plain"),
        ),
        (
            Wu0gRequestKind::Causal,
            "causal-launch-field",
            replace_wu0g_request_field(
                &causal,
                "launch_identity",
                &sha256_hex(b"forbidden-launch"),
            ),
        ),
        (
            Wu0gRequestKind::Causal,
            "causal-pair-field",
            replace_wu0g_request_field(&causal, "pair_ordinal", "0"),
        ),
        (
            Wu0gRequestKind::Causal,
            "causal-perf-field",
            replace_wu0g_request_field(&causal, "perf_event", "instructions:u"),
        ),
        (
            Wu0gRequestKind::Performance,
            "performance-rung-field",
            replace_wu0g_request_field(
                &performance,
                "rung_identity",
                &sha256_hex(b"forbidden-rung"),
            ),
        ),
        (
            Wu0gRequestKind::Causal,
            "causal-rung-none",
            replace_wu0g_request_field(&causal, "rung_ordinal", "none"),
        ),
        (
            Wu0gRequestKind::Causal,
            "causal-rung-five",
            replace_wu0g_request_field(&causal, "rung_ordinal", "5"),
        ),
        (
            Wu0gRequestKind::Performance,
            "performance-pair-none",
            replace_wu0g_request_field(&performance, "pair_ordinal", "none"),
        ),
        (
            Wu0gRequestKind::Performance,
            "performance-pair-five",
            replace_wu0g_request_field(&performance, "pair_ordinal", "5"),
        ),
        (
            Wu0gRequestKind::Performance,
            "performance-launch-none",
            replace_wu0g_request_field(&performance, "launch_ordinal", "none"),
        ),
        (
            Wu0gRequestKind::Performance,
            "performance-launch-ten",
            replace_wu0g_request_field(&performance, "launch_ordinal", "10"),
        ),
        (
            Wu0gRequestKind::Causal,
            "rss-over-memory",
            replace_wu0g_request_field(
                &replace_wu0g_request_field(&causal, "memory_limit_bytes", "268435456"),
                "rss_limit_bytes",
                "314572800",
            ),
        ),
        (
            Wu0gRequestKind::Causal,
            "nofile-soft-over-hard",
            replace_wu0g_request_field(
                &replace_wu0g_request_field(&causal, "nofile_soft", "256"),
                "nofile_hard",
                "128",
            ),
        ),
    ];
    for (key, maximum) in [
        ("deadline_ms", 30_000),
        ("memory_limit_bytes", 536_870_912),
        ("rss_limit_bytes", 402_653_184),
        ("nofile_soft", 256),
        ("nofile_hard", 256),
    ] {
        cases.push((
            Wu0gRequestKind::Causal,
            "zero-limit",
            replace_wu0g_request_field(&causal, key, "0"),
        ));
        cases.push((
            Wu0gRequestKind::Causal,
            "over-limit",
            replace_wu0g_request_field(&causal, key, &(maximum + 1).to_string()),
        ));
    }
    for key in [
        "binary_identity",
        "candidate_identity",
        "host_identity",
        "plan_identity",
        "prlimit_identity",
        "result_identity",
        "rung_identity",
        "workload_identity",
    ] {
        cases.push((
            Wu0gRequestKind::Causal,
            "malformed-causal-identity",
            replace_wu0g_request_field(&causal, key, &"f".repeat(63)),
        ));
        cases.push((
            Wu0gRequestKind::Causal,
            "mismatched-causal-identity",
            replace_wu0g_request_field(&causal, key, &"e".repeat(64)),
        ));
    }
    for key in ["launch_identity", "pair_identity", "perf_identity"] {
        cases.push((
            Wu0gRequestKind::Performance,
            "malformed-performance-identity",
            replace_wu0g_request_field(&performance, key, &"f".repeat(63)),
        ));
        cases.push((
            Wu0gRequestKind::Performance,
            "mismatched-performance-identity",
            replace_wu0g_request_field(&performance, key, &"e".repeat(64)),
        ));
    }
    let lines = causal.split(|byte| *byte == b'\n').collect::<Vec<_>>();
    let mut missing = lines.clone();
    missing.remove(2);
    cases.push((Wu0gRequestKind::Causal, "missing", missing.join(&b'\n')));
    let mut duplicate = lines.clone();
    duplicate.insert(2, duplicate[1]);
    cases.push((Wu0gRequestKind::Causal, "duplicate", duplicate.join(&b'\n')));
    let mut unknown = lines.clone();
    unknown.insert(2, b"unknown=1:x");
    cases.push((Wu0gRequestKind::Causal, "unknown", unknown.join(&b'\n')));
    let mut reordered = lines;
    reordered.swap(1, 2);
    cases.push((Wu0gRequestKind::Causal, "reordered", reordered.join(&b'\n')));

    for (index, (kind, label, request)) in cases.into_iter().enumerate() {
        let fixture = prepare_wu0g_fixture(&scratch.root, &format!("invalid-{index}"), kind);
        std::fs::write(&fixture.request_path, request).expect("install invalid request");
        let bounded = run_direct_wu0g(&fixture, "invalid");
        assert_rejected_before_launch(&fixture, label, &bounded);
    }
    scratch.finish();
}

fn replace_fixture_request_field(fixture: &mut Wu0gFixture, key: &str, value: &str) {
    fixture.request_bytes = replace_wu0g_request_field(&fixture.request_bytes, key, value);
    fixture
        .request_fields
        .insert(key.to_owned(), value.to_owned());
    std::fs::write(&fixture.request_path, &fixture.request_bytes).unwrap();
}

fn install_replacement_attacker(path: &Path, marker: &Path) {
    let source = format!(
        "#!/usr/bin/perl\nuse strict; use warnings; open my $h, '>', '{}' or die $!; print {{$h}} \"replacement-executed\\n\"; close $h or die $!; exit 97;\n",
        marker.display()
    );
    create_exclusive(path, source.as_bytes(), true);
}

fn assert_stable_tool_replacement(
    scratch: &AcceptanceScratch,
    name: &str,
    kind: Wu0gRequestKind,
    source: Option<&Path>,
    identity_field: &str,
    injected_environment: Option<&str>,
) {
    let mut fixture = prepare_wu0g_fixture(&scratch.root, name, kind);
    let target = if let Some(source) = source {
        let target = fixture.root.join(format!("tools/{name}"));
        std::fs::copy(source, &target).expect("copy stable tool fixture");
        let mut permissions = std::fs::metadata(&target).unwrap().permissions();
        permissions.set_mode(0o500);
        std::fs::set_permissions(&target, permissions).unwrap();
        let digest = sha256_path(&target);
        replace_fixture_request_field(&mut fixture, identity_field, &digest);
        target
    } else {
        fixture.frozen_libtest.clone()
    };
    let before = std::fs::metadata(&target).unwrap();
    let opened_identity = (before.dev(), before.ino());
    let opened_digest = sha256_path(&target);
    let retained = fixture.root.join(format!("{name}.opened-victim"));
    std::fs::hard_link(&target, &retained).expect("retain Rust-owned opened inode");
    let marker = fixture.root.join(format!("{name}.replacement.marker"));
    let replacement = fixture.root.join(format!("{name}.replacement"));
    install_replacement_attacker(&replacement, &marker);
    let replacement_identity = file_identity(&replacement);
    let attack = StableReplacement {
        target: target.clone(),
        replacement,
        opened_identity,
    };
    let injected = injected_environment.map(|key| (key, target.as_path()));
    let bounded = run_direct_wu0g_with(&fixture, "replacement", injected, Some(&attack));
    assert!(
        bounded.replacement_performed,
        "{name} was not replaced after open"
    );
    assert_ne!(
        bounded.raw_wait_status, 0,
        "path drift must be inconclusive"
    );
    assert_eq!(
        bounded.observed_launches.len(),
        1,
        "{name} stable handle was not executed"
    );
    assert!(!marker.exists(), "{name} attacker executable ran");
    let retained_after = std::fs::metadata(&retained).unwrap();
    assert_eq!(
        (retained_after.dev(), retained_after.ino()),
        opened_identity
    );
    assert_eq!(sha256_path(&retained), opened_digest);
    let target_after = std::fs::metadata(&target).unwrap();
    assert_eq!(
        (target_after.dev(), target_after.ino()),
        (replacement_identity.device, replacement_identity.inode)
    );
    assert_eq!(sha256_path(&target), replacement_identity.digest);
    assert_all_observed_identities_gone(&bounded);
    assert_observed_cgroups_eventually_absent(&bounded.observed_launch_cgroups);
}

#[test]
#[ignore = "WU0G stable libtest/prlimit/perf handles; Rust-owned post-open replacements"]
fn wu0g_direct_route_executes_stable_tool_handles_and_rejects_path_drift() {
    let scratch = AcceptanceScratch::create();
    assert_stable_tool_replacement(
        &scratch,
        "libtest",
        Wu0gRequestKind::Causal,
        None,
        "binary_identity",
        None,
    );
    assert_stable_tool_replacement(
        &scratch,
        "prlimit",
        Wu0gRequestKind::Causal,
        Some(Path::new("/usr/bin/prlimit")),
        "prlimit_identity",
        Some("TYPOKAT_WU0E_TEST_WU0G_PRLIMIT_PATH"),
    );
    assert_stable_tool_replacement(
        &scratch,
        "perf",
        Wu0gRequestKind::Performance,
        Some(Path::new("/usr/bin/perf")),
        "perf_identity",
        Some("TYPOKAT_WU0E_TEST_WU0G_PERF_PATH"),
    );
    scratch.finish();
}

fn assert_unchanged_path_victim(path: &Path, expected: &FileIdentity) {
    assert_eq!(&file_identity(path), expected);
}

#[test]
#[ignore = "WU0G real CLI path attacks; hard-bounded, zero launch, victim unchanged"]
fn wu0g_direct_route_rejects_path_alias_special_file_and_replacement_attacks() {
    use std::os::unix::fs::symlink;

    let scratch = AcceptanceScratch::create();
    let victim = scratch.root.join("path-attack-victim");
    create_exclusive(&victim, b"rust-owned-path-victim-v1\n", false);
    let victim_identity = file_identity(&victim);

    for (index, (label, key, value)) in [
        ("absolute", "artifact_relative_path", "/tmp/wu0g-escape"),
        (
            "traversal",
            "semantic_artifact_relative_path",
            "../path-attack-victim",
        ),
        ("alias", "artifact_relative_path", "artifacts/semantic.bin"),
        (
            "duplicate-path",
            "result_relative_path",
            "requests/launch-0.request",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut fixture = prepare_wu0g_fixture(
            &scratch.root,
            &format!("path-field-{index}"),
            Wu0gRequestKind::Causal,
        );
        replace_fixture_request_field(&mut fixture, key, value);
        let bounded = run_direct_wu0g(&fixture, "path");
        assert_rejected_before_launch(&fixture, label, &bounded);
        assert_unchanged_path_victim(&victim, &victim_identity);
    }

    let leaf = prepare_wu0g_fixture(&scratch.root, "path-symlink-leaf", Wu0gRequestKind::Causal);
    std::fs::remove_file(&leaf.request_path).unwrap();
    symlink(&victim, &leaf.request_path).unwrap();
    let bounded = run_direct_wu0g(&leaf, "path");
    assert_rejected_before_launch(&leaf, "symlink leaf", &bounded);

    let parent = prepare_wu0g_fixture(
        &scratch.root,
        "path-symlink-parent",
        Wu0gRequestKind::Causal,
    );
    let real_requests = scratch.root.join("real-symlink-parent-requests");
    std::fs::create_dir(&real_requests).unwrap();
    create_exclusive(
        &real_requests.join("launch-0.request"),
        &parent.request_bytes,
        false,
    );
    std::fs::remove_file(&parent.request_path).unwrap();
    std::fs::remove_dir(parent.root.join("requests")).unwrap();
    symlink(&real_requests, parent.root.join("requests")).unwrap();
    let bounded = run_direct_wu0g(&parent, "path");
    assert_rejected_before_launch(&parent, "symlink parent", &bounded);

    let fifo = prepare_wu0g_fixture(&scratch.root, "path-fifo", Wu0gRequestKind::Causal);
    std::fs::remove_file(&fifo.request_path).unwrap();
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo.request_path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let bounded = run_direct_wu0g(&fifo, "path");
    assert_rejected_before_launch(&fifo, "FIFO", &bounded);

    let mut device = prepare_wu0g_fixture(&scratch.root, "path-device", Wu0gRequestKind::Causal);
    device.request_path = PathBuf::from("/dev/null");
    let bounded = run_direct_wu0g(&device, "path");
    assert_rejected_before_launch(&device, "device", &bounded);

    for (label, extra) in [("preexisting", false), ("extra-result", true)] {
        let fixture = prepare_wu0g_fixture(
            &scratch.root,
            &format!("path-{label}"),
            Wu0gRequestKind::Causal,
        );
        std::fs::create_dir(&fixture.result_dir).unwrap();
        if extra {
            create_exclusive(
                &fixture.result_dir.join("attacker.extra"),
                b"extra\n",
                false,
            );
        }
        let bounded = run_direct_wu0g(&fixture, "path");
        assert_rejected_before_launch(&fixture, label, &bounded);
    }

    let oversized = prepare_wu0g_fixture(&scratch.root, "path-oversized", Wu0gRequestKind::Causal);
    std::fs::write(
        &oversized.request_path,
        vec![b'x'; usize::try_from(WU0G_REQUEST_CAP_BYTES + 1).unwrap()],
    )
    .unwrap();
    let bounded = run_direct_wu0g(&oversized, "path");
    assert_rejected_before_launch(&oversized, "oversized request", &bounded);

    let replacement =
        prepare_wu0g_fixture(&scratch.root, "path-replacement", Wu0gRequestKind::Causal);
    let request_meta = std::fs::metadata(&replacement.request_path).unwrap();
    let attacker = replacement.root.join("replacement-request.attacker");
    create_exclusive(&attacker, b"attacker-request\n", false);
    let attack = StableReplacement {
        target: replacement.request_path.clone(),
        replacement: attacker,
        opened_identity: (request_meta.dev(), request_meta.ino()),
    };
    let bounded = run_direct_wu0g_with(&replacement, "path", None, Some(&attack));
    assert!(bounded.replacement_performed);
    assert_rejected_before_launch(&replacement, "request replacement", &bounded);

    assert_unchanged_path_victim(&victim, &victim_identity);
    scratch.finish();
}

#[test]
#[ignore = "WU0E delegated-cgroup runner hardening evidence"]
fn runner_hardening_produces_independently_verifiable_evidence() {
    use std::os::unix::fs::symlink;

    let scratch = AcceptanceScratch::create();
    let nonce = format!("{:016x}", NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed));

    let trusted = scratch.fixtures.join("trusted-exec.pl");
    let replacement = scratch.fixtures.join("replacement-exec.pl");
    let replacement_marker = scratch.fixtures.join("replacement-executed.marker");
    let victim = scratch.fixtures.join("victim.bin");
    let append_probe = scratch.fixtures.join("append-probe.pl");
    let nested_failure_probe = scratch.fixtures.join("nested-failure.pl");
    let schedule_complete_sink = scratch.fixtures.join("schedule-complete.sink");
    let schedule_stop_sink = scratch.fixtures.join("schedule-stop.sink");
    let failure_order_sink = scratch.fixtures.join("failure-order.sink");
    let scope_abort_spy = scratch.fixtures.join("scope-abort-spy.pl");
    let scope_abort_spy_sink = scratch.fixtures.join("scope-abort-spy.sink");
    let retained_exception_abort_spy = scratch
        .fixtures
        .join("retained-exception-scope-abort-spy.pl");
    let retained_exception_abort_sink =
        scratch.fixtures.join("retained-exception-scope-abort.sink");
    let production_hook_probe = scratch.fixtures.join("production-hook-probe.pl");
    let production_hook_sink = scratch.fixtures.join("production-hook-routing.sink");
    let production_hook_seed = scratch.fixtures.join("production-hook-seed.fixture");
    let validator_trusted = scratch.fixtures.join("candidate-b-validator-trusted.pl");
    let validator_replacement = scratch
        .fixtures
        .join("candidate-b-validator-replacement.pl");
    let validator_replacement_marker = scratch
        .fixtures
        .join("candidate-b-validator-replacement.marker");
    let synthetic_drain_view = scratch.fixtures.join("synthetic-drain-view.fixture");
    let synthetic_drain_view_bytes = b"typokat-wu0e-synthetic-drain-view-v1\nsource=rust-owned-injected-policy-input\ncgroup_populated=1\npgid_empty=0\ndrain_expired=1\n";
    create_exclusive(
        &trusted,
        b"#!/usr/bin/perl\nprint \"trusted_marker=1\\n\";\n",
        true,
    );
    let replacement_source = format!(
        "#!/usr/bin/perl\nopen my $h, '>', '{}' or die $!; print {{$h}} \"1\\n\"; close $h; print \"replacement_marker=1\\n\";\n",
        replacement_marker.display()
    );
    create_exclusive(&replacement, replacement_source.as_bytes(), true);
    create_exclusive(&victim, b"typokat-wu0e-victim-v1\n", false);
    create_append_probe(&append_probe);
    create_exclusive(
        &nested_failure_probe,
        b"#!/usr/bin/perl\nprint STDERR \"nested-failure-stderr\\n\"; exit 73;\n",
        true,
    );
    create_exclusive(&schedule_complete_sink, b"", false);
    create_exclusive(&schedule_stop_sink, b"", false);
    create_exclusive(&failure_order_sink, b"", false);
    create_scope_abort_spy(&scope_abort_spy);
    create_exclusive(&scope_abort_spy_sink, b"", false);
    create_retained_exception_abort_spy(&retained_exception_abort_spy);
    create_exclusive(&retained_exception_abort_sink, b"", false);
    create_production_hook_probe(&production_hook_probe);
    create_exclusive(&production_hook_sink, b"", false);
    let production_hook_seed_bytes = format!(
        "typokat-wu0e-production-hook-seed-v1\nnonce={nonce}\nsource=rust-owned-dynamic-input\n"
    );
    create_exclusive(
        &production_hook_seed,
        production_hook_seed_bytes.as_bytes(),
        false,
    );
    create_validator_probe(&validator_trusted);
    let validator_replacement_source = format!(
        "#!/usr/bin/perl\nuse strict; use warnings; open my $h, '>', '{}' or die $!; print {{$h}} \"replacement-executed\\n\"; close $h; print \"replacement_validator_marker=1\\n\";\n",
        validator_replacement_marker.display()
    );
    create_exclusive(
        &validator_replacement,
        validator_replacement_source.as_bytes(),
        true,
    );
    create_exclusive(&synthetic_drain_view, synthetic_drain_view_bytes, false);

    let real_parent = scratch.fixtures.join("real-parent");
    std::fs::create_dir(&real_parent).unwrap();
    let symlink_parent = scratch.fixtures.join("symlink-parent");
    symlink(&real_parent, &symlink_parent).unwrap();
    let parent_sentinel = real_parent.join("parent-sentinel.bin");
    create_exclusive(&parent_sentinel, b"parent-sentinel-v1\n", false);
    let temp_sentinel = scratch.fixtures.join("temp-sentinel.bin");
    create_exclusive(&temp_sentinel, b"temp-sentinel-v1\n", false);
    let precreated_temp = scratch.fixtures.join("precreated-temp.bin");
    symlink(&temp_sentinel, &precreated_temp).unwrap();
    let publication_target = scratch.fixtures.join("publication-target.bin");
    create_exclusive(&publication_target, b"publication-target-v1\n", false);
    let victim_before = std::fs::metadata(&victim).unwrap();
    let victim_digest = sha256_hex(&read_bounded_regular(&victim, 1_024));
    let parent_sentinel_before = file_identity(&parent_sentinel);
    let temp_sentinel_before = file_identity(&temp_sentinel);
    let publication_target_before = file_identity(&publication_target);
    let precreated_temp_before = std::fs::symlink_metadata(&precreated_temp).unwrap();
    let symlink_parent_before = std::fs::symlink_metadata(&symlink_parent).unwrap();
    let validator_trusted_sha = sha256_hex(&read_bounded_regular(&validator_trusted, 16 * 1_024));
    let production_hook_seed_sha = sha256_hex(production_hook_seed_bytes.as_bytes());

    let bounded = run_bounded_self_test(&scratch, &nonce);
    assert!(
        bounded.status.success(),
        "artifacts: {}",
        scratch.root.display()
    );
    assert!(bounded.max_descendant_rss <= MAX_OUTER_RSS_BYTES);
    assert!(!bounded.stdout_oversized);
    assert!(!bounded.stderr_oversized);
    let stdout = read_bounded_regular(&scratch.stdout, MAX_CAPTURE_BYTES);
    let stderr = read_bounded_regular(&scratch.stderr, MAX_CAPTURE_BYTES);
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
    assert_eq!(
        stdout,
        format!(
            "typokat-wu0e-hardening-evidence-v1 result=ok nonce={nonce} evidence_dir={}\n",
            scratch.evidence.display()
        )
        .as_bytes()
    );

    let dossier = read_bounded_regular(&scratch.evidence.join("dossier-equal.txt"), 128 * 1_024);
    assert_eq!(dossier, exact_dossier().as_bytes());
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("dossier-equal.sha256"), 128),
        format!("{}\n", sha256_hex(&dossier)).as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("dossier-mismatch.stderr"), 1_024),
        format!(
            "wu0e-diagnostic: completed semantic mismatch: plain={A_SHA} measured-off={B_SHA}\n"
        )
        .as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("termination-cases.txt"), 16 * 1_024),
        exact_termination_cases().as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("preflight-failures.txt"), 16 * 1_024,),
        exact_preflight_failures().as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("rss-churn-cases.txt"), 16 * 1_024),
        exact_rss_churn_cases().as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("linux-state-cases.txt"), 1_024),
        b"typokat-wu0e-linux-state-fixtures-v1\ninput=t output=live\ninput=x output=dead\n"
    );
    assert_eq!(
        read_bounded_regular(
            &scratch.evidence.join("schedule-complete.journal"),
            64 * 1_024
        ),
        exact_schedule_complete().as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("schedule-stop.journal"), 64 * 1_024),
        exact_schedule_stop().as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&schedule_complete_sink, 64 * 1_024),
        exact_schedule_complete().as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&schedule_stop_sink, 64 * 1_024),
        exact_schedule_stop().as_bytes()
    );
    let production_hook_routing = exact_production_hook_routing(
        &validator_trusted,
        &scratch.fixtures,
        &production_hook_seed_sha,
        &validator_trusted_sha,
    );
    assert_eq!(
        read_bounded_regular(
            &scratch.evidence.join("production-hook-routing.journal"),
            64 * 1_024,
        ),
        production_hook_routing.as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&production_hook_sink, 64 * 1_024),
        production_hook_routing.as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&production_hook_seed, 4 * 1_024),
        production_hook_seed_bytes.as_bytes()
    );
    let nested_failure_meta_path = scratch.evidence.join("nested-failure.process-meta");
    let failure_order = exact_failure_order(&nested_failure_meta_path);
    assert_eq!(
        read_bounded_regular(&failure_order_sink, 16 * 1_024),
        failure_order.as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("failure-order.journal"), 16 * 1_024),
        failure_order.as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("nested-failure.stderr"), 1_024),
        b"nested-failure-stderr\n"
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("nested-failure.status"), 32),
        b"73\n"
    );
    let nested_failure = assert_process_metadata(&nested_failure_meta_path, 1_073_741_824);
    assert_eq!(numeric(&nested_failure, "meta_fsync_completed"), 1);
    assert_eq!(
        nested_failure.get("termination").map(String::as_str),
        Some("infrastructure")
    );
    assert_eq!(
        nested_failure
            .get("infrastructure_error")
            .map(String::as_str),
        Some("nested-self-test-failure")
    );
    assert_eq!(numeric(&nested_failure, "validator_launched"), 0);

    let exact_reexec = format!(
        "/usr/bin/systemd-run\n--user\n--scope\n--quiet\n--no-ask-password\n--property=Delegate=yes\n--expand-environment=no\n--\n/usr/bin/perl\n{}\n--self-test-evidence\n{}\n{}\n{nonce}\n",
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tooling/wu0e-diagnostic/run.pl")
            .display(),
        scratch.evidence.display(),
        scratch.fixtures.display(),
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("reexec-argv.txt"), 16 * 1_024),
        exact_reexec.as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("wrapper.stdout"), 1_024),
        b"wrapper-stdout\n"
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("wrapper.stderr"), 1_024),
        b"wrapper-stderr\n"
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("wrapper.status"), 32),
        b"23\n"
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("systemd-run-count"), 32),
        b"1\n"
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("forged-marker.stderr"), 1_024),
        b"wu0e-diagnostic: forged delegated-scope marker\n"
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("nested-marker.stderr"), 1_024),
        b"wu0e-diagnostic: nested delegated-scope reexec\n"
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("stable-exec.stdout"), 1_024),
        b"trusted_marker=1\n"
    );
    assert!(!replacement_marker.exists());
    assert_eq!(
        read_bounded_regular(
            &scratch.evidence.join("stable-exec.path-drift.stderr"),
            1_024
        ),
        b"wu0e-diagnostic: frozen executable pathname identity drifted\n"
    );
    assert_eq!(
        read_bounded_regular(
            &scratch
                .evidence
                .join("candidate-b-validator-path-drift.stderr"),
            1_024,
        ),
        b"wu0e-diagnostic: frozen executable pathname identity drifted\n"
    );
    assert_eq!(
        read_bounded_regular(
            &scratch.evidence.join("candidate-b-validator.stdout"),
            4 * 1_024,
        ),
        b"trusted_validator_marker=1\ntypokat-wu0e-validation-v1 mode=candidate-b termination=normal status=complete semantic_sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
    );
    assert_eq!(
        read_bounded_regular(
            &scratch
                .evidence
                .join("candidate-b-validator-launch.journal"),
            4 * 1_024,
        ),
        exact_candidate_b_validator_path_drift().as_bytes()
    );
    assert!(!validator_replacement_marker.exists());
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("artifact-replacement.stderr"), 1_024),
        b"wu0e-diagnostic: artifact inode changed during bounded access\n"
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("filesystem-cases.txt"), 4 * 1_024),
        b"typokat-wu0e-filesystem-fixtures-v1\ncase=symlink-parent outcome=rejected error=parent-not-real\ncase=precreated-temp-symlink outcome=rejected error=exclusive-nofollow-create\ncase=publication-target-race outcome=rejected error=no-replace-publication\n"
    );

    let victim_after = std::fs::metadata(&victim).unwrap();
    assert_eq!(
        (victim_before.dev(), victim_before.ino()),
        (victim_after.dev(), victim_after.ino())
    );
    assert_eq!(
        victim_digest,
        sha256_hex(&read_bounded_regular(&victim, 1_024))
    );
    assert_eq!(file_identity(&parent_sentinel), parent_sentinel_before);
    assert_eq!(file_identity(&temp_sentinel), temp_sentinel_before);
    assert_eq!(
        file_identity(&publication_target),
        publication_target_before
    );
    let precreated_temp_after = std::fs::symlink_metadata(&precreated_temp).unwrap();
    assert!(precreated_temp_after.file_type().is_symlink());
    assert_eq!(
        (precreated_temp_after.dev(), precreated_temp_after.ino()),
        (precreated_temp_before.dev(), precreated_temp_before.ino())
    );
    assert_eq!(std::fs::read_link(&precreated_temp).unwrap(), temp_sentinel);
    let symlink_parent_after = std::fs::symlink_metadata(&symlink_parent).unwrap();
    assert!(symlink_parent_after.file_type().is_symlink());
    assert_eq!(
        (symlink_parent_after.dev(), symlink_parent_after.ino()),
        (symlink_parent_before.dev(), symlink_parent_before.ino())
    );
    assert_eq!(std::fs::read_link(&symlink_parent).unwrap(), real_parent);

    let monitor = assert_process_metadata(
        &scratch.evidence.join("monitor-exception.process-meta"),
        1_073_741_824,
    );
    assert_eq!(
        monitor.get("termination").map(String::as_str),
        Some("infrastructure")
    );
    assert_eq!(
        monitor.get("infrastructure_error").map(String::as_str),
        Some("synthetic-monitor-exception")
    );
    for key in [
        "readiness_seen",
        "membership_verified",
        "setsid_verified",
        "direct_kill_attempted",
        "pgid_kill_attempted",
        "cgroup_kill_attempted",
        "cleanup_populated_zero",
        "cleanup_pgid_empty",
        "leader_reaped",
        "cgroup_removed",
    ] {
        assert_eq!(numeric(&monitor, key), 1);
    }
    assert_eq!(numeric(&monitor, "emergency_attempts"), 1);
    assert!(numeric(&monitor, "emergency_attempt_1_elapsed_us") <= 250_000);
    assert_eq!(numeric(&monitor, "validator_launched"), 0);
    assert_eq!(numeric(&monitor, "cgroup_retained"), 0);
    assert_pid_identity_gone(
        u32::try_from(numeric(&monitor, "leader_pid")).unwrap(),
        numeric(&monitor, "leader_start_ticks"),
    );
    assert!(!Path::new(monitor.get("launch_cgroup").unwrap()).exists());

    let synthetic_drain = assert_process_metadata(
        &scratch
            .evidence
            .join("synthetic-drain-retention.process-meta"),
        1_073_741_824,
    );
    assert_eq!(
        synthetic_drain.get("termination").map(String::as_str),
        Some("infrastructure")
    );
    assert_eq!(
        synthetic_drain
            .get("infrastructure_error")
            .map(String::as_str),
        Some("post-kill-drain-expired")
    );
    assert_eq!(
        synthetic_drain.get("fixture_source").map(String::as_str),
        Some("rust-owned-injected-policy-input")
    );
    assert_eq!(
        synthetic_drain.get("kind").map(String::as_str),
        Some("synthetic-drain-policy-fixture")
    );
    assert_eq!(
        synthetic_drain.get("drain_view_source").map(String::as_str),
        Some("synthetic-injected")
    );
    assert_eq!(
        synthetic_drain.get("fixture_sha256").map(String::as_str),
        Some(sha256_hex(synthetic_drain_view_bytes).as_str())
    );
    assert_eq!(
        read_bounded_regular(&synthetic_drain_view, 1_024),
        synthetic_drain_view_bytes
    );
    assert!(!synthetic_drain.contains_key("live_member_state"));
    assert_eq!(numeric(&synthetic_drain, "injected_cgroup_populated"), 1);
    assert_eq!(numeric(&synthetic_drain, "injected_pgid_empty"), 0);
    assert_eq!(numeric(&synthetic_drain, "injected_drain_expired"), 1);
    assert_eq!(numeric(&synthetic_drain, "emergency_attempts"), 2);
    assert!(numeric(&synthetic_drain, "emergency_attempt_1_elapsed_us") <= 250_000);
    assert!(numeric(&synthetic_drain, "emergency_attempt_2_elapsed_us") <= 250_000);
    assert_eq!(numeric(&synthetic_drain, "validator_launched"), 0);
    assert_eq!(numeric(&synthetic_drain, "cgroup_retained"), 1);
    assert_eq!(numeric(&synthetic_drain, "cgroup_removed"), 0);
    assert_eq!(numeric(&synthetic_drain, "cleanup_populated_zero"), 0);
    assert_eq!(numeric(&synthetic_drain, "cleanup_pgid_empty"), 0);
    assert_eq!(numeric(&synthetic_drain, "leader_reaped"), 0);
    assert_eq!(numeric(&synthetic_drain, "scope_abort_requested"), 1);
    for key in [
        "direct_kill_attempted",
        "pgid_kill_attempted",
        "cgroup_kill_attempted",
    ] {
        assert_eq!(numeric(&synthetic_drain, key), 1);
    }
    assert!(!Path::new(synthetic_drain.get("launch_cgroup").unwrap()).exists());

    let retained_exception_meta_path = scratch.evidence.join("retained-exception.process-meta");
    let retained_exception = assert_process_metadata(&retained_exception_meta_path, 1_073_741_824);
    assert_eq!(
        retained_exception.get("termination").map(String::as_str),
        Some("infrastructure")
    );
    assert_eq!(
        retained_exception
            .get("infrastructure_error")
            .map(String::as_str),
        Some("synthetic-retained-lifecycle-exception")
    );
    assert_eq!(
        retained_exception
            .get("exception_phase")
            .map(String::as_str),
        Some("post-fork")
    );
    assert_eq!(numeric(&retained_exception, "meta_fsync_completed"), 1);
    assert_eq!(numeric(&retained_exception, "cgroup_retained"), 1);
    assert_eq!(numeric(&retained_exception, "scope_abort_requested"), 1);
    assert_eq!(numeric(&retained_exception, "validator_launched"), 0);
    let retained_unit = retained_exception.get("scope_unit").unwrap();
    let retained_control_group = retained_exception.get("scope_control_group").unwrap();
    assert_eq!(
        read_bounded_regular(&retained_exception_abort_sink, 4 * 1_024),
        exact_scope_abort_spy(retained_unit, retained_control_group).as_bytes()
    );
    assert_eq!(
        read_bounded_regular(
            &scratch.evidence.join("retained-exception-order.journal"),
            16 * 1_024,
        ),
        exact_retained_exception_order(
            &retained_exception_meta_path,
            retained_unit,
            retained_control_group,
        )
        .as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("retained-exception.stderr"), 1_024,),
        b"wu0e-diagnostic: synthetic-retained-lifecycle-exception\n"
    );
    assert!(!Path::new(retained_exception.get("launch_cgroup").unwrap()).exists());

    let low_memory = assert_process_metadata(
        &scratch.evidence.join("low-memory.process-meta"),
        64 * 1_024 * 1_024,
    );
    assert!(numeric(&low_memory, "memory_max") < MAX_OUTER_RSS_BYTES);
    assert_eq!(
        low_memory.get("termination").map(String::as_str),
        Some("rss")
    );
    assert_eq!(numeric(&low_memory, "real_kernel_event"), 1);
    let source = low_memory.get("memory_source").unwrap().as_str();
    let causal_delta = match source {
        "oom" => numeric(&low_memory, "events_oom_delta"),
        "oom_kill" => numeric(&low_memory, "events_oom_kill_delta"),
        "oom_group_kill" => numeric(&low_memory, "events_oom_group_kill_delta"),
        other => panic!("non-causal memory source: {other}"),
    };
    assert!(numeric(&low_memory, "events_max_delta") > 0);
    assert!(causal_delta > 0);

    let delegation = parse_record(
        &scratch.evidence.join("delegation.meta"),
        "typokat-wu0e-delegation-meta-v1",
    );
    let proc_control_group = delegation.get("proc_control_group").unwrap();
    let scope_unit = delegation.get("scope_unit").unwrap();
    assert!(proc_control_group.starts_with('/') && !proc_control_group.contains(".."));
    assert_eq!(
        delegation.get("systemctl_control_group"),
        Some(proc_control_group)
    );
    assert_eq!(
        delegation.get("systemctl_delegate").map(String::as_str),
        Some("yes")
    );
    assert_eq!(
        delegation
            .get("runner_enabled_controller")
            .map(String::as_str),
        Some("memory")
    );
    assert_eq!(
        delegation.get("controllers_before"),
        delegation.get("controllers_after")
    );
    assert_eq!(
        read_bounded_regular(&scope_abort_spy_sink, 4 * 1_024),
        exact_scope_abort_spy(scope_unit, proc_control_group).as_bytes()
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("scope-abort.outcome"), 4 * 1_024),
        format!(
            "typokat-wu0e-scope-abort-v2 retained_at_process_meta=1 abort_request_observed=1 abort_request_callback_count=1 systemctl_argv=/usr/bin/systemctl|--user|--no-block|stop|{scope_unit} retained_launch_removed=1 outer_scope_observation=deferred-to-rust-parent\n"
        )
        .as_bytes()
    );
    let scope_path = Path::new("/sys/fs/cgroup").join(proc_control_group.trim_start_matches('/'));
    let supervisor_path = Path::new(delegation.get("supervisor_cgroup").unwrap());
    assert_eq!(supervisor_path, scope_path.join("supervisor"));
    assert!(!supervisor_path.exists());
    assert_eq!(
        std::fs::symlink_metadata(&scope_path).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let parent_scope_outcome = b"typokat-wu0e-scope-abort-parent-v1 outer_scope_disappearance_observed_by_parent=1 exit_cause=normal-evidence-scope-completion-not-attributed-to-abort\n";
    let parent_scope_outcome_path = scratch.evidence.join("scope-abort.parent-outcome");
    create_exclusive(&parent_scope_outcome_path, parent_scope_outcome, false);
    assert_eq!(
        read_bounded_regular(&parent_scope_outcome_path, 1_024),
        parent_scope_outcome
    );
    assert_eq!(
        delegation.get("teardown_termination").map(String::as_str),
        Some("normal")
    );
    let coordinator_pid = u32::try_from(numeric(&delegation, "coordinator_pid")).unwrap();
    let coordinator_start_ticks = numeric(&delegation, "coordinator_start_ticks");
    assert_pid_identity_gone(coordinator_pid, coordinator_start_ticks);
    let exact_delegation_journal = format!(
        "typokat-wu0e-delegation-journal-v1\n\
seq=0 event=scope-cross-check control_group={proc_control_group} delegate=yes\n\
seq=1 event=supervisor-created path={}\n\
seq=2 event=coordinator-moved pid={coordinator_pid} destination={}\n\
seq=3 event=delegated-root-empty members=0\n\
seq=4 event=controller-enabled name=memory\n\
seq=5 event=launch-fixtures-complete\n\
seq=6 event=controller-disabled name=memory\n\
seq=7 event=coordinator-moved-back pid={coordinator_pid} destination={}\n\
seq=8 event=supervisor-empty members=0\n\
seq=9 event=supervisor-removed path={}\n",
        supervisor_path.display(),
        supervisor_path.display(),
        scope_path.display(),
        supervisor_path.display(),
    );
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("delegation.journal"), 16 * 1_024),
        exact_delegation_journal.as_bytes()
    );

    let preflight = parse_record(
        &scratch.evidence.join("preflight.meta"),
        "typokat-wu0e-cgroup-preflight-v1",
    );
    assert_eq!(
        preflight.get("cgroup_type").map(String::as_str),
        Some("domain")
    );
    assert_eq!(
        preflight.get("checked_files").map(String::as_str),
        Some(
            "cgroup.type,cgroup.procs,cgroup.events,cgroup.kill,memory.max,memory.swap.max,memory.oom.group,memory.current,memory.peak,memory.events.local"
        )
    );
    assert_eq!(
        preflight.get("child_action_order").map(String::as_str),
        Some("self-move,setsid,readiness,environment,exec")
    );
    assert_eq!(
        preflight
            .get("parent_readiness_evidence")
            .map(String::as_str),
        Some("membership,pgid")
    );
    assert_eq!(
        preflight.get("cgroup_kill_access").map(String::as_str),
        Some("writable")
    );
    assert_eq!(numeric(&preflight, "memory_max_readback"), 1_073_741_824);
    assert_eq!(numeric(&preflight, "memory_swap_max_readback"), 0);
    assert_eq!(numeric(&preflight, "memory_oom_group_readback"), 1);
    assert_eq!(numeric(&preflight, "rss_retry_attempts"), 3);
    assert_eq!(numeric(&preflight, "rss_retry_deadline_us"), 10_000);
    assert_eq!(
        numeric(&preflight, "unresolved_churn_termination_infrastructure"),
        1
    );
    let preflight_action_trace = read_bounded_regular(
        &scratch.evidence.join("preflight-action.journal"),
        16 * 1_024,
    );
    assert_eq!(
        preflight_action_trace,
        exact_preflight_action_trace().as_bytes()
    );
    assert_eq!(
        preflight.get("action_trace_source").map(String::as_str),
        Some("real-hardened-child")
    );
    assert_eq!(
        preflight.get("action_trace_artifact").map(String::as_str),
        Some("preflight-action.journal")
    );
    assert_eq!(
        preflight.get("action_trace_sha256").map(String::as_str),
        Some(sha256_hex(&preflight_action_trace).as_str())
    );
    assert_eq!(numeric(&preflight, "action_trace_launch_count"), 1);
    assert!(!Path::new(preflight.get("launch_cgroup").unwrap()).exists());

    let teardown_failure = assert_process_metadata(
        &scratch.evidence.join("teardown-failure.process-meta"),
        1_073_741_824,
    );
    assert_eq!(
        teardown_failure.get("termination").map(String::as_str),
        Some("infrastructure")
    );
    assert_eq!(
        teardown_failure
            .get("infrastructure_error")
            .map(String::as_str),
        Some("synthetic-delegated-root-teardown-failure")
    );
    assert_eq!(numeric(&teardown_failure, "validator_launched"), 0);
    assert_eq!(numeric(&teardown_failure, "scope_abort_requested"), 1);
    for fields in [
        &monitor,
        &synthetic_drain,
        &low_memory,
        &nested_failure,
        &teardown_failure,
        &retained_exception,
    ] {
        assert_eq!(fields.get("scope_unit"), delegation.get("scope_unit"));
        assert_eq!(
            fields.get("scope_control_group"),
            delegation.get("proc_control_group")
        );
        let launch_cgroup = Path::new(fields.get("launch_cgroup").unwrap());
        assert!(launch_cgroup.starts_with(&scope_path));
    }

    let metadata_files = std::fs::read_dir(&scratch.evidence)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "process-meta")
        })
        .collect::<Vec<_>>();
    assert_eq!(metadata_files.len(), 6);
    for path in metadata_files {
        let expected = if path
            .file_name()
            .is_some_and(|name| name == "low-memory.process-meta")
        {
            64 * 1_024 * 1_024
        } else {
            1_073_741_824
        };
        assert_process_metadata(&path, expected);
    }

    let expected_files = [
        "artifact-replacement.stderr",
        "candidate-b-validator-launch.journal",
        "candidate-b-validator-path-drift.stderr",
        "candidate-b-validator.stdout",
        "delegation.journal",
        "delegation.meta",
        "dossier-equal.sha256",
        "dossier-equal.txt",
        "dossier-mismatch.stderr",
        "filesystem-cases.txt",
        "failure-order.journal",
        "forged-marker.stderr",
        "linux-state-cases.txt",
        "low-memory.process-meta",
        "monitor-exception.process-meta",
        "nested-failure.process-meta",
        "nested-failure.status",
        "nested-failure.stderr",
        "nested-marker.stderr",
        "preflight-failures.txt",
        "preflight-action.journal",
        "preflight.meta",
        "production-hook-routing.journal",
        "reexec-argv.txt",
        "retained-exception-order.journal",
        "retained-exception.process-meta",
        "retained-exception.stderr",
        "rss-churn-cases.txt",
        "schedule-complete.journal",
        "schedule-stop.journal",
        "scope-abort.outcome",
        "scope-abort.parent-outcome",
        "stable-exec.path-drift.stderr",
        "stable-exec.stdout",
        "synthetic-drain-retention.process-meta",
        "systemd-run-count",
        "teardown-failure.process-meta",
        "termination-cases.txt",
        "wrapper.status",
        "wrapper.stderr",
        "wrapper.stdout",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let actual_files = std::fs::read_dir(&scratch.evidence)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .map(|name| name.into_string().expect("ASCII evidence name"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_files,
        expected_files
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );

    scratch.finish();
}

#[test]
fn hardening_remains_diagnostic_only_and_leaves_wu0d_unchanged() {
    let wu0d = include_str!("wu0d_candidate_release.rs");
    let wu0d_runner = include_str!("../../../tooling/wu0d-release/run.pl");
    let wu0e_runner = include_str!("../../../tooling/wu0e-diagnostic/run.pl");

    assert!(!wu0d.contains("wu0e"));
    assert!(!wu0d_runner.contains("wu0e"));
    assert!(!wu0e_runner.contains("tooling/wu0d-release"));
    assert!(wu0d.contains("const MAX_ELAPSED_US: u64 = 5_000_000;"));
    assert!(wu0d_runner.contains("my $TIMEOUT_SECONDS = 5;"));
    for forbidden in [
        "GateDecision",
        "evaluate_candidate_b_release",
        "validate_candidate_b_release_evidence_file",
        "TYPOKAT_WU0D_RELEASE_EVIDENCE_PATH",
        "typokat-wu0d-release-evidence-v1",
        "authorizes_candidate_b",
    ] {
        assert!(!wu0e_runner.contains(forbidden));
    }
    for obsolete_v1_route in [
        "\nsub run_workload {",
        "\nsub run_validator {",
        "\nsub write_process_meta {",
        "\nsub write_dossier {",
        "typokat-wu0e-diagnostic-dossier-v1",
    ] {
        assert!(
            !wu0e_runner.contains(obsolete_v1_route),
            "obsolete WU0E v1 route remains: {obsolete_v1_route}"
        );
    }
}
