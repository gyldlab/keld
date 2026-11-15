//! Pure comparison oracles for Prepared2 admission and cleanup receipts.
use std::io;
use std::time::Instant;

use super::records_and_cut_types::{ControlFields, DeliveryCut, RelayReceipt};
use super::{
    GUARDIAN_SHUTDOWN_DEADLINE, SupervisorOutcome, is_owned_preparation_interruption,
    runtime_io_kind,
};

pub(super) fn assert_cleanup_route(cut: DeliveryCut, receipts: &[RelayReceipt]) {
    let revokes: Vec<_> = receipts
        .iter()
        .enumerate()
        .filter_map(|(index, receipt)| match receipt {
            RelayReceipt::NormalRevokeTwo { request, response } => Some((index, request, response)),
            _ => None,
        })
        .collect();
    let clears: Vec<_> = receipts
        .iter()
        .enumerate()
        .filter_map(|(index, receipt)| match receipt {
            RelayReceipt::NormalClearTwo { request, response } => Some((index, request, response)),
            _ => None,
        })
        .collect();
    assert_eq!(
        receipts
            .iter()
            .filter(|receipt| matches!(receipt, RelayReceipt::GuardianControlEof))
            .count(),
        1,
        "actual guardian control EOF absent/duplicated"
    );
    if matches!(
        cut,
        DeliveryCut::FullBeforeLease | DeliveryCut::LeaseBeforeSpawn
    ) {
        assert_eq!(
            revokes.len(),
            1,
            "normal lease Revoke2 request/ack missing or duplicated"
        );
        assert_eq!(
            clears.len(),
            1,
            "normal lease Clear2 request/ack missing or duplicated"
        );
        // Literal approved private wire fields are an independent
        // oracle; changing production constants cannot change these.
        let expected = [
            ControlFields {
                kind: 5,
                cause: 2,
                attempt: 2,
                pid: 0,
                payload_len: 0,
            },
            ControlFields {
                kind: 6,
                cause: 0,
                attempt: 2,
                pid: 0,
                payload_len: 0,
            },
            ControlFields {
                kind: 7,
                cause: 0,
                attempt: 2,
                pid: 0,
                payload_len: 0,
            },
            ControlFields {
                kind: 8,
                cause: 0,
                attempt: 2,
                pid: 0,
                payload_len: 0,
            },
        ];
        let actual = [revokes[0].1, revokes[0].2, clears[0].1, clears[0].2];
        assert_eq!(
            actual,
            [&expected[0], &expected[1], &expected[2], &expected[3]]
        );
        assert!(
            revokes[0].0 < clears[0].0,
            "normal Clear2 acknowledgment preceded Revoke2 acknowledgment"
        );
    } else {
        assert!(
            revokes.is_empty(),
            "unclaimed zero/partial cut acquired normal Revoke2 path"
        );
        assert!(
            clears.is_empty(),
            "unclaimed zero/partial cut acquired normal Clear2 path"
        );
        // Together with the separately asserted exact host-owned
        // Revoked2/Shutdown and successful host join, actual EOF plus
        // absent normal exchanges identifies the host EOF cleanup.
    }
}

pub(super) fn assert_cut_outcome(
    cut: DeliveryCut,
    terminal: &crate::SupervisorEvent,
    outcome: &SupervisorOutcome,
    began: Option<Instant>,
) {
    if cut == DeliveryCut::Partial {
        assert!(began.expect("actual prefix write clock").elapsed() >= GUARDIAN_SHUTDOWN_DEADLINE);
        assert!(matches!(
            terminal,
            crate::SupervisorEvent::Failed { attempt: 2 }
        ));
        match outcome {
            SupervisorOutcome::Failed(error) => {
                assert_eq!(runtime_io_kind(error), Some(io::ErrorKind::TimedOut));
                assert!(!is_owned_preparation_interruption(error, 2));
                assert!(error.to_string().contains("read deadline"));
            }
            other => panic!("partial Prepared timeout was hidden: {other:?}"),
        }
    } else {
        assert!(matches!(terminal, crate::SupervisorEvent::Stopped));
        assert!(matches!(outcome, SupervisorOutcome::Stopped));
    }
}
