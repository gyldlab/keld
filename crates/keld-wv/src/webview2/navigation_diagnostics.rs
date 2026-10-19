//! Bounded, monotonic observations of first `WebView2` navigation (GH-814).
//! No URLs, profile paths, origins, or renderer content enter diagnostics.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use windows::core::Result as ComResult;

pub(super) struct FirstNavigationDiagnostic {
    created: Instant,
    dispatch_attempted: Option<Instant>,
    navigation_started: Option<Instant>,
    dom_content_loaded: Option<Instant>,
    dom_id_error: Option<String>,
    dom_registration_error: Option<String>,
    // Opaque WebView2 IDs stay in memory only; snapshots expose equality,
    // never a URL, HTML, a profile path, or an individual navigation ID.
    starting_navigation_id: Option<u64>,
    completed_navigation_id: Option<u64>,
    starting_id_error: Option<String>,
    completed_id_error: Option<String>,
    loop_setup_started: Option<Instant>,
    first_loop_turn: Option<Instant>,
    terminal: bool,
    phase: &'static str,
    phase_at: Instant,
    detail: Option<String>,
}

impl FirstNavigationDiagnostic {
    pub(super) fn new() -> Self {
        let now = Instant::now();
        Self {
            created: now,
            dispatch_attempted: None,
            navigation_started: None,
            dom_content_loaded: None,
            dom_id_error: None,
            dom_registration_error: None,
            starting_navigation_id: None,
            completed_navigation_id: None,
            starting_id_error: None,
            completed_id_error: None,
            loop_setup_started: None,
            first_loop_turn: None,
            terminal: false,
            phase: "window-creation",
            phase_at: now,
            detail: None,
        }
    }

    pub(super) fn dispatch_attempted(&mut self) {
        if !self.terminal {
            self.dispatch_attempted = Some(Instant::now());
            self.record("navigation-dispatch-attempted");
        }
    }

    // An actual WebView2 NavigationStarting COM callback is distinct from
    // Keld calling NavigateToString and from Tao entering its first loop turn.
    // Only the first callback is relevant to startup diagnostics.
    pub(super) fn navigation_starting(&mut self, navigation_id: ComResult<u64>) {
        if self.dispatch_attempted.is_some() && self.navigation_started.is_none() && !self.terminal
        {
            self.navigation_started = Some(Instant::now());
            match navigation_id {
                Ok(id) => self.starting_navigation_id = Some(id),
                Err(error) => {
                    self.starting_id_error = Some(format!("{:#010x}", error.code().0));
                }
            }
            self.record("navigation-starting");
        }
    }

    /// A real `WebView2` `DOMContentLoaded` callback means that the initial HTML
    /// was parsed, ahead of the image/resource loads needed for `NavigationCompleted`.
    /// Require the same opaque `NavigationId` as the first `NavigationStarting`:
    /// unrelated documents cannot impersonate the app's initial DOM milestone.
    pub(super) fn dom_content_loaded(&mut self, navigation_id: ComResult<u64>) {
        if self.dispatch_attempted.is_none() || self.terminal || self.dom_content_loaded.is_some() {
            return;
        }
        match navigation_id {
            Ok(id) if self.starting_navigation_id == Some(id) => {
                self.dom_content_loaded = Some(Instant::now());
                self.record("dom-content-loaded");
            }
            Err(error) => self.dom_id_error = Some(format!("{:#010x}", error.code().0)),
            Ok(_) => {}
        }
    }

    /// A diagnostic subscription failure must remain visible while the actual
    /// guarded initial navigation continues on its existing contract.
    pub(super) fn dom_registration_failed(&mut self, error: &windows::core::Error) {
        self.dom_registration_error = Some(format!("{:#010x}", error.code().0));
    }

