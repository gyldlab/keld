use super::*;
fn source(n: u8) -> VerifiedSourceId {
    VerifiedSourceId([n; 32])
}
fn native(n: u64) -> NativeCommitIdentity {
    NativeCommitIdentity(NonZeroU64::new(n).expect("native"))
}
#[test]
fn duplicate_commit_reuses_one_lease_and_host_load_revokes_before_commit() {
    let mut owner = NativeDocumentOwner::new(WebviewId(1));
    let a = owner.begin_host_load(source(1)).expect("A");
    let stamp = owner
        .commit_admitted(a, source(1), native(1))
        .expect("commit A");
    let lease = owner.lease_for(stamp).expect("A lease");
    assert_eq!(owner.commit_admitted(a, source(1), native(1)), Ok(stamp));
    let duplicate = owner.lease_for(stamp).expect("duplicate lease");
    assert!(Arc::ptr_eq(&lease.live, &duplicate.live));
    assert!(lease.is_current());
    let _b = owner.begin_host_load(source(2)).expect("B");
    assert!(!lease.is_current());
    assert!(owner.lease_for(stamp).is_none());
}
#[test]
fn stale_a_fail_cannot_revoke_b_but_matching_fail_destroy_and_replacement_do() {
    for terminal in 0..3 {
        let mut owner = NativeDocumentOwner::new(WebviewId(1));
        let a = owner.begin_host_load(source(1)).expect("A");
        let b = owner.begin_host_load(source(2)).expect("B");
        let stamp = owner
            .commit_admitted(b, source(2), native(2))
            .expect("commit B");
        let lease = owner.lease_for(stamp).expect("B lease");
        assert!(!owner.fail_attempt(a));
        assert!(lease.is_current());
        match terminal {
            0 => {
                assert!(owner.fail_attempt(b));
            }
            1 => owner.destroy(),
            _ => owner.unexpected_mainframe_commit(),
        }
        assert!(!lease.is_current());
    }
}

#[test]
fn lease_binds_to_the_actual_retained_source_and_its_own_revocation_witness() {
    use crate::resource_source::{AdmittedWebAppSource, windows::tests::Fixture};
    use std::path::Path;

    let fixture = Fixture::new();
    let bound = fixture.source_arc();
    let equal_inventory = AdmittedWebAppSource::from_admitted_windows_graph(
        bound
            .webapp_root
            .try_clone()
            .expect("same retained directory"),
        bound.source_id().0,
        bound.assets.values().cloned().collect(),
        Path::new(&bound.entry),
        bound.resolved_window_policy().clone(),
    )
    .expect("independent binding to the same files and policy");
    let mut owner = NativeDocumentOwner::new(WebviewId(8));
    owner.bind_source(&bound).expect("bind retained source");
    owner
        .bind_source(&Arc::clone(&bound))
        .expect("same allocation");
    assert_eq!(
        owner.bind_source(&equal_inventory),
        Err(DocumentFailure::SourceMismatch)
    );
    assert!(Arc::ptr_eq(
        &owner.source_binding().expect("live owner"),
        &bound
    ));
    let attempt = owner
        .begin_host_load(bound.source_id())
        .expect("admit original");
    assert_eq!(attempt.view(), owner.view());
    assert_eq!(
        owner.active_resource_lease(bound.source_id()),
        owner.resource_lease(attempt, bound.source_id()),
    );
    let stamp = owner
        .commit_admitted(attempt, bound.source_id(), native(1))
        .expect("matched commit");
    let lease = owner.lease_for(stamp).expect("current commit");
    assert_eq!(lease.stamp(), stamp);
    assert!(lease.belongs_to_source(&bound));
    assert!(!lease.belongs_to_source(&equal_inventory));

    drop(owner);
    assert!(!lease.is_current());
    // A later native lifetime can repeat the scalar observations.
    let mut other = NativeDocumentOwner::new(WebviewId(8));
    other.bind_source(&bound).expect("same source");
    let other_attempt = other
        .begin_host_load(bound.source_id())
        .expect("separate attempt");
    let other_stamp = other
        .commit_admitted(other_attempt, bound.source_id(), native(1))
        .expect("separate commit");
    assert_eq!(stamp, other_stamp, "the scalar facts coincide");
    let other_lease = other.lease_for(other_stamp).expect("other owner");
    assert!(!lease.same_binding(&other_lease));
    assert!(!lease.is_current());
    assert!(other_lease.is_current());
}

#[test]
fn renderer_exit_reaches_exported_leases_and_prevents_reissue() {
    let lifetime = RendererLifetime::default();
    let mut owner =
        NativeDocumentOwner::for_windows_renderer(WebviewId(10), Some(lifetime.clone()));
    assert!(owner.observes_renderer());
    let attempt = owner.begin_host_load(source(1)).expect("pending");
    let stamp = owner
        .commit_admitted(attempt, source(1), native(1))
        .expect("matched commit");
    let lease = owner.lease_for(stamp).expect("current renderer");
    let exported = lease.clone();
    assert!(exported.is_current());
    lifetime.observe_exit();
    assert!(owner.renderer_exited());
    assert!(!lease.is_current());
    assert!(!exported.is_current());
    assert!(owner.lease_for(stamp).is_none());
}
