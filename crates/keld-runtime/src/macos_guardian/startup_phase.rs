//! Failure-only evidence for the first authenticated macOS Guardian generation.
//! This is a diagnostic observer; it never grants execution or changes a deadline.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const UNOBSERVED: u64 = u64::MAX;

/// The local Guardian error site, not an assertion about Host IPC admission.
/// A Supervisor terminal event can happen after an authenticated Host Bound.
#[derive(Clone, Copy)]
pub(super) enum GuardianFailureSite {
    Registration,
    InitialPreparation,
    SupervisorTerminal,
}

impl GuardianFailureSite {
    const fn label(self) -> &'static str {
        match self {
            Self::Registration => "guardian_registration",
            Self::InitialPreparation => "guardian_initial_preparation",
            Self::SupervisorTerminal => "guardian_supervisor_terminal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GuardianPhase {
    Kgr1Authenticated,
    Kgc1Prepared,
    BunFactoryReturned,
    BunSpawnReported,
    Kgc1Registered,
}

impl GuardianPhase {
    #[cfg(test)]
    const ORDER: [Self; 5] = [
        Self::Kgr1Authenticated,
        Self::Kgc1Prepared,
        Self::BunFactoryReturned,
        Self::BunSpawnReported,
        Self::Kgc1Registered,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Kgr1Authenticated => 0,
            Self::Kgc1Prepared => 1,
            Self::BunFactoryReturned => 2,
            Self::BunSpawnReported => 3,
            Self::Kgc1Registered => 4,
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NextAuthBoundary {
    GuardianKgr1,
    Kgc1Prepared,
    BunFactory,
    BunSpawnReporter,
    Kgc1Registered,
    HostHelloAuthenticated,
    HostInitAcknowledged,
    HostBound,
}

/// Monotonic local observations from the Guardian's existing authenticated path.
///
/// These process-local times do not prove a Bun exec or Host IPC authentication.
/// A later record never fills a missing earlier stage. The Host owns HELLO/Bound
/// evidence independently; only the diagnostic classifier accepts that proof.
pub(super) struct GuardianPhaseLedger {
    started: Instant,
    observed_ms: [AtomicU64; 5],
}

impl GuardianPhaseLedger {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            observed_ms: std::array::from_fn(|_| AtomicU64::new(UNOBSERVED)),
        }
    }

    pub(super) fn record(&self, phase: GuardianPhase) {
        let elapsed = self.started.elapsed().as_millis();
        let bounded_ms = u64::try_from(elapsed).unwrap_or(UNOBSERVED - 1);
        self.observed_ms[phase.index()].fetch_min(bounded_ms, Ordering::Release);
    }

    fn observed_millis(&self, phase: GuardianPhase) -> Option<u64> {
        let value = self.observed_ms[phase.index()].load(Ordering::Acquire);
        (value != UNOBSERVED).then_some(value)
    }

    #[cfg(test)]
    pub(super) fn first_unproven(
        &self,
        host_hello_observed: bool,
        host_init_ack_observed: bool,
    ) -> NextAuthBoundary {
        for (phase, missing) in GuardianPhase::ORDER.into_iter().zip([
            NextAuthBoundary::GuardianKgr1,
            NextAuthBoundary::Kgc1Prepared,
            NextAuthBoundary::BunFactory,
            NextAuthBoundary::BunSpawnReporter,
            NextAuthBoundary::Kgc1Registered,
        ]) {
            if self.observed_millis(phase).is_none() {
                return missing;
            }
        }
        // The Guardian may not infer authenticated HELLO, runtime-facts Init
        // or Bound from KGC1. Only independent Host-owned proof can advance.
        match (host_hello_observed, host_init_ack_observed) {
            (false, _) => NextAuthBoundary::HostHelloAuthenticated,
            (true, false) => NextAuthBoundary::HostInitAcknowledged,
            (true, true) => NextAuthBoundary::HostBound,
        }
    }

    /// A task-owned fixture may inspect a local stage without forging any
    /// KGR1 authentication that its synthetic stream does not exercise.
    #[cfg(test)]
    pub(super) fn test_observed(&self, phase: GuardianPhase) -> bool {
        self.observed_millis(phase).is_some()
    }

