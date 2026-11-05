//! The platform-neutral `WebEngine` contract (v0) and its supporting types.
//!
//! This is a documented subset of the normative trait sketch in
//! `docs/architecture/05-webview-and-native.md` §1 — only the methods the
//! Phase 1 macOS hello slice can actually implement today. Deviations from
//! the sketch, and why:
//!
//! - `post` and `register_scheme` are omitted: the kipc control-frame types
//!   (`ControlFrame`, `SchemeHandler`) do not exist yet, and wry custom
//!   protocols are builder-time options, so a post-creation `register_scheme`
//!   cannot be honored by the interim wry scaffolding. Both return with the
//!   kipc integration milestone.
//! - `eval` takes a plain `&str` and returns `Result` instead of
//!   `ScriptRef<'_>` + `EvalCallback`: the callback plumbing belongs to the
//!   command-queue design keld-core will own.
//! - `create` has no `HostHooks` parameter and `set_bounds` has no `Anchor`:
//!   both types depend on the multi-webview composition work; v0 is one
//!   window-filling webview per window.
//! - No `Send` supertrait: backends hold UI-thread-only platform handles
//!   (`WKWebView` today), and the crate invariant is "all engine/window
//!   mutations on the UI thread via command queue" (crate `AGENTS.md`). The
//!   `Send` bound returns with the command-queue design review.
//! - `set_bounds`, `devtools`, and `destroy` return `Result`: a stale
//!   [`WebviewId`] must surface as a typed error, never a panic (root
//!   `AGENTS.md`: no panics in libs).

use crate::WebviewId;
use crate::error::WvError;
#[cfg(target_os = "macos")]
use std::time::Instant;

/// Immutable application metadata appended to the native macOS engine user agent.
///
/// This cold value grants no authority and cannot replace the engine user agent.
/// Validation is shared by literal configuration and strict boot descriptors.
#[derive(Debug, PartialEq, Eq)]
pub struct ApplicationNameForUserAgent(Box<str>);

impl ApplicationNameForUserAgent {
    /// Validates 1..=256 visible ASCII bytes, permitting only internal spaces.
    ///
    /// # Errors
    /// Returns [`WvError::Webview`] for empty, oversized, non-ASCII or control
    /// text, or a leading/trailing space. Supply a bounded literal app label.
    pub fn new(value: String) -> Result<Self, WvError> {
        if value.is_empty()
            || value.len() > 256
            || value.starts_with(' ')
            || value.ends_with(' ')
            || value
                .bytes()
                .any(|byte| !byte.is_ascii_graphic() && byte != b' ')
        {
            return Err(WvError::Webview(String::from(
                "applicationNameForUserAgent must contain 1..=256 visible ASCII bytes with internal spaces only; remove leading/trailing spaces, controls and non-ASCII text",
            )));
        }
        Ok(Self(value.into_boxed_str()))
    }

    /// Borrows the validated metadata without allocating or granting authority.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod application_name_for_user_agent_tests;

/// Logical (DPI-independent) size in points.
///
/// On macOS these are `NSWindow` points. `backingScaleFactor` maps them to
/// device pixels (2.0 on Retina), so 960×640 points is 1920×1280 px on a
/// 2× display. macOS 10.7+; Apple:
/// <https://developer.apple.com/documentation/appkit/nswindow/backingscalefactor>
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalSize {
    /// Width in logical points.
    pub width: f64,
    /// Height in logical points.
    pub height: f64,
}

/// Logical rectangle: position plus size, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge in logical points.
    pub x: f64,
    /// Top edge in logical points.
    pub y: f64,
    /// Width in logical points.
    pub width: f64,
    /// Height in logical points.
    pub height: f64,
}

/// What a webview should display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    /// An inline HTML document, rendered as-is.
    Html(String),
    /// A URL to load (scheme included, e.g. `https://…`).
    Url(String),
}

/// A devtools request for [`WebEngine::devtools`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevtoolsAction {
    /// Open (or focus) the inspector attached to the webview.
    Open,
    /// Close the inspector if it is open.
    Close,
}

