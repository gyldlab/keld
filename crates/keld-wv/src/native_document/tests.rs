use super::*;

fn source(v: u8) -> VerifiedSourceId {
    VerifiedSourceId([v; 32])
}

fn native(v: u64) -> NativeCommitIdentity {
    NativeCommitIdentity(NonZeroU64::new(v).expect("test native identity"))
}

#[test]
fn pending_first_source_has_zero_bridge_authority_until_actual_commit() {
    let mut doc = NativeDocumentOwner::new(WebviewId(5));
    assert_eq!(doc.generation(), Some(0));
    assert_eq!(doc.current_stamp(), None);
    let a = doc.begin_host_load(source(7)).expect("pending A");
    let lease = doc.resource_lease(a, source(7)).expect("resource only");
    assert!(doc.is_current_lease(lease));
    assert_eq!(doc.current_stamp(), None);
    assert_eq!(doc.resource_lease(a, source(8)), None);
    let committed = doc
        .commit_admitted(a, source(7), native(10))
        .expect("commit");
    // GH542: trusted Host load revokes +1 BEFORE native call. Matching
    // real cross-document commit adds another checked +1 (0->1->2).
    assert_eq!(committed.generation.get(), 2);
    assert_eq!(doc.current_stamp(), Some(committed));
    assert!(doc.is_current_lease(lease));
    assert_eq!(doc.commit_admitted(a, source(7), native(10)), Ok(committed));
    assert_eq!(doc.generation(), Some(2));
}

#[test]
fn late_a_fail_and_duplicate_after_b_begin_or_commit_leave_b_usable() {
    let mut doc = NativeDocumentOwner::new(WebviewId(7));
    let a = doc.begin_host_load(source(1)).expect("A");
    let lease_a = doc.resource_lease(a, source(1)).expect("A lease");
    let b = doc.begin_host_load(source(2)).expect("B");
    let lease_b = doc.resource_lease(b, source(2)).expect("B lease");
    assert!(!doc.fail_attempt(a));
    assert!(!doc.is_current_lease(lease_a));
    assert!(doc.is_current_lease(lease_b));
    let committed_b = doc
        .commit_admitted(b, source(2), native(20))
        .expect("B commit");
    assert!(!doc.fail_attempt(a));
    assert!(!doc.fail_attempt(a));
    assert_eq!(doc.current_stamp(), Some(committed_b));
    assert!(doc.is_current_lease(lease_b));
    assert_eq!(
        doc.commit_admitted(a, source(1), native(10)),
        Err(DocumentFailure::StaleAttempt)
    );
}

#[test]
fn matching_b_failure_after_commit_revokes_b_but_late_a_failure_does_not() {
    let mut doc = NativeDocumentOwner::new(WebviewId(4));
    let a = doc.begin_host_load(source(1)).expect("A");
    let b = doc.begin_host_load(source(2)).expect("B");
    let b_lease = doc.resource_lease(b, source(2)).expect("B lease");
    let b_stamp = doc.commit_admitted(b, source(2), native(88)).expect("B");
    assert!(!doc.fail_attempt(a), "late A never revokes B");
    assert_eq!(doc.current_stamp(), Some(b_stamp));
    assert!(doc.is_current_lease(b_lease));
    assert!(
        doc.fail_attempt(b),
        "matching live B failed navigation must revoke B"
    );
    assert_eq!(doc.current_stamp(), None);
    assert!(!doc.is_current_lease(b_lease));
    assert!(!doc.fail_attempt(b), "duplicate failure is inert");
    assert!(!doc.fail_attempt(a), "old A still inert");
    let c = doc
        .begin_host_load(source(3))
        .expect("fresh C after failure");
    assert_ne!(c, b);
    assert_eq!(doc.generation(), Some(4));
}

#[test]
fn matching_current_failure_does_not_restore_old_document() {
    let mut doc = NativeDocumentOwner::new(WebviewId(7));
    let a = doc.begin_host_load(source(1)).expect("A");
    doc.commit_admitted(a, source(1), native(30))
        .expect("A commit");
    let b = doc.begin_host_load(source(2)).expect("B");
    assert_eq!(doc.current_stamp(), None);
    assert!(doc.fail_attempt(b));
    assert_eq!(doc.current_stamp(), None);
    assert_eq!(doc.resource_lease(b, source(2)), None);
    assert!(!doc.fail_attempt(a));
    let c = doc.begin_host_load(source(3)).expect("C");
    assert_ne!(b, c);
    assert_eq!(doc.generation(), Some(4));
}

#[test]
fn source_identity_and_native_callback_identity_are_both_required() {
    let mut doc = NativeDocumentOwner::new(WebviewId(7));
    let a = doc.begin_host_load(source(1)).expect("A");
    assert_eq!(
        doc.commit_admitted(a, source(2), native(22)),
        Err(DocumentFailure::SourceMismatch)
    );
    assert_eq!(doc.current_stamp(), None);
    let b = doc.begin_host_load(source(1)).expect("B");
    let stamp = doc
        .commit_admitted(b, source(1), native(33))
        .expect("B commit");
    assert_eq!(
        doc.commit_admitted(b, source(1), native(34)),
        Err(DocumentFailure::NativeMismatch)
    );
    assert_eq!(doc.current_stamp(), None);
    assert!(doc.generation().expect("closed gen") > stamp.generation.get());
}

