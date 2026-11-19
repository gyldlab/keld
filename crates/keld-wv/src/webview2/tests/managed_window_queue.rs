//! D6 deferred creates run in FIFO order only after native custody is empty.

use std::convert::Infallible;
use std::time::Instant;

use super::{
    CreateAdmission, DeferredCreate, ManagedCustody, Releasing, STAGED_RELEASE_RECHECK, StageResult,
};
use crate::WebviewId;
use crate::engine::{AppWindowEvent, BuildStage};

fn create(build: u64) -> DeferredCreate {
    DeferredCreate {
        build,
        width: 960,
        height: 640,
        x: Some(-120),
        y: None,
    }
}

#[test]
fn deferred_fifo_waits_for_release_and_a_buffered_destroy_revokes_head_admission() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    assert_eq!(
        custody.admit(create(1), false),
        CreateAdmission::Start(create(1))
    );
    custody
        .park(1, 101, Releasing::Destroy(WebviewId(1)), now)
        .expect("release");
    for build in [2, 3] {
        assert_eq!(custody.admit(create(build), false), CreateAdmission::Queued);
    }
    assert_eq!(
        custody.post_deferred_wake(false, now, || panic!("release still pending")),
        None
    );
    assert_eq!(custody.take_deferred_wake(false), None, "no invented wake");
    assert!(matches!(
        custody.stage_destroyed(1, || true),
        StageResult::Staged { .. }
    ));
    assert_eq!(
        custody.publish_staged_with(|_| false, |_, _, _| Ok::<(), Infallible>(())),
        Ok(1)
    );
    assert_eq!(
        custody.deferred.front(),
        Some(&create(2)),
        "release wakes never consume builds"
    );

    let mut posts = 0;
    assert_eq!(
        custody.post_deferred_wake(false, now, || {
            posts += 1;
            true
        }),
        None
    );
    assert_eq!(
        custody.post_deferred_wake(false, now, || {
            posts += 1;
            true
        }),
        None
    );
    assert_eq!(posts, 1, "at most one dedicated wake is outstanding");
    assert_eq!(custody.admit(create(4), false), CreateAdmission::Queued);
    assert_eq!(custody.take_deferred_wake(false), Some(create(2)));
    assert_eq!(
        custody.take_deferred_wake(false),
        None,
        "one build per dedicated wake"
    );

    assert_eq!(custody.post_deferred_wake(false, now, || true), None);
    custody
        .park(2, 202, Releasing::Destroy(WebviewId(2)), now)
        .expect("buffered destroy");
    assert_eq!(
        custody.take_deferred_wake(false),
        None,
        "the buffered destroy wins"
    );
    assert!(
        !custody.deferred_wake_outstanding,
        "a consumed wake never stays armed"
    );
    assert_eq!(custody.deferred.front(), Some(&create(3)));
    assert!(matches!(
        custody.stage_destroyed(2, || true),
        StageResult::Staged { .. }
    ));
    assert_eq!(
        custody.publish_staged_with(|_| false, |_, _, _| Ok::<(), Infallible>(())),
        Ok(1)
    );
    assert_eq!(custody.post_deferred_wake(false, now, || true), None);
    assert_eq!(custody.take_deferred_wake(false), Some(create(3)));
    assert_eq!(custody.post_deferred_wake(false, now, || true), None);
    assert_eq!(custody.take_deferred_wake(false), Some(create(4)));
    assert_eq!(
        custody.post_deferred_wake(false, now, || panic!("queue is empty")),
        None
    );
}

#[test]
fn a_failed_deferred_post_leaves_the_head_queued_and_supplies_its_own_recheck() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    custody
        .park(1, 101, Releasing::Destroy(WebviewId(1)), now)
        .expect("release");
    assert_eq!(custody.admit(create(23), false), CreateAdmission::Queued);
    assert!(matches!(
        custody.stage_destroyed(1, || true),
        StageResult::Staged { .. }
    ));
    assert_eq!(
        custody.publish_staged_with(|_| false, |_, _, _| Ok::<(), Infallible>(())),
        Ok(1)
    );
    assert_eq!(custody.next_deadline(now), None, "no release timer remains");
    assert_eq!(
        custody.post_deferred_wake(false, now, || false),
        Some(now + STAGED_RELEASE_RECHECK),
    );
    assert!(!custody.deferred_wake_outstanding);
    assert_eq!(custody.take_deferred_wake(false), None);
    assert_eq!(custody.deferred.front(), Some(&create(23)));
    assert_eq!(
        custody.post_deferred_wake(false, now + STAGED_RELEASE_RECHECK, || true),
        None
    );
    assert_eq!(custody.take_deferred_wake(false), Some(create(23)));
}

#[test]
fn terminal_drain_pops_every_request_in_order_and_preserves_the_first_send_failure() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    custody
        .park(1, 101, Releasing::Destroy(WebviewId(1)), now)
        .expect("release");
    for build in [21, 22, 23] {
        assert_eq!(custody.admit(create(build), false), CreateAdmission::Queued);
    }
    assert!(matches!(
        custody.stage_destroyed(1, || true),
        StageResult::Staged { .. }
    ));
    assert_eq!(
        custody.publish_staged_with(|_| false, |_, _, _| Ok::<(), Infallible>(())),
        Ok(1)
    );
    assert_eq!(custody.post_deferred_wake(false, now, || true), None);
    assert_eq!(
        custody.admit(create(24), true),
        CreateAdmission::Refused(create(24))
    );
    assert_eq!(
        custody.take_deferred_wake(true),
        None,
        "terminal intent wins over a queued head wake"
    );

    let mut attempted = Vec::new();
    assert_eq!(
        custody.refuse_deferred(|event| {
            attempted.push(event);
            Err(attempted.len())
        }),
        Err(1)
    );
    assert_eq!(
        attempted,
        [
            AppWindowEvent::WindowBuildFailed {
                build: 21,
                stage: BuildStage::Webview
            },
            AppWindowEvent::WindowBuildFailed {
                build: 22,
                stage: BuildStage::Webview
            },
            AppWindowEvent::WindowBuildFailed {
                build: 23,
                stage: BuildStage::Webview
            },
        ]
    );
    assert!(
        custody.deferred.is_empty(),
        "failed sends cannot leave a hidden backlog at exit"
    );
    assert!(!custody.deferred_wake_outstanding);
    assert_eq!(
        custody.take_deferred_wake(false),
        None,
        "late dedicated wake cannot resurrect work"
    );
    assert_eq!(
        custody.refuse_deferred(|_| -> Result<(), usize> {
            panic!("a refused request was attempted twice");
        }),
        Ok(())
    );
    assert_eq!(
        custody.post_deferred_wake(true, now, || panic!("terminal")),
        None
    );
    assert_eq!(
        custody.admit(create(25), true),
        CreateAdmission::Refused(create(25))
    );
    assert!(custody.exit_ready(true, true));
}