/// Creation parameters for a webview and its host window.
///
/// v0 is one window-filling webview per window, so the spec describes both.
/// Fields are the minimum the hello slice needs — no speculative options.
#[derive(Debug, Clone, PartialEq)]
pub struct WebviewSpec {
    /// Host window title.
    pub title: String,
    /// Initial inner size of the host window.
    pub size: LogicalSize,
    /// Content to display once the webview exists.
    pub initial: NavTarget,
}

/// An application query admitted by Core for the one GH459 original index.
/// The actual URI/path and source are selected by the authenticated Host, not
/// carried into the native command or used as any document/IPC principal.
/// Only cold original-load commands own this bounded heap payload; ordinary
/// GUI events retain no query allocation. The UI borrow ends after native capture.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalEntryQuery {
    text: Box<str>,
}

#[cfg(target_os = "macos")]
impl OriginalEntryQuery {
    /// Accept the bounded original query without interpreting or decoding it.
    ///
    /// # Errors
    /// Rejects overlong, fragment-bearing and control-character input.
    pub fn new(query: &str) -> Result<Self, &'static str> {
        Self::validate(query)?;
        Ok(Self { text: query.into() })
    }

    fn validate(query: &str) -> Result<(), &'static str> {
        if query.len() > 2048 || query.chars().any(|c| c == '#' || c.is_control()) {
            return Err("invalid original source query");
        }
        Ok(())
    }

    /// Borrow the exact originally admitted query without allocation/truncation.
    ///
    /// # Errors
    /// Refuses internally invalid bounded storage; no partial query is returned.
    pub fn as_str(&self) -> Result<&str, &'static str> {
        Self::validate(&self.text)?;
        Ok(&self.text)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod original_entry_query_tests {
    use super::OriginalEntryQuery;

    #[test]
    fn only_cold_query_payload_owns_indirection_and_hot_commands_stay_small() {
        assert!(std::mem::size_of::<super::AppWindowCommand>() <= 256);
        assert!(std::mem::size_of::<OriginalEntryQuery>() <= 2 * std::mem::size_of::<usize>());
        let query = OriginalEntryQuery::new("appLang=é").expect("valid UTF8");
        let owned = query.clone();
        drop(query);
        assert_eq!(owned.as_str(), Ok("appLang=é"));
        assert!(OriginalEntryQuery { text: "#".into() }.as_str().is_err());
    }

    #[test]
    fn bounded_query_keeps_exact_utf8_without_grant_or_truncation() {
        let max = "x".repeat(2048);
        assert_eq!(
            OriginalEntryQuery::new(&max).unwrap().as_str(),
            Ok(max.as_str())
        );
        assert!(OriginalEntryQuery::new(&(max + "x")).is_err());
        for denied in ["#", "key=bad\u{0000}value", "line\nfeed"] {
            assert!(OriginalEntryQuery::new(denied).is_err(), "{denied:?}");
        }
        assert_eq!(
            OriginalEntryQuery::new("appLang=fr&queryObj=%257b")
                .unwrap()
                .as_str(),
            Ok("appLang=fr&queryObj=%257b")
        );
    }
}

