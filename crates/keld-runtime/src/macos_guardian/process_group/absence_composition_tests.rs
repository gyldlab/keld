//! Component composition only: no kernel signals, real processes or native custody proof.
use std::cell::Cell;
use std::time::Duration;

use super::{
    Errno, GROUP_SIGNAL_PHASE, GroupIdentity, Instant, RuntimeError,
    classify_attested_group_signal, finish_attested_group_termination, io, lifecycle_error,
    reconcile_denied_signal_after_reap_with_observers,
    require_attested_group_absence_with_observers, terminate_attested_group_with_observers,
};

fn identity() -> GroupIdentity {
    GroupIdentity {
        pid: 123,
        real_uid: 501,
        started_sec: 42,
        started_usec: 0,
    }
}

fn signal_error(errno: Errno) -> RuntimeError {
    lifecycle_error(
        GROUP_SIGNAL_PHASE,
        io::Error::from_raw_os_error(errno as i32),
    )
}

// Both routes compute the outcome through production branching and the real
// absence/classifier functions. Nothing injects an AbsenceProven enum value.
fn completed_proof_survives_return_after_deadline(denied_signal: bool) {
    let group = identity();
    let before = Instant::now();
    let deadline = before + Duration::from_secs(1);
    let clock = Cell::new(before);
    let proofs = Cell::new(0);
    let probes = Cell::new(0);
    let signals = Cell::new(0);
    let first_probes = Cell::new(0);
    let outcome = terminate_attested_group_with_observers(
        group,
        || {
            if denied_signal {
                Ok(group)
            } else {
                Err(lifecycle_error(
                    "fixture identity unavailable",
                    io::Error::from_raw_os_error(3),
                ))
            }
        },
        |_| {
            first_probes.set(first_probes.get() + 1);
            (Err(Errno::ESRCH), Err(Errno::ESRCH))
        },
        || {
            signals.set(signals.get() + 1);
            assert!(denied_signal, "fallback must not signal");
            classify_attested_group_signal(Err(signal_error(Errno::EPERM)), |denied| {
                proofs.set(proofs.get() + 1);
                let result = reconcile_denied_signal_after_reap_with_observers(
                    group,
                    denied,
                    deadline,
                    || clock.get(),
                    |_| {
                        probes.set(probes.get() + 1);
                        (Err(Errno::ESRCH), Err(Errno::ESRCH))
                    },
                );
                assert!(
                    result.is_ok(),
                    "real EPERM reconciliation must prove absence first"
                );
                clock.set(deadline + Duration::from_secs(1));
                result
            })
        },
        || {
            proofs.set(proofs.get() + 1);
            let result = require_attested_group_absence_with_observers(
                group,
                deadline,
                || clock.get(),
                |_| {
                    probes.set(probes.get() + 1);
                    (Err(Errno::ESRCH), Err(Errno::ESRCH))
                },
            );
            assert!(result.is_ok(), "real fallback must prove absence first");
            clock.set(deadline + Duration::from_secs(1));
            result
        },
    );
    assert!(clock.get() > deadline);
    let result = finish_attested_group_termination(outcome, || {
        proofs.set(proofs.get() + 1);
        require_attested_group_absence_with_observers(
            group,
            deadline,
            || clock.get(),
            |_| panic!("expired proof must not probe"),
        )
    });
    assert!(
        result.is_ok(),
        "completed proof must not be repeated after expiry: {result:?}"
    );
    assert_eq!(proofs.get(), 1);
    assert_eq!(probes.get(), 1);
    assert_eq!(signals.get(), usize::from(denied_signal));
    assert_eq!(first_probes.get(), usize::from(!denied_signal));
}

#[test]
fn fallback_absence_is_not_reproved_after_original_deadline() {
    completed_proof_survives_return_after_deadline(false);
}

#[test]
fn reconciled_eperm_absence_is_not_reproved_after_original_deadline() {
    completed_proof_survives_return_after_deadline(true);
}

