//! Ignored real-OS adapter controls. These do not claim Core/W2 acceptance.
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::expect_used)] // Assertion-only integration fixture; callbacks contain unwinds.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    BM_CLICK, CreateWindowExW, DestroyWindow, EnumChildWindows, EnumThreadWindows, GW_OWNER,
    GetClassNameW, GetDlgItem, GetWindow, GetWindowTextW, IDCANCEL, IDOK, IsWindow,
    IsWindowVisible, KillTimer, SendMessageW, SetTimer, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};
use windows::core::{BOOL, w};

use keld_native::dialog::windows::{WindowsDialogOptions, show};
use keld_native::dialog::{
    DialogReplyV1, MessageKind, MessageOptions, OpenDialogOptions, SaveDialogOptions,
};

struct Parent(Rc<Cell<Option<HWND>>>);
impl Parent {
    #[allow(unsafe_code)] // Test-only fixture creation in the nested allowance.
    fn new() -> Self {
        // SAFETY: this test thread creates/retains its own STA and visible built-in
        // STATIC window. No foreign HWND or application process is modified.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .expect("test STA");
            Self(Rc::new(Cell::new(Some(
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!("Keld native dialog acceptance"),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    200,
                    200,
                    560,
                    380,
                    None,
                    None,
                    None,
                    None,
                )
                .expect("owned visible parent"),
            ))))
        }
    }
}
impl Parent {
    fn hwnd(&self) -> HWND {
        self.0.get().expect("retained test parent")
    }
}
impl Drop for Parent {
    #[allow(unsafe_code)] // Same test-owned parent and apartment.
    fn drop(&mut self) {
        // SAFETY: retained own window on its creating thread; a parent-destruction
        // control may already have destroyed it. Balance only this test's COM init.
        unsafe {
            if let Some(parent) = self.0.take() {
                DestroyWindow(parent).expect("destroy owned test parent");
            }
            CoUninitialize();
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Cancel,
    UserCancel,
    Select,
    Message { response: i32, initial: bool },
    MessageUntoggled { response: i32 },
    DestroyParent,
}

struct Probe {
    parent: HWND,
    parent_owner: Rc<Cell<Option<HWND>>>,
    action: Action,
    cancel: Arc<AtomicBool>,
    started: Instant,
    id: Cell<usize>,
    seen: Cell<Option<HWND>>,
    visible_at: Cell<Option<Instant>>,
    acted: Cell<bool>,
    checkbox: Cell<Option<bool>>,
    verification: RefCell<Option<Verification>>,
    failed: Cell<bool>,
}
impl Probe {
    fn poll_verification(&self) -> bool {
        if self.acted.get() {
            return true;
        }
        let after = self
            .verification
            .borrow_mut()
            .as_mut()
            .expect("UIA observer was actually started")
            .poll();
        let Some(after) = after else { return false };
        self.checkbox.set(Some(after));
        true
    }
}

thread_local! {
    static PROBE: RefCell<Option<Rc<Probe>>> = const { RefCell::new(None) };
}
fn probe() -> Option<Rc<Probe>> {
    PROBE
        .try_with(|slot| slot.try_borrow().ok().and_then(|slot| slot.clone()))
        .ok()
        .flatten()
}

struct Observer(Rc<Probe>);
impl Observer {
    #[allow(unsafe_code)] // A test-owned timer, not a production cancellation policy.
    fn start(parent: &Parent, action: Action, cancel: Arc<AtomicBool>) -> Self {
        let state = Rc::new(Probe {
            parent: parent.hwnd(),
            parent_owner: Rc::clone(&parent.0),
            action,
            cancel,
            started: Instant::now(),
            id: Cell::new(0),
            seen: Cell::new(None),
            visible_at: Cell::new(None),
            acted: Cell::new(false),
            checkbox: Cell::new(None),
            verification: RefCell::new(None),
            failed: Cell::new(false),
        });
        PROBE.set(Some(Rc::clone(&state)));
        // SAFETY: current-STA timer, lookup-only callback, retained until Drop.
        let id = unsafe { SetTimer(None, 0, 40, Some(observe)) };
        assert_ne!(id, 0);
        state.id.set(id);
        Self(state)
    }

    #[allow(unsafe_code)] // Independent actual HWND disappearance query.
    fn drained(&self) {
        assert!(
            !self.0.failed.get(),
            "native observer timed out or panicked: seen={:?} acted={} owner={:?} elapsed_ms={}",
            self.0.seen.get(),
            self.0.acted.get(),
            self.0.parent_owner.get(),
            self.0.started.elapsed().as_millis()
        );
        assert!(
            self.0.acted.get(),
            "intended native control action never occurred"
        );
        let panel = self
            .0
            .seen
            .get()
            .expect("observer saw an actual visible owned panel");
        // SAFETY: observation of an opaque handle only, no pointer adoption.
        assert!(
            !unsafe { IsWindow(Some(panel)).as_bool() },
            "actual panel HWND survived Show"
        );
    }
}
impl Drop for Observer {
    #[allow(unsafe_code)] // Only the timer allocated by start on this thread.
    fn drop(&mut self) {
        // SAFETY: exact current-thread timer registration; queued callbacks use
        // a fallible TLS lookup and cannot access freed storage after removal.
        unsafe { KillTimer(None, self.0.id.get()) }.expect("observer timer removed");
        PROBE.set(None);
        // Reap the exact UIA child before this test can create any later panel.
        self.0.verification.borrow_mut().take();
    }
}

#[allow(unsafe_code)] // Current-thread test enumeration and bounded cleanup control.
unsafe extern "system" fn observe(_: HWND, _: u32, id: usize, _: u32) {
    let Some(state) = probe().filter(|state| state.id.get() == id) else {
        return;
    };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if state.started.elapsed() > Duration::from_secs(10) {
            state.failed.set(true);
            state.cancel.store(true, Ordering::Release);
            if let Some(client) = state.verification.borrow_mut().as_mut() {
                // Never join a live UIA client while this STA owns Show.
                if let Err(error) = client.request_stop() {
                    eprintln!("KELD_UIA_STOP_FAILED: {error}");
                }
            }
            if state.started.elapsed() > Duration::from_secs(12)
                && let Some(parent) = state.parent_owner.get()
            {
                // SAFETY: last-resort cleanup of this exact owned fixture parent
                // only. Failure remains RED; successful destruction retires custody.
                if unsafe { DestroyWindow(parent) }.is_ok() {
                    state.parent_owner.set(None);
                }
            }
        }
        // SAFETY: enumerate only this fixture STA; callback has no raw Rust data.
        assert!(unsafe {
            EnumThreadWindows(GetCurrentThreadId(), Some(window), LPARAM(0)).as_bool()
        });
    }))
    .is_err()
    {
        state.failed.set(true);
        state.cancel.store(true, Ordering::Release);
    }
}