/// Command delivered from the primary app-link owner to the platform UI loop.
#[derive(Debug, Clone)]
pub enum AppWindowCommand {
    /// The session is ending: close windows and exit the loop. Sent after the
    /// correlated Quit reply has been written, or by the host itself at a profile
    /// handover, where the application keeps running and the host writes
    /// `LastWindowClosed` once the loop has returned (the loop emits none on this
    /// command; KEL-53 §4 "Profile handover", step 3a).
    Quit,
    /// The primary app session failed; close windows and exit with an error.
    Fatal,
    /// Build a registry-owned window and webview on the UI thread. `build` is
    /// opaque host correlation, never a native handle or an identity minted here.
    CreateWindow {
        /// Host-owned correlation for the eventual build result.
        build: u64,
        /// Requested outer-frame width in logical points.
        width: u32,
        /// Requested outer-frame height in logical points.
        height: u32,
        /// Requested outer left edge; absence preserves the OS placement.
        x: Option<i32>,
        /// Requested outer top edge; absence preserves the OS placement.
        y: Option<i32>,
        /// Exact already-admitted per-window source; no ambient/global view guess.
        #[cfg(target_os = "macos")]
        original_source: Option<std::sync::Arc<crate::wkwebview::AdmittedWebAppSource>>,
        /// Exact immutable image/configuration retained by the resulting native view.
        #[cfg(target_os = "macos")]
        preload_image: Option<std::sync::Arc<crate::wkwebview::AppPreloadImage>>,
    },
    /// Request a maximized state. A later native fact confirms its outcome.
    SetMaximized {
        /// Backend-owned view to mutate.
        view: WebviewId,
        /// Requested state, not an observed native fact.
        maximized: bool,
    },
    /// Request full-screen state. A later native fact confirms its outcome.
    SetFullScreen {
        /// Backend-owned view to mutate.
        view: WebviewId,
        /// Requested state, not an observed native fact.
        full_screen: bool,
    },
    /// Release the bridge, webview and native window, in that order. Only the
    /// subsequent `WindowReleased` event proves the native ownership was released.
    DestroyWindow {
        /// Backend-owned view to release.
        view: WebviewId,
    },
    /// Conditionally retire only the exact original native document, then use
    /// the existing physical release owner. Acceptance is not a release receipt.
    #[cfg(target_os = "macos")]
    DestroyOriginalDocument {
        /// Actual Core-mapped backend view, never a caller-cast `WindowRef`.
        view: WebviewId,
        /// Exact opaque native lease resolved under the authenticated Core owner.
        expected: crate::wkwebview::MacNativeDocumentLease,
        /// Same bounded object retained by the Core Entry until reconciliation.
        admission: std::sync::Arc<crate::wkwebview::OriginalDocumentDestroyAdmission>,
    },
    /// The authenticated GH459 Host already matched the one captured original
    /// file URL and current `HostWindowRef` -> native `WebviewId`. Only the native
    /// GH542 owner can begin a new attempt and mint a committed document.
    #[cfg(target_os = "macos")]
    LoadAdmittedOriginal {
        /// Immutable accepted Host operation and actual mapped view.
        context: NativeNavigationContext,
        /// Data-only bounded query; no native pointer/source authority.
        query: OriginalEntryQuery,
    },
    /// Wake the existing macOS main-owned authentication presentation service.
    #[cfg(target_os = "macos")]
    MacAuthWake,
    /// Wake the existing macOS Tao main service for a guarded native dialog.
    /// The actual bounded request/cancel custody stays in Core/Native; the
    /// command contains no caller-minted window pointer or payload.
    #[cfg(target_os = "macos")]
    MacDialogWake,
}

/// Physical `AppKit` Message outcome observed on the macOS UI owner. The
/// original request's authenticated correlation and nonce remain Core-owned.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacDialogUiOutcome {
    /// A real native button click and observed checkbox state.
    Selected {
        /// Actual zero-based clicked native button.
        response: u32,
        /// Checkbox state after the real native callback.
        checkbox_checked: bool,
    },
    /// A native Open/Save result was captured in the existing Core custody
    /// before this lightweight event marker was queued. The actual selected
    /// paths are never stored in this copyable OS/event type.
    FileResult,
    /// A native dismissal, never a fabricated selected button.
    Dismissed,
    /// The native presentation or callback did not meet its owner contract.
    Failed,
}

