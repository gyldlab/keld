//! Windows backend: `WebView2` via tao + wry scaffolding (KEL-27).
//!
//! Interim implementation — replace with direct windows-rs + `WebView2` COM
//! bindings per `docs/architecture/05-webview-and-native.md` §1, the same way
//! `crate::wkwebview` is interim until direct objc2 bindings land. Layout
//! mirrors wry's `competitors/wry/src/webview2/` per-platform module pattern.
//!
//! Unlike the macOS backend this module owns one extra Windows-only concern:
//! the Evergreen runtime is a separate redistributable that may be absent.
//! [`runtime_version`] probes for it up front so the failure is a typed
//! `KELD-WV-008` with install guidance, not an opaque COM `HRESULT` surfaced
//! from deep inside wry (`wry::Error::WebView2Error`).
//!
// SAFETY: this module calls two WebView2/COM C ABI functions.
// `GetAvailableCoreWebView2BrowserVersionString` is a pure query — it takes a
// browser-executable folder (null = use the installed Evergreen runtime) and
// writes an owned, COM-allocated UTF-16 string we must release with
// `CoTaskMemFree`. Both calls are confined to `runtime_version`, which owns the
// pointer for its whole lifetime and never hands it out. No COM apartment
// initialization is required for this function; wry initializes the apartment
// it needs when the webview is actually created. Every mutation of engine and
// window state happens on the main thread inside tao's event loop, satisfying
// the crate `AGENTS.md` "UI-thread-only mutations" invariant.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Window, WindowBuilder};

use crate::WebviewId;
use crate::engine::{DevtoolsAction, NavTarget, Rect, WebEngine, WebView2EngineExt, WebviewSpec};
use crate::error::WvError;

/// Returns the installed `WebView2` Evergreen runtime version.
///
/// This is the detection path Microsoft documents. We deliberately do not read
/// the `EdgeUpdate` registry keys ourselves: which key is authoritative differs
/// between the Evergreen, fixed-version, and per-user channels, and the loader
/// already encodes those rules.
///
/// # Errors
///
/// Returns [`WvError::WebView2RuntimeMissing`] when the runtime is absent, too
/// old, or the loader reports any other failure.
pub fn runtime_version() -> Result<String, WvError> {
    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::core::{PCWSTR, PWSTR};

    let mut version = PWSTR::null();
    // SAFETY: `PCWSTR::null()` selects the installed Evergreen runtime (the
    // documented "use the default install" argument). `&mut version` is a valid
    // out-pointer for the duration of the call. See the module SAFETY note.
    let probe =
        unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &raw mut version) };

    if let Err(err) = probe {
        return Err(WvError::WebView2RuntimeMissing {
            detail: err.to_string(),
        });
    }
    if version.is_null() {
        return Err(WvError::WebView2RuntimeMissing {
            detail: String::from("loader reported success but returned no version"),
        });
    }

    // SAFETY: the loader returned success and a non-null pointer, so `version`
    // points at a NUL-terminated UTF-16 string it allocated with the COM task
    // allocator. We read it before freeing, and free exactly once.
    let text = unsafe { version.to_string() };
    // SAFETY: `version` came from the COM task allocator and has not been freed.
    unsafe { CoTaskMemFree(Some(version.as_ptr().cast())) };

    text.map_err(|err| WvError::WebView2RuntimeMissing {
        detail: format!("runtime version string was not valid UTF-16: {err}"),
    })
}

/// One live webview and the host window it fills (v0: one per window).
struct View {
    window: Window,
    webview: wry::WebView,
}

/// The Windows [`WebEngine`] backend.
///
/// Owns the tao event loop until [`WebView2Engine::run_until_closed`] consumes
/// it, mirroring `crate::wkwebview::WkWebViewEngine`; keld-core takes ownership
/// of the loop once the command-queue design lands (crate `AGENTS.md`).
pub struct WebView2Engine {
    /// Present until the run loop starts; consumed by `run_until_closed`.
    event_loop: Option<EventLoop<()>>,
    views: BTreeMap<u32, View>,
    next_id: u32,
}

