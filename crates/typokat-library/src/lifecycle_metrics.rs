//! Removable sidecar metrics for authenticated lifecycle measurements.

use crate::frontend::FileInput;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const PATH_ENV: &str = "TYPOKAT_LIBRARY_LIFECYCLE_METRICS";
const NONCE_ENV: &str = "TYPOKAT_LIBRARY_LIFECYCLE_NONCE";

static SINK: OnceLock<Option<MetricsSink>> = OnceLock::new();
static NEXT_FRAME: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static ACTIVE_FRAME: RefCell<Option<ActiveFrame>> = const { RefCell::new(None) };
}

struct MetricsSink {
    path: PathBuf,
    nonce: String,
    writer: Mutex<()>,
}

struct ActiveFrame {
    sequence: u64,
    started: Instant,
}

#[derive(Clone, Copy)]
pub(crate) enum ProjectRoute {
    Shared,
    Private,
    CompleteSourceFallback,
    UserParseRejected,
    PreflightRejected,
}

impl ProjectRoute {
    const fn label(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Private => "private",
            Self::CompleteSourceFallback => "complete-source-fallback",
            Self::UserParseRejected => "user-parse-rejected",
            Self::PreflightRejected => "preflight-rejected",
        }
    }
}

pub(crate) fn record_initialization(compile_us: u64, publication_us: u64, total_us: u64) {
    write_event("init", 0, "ready", compile_us, publication_us, total_us);
}

pub(crate) fn begin_project_route(inputs: &[FileInput]) {
    if sink().is_none() {
        return;
    }
    let sequence = NEXT_FRAME.fetch_add(1, Ordering::Relaxed);
    let digest = input_name_digest(inputs);
    ACTIVE_FRAME.with_borrow_mut(|active| {
        *active = Some(ActiveFrame {
            sequence,
            started: Instant::now(),
        });
    });
    write_event("frame", sequence, &digest, 0, 0, 0);
}

pub(crate) fn record_project_route(route: ProjectRoute) {
    let active = ACTIVE_FRAME.with_borrow(|active| {
        active
            .as_ref()
            .map(|frame| (frame.sequence, elapsed_us(frame.started)))
    });
    if let Some((sequence, elapsed)) = active {
        write_event("route", sequence, route.label(), elapsed, 0, 0);
    }
}

fn sink() -> Option<&'static MetricsSink> {
    SINK.get_or_init(|| {
        let path = std::env::var_os(PATH_ENV).map(PathBuf::from)?;
        let nonce = std::env::var(NONCE_ENV).ok()?;
        if nonce.len() != 64
            || !nonce
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        Some(MetricsSink {
            path,
            nonce,
            writer: Mutex::new(()),
        })
    })
    .as_ref()
}

fn write_event(kind: &str, sequence: u64, value: &str, first: u64, second: u64, third: u64) {
    let Some(sink) = sink() else {
        return;
    };
    let Ok(_guard) = sink.writer.lock() else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().append(true).open(&sink.path) else {
        return;
    };
    let _ignored = writeln!(
        file,
        "1\t{}\t{}\t{kind}\t{sequence}\t{value}\t{first}\t{second}\t{third}",
        sink.nonce,
        std::process::id(),
    );
}

fn input_name_digest(inputs: &[FileInput]) -> String {
    let mut digest = Sha256::new();
    digest.update(
        u64::try_from(inputs.len())
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    for input in inputs {
        let name = input.name.as_bytes();
        digest.update(u64::try_from(name.len()).unwrap_or(u64::MAX).to_le_bytes());
        digest.update(name);
    }
    format!("{:x}", digest.finalize())
}

fn elapsed_us(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros())
        .unwrap_or(u64::MAX)
        .max(1)
}
