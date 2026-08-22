//! Hello-window startup phases (KEL-62).
//!
//! Architecture 01 §5 budgets **cold start → first paint** via the KEL-64
//! external double-rAF image beacon on a monotonic clock armed before process
//! spawn. That oracle is not this module.
//!
//! This trace measures **construction and navigation-completion diagnostics**
//! on a post-`WkWebViewEngine::new` clock. A titled native window (`window-visible`
//! / `MainWindowHandle`) is the wrong metric — on Windows WebView2 it can fire
//! during `WebViewBuilder::build` before content paints (see
//! `docs/engineering/budget-scoreboard.md` § "Time to first paint", KEL-62).
//!
//! wry `PageLoadEvent::Finished` is **navigation completion**, not composited
//! first paint. `PageLoadEvent::Started` is navigation begin and MUST NOT count.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// Ordered hello-window construction phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupPhase {
    /// tao `EventLoop::new` returned.
    EventLoop,
    /// Host window exists (`WindowBuilder::build`).
    WindowCreated,
    /// Webview attached (`WebViewBuilder::build`).
    WebviewAttached,
    /// wry `PageLoadEvent::Finished` (navigation completed; not KEL-64 paint).
    NavFinished,
}

impl StartupPhase {
    /// Wire/report name. `nav_finished` is never `window_visible` or `first_paint`.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::EventLoop => "event_loop",
            Self::WindowCreated => "window_created",
            Self::WebviewAttached => "webview_attached",
            Self::NavFinished => "nav_finished",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::EventLoop => 0,
            Self::WindowCreated => 1,
            Self::WebviewAttached => 2,
            Self::NavFinished => 3,
        }
    }
}

/// wry page-load kinds without taking a wry dependency on every OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageLoad {
    /// Navigation began. Not navigation completion.
    Started,
    /// Navigation completed (wry `PageLoadEvent::Finished`).
    Finished,
}

/// Maps a page-load event to a startup phase.
///
/// `Started` is not navigation completion — treating it as such would reintroduce
/// the KEL-62 `window-visible` artifact (too early).
#[must_use]
pub(crate) fn phase_for_page_load(event: PageLoad) -> Option<StartupPhase> {
    match event {
        PageLoad::Started => None,
        PageLoad::Finished => Some(StartupPhase::NavFinished),
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

/// Whether a terminal trace dump should run because navigation never finished.
#[must_use]
pub(crate) fn should_emit_nav_never_finished(trace: &StartupTrace) -> bool {
    trace_enabled() && trace.offset(StartupPhase::NavFinished).is_none()
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

    /// Records navigation completion only for [`PageLoad::Finished`].
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

    /// One-line dump: `event_loop=…ms window_created=…ms … nav_finished=…ms`.
    ///
    /// Missing phases are `never`. The string never contains `window_visible` or
    /// architecture `first_paint`.
    #[must_use]
    pub(crate) fn report(&self) -> String {
        let mut out = String::from("KELD_STARTUP");
        for phase in [
            StartupPhase::EventLoop,
            StartupPhase::WindowCreated,
            StartupPhase::WebviewAttached,
            StartupPhase::NavFinished,
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

    /// Emits [`Self::report`] when tracing is enabled and [`StartupPhase::NavFinished`]
    /// was never marked (for example the user closed before `Finished`).
    pub(crate) fn emit_if_nav_never_finished(&self) {
        if should_emit_nav_never_finished(self) {
            eprintln!("{}", self.report());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::{
        PageLoad, StartupPhase, StartupTrace, phase_for_page_load, should_emit_nav_never_finished,
        trace_enabled_from,
    };

    #[test]
    fn started_is_not_nav_finished() {
        assert_eq!(phase_for_page_load(PageLoad::Started), None);
        let mut trace = StartupTrace::new();
        trace.mark(StartupPhase::WindowCreated);
        trace.on_page_load(PageLoad::Started);
        assert!(
            trace.offset(StartupPhase::NavFinished).is_none(),
            "KEL-62: PageLoad::Started must not count as navigation completion"
        );
        assert!(
            trace.offset(StartupPhase::WindowCreated).is_some(),
            "window_created must stay a distinct phase"
        );
    }

    #[test]
    fn finished_is_nav_finished_not_window_created() -> Result<(), String> {
        assert_eq!(
            phase_for_page_load(PageLoad::Finished),
            Some(StartupPhase::NavFinished)
        );
        assert_ne!(
            StartupPhase::NavFinished.as_str(),
            StartupPhase::WindowCreated.as_str()
        );
        assert_ne!(StartupPhase::NavFinished.as_str(), "window_visible");
        assert_ne!(StartupPhase::NavFinished.as_str(), "first_paint");

        let mut trace = StartupTrace::new();
        trace.mark(StartupPhase::WindowCreated);
        trace.mark(StartupPhase::WebviewAttached);
        trace.on_page_load(PageLoad::Finished);
        let window = trace
            .offset(StartupPhase::WindowCreated)
            .ok_or("window_created must be marked")?;
        let nav = trace
            .offset(StartupPhase::NavFinished)
            .ok_or("nav_finished must be marked after Finished")?;
        assert!(
            nav >= window,
            "nav_finished must not precede window_created: {nav:?} vs {window:?}"
        );
        let report = trace.report();
        assert!(report.contains("nav_finished="), "{report}");
        assert!(report.contains("window_created="), "{report}");
        assert!(
            !report.contains("window_visible"),
            "KEL-62: report must not use the HWND metric name: {report}"
        );
        assert!(
            !report.contains("first_paint="),
            "KEL-64 first paint is external beacon, not this trace: {report}"
        );
        Ok(())
    }

    #[test]
    fn nav_finished_first_write_wins() -> Result<(), String> {
        let mut trace = StartupTrace::new();
        trace.on_page_load(PageLoad::Finished);
        let first = trace
            .offset(StartupPhase::NavFinished)
            .ok_or("nav_finished must be set after Finished")?;
        trace.on_page_load(PageLoad::Finished);
        assert_eq!(trace.offset(StartupPhase::NavFinished), Some(first));
        Ok(())
    }

    #[test]
    fn missing_nav_finished_is_never_not_window_created() {
        let mut trace = StartupTrace::new();
        trace.mark(StartupPhase::EventLoop);
        let report = trace.report();
        assert!(report.contains("nav_finished=never"), "{report}");
        assert!(report.contains("event_loop="), "{report}");
        assert!(report.starts_with("KELD_STARTUP "), "{report}");
    }

    #[test]
    fn should_emit_nav_never_finished_when_trace_off() {
        let trace = StartupTrace::new();
        assert!(
            !should_emit_nav_never_finished(&trace),
            "opt-in trace must not dump without KELD_STARTUP_TRACE"
        );
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
