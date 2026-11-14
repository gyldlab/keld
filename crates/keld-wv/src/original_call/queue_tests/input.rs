//! Input credit is reserved before actual allocations and moves without minting.
use super::*;

#[test]
fn input_growth_refuses_foreign_account_and_overflow_without_mutation() {
    let mut a = Harness::new();
    let mut b = a.sharing_source();
    let mut input = a.queue.begin_input(64).unwrap();
    let before = (
        a.queue.retained_bytes(),
        b.queue.retained_bytes(),
        input.retained_bytes(),
    );
    assert!(b.queue.grow_input(&mut input, 1).is_err());
    assert!(a.queue.grow_input(&mut input, usize::MAX).is_err());
    assert_eq!(
        (
            a.queue.retained_bytes(),
            b.queue.retained_bytes(),
            input.retained_bytes()
        ),
        before
    );
    let mut input = Some(input);
    assert!(b.queue.release_input(&mut input).is_err());
    assert!(input.is_some());
    a.queue.release_input(&mut input).unwrap();
    assert!(input.is_none());
    assert!(a.queue.is_drained());
    assert!(b.queue.is_drained());
}

#[test]
fn partial_input_keeps_credit_until_actual_body_disposal() {
    let mut h = Harness::new();
    let baseline = h.queue.retained_bytes();
    let mut charge = Some(h.queue.begin_input(64).unwrap());
    h.queue.grow_input(charge.as_mut().unwrap(), 32).unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let body = Payload {
        bytes: String::from("partial"),
        drops: Arc::clone(&drops),
    };
    assert_eq!(h.queue.retained_records(), 1);
    assert!(!h.queue.is_drained());
    assert!(h.queue.retained_bytes() > baseline);
    drop(body); // The actual decoder/raw owner is gone before explicit release.
    assert_eq!(drops.load(Ordering::Acquire), 1);
    h.queue.release_input(&mut charge).unwrap();
    assert!(h.queue.release_input(&mut charge).is_err());
    assert!(h.queue.is_drained());
    assert_eq!(h.queue.retained_bytes(), baseline);
}

#[test]
fn prepaid_input_transfers_same_body_and_single_record_then_releases_once() {
    let mut h = Harness::new();
    let baseline = h.queue.retained_bytes();
    let mut charge = Some(h.queue.begin_input(0).unwrap());
    let drops = Arc::new(AtomicUsize::new(0));
    let payload = Payload {
        bytes: String::from("one owned body"),
        drops: Arc::clone(&drops),
    };
    let pointer = payload.bytes.as_ptr();
    let mut input = Some(OriginalCallInput {
        id: 23,
        lease: h.lease.clone(),
        payload_bytes: payload.bytes.capacity(),
        payload,
        credential: String::from("app"),
        channel: None,
        role_witness: None,
        facts: None,
    });
    let required = OriginalCallQueue::<Payload>::input_bytes(input.as_ref().unwrap()).unwrap();
    assert!(
        h.queue
            .enqueue_prepaid(
                &mut input,
                &mut charge,
                &h.binding,
                &h.owner,
                &h.endpoint,
                None,
                &mut h.fault
            )
            .is_err()
    );
    assert!(
        input.is_some() && charge.is_some(),
        "underpayment preserves both actual owners"
    );
    let additional = required - charge.as_ref().unwrap().retained_bytes();
    h.queue
        .grow_input(charge.as_mut().unwrap(), additional)
        .unwrap();
    let admitted_bytes = h.queue.retained_bytes();
    h.queue
        .enqueue_prepaid(
            &mut input,
            &mut charge,
            &h.binding,
            &h.owner,
            &h.endpoint,
            None,
            &mut h.fault,
        )
        .unwrap();
    assert!(input.is_none() && charge.is_none());
    assert_eq!(h.queue.retained_records(), 1, "no second admission minted");
    assert_eq!(h.queue.retained_bytes(), admitted_bytes);
    assert_eq!(
        h.queue
            .local_calls
            .front()
            .unwrap()
            .payload
            .as_ref()
            .unwrap()
            .bytes
            .as_ptr(),
        pointer
    );
    assert_eq!(drops.load(Ordering::Acquire), 0);
    h.queue.cancel_queued().unwrap();
    assert_eq!(drops.load(Ordering::Acquire), 1);
    assert!(h.queue.is_drained());
    assert_eq!(h.queue.retained_bytes(), baseline);
}

#[test]
fn pre_copy_cap_refuses_before_caller_allocates_and_retains_prior_charge() {
    let mut h = Harness::new();
    let mut charge = Some(h.queue.begin_input(0).unwrap());
    let before = h.queue.retained_bytes();
    let mut allocated = false;
    let reserve = h.queue.grow_input(charge.as_mut().unwrap(), 524_288);
    if reserve.is_ok() {
        allocated = true;
    }
    assert!(reserve.is_err());
    assert!(!allocated);
    assert_eq!(h.queue.retained_bytes(), before);
    assert_eq!(h.queue.retained_records(), 1);
    h.queue.release_input(&mut charge).unwrap();
    assert!(h.queue.is_drained());
}

#[test]
fn only_disposed_transient_bytes_return_without_releasing_live_record() {
    let mut a = Harness::new();
    let mut b = a.sharing_source();
    let baseline = a.queue.retained_bytes();
    let mut token = a.queue.begin_input(64).unwrap();
    let floor = token.retained_bytes();
    a.queue.grow_input(&mut token, 128).unwrap();
    let raw = vec![1u8; 128]; // Reserved before allocation.
    assert!(
        b.queue
            .release_disposed_input_storage(&mut token, 128)
            .is_err()
    );
    assert!(
        a.queue
            .release_disposed_input_storage(&mut token, 129)
            .is_err()
    );
    assert_eq!(token.retained_bytes(), floor + 128);
    drop(raw); // Actual transient storage is gone first.
    a.queue
        .release_disposed_input_storage(&mut token, 128)
        .unwrap();
    assert_eq!(token.retained_bytes(), floor);
    assert_eq!(a.queue.retained_records(), 1);
    assert_eq!(a.queue.retained_bytes(), baseline + floor);
    let mut token = Some(token);
    a.queue.release_input(&mut token).unwrap();
    assert!(a.queue.is_drained());
}