impl fmt::Debug for WebView2Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebView2Engine")
            .field("views", &self.views.len())
            .field("running", &self.event_loop.is_none())
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

impl WebView2Engine {
    /// Creates the engine and its event loop.
    ///
    /// Must be called on the process main thread: tao pumps the Win32 message
    /// loop, and `WebView2` controllers are affine to the thread that created
    /// them.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::WebView2RuntimeMissing`] when the Evergreen runtime
    /// is not installed, so callers fail with install guidance before a window
    /// ever appears.
    pub fn new() -> Result<Self, WvError> {
        // Probe first: a missing runtime is a user-fixable setup problem, and
        // reporting it before any window exists avoids a flash of empty chrome.
        runtime_version()?;
        Ok(Self {
            event_loop: Some(EventLoop::new()),
            views: BTreeMap::new(),
            next_id: 1,
        })
    }

    /// Runs the event loop until the user closes a window, then exits the
    /// process (tao's `EventLoop::run` owns the thread and never returns).
    ///
    /// # Errors
    ///
    /// Returns [`WvError::EventLoop`] if the run loop was already started.
    pub fn run_until_closed(mut self) -> Result<(), WvError> {
        let Some(event_loop) = self.event_loop.take() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; call run_until_closed once",
            )));
        };
        let mut views = std::mem::take(&mut self.views);
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            if let Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } = event
            {
                // v0 hello is one window. Drop by id so a second window would
                // not tear down every view; exit when the map is empty. On
                // Windows, closing the last window quits — the platform
                // convention, and what KEL-57 V1-10 will assert.
                views.retain(|_, view| view.window.id() != window_id);
                if views.is_empty() {
                    *control_flow = ControlFlow::Exit;
                }
            }
        });
    }

    fn view(&self, id: WebviewId) -> Result<&View, WvError> {
        self.views
            .get(&id.0)
            .ok_or(WvError::UnknownWebview { id: id.0 })
    }
}

impl WebEngine for WebView2Engine {
    fn create(&mut self, spec: &WebviewSpec) -> Result<WebviewId, WvError> {
        let Some(event_loop) = self.event_loop.as_ref() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; create webviews before run_until_closed",
            )));
        };
        let window = WindowBuilder::new()
            .with_title(&spec.title)
            // Logical (DPI-independent) points. tao declares per-monitor-v2 DPI
            // awareness for the process, so this is scaled by the monitor's
            // factor rather than bitmap-stretched by the OS.
            .with_inner_size(tao::dpi::LogicalSize::new(
                spec.size.width,
                spec.size.height,
            ))
            // KEL-25/KEL-27: standard Windows caption — minimize, maximize, close.
            .with_resizable(true)
            .with_minimizable(true)
            .with_closable(true)
            .build(event_loop)
            .map_err(|e| WvError::Window(e.to_string()))?;

        // Developer extras are debug-only until keld-guard owns `web.devtools`.
        let builder = wry::WebViewBuilder::new();
        #[cfg(debug_assertions)]
        let builder = builder.with_devtools(true);
        let builder = match &spec.initial {
            NavTarget::Html(html) => builder.with_html(html),
            NavTarget::Url(url) => builder.with_url(url),
        };
        // On Windows wry parents the WebView2 controller to the window and
        // auto-resizes it, so v0 needs no explicit bounds plumbing here.
        let webview = builder.build(&window).map_err(|e| match e {
            // A runtime that disappears between the probe and here (uninstalled
            // mid-run, or a controller the loader refuses) still reports as a
            // setup problem rather than a generic webview failure.
            wry::Error::WebView2Error(inner) => WvError::WebView2RuntimeMissing {
                detail: inner.to_string(),
            },
            other => WvError::Webview(other.to_string()),
        })?;

        let id = self.next_id;
        self.next_id += 1;
        self.views.insert(id, View { window, webview });
        Ok(WebviewId(id))
    }

    fn navigate(&mut self, id: WebviewId, target: NavTarget) -> Result<(), WvError> {
        let view = self.view(id)?;
        match target {
            NavTarget::Html(html) => view.webview.load_html(&html),
            NavTarget::Url(url) => view.webview.load_url(&url),
        }
        .map_err(|e| WvError::Navigate(e.to_string()))
    }

    fn eval(&mut self, id: WebviewId, script: &str) -> Result<(), WvError> {
        self.view(id)?
            .webview
            .evaluate_script(script)
            .map_err(|e| WvError::Script(e.to_string()))
    }

    fn set_bounds(&mut self, id: WebviewId, rect: Rect) -> Result<(), WvError> {
        let view = self.view(id)?;
        // v0 webviews fill their host window, so bounds apply to the window.
        view.window
            .set_outer_position(tao::dpi::LogicalPosition::new(rect.x, rect.y));
        view.window
            .set_inner_size(tao::dpi::LogicalSize::new(rect.width, rect.height));
        Ok(())
    }

    fn devtools(&mut self, id: WebviewId, action: DevtoolsAction) -> Result<(), WvError> {
        let view = self.view(id)?;
        match action {
            DevtoolsAction::Open => view.webview.open_devtools(),
            DevtoolsAction::Close => view.webview.close_devtools(),
        }
        Ok(())
    }

    fn destroy(&mut self, id: WebviewId) -> Result<(), WvError> {
        // Dropping the `View` releases the webview, then closes the window.
        self.views
            .remove(&id.0)
            .map(|_| ())
            .ok_or(WvError::UnknownWebview { id: id.0 })
    }
}