/// Observable UI-owned milestone delivered to the host session coordinator.
#[derive(Debug, PartialEq)]
#[cfg_attr(not(target_os = "macos"), derive(Clone))]
pub enum AppWindowEvent {
    /// Initial renderer navigation completed on the live backend.
    NavigationReady,
    /// The macOS application loop delivered its actual first Init turn.
    /// The timestamp is sampled on the UI thread, not by the possibly delayed
    /// coordinator. It is not inferred from HELLO, navigation or a queued reply.
    #[cfg(target_os = "macos")]
    HostInitialized {
        /// Monotonic observation time of the real first `AppKit` Init turn.
        observed_at: Instant,
    },
    /// Complete actual `AppKit` sample, or explicit unavailable result.
    /// Sent from genuine Init before `HostInitialized`; never a user-supplied fact.
    #[cfg(target_os = "macos")]
    DesktopObserved {
        /// An owned native observation, with no platform handles retained.
        facts: Option<AppDesktopFacts>,
    },
    /// The primary-window count transitioned from one to zero.
    LastWindowClosed,
    /// The native build completed; this snapshot is read from the built window.
    WindowBuilt {
        /// The host correlation from `CreateWindow`.
        build: u64,
        /// Backend-owned view, distinct from the host window identity.
        view: WebviewId,
        /// Initial native state, including OS-chosen placement.
        snapshot: WindowSnapshot,
    },
    /// A native build failed. Its full typed error is logged on the UI thread.
    WindowBuildFailed {
        /// The host correlation from `CreateWindow`.
        build: u64,
        /// The native stage that failed.
        stage: BuildStage,
    },
    /// The native close button requested closing a registry-owned view.
    WindowCloseRequested {
        /// Backend-owned view whose native close was requested.
        view: WebviewId,
    },
    /// Observed outer bounds in logical points, including frame and title bar.
    WindowBounds {
        /// Backend-owned view that produced the fact.
        view: WebviewId,
        /// Observed left edge.
        x: i32,
        /// Observed top edge.
        y: i32,
        /// Observed outer width.
        width: u32,
        /// Observed outer height.
        height: u32,
    },
    /// Observed native focus, not a setter acknowledgement.
    WindowFocus {
        /// Backend-owned view that produced the fact.
        view: WebviewId,
        /// Observed focus state.
        focused: bool,
    },
    /// Observed native maximized state, not a setter acknowledgement.
    WindowMaximized {
        /// Backend-owned view that produced the fact.
        view: WebviewId,
        /// Observed maximized state.
        maximized: bool,
    },
    /// Observed native full-screen state, not a setter acknowledgement.
    WindowFullScreen {
        /// Backend-owned view that produced the fact.
        view: WebviewId,
        /// Observed full-screen state.
        full_screen: bool,
    },
    /// The bridge, webview and window have actually been released.
    WindowReleased {
        /// Backend-owned view whose ownership was released.
        view: WebviewId,
    },
    /// Conditional native admission decision on the exact shared object.
    /// Accepted is irreversible retirement; only `WindowReleased` proves release.
    #[cfg(target_os = "macos")]
    OriginalDocumentDestroyDecision {
        /// Actual view inspected on the UI owner.
        view: WebviewId,
        /// Same Arc association as the pending Core Entry, not a new numeric ID.
        admission: std::sync::Arc<crate::wkwebview::OriginalDocumentDestroyAdmission>,
    },
    /// A fact from the same native GH542 owner as the actual `WKNavigation`.
    /// This observation is never a grant; the Core must also verify current
    /// Bun role, authentic `HostWindowRef` and native stamp on each use.
    #[cfg(target_os = "macos")]
    MacOriginalNavigation {
        /// Original accepted Host operation, captured before native dispatch.
        context: NativeNavigationContext,
        /// Actual native observation for that operation.
        fact: MacOriginalNavigationFact,
    },
    /// `AppKit` reported a physically drained Message sheet (or a refused
    /// presentation that never created one). The Core custody owner decides
    /// whether the original correlation is still live and which reply to send.
    #[cfg(target_os = "macos")]
    MacDialogFinished {
        /// Original authenticated Host generation attempt.
        attempt: u32,
        /// Original nonzero nonce, not a new authority handle.
        nonce: u64,
        /// `AppKit`-owned primitive outcome after its callback/physical drain.
        outcome: MacDialogUiOutcome,
    },
}

/// Immutable Host-only association with one accepted original load call.
/// Core retains authenticated caller/window metadata before dispatch; this
/// completion correlation is never a principal, grant or wire field.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeNavigationContext {
    operation: std::num::NonZeroU64,
    view: WebviewId,
}