    /// Observe the completion's opaque navigation identity before committing
    /// its existing result. Ignore pre-dispatch callbacks: they cannot belong
    /// to the initial app navigation and must not become its identity witness.
    pub(super) fn navigation_completed_id(&mut self, navigation_id: ComResult<u64>) {
        if self.terminal || self.dispatch_attempted.is_none() {
            return;
        }
        // A later callback must not inherit an earlier completion's ID or
        // getter error when WebView2 reports another navigation.
        self.completed_navigation_id = None;
        self.completed_id_error = None;
        match navigation_id {
            Ok(id) => self.completed_navigation_id = Some(id),
            Err(error) => {
                self.completed_id_error = Some(format!("{:#010x}", error.code().0));
            }
        }
    }

    pub(super) fn loop_setup_started(&mut self) {
        self.loop_setup_started = Some(Instant::now());
        if !self.terminal {
            self.record("message-loop-setup-started");
        }
    }

    // Record exactly the first Tao callback; the loop can be constructed long
    // before Windows actually enters its first message-pump turn.
    pub(super) fn first_loop_turn(&mut self) {
        if self.first_loop_turn.is_none() {
            self.first_loop_turn = Some(Instant::now());
        }
    }

    pub(super) fn deadline_elapsed(&mut self) {
        if !self.terminal {
            self.terminal = true;
            self.record("navigation-deadline-elapsed");
        }
    }

    fn record(&mut self, phase: &'static str) {
        self.phase = phase;
        self.phase_at = Instant::now();
    }

    /// Returns true only for the first successful navigation notification.
    /// Any COM failure is a typed startup failure, never an absent callback.
    pub(super) fn completed(
        &mut self,
        result: ComResult<bool>,
        web_error: Option<String>,
        ready: &AtomicBool,
        failed: &AtomicBool,
    ) -> bool {
        // Ignore callbacks received before the application dispatches its
        // initial navigation; do not mark readiness or finish the startup.
        if self.terminal || self.dispatch_attempted.is_none() {
            return false;
        }
        // WebView2 can interleave completions for different navigations.
        // A proven unequal pair must not mark the initial app navigation
        // ready, nor consume its terminal latch. Missing IDs preserve the
        // existing callback result handling and original startup deadline.
        if matches!(
            (self.starting_navigation_id, self.completed_navigation_id),
            (Some(start), Some(completed)) if start != completed
        ) {
            self.record("navigation-completion-id-mismatch");
            return false;
        }
        self.terminal = true;
        match result {
            Ok(true) => {
                self.record("navigation-completed");
                !ready.swap(true, Ordering::AcqRel)
            }
            Ok(false) => {
                self.record("navigation-failed");
                self.detail = web_error.map(|status| format!("web_error={status}"));
                if !ready.load(Ordering::Acquire) {
                    failed.store(true, Ordering::Release);
                }
                false
            }
            Err(error) => {
                self.record("navigation-completed-com-error");
                self.detail = Some(format!("is_success_hresult={:#010x}", error.code().0));
                if !ready.load(Ordering::Acquire) {
                    failed.store(true, Ordering::Release);
                }
                false
            }
        }
    }

    pub(super) fn snapshot(&self) -> String {
        let now = Instant::now();
        let since_created = now.saturating_duration_since(self.created).as_millis();
        let dispatch_attempt_ms = self
            .dispatch_attempted
            .map(|at| at.saturating_duration_since(self.created).as_millis());
        let navigation_start_ms = self
            .navigation_started
            .map(|at| at.saturating_duration_since(self.created).as_millis());
        let dom_content_loaded_ms = self
            .dom_content_loaded
            .map(|at| at.saturating_duration_since(self.created).as_millis());
        let loop_ms = self
            .loop_setup_started
            .map(|at| at.saturating_duration_since(self.created).as_millis());
        let first_loop_turn_ms = self
            .first_loop_turn
            .map(|at| at.saturating_duration_since(self.created).as_millis());
        let phase_ms = self
            .phase_at
            .saturating_duration_since(self.created)
            .as_millis();
        let detail = self.detail.as_deref().unwrap_or("none");
        // Equality, not individual IDs, is the only identity data emitted.
        // None means a callback or COM identity getter was unavailable.
        let navigation_ids_match = self
            .starting_navigation_id
            .zip(self.completed_navigation_id)
            .map(|(start, completed)| start == completed);
        let start_id_error = self.starting_id_error.as_deref().unwrap_or("none");
        let completed_id_error = self.completed_id_error.as_deref().unwrap_or("none");
        let dom_id_error = self.dom_id_error.as_deref().unwrap_or("none");
        let dom_registration_error = self.dom_registration_error.as_deref().unwrap_or("none");
        format!(
            "first-navigation phase={} phase_at_ms={} total_ms={} dispatch_attempt_at_ms={:?} navigation_start_at_ms={:?} dom_content_loaded_at_ms={:?} loop_setup_at_ms={:?} first_loop_turn_at_ms={:?} detail={} navigation_ids_match={:?} navigation_start_id_hresult={} navigation_completed_id_hresult={} dom_id_hresult={} dom_registration_hresult={}",
            self.phase,
            phase_ms,
            since_created,
            dispatch_attempt_ms,
            navigation_start_ms,
            dom_content_loaded_ms,
            loop_ms,
            first_loop_turn_ms,
            detail,
            navigation_ids_match,
            start_id_error,
            completed_id_error,
            dom_id_error,
            dom_registration_error
        )
    }
}

