//! Hello-window startup phases (KEL-62).
//!
//! Architecture 01 §5 budgets **cold start → first paint**, not a titled native
//! window becoming observable (`window-visible` / `MainWindowHandle`). That
//! HWND metric fires during `WebViewBuilder::build` on Windows — before
//! content paints — and is not comparable to a framework that surfaces its
//! window earlier relative to webview construction.
//!
//! v0 first paint is wry `PageLoadEvent::Finished` (navigation completed).
//! `PageLoadEvent::Started` is navigation begin and MUST NOT count.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// Ordered hello-window phases. `FirstPaint` is not window creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupPhase {
    /// tao `EventLoop::new` returned.
    EventLoop,
    /// Host window exists (`WindowBuilder::build`).
    WindowCreated,
    /// Webview attached (`WebViewBuilder::build`).
    WebviewAttached,
    /// Document finished loading (wry `PageLoadEvent::Finished`).
    FirstPaint,
}

impl StartupPhase {
    /// Wire/report name. `first_paint` is never `window_visible`.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::EventLoop => "event_loop",
            Self::WindowCreated => "window_created",
            Self::WebviewAttached => "webview_attached",
            Self::FirstPaint => "first_paint",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::EventLoop => 0,
            Self::WindowCreated => 1,
            Self::WebviewAttached => 2,
            Self::FirstPaint => 3,
        }
    }
}

/// wry page-load kinds without taking a wry dependency on every OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageLoad {
    /// Navigation began. Not first paint.
    Started,
    /// Navigation completed. v0 first-paint oracle.
    Finished,
}

/// Maps a page-load event to a startup phase.
///
/// `Started` is not first paint — treating it as paint would reintroduce the
/// KEL-62 `window-visible` artifact (too early).
#[must_use]
pub(crate) fn phase_for_page_load(event: PageLoad) -> Option<StartupPhase> {
    match event {
        PageLoad::Started => None,
        PageLoad::Finished => Some(StartupPhase::FirstPaint),
    }
}

/// Whether `KELD_STARTUP_TRACE` should dump the report to stderr.
#[must_use]
pub(crate) fn trace_enabled_from(value: Option<&OsStr>) -> bool {
    match value {
        None => false,
        Some(v) if v.is_empty() || v == "0" => false,
        Some(_) => true,
    }
}

/// `KELD_STARTUP_TRACE` from the process environment.
#[must_use]
pub(crate) fn trace_enabled() -> bool {
    trace_enabled_from(std::env::var_os("KELD_STARTUP_TRACE").as_deref())
}

/// Monotonic offsets from construction. No heap on the mark path.
#[derive(Debug)]
pub(crate) struct StartupTrace {
    origin: Instant,
    offsets: [Option<Duration>; 4],
}

impl StartupTrace {
    /// Starts the clock. Call before `EventLoop::new`.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            origin: Instant::now(),
            offsets: [None; 4],
        }
    }

    /// Records `phase` once (first write wins).
    pub(crate) fn mark(&mut self, phase: StartupPhase) {
        let slot = &mut self.offsets[phase.index()];
        if slot.is_none() {
            *slot = Some(self.origin.elapsed());
        }
    }

    /// Records first paint only for [`PageLoad::Finished`].
    pub(crate) fn on_page_load(&mut self, event: PageLoad) {
        if let Some(phase) = phase_for_page_load(event) {
            self.mark(phase);
        }
    }

    /// Offset for `phase` if it has been marked.
    #[must_use]
    pub(crate) fn offset(&self, phase: StartupPhase) -> Option<Duration> {
        self.offsets[phase.index()]
    }

    /// One-line dump: `event_loop=…ms window_created=…ms … first_paint=…ms`.
    ///
    /// Missing phases are `never`. The string never contains `window_visible`.
    #[must_use]
    pub(crate) fn report(&self) -> String {
        let mut out = String::from("KELD_STARTUP");
        for phase in [
            StartupPhase::EventLoop,
            StartupPhase::WindowCreated,
            StartupPhase::WebviewAttached,
            StartupPhase::FirstPaint,
        ] {
            out.push(' ');
            out.push_str(phase.as_str());
            out.push('=');
            match self.offset(phase) {
                Some(d) => {
                    let _ = write!(out, "{}ms", d.as_millis());
                }
                None => out.push_str("never"),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::{PageLoad, StartupPhase, StartupTrace, phase_for_page_load, trace_enabled_from};

    #[test]
    fn started_is_not_first_paint() {
        assert_eq!(phase_for_page_load(PageLoad::Started), None);
        let mut trace = StartupTrace::new();
        trace.mark(StartupPhase::WindowCreated);
        trace.on_page_load(PageLoad::Started);
        assert!(
            trace.offset(StartupPhase::FirstPaint).is_none(),
            "KEL-62: PageLoad::Started must not count as first paint"
        );
        assert!(
            trace.offset(StartupPhase::WindowCreated).is_some(),
            "window_created must stay a distinct phase"
        );
    }

    #[test]
    fn finished_is_first_paint_not_window_created() {
        assert_eq!(
            phase_for_page_load(PageLoad::Finished),
            Some(StartupPhase::FirstPaint)
        );
        assert_ne!(
            StartupPhase::FirstPaint.as_str(),
            StartupPhase::WindowCreated.as_str()
        );
        assert_ne!(StartupPhase::FirstPaint.as_str(), "window_visible");

        let mut trace = StartupTrace::new();
        trace.mark(StartupPhase::WindowCreated);
        trace.mark(StartupPhase::WebviewAttached);
        trace.on_page_load(PageLoad::Finished);
        let window = trace
            .offset(StartupPhase::WindowCreated)
            .expect("window_created");
        let paint = trace.offset(StartupPhase::FirstPaint).expect("first_paint");
        assert!(
            paint >= window,
            "first_paint must not precede window_created: {paint:?} vs {window:?}"
        );
        let report = trace.report();
        assert!(report.contains("first_paint="), "{report}");
        assert!(report.contains("window_created="), "{report}");
        assert!(
            !report.contains("window_visible"),
            "KEL-62: report must not use the HWND metric name: {report}"
        );
    }

    #[test]
    fn first_paint_first_write_wins() {
        let mut trace = StartupTrace::new();
        trace.on_page_load(PageLoad::Finished);
        let first = trace.offset(StartupPhase::FirstPaint).expect("first");
        trace.on_page_load(PageLoad::Finished);
        assert_eq!(trace.offset(StartupPhase::FirstPaint), Some(first));
    }

    #[test]
    fn missing_first_paint_is_never_not_window_created() {
        let mut trace = StartupTrace::new();
        trace.mark(StartupPhase::EventLoop);
        let report = trace.report();
        assert!(report.contains("first_paint=never"), "{report}");
        assert!(report.contains("event_loop="), "{report}");
        assert!(report.starts_with("KELD_STARTUP "), "{report}");
    }

    #[test]
    fn trace_env_is_opt_in() {
        assert!(!trace_enabled_from(None));
        assert!(!trace_enabled_from(Some(OsStr::new(""))));
        assert!(!trace_enabled_from(Some(OsStr::new("0"))));
        assert!(trace_enabled_from(Some(OsStr::new("1"))));
        assert!(trace_enabled_from(Some(OsStr::new("true"))));
    }
}