#[cfg(target_os = "macos")]
impl NativeNavigationContext {
    /// Capture the registry's admitted nonzero operation and mapped view.
    #[must_use]
    pub const fn new(operation: std::num::NonZeroU64, view: WebviewId) -> Self {
        Self { operation, view }
    }
    /// Original Host completion correlation; conveys no authority.
    #[must_use]
    pub const fn operation(self) -> std::num::NonZeroU64 {
        self.operation
    }
    /// Actual native view mapped by the authenticated Core window owner.
    #[must_use]
    pub const fn view(self) -> WebviewId {
        self.view
    }
}

/// Bounded original-load refusal before any native attempt is minted.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeNavigationRefusal {
    /// The mapped view is absent or does not match this native owner.
    UnknownView,
    /// No verified original source is admitted to this native view.
    MissingSource,
    /// The bounded original query is malformed or unavailable.
    InvalidQuery,
    /// Foundation rejected the URL or its normalized shared resource limit.
    NativeUrlRejected,
    /// The native document owner is destroyed or its counter exhausted.
    Unavailable,
    /// The shared source retained-byte account has no capacity for this alias.
    RetainedByteLimit,
}

#[cfg(target_os = "macos")]
impl core::fmt::Display for NativeNavigationRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let detail = match self {
            Self::UnknownView => "mapped native view is unavailable; drop stale window references",
            Self::MissingSource => {
                "verified source is unavailable; admit the selected graph and policy before loading"
            }
            Self::InvalidQuery => {
                "query is malformed; use the selected entry without fragments or controls"
            }
            Self::NativeUrlRejected => {
                "normalized native URL is invalid or over budget; shorten its query"
            }
            Self::Unavailable => {
                "native document owner is unavailable; create a fresh admitted window"
            }
            Self::RetainedByteLimit => {
                "retained navigation budget is full; release completed observations before loading again"
            }
        };
        let code = if matches!(self, Self::UnknownView) {
            "KELD-WV-007"
        } else {
            "KELD-WV-005"
        };
        write!(f, "{code}: {detail}")
    }
}
#[cfg(target_os = "macos")]
impl std::error::Error for NativeNavigationRefusal {}

/// Actual native observations for one captured Host operation.
#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq, Eq)]
pub enum MacOriginalNavigationFact {
    /// Refusal before minting any `NativeAttempt`; not a fatal app failure.
    Refused {
        /// Closed original-load refusal category.
        error: NativeNavigationRefusal,
    },
    /// The actual host-owned loadRequest returned a retained navigation.
    Started {
        /// Opaque actual native attempt; never synthesized by Core or a page.
        attempt: crate::wkwebview::MacNativeAttempt,
    },
    /// The matched native callback committed the qualified original document.
    Committed {
        /// Actual native committed observation; not future-operation authority.
        stamp: crate::wkwebview::MacNativeDocumentStamp,
        /// Qualified actual URL sharing its source-wide retained-byte charge.
        native_url: crate::wkwebview::MacNavigationUrl,
        /// Irreversible revocation witness minted by that same actual commit.
        document: crate::wkwebview::MacNativeDocumentLease,
    },
    /// The matching actual navigation finished; commit alone is insufficient.
    Finished {
        /// Actual matched committed observation.
        stamp: crate::wkwebview::MacNativeDocumentStamp,
    },
    /// A real admitted attempt failed or its native load return was absent.
    Failed {
        /// Actual native attempt whose operation failed.
        attempt: crate::wkwebview::MacNativeAttempt,
        /// Old committed observation invalidated by this exact failure.
        revoked: Option<crate::wkwebview::MacNativeDocumentStamp>,
    },
    /// The native owner retired a previously observed document.
    Revoked {
        /// Old observation; never authority for a later native operation.
        previous: Option<crate::wkwebview::MacNativeDocumentStamp>,
    },
}

/// Stage of a failed native registry-window build (GH-531).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStage {
    /// Native window creation or its initial geometry failed.
    Window,
    /// Guarded webview construction or profile verification failed.
    Webview,
}

