//! Window message-pump behavior under browser exit and launch deadlines.
//! Tests exercise actual Windows message queue and thread CPU accounting.

use super::super::{LaunchCallback, WebView2LaunchPhase};
use super::{
    ProfileReleaseWait, ReleaseSite, alive_process_handle, exited_process_handle, release_channel,
    wait_for_browser_exit, wait_for_launch_with_message_pump_until,
};

/// The probe's pump driver wakes on the process handle: a signaled handle
/// with an empty receiver and a zero grace fails at once, attributed to
/// the probe site, instead of spinning or waiting for a message.
#[test]
fn exit_wait_pump_fails_promptly_for_a_signaled_process_without_an_event() {
    let (_sender, receiver) = release_channel();
    let release = ProfileReleaseWait::with_bounds(
        Some(receiver),
        Some(exited_process_handle()),
        ReleaseSite::Probe,
        std::time::Duration::ZERO,
        std::time::Duration::ZERO,
    );
    let error = wait_for_browser_exit(&release)
        .expect_err("a gone probe browser without its event must fail");
    assert!(
        error
            .to_string()
            .contains("the exclusive-UDF probe browser process exited without raising"),
        "{error}"
    );
}

/// This thread's kernel plus user CPU time from the scheduler's own
/// accounting, so the oracle is independent of how the pump is written.
fn thread_cpu_time() -> std::time::Duration {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: `GetCurrentThread` returns the caller's non-owning pseudo
    // handle; all four FILETIME outputs live for the synchronous query.
    // Contract:
    // https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getthreadtimes
    unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    }
    .expect("query this thread's times");
    let ticks =
        |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    std::time::Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}

/// Once the grace is armed on an exited process, the pump must block on
/// messages or the remaining grace alone. A signaled handle in the wait
/// set returns at once, which spun one core for the whole grace (KEL-135
/// review). Oracle: the scheduler's CPU accounting for this thread
/// against the wall time the wait took.
#[test]
fn exit_wait_pump_idles_through_the_grace_after_the_process_exited() {
    let grace = std::time::Duration::from_millis(400);
    let (_sender, receiver) = release_channel();
    // A zero hang guard: the exited handle must be judged by the grace
    // alone, so the fault below must be the grace, never the guard.
    let release = ProfileReleaseWait::with_bounds(
        Some(receiver),
        Some(exited_process_handle()),
        ReleaseSite::Host,
        grace,
        std::time::Duration::ZERO,
    );
    let cpu_before = thread_cpu_time();
    let started = std::time::Instant::now();
    let error = wait_for_browser_exit(&release)
        .expect_err("a gone browser without its event fails after the grace");
    let wall = started.elapsed();
    let cpu = thread_cpu_time().saturating_sub(cpu_before);
    println!(
        "KELD_WV_GRACE_WAIT cpu_ms={} wall_ms={}",
        cpu.as_millis(),
        wall.as_millis()
    );
    assert!(
        error
            .to_string()
            .contains("the host browser process exited without raising"),
        "{error}"
    );
    assert!(
        wall >= grace,
        "the grace must elapse before the fault: wall={wall:?}"
    );
    assert!(
        cpu * 4 < wall,
        "the grace wait must idle, not spin: cpu={cpu:?} wall={wall:?}"
    );
}

/// With a live process the pump has no timer: it keeps pumping unrelated
/// messages until the event arrives from another thread with a wake.
#[test]
fn exit_wait_pump_keeps_pumping_while_the_process_is_alive_until_the_event() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_NULL,
    };

    let mut message = MSG::default();
    // SAFETY: the no-remove query creates this test thread's message queue;
    // the output is live. Contract:
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-peekmessagew
    let _ = unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE) };
    // SAFETY: a current-thread id query has no preconditions.
    let thread_id = unsafe { GetCurrentThreadId() };
    let (sender, receiver) = release_channel();
    let release = ProfileReleaseWait::with_bounds(
        Some(receiver),
        Some(alive_process_handle()),
        ReleaseSite::Host,
        std::time::Duration::ZERO,
        std::time::Duration::from_hours(1),
    );
    let worker = std::thread::spawn(move || {
        // SAFETY: both posts target this test's own live queue with
        // pointer-free messages; the first is unrelated input the pump
        // must survive, the second is the wake that follows the event.
        // Contract: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-postthreadmessagew
        unsafe { PostThreadMessageW(thread_id, WM_APP, WPARAM(0), LPARAM(0)) }
            .expect("queue unrelated message");
        sender.send(Ok(())).expect("deliver the release event");
        // SAFETY: as above.
        unsafe { PostThreadMessageW(thread_id, WM_NULL, WPARAM(0), LPARAM(0)) }
            .expect("queue the wake");
    });
    wait_for_browser_exit(&release).expect("a live browser's event must be observed");
    worker.join().expect("worker thread");
    assert!(release.observed.get());
    assert_eq!(
        release.post_exit_deadline.get(),
        None,
        "the live process never armed a grace"
    );
}