#[test]
fn delivered_signal_still_requires_original_deadline_absence() {
    for expired in [false, true] {
        let group = identity();
        let before = Instant::now();
        let deadline = before + Duration::from_secs(1);
        let proofs = Cell::new(0);
        let signals = Cell::new(0);
        let outcome = terminate_attested_group_with_observers(
            group,
            || Ok(group),
            |_| panic!("birth matched"),
            || {
                signals.set(signals.get() + 1);
                classify_attested_group_signal(Ok(()), |_| panic!("signal was delivered"))
            },
            || panic!("no fallback on birth match"),
        );
        let result = finish_attested_group_termination(outcome, || {
            proofs.set(proofs.get() + 1);
            require_attested_group_absence_with_observers(
                group,
                deadline,
                || if expired { deadline } else { before },
                |_| (Err(Errno::ESRCH), Err(Errno::ESRCH)),
            )
        });
        assert_eq!(signals.get(), 1);
        assert_eq!(proofs.get(), 1, "delivery is not absence");
        assert_eq!(result.is_ok(), !expired);
        if let Err(RuntimeError::Lifecycle { phase, source }) = result {
            assert_eq!(phase, "macOS guardian generation clear group absence");
            assert_eq!(source.kind(), io::ErrorKind::TimedOut);
        } else {
            assert!(!expired);
        }
    }
}

#[test]
fn signal_failures_preserve_phase_and_do_not_gain_a_second_proof() {
    for errno in [Errno::EPERM, Errno::EINVAL] {
        let group = identity();
        let deadline = Instant::now();
        let signals = Cell::new(0);
        let outcome = terminate_attested_group_with_observers(
            group,
            || {
                Err(lifecycle_error(
                    "fixture missing leader",
                    io::Error::from_raw_os_error(3),
                ))
            },
            |_| (Err(Errno::ESRCH), Ok(())),
            || {
                signals.set(signals.get() + 1);
                classify_attested_group_signal(Err(signal_error(errno)), |denied| {
                    reconcile_denied_signal_after_reap_with_observers(
                        group,
                        denied,
                        deadline,
                        || deadline,
                        |_| panic!("expired or non-EPERM failure must not probe"),
                    )
                })
            },
            || panic!("first signallable observation uses the signal branch"),
        );
        let result = finish_attested_group_termination(outcome, || {
            panic!("failed first cleanup must not get another proof")
        });
        assert_eq!(signals.get(), 1);
        match result {
            Err(RuntimeError::Lifecycle { phase, source }) => {
                assert_eq!(phase, GROUP_SIGNAL_PHASE);
                if errno == Errno::EPERM {
                    assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
                } else {
                    assert_eq!(source.raw_os_error(), Some(errno as i32));
                }
            }
            other => panic!("expected original signal failure, got {other:?}"),
        }
    }
}

#[test]
fn changed_birth_never_signals_or_probes_absence() {
    let group = identity();
    let changed = GroupIdentity {
        started_usec: 1,
        ..group
    };
    let outcome = terminate_attested_group_with_observers(
        group,
        || Ok(changed),
        |_| panic!("changed identity must not probe"),
        || panic!("changed identity must not signal"),
        || panic!("changed identity must not prove absence"),
    );
    match finish_attested_group_termination(outcome, || panic!("identity failure is final")) {
        Err(RuntimeError::Lifecycle { phase, source }) => {
            assert_eq!(phase, "macOS guardian registered group identity changed");
            assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
        }
        other => panic!("expected changed-identity error, got {other:?}"),
    }
}

#[test]
fn first_probe_delivery_requires_one_absence_proof() {
    let group = identity();
    let now = Instant::now();
    let deadline = now + Duration::from_secs(1);
    let signals = Cell::new(0);
    let proofs = Cell::new(0);
    let outcome = terminate_attested_group_with_observers(
        group,
        || {
            Err(lifecycle_error(
                "fixture missing leader",
                io::Error::from_raw_os_error(3),
            ))
        },
        |_| (Err(Errno::ESRCH), Ok(())),
        || {
            signals.set(signals.get() + 1);
            classify_attested_group_signal(Ok(()), |_| panic!("delivered signal"))
        },
        || panic!("signallable first probe must not use fallback"),
    );
    let result = finish_attested_group_termination(outcome, || {
        proofs.set(proofs.get() + 1);
        require_attested_group_absence_with_observers(
            group,
            deadline,
            || now,
            |_| (Err(Errno::ESRCH), Err(Errno::ESRCH)),
        )
    });
    assert!(result.is_ok());
    assert_eq!(signals.get(), 1);
    assert_eq!(proofs.get(), 1);
}
