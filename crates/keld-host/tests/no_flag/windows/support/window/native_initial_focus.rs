//! Read the Host UI thread directly for independent initial activation evidence.

use super::NativeWindowSnapshot;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GUITHREADINFO, GetGUIThreadInfo, GetWindowThreadProcessId, IsChild,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct ThreadFocus {
    pub(crate) thread: u32,
    pub(crate) active: usize,
    pub(crate) focus: usize,
    pub(crate) focus_is_child: bool,
}

fn owner(hwnd: usize, pid: u32) -> (HWND, u32) {
    NativeWindowSnapshot::capture(hwnd as u64, pid).expect("retained fixture window");
    let mut actual = 0;
    // SAFETY: read the retained fixture HWND's creating thread and process only.
    let thread = unsafe { GetWindowThreadProcessId(hwnd as HWND, &raw mut actual) };
    assert_ne!(thread, 0, "fixture UI thread exists");
    assert_eq!(
        actual, pid,
        "fixture UI thread belongs to the retained Host"
    );
    (hwnd as HWND, thread)
}

pub(crate) fn read(hwnd: usize, pid: u32) -> ThreadFocus {
    let (raw, thread) = owner(hwnd, pid);
    let mut gui = GUITHREADINFO {
        cbSize: u32::try_from(std::mem::size_of::<GUITHREADINFO>()).expect("GUI info size"),
        ..GUITHREADINFO::default()
    };
    // SAFETY: read the Host's actual UI queue, not the foreground thread (0).
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getguithreadinfo
    assert_ne!(
        unsafe { GetGUIThreadInfo(thread, &raw mut gui) },
        0,
        "Host GUI thread state"
    );
    ThreadFocus {
        thread,
        active: gui.hwndActive as usize,
        focus: gui.hwndFocus as usize,
        // SAFETY: read-only relationship query between numeric native handles.
        focus_is_child: unsafe { IsChild(raw, gui.hwndFocus) } != 0,
    }
}
