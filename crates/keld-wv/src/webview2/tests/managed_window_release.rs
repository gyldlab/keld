//! D6/D12 pure custody contracts. Fake integer keys/observations never reach Win32.
//! Native acceptance must separately prove Tao dispatch, actual HWND retirement,
//! child/controller cleanup and the real facade's generation/close protocol.

use std::convert::Infallible;
use std::time::{Duration, Instant};

use super::{
    ManagedCustody, RELEASING_DESTROYED_BOUND, Releasing, STAGED_RELEASE_RECHECK, StageResult,
};
use crate::WebviewId;
use crate::engine::{AppWindowEvent, BuildStage};

#[test]
fn parked_then_staged_then_native_absent_is_one_ordered_release() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    let view = WebviewId(71);
    custody
        .park(1, 101, Releasing::Destroy(view), now)
        .expect("park");
    let mut answers = Vec::new();
    let mut native_reads = 0;
    assert_eq!((custody.len(), custody.is_empty()), (1, false));
    assert_eq!(
        custody.publish_staged_with(
            |_| {
                native_reads += 1;
                false
            },
            |_, _, event| {
                answers.push(event);
                Ok::<(), Infallible>(())
            },
        ),
        Ok(0),
    );
    assert_eq!(native_reads, 0, "a parked entry has no Destroyed receipt");
    assert_eq!(answers, Vec::<AppWindowEvent>::new());
    assert_eq!(
        custody.next_deadline(now),
        Some(now + RELEASING_DESTROYED_BOUND)
    );

    let mut wake_posts = 0;
    assert_eq!(
        custody.stage_destroyed(1, || {
            wake_posts += 1;
            false
        }),
        StageResult::Staged { wake_posted: false },
    );
    assert_eq!(
        custody.stage_destroyed(1, || {
            wake_posts += 1;
            true
        }),
        StageResult::AlreadyStaged,
    );
    assert_eq!(wake_posts, 1, "duplicate Destroyed cannot re-post");
    assert_eq!(
        custody.next_deadline(now),
        Some(now + STAGED_RELEASE_RECHECK)
    );
    assert_eq!(
        custody.publish_staged_with(
            |hwnd| {
                assert_eq!(hwnd, 101);
                true
            },
            |_, _, event| {
                answers.push(event);
                Ok::<(), Infallible>(())
            },
        ),
        Ok(0),
        "the HWND may still exist after WM_DESTROY",
    );
    assert_eq!(answers, Vec::<AppWindowEvent>::new());
    assert!(custody.contains(1));
    assert!(!custody.exit_ready(true, true));

    assert_eq!(
        custody.publish_staged_with(
            |hwnd| {
                assert_eq!(hwnd, 101);
                false
            },
            |hwnd, answer, event| {
                assert_eq!(hwnd, 101);
                assert_eq!(answer, Releasing::Destroy(view));
                answers.push(event);
                Ok::<(), Infallible>(())
            },
        ),
        Ok(1),
    );
    assert_eq!(answers, [AppWindowEvent::WindowReleased { view }]);
    assert_eq!((custody.len(), custody.is_empty()), (0, true));
    assert!(!custody.contains(1));
    assert_eq!(custody.next_deadline(now), None);
    assert_eq!(
        custody.stage_destroyed(1, || panic!("stale wake")),
        StageResult::Unknown
    );
    assert_eq!(
        custody.publish_staged_with(
            |_| panic!("retired entries must never query a recycled HWND"),
            |_, _, event| {
                answers.push(event);
                Ok::<(), Infallible>(())
            },
        ),
        Ok(0),
    );
    assert_eq!(answers.len(), 1);
}

