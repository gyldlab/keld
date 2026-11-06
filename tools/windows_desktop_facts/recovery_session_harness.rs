// W4 component proof: exact W2 DesktopSession and AppDesktopCapture sources.
// Only the OS acquisition and the enclosing event enum/error are test doubles.
// No Core writer, native Windows API, public Ready, or recovery policy is mocked as proof.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

#[path = "W4_CAPTURE_SOURCE"]
mod capture;
use capture::{AppDesktopCapture, AppDesktopDisplay, AppDesktopFacts, AppDesktopRect};

#[derive(Debug, PartialEq)]
enum AppWindowEvent {
    DesktopObserved {
        facts: Option<AppDesktopFacts>,
    },
    HostInitialized {
        observed_at: Instant,
    },
    WindowsDesktopObserved {
        capture: AppDesktopCapture,
        facts: Option<AppDesktopFacts>,
    },
}

#[derive(Debug)]
enum WvError {
    EventLoop(String),
}
impl std::fmt::Display for WvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EventLoop(message) => f.write_str(message),
        }
    }
}

mod webview2 {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct Query {
        value: Option<AppDesktopFacts>,
        retire_during_capture: Option<Arc<AtomicBool>>,
    }
    #[derive(Default)]
    struct Acquisition {
        queries: VecDeque<Query>,
        calls: usize,
    }
    thread_local! { static ACQUISITION: RefCell<Acquisition> = RefCell::default(); }
    mod desktop_sample {
        use super::*;
        #[derive(Default)]
        pub(super) struct DesktopSampler;
        impl DesktopSampler {
            pub(super) fn capture(&mut self) -> Result<AppDesktopFacts, WvError> {
                ACQUISITION.with(|cell| {
                    let mut state = cell.borrow_mut();
                    state.calls += 1;
                    let query = state
                        .queries
                        .pop_front()
                        .expect("unexpected physical-query request");
                    if let Some(reader) = query.retire_during_capture {
                        reader.store(true, Ordering::Release);
                    }
                    query
                        .value
                        .ok_or_else(|| WvError::EventLoop("injected acquisition failure".into()))
                })
            }
        }
    }

    // Matches the existing event-forwarding seam. This test does not execute tao's Init trigger.
    fn report_host_initialized(
        events: &mpsc::Sender<AppWindowEvent>,
        observed_at: Instant,
    ) -> bool {
        events
            .send(AppWindowEvent::HostInitialized { observed_at })
            .is_ok()
    }
    #[path = "W4_SESSION_SOURCE"]
    mod desktop_session;

