//! Literal minimize/restore in one retained test child; the parent owns the bound.

use crate::support::process::{
    open_process_for_wait, pipe_text_by, process_times, read_pipe_on_thread,
    wait_for_process_signal,
};
use crate::support::signed_process::SignedStateProcessGuard;
use std::os::windows::io::AsRawHandle as _;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_MINIMIZE, SW_RESTORE, ShowWindow};

const INPUT: &str = "KELD_GH1188_RESTORE_INPUT";
const HELPER: &str = "support::window::native_control::native_restore::restore_child_helper";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShowState {
    Minimize,
    Restore,
}

impl ShowState {
    const fn name(self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Restore => "restore",
        }
    }

    const fn command(self) -> i32 {
        match self {
            Self::Minimize => SW_MINIMIZE,
            Self::Restore => SW_RESTORE,
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "minimize" => Some(Self::Minimize),
            "restore" => Some(Self::Restore),
            _ => None,
        }
    }
}

pub(super) fn restore(hwnd: usize, pid: u32, deadline: Instant) {
    show(hwnd, pid, deadline, ShowState::Restore);
}

pub(super) fn minimize(hwnd: usize, pid: u32, deadline: Instant) {
    show(hwnd, pid, deadline, ShowState::Minimize);
}

fn show(hwnd: usize, pid: u32, deadline: Instant, state: ShowState) {
    assert!(
        Instant::now() < deadline,
        "INVALID native stimulus: Restore deadline expired before child launch"
    );
    let target = open_process_for_wait(pid, false);
    let birth = process_times(target.as_raw_handle().cast()).creation;
    assert_eq!(
        wait_for_process_signal(&target, 0),
        WAIT_TIMEOUT,
        "INVALID native stimulus: Restore target already exited"
    );
    let input = format!("{hwnd}:{pid}:{birth}:{}", state.name());
    let mut helper = SignedStateProcessGuard::new(
        Command::new(std::env::current_exe().expect("current no_flag_windows executable"))
            .args([HELPER, "--exact", "--nocapture", "--test-threads=1"])
            .env(INPUT, &input)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn retained Restore child"),
    );
    let stdout = read_pipe_on_thread(helper.child_mut().stdout.take().expect("Restore stdout"));
    let stderr = read_pipe_on_thread(helper.child_mut().stderr.take().expect("Restore stderr"));
    let helper_birth = process_times(helper.child_mut().as_raw_handle().cast()).creation;
    let helper_process = open_process_for_wait(helper.host_pid(), false);
    assert_eq!(
        process_times(helper_process.as_raw_handle().cast()).creation,
        helper_birth,
        "Restore child process identity"
    );
    loop {
        assert!(
            Instant::now() < deadline,
            "INVALID native stimulus: Restore child exceeded the original deadline"
        );
        match wait_for_process_signal(&helper_process, 0) {
            WAIT_OBJECT_0 => break,
            WAIT_TIMEOUT => {}
            state => panic!("INVALID native stimulus: Restore child wait failed: {state}"),
        }
        // The comparator remains on this thread. Keep its existing queue alive
        // while the retained child performs the synchronous native call.
        super::pump();
        thread::park_timeout(
            Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    let status = helper.wait(deadline);
    let output = pipe_text_by(&stdout, deadline);
    let error = pipe_text_by(&stderr, deadline);
    assert!(
        Instant::now() < deadline && status.success(),
        "INVALID native stimulus: Restore child did not finish in-bound: {status}\n{output}\n{error}"
    );
    let marker = format!("KELD_GH1188_RESTORE_CHILD_RETURN {input}");
    assert_eq!(
        output.matches(&marker).count(),
        1,
        "INVALID native stimulus: missing exact Restore child completion\n{output}\n{error}"
    );
    assert_eq!(
        wait_for_process_signal(&target, 0),
        WAIT_TIMEOUT,
        "INVALID native stimulus: Restore target exited during the call"
    );
    // A returned call is not a state oracle. The caller keeps its original
    // IsIconic, foreground, frame, page visibility and child-focus assertions.
}

#[test]
fn restore_child_helper() {
    let Some(input) = std::env::var_os(INPUT) else {
        return;
    };
    let input = input.into_string().expect("Restore child input is UTF-8");
    let fields: Vec<_> = input.split(':').collect();
    assert_eq!(fields.len(), 4, "exact native show-state child input");
    let hwnd = fields[0].parse::<usize>().expect("Restore HWND");
    let pid = fields[1].parse::<u32>().expect("Restore target PID");
    let birth = fields[2].parse::<u64>().expect("Restore target birth");
    let state = ShowState::parse(fields[3]).expect("only literal minimize or restore is admitted");
    assert_ne!(pid, std::process::id(), "Restore targets the retained Host");
    let target = open_process_for_wait(pid, false);
    assert_eq!(
        process_times(target.as_raw_handle().cast()).creation,
        birth,
        "Restore target process identity changed"
    );
    assert_eq!(wait_for_process_signal(&target, 0), WAIT_TIMEOUT);
    let raw = super::target(hwnd, pid);
    // SAFETY: the parent retains the original Host while this child validates its
    // PID/birth and HWND. Only this child can block in either literal AC26 call.
    // BOOL reports prior visibility, not success; the parent observes real state.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-showwindow
    let previously_visible = unsafe { ShowWindow(raw, state.command()) };
    println!(
        "KELD_GH1188_RESTORE_CHILD_RETURN {input} previously_visible={}",
        previously_visible != 0
    );
}

#[test]
fn show_state_admits_only_literal_minimize_and_restore() {
    assert_eq!(ShowState::parse("minimize"), Some(ShowState::Minimize));
    assert_eq!(ShowState::parse("restore"), Some(ShowState::Restore));
    assert_eq!(ShowState::Minimize.command(), SW_MINIMIZE);
    assert_eq!(ShowState::Restore.command(), SW_RESTORE);
    for rejected in [
        "",
        "6",
        "9",
        "hide",
        "show",
        "maximize",
        "force-minimize",
        "restore ",
    ] {
        assert_eq!(ShowState::parse(rejected), None);
    }
}
