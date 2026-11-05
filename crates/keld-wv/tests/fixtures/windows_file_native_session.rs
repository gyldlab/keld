//! Owns the raw native probe's STA, window, `WebView2` objects and callback observations.
//! No Keld policy, file parser, identity, facade or transport is implemented here.

#[path = "windows_file_native_callbacks.rs"]
mod callbacks;
#[path = "windows_file_native_operations.rs"]
mod operations;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use webview2_com::Microsoft::Web::WebView2::Win32 as wv;
use webview2_com::{
    BrowserProcessExitedEventHandler, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, take_pwstr,
};
use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, MSG, MWMO_INPUTAVAILABLE,
    MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT, TranslateMessage,
    WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
};
use windows::core::{BOOL, HSTRING, Interface, PCWSTR, PWSTR, w};

type NativeResult<T> = windows::core::Result<T>;
type Completion<T> = Rc<RefCell<Option<NativeResult<T>>>>;
type SharedTrace = Rc<RefCell<Trace>>;
// Observe the exact draft-r18 candidate; this fixture never treats it as authority.
const HEADER: &str = "X-Keld-Host-Attempt";

#[derive(Debug)]
struct Trace {
    started: Instant,
    phase: String,
    events: Vec<Value>,
}

impl Trace {
    fn record(&mut self, kind: &str, data: Value) {
        let mut event = json!({
            "seq":self.events.len() + 1,"elapsed_us":self.started.elapsed().as_micros(),
            "phase":self.phase,"kind":kind
        });
        event["data"] = data;
        self.events.push(event);
    }
}

fn hresult(error: &windows::core::Error) -> String {
    format!("0x{:08X}", error.code().0)
}

fn failure(operation: &str, error: &windows::core::Error) -> String {
    format!("{operation}: {} {}", hresult(error), error.message())
}

fn read_string(read: impl FnOnce(*mut PWSTR) -> NativeResult<()>) -> NativeResult<String> {
    let mut pointer = PWSTR::null();
    let result = read(&raw mut pointer);
    let text = take_pwstr(pointer);
    result.map(|()| text)
}

fn record_result(trace: &SharedTrace, kind: &str, data: NativeResult<Value>) {
    let observed =
        data.unwrap_or_else(|error| json!({"error":hresult(&error),"message":error.message()}));
    trace.borrow_mut().record(kind, observed);
}