/// Native state read on the UI thread immediately after a successful build.
/// Bounds describe the outer frame in logical points; requested values are not
/// substituted for OS-chosen placement or native state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSnapshot {
    /// Outer left edge, relative to the primary display's top-left origin.
    pub x: i32,
    /// Outer top edge, relative to the primary display's top-left origin.
    pub y: i32,
    /// Outer width including native frame and title bar.
    pub width: u32,
    /// Outer height including native frame and title bar.
    pub height: u32,
    /// Whether the native window is focused.
    pub focused: bool,
    /// Whether the native window is maximized.
    pub maximized: bool,
    /// Whether the native window is in full screen.
    pub full_screen: bool,
}

/// An owned `AppKit` point-space rectangle after checked primary-origin conversion.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppDesktopRect {
    /// Horizontal offset from primary top-left, signed for secondary screens.
    pub x: i32,
    /// Vertical offset from primary top-left, signed for secondary screens.
    pub y: i32,
    /// Outer frame width in logical points.
    pub width: u32,
    /// Outer frame height in logical points.
    pub height: u32,
}

/// One display captured by the macOS UI loop before the Host Init milestone.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq)]
pub struct AppDesktopDisplay {
    /// Native nonzero u32 display identity, not an authorization identifier.
    pub id: u32,
    /// Full display frame in logical points.
    pub bounds: AppDesktopRect,
    /// Actual usable display frame in logical points.
    pub work_area: AppDesktopRect,
    /// Positive finite backing scale.
    pub scale_factor: f64,
}

/// Owned copy of the native UI loop's current complete desktop capture.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq)]
pub struct AppDesktopFacts {
    /// Native primary display identity.
    pub primary_id: u32,
    /// One to 32 unique native displays.
    pub displays: Vec<AppDesktopDisplay>,
    /// Resolved light/dark effective application appearance.
    pub should_use_dark_colors: bool,
}

/// Only the initial renderer owns the original startup navigation deadline.
/// Registry-created pages can appear arbitrarily later and never rearm it.
#[cfg(any(target_os = "macos", test))]
pub(crate) const fn initial_renderer_pending(registry_managed: bool, closing: bool) -> bool {
    !registry_managed && !closing
}

#[cfg(test)]
mod initial_renderer_tests {
    use std::time::{Duration, Instant};

    use super::initial_renderer_pending;

    #[test]
    fn late_registry_windows_never_inherit_the_initial_renderers_expired_clock() {
        let deadline = Instant::now();
        let much_later = deadline + Duration::from_secs(60);
        for (registry_managed, closing, expected_live_initial_renderer) in [
            (false, false, true),
            (false, true, false),
            (true, false, false),
            (true, true, false),
        ] {
            assert_eq!(
                crate::navigation_deadline_expired(
                    false,
                    initial_renderer_pending(registry_managed, closing),
                    false,
                    much_later,
                    deadline,
                ),
                expected_live_initial_renderer,
                "only an unclosed legacy renderer can own the expired startup clock",
            );
        }
        assert!(
            !crate::navigation_deadline_expired(false, false, false, much_later, deadline,),
            "a windowless facade has no initial renderer deadline"
        );
    }
}

/// The only permitted source of the initial Host-to-app Ready event.
/// Renderer-declared apps keep their historical navigation milestone.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppReadyTrigger {
    /// Renderer-declared boot becomes Ready only after first navigation.
    NavigationReady,
    /// macOS facade boot becomes Ready on the actual UI-loop Init turn.
    HostInitialized,
}

impl Default for WebviewSpec {
    /// A 960x640 window titled "Keld" on a blank page — the Phase 1 hello
    /// slice geometry.
    fn default() -> Self {
        Self {
            title: String::from("Keld"),
            size: LogicalSize {
                width: 960.0,
                height: 640.0,
            },
            initial: NavTarget::Html(String::new()),
        }
    }
}

