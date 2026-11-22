//! Failure-only evidence for the first authenticated macOS Guardian generation.
//! This is a diagnostic observer; it never grants execution or changes a deadline.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const UNOBSERVED: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GuardianPhase {
    Kgr1Authenticated,
    Kgc1Prepared,
    BunFactoryReturned,
    BunSpawnReported,
    Kgc1Registered,
}

impl GuardianPhase {
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
        let bounded_ms = elapsed.min(u128::from(UNOBSERVED - 1)) as u64;
        self.observed_ms[phase.index()].fetch_min(bounded_ms, Ordering::Release);
    }

    fn observed_millis(&self, phase: GuardianPhase) -> Option<u64> {
        let value = self.observed_ms[phase.index()].load(Ordering::Acquire);
        (value != UNOBSERVED).then_some(value)
    }

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

    pub(super) fn failure_report(&self) -> String {
        let when = |phase| {
            self.observed_millis(phase)
                .map_or_else(|| String::from("unobserved"), |millis| millis.to_string())
        };
        // Only fixed stage identifiers and monotonic offsets appear in stderr.
        // No app-link, token, private environment, PID, command, or launch facts.
        format!(
            "KELD-CORE-037 auth-phase failure-only=true kgr1_ms={} kgc1_prepared_ms={} bun_factory_ms={} bun_spawn_reported_ms={} kgc1_registered_ms={} hello=host-unobserved init_ack=host-unobserved bound=host-unobserved first_unproven={:?}",
            when(GuardianPhase::Kgr1Authenticated),
            when(GuardianPhase::Kgc1Prepared),
            when(GuardianPhase::BunFactoryReturned),
            when(GuardianPhase::BunSpawnReported),
            when(GuardianPhase::Kgc1Registered),
            self.first_unproven(false, false),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{GuardianPhase, GuardianPhaseLedger, NextAuthBoundary};

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
        let report = ledger.failure_report();
        assert!(report.starts_with("KELD-CORE-037 auth-phase"));
        assert!(report.contains("kgc1_registered_ms="));
        assert!(report.contains("hello=host-unobserved"));
        assert!(report.contains("init_ack=host-unobserved"));
        assert!(report.contains("bound=host-unobserved"));
        assert!(!report.contains("bun_exec=true"));
        assert!(!report.contains("KELD_APP_LINK"));
    }
}
