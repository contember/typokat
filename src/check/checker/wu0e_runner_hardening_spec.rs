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
//! disappearance is verified only as the later outcome of aborting the enclosing scope.
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
//!
//! The acceptance itself never uses unbounded `Command::output()`. A Rust-owned watchdog runs the
//! runner in a fresh session. Two bounded pipe readers accept at most limit+1 bytes and then close,
//! so stdout/stderr are OS-bounded without a shell, unsafe code, or resource-limit shim. The
//! watchdog polls summed descendant RSS, performs a final post-exit sample/check, enforces a
//! deadline, and kills the outer process tree on failure; it does not misdescribe polling as an
//! absolute hard RSS cap. Once reexeced, the launch fixtures additionally have their real cgroup
//! memory backstops. Scratch is removed only after success, so failure artifacts survive.

#![cfg(target_os = "linux")]

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
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

struct BoundedRun {
    status: ExitStatus,
    max_descendant_rss: u64,
    stdout_oversized: bool,
    stderr_oversized: bool,
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

fn run_bounded_self_test(scratch: &AcceptanceScratch, nonce: &str) -> BoundedRun {
    let runner = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tooling")
        .join("wu0e-diagnostic")
        .join("run.pl");
    let mut child = Command::new("/usr/bin/setsid")
        .arg("/usr/bin/perl")
        .arg(runner)
        .arg("--self-test-evidence")
        .arg(&scratch.evidence)
        .arg(&scratch.fixtures)
        .arg(nonce)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch bounded WU0E self-test");
    let stdout_capture = start_bounded_capture(
        child.stdout.take().expect("take child stdout"),
        scratch.stdout.clone(),
    );
    let stderr_capture = start_bounded_capture(
        child.stderr.take().expect("take child stderr"),
        scratch.stderr.clone(),
    );
    let root = child.id();
    let started = Instant::now();
    let mut max_descendant_rss = 0;
    let mut known_identities = BTreeMap::new();
    loop {
        if let Some(status) = child.try_wait().expect("poll WU0E self-test") {
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
                        scratch.root.display()
                    );
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let stdout_oversized = stdout_capture.finish();
            let stderr_oversized = stderr_capture.finish();
            return BoundedRun {
                status,
                max_descendant_rss,
                stdout_oversized,
                stderr_oversized,
            };
        }
        let snapshot = proc_snapshot();
        for pid in tree_members(root, &snapshot) {
            known_identities.insert(pid, snapshot.get(&pid).unwrap().start_ticks);
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
                scratch.root.display()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
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
    assert_eq!(
        read_bounded_regular(&scratch.evidence.join("scope-abort.outcome"), 1_024),
        b"typokat-wu0e-scope-abort-v1 retained_at_process_meta=1 abort_requested=1 scope_disappeared=1\n"
    );
    assert!(!Path::new(synthetic_drain.get("launch_cgroup").unwrap()).exists());

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
    let scope_path = Path::new("/sys/fs/cgroup").join(proc_control_group.trim_start_matches('/'));
    let supervisor_path = Path::new(delegation.get("supervisor_cgroup").unwrap());
    assert_eq!(supervisor_path, scope_path.join("supervisor"));
    assert!(!supervisor_path.exists());
    assert!(!scope_path.exists());
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
    assert_eq!(metadata_files.len(), 5);
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
        "preflight.meta",
        "reexec-argv.txt",
        "rss-churn-cases.txt",
        "schedule-complete.journal",
        "schedule-stop.journal",
        "scope-abort.outcome",
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
}
