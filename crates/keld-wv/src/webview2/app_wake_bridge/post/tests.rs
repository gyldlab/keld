//! Same unit-post helper and actual retained Drop witness; no native claim.
use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Owned {
    value: Box<u8>,
    drops: Arc<AtomicUsize>,
}
impl Drop for Owned {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::AcqRel);
    }
}
struct Owner {
    current: Option<Owned>,
    attempted: bool,
    failure: Option<Failure>,
}
impl PostState for Owner {
    fn begin_post(&mut self) -> Result<bool, Failure> {
        if self.current.is_none() || self.attempted {
            return Ok(false);
        }
        self.attempted = true;
        Ok(true)
    }
    fn post_failed(&mut self, fault: Failure) {
        self.failure.get_or_insert(fault);
    }
}
fn owner() -> (Mutex<Owner>, Arc<AtomicUsize>, *const u8) {
    let drops = Arc::new(AtomicUsize::new(0));
    let value = Owned {
        value: Box::new(17),
        drops: Arc::clone(&drops),
    };
    let address = std::ptr::from_ref(value.value.as_ref());
    (
        Mutex::new(Owner {
            current: Some(value),
            attempted: false,
            failure: None,
        }),
        drops,
        address,
    )
}
#[test]
fn failed_unit_post_keeps_exact_payload_when_no_wake_is_delivered() {
    let (owner, drops, address) = owner();
    let delivered_wake = false;
    assert!(matches!(
        post_staged(&owner, || Err(())),
        Err(Failure::PostRejected)
    ));
    let state = owner.lock().unwrap();
    assert!(!delivered_wake);
    assert!(matches!(state.failure, Some(Failure::PostRejected)));
    assert_eq!(
        std::ptr::from_ref(state.current.as_ref().unwrap().value.as_ref()),
        address
    );
    assert_eq!(drops.load(Ordering::Acquire), 0);
    drop(state);
    drop(owner);
    assert_eq!(drops.load(Ordering::Acquire), 1);
}
#[test]
fn unwound_unit_post_keeps_exact_payload_and_latches_failure_without_a_wake() {
    let (owner, drops, address) = owner();
    assert!(matches!(
        post_staged(&owner, || panic!("actual unit-post boundary")),
        Err(Failure::PostUnwound)
    ));
    let state = owner.lock().unwrap();
    assert!(matches!(state.failure, Some(Failure::PostUnwound)));
    assert_eq!(
        std::ptr::from_ref(state.current.as_ref().unwrap().value.as_ref()),
        address
    );
    assert_eq!(drops.load(Ordering::Acquire), 0);
    drop(state);
    drop(owner);
    assert_eq!(drops.load(Ordering::Acquire), 1);
}
#[test]
fn post_holds_no_owner_guard_and_actual_ui_take_moves_only_once() {
    let (owner, drops, address) = owner();
    let mut pending = None;
    assert!(
        post_staged(&owner, || {
            // Models actual UI dequeue during the native call. The same helper has
            // already released the mutex; payload moves into its retained target.
            pending = owner
                .try_lock()
                .expect("no guard across post")
                .current
                .take();
            Ok(())
        })
        .unwrap()
    );
    assert_eq!(
        std::ptr::from_ref(pending.as_ref().unwrap().value.as_ref()),
        address
    );
    assert_eq!(drops.load(Ordering::Acquire), 0);
    assert!(!post_staged(&owner, || panic!("duplicate wake cannot re-send")).unwrap());
    drop(pending);
    assert_eq!(drops.load(Ordering::Acquire), 1);
}
