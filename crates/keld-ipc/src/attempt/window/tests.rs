//! The health window's exact edges on an injected clock (KEL-53 §4 *Health
//! sequence*; §7 "8 (health sequence)").
//!
//! Oracle: the spec sentence, written out as a table over `t`, the instant 30
//! seconds plus the margin after the owner read `KELD-AY1`. Every instant is
//! built from one base `Instant` by exact `Duration` offsets; no test waits.

use std::time::{Duration, Instant};

use super::{
    ATTEMPT_HEALTH_WINDOW, AttemptHealthWindowFailure, HealthWindow, WindowRead, WindowStep,
};

const NANO: Duration = Duration::from_nanos(1);
/// An arbitrary margin standing in for G, which S6c measures.
const MARGIN: Duration = Duration::from_millis(1_250);

/// One nanosecond before `t`, which is always 31.25 s after a live instant.
fn just_before(t: Instant) -> Instant {
    t.checked_sub(NANO)
        .expect("one nanosecond before a later instant")
}

/// `(ready, window, t)`: the window from a `KELD-AY1` read at `ready`, and its
/// end `t`, computed here from the spec's 30 seconds, not from the module.
fn window() -> (Instant, HealthWindow, Instant) {
    let ready = Instant::now();
    let t = ready + Duration::from_secs(30) + MARGIN;
    let window = HealthWindow::after_ready(ready, MARGIN).expect("a 31.25 s window fits the clock");
    (ready, window, t)
}

#[test]
fn the_fixed_part_of_the_window_is_thirty_seconds() {
    assert_eq!(ATTEMPT_HEALTH_WINDOW, Duration::from_secs(30));
}

#[test]
fn the_window_reads_until_exactly_its_end() {
    let (ready, window, t) = window();
    assert_eq!(window.begin(ready), WindowStep::Read { until: t });
    assert_eq!(window.begin(just_before(t)), WindowStep::Read { until: t });
}

/// "no further byte received": a byte refuses at every observed instant,
/// including one observed at or after the end.
#[test]
fn a_byte_refuses_health_whenever_it_is_observed() {
    let (ready, window, t) = window();
    for now in [ready, just_before(t), t, t + NANO] {
        assert_eq!(
            window.after_read(WindowRead::Byte, now),
            WindowStep::Refuse(AttemptHealthWindowFailure::ByteReceived),
            "{:?} after AY1",
            now - ready
        );
    }
}

/// "the connection open": end of file refuses, also one observed at the end.
#[test]
fn end_of_file_refuses_health_whenever_it_is_observed() {
    let (ready, window, t) = window();
    for now in [ready, just_before(t), t, t + NANO] {
        assert_eq!(
            window.after_read(WindowRead::EndOfFile, now),
            WindowStep::Refuse(AttemptHealthWindowFailure::EndOfFile),
            "{:?} after AY1",
            now - ready
        );
    }
}

/// A wait that wakes before the end, even by one nanosecond, reads again
/// until the same end; only a silent read that returns at or after the end
/// reaches the launch-handle check.
#[test]
fn a_silent_read_checks_the_launch_only_at_or_after_the_end() {
    let (ready, window, t) = window();
    for now in [
        ready,
        t.checked_sub(Duration::from_millis(1))
            .expect("the window ends 31 s after a live instant"),
        just_before(t),
    ] {
        assert_eq!(
            window.after_read(WindowRead::DeadlineElapsed, now),
            WindowStep::Read { until: t },
            "{:?} after AY1",
            now - ready
        );
    }
    for now in [t, t + NANO, t + Duration::from_secs(1)] {
        assert_eq!(
            window.after_read(WindowRead::DeadlineElapsed, now),
            WindowStep::CheckLaunch
        );
    }
    assert_eq!(window.begin(t), WindowStep::CheckLaunch);
}

/// "its launch handle is unsignaled": only an unsignaled handle at the end
/// accepts; a signaled or unreadable one refuses.
#[test]
fn only_an_unsignaled_launch_handle_at_the_end_accepts() {
    assert_eq!(HealthWindow::after_launch_check(Some(false)), Ok(()));
    assert_eq!(
        HealthWindow::after_launch_check(Some(true)),
        Err(AttemptHealthWindowFailure::LaunchExited)
    );
    assert_eq!(
        HealthWindow::after_launch_check(None),
        Err(AttemptHealthWindowFailure::LaunchUnverifiable)
    );
}

/// The margin moves the end: the same silent read at `30 s + MARGIN - 1 ns`
/// reads on with the margin and checks the launch with a zero margin, the G
/// = 0 case. A window that ignored the margin would accept an exit in the
/// last G.
#[test]
fn the_margin_extends_the_window_past_thirty_seconds() {
    let ready = Instant::now();
    let with_margin = HealthWindow::after_ready(ready, MARGIN).expect("fits the clock");
    let without = HealthWindow::after_ready(ready, Duration::ZERO).expect("fits the clock");
    let late = just_before(ready + Duration::from_secs(30) + MARGIN);
    assert_eq!(
        with_margin.after_read(WindowRead::DeadlineElapsed, late),
        WindowStep::Read { until: late + NANO }
    );
    assert_eq!(
        without.after_read(WindowRead::DeadlineElapsed, late),
        WindowStep::CheckLaunch
    );
}

/// The zero margin's window still lasts the spec's 30 seconds exactly.
#[test]
fn a_zero_margin_window_ends_thirty_seconds_after_ay1() {
    let ready = Instant::now();
    let t = ready + Duration::from_secs(30);
    let window = HealthWindow::after_ready(ready, Duration::ZERO).expect("fits the clock");
    assert_eq!(window.begin(just_before(t)), WindowStep::Read { until: t });
    assert_eq!(window.begin(t), WindowStep::CheckLaunch);
}

#[test]
fn a_window_whose_length_or_end_overflows_the_clock_is_refused() {
    assert_eq!(
        HealthWindow::after_ready(Instant::now(), Duration::MAX),
        Err(AttemptHealthWindowFailure::Unrepresentable)
    );
    assert_eq!(
        HealthWindow::of_length(Instant::now(), Duration::MAX),
        Err(AttemptHealthWindowFailure::Unrepresentable)
    );
}
