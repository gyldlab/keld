use std::io;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, TryLockError, mpsc};
use std::thread::{self, JoinHandle};

use super::{
    COLLECTION, CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED, CONTROL_REVOKE,
    CONTROL_REVOKED, GenerationControlRecord, GenerationProvisionFence, GuardedPrimaryUpdate,
    RecoveryState, RevocationCause, RoleEvent, RuntimeError, lifecycle_error, lock_recovery,
    parse_app_link, receive_record, run_generation_host, runtime_io_kind, timed_pair,
    write_generation_control,
};

struct RetiredOwner {
    guardian: Option<UnixStream>,
    recovery: Arc<Mutex<RecoveryState>>,
    updates: Receiver<GuardedPrimaryUpdate>,
    worker: Option<JoinHandle<Result<(), RuntimeError>>>,
}

impl RetiredOwner {
    fn stream(&mut self) -> &mut UnixStream {
        self.guardian.as_mut().expect("retained private peer")
    }

    fn join(&mut self) -> Result<Vec<GuardedPrimaryUpdate>, RuntimeError> {
        self.worker
            .take()
            .expect("one actual host owner")
            .join()
            .map_err(|_| {
                lifecycle_error(
                    "test actual mint owner join",
                    io::Error::other("owner panicked"),
                )
            })??;
        Ok(self.updates.try_iter().collect())
    }
}

impl Drop for RetiredOwner {
    fn drop(&mut self) {
        lock_recovery(&self.recovery).deny();
        self.guardian.take();
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("KEL143_MINT_RESCUE owner error: {error}"),
                Err(error) => eprintln!("KEL143_MINT_RESCUE panic: {error:?}"),
            }
        }
    }
}

fn prepare_retired_owner(fence: Option<GenerationProvisionFence>) -> RetiredOwner {
    let (host, guardian) = timed_pair();
    let (updates_tx, updates_rx) = mpsc::channel();
    let current = Arc::new(Mutex::new(None));
    let recovery = Arc::new(Mutex::new(RecoveryState::new()));
    lock_recovery(&recovery).arm();
    let state = Arc::clone(&recovery);
    let worker = thread::spawn(move || {
        run_generation_host(
            super::super::super::generation_host::GenerationHostAdmission::new(
                std::process::id(),
                std::time::Instant::now() + super::GUARDIAN_SHUTDOWN_DEADLINE,
            ),
            host,
            &updates_tx,
            &current,
            &state,
            fence.as_ref(),
            None,
        )
    });
    let mut owner = RetiredOwner {
        guardian: Some(guardian),
        recovery,
        updates: updates_rx,
        worker: Some(worker),
    };
    write_generation_control(
        owner.stream(),
        &GenerationControlRecord::new(CONTROL_PREPARE, 1),
    )
    .expect("actual Prepare1");
    assert_eq!(
        receive_record(owner.stream())
            .expect("actual Prepared1")
            .kind,
        CONTROL_PREPARED
    );
    for (kind, expected) in [
        (CONTROL_REVOKE, CONTROL_REVOKED),
        (CONTROL_CLEAR, CONTROL_CLEARED),
    ] {
        let record = if kind == CONTROL_REVOKE {
            GenerationControlRecord::new(kind, 1).with_cause(RevocationCause::AdmissionFailed)
        } else {
            GenerationControlRecord::new(kind, 1)
        };
        write_generation_control(owner.stream(), &record)
            .expect("actual retired predecessor transition");
        assert_eq!(
            receive_record(owner.stream())
                .expect("actual predecessor acknowledgment")
                .kind,
            expected
        );
    }
    owner
}