/// The launch pump keeps the absolute bound: a ready result arriving at
/// or after the launch deadline loses to it. This bounds launches only;
/// browser shutdown never uses this pump.
#[test]
fn expired_launch_pump_rejects_ready_result_before_queued_messages() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, WM_NULL,
    };

    let mut message = MSG::default();
    // SAFETY: the no-remove query creates this test thread's message queue;
    // the output is live. Contract:
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-peekmessagew
    let _ = unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE) };
    // SAFETY: posts inert WM_NULL to the current test thread's live queue.
    // Contract: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-postthreadmessagew
    unsafe { PostThreadMessageW(GetCurrentThreadId(), WM_NULL, WPARAM(0), LPARAM(0)) }
        .expect("queue inert message");
    let (sender, receiver) = std::sync::mpsc::channel();
    sender
        .send(LaunchCallback::captured(7_u8))
        .expect("queue ready result");
    let started = std::time::Instant::now();
    let error = wait_for_launch_with_message_pump_until(
        &receiver,
        started,
        WebView2LaunchPhase::Controller,
        started,
        started,
    )
    .expect_err("expired launch work must lose to the absolute deadline");
    assert!(
        error.to_string().contains("launch deadline expired"),
        "{error}"
    );
    // SAFETY: removes only this test's inert WM_NULL records without
    // dispatch. Contract: PeekMessageW above.
    while unsafe { PeekMessageW(&raw mut message, None, WM_NULL, WM_NULL, PM_REMOVE) }.as_bool() {}
}