#[test]
fn a_collision_preserves_the_original_answer_and_failed_build_stages() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    let original = Releasing::FailedBuild {
        build: 33,
        stage: BuildStage::Window,
    };
    custody
        .park(1, 101, original, now)
        .expect("original custodian");
    let error = custody
        .park(
            1,
            999,
            Releasing::Destroy(WebviewId(900)),
            now + Duration::from_secs(1),
        )
        .expect_err("a second custodian must not replace the first");
    assert!(error.to_string().contains("KELD-WV-009"), "{error}");
    assert_eq!(custody.entry(1), Some((101, original)));

    let second = Releasing::FailedBuild {
        build: 34,
        stage: BuildStage::Webview,
    };
    custody
        .park(2, 202, second, now)
        .expect("independent custodian");
    assert_eq!(
        custody.stage_destroyed(3, || panic!("foreign Destroyed")),
        StageResult::Unknown
    );
    for key in [1, 2] {
        assert_eq!(
            custody.stage_destroyed(key, || true),
            StageResult::Staged { wake_posted: true },
        );
    }
    let mut answers = Vec::new();
    assert_eq!(
        custody.publish_staged_with(
            |_| false,
            |_, _, event| {
                answers.push(event);
                Ok::<(), Infallible>(())
            },
        ),
        Ok(2),
    );
    assert_eq!(
        answers,
        [
            AppWindowEvent::WindowBuildFailed {
                build: 33,
                stage: BuildStage::Window
            },
            AppWindowEvent::WindowBuildFailed {
                build: 34,
                stage: BuildStage::Webview
            },
        ]
    );
}

#[test]
fn failed_publication_propagates_once_and_keeps_other_custody_visible() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    for key in [1, 2] {
        custody
            .park(
                key,
                isize::from(key),
                Releasing::Destroy(WebviewId(u32::from(key))),
                now,
            )
            .expect("park independent release");
        assert!(matches!(
            custody.stage_destroyed(key, || true),
            StageResult::Staged { .. }
        ));
    }
    let mut attempts = Vec::new();
    assert_eq!(
        custody.publish_staged_with(
            |_| false,
            |_, _, event| {
                attempts.push(event);
                Err("coordinator gone")
            },
        ),
        Err("coordinator gone"),
    );
    assert_eq!(
        attempts,
        [AppWindowEvent::WindowReleased { view: WebviewId(1) }]
    );
    assert!(
        !custody.contains(1),
        "failed send cannot be attempted twice"
    );
    assert!(
        custody.contains(2),
        "another native custodian is not silently discarded"
    );
    assert!(!custody.exit_ready(true, true));
    assert_eq!(
        custody.publish_staged_with(
            |_| false,
            |_, _, event| {
                attempts.push(event);
                Err("still gone")
            },
        ),
        Err("still gone"),
    );
    assert_eq!(
        attempts,
        [
            AppWindowEvent::WindowReleased { view: WebviewId(1) },
            AppWindowEvent::WindowReleased { view: WebviewId(2) },
        ]
    );
    assert!(!custody.contains(2));
}

#[test]
fn both_release_kinds_expire_at_the_original_bound_in_both_stages() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    for (key, answer, staged) in [
        (1, Releasing::Destroy(WebviewId(11)), false),
        (2, Releasing::Destroy(WebviewId(12)), true),
        (
            3,
            Releasing::FailedBuild {
                build: 13,
                stage: BuildStage::Window,
            },
            false,
        ),
        (
            4,
            Releasing::FailedBuild {
                build: 14,
                stage: BuildStage::Webview,
            },
            true,
        ),
    ] {
        custody
            .park(key, isize::from(key) * 100, answer, now)
            .expect("park");
        if staged {
            assert!(matches!(
                custody.stage_destroyed(key, || false),
                StageResult::Staged { .. }
            ));
        }
    }
    let deadline = now + RELEASING_DESTROYED_BOUND;
    let mut expired = Vec::new();
    let just_before = deadline
        .checked_sub(Duration::from_nanos(1))
        .expect("expiry has a preceding instant");
    assert_eq!(
        custody.expire(just_before, |key, hwnd| {
            expired.push((key, hwnd));
        }),
        0
    );
    assert_eq!(expired, Vec::<(u8, isize)>::new());
    assert_eq!(
        custody.expire(deadline, |key, hwnd| expired.push((key, hwnd))),
        4
    );
    assert_eq!(expired, [(1, 100), (2, 200), (3, 300), (4, 400)]);
    assert_eq!(
        custody.expire(deadline, |_, _| panic!("expiry reported twice")),
        0
    );
    assert_eq!(custody.next_deadline(deadline), None);
}

