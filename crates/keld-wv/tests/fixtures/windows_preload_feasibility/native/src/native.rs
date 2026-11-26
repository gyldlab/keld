//! Owns this fixture's WebView/profile lifetime and verified resource release.
use crate::config::Config;
use crate::events;
use crate::pump::{hr, pump_until, wait, wide};
use crate::resources::{Apartment, ProcessHandle, Window};
use crate::trace::Shared;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND, COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND_NORMAL,
    COREWEBVIEW2_PROCESS_KIND, ICoreWebView2, ICoreWebView2Controller, ICoreWebView2Environment,
    ICoreWebView2Environment5, ICoreWebView2Environment8, ICoreWebView2EnvironmentOptions,
};
use webview2_com::{
    BrowserProcessExitedEventHandler, CoreWebView2EnvironmentOptions,
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    Microsoft::Web::WebView2::Win32::CreateCoreWebView2EnvironmentWithOptions,
};
use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::{HSTRING, Interface, PCWSTR};
pub(crate) struct Session {
    pub(crate) webview: Option<ICoreWebView2>,
    controller: Option<ICoreWebView2Controller>,
    environment: Option<ICoreWebView2Environment>,
    environment5: Option<ICoreWebView2Environment5>,
    exit_token: i64,
    window: Window,
    processes: Vec<ProcessHandle>,
    profile: PathBuf,
    pub(crate) runtime_version: String,
    pub(crate) browser_pid: u32,
    _apartment: Apartment,
}
impl Session {
    pub(crate) fn create(config: &Config, state: &Shared) -> Result<Self, String> {
        let profile = config.out_dir.join("profile");
        std::fs::create_dir(&profile)
            .map_err(|e| format!("create exclusive fixture profile: {e}"))?;
        state
            .borrow_mut()
            .push("OwnedProfileCreated", json!({"path": profile}));
        let apartment = Apartment::create()?;
        let window = Window::create(config.visible)?;
        let hwnd = window.hwnd.ok_or("owned HWND missing")?;
        let slot = Rc::new(RefCell::new(None));
        let output = slot.clone();
        let handler =
            CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |status, env| {
                *output.borrow_mut() = Some(
                    status.and_then(|()| env.ok_or_else(|| windows::core::Error::from(E_POINTER))),
                );
                Ok(())
            }));
        let options = CoreWebView2EnvironmentOptions::default();
        // SAFETY: fresh fixture-only profile; STA completion pumped below.
        unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                &HSTRING::from(profile.as_os_str()),
                &ICoreWebView2EnvironmentOptions::from(options),
                &handler,
            )
        }
        .map_err(|e| e.to_string())?;
        wait(state, config.timeout, "environment creation", || {
            slot.borrow().is_some()
        })?;
        let env = slot
            .borrow_mut()
            .take()
            .ok_or("missing environment callback")?
            .map_err(|e| e.to_string())?;
        // SAFETY: live environment getter on the owning STA.
        let runtime_version =
            unsafe { wide(|p| env.BrowserVersionString(p)) }.map_err(|e| e.to_string())?;
        let env5: ICoreWebView2Environment5 = env.cast().map_err(|e| e.to_string())?;
        let log = state.clone();
        let handler = BrowserProcessExitedEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                let mut pid = 0u32;
                let mut kind = COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND(0);
                // SAFETY: native environment event args on this STA.
                unsafe {
                    args.BrowserProcessId(&mut pid)?;
                    args.BrowserProcessExitKind(&mut kind)?;
                }
                log.borrow_mut().push(
                    "BrowserProcessExited",
                    json!({"browserPid": pid, "exitKind": kind.0}),
                );
            }
            Ok(())
        }));
        let mut exit_token = 0i64;
        // SAFETY: retain environment until the matching browser-exit event is observed.
        unsafe { env5.add_BrowserProcessExited(&handler, &mut exit_token) }
            .map_err(|e| e.to_string())?;
        let mut session = Self {
            webview: None,
            controller: None,
            environment: Some(env),
            environment5: Some(env5),
            exit_token,
            window,
            processes: Vec::new(),
            profile,
            runtime_version,
            browser_pid: 0,
            _apartment: apartment,
        };
        session.create_controller(hwnd, config, state)?;
        Ok(session)
    }
    fn create_controller(
        &mut self,
        hwnd: HWND,
        config: &Config,
        state: &Shared,
    ) -> Result<(), String> {
        let env = self.environment.as_ref().ok_or("environment missing")?;
        let slot = Rc::new(RefCell::new(None));
        let output = slot.clone();
        let handler =
            CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |status, ctl| {
                *output.borrow_mut() = Some(
                    status.and_then(|()| ctl.ok_or_else(|| windows::core::Error::from(E_POINTER))),
                );
                Ok(())
            }));
        // SAFETY: live environment and owned HWND on the same STA.
        unsafe { env.CreateCoreWebView2Controller(hwnd, &handler) }.map_err(|e| e.to_string())?;
        wait(state, config.timeout, "controller creation", || {
            slot.borrow().is_some()
        })?;
        self.controller = Some(
            slot.borrow_mut()
                .take()
                .ok_or("missing controller callback")?
                .map_err(|e| e.to_string())?,
        );
        let ctl = self.controller.as_ref().ok_or("controller missing")?;
        let mut rect = RECT::default();
        // SAFETY: native window/WebView mutations remain on the owning STA.
        unsafe {
            GetClientRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
            ctl.SetBounds(rect).map_err(|e| e.to_string())?;
            ctl.SetIsVisible(config.visible)
                .map_err(|e| e.to_string())?;
            self.webview = Some(ctl.CoreWebView2().map_err(|e| e.to_string())?);
        }
        let wv = self.webview.as_ref().ok_or("WebView missing")?;
        // SAFETY: narrow fixture settings before first navigation; never expose host objects.
        unsafe {
            wv.BrowserProcessId(&mut self.browser_pid)
                .map_err(|e| e.to_string())?;
            let settings = wv.Settings().map_err(|e| e.to_string())?;
            settings
                .SetAreHostObjectsAllowed(false)
                .map_err(|e| e.to_string())?;
            settings
                .SetAreDevToolsEnabled(false)
                .map_err(|e| e.to_string())?;
            settings
                .SetAreDefaultScriptDialogsEnabled(false)
                .map_err(|e| e.to_string())?;
            settings
                .SetIsWebMessageEnabled(true)
                .map_err(|e| e.to_string())?;
        }
        events::install(wv, state, config)?;
        state.borrow_mut().push(
            "NativeSessionCreated",
            json!({"runtimeVersion": self.runtime_version,
            "browserPid": self.browser_pid, "visible": config.visible,
            "rawHostObjectsAllowed": false, "profile": self.profile}),
        );
        self.census(state, "after-controller")?;
        if !self.processes.iter().any(|p| p.pid == self.browser_pid) {
            return Err("browser process HANDLE not acquired before navigation".to_owned());
        }
        Ok(())
    }
    pub(crate) fn census(&mut self, state: &Shared, label: &str) -> Result<Value, String> {
        let env: ICoreWebView2Environment8 = self
            .environment
            .as_ref()
            .ok_or("environment missing")?
            .cast()
            .map_err(|e| e.to_string())?;
        // SAFETY: read this fresh-profile environment’s native process collection on its STA.
        let infos = unsafe { env.GetProcessInfos() }.map_err(|e| e.to_string())?;
        let mut count = 0u32;
        // SAFETY: live collection getter.
        unsafe { infos.Count(&mut count) }.map_err(|e| e.to_string())?;
        let mut items = Vec::new();
        for index in 0..count {
            let mut pid = 0i32;
            let mut kind = COREWEBVIEW2_PROCESS_KIND(0);
            // SAFETY: valid collection index; native values copied before releasing the COM item.
            unsafe {
                let info = infos.GetValueAtIndex(index).map_err(|e| e.to_string())?;
                info.ProcessId(&mut pid).map_err(|e| e.to_string())?;
                info.Kind(&mut kind).map_err(|e| e.to_string())?;
            }
            let pid = u32::try_from(pid).map_err(|_| "invalid process id from native census")?;
            let mut open_error = None;
            if !self.processes.iter().any(|p| p.pid == pid && !p.signaled()) {
                // SAFETY: SYNCHRONIZE-only handle to a process returned by our environment.
                match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
                    Ok(handle) => self.processes.push(ProcessHandle {
                        pid,
                        kind: kind.0,
                        handle: Some(handle),
                    }),
                    Err(error) => open_error = Some(error.to_string()),
                }
            }
            items.push(json!({"pid": pid, "kind": kind.0, "openHandleError": open_error}));
        }
        let result = json!({"label": label, "processes": items,
            "scope": "WebView2 GetProcessInfos for owned profile; API excludes crashpad"});
        state.borrow_mut().push("ProcessCensus", result.clone());
        Ok(result)
    }
    pub(crate) fn close(&mut self, state: &Shared, config: &Config) -> Value {
        let census = self
            .census(state, "before-close")
            .unwrap_or_else(|e| json!({"error": e}));
        let close_result = self.controller.as_ref().map(|controller| {
            // SAFETY: explicit Close on this owned controller on its STA.
            unsafe { controller.Close() }
        });
        let close_status = close_result
            .as_ref()
            .map(hr)
            .unwrap_or_else(|| "absent".to_owned());
        state
            .borrow_mut()
            .push("ControllerClose", json!({"status": close_status}));
        self.webview.take();
        self.controller.take();
        let exited = || {
            state.borrow().events.iter().any(|event| {
                event["event"] == "BrowserProcessExited"
                    && event["browserPid"].as_u64() == Some(u64::from(self.browser_pid))
                    && event["exitKind"].as_i64()
                        == Some(i64::from(COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND_NORMAL.0))
            })
        };
        let wait_finished = pump_until(Instant::now() + config.cleanup_timeout, || {
            exited() && self.processes.iter().all(ProcessHandle::signaled)
        });
        let browser_exited = exited();
        let handles: Vec<Value> = self
            .processes
            .iter()
            .map(|p| json!({"pid": p.pid, "kind": p.kind, "signaled": p.signaled()}))
            .collect();
        let all_signaled =
            !self.processes.is_empty() && self.processes.iter().all(ProcessHandle::signaled);
        let mut removal = "retained: browser-release proof incomplete".to_owned();
        if wait_finished && browser_exited && all_signaled && close_status == "S_OK" {
            removal = match std::fs::remove_dir_all(&self.profile) {
                Ok(()) => "removed".to_owned(),
                Err(error) => format!("retained: {error}"),
            };
        }
        let window = self.window.close();
        let event_remove = self.environment5.as_ref().map(|env| {
            // SAFETY: remove exactly this session's environment callback token.
            hr(&unsafe { env.remove_BrowserProcessExited(self.exit_token) })
        });
        self.environment5.take();
        self.environment.take();
        let releases: Vec<Value> = self
            .processes
            .iter_mut()
            .map(|p| json!({"pid": p.pid, "closeHandle": p.release()}))
            .collect();
        self.processes.clear();
        let complete = browser_exited
            && all_signaled
            && close_status == "S_OK"
            && window["windowDestroyed"] == true
            && window["classUnregistered"] == true
            && removal == "removed"
            && event_remove.as_deref() == Some("S_OK")
            && releases.iter().all(|p| p["closeHandle"] == "S_OK");
        let browser_handle_signaled = self.browser_pid != 0
            && handles.iter().any(|p| {
                p["pid"].as_u64() == Some(u64::from(self.browser_pid)) && p["signaled"] == true
            });
        let result = json!({"required": true, "complete": complete, "browserExited": browser_exited,
            "controllerClosed": close_status == "S_OK",
            "browserExitEventObserved": browser_exited,
            "browserHandleSignaled": browser_handle_signaled,
            "profileRemoved": removal == "removed",
            "windowDestroyed": window["windowDestroyed"],
            "matchingBrowserPid": self.browser_pid, "controllerClose": close_status,
            "waitFinished": wait_finished, "allObservedProcessHandlesSignaled": all_signaled,
            "processHandles": handles, "processHandleReleases": releases, "window": window,
            "profilePath": self.profile, "profileDisposition": removal,
            "browserExitHandlerRemoval": event_remove, "preCloseCensus": census,
            "fallbackKilledProcesses": false});
        state.borrow_mut().push("CleanupObserved", result.clone());
        result
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(controller) = self.controller.take() {
            // SAFETY: fallback release of our own controller only; never counted as verified cleanup.
            let _ = unsafe { controller.Close() };
        }
        self.webview.take();
        self.environment5.take();
        self.environment.take();
    }
}
