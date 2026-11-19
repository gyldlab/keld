use super::{RendererBridgeOutcome, RendererOutcomes};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};

#[test]
fn capacity_wake_follows_one_actual_outcome_and_disconnect_stays_terminal() {
    let (sender, receiver) = mpsc::sync_channel(1);
    let signals = Arc::new(AtomicUsize::new(0));
    let outcomes = RendererOutcomes {
        sender: Mutex::new(Some(sender)),
        wake: OnceLock::new(),
    };
    let observed = Arc::clone(&signals);
    assert!(
        outcomes
            .wake
            .set(Box::new(move || {
                observed.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }))
            .is_ok()
    );

    outcomes.capacity().expect("one actual outcome");
    outcomes.capacity().expect("retry only coalesces");
    assert_eq!(signals.load(Ordering::Relaxed), 1);
    assert!(matches!(
        receiver.try_recv(),
        Ok(RendererBridgeOutcome::ControlCapacity)
    ));
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));

    outcomes.capacity().expect("slot now has another outcome");
    assert_eq!(signals.load(Ordering::Relaxed), 2);
    drop(receiver);
    assert!(
        outcomes
            .capacity()
            .expect_err("receiver gone")
            .to_string()
            .contains("original native receiver disconnected")
    );
    assert_eq!(signals.load(Ordering::Relaxed), 2);
}