    pub(super) fn failure_report(&self, site: GuardianFailureSite) -> String {
        let when = |phase| {
            self.observed_millis(phase)
                .map_or_else(|| String::from("unobserved"), |millis| millis.to_string())
        };
        // Only fixed stage identifiers and monotonic offsets appear in stderr.
        // First-generation milestones are NOT evidence of present Host Bound
        // state or the cause of a later Supervisor crash. Never invent Host
        // HELLO/Init/Bound state from this Guardian-local ledger. The real
        // KELD-CORE-037 diagnosis is owned by Host, not this trace.
        // No app-link, token, private environment, PID, command, or launch facts.
        format!(
            "GH1404_GUARDIAN_PHASE failure-only=true scope={} observations=first-guardian-generation kgr1_ms={} kgc1_prepared_ms={} bun_factory_ms={} bun_spawn_reported_ms={} kgc1_registered_ms={} host_hello=not-measured host_init_ack=not-measured host_bound=not-measured",
            site.label(),
            when(GuardianPhase::Kgr1Authenticated),
            when(GuardianPhase::Kgc1Prepared),
            when(GuardianPhase::BunFactoryReturned),
            when(GuardianPhase::BunSpawnReported),
            when(GuardianPhase::Kgc1Registered),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{GuardianFailureSite, GuardianPhase, GuardianPhaseLedger, NextAuthBoundary};

    #[test]
    fn first_unproven_separates_prepared_factory_spawn_registered_hello_and_bound() {
        let ledger = GuardianPhaseLedger::new();
        assert_eq!(
            ledger.first_unproven(false, false),
            NextAuthBoundary::GuardianKgr1
        );
        for (phase, next) in [
            (
                GuardianPhase::Kgr1Authenticated,
                NextAuthBoundary::Kgc1Prepared,
            ),
            (GuardianPhase::Kgc1Prepared, NextAuthBoundary::BunFactory),
            (
                GuardianPhase::BunFactoryReturned,
                NextAuthBoundary::BunSpawnReporter,
            ),
            (
                GuardianPhase::BunSpawnReported,
                NextAuthBoundary::Kgc1Registered,
            ),
            (
                GuardianPhase::Kgc1Registered,
                NextAuthBoundary::HostHelloAuthenticated,
            ),
        ] {
            ledger.record(phase);
            assert_eq!(ledger.first_unproven(false, false), next, "phase={phase:?}");
        }
        // Only independent authenticated Host receipts can advance these.
        assert_eq!(
            ledger.first_unproven(true, false),
            NextAuthBoundary::HostInitAcknowledged
        );
        assert_eq!(
            ledger.first_unproven(true, true),
            NextAuthBoundary::HostBound
        );
        assert_eq!(
            ledger.first_unproven(false, true),
            NextAuthBoundary::HostHelloAuthenticated
        );
    }

    #[test]
    fn a_later_marker_cannot_fill_a_missing_prepared_or_factory_stage() {
        let ledger = GuardianPhaseLedger::new();
        ledger.record(GuardianPhase::Kgr1Authenticated);
        ledger.record(GuardianPhase::BunSpawnReported);
        assert_eq!(
            ledger.first_unproven(false, false),
            NextAuthBoundary::Kgc1Prepared
        );
        ledger.record(GuardianPhase::Kgc1Prepared);
        assert_eq!(
            ledger.first_unproven(false, false),
            NextAuthBoundary::BunFactory
        );
    }

    #[test]
    fn guardian_failure_report_does_not_invent_host_hello_bound_or_bun_exec() {
        let ledger = GuardianPhaseLedger::new();
        for stage in [
            GuardianPhase::Kgr1Authenticated,
            GuardianPhase::Kgc1Prepared,
            GuardianPhase::BunFactoryReturned,
            GuardianPhase::BunSpawnReported,
            GuardianPhase::Kgc1Registered,
        ] {
            ledger.record(stage);
        }
        let report = ledger.failure_report(GuardianFailureSite::InitialPreparation);
        assert!(report.starts_with("GH1404_GUARDIAN_PHASE failure-only=true"));
        assert!(report.contains("kgc1_registered_ms="));
        assert!(report.contains("host_hello=not-measured"));
        assert!(report.contains("host_init_ack=not-measured"));
        assert!(report.contains("host_bound=not-measured"));
        assert!(!report.contains("bun_exec=true"));
        assert!(!report.contains("KELD_APP_LINK"));
    }

    #[test]
    fn rejected_private_registration_does_not_forge_authenticated_host_state() {
        let report = GuardianPhaseLedger::new().failure_report(GuardianFailureSite::Registration);
        assert!(report.contains("scope=guardian_registration"));
        assert!(report.contains("kgr1_ms=unobserved"));
        assert!(report.contains("host_hello=not-measured"));
        assert!(!report.contains("KELD-CORE-037"));
    }

    #[test]
    fn real_kgc1_observation_cannot_fill_missing_kgr1_registration() {
        let ledger = GuardianPhaseLedger::new();
        assert!(!ledger.test_observed(GuardianPhase::Kgr1Authenticated));
        ledger.record(GuardianPhase::Kgc1Prepared);
        assert!(ledger.test_observed(GuardianPhase::Kgc1Prepared));
        assert!(
            !ledger.test_observed(GuardianPhase::Kgr1Authenticated),
            "KGC1 cannot fabricate an earlier KGR1 handshake"
        );
    }

    #[test]
    fn post_bound_supervisor_crash_must_not_be_reported_as_initial_auth_failure() {
        let ledger = GuardianPhaseLedger::new();
        for stage in GuardianPhase::ORDER {
            ledger.record(stage);
        }
        // The independent Host's already-authenticated Bound is a test premise,
        // not a capability inferred by the Guardian from local KGC1 records.
        assert_eq!(
            ledger.first_unproven(true, true),
            NextAuthBoundary::HostBound
        );
        let report = ledger.failure_report(GuardianFailureSite::SupervisorTerminal);
        assert!(
            !report.contains("KELD-CORE-037") && !report.contains("initial-auth"),
            "later Bun crash must not be relabeled a Host initial-auth failure: {report}"
        );
        assert!(
            report.contains("scope=guardian_supervisor_terminal"),
            "a later terminal failure requires explicit attribution: {report}"
        );
        assert!(
            report.contains("host_bound=not-measured"),
            "Guardian may not infer current Host Bound state: {report}"
        );
        assert!(
            !report.contains("first_unproven="),
            "a late crash is not evidence of an unproven initial HELLO: {report}"
        );
    }
}
