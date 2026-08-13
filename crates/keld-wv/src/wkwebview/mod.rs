//! macOS backend: `WKWebView` via tao + wry scaffolding.
//!
//! Interim implementation — replace with direct objc2 bindings per
//! `docs/architecture/05-webview-and-native.md` §1 (tracked in
//! `docs/agents/learnings.md`). Layout mirrors wry's
//! `competitors/wry/src/wkwebview/` per-platform module pattern.
//!
// SAFETY: wry/tao platform backends call Objective-C WebKit/AppKit APIs that
// are only valid on the process main thread. This module is macOS-only and
// single-threaded by construction: `WkWebViewEngine::new` must run on the main
// thread (tao enforces this), and every mutation happens either before the run
// loop starts or inside tao's event loop on that same UI thread — satisfying
// the crate `AGENTS.md` "UI-thread-only mutations" invariant. No `unsafe`
// blocks appear directly in this module today; the allow sanctions the
// transitive platform calls and future direct objc2 bindings, which must each
// carry their own `// SAFETY:` proof.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Window, WindowBuilder};

use keld_guard::PermissionsManifest;

use crate::WebviewId;
use crate::engine::{DevtoolsAction, NavTarget, Rect, WebEngine, WebviewSpec, WkWebViewEngineExt};
use crate::error::WvError;
use crate::media::with_guarded_media_permissions;

/// One live webview and the host window it fills (v0: one per window).
struct View {
    window: Window,
    webview: wry::WebView,
}

/// The macOS [`WebEngine`] backend.
///
/// Owns the tao event loop until [`WkWebViewEngine::run_until_closed`]
/// consumes it — tao's `EventLoop::run` never returns, so the run loop lives
/// here for now; keld-core takes ownership of the loop once the command-queue
/// design lands (crate `AGENTS.md`).
pub struct WkWebViewEngine {
    /// Present until the run loop starts; consumed by `run_until_closed`.
    event_loop: Option<EventLoop<()>>,
    views: BTreeMap<u32, View>,
    next_id: u32,
}

impl fmt::Debug for WkWebViewEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WkWebViewEngine")
            .field("views", &self.views.len())
            .field("running", &self.event_loop.is_none())
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

impl WkWebViewEngine {
    /// Creates the engine and its event loop.
    ///
    /// Must be called on the process main thread — `AppKit`'s platform
    /// contract, enforced by tao (which aborts otherwise).
    #[must_use]
    pub fn new() -> Self {
        Self {
            event_loop: Some(EventLoop::new()),
            views: BTreeMap::new(),
            next_id: 1,
        }
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
                // not tear down every view; exit when the map is empty.
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

impl Default for WkWebViewEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WebEngine for WkWebViewEngine {
    fn create(&mut self, spec: &WebviewSpec) -> Result<WebviewId, WvError> {
        let Some(event_loop) = self.event_loop.as_ref() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; create webviews before run_until_closed",
            )));
        };
        let window = WindowBuilder::new()
            .with_title(&spec.title)
            // Logical points (not device pixels). tao maps through
            // NSWindow.backingScaleFactor so Retina 2× gets 2× the pixel size.
            // macOS 10.7+; Apple:
            // https://developer.apple.com/documentation/appkit/nswindow/backingscalefactor
            .with_inner_size(tao::dpi::LogicalSize::new(
                spec.size.width,
                spec.size.height,
            ))
            // KEL-25/KEL-26: traffic-light chrome — resize, minimize, close.
            // tao 0.35 defaults NSApplicationActivationPolicy to Regular
            // (https://docs.rs/tao/0.35.3/tao/platform/macos/trait.EventLoopExtMacOS.html).
            .with_resizable(true)
            .with_minimizable(true)
            .with_closable(true)
            .build(event_loop)
            .map_err(|e| WvError::Window(e.to_string()))?;

        // Developer extras are debug-only until keld-guard owns `web.devtools`.
        let builder = wry::WebViewBuilder::new();
        #[cfg(debug_assertions)]
        let builder = builder.with_devtools(true);
        // KEL-59: wry 0.56 still auto-grants camera/mic when this handler is
        // omitted (`WKPermissionDecision::Grant`). Empty manifest → default-deny.
        let builder = with_guarded_media_permissions(builder, PermissionsManifest::default());
        let builder = match &spec.initial {
            NavTarget::Html(html) => builder.with_html(html),
            NavTarget::Url(url) => builder.with_url(url),
        };
        let webview = builder
            .build(&window)
            .map_err(|e| WvError::Webview(e.to_string()))?;

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

impl WkWebViewEngineExt for WkWebViewEngine {}

/// Opens a window from `spec` and runs until the user closes it.
///
/// Thin wrapper for the Phase 1 hello slice: builds a [`WkWebViewEngine`],
/// creates one webview, and hands the thread to the run loop.
///
/// # Errors
///
/// Returns [`WvError`] if window or webview creation fails.
pub fn run_hello(spec: &WebviewSpec) -> Result<(), WvError> {
    let mut engine = WkWebViewEngine::new();
    engine.create(spec)?;
    engine.run_until_closed()
}