#[test]
fn generation_host_provision_mutex_orders_actual_mint_and_denial() {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let mut owner = prepare_retired_owner(Some(GenerationProvisionFence {
        attempt: 2,
        entered: entered_tx,
        release: release_rx,
    }));
    let recovery = Arc::clone(&owner.recovery);
    lock_recovery(&recovery).arm();
    write_generation_control(
        owner.stream(),
        &GenerationControlRecord::new(CONTROL_PREPARE, 2),
    )
    .expect("actual Prepare2");
    let entered = entered_rx.recv_timeout(COLLECTION);
    // This is an independent lock observable while actual host mint is
    // fenced. The fault releases the guard before this same fence.
    let denial_won_before_mint = match recovery.try_lock() {
        Ok(mut state) => {
            state.deny();
            true
        }
        Err(TryLockError::WouldBlock) => false,
        Err(TryLockError::Poisoned(error)) => {
            error.into_inner().deny();
            true
        }
    };
    let released = release_tx.send(());
    let response = receive_record(owner.stream());
    let events_at_mint: Vec<_> = owner.updates.try_iter().collect();
    lock_recovery(&recovery).deny();
    let mut listener_was_live = false;
    let mut listener_gone = false;
    if let Ok(response) = &response {
        let (endpoint, _) =
            parse_app_link(std::str::from_utf8(&response.payload).expect("actual app link UTF8"))
                .expect("actual minted locator");
        listener_was_live = UnixStream::connect(endpoint).is_ok();
        for (kind, expected) in [
            (CONTROL_REVOKE, CONTROL_REVOKED),
            (CONTROL_CLEAR, CONTROL_CLEARED),
        ] {
            let record = if kind == CONTROL_REVOKE {
                GenerationControlRecord::new(kind, 2).with_cause(RevocationCause::Shutdown)
            } else {
                GenerationControlRecord::new(kind, 2)
            };
            write_generation_control(owner.stream(), &record)
                .expect("actual successor cleanup transition");
            assert_eq!(
                receive_record(owner.stream())
                    .expect("actual successor cleanup acknowledgment")
                    .kind,
                expected
            );
        }
        listener_gone = UnixStream::connect(endpoint).is_err();
    }
    lock_recovery(&recovery).shutdown_acknowledged = true;
    owner.guardian.take();
    let joined = owner.join();
    entered.expect("actual pre-mint owner fence reached");
    released.expect("release actual mint fence before assertions");
    let response = response.expect("actual owner minted matching Prepared2");
    assert_eq!((response.kind, response.attempt), (CONTROL_PREPARED, 2));
    let after_cleanup =
        joined.expect("actual host owner joins after exact cleanup and attributed EOF");
    assert!(after_cleanup.iter().any(|update| matches!(
        update,
        GuardedPrimaryUpdate::Role(RoleEvent::Revoked {
            attempt: 2,
            cause: crate::role::RoleRevocationCause::Shutdown,
            ..
        })
    )));
    assert!(events_at_mint.iter().any(|update| matches!(
        update,
        GuardedPrimaryUpdate::Role(RoleEvent::Provisioned { attempt: 2, .. })
    )));
    assert!(listener_was_live && listener_gone);
    println!(
        "KEL143_MINT_FENCE denied_before_actual_mint={denial_won_before_mint} actual_provisioned=2 listener_was_live={listener_was_live} listener_absent={listener_gone}"
    );
    assert!(
        !denial_won_before_mint,
        "actual mint followed observed denial at the shared pre-mint fence"
    );
}

#[test]
fn generation_host_denial_winner_has_no_successor_locator() {
    let mut owner = prepare_retired_owner(None);
    let recovery = Arc::clone(&owner.recovery);
    {
        let mut state = lock_recovery(&recovery);
        state.deny();
        state.arm();
        state.shutdown_acknowledged = true;
    }
    write_generation_control(
        owner.stream(),
        &GenerationControlRecord::new(CONTROL_PREPARE, 2),
    )
    .expect("actual denied Prepare2");
    // Draining EOF is the actual owner's final processing witness;
    // absence during a guessed timed interval would not establish it.
    owner
        .stream()
        .shutdown(Shutdown::Write)
        .expect("real private request EOF after complete Prepare2");
    let updates = owner
        .join()
        .expect("actual denied host owner joins after resource-free attributed completion");
    assert!(!updates.iter().any(|update| matches!(
        update,
        GuardedPrimaryUpdate::Role(RoleEvent::Provisioned { attempt: 2, .. })
    )));
    let closure = receive_record(owner.stream()).expect_err("denial fabricated Prepared2 locator");
    assert_eq!(
        runtime_io_kind(&closure),
        Some(io::ErrorKind::UnexpectedEof)
    );
    println!("KEL143_MINT_FENCE denial_winner=true successor_locator=false actual_owner_eof=true");
}