#[allow(unsafe_code)] // Test-only bounded Win32 message pump on the current STA.
fn pump_until(deadline: Instant, mut ready: impl FnMut() -> bool) -> bool {
    while !ready() {
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let millis = u32::try_from((deadline - now).as_millis().min(25)).unwrap_or(25);
        // SAFETY: wait for this thread's input only; the absolute deadline never renews.
        unsafe {
            let _ = MsgWaitForMultipleObjectsEx(None, millis, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        }
        let mut message = MSG::default();
        // SAFETY: message is writable; all messages are dispatched on their owning thread.
        while unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: PeekMessage initialized this MSG on this thread.
            unsafe {
                let _ = TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
    }
    true
}

/// All native objects belong to this one test process and STA.
#[derive(Debug)]
pub(super) struct Session {
    trace: SharedTrace,
    work_deadline: Instant,
    cleanup_deadline: Instant,
    profile: PathBuf,
    uri: String,
    document: Vec<u8>,
    com_initialized: bool,
    window: Option<HWND>,
    environment: Option<wv::ICoreWebView2Environment>,
    environment5: Option<wv::ICoreWebView2Environment5>,
    pending_environment: Option<Completion<wv::ICoreWebView2Environment>>,
    pending_controller: Option<Completion<wv::ICoreWebView2Controller>>,
    exit_token: Option<i64>,
    controller: Option<wv::ICoreWebView2Controller>,
    webview: Option<wv::ICoreWebView2>,
    pub(super) browser_pid: u32,
    pub(super) runtime_version: String,
    pub(super) devtools_enabled: Option<bool>,
}

impl Session {
    pub(super) fn new(profile: &Path, uri: String, document: Vec<u8>) -> Self {
        let started = Instant::now();
        Self {
            trace: Rc::new(RefCell::new(Trace {
                started,
                phase: "setup".into(),
                events: Vec::new(),
            })),
            work_deadline: started + Duration::from_secs(55),
            cleanup_deadline: started + Duration::from_secs(90),
            profile: profile.to_owned(),
            uri,
            document,
            com_initialized: false,
            window: None,
            environment: None,
            environment5: None,
            pending_environment: None,
            pending_controller: None,
            exit_token: None,
            controller: None,
            webview: None,
            browser_pid: 0,
            runtime_version: String::new(),
            devtools_enabled: None,
        }
    }

    pub(super) fn phase(&self, phase: &str) {
        phase.clone_into(&mut self.trace.borrow_mut().phase);
    }
    pub(super) fn events(&self) -> Vec<Value> {
        self.trace.borrow().events.clone()
    }
    fn view(&self) -> Result<&wv::ICoreWebView2, String> {
        self.webview
            .as_ref()
            .ok_or_else(|| "native view not created".into())
    }

    fn completion<T>(&self, slot: &Completion<T>, operation: &str) -> Result<T, String> {
        let deadline = self
            .work_deadline
            .min(Instant::now() + Duration::from_secs(20));
        if !pump_until(deadline, || slot.borrow().is_some()) {
            return Err(format!(
                "{operation}: absolute observation deadline expired"
            ));
        }
        if let Some(Err(error)) = slot.borrow().as_ref() {
            // Retain a reported native failure for the creation cleanup owner. An empty
            // completion cell must mean still pending, not an already-consumed rejection.
            return Err(format!(
                "{operation}: {} {}",
                hresult(error),
                error.message()
            ));
        }
        slot.borrow_mut()
            .take()
            .ok_or_else(|| format!("{operation}: missing completion"))?
            .map_err(|error| failure(operation, &error))
    }

    pub(super) fn wait_event(&self, kind: &str) -> Result<(), String> {
        let phase = self.trace.borrow().phase.clone();
        let ready = || {
            self.trace
                .borrow()
                .events
                .iter()
                .any(|event| event["phase"] == phase && event["kind"] == kind)
        };
        if pump_until(
            self.work_deadline
                .min(Instant::now() + Duration::from_secs(10)),
            ready,
        ) {
            Ok(())
        } else {
            Err(format!(
                "{phase}: {kind} was not observed before its deadline"
            ))
        }
    }

    #[allow(unsafe_code)] // Test-only COM/window acquisition; fields retain every acquired object.
    pub(super) fn start(&mut self) -> Result<(), String> {
        // SAFETY: this helper thread initializes and subsequently owns its single STA.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| failure("CoInitializeEx", &error))?;
        self.com_initialized = true;
        // SAFETY: built-in STATIC class; hidden owned top-level window, no subclass or external HWND.
        self.window = Some(
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!("Keld W3 observation"),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    640,
                    480,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .map_err(|error| failure("CreateWindowExW", &error))?,
        );
        let slot: Completion<wv::ICoreWebView2Environment> = Rc::new(RefCell::new(None));
        self.pending_environment = Some(Rc::clone(&slot));
        let output = Rc::clone(&slot);
        let callback = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |result, environment| {
                *output.borrow_mut() =
                    Some(result.and_then(|()| environment.ok_or_else(|| E_POINTER.into())));
                Ok(())
            },
        ));
        let options = wv::ICoreWebView2EnvironmentOptions::from(
            webview2_com::CoreWebView2EnvironmentOptions::default(),
        );
        // SAFETY: owned profile path and callback; no additional arguments, remote-debugging or global settings.
        let submitted = unsafe {
            wv::CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                &HSTRING::from(self.profile.as_os_str()),
                &options,
                &callback,
            )
        };
        if let Err(error) = submitted {
            self.pending_environment = None;
            return Err(failure("CreateEnvironment", &error));
        }
        let environment = self.completion(&slot, "CreateEnvironment callback")?;
        self.pending_environment = None;
        self.environment = Some(environment.clone());
        // SAFETY: live environment on its creating STA; returned PWSTR is released by read_string.
        self.runtime_version = read_string(|out| unsafe { environment.BrowserVersionString(out) })
            .map_err(|error| failure("BrowserVersionString", &error))?;
        self.register_browser_exit(&environment)?;
        let slot: Completion<wv::ICoreWebView2Controller> = Rc::new(RefCell::new(None));
        self.pending_controller = Some(Rc::clone(&slot));
        let output = Rc::clone(&slot);
        let callback = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
            move |result, controller| {
                *output.borrow_mut() =
                    Some(result.and_then(|()| controller.ok_or_else(|| E_POINTER.into())));
                Ok(())
            },
        ));
        // SAFETY: our live environment and owned window; callback runs through this thread's pump.
        let submitted = unsafe {
            environment
                .CreateCoreWebView2Controller(self.window.ok_or("missing owned window")?, &callback)
        };
        if let Err(error) = submitted {
            self.pending_controller = None;
            return Err(failure("CreateController", &error));
        }
        let controller = self.completion(&slot, "CreateController callback")?;
        self.pending_controller = None;
        self.controller = Some(controller.clone());
        self.configure_view(&controller)
    }

    #[allow(unsafe_code)] // Test-only configuration of the controller retained by this STA.
    fn configure_view(&mut self, controller: &wv::ICoreWebView2Controller) -> Result<(), String> {
        // SAFETY: owned controller and view remain on this STA. No foreground/focus operation occurs.
        unsafe {
            controller.SetBounds(RECT {
                left: 0,
                top: 0,
                right: 640,
                bottom: 480,
            })
        }
        .map_err(|error| failure("controller bounds", &error))?;
        // SAFETY: controller is live on its creating STA.
        let view = unsafe { controller.CoreWebView2() }
            .map_err(|error| failure("controller view", &error))?;
        self.webview = Some(view.clone());
        // SAFETY: live view; getters write owned outputs and settings stay on the STA.
        unsafe {
            view.BrowserProcessId(&raw mut self.browser_pid)
                .map_err(|error| failure("BrowserProcessId", &error))?;
            let settings = view
                .Settings()
                .map_err(|error| failure("Settings", &error))?;
            settings
                .SetAreDevToolsEnabled(false)
                .map_err(|error| failure("disable DevTools", &error))?;
            let mut enabled = BOOL(1);
            settings
                .AreDevToolsEnabled(&raw mut enabled)
                .map_err(|error| failure("DevTools readback", &error))?;
            self.devtools_enabled = Some(enabled.as_bool());
        }
        self.register_navigation()?;
        self.register_resources()?;
        self.trace.borrow_mut().record(
            "handlers-installed",
            json!({"before_first_navigation":true}),
        );
        Ok(())
    }

    #[allow(unsafe_code)] // Test-only environment callback registration before controller/browser creation.
    fn register_browser_exit(
        &mut self,
        environment: &wv::ICoreWebView2Environment,
    ) -> Result<(), String> {
        let environment5 = environment
            .cast::<wv::ICoreWebView2Environment5>()
            .map_err(|error| failure("Environment5", &error))?;
        let trace = Rc::clone(&self.trace);
        let callback = BrowserProcessExitedEventHandler::create(Box::new(move |_, args| {
            let observed = (|| {
                let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                let mut pid = 0;
                let mut kind = wv::COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND_NORMAL;
                // SAFETY: native callback args and stack outputs are live on the environment STA.
                unsafe {
                    args.BrowserProcessId(&raw mut pid)?;
                    args.BrowserProcessExitKind(&raw mut kind)?;
                }
                Ok(
                    json!({"pid":pid,"normal":kind == wv::COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND_NORMAL,"kind":kind.0}),
                )
            })();
            record_result(&trace, "browser-process-exited", observed);
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: live environment retains callback until explicit removal during shutdown.
        unsafe { environment5.add_BrowserProcessExited(&callback, &raw mut token) }
            .map_err(|error| failure("register BrowserProcessExited", &error))?;
        self.environment5 = Some(environment5);
        self.exit_token = Some(token);
        Ok(())
    }

    #[allow(unsafe_code)] // Test-only late-creation cleanup uses only objects returned to this STA.
    fn collect_late_creations(&mut self, faults: &mut Vec<String>) {
        if let Some(slot) = self.pending_environment.take() {
            if pump_until(self.cleanup_deadline, || slot.borrow().is_some()) {
                match slot.borrow_mut().take() {
                    Some(Ok(environment)) => {
                        self.environment = Some(environment.clone());
                        if let Err(error) = self.register_browser_exit(&environment) {
                            faults.push(error);
                        }
                        self.trace
                            .borrow_mut()
                            .record("late-environment-completion", json!({"observed":true}));
                    }
                    Some(Err(error)) => faults.push(failure("late environment failure", &error)),
                    None => faults.push("late environment completion vanished".into()),
                }
            } else {
                self.pending_environment = Some(slot);
                faults.push(
                    "environment callback remains pending; timeout did not prove cancellation"
                        .into(),
                );
            }
        }
        if let Some(slot) = self.pending_controller.take() {
            if pump_until(self.cleanup_deadline, || slot.borrow().is_some()) {
                match slot.borrow_mut().take() {
                    Some(Ok(controller)) => {
                        self.controller = Some(controller);
                        self.trace
                            .borrow_mut()
                            .record("late-controller-completion", json!({"will_close":true}));
                    }
                    Some(Err(error)) => faults.push(failure("late controller failure", &error)),
                    None => faults.push("late controller completion vanished".into()),
                }
            } else {
                self.pending_controller = Some(slot);
                faults.push(
                    "controller callback remains pending; timeout did not prove cancellation"
                        .into(),
                );
            }
        }
        if self.browser_pid == 0
            && let Some(controller) = &self.controller
        {
            // SAFETY: this controller was returned by our creation callback on this STA.
            let pid = unsafe {
                controller
                    .CoreWebView2()
                    .and_then(|view| view.BrowserProcessId(&raw mut self.browser_pid))
            };
            if let Err(error) = pid {
                faults.push(failure("late controller BrowserProcessId", &error));
            }
        }
    }

    #[allow(unsafe_code)] // Test-only normal shutdown of owned objects; no external process is signalled.
    pub(super) fn shutdown(&mut self) -> Value {
        self.phase("cleanup");
        let mut faults = Vec::new();
        let mut closed = false;
        self.collect_late_creations(&mut faults);
        if let Some(controller) = self.controller.take() {
            // SAFETY: controller was created by this STA; Close is the native shutdown operation.
            match unsafe { controller.Close() } {
                Ok(()) => closed = true,
                Err(error) => faults.push(failure("Controller.Close", &error)),
            }
            drop(controller);
        }
        drop(self.webview.take());
        let pid = self.browser_pid;
        let exited = pid != 0
            && pump_until(self.cleanup_deadline, || {
                self.trace.borrow().events.iter().any(|event| {
                    event["kind"] == "browser-process-exited"
                        && event["data"]["pid"] == pid
                        && event["data"]["normal"] == true
                })
            });
        if !exited {
            faults.push(
                "normal BrowserProcessExited for the owned PID was not observed; profile retained"
                    .into(),
            );
        }
        if let (Some(environment), Some(token)) = (&self.environment5, self.exit_token.take()) {
            // SAFETY: token was registered exactly once on this still-live environment/STA.
            if let Err(error) = unsafe { environment.remove_BrowserProcessExited(token) } {
                faults.push(failure("remove BrowserProcessExited", &error));
            }
        }
        drop(self.environment5.take());
        drop(self.environment.take());
        let mut window_destroyed = false;
        if let Some(window) = self.window.take() {
            // SAFETY: this is the HWND created and exclusively owned by the helper STA.
            match unsafe { DestroyWindow(window) } {
                Ok(()) => window_destroyed = true,
                Err(error) => faults.push(failure("DestroyWindow", &error)),
            }
        }
        if self.com_initialized {
            // SAFETY: all owned COM references have been dropped on the initialization thread.
            unsafe {
                CoUninitialize();
            }
            self.com_initialized = false;
        }
        let mut profile_removed = false;
        if exited {
            match std::fs::remove_dir_all(&self.profile) {
                Ok(()) => profile_removed = true,
                Err(error) => faults.push(format!(
                    "remove owned profile after native release: {error}"
                )),
            }
        }
        let cleanup = json!({"controller_closed":closed,"window_destroyed":window_destroyed,"profile_removed":profile_removed,"com_uninitialized":!self.com_initialized,"faults":faults});
        self.trace
            .borrow_mut()
            .record("cleanup-result", cleanup.clone());
        cleanup
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.com_initialized || self.window.is_some() || self.controller.is_some() {
            // Emergency unwind still uses owned Close and the original deadline. It cannot turn
            // a missing receipt, panic, or incomplete observation into a passing parent test.
            let _ = self.shutdown();
        }
    }
}
