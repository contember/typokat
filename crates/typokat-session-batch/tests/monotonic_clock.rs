//! Acceptance for process-comparable runner timestamps.

use rustix::time::{clock_gettime, ClockId};
use typokat_check::check::checker::project_local::SessionBenchClock;

fn monotonic_ns() -> u64 {
    let now = clock_gettime(ClockId::Monotonic);
    u64::try_from(now.tv_sec)
        .expect("nonnegative monotonic seconds")
        .checked_mul(1_000_000_000)
        .and_then(|seconds| {
            seconds.checked_add(u64::try_from(now.tv_nsec).expect("nonnegative monotonic nanos"))
        })
        .expect("monotonic nanosecond overflow")
}

#[test]
fn session_clock_uses_the_process_monotonic_time_domain() {
    let before = monotonic_ns();
    let clock = SessionBenchClock::start();
    let observed = clock.elapsed_ns();
    let after = monotonic_ns();

    assert!(
        before <= observed,
        "runner clock predates its external boundary"
    );
    assert!(
        observed <= after,
        "runner clock escapes its external boundary"
    );
}