#[allow(unsafe_code)] // Test-owned parent/child observation and actual control clicks.
unsafe extern "system" fn window(hwnd: HWND, _: LPARAM) -> BOOL {
    let Some(state) = probe() else { return BOOL(1) };
    let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: opaque native queries. Act only on a visible window owned by
        // our exact retained fixture parent; unrelated desktop windows are skipped.
        if state.parent_owner.get() != Some(state.parent)
            || unsafe { GetWindow(hwnd, GW_OWNER) }.ok() != Some(state.parent)
            || !unsafe { IsWindowVisible(hwnd).as_bool() }
        {
            return;
        }
        if state.failed.get() {
            // SAFETY: this fresh current-thread enumeration rechecks ownership
            // against our still-retained parent before independent native Cancel.
            // A historical numeric HWND observation grants no cleanup authority.
            let button = unsafe { GetDlgItem(Some(hwnd), IDCANCEL.0) }
                .or_else(|_| unsafe { GetDlgItem(Some(hwnd), 102) });
            if let Ok(button) = button {
                unsafe { SendMessageW(button, BM_CLICK, None, None) };
            }
            return;
        }
        if state.seen.get().is_none() {
            state.seen.set(Some(hwnd));
            state.visible_at.set(Some(Instant::now()));
        }
        if state.seen.get() != Some(hwnd) || state.acted.get() {
            return;
        }
        if state
            .visible_at
            .get()
            .is_none_or(|at| at.elapsed() < Duration::from_millis(400))
        {
            return;
        }
        match state.action {
            Action::Cancel => {
                state.acted.set(true);
                state.cancel.store(true, Ordering::Release);
            }
            Action::DestroyParent => {
                state.acted.set(true);
                // SAFETY: exact test-created parent on the creating thread.
                unsafe { DestroyWindow(state.parent) }.expect("destroy current parent");
                state.parent_owner.set(None);
            }
            Action::Select | Action::UserCancel => {
                let id = if matches!(state.action, Action::UserCancel) {
                    IDCANCEL.0
                } else {
                    IDOK.0
                };
                // SAFETY: resolve and click an actual control in our own panel;
                // Cancel exercises the OS result, without setting Host cancellation.
                if let Ok(button) = unsafe { GetDlgItem(Some(hwnd), id) } {
                    state.acted.set(true);
                    unsafe { SendMessageW(button, BM_CLICK, None, None) };
                }
            }
            Action::Message { .. } | Action::MessageUntoggled { .. } => {
                if let Action::Message { initial, .. } = state.action
                    && state.checkbox.get().is_none()
                {
                    if state.verification.borrow().is_none() {
                        let client = Verification::start(hwnd, initial);
                        *state.verification.borrow_mut() = Some(client);
                    }
                    if !state.poll_verification() {
                        return;
                    }
                }
                // SAFETY: fresh visible current-STA exact-parent panel; actual
                // nested native Button labels, no synthesized TDM response.
                let _ =
                    unsafe { EnumChildWindows(Some(hwnd), Some(click_message_button), LPARAM(0)) };
            }
        }
    }));
    if observed.is_err() {
        state.failed.set(true);
        state.cancel.store(true, Ordering::Release);
    }
    BOOL(1)
}

