//! One ordinary caption click on the retained test-owned comparator.
//! Global input cannot be atomically addressed to an HWND. Immediate eligibility
//! checks and actual tagged delivery are required; partial input is always INVALID.

use std::ptr;
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetCapture, INPUT, INPUT_0, INPUT_MOUSE, IsWindowEnabled, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput, VK_LBUTTON,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GUITHREADINFO, GetCursorPos, GetForegroundWindow, GetGUIThreadInfo,
    GetMessageExtraInfo, GetSystemMetrics, GetWindowRect, GetWindowThreadProcessId, HTCAPTION,
    HWND_TOP, IsWindow, IsWindowVisible, MSG, PM_REMOVE, PeekMessageW, SM_CXPADDEDBORDER,
    SM_CYCAPTION, SM_CYFRAME, SM_SWAPBUTTON, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageW,
    SetCursorPos, SetWindowPos, TranslateMessage, WM_NCHITTEST, WM_NCLBUTTONDOWN, WindowFromPoint,
};

use super::FocusComparator;

// A scalar message tag, never a borrowed address or a new native registration.
const CLICK_TAG: usize = 0x4b45_4c44;

impl FocusComparator {
    pub(crate) fn click_caption(&self, deadline: Instant) {
        super::pump();
        raise_without_activation(self);
        let point = caption_point(self);
        require_caption(self, point);
        require_released_input();
        // SAFETY: ordinary cursor motion to the verified owned caption. No
        // clipping, desktop switch, attachment, foreground grant or policy edit.
        assert_ne!(
            unsafe { SetCursorPos(point.x, point.y) },
            0,
            "INVALID owned-caption cursor motion"
        );
        let mut actual = POINT::default();
        // SAFETY: synchronous read into initialized stack storage.
        assert_ne!(
            unsafe { GetCursorPos(&raw mut actual) },
            0,
            "cursor readback"
        );
        assert_eq!(
            (actual.x, actual.y),
            (point.x, point.y),
            "INVALID owned-caption cursor readback"
        );
        let input_size = i32::try_from(std::mem::size_of::<INPUT>()).expect("INPUT size");
        let inputs = [
            mouse_input(MOUSEEVENTF_MOVE),
            mouse_input(MOUSEEVENTF_LEFTDOWN),
            mouse_input(MOUSEEVENTF_LEFTUP),
        ];
        // Recheck identity, point ownership, caption hit and physical button
        // immediately before insertion. These checks are not an atomic lock on
        // the shared cursor/window state; actual tagged delivery remains required.
        require_caption(self, point);
        require_released_input();
        assert!(
            Instant::now() < deadline,
            "INVALID expired click preparation"
        );
        // SAFETY: the typed three-element stack array lives through this
        // synchronous copy. The zero-distance move keeps the checked cursor
        // point. Its scalar tag retains no pointer. Only this packet is inserted.
        let inserted = unsafe { SendInput(3, inputs.as_ptr(), input_size) };
        if inserted != 3 {
            if inserted != 0 {
                checked_release(input_size);
            }
            panic!("INVALID caption input: inserted={inserted}/3; no restore qualification");
        }
        if let Err(observed) = wait_for_caption_delivery(self, point, deadline) {
            checked_release(input_size);
            panic!("INVALID caption delivery: tagged_downs={observed}; no restore qualification");
        }
        println!(
            "KELD_GH1188_AC26_OWNED_CAPTION_CLICK helper={} pid={} point={},{} inserted=3 tagged_downs=1 released=true",
            self.hwnd(),
            std::process::id(),
            point.x,
            point.y
        );
    }
}

fn raise_without_activation(helper: &FocusComparator) {
    assert!(live_on_owner_thread(helper), "INVALID comparator identity");
    // SAFETY: numeric readback only; the previous foreground owner is not mutated.
    let foreground = unsafe { GetForegroundWindow() };
    // SAFETY: reorder only this guard's original same-thread HWND in the normal
    // z-order band. No move, size, topmost state or activation is requested.
    assert_ne!(
        unsafe {
            SetWindowPos(
                helper.0,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        },
        0,
        "INVALID owned-caption placement"
    );
    // SAFETY: readback cannot activate any HWND. A concurrent foreground change
    // also refuses this preparation, rather than manufacturing input eligibility.
    assert_eq!(
        unsafe { GetForegroundWindow() },
        foreground,
        "INVALID owned-caption placement changed foreground"
    );
}

fn live_on_owner_thread(helper: &FocusComparator) -> bool {
    let mut pid = 0;
    // SAFETY: read-only queries of the guard's original HWND. The borrowed guard
    // remains alive; PID and current thread must both still match its creator.
    unsafe {
        GetWindowThreadProcessId(helper.0, &raw mut pid) == GetCurrentThreadId()
            && pid == std::process::id()
            && IsWindow(helper.0) != 0
            && IsWindowVisible(helper.0) != 0
            && IsWindowEnabled(helper.0) != 0
    }
}

fn caption_point(helper: &FocusComparator) -> POINT {
    assert!(live_on_owner_thread(helper), "INVALID comparator identity");
    let mut rect = RECT::default();
    // SAFETY: the retained comparator was just matched to this process/thread;
    // the initialized rectangle is borrowed only for this synchronous query.
    assert_ne!(
        unsafe { GetWindowRect(helper.0, &raw mut rect) },
        0,
        "helper frame"
    );
    // SAFETY: DPI and metrics are read-only. The caller already holds the
    // existing PhysicalCoordinates guard; no process/monitor setting changes.
    let (dpi, frame, padding, caption) = unsafe {
        let dpi = GetDpiForWindow(helper.0);
        (
            dpi,
            GetSystemMetricsForDpi(SM_CYFRAME, dpi),
            GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi),
            GetSystemMetricsForDpi(SM_CYCAPTION, dpi),
        )
    };
    assert!(
        dpi > 0 && frame >= 0 && padding >= 0 && caption > 0,
        "caption metrics"
    );
    let width = rect.right.checked_sub(rect.left).expect("helper width");
    assert!(width > 0, "positive helper width");
    POINT {
        x: rect.left.checked_add(width / 3).expect("caption X"),
        y: rect
            .top
            .checked_add(frame)
            .and_then(|value| value.checked_add(padding))
            .and_then(|value| value.checked_add(caption / 2))
            .expect("caption Y"),
    }
}

