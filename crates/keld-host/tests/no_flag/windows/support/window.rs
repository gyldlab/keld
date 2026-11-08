//! Independent native HWND and window-title observation.

use crate::PRODUCT_TITLE;
use serde_json::Value;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub(crate) use census::app_top_level_windows;

/// The per-PID top-level window census. Its consumers are the test-admitted facade
/// cells until product admission (GH-1188 T2).
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod census {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
    };

    /// One top-level window of a process, hidden or not.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct TopLevelWindow {
        pub(crate) hwnd: isize,
        pub(crate) class: String,
        pub(crate) visible: bool,
    }

    /// `EnumWindows` state: the process under census and what it owns.
    struct Census {
        pid: u32,
        windows: Vec<TopLevelWindow>,
    }

    /// Every top-level window of `pid`, by `EnumWindows` and `GetWindowThreadProcessId`,
    /// so a hidden window cannot read as absent (`MainWindowHandle == 0` proves nothing).
    pub(crate) fn top_level_windows(pid: u32) -> Vec<TopLevelWindow> {
        unsafe extern "system" fn visit(hwnd: HWND, state: LPARAM) -> i32 {
            // SAFETY: `state` is the address of the `Census` that `top_level_windows` keeps
            // exclusively borrowed across this synchronous `EnumWindows` call.
            let census = unsafe { &mut *(state as *mut Census) };
            let mut owner = 0_u32;
            // SAFETY: `hwnd` is the handle Windows passed this callback; `owner` outlives
            // the synchronous call.
            unsafe { GetWindowThreadProcessId(hwnd, &raw mut owner) };
            if owner == census.pid {
                let mut class = [0_u16; 256];
                // SAFETY: the buffer and its length are this local array's; a window that
                // vanished only yields 0 characters.
                let copied = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), 256) };
                let copied = usize::try_from(copied).unwrap_or(0);
                census.windows.push(TopLevelWindow {
                    hwnd: hwnd as isize,
                    class: String::from_utf16_lossy(&class[..copied]),
                    // SAFETY: a stale handle only reads as not visible.
                    visible: unsafe { IsWindowVisible(hwnd) } != 0,
                });
            }
            TRUE
        }
        let mut census = Census {
            pid,
            windows: Vec::new(),
        };
        // SAFETY: `visit` matches `WNDENUMPROC`; the `Census` address stays valid and is
        // used only by `visit` for the duration of this synchronous call.
        let enumerated = unsafe { EnumWindows(Some(visit), &raw mut census as LPARAM) };
        assert_ne!(
            enumerated,
            0,
            "EnumWindows: {}",
            std::io::Error::last_os_error()
        );
        census.windows
    }

    /// The top-level windows a live host owns with no application window: Tao's thread
    /// event target, which `IsWindowVisible` reports visible, and the default IME window
    /// (read on a zero-window host, Windows 11 26300, tao 0.35.3). Any other class counts
    /// as an application window, visible or hidden.
    const HOST_INFRASTRUCTURE_WINDOW_CLASSES: [&str; 2] = ["Tao Thread Event Target", "IME"];

    /// [`top_level_windows`] without the host's own infrastructure windows: the census an
    /// application window, shown or hidden, must appear in.
    pub(crate) fn app_top_level_windows(pid: u32) -> Vec<TopLevelWindow> {
        top_level_windows(pid)
            .into_iter()
            .filter(|window| !HOST_INFRASTRUCTURE_WINDOW_CLASSES.contains(&window.class.as_str()))
            .collect()
    }
}

pub(crate) fn wait_for_host_window(pid: u32, deadline: Instant) -> Value {
    loop {
        let script = format!(
            "$p=Get-Process -Id {pid} -ErrorAction Stop; [pscustomobject]@{{handle=[uint64]$p.MainWindowHandle.ToInt64();title=$p.MainWindowTitle}} | ConvertTo-Json -Compress"
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .expect("query host window");
        if output.status.success() {
            let observation: Value =
                serde_json::from_slice(&output.stdout).expect("host window observation JSON");
            if observation["handle"]
                .as_u64()
                .is_some_and(|handle| handle != 0)
                && observation["title"] == PRODUCT_TITLE
            {
                return observation;
            }
        }
        assert!(
            Instant::now() < deadline,
            "host HWND/title observation timed out"
        );
        thread::park_timeout(Duration::from_millis(20));
    }
}