#[allow(unsafe_code)] // Actual button descendant of the freshly owned panel.
unsafe extern "system" fn click_message_button(hwnd: HWND, _: LPARAM) -> BOOL {
    let Some(state) = probe() else { return BOOL(1) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if state.acted.get() {
            return;
        }
        let (Action::Message { response, .. } | Action::MessageUntoggled { response }) =
            state.action
        else {
            return;
        };
        let mut class = [0_u16; 16];
        // SAFETY: these read-only queries address a current actual child from
        // EnumChildWindows, whose parent/STA custody was checked immediately.
        let count = unsafe { GetClassNameW(hwnd, &mut class) };
        let length = usize::try_from(count).unwrap_or(0).min(class.len());
        if String::from_utf16_lossy(&class[..length]) != "Button" {
            return;
        }
        let mut label = [0_u16; 128];
        // SAFETY: bounded buffer for a real fixture-owned native Button.
        let length = unsafe { GetWindowTextW(hwnd, &mut label) };
        let length = usize::try_from(length).unwrap_or(0).min(label.len());
        let label = String::from_utf16_lossy(&label[..length]);
        let expected = match response {
            0 => "&Save",
            1 => "&Discard Changes",
            2 => "Cancel",
            _ => return,
        };
        if label != expected || !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return;
        }
        state.acted.set(true);
        // SAFETY: exactly the real declared custom button. No state borrow is
        // held across this synchronous reentrant native control action.
        unsafe { SendMessageW(hwnd, BM_CLICK, None, None) };
    }));
    if result.is_err() {
        state.failed.set(true);
        state.cancel.store(true, Ordering::Release);
    }
    BOOL(1)
}

// An owned out-of-process UIA client keeps provider calls off this test's STA.
// It toggles and reads the virtual checkbox. The STA then clicks the actual
// Button; no live child is joined while Show is running.
struct Verification {
    child: Child,
    panel: isize,
    initial: bool,
}
impl Verification {
    fn start(panel: HWND, initial: bool) -> Self {
        let panel = panel.0 as isize;
        let child = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-MTA", "-Command"])
            .arg(include_str!("verification.ps1"))
            .env("KELD_TEST_DIALOG_PROCESS", std::process::id().to_string())
            .env("KELD_TEST_DIALOG_WINDOW", panel.to_string())
            .env("KELD_TEST_DIALOG_INITIAL", if initial { "1" } else { "0" })
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("owned UIA observer process");
        Self {
            child,
            panel,
            initial,
        }
    }