fn require_caption(helper: &FocusComparator, point: POINT) {
    assert!(
        live_on_owner_thread(helper),
        "INVALID comparator identity changed"
    );
    let x = i16::try_from(point.x)
        .expect("caption X fits WM_NCHITTEST")
        .to_le_bytes();
    let y = i16::try_from(point.y)
        .expect("caption Y fits WM_NCHITTEST")
        .to_le_bytes();
    let packed = isize::try_from(i32::from_le_bytes([x[0], x[1], y[0], y[1]]))
        .expect("signed screen point fits LPARAM");
    // SAFETY: WindowFromPoint is read-only. The pointer-free hit test is sent
    // synchronously only to this retained same-thread built-in system window.
    let (under_point, hit) = unsafe {
        (
            WindowFromPoint(point),
            SendMessageW(helper.0, WM_NCHITTEST, 0, packed),
        )
    };
    assert_eq!(
        under_point, helper.0,
        "INVALID caption is occluded or foreign; point=({}, {}) native_hit={hit}",
        point.x, point.y
    );
    assert_eq!(
        hit,
        isize::try_from(HTCAPTION).expect("HTCAPTION"),
        "INVALID caption hit"
    );
}

fn require_released_input() {
    let mut foreground = GUITHREADINFO {
        cbSize: u32::try_from(std::mem::size_of::<GUITHREADINFO>()).expect("GUI info size"),
        ..GUITHREADINFO::default()
    };
    // SAFETY: read-only physical-button, mapping and capture queries. No existing
    // press/capture is canceled to manufacture an eligible click.
    let eligible = unsafe {
        GetAsyncKeyState(i32::from(VK_LBUTTON)) >= 0
            && GetSystemMetrics(SM_SWAPBUTTON) == 0
            && GetCapture().is_null()
            && GetGUIThreadInfo(0, &raw mut foreground) != 0
            && foreground.hwndCapture.is_null()
    };
    assert!(
        eligible,
        "INVALID caption input: button down, swapped, capture or unreadable GUI"
    );
}

fn mouse_input(flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: CLICK_TAG,
            },
        },
    }
}

fn checked_release(input_size: i32) {
    let release = mouse_input(MOUSEEVENTF_LEFTUP);
    // SAFETY: one immediate release after a possibly inserted down. The typed
    // stack value lives through the call; there is no retry or semantic credit.
    let inserted = unsafe { SendInput(1, &raw const release, input_size) };
    assert_eq!(
        inserted, 1,
        "INVALID caption input: left-up cleanup insertion failed"
    );
    super::pump();
}

fn wait_for_caption_delivery(
    helper: &FocusComparator,
    point: POINT,
    deadline: Instant,
) -> Result<(), u32> {
    let mut observed = 0_u32;
    loop {
        if Instant::now() >= deadline {
            return Err(observed);
        }
        let mut message = MSG::default();
        // SAFETY: drain the existing test-thread queue, never a different
        // thread's queue. Every retrieved message gets its normal dispatch.
        if unsafe { PeekMessageW(&raw mut message, ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if message.hwnd == helper.0
                && message.message == WM_NCLBUTTONDOWN
                && message.wParam == usize::try_from(HTCAPTION).expect("HTCAPTION")
                && message_point(message.lParam) == (point.x, point.y)
                // SAFETY: scalar extra data of the just-retrieved message,
                // inspected before any dispatch can replace that association.
                && usize::try_from(unsafe { GetMessageExtraInfo() }).ok() == Some(CLICK_TAG)
            {
                observed = observed.saturating_add(1);
            }
            // SAFETY: same ordinary dispatch as the existing pump; no WndProc,
            // fake sent-message acknowledgement, hook, swallowed or reordered event.
            unsafe {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
            continue;
        }
        if observed > 1 || !live_on_owner_thread(helper) {
            return Err(observed);
        }
        // A caption down can enter the system move loop and consume up there.
        // Require returned dispatch, an empty outer queue, released button/no
        // capture and actual helper foreground; a queued click is insufficient.
        // SAFETY: read-only current-thread and foreground numeric state.
        let settled = unsafe {
            GetAsyncKeyState(i32::from(VK_LBUTTON)) >= 0
                && GetCapture().is_null()
                && GetForegroundWindow() == helper.0
        };
        if observed == 1 && settled {
            return Ok(());
        }
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn message_point(value: isize) -> (i32, i32) {
    let bytes = value.to_le_bytes();
    (
        i32::from(i16::from_le_bytes([bytes[0], bytes[1]])),
        i32::from(i16::from_le_bytes([bytes[2], bytes[3]])),
    )
}
