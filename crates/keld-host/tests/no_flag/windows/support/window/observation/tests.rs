//! Hidden native fixtures validate the observer; they are not KELD window acceptance.

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::ptr;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, MoveWindow, WS_CHILD, WS_OVERLAPPEDWINDOW,
};

use super::{ChildCensus, NativeWindowSnapshot, visit_child};

struct OwnedTestWindow(HWND);

impl OwnedTestWindow {
    fn new(parent: HWND) -> Self {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "Keld native observation control\0".encode_utf16().collect();
        let style = if parent.is_null() {
            WS_OVERLAPPEDWINDOW
        } else {
            WS_CHILD
        };
        // SAFETY: the built-in STATIC class needs no custom callback or instance.
        // Inputs are terminated UTF-16. All windows are hidden, on this test thread,
        // and the parent is another live OwnedTestWindow on the same thread.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-createwindowexw
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                style,
                20,
                20,
                320,
                240,
                parent,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        assert!(
            !hwnd.is_null(),
            "create hidden observer fixture: {}",
            std::io::Error::last_os_error()
        );
        Self(hwnd)
    }

    fn capture(&self) -> NativeWindowSnapshot {
        NativeWindowSnapshot::capture(self.0 as usize as u64, std::process::id())
            .expect("observe actual hidden fixture")
    }
}

impl Drop for OwnedTestWindow {
    fn drop(&mut self) {
        // SAFETY: this guard owns the window created on this same test thread.
        // Descendant guards are dropped before their parent, so no double teardown.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-destroywindow
        unsafe { DestroyWindow(self.0) };
    }
}

#[test]
fn native_window_observer_rejects_null_missing_and_foreign_parents() {
    assert_eq!(
        NativeWindowSnapshot::capture(0, std::process::id())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidInput
    );
    let root = OwnedTestWindow::new(ptr::null_mut());
    assert_eq!(
        NativeWindowSnapshot::capture(root.0 as usize as u64, 0)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidInput
    );
    // This field deliberately disagrees with the OS, not a request to act on a PID.
    let foreign = if std::process::id() == 1 { 2 } else { 1 };
    assert_eq!(
        NativeWindowSnapshot::capture(root.0 as usize as u64, foreign)
            .unwrap_err()
            .kind(),
        ErrorKind::PermissionDenied
    );
    let value = root.0 as usize as u64;
    drop(root);
    assert!(
        NativeWindowSnapshot::capture(value, std::process::id()).is_err(),
        "destroyed window must not become an empty successful census"
    );
}

#[test]
fn native_window_observer_sees_descendants_geometry_changes_and_teardown() {
    let root = OwnedTestWindow::new(ptr::null_mut());
    let child = OwnedTestWindow::new(root.0);
    let grandchild = OwnedTestWindow::new(child.0);
    let before = root.capture();
    let expected = BTreeSet::from([child.0 as usize, grandchild.0 as usize]);
    assert_eq!(
        before.children.iter().copied().collect::<BTreeSet<_>>(),
        expected,
        "native descendant set is incomplete"
    );
    assert_eq!(
        before.children.len(),
        expected.len(),
        "duplicate descendants are not hidden"
    );
    assert_eq!(
        before.live_handles().len(),
        3,
        "pre-teardown native liveness must be non-vacuous"
    );
    // SAFETY: this same thread owns the hidden fixture. This changes no foreground
    // window, desktop configuration or product behavior. MoveWindow completes on
    // the owning thread; subsequent geometry is an independent native observation.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-movewindow
    assert_ne!(unsafe { MoveWindow(root.0, 41, 63, 410, 290, 0) }, 0);
    let after = root.capture();
    assert_ne!(
        before.outer_rect, after.outer_rect,
        "a constant rectangle cannot report a real move"
    );
    assert!(after.outer_rect[2] > after.outer_rect[0] && after.outer_rect[3] > after.outer_rect[1]);
    drop(grandchild);
    drop(child);
    drop(root);
    assert!(
        before.live_handles().is_empty(),
        "previously live native windows survived fixture teardown: {before:?}"
    );
}

#[test]
fn native_window_observer_keeps_an_empty_valid_parent_distinct_from_a_missing_one() {
    let root = OwnedTestWindow::new(ptr::null_mut());
    let observed = root.capture();
    assert_eq!(observed.children, [] as [usize; 0]);
    assert_eq!(observed.live_handles(), vec![root.0 as usize]);
    drop(root);
    assert_eq!(observed.live_handles(), [] as [usize; 0]);
}

#[test]
fn native_window_observer_refuses_maximum_plus_one_instead_of_truncating() {
    const EXPECTED_MAX_CHILDREN: usize = 4096; // independent documented fixture bound
    // Callback-bound control only: numeric values are recorded, never queried or
    // used as windows. The native test above proves actual enumeration separately.
    let mut census = ChildCensus::default();
    for n in 1..=EXPECTED_MAX_CHILDREN {
        // SAFETY: valid exclusive census pointer with the callback's exact lifetime.
        assert_ne!(
            unsafe { visit_child(n as HWND, &raw mut census as isize) },
            0
        );
    }
    assert!(!census.overflow);
    assert_eq!(census.handles.len(), EXPECTED_MAX_CHILDREN);
    // SAFETY: same callback state; the extra value must not enter the output.
    assert_eq!(
        unsafe {
            visit_child(
                (EXPECTED_MAX_CHILDREN + 1) as HWND,
                &raw mut census as isize,
            )
        },
        0
    );
    assert!(census.overflow);
    assert_eq!(census.handles.len(), EXPECTED_MAX_CHILDREN);
}