    fn request_stop(&mut self) -> std::io::Result<()> {
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        Ok(())
    }

    fn poll(&mut self) -> Option<bool> {
        // Only read/reap after exit, so the STA remains free for provider requests.
        let status = self.child.try_wait().expect("query owned UIA child")?;
        let mut stdout = String::new();
        self.child
            .stdout
            .take()
            .expect("UIA stdout")
            .take(8193)
            .read_to_string(&mut stdout)
            .expect("bounded UIA receipt");
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .expect("UIA stderr")
            .take(8193)
            .read_to_string(&mut stderr)
            .expect("bounded UIA diagnostics");
        assert!(
            stdout.len() <= 8192 && stderr.len() <= 8192,
            "bounded UIA output"
        );
        assert!(status.success(), "UIA verification failed: {stderr}");
        let receipt: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("actual UIA receipt");
        assert_eq!(
            receipt["process"].as_u64(),
            Some(u64::from(std::process::id()))
        );
        assert_eq!(receipt["window"].as_i64(), i64::try_from(self.panel).ok());
        assert!(
            matches!(receipt["before"].as_u64(), Some(0 | 1)),
            "actual binary provider state before action"
        );
        let after = receipt["after"]
            .as_u64()
            .expect("actual binary checkbox state");
        assert_eq!(after, u64::from(!self.initial), "real UIA toggle");
        println!("KELD_NATIVE_VERIFICATION {}", stdout.trim());
        Some(after != 0)
    }
}
impl Drop for Verification {
    fn drop(&mut self) {
        // Exact Child kernel handle, never PID search or a broad process kill.
        // Failure cleanup remains RED; no later panel begins until the child ends.
        self.request_stop()
            .expect("stop exact owned UIA child after Show");
        let started = Instant::now();
        while self
            .child
            .try_wait()
            .expect("reap exact owned UIA child")
            .is_none()
        {
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "UIA child did not stop"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn message(initial: bool) -> MessageOptions {
    MessageOptions {
        message: "Actual native result".into(),
        kind: MessageKind::Question,
        buttons: vec!["&Save".into(), "&Discard Changes".into(), "Cancel".into()],
        default_id: Some(0),
        cancel_id: Some(2),
        normalize_access_keys: true,
        title: Some("Keld dialog proof".into()),
        detail: Some("Native checkbox and button.".into()),
        no_link: true,
        checkbox_label: Some("Remember choice".into()),
        checkbox_checked: Some(initial),
    }
}

fn run(
    parent: &Parent,
    options: WindowsDialogOptions<'_>,
    action: Action,
) -> Option<DialogReplyV1> {
    let canceled = Arc::new(AtomicBool::new(false));
    let observe = Observer::start(parent, action, Arc::clone(&canceled));
    let result = show(
        parent.hwnd().0 as isize,
        options,
        Arc::new(move || canceled.load(Ordering::Acquire)),
    )
    .expect("actual native result");
    observe.drained();
    if let Some(actual) = observe.0.checkbox.get() {
        assert!(
            matches!(&result, Some(DialogReplyV1::Message { checkbox_checked, .. }) if *checkbox_checked == actual),
            "returned checkbox must equal actual native control"
        );
    }
    println!(
        "KELD_NATIVE_DIALOG_DRAINED kind={} action={action:?} elapsed_ms={}",
        match options {
            WindowsDialogOptions::Open(_) => "open",
            WindowsDialogOptions::Save(_) => "save",
            WindowsDialogOptions::Message(_) => "message",
        },
        observe.0.started.elapsed().as_millis()
    );
    result
}

#[test]
#[ignore = "opens actual Windows native dialogs; run serially with desktop custody"]
#[allow(clippy::too_many_lines)] // One serial real-OS sequence proves recovery after each control.
fn actual_windows_dialog_cancellation_selection_and_recovery() {
    let parent = Parent::new();
    let base = std::env::temp_dir().join(format!("keld-dialog-native-{}", std::process::id()));
    assert!(
        base.to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("D:"),
        "acceptance scratch must remain on D:"
    );
    std::fs::create_dir(&base).expect("fresh run-owned fixture directory");
    let selected = base.join("selected sentinel \u{00fc}.txt");
    std::fs::write(&selected, b"unchanged sentinel").expect("owned selection fixture");
    let saved = base.join("save location \u{00fc}.txt");
    let open = OpenDialogOptions {
        default_path: Some(selected.to_string_lossy().into_owned()),
        filters: vec![],
        properties: vec![],
    };
    let save = SaveDialogOptions {
        default_path: Some(saved.to_string_lossy().into_owned()),
        filters: vec![],
        security_scoped_bookmarks: false,
    };
    assert_eq!(
        run(&parent, WindowsDialogOptions::Open(&open), Action::Cancel),
        None
    );
    assert_eq!(
        run(&parent, WindowsDialogOptions::Save(&save), Action::Cancel),
        None
    );
    assert_eq!(
        run(
            &parent,
            WindowsDialogOptions::Message(&message(false)),
            Action::Cancel
        ),
        None
    );
    assert_eq!(
        run(&parent, WindowsDialogOptions::Open(&open), Action::Select),
        Some(DialogReplyV1::Open {
            canceled: false,
            file_paths: vec![selected.to_string_lossy().into_owned()]
        })
    );
    assert_eq!(
        run(&parent, WindowsDialogOptions::Save(&save), Action::Select),
        Some(DialogReplyV1::Save {
            canceled: false,
            file_path: saved.to_string_lossy().into_owned()
        })
    );
    assert_eq!(
        run(
            &parent,
            WindowsDialogOptions::Open(&open),
            Action::UserCancel
        ),
        Some(DialogReplyV1::Open {
            canceled: true,
            file_paths: vec![]
        })
    );
    assert_eq!(
        run(
            &parent,
            WindowsDialogOptions::Save(&save),
            Action::UserCancel
        ),
        Some(DialogReplyV1::Save {
            canceled: true,
            file_path: String::new()
        })
    );
    assert!(!saved.exists(), "Save must not create its application file");
    assert_eq!(
        std::fs::read(&selected).expect("sentinel still present"),
        b"unchanged sentinel"
    );
    // The actual native result independently proves both initial checkbox values;
    // DirectUI's initial UIA ToggleState is known to be stale for checked=true.
    for initial in [false, true] {
        assert_eq!(
            run(
                &parent,
                WindowsDialogOptions::Message(&message(initial)),
                Action::MessageUntoggled { response: 2 }
            ),
            Some(DialogReplyV1::Message {
                response: 2,
                checkbox_checked: initial
            }),
            "untoggled native verification output"
        );
    }
    for (initial, response) in [(false, 0), (true, 1)] {
        assert_eq!(
            run(
                &parent,
                WindowsDialogOptions::Message(&message(initial)),
                Action::Message { response, initial }
            ),
            Some(DialogReplyV1::Message {
                response: u32::try_from(response).expect("declared nonnegative index"),
                checkbox_checked: !initial
            })
        );
    }
    let recovery = message(false);
    for options in [
        WindowsDialogOptions::Open(&open),
        WindowsDialogOptions::Save(&save),
        WindowsDialogOptions::Message(&recovery),
    ] {
        assert_eq!(
            show(parent.hwnd().0 as isize, options, Arc::new(|| true))
                .expect("cancel before presentation"),
            None
        );
        let retiring = Parent::new();
        assert_eq!(run(&retiring, options, Action::DestroyParent), None);
        // A later operation on the still-owned original parent must survive.
        assert_eq!(
            run(
                &parent,
                WindowsDialogOptions::Message(&recovery),
                Action::MessageUntoggled { response: 2 }
            ),
            Some(DialogReplyV1::Message {
                response: 2,
                checkbox_checked: false
            })
        );
    }
    std::fs::remove_file(&selected).expect("remove owned sentinel");
    std::fs::remove_dir(&base).expect("remove empty owned fixture directory");
}