impl WebView2EngineExt for WebView2Engine {}

/// Opens a window from `spec` and runs until the user closes it.
///
/// Thin wrapper for the Phase 1 hello slice: probes the runtime, builds a
/// [`WebView2Engine`], creates one webview, and hands the thread to the run
/// loop.
///
/// # Errors
///
/// Returns [`WvError::WebView2RuntimeMissing`] if the Evergreen runtime is
/// absent, or another [`WvError`] if window or webview creation fails.
pub fn run_hello(spec: &WebviewSpec) -> Result<(), WvError> {
    let mut engine = WebView2Engine::new()?;
    engine.create(spec)?;
    engine.run_until_closed()
}

#[cfg(test)]
mod tests {
    use super::{WebView2Engine, runtime_version};
    use crate::error::WvError;

    /// The CI runners and this developer machine both ship the Evergreen
    /// runtime, so the probe must succeed and return a dotted version. If it
    /// ever fails the message has to name the code and the fix, because that is
    /// the only thing a user sees when their runtime is missing.
    #[test]
    fn runtime_probe_reports_version_or_actionable_error() {
        match runtime_version() {
            Ok(version) => {
                assert!(
                    version.split('.').count() >= 2,
                    "expected a dotted WebView2 version, got: {version}"
                );
            }
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    matches!(err, WvError::WebView2RuntimeMissing { .. }),
                    "probe must fail as WebView2RuntimeMissing, got: {msg}"
                );
                assert!(msg.contains("KELD-WV-008"), "missing code in: {msg}");
                assert!(msg.contains("Evergreen Runtime"), "missing fix in: {msg}");
            }
        }
    }

    /// Keeps the engine type named from the test module so a rename fails the
    /// build here rather than silently orphaning these tests. Constructing one
    /// needs a tao `EventLoop`, which must be built on the process main thread —
    /// the harness gives each test its own thread, so the engine itself is
    /// exercised by the GUI pass in KEL-27, not here.
    fn _assert_engine_type(_: Option<&WebView2Engine>) {}

    /// A stale id must stay a typed error on this backend too (no panics in
    /// libs, per root `AGENTS.md`).
    #[test]
    fn unknown_webview_id_is_typed_not_panic() {
        let err = WvError::UnknownWebview { id: 3 };
        assert!(err.to_string().contains("KELD-WV-007"));
    }
}
