// Copyright 2026 Keld contributors
// SPDX-License-Identifier: Apache-2.0

//! Pure controls for the actual queue handoff after a successful native wake.
//! These do not claim native HWND/PostMessage execution.

use super::send;
use crossbeam_channel::unbounded;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug)]
struct OwnedEvent {
  allocation: Box<u8>,
  drops: Arc<AtomicUsize>,
}

impl Drop for OwnedEvent {
  fn drop(&mut self) {
    self.drops.fetch_add(1, Ordering::AcqRel);
  }
}

#[test]
fn closed_receiver_returns_the_exact_undropped_event() {
  let drops = Arc::new(AtomicUsize::new(0));
  let event = OwnedEvent {
    allocation: Box::new(7),
    drops: Arc::clone(&drops),
  };
  let address = std::ptr::from_ref(event.allocation.as_ref());
  let (sender, receiver) = unbounded();
  drop(receiver);

  let failure = send(&sender, event).expect_err("closed queue must not report delivery");
  assert_eq!(
    drops.load(Ordering::Acquire),
    0,
    "failed send discarded custody"
  );
  assert_eq!(std::ptr::from_ref(failure.0.allocation.as_ref()), address);
  drop(failure);
  assert_eq!(drops.load(Ordering::Acquire), 1);
}

#[test]
fn live_receiver_owns_the_same_event_until_actual_dequeue_and_drop() {
  let drops = Arc::new(AtomicUsize::new(0));
  let event = OwnedEvent {
    allocation: Box::new(11),
    drops: Arc::clone(&drops),
  };
  let address = std::ptr::from_ref(event.allocation.as_ref());
  let (sender, receiver) = unbounded();

  send(&sender, event).expect("live existing queue accepts its event");
  assert_eq!(drops.load(Ordering::Acquire), 0);
  let delivered = receiver.try_recv().expect("same event is actually queued");
  assert_eq!(std::ptr::from_ref(delivered.allocation.as_ref()), address);
  assert!(receiver.try_recv().is_err(), "there is one actual event");
  assert_eq!(drops.load(Ordering::Acquire), 0);
  drop(delivered);
  assert_eq!(drops.load(Ordering::Acquire), 1);
}