    fn facts(dark: bool) -> AppDesktopFacts {
        AppDesktopFacts {
            primary_id: 17,
            displays: vec![AppDesktopDisplay {
                id: 17,
                bounds: AppDesktopRect {
                    x: 0,
                    y: 0,
                    width: 900,
                    height: 600,
                },
                work_area: AppDesktopRect {
                    x: 0,
                    y: 0,
                    width: 900,
                    height: 560,
                },
                scale_factor: 1.25,
            }],
            should_use_dark_colors: dark,
        }
    }
    fn input(value: Option<bool>, retire: Option<Arc<AtomicBool>>) {
        ACQUISITION.with(|s| {
            s.borrow_mut().queries.push_back(Query {
                value: value.map(facts),
                retire_during_capture: retire,
            })
        });
    }
    fn calls() -> usize {
        ACQUISITION.with(|s| s.borrow().calls)
    }
    fn reset() {
        ACQUISITION.with(|s| *s.borrow_mut() = Acquisition::default());
    }
    fn reader(attempt: u32) -> (AppDesktopCapture, Arc<AtomicBool>) {
        let stop = Arc::new(AtomicBool::new(false));
        (
            AppDesktopCapture::from_current(attempt, Arc::clone(&stop)),
            stop,
        )
    }
    fn values(rx: &mpsc::Receiver<AppWindowEvent>) -> Vec<AppWindowEvent> {
        rx.try_iter().collect()
    }
    fn observed(capture: &AppDesktopCapture, value: Option<bool>) -> AppWindowEvent {
        AppWindowEvent::WindowsDesktopObserved {
            capture: capture.clone(),
            facts: value.map(facts),
        }
    }
    fn boot(
        capture: &AppDesktopCapture,
    ) -> (
        desktop_session::DesktopSession,
        mpsc::Sender<AppWindowEvent>,
        mpsc::Receiver<AppWindowEvent>,
    ) {
        reset();
        let (tx, rx) = mpsc::channel();
        let mut session = desktop_session::DesktopSession::default();
        session
            .bind(capture.clone(), &tx)
            .expect("bind before Init");
        assert_eq!(calls(), 0, "binding before genuine Init does not sample");
        assert!(values(&rx).is_empty());
        input(Some(false), None);
        assert!(session.first_init(&tx, || false));
        let events = values(&rx);
        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0],
            AppWindowEvent::DesktopObserved {
                facts: Some(facts(false))
            }
        );
        assert!(matches!(events[1], AppWindowEvent::HostInitialized { .. }));
        assert_eq!(events[2], observed(capture, Some(false)));
        assert_eq!(calls(), 1);
        (session, tx, rx)
    }

    #[test]
    fn replacement_binding_reports_its_new_query_failure() {
        let (a, stop_a) = reader(7);
        let (mut session, tx, rx) = boot(&a);
        stop_a.store(true, Ordering::Release);
        let (b, stop_b) = reader(8);
        input(None, None);
        session.bind(b.clone(), &tx).expect("replacement bind");
        assert_eq!(calls(), 2, "replacement physically samples again");
        let events = values(&rx);
        assert_eq!(
            events,
            vec![observed(&b, None)],
            "failure is not replaced by the previous valid facts"
        );
        assert!(b.matches(8, &stop_b));
        assert!(!a.matches(8, &stop_b));
    }

    #[test]
    fn retired_bind_does_not_displace_current_producer() {
        let (a, stop_a) = reader(7);
        let (mut session, tx, rx) = boot(&a);
        stop_a.store(true, Ordering::Release);
        let (b, _) = reader(8);
        input(Some(false), None);
        session.bind(b.clone(), &tx).expect("B bind");
        assert_eq!(values(&rx), vec![observed(&b, Some(false))]);
        session
            .bind(a.clone(), &tx)
            .expect("late A bind is ignored");
        assert_eq!(calls(), 2);
        assert!(values(&rx).is_empty());
        input(Some(true), None);
        session.refresh(&tx).expect("B refresh after late A bind");
        assert_eq!(calls(), 3);
        assert_eq!(values(&rx), vec![observed(&b, Some(true))]);
    }

    #[test]
    fn retired_current_producer_never_requests_another_query() {
        let (a, stop_a) = reader(7);
        let (mut session, tx, rx) = boot(&a);
        stop_a.store(true, Ordering::Release);
        session.refresh(&tx).expect("retired refresh is inert");
        assert_eq!(calls(), 1);
        assert!(values(&rx).is_empty());
        let (b, _) = reader(8);
        input(Some(true), None);
        session
            .bind(b.clone(), &tx)
            .expect("live successor resumes capture");
        assert_eq!(calls(), 2);
        assert_eq!(values(&rx), vec![observed(&b, Some(true))]);
    }

    #[test]
    fn retirement_during_query_preserves_old_reader_provenance() {
        let (a, stop_a) = reader(7);
        let (mut session, tx, rx) = boot(&a);
        input(Some(true), Some(Arc::clone(&stop_a)));
        session.refresh(&tx).expect("captured completion");
        let events = values(&rx);
        assert_eq!(events, vec![observed(&a, Some(true))]);
        let AppWindowEvent::WindowsDesktopObserved { capture, .. } = &events[0] else {
            panic!("capture missing")
        };
        assert!(
            !capture.matches(7, &stop_a),
            "real identity predicate rejects the now-retired reader"
        );
        let (b, stop_b) = reader(7);
        assert!(
            !capture.matches(7, &stop_b),
            "same attempt number does not transfer the completion"
        );
        input(Some(false), None);
        session.bind(b.clone(), &tx).expect("live replacement");
        assert_eq!(values(&rx), vec![observed(&b, Some(false))]);
        assert!(b.matches(7, &stop_b));
        assert_eq!(calls(), 3);
    }

    #[test]
    fn capture_matches_requires_the_same_live_reader_even_with_equal_attempt() {
        let (a, stop_a) = reader(7);
        let (b, stop_b) = reader(7);
        assert_eq!(a, a.clone());
        assert_ne!(a, b);
        assert!(a.matches(7, &stop_a));
        assert!(!a.matches(8, &stop_a));
        assert!(!a.matches(7, &stop_b));
        stop_a.store(true, Ordering::Release);
        assert!(!a.matches(7, &stop_a));
        assert!(b.matches(7, &stop_b));
    }

    #[test]
    fn failed_then_healthy_refresh_emits_only_current_observations() {
        let (a, _) = reader(7);
        let (mut session, tx, rx) = boot(&a);
        input(None, None);
        session.refresh(&tx).expect("unavailable result");
        assert_eq!(values(&rx), vec![observed(&a, None)]);
        input(Some(true), None);
        session.refresh(&tx).expect("recovered acquisition");
        assert_eq!(values(&rx), vec![observed(&a, Some(true))]);
        assert_eq!(calls(), 3);
    }
}