#[cfg(test)]
mod tests {
    use super::FirstNavigationDiagnostic;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn dom_content_phase_is_distinct_from_full_navigation_and_id_bound() {
        let mut diag = FirstNavigationDiagnostic::new();
        diag.dom_content_loaded(Ok(24));
        assert!(diag.snapshot().contains("dom_content_loaded_at_ms=None"));
        diag.dispatch_attempted();
        diag.navigation_starting(Ok(25));
        diag.dom_content_loaded(Ok(24));
        assert!(diag.snapshot().contains("dom_content_loaded_at_ms=None"));
        diag.dom_content_loaded(Ok(25));
        let html_parsed = diag.dom_content_loaded;
        let snapshot = diag.snapshot();
        assert!(
            snapshot.contains("dom_content_loaded_at_ms=Some("),
            "{snapshot}"
        );
        assert!(snapshot.contains("phase=dom-content-loaded"), "{snapshot}");
        assert!(snapshot.contains("navigation_ids_match=None"), "{snapshot}");
        assert!(!snapshot.contains("http://"), "{snapshot}");

        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        diag.navigation_completed_id(Ok(25));
        assert!(diag.completed(Ok(true), None, &ready, &failed));
        assert!(ready.load(Ordering::Acquire));
        assert_eq!(diag.dom_content_loaded, html_parsed);
        diag.dom_content_loaded(Ok(25));
        assert_eq!(diag.dom_content_loaded, html_parsed);
    }

    #[test]
    fn dom_identity_getter_failure_is_not_false_html_parse() {
        let mut diag = FirstNavigationDiagnostic::new();
        diag.dispatch_attempted();
        diag.navigation_starting(Ok(7));
        diag.dom_content_loaded(Err(windows::core::Error::from(
            windows::Win32::Foundation::E_POINTER,
        )));
        let snapshot = diag.snapshot();
        assert!(
            snapshot.contains("dom_content_loaded_at_ms=None"),
            "{snapshot}"
        );
        assert!(snapshot.contains("dom_id_hresult=0x80004003"), "{snapshot}");
        diag.dom_registration_failed(&windows::core::Error::from(
            windows::Win32::Foundation::E_POINTER,
        ));
        let snapshot = diag.snapshot();
        assert!(
            snapshot.contains("dom_registration_hresult=0x80004003"),
            "{snapshot}"
        );
    }

    #[test]
    fn first_tao_turn_is_a_distinct_observation_from_loop_setup() {
        let mut diag = FirstNavigationDiagnostic::new();
        diag.dispatch_attempted();
        diag.loop_setup_started();
        let before = diag.snapshot();
        assert!(
            before.contains("first_loop_turn_at_ms=None"),
            "the first Tao callback must be distinguishable from merely creating its loop: {before}"
        );
        diag.first_loop_turn();
        let first = diag.first_loop_turn;
        let after = diag.snapshot();
        assert!(after.contains("first_loop_turn_at_ms=Some("), "{after}");
        diag.first_loop_turn();
        assert_eq!(
            diag.first_loop_turn, first,
            "later callbacks cannot rewrite the first-turn clock"
        );
    }