#[test]
fn dropping_the_only_native_owner_revokes_its_existing_lease() {
    let mut owner = NativeDocumentOwner::new(WebviewId(17));
    let attempt = owner.begin_host_load(source(1)).expect("one model attempt");
    let stamp = owner
        .commit_admitted(attempt, source(1), native(1))
        .expect("one model commit");
    let lease = owner.lease_for(stamp).expect("existing model lease");
    assert!(lease.is_current());
    drop(owner);
    assert!(!lease.is_current());
}

#[test]
fn actual_unexpected_replacement_revokes_checked_stamp() {
    let mut doc = NativeDocumentOwner::new(WebviewId(10));
    let a = doc.begin_host_load(source(1)).expect("A");
    let stamp = doc
        .commit_admitted(a, source(1), native(1))
        .expect("A commit");
    assert_eq!(doc.current_stamp(), Some(stamp));
    doc.unexpected_mainframe_commit();
    assert_eq!(doc.current_stamp(), None);
    assert_eq!(doc.generation(), Some(3));
}

#[test]
fn exhausted_and_destroyed_are_terminal_no_counter_wrap() {
    let mut doc = NativeDocumentOwner {
        view: WebviewId(2),
        next_attempt: 42,
        live_lease: None,
        #[cfg(windows)]
        renderer: None,
        source: None,
        phase: Phase::FailedClosed {
            generation: u32::MAX,
        },
    };
    assert_eq!(
        doc.begin_host_load(source(1)),
        Err(DocumentFailure::Exhausted)
    );
    assert_eq!(
        doc.begin_host_load(source(1)),
        Err(DocumentFailure::Exhausted)
    );
    let mut serial = NativeDocumentOwner {
        view: WebviewId(2),
        next_attempt: u64::MAX,
        live_lease: None,
        #[cfg(windows)]
        renderer: None,
        source: None,
        phase: Phase::Unbound { generation: 0 },
    };
    assert_eq!(
        serial.begin_host_load(source(1)),
        Err(DocumentFailure::Exhausted)
    );
    let mut dead = NativeDocumentOwner::new(WebviewId(1));
    dead.destroy();
    assert_eq!(dead.current_stamp(), None);
    assert_eq!(
        dead.begin_host_load(source(1)),
        Err(DocumentFailure::Destroyed)
    );
}

#[test]
fn foreign_view_and_stale_source_never_qualify() {
    let mut first = NativeDocumentOwner::new(WebviewId(1));
    let mut second = NativeDocumentOwner::new(WebviewId(2));
    let a = first.begin_host_load(source(1)).expect("first");
    let b = second.begin_host_load(source(1)).expect("second");
    assert_eq!(second.resource_lease(a, source(1)), None);
    assert_eq!(
        second.commit_admitted(a, source(1), native(22)),
        Err(DocumentFailure::WrongView)
    );
    assert_eq!(second.current_stamp(), None);
    assert_eq!(
        second
            .commit_admitted(b, source(1), native(33))
            .unwrap()
            .view,
        WebviewId(2)
    );
}

#[test]
fn unexpected_native_replacement_retires_pending_source_lease() {
    let mut doc = NativeDocumentOwner::new(WebviewId(13));
    let source_a = source(7);
    let attempt = doc.begin_host_load(source_a).expect("pending load");
    let lease = doc
        .resource_lease(attempt, source_a)
        .expect("pending lease");
    assert_eq!(doc.current_stamp(), None);
    assert!(doc.is_current_lease(lease));

    // A native commit/finish without its retained navigation witness
    // must use this existing fail-closed transition, never a guessed
    // current attempt. This model proves revocation, not callback wiring.
    doc.unexpected_mainframe_commit();

    assert_eq!(doc.current_stamp(), None);
    assert!(!doc.is_current_lease(lease));
    assert_eq!(doc.active_resource_lease(source_a), None);
    assert_eq!(
        doc.commit_admitted(attempt, source_a, native(71)),
        Err(DocumentFailure::StaleAttempt)
    );
    let next = doc.begin_host_load(source(8)).expect("fresh host load");
    assert_ne!(attempt, next);
    assert!(doc.resource_lease(next, source(8)).is_some());
    assert!(!doc.is_current_lease(lease));
}

#[test]
fn unexpected_native_replacement_retires_live_stamp_and_source_lease() {
    let mut doc = NativeDocumentOwner::new(WebviewId(14));
    let source_a = source(9);
    let attempt = doc.begin_host_load(source_a).expect("host load");
    let stamp = doc
        .commit_admitted(attempt, source_a, native(91))
        .expect("qualified native commit");
    let lease = doc.resource_lease(attempt, source_a).expect("live lease");
    assert_eq!(doc.current_stamp(), Some(stamp));
    assert!(doc.is_current_lease(lease));

    doc.unexpected_mainframe_commit();

    assert_eq!(doc.current_stamp(), None);
    assert!(!doc.is_current_lease(lease));
    assert_eq!(doc.active_resource_lease(source_a), None);
    assert_eq!(doc.generation(), Some(3));
    assert_eq!(
        doc.commit_admitted(attempt, source_a, native(91)),
        Err(DocumentFailure::StaleAttempt)
    );
    doc.destroy();
    doc.unexpected_mainframe_commit();
    assert_eq!(doc.generation(), None);
    assert_eq!(doc.current_stamp(), None);
    assert_eq!(
        doc.begin_host_load(source_a),
        Err(DocumentFailure::Destroyed)
    );
}