/// The engine contract every platform backend implements.
///
/// Backends: `crate::wkwebview` (macOS, live), `crate::webview2` (Windows,
/// live — KEL-27/KEL-65) and `crate::webkitgtk` (Linux, live; KEL-28 runtime
/// product acceptance still open). See the module docs above for how this v0
/// surface relates to the spec 05 §1 sketch.
/// Trait changes require design review (crate `AGENTS.md`).
pub trait WebEngine {
    /// Creates a webview (and, in v0, its host window) from `spec`.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::Window`] or [`WvError::Webview`] when platform
    /// window/webview creation fails, or [`WvError::EventLoop`] when the
    /// backend can no longer create views (e.g. its run loop already owns
    /// the thread).
    fn create(&mut self, spec: &WebviewSpec) -> Result<WebviewId, WvError>;

    /// Points the webview at new content.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`, or
    /// [`WvError::Navigate`] when the platform load call fails.
    fn navigate(&mut self, id: WebviewId, target: NavTarget) -> Result<(), WvError>;

    /// Evaluates `script` in the webview's top frame, fire-and-forget.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`, or
    /// [`WvError::Script`] when the platform refuses the script.
    fn eval(&mut self, id: WebviewId, script: &str) -> Result<(), WvError>;

    /// Repositions/resizes the webview. In v0 the webview fills its host
    /// window, so `rect` applies to the window (outer position, inner size).
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`.
    fn set_bounds(&mut self, id: WebviewId, rect: Rect) -> Result<(), WvError>;

    /// Opens or closes the inspector for the webview.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`.
    fn devtools(&mut self, id: WebviewId, action: DevtoolsAction) -> Result<(), WvError>;

    /// Tears down the webview and, in v0, its host window.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::UnknownWebview`] for a stale `id`.
    fn destroy(&mut self, id: WebviewId) -> Result<(), WvError>;
}

/// macOS (`WKWebView`) engine extensions.
///
/// Marker-level in v0: it fixes the shape wry proves out with
/// `WebViewExtMacOS` (`competitors/wry/src/lib.rs`) — native handle access,
/// reparenting, traffic-light insets, print options. Methods land as the
/// backend grows direct objc2 bindings (spec 05 §1); the trait definition is
/// platform-neutral so every platform's clippy pass sees it.
pub trait WkWebViewEngineExt: WebEngine {}

/// Windows (`WebView2`) engine extensions — implemented under KEL-27.
///
/// Marker-level in v0: fixes the shape wry proves out with
/// `WebViewExtWindows`/`WebViewBuilderExtWindows` — controller/environment
/// options, additional browser arguments, memory-usage target. The trait
/// definition is platform-neutral so every platform's clippy pass sees it.
pub trait WebView2EngineExt: WebEngine {}

/// Linux (`WebKitGTK`) engine extensions — implemented under KEL-28.
///
/// Marker-level in v0: fixes the shape wry proves out with
/// `WebViewExtUnix`/`WebViewBuilderExtUnix` — attaching the webview into an
/// existing GTK container instead of owning the window, plus the
/// degraded-rendering probe required by crate `AGENTS.md`. The trait
/// definition is platform-neutral so every platform's clippy pass sees it.
pub trait WebKitGtkEngineExt: WebEngine {}

#[cfg(test)]
mod tests {
    use super::{LogicalSize, NavTarget, WebviewSpec};

    #[test]
    fn webview_spec_default_matches_hello_slice() {
        let spec = WebviewSpec::default();
        assert_eq!(spec.title, "Keld");
        assert_eq!(
            spec.size,
            LogicalSize {
                width: 960.0,
                height: 640.0
            }
        );
        assert_eq!(spec.initial, NavTarget::Html(String::new()));
    }

    #[test]
    fn nav_target_variants_carry_their_payloads() {
        let html = NavTarget::Html(String::from("<h1>hi</h1>"));
        let url = NavTarget::Url(String::from("https://keld.dev"));
        assert_eq!(html, NavTarget::Html(String::from("<h1>hi</h1>")));
        assert_eq!(url, NavTarget::Url(String::from("https://keld.dev")));
        assert_ne!(html, url);
    }
}