    #[test]
    fn navigation_start_callback_is_distinct_from_dispatch_and_first_loop_turn() {
        let mut diag = FirstNavigationDiagnostic::new();
        diag.navigation_starting(Ok(17));
        let undispatched = diag.snapshot();
        assert!(
            undispatched.contains("navigation_start_at_ms=None"),
            "a pre-dispatch callback must not impersonate the app's initial navigation: {undispatched}"
        );
        diag.dispatch_attempted();
        let before = diag.snapshot();
        assert!(before.contains("navigation_start_at_ms=None"), "{before}");
        assert!(before.contains("first_loop_turn_at_ms=None"), "{before}");

        diag.navigation_starting(Ok(17));
        let first_start = diag.navigation_started;
        let during = diag.snapshot();
        assert!(during.contains("phase=navigation-starting"), "{during}");
        assert!(during.contains("navigation_start_at_ms=Some("), "{during}");
        assert!(during.contains("first_loop_turn_at_ms=None"), "{during}");
        diag.navigation_starting(Ok(17));
        assert_eq!(diag.navigation_started, first_start);

        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        assert!(diag.completed(Ok(true), None, &ready, &failed));
        let completed = diag.snapshot();
        assert!(
            completed.contains("navigation_start_at_ms=Some("),
            "{completed}"
        );
        assert!(
            completed.contains("phase=navigation-completed"),
            "{completed}"
        );
        assert!(!completed.contains("http://"), "{completed}");
    }