#[test]
fn staging_or_repeated_wakes_never_renew_the_parked_clock() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    custody
        .park(1, 101, Releasing::Destroy(WebviewId(1)), now)
        .expect("first");
    custody
        .park(
            2,
            202,
            Releasing::Destroy(WebviewId(2)),
            now + Duration::from_secs(3),
        )
        .expect("later");
    let original_deadline = now + RELEASING_DESTROYED_BOUND;
    assert_eq!(custody.next_deadline(now), Some(original_deadline));
    assert!(matches!(
        custody.stage_destroyed(1, || true),
        StageResult::Staged { .. }
    ));
    let almost_expired = original_deadline
        .checked_sub(Duration::from_millis(2))
        .expect("parked deadline has a preceding instant");
    assert_eq!(
        custody.next_deadline(almost_expired),
        Some(original_deadline)
    );
    assert_eq!(
        custody.stage_destroyed(1, || panic!("no re-post")),
        StageResult::AlreadyStaged
    );
    let mut expired = Vec::new();
    assert_eq!(
        custody.expire(original_deadline, |key, hwnd| expired.push((key, hwnd))),
        1
    );
    assert_eq!(expired, [(1, 101)]);
    assert_eq!(
        custody.entry(2),
        Some((202, Releasing::Destroy(WebviewId(2))))
    );
    assert_eq!(
        custody.next_deadline(original_deadline),
        Some(original_deadline + Duration::from_secs(3))
    );
}

#[test]
fn completed_native_release_is_published_before_the_same_turn_expiry_check() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    custody
        .park(1, 101, Releasing::Destroy(WebviewId(1)), now)
        .expect("park");
    assert!(matches!(
        custody.stage_destroyed(1, || true),
        StageResult::Staged { .. }
    ));
    let mut answers = Vec::new();
    assert_eq!(
        custody.publish_staged_with(
            |_| false,
            |_, _, event| {
                answers.push(event);
                Ok::<(), Infallible>(())
            },
        ),
        Ok(1),
    );
    assert_eq!(
        custody.expire(now + RELEASING_DESTROYED_BOUND, |_, _| {
            panic!("a completed release cannot become a false expiry");
        }),
        0
    );
    assert_eq!(
        answers,
        [AppWindowEvent::WindowReleased { view: WebviewId(1) }]
    );
}

#[test]
fn exit_needs_terminal_barrier_and_an_empty_release_map() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    for (terminal, barrier, expected) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        assert_eq!(custody.exit_ready(terminal, barrier), expected);
    }
    custody
        .park(1, 101, Releasing::Destroy(WebviewId(1)), now)
        .expect("park");
    assert!(
        !custody.exit_ready(true, true),
        "parked native custody blocks exit"
    );
    assert!(matches!(
        custody.stage_destroyed(1, || true),
        StageResult::Staged { .. }
    ));
    assert!(
        !custody.exit_ready(true, true),
        "staged native custody still blocks exit"
    );
    assert_eq!(
        custody.publish_staged_with(|_| false, |_, _, _| Ok::<(), Infallible>(())),
        Ok(1)
    );
    assert!(custody.exit_ready(true, true));
}

#[test]
fn a_stale_destroyed_key_cannot_stage_a_distinct_successor() {
    let now = Instant::now();
    let mut custody = ManagedCustody::<u8>::default();
    custody
        .park(7, 707, Releasing::Destroy(WebviewId(40)), now)
        .expect("old");
    assert!(matches!(
        custody.stage_destroyed(7, || true),
        StageResult::Staged { .. }
    ));
    assert_eq!(
        custody.publish_staged_with(|_| false, |_, _, _| Ok::<(), Infallible>(())),
        Ok(1)
    );
    custody
        .park(8, 808, Releasing::Destroy(WebviewId(41)), now)
        .expect("successor");
    assert_eq!(
        custody.stage_destroyed(7, || panic!("stale wake")),
        StageResult::Unknown
    );
    assert_eq!(
        custody.publish_staged_with(
            |_| panic!("successor is not staged"),
            |_, _, _| Ok::<(), Infallible>(()),
        ),
        Ok(0),
    );
    assert_eq!(
        custody.entry(8),
        Some((808, Releasing::Destroy(WebviewId(41))))
    );
}