/// Failure-first GH-814 observation: an expired launch still fails WV-009,
/// but the real emitted diagnostic must identify whether the callback channel
/// had an entry at the time the deadline was observed. This catches a lost
/// source-level observation even when the typed refusal remains correct.
#[test]
fn expired_launch_reports_callback_channel_without_changing_refusal() {
    let output = std::process::Command::new(
        std::env::current_exe().expect("current keld-wv test executable"),
    )
    .args([
        "--exact",
        "webview2::tests::gh814_launch_timeout_log_child",
        "--nocapture",
    ])
    .env("KELD_GH814_LAUNCH_LOG_PHASE", "controller")
    .output()
    .expect("run real expired-launch diagnostic in a subprocess");
    assert!(
        output.status.success(),
        "original WV-009 must survive: {output:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("bounded ASCII diagnostic");
    let emitted = stderr
        .lines()
        .find(|line| line.contains("KELD_GH814_WEBVIEW2_LAUNCH_TIMEOUT"))
        .expect("actual failure-side diagnostic");
    assert!(
        emitted.contains("callback_channel_state=not_observed"),
        "missing callback-channel state must be explicit: {emitted}"
    );
    assert!(
        emitted.contains("pump_observed_ms="),
        "the deadline observation must use its phase monotonic clock: {emitted}"
    );
}

/// Synthetic monotonic enqueue timestamps independently falsify attribution:
/// an already queued pre-deadline completion must be distinguished from one
/// whose callback only ran after the deadline. Both MUST still refuse WV-009.
/// These synthetic timestamps do not assert when `WebView2` posted an OS event.
#[test]
fn expired_launch_distinguishes_callback_enqueue_order_without_accepting_late_ready() {
    for (case, expected_state, expected_thread) in [
        ("before", "captured_before_deadline", "matches_pump"),
        ("after", "captured_at_or_after_deadline", "matches_pump"),
        (
            "foreign_before",
            "captured_before_deadline",
            "different_thread",
        ),
        ("disconnected", "sender_disconnected", "not_observed"),
    ] {
        let output = std::process::Command::new(
            std::env::current_exe().expect("current keld-wv test executable"),
        )
        .args([
            "--exact",
            "webview2::tests::gh814_launch_timeout_log_child",
            "--nocapture",
        ])
        .env("KELD_GH814_LAUNCH_LOG_PHASE", "controller")
        .env("KELD_GH814_CALLBACK_QUEUE_CASE", case)
        .output()
        .expect("run exact expired-launch callback case in its own process");
        assert!(
            output.status.success(),
            "late callback must not turn WV-009 into success: {case} {output:?}"
        );
        let stderr = String::from_utf8(output.stderr).expect("bounded ASCII diagnostic");
        let emitted = stderr
            .lines()
            .find(|line| line.contains("KELD_GH814_WEBVIEW2_LAUNCH_TIMEOUT"))
            .expect("callback deadline evidence must be emitted");
        assert!(
            emitted.contains(&format!("callback_channel_state={expected_state}")),
            "{case}: {emitted}"
        );
        assert!(
            emitted.contains(&format!("callback_thread_relation={expected_thread}")),
            "{case}: callback thread must be compared with the actual pump thread: {emitted}"
        );
        assert!(
            emitted.contains("callback_captured_ms=") && emitted.contains("pump_observed_ms="),
            "{case}: missing monotonic phase-only timestamps: {emitted}"
        );
        assert!(
            !emitted.contains("http")
                && !emitted.contains("profile=")
                && !emitted.contains("pid=")
                && !emitted.contains("ThreadId("),
            "diagnostics may not disclose URLs, profiles, PIDs or raw thread ids: {emitted}"
        );
    }
}

/// Both a fast-returning COM launch and one that consumed the whole deadline
/// must remain WV-009 on expiry. A fixed emitted number or wrong boundary
/// comparison must fail this independently timed test-owned control.
#[test]
fn expired_launch_distinguishes_synchronous_call_time_from_callback_delay() {
    for (case, returned_ms, deadline_state) in [
        ("before", "100", "returned_before_deadline"),
        ("at", "500", "returned_at_or_after_deadline"),
        ("after", "600", "returned_at_or_after_deadline"),
    ] {
        let output = std::process::Command::new(
            std::env::current_exe().expect("current keld-wv test executable"),
        )
        .args([
            "--exact",
            "webview2::tests::gh814_launch_timeout_log_child",
            "--nocapture",
        ])
        .env("KELD_GH814_LAUNCH_LOG_PHASE", "controller")
        .env("KELD_GH814_CALLBACK_QUEUE_CASE", "before")
        .env("KELD_GH814_LAUNCH_CALL_CASE", case)
        .output()
        .expect("run exact synchronous-call diagnostic child");
        assert!(
            output.status.success(),
            "WV-009 refusal must survive: {output:?}"
        );
        let stderr = String::from_utf8(output.stderr).expect("bounded ASCII diagnostic");
        let emitted = stderr
            .lines()
            .find(|line| line.contains("KELD_GH814_WEBVIEW2_LAUNCH_TIMEOUT"))
            .expect("real launch timeout diagnostic");
        assert!(
            emitted.contains(&format!("launch_call_returned_ms={returned_ms}"))
                && emitted.contains(&format!("launch_call_deadline_state={deadline_state}")),
            "{case}: synchronous return must be distinguished from callback capture: {emitted}"
        );
        assert!(
            emitted.contains("callback_channel_state=captured_before_deadline")
                && emitted.contains("callback_captured_ms=250")
                && emitted.contains("callback_thread_relation=matches_pump"),
            "{case}: independent callback evidence must survive: {emitted}"
        );
        assert!(
            !emitted.contains("ThreadId(")
                && !emitted.contains("profile=")
                && !emitted.contains("pid="),
            "GH814 diagnostics must remain redacted: {emitted}"
        );
    }
}