    #[test]
    fn late_navigation_start_does_not_replace_a_terminal_startup_phase() {
        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        let mut completed = FirstNavigationDiagnostic::new();
        completed.dispatch_attempted();
        assert!(completed.completed(Ok(true), None, &ready, &failed));
        completed.navigation_starting(Ok(17));
        let snapshot = completed.snapshot();
        assert!(
            snapshot.contains("phase=navigation-completed"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("navigation_start_at_ms=None"),
            "{snapshot}"
        );

        // A synchronous COM completion may precede Tao run-loop setup.
        // That later setup must never erase the terminal result and permit
        // an unrelated NavigationStarting callback to impersonate startup.
        completed.loop_setup_started();
        completed.navigation_starting(Ok(17));
        let snapshot = completed.snapshot();
        assert!(
            snapshot.contains("phase=navigation-completed"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("navigation_start_at_ms=None"),
            "{snapshot}"
        );
        assert!(snapshot.contains("loop_setup_at_ms=Some("), "{snapshot}");

        let mut expired = FirstNavigationDiagnostic::new();
        expired.dispatch_attempted();
        expired.deadline_elapsed();
        expired.navigation_starting(Ok(17));
        let snapshot = expired.snapshot();
        assert!(
            snapshot.contains("phase=navigation-deadline-elapsed"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("navigation_start_at_ms=None"),
            "{snapshot}"
        );

        expired.loop_setup_started();
        expired.navigation_starting(Ok(17));
        let snapshot = expired.snapshot();
        assert!(
            snapshot.contains("phase=navigation-deadline-elapsed"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("navigation_start_at_ms=None"),
            "{snapshot}"
        );
    }

    #[test]
    fn navigation_ids_are_correlated_without_exposing_opaque_identifiers() {
        let mut matching = FirstNavigationDiagnostic::new();
        matching.dispatch_attempted();
        matching.navigation_starting(Ok(872_341_901));
        matching.navigation_completed_id(Ok(872_341_901));
        let match_evidence = matching.snapshot();
        assert!(
            match_evidence.contains("navigation_ids_match=Some(true)"),
            "{match_evidence}"
        );
        assert!(!match_evidence.contains("872341901"), "{match_evidence}");

        let mut mismatched = FirstNavigationDiagnostic::new();
        mismatched.dispatch_attempted();
        mismatched.navigation_starting(Ok(41));
        mismatched.navigation_completed_id(Ok(42));
        assert!(
            mismatched
                .snapshot()
                .contains("navigation_ids_match=Some(false)"),
            "a different navigation must not appear correlated"
        );

        let mut unavailable = FirstNavigationDiagnostic::new();
        unavailable.dispatch_attempted();
        unavailable.navigation_starting(Err(windows::core::Error::from(
            windows::Win32::Foundation::E_POINTER,
        )));
        unavailable.navigation_completed_id(Ok(42));
        let unavailable_evidence = unavailable.snapshot();
        assert!(
            unavailable_evidence.contains("navigation_ids_match=None"),
            "{unavailable_evidence}"
        );
        assert!(
            unavailable_evidence.contains("navigation_start_id_hresult=0x80004003"),
            "{unavailable_evidence}"
        );
    }

    #[test]
    fn pre_dispatch_completion_cannot_claim_initial_ready_or_terminal_failure() {
        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        let mut diagnostic = FirstNavigationDiagnostic::new();

        // A WebView2 callback unrelated to app navigation can arrive after
        // handler registration but before shipping navigate_initial().
        diagnostic.navigation_completed_id(Ok(101));
        assert!(!diagnostic.completed(Ok(true), None, &ready, &failed));
        assert!(!ready.load(Ordering::Acquire));
        assert!(!failed.load(Ordering::Acquire));
        assert!(!diagnostic.terminal);
        assert!(diagnostic.completed_navigation_id.is_none());

        assert!(!diagnostic.completed(
            Ok(false),
            Some(String::from("CONNECTION_ABORTED")),
            &ready,
            &failed,
        ));
        assert!(!ready.load(Ordering::Acquire));
        assert!(!failed.load(Ordering::Acquire));
        assert!(!diagnostic.terminal);

        // Pre-dispatch callbacks must not consume the terminal latch. The
        // first dispatched app navigation still gets its actual outcome.
        diagnostic.dispatch_attempted();
        diagnostic.navigation_starting(Ok(202));
        diagnostic.navigation_completed_id(Ok(202));
        assert!(diagnostic.completed(Ok(true), None, &ready, &failed));
        assert!(ready.load(Ordering::Acquire));
        assert!(!failed.load(Ordering::Acquire));
        assert!(
            diagnostic
                .snapshot()
                .contains("navigation_ids_match=Some(true)"),
            "the real app event pair must still correlate"
        );
    }

    #[test]
    fn mismatched_post_dispatch_completion_cannot_claim_initial_navigation() {
        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        let mut diagnostic = FirstNavigationDiagnostic::new();
        diagnostic.dispatch_attempted();
        diagnostic.navigation_starting(Ok(41));
        diagnostic.navigation_completed_id(Ok(42));

        assert!(
            !diagnostic.completed(Ok(true), None, &ready, &failed),
            "another navigation's completion must not commit initial ready"
        );
        assert!(!ready.load(Ordering::Acquire));
        assert!(!failed.load(Ordering::Acquire));
        assert!(!diagnostic.terminal);
        assert!(
            diagnostic
                .snapshot()
                .contains("navigation_ids_match=Some(false)"),
            "opaque mismatch remains observable without exposing IDs"
        );

        diagnostic.navigation_completed_id(Ok(41));
        assert!(diagnostic.completed(Ok(true), None, &ready, &failed));
        assert!(ready.load(Ordering::Acquire));
        assert!(!failed.load(Ordering::Acquire));

        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        let mut diagnostic = FirstNavigationDiagnostic::new();
        diagnostic.dispatch_attempted();
        diagnostic.navigation_starting(Ok(71));
        diagnostic.navigation_completed_id(Ok(72));
        assert!(!diagnostic.completed(
            Ok(false),
            Some(String::from("CONNECTION_ABORTED")),
            &ready,
            &failed
        ));
        assert!(!failed.load(Ordering::Acquire));
        assert!(!diagnostic.terminal);

        diagnostic.navigation_completed_id(Ok(71));
        assert!(!diagnostic.completed(
            Ok(false),
            Some(String::from("CONNECTION_ABORTED")),
            &ready,
            &failed
        ));
        assert!(failed.load(Ordering::Acquire));
        assert!(!ready.load(Ordering::Acquire));
    }

    #[test]
    fn navigation_id_getter_error_clears_stale_completion_identity() {
        let mut diag = FirstNavigationDiagnostic::new();
        diag.dispatch_attempted();
        diag.navigation_starting(Ok(91));
        diag.navigation_completed_id(Ok(92));
        assert!(diag.snapshot().contains("navigation_ids_match=Some(false)"));

        // A separate callback with a failed getter must not inherit the prior
        // mismatched ID. The real callback's COM failure remains terminal.
        diag.navigation_completed_id(Err(windows::core::Error::from(
            windows::Win32::Foundation::E_POINTER,
        )));
        let snapshot = diag.snapshot();
        assert!(snapshot.contains("navigation_ids_match=None"), "{snapshot}");
        assert!(
            snapshot.contains("navigation_completed_id_hresult=0x80004003"),
            "{snapshot}"
        );
        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        assert!(!diag.completed(
            Err(windows::core::Error::from(
                windows::Win32::Foundation::E_POINTER
            )),
            None,
            &ready,
            &failed,
        ));
        assert!(failed.load(Ordering::Acquire));
        assert!(!ready.load(Ordering::Acquire));
    }

    #[test]
    fn phase_names_distinguish_dispatch_attempt_from_loop_setup() {
        let mut diag = FirstNavigationDiagnostic::new();
        diag.dispatch_attempted();
        let attempt = diag.snapshot();
        assert!(attempt.contains("navigation-dispatch-attempted"));
        assert!(attempt.contains("dispatch_attempt_at_ms=Some("));
        assert!(!attempt.contains("navigation-completed"));
        diag.loop_setup_started();
        let setup = diag.snapshot();
        assert!(setup.contains("message-loop-setup-started"));
        assert!(setup.contains("loop_setup_at_ms=Some("));
        assert!(!setup.contains("message-loop-entered"));
    }

    #[test]
    fn failed_is_success_com_hresult_is_immediately_fatal_and_recorded() {
        let mut diag = FirstNavigationDiagnostic::new();
        diag.dispatch_attempted();
        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        let com_error = windows::core::Error::from(windows::Win32::Foundation::E_POINTER);
        assert!(!diag.completed(Err(com_error), None, &ready, &failed));
        assert!(failed.load(Ordering::Acquire));
        assert!(!ready.load(Ordering::Acquire));
        let evidence = diag.snapshot();
        assert!(evidence.contains("navigation-completed-com-error"));
        assert!(
            evidence.contains("is_success_hresult=0x80004003"),
            "{evidence}"
        );
        assert!(!evidence.contains("http://"));
    }

    #[test]
    fn navigation_failure_retains_web_error_and_success_only_signals_once() {
        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        let mut diag = FirstNavigationDiagnostic::new();
        diag.dispatch_attempted();
        assert!(!diag.completed(
            Ok(false),
            Some("CONNECTION_ABORTED".to_owned()),
            &ready,
            &failed
        ));
        assert!(failed.load(Ordering::Acquire));
        assert!(diag.snapshot().contains("web_error=CONNECTION_ABORTED"));

        let ready = AtomicBool::new(false);
        let failed = AtomicBool::new(false);
        let mut diag = FirstNavigationDiagnostic::new();
        diag.dispatch_attempted();
        assert!(diag.completed(Ok(true), None, &ready, &failed));
        assert!(!diag.completed(Ok(true), None, &ready, &failed));
        assert!(ready.load(Ordering::Acquire));
        assert!(!failed.load(Ordering::Acquire));
        let com_error = windows::core::Error::from(windows::Win32::Foundation::E_POINTER);
        assert!(!diag.completed(Err(com_error), None, &ready, &failed));
        assert!(
            !failed.load(Ordering::Acquire),
            "late navigation cannot fail startup"
        );
    }
}
