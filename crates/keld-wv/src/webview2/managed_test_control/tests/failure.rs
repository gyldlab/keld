//! Pure U7 selection and dispatch ordering; no Tao loop or native handle.
use super::{BuildPoint, DestroyedMode, FailureControl, FailureSpec};

#[test]
fn only_the_approved_points_and_destroyed_modes_parse() {
    for (point, expected) in [
        ("hwnd", BuildPoint::Hwnd),
        ("webview", BuildPoint::Webview),
        ("view", BuildPoint::View),
    ] {
        for (suffix, mode) in [
            ("", DestroyedMode::Deliver),
            (":swallow", DestroyedMode::Swallow),
            (":defer", DestroyedMode::Defer),
        ] {
            assert_eq!(
                FailureSpec::parse(&format!("{point}{suffix}")).expect("approved fixture"),
                FailureSpec {
                    point: expected,
                    mode
                },
            );
        }
    }
    for bad in [
        "",
        " HWND",
        "controller",
        "hwnd:",
        "hwnd:deliver",
        "view:defer:swallow",
    ] {
        let error = FailureSpec::parse(bad).expect_err("unapproved shape");
        assert!(
            error.to_string().contains("KELD_TEST_WINDOWS_FAIL_BUILD"),
            "{error}"
        );
    }
}

#[test]
fn the_fault_belongs_to_the_first_claimed_build_and_fires_only_at_its_point() {
    let mut control = FailureControl::<u8>::new(Some(FailureSpec::parse("view").expect("fixture")));
    let mut first = control.begin_build(91);
    let mut second = control.begin_build(92);
    control
        .fail_at(&mut first, BuildPoint::Hwnd, 1, 101)
        .expect("earlier step");
    control
        .fail_at(&mut first, BuildPoint::Webview, 1, 101)
        .expect("earlier step");
    control
        .fail_at(&mut second, BuildPoint::View, 2, 202)
        .expect("no second build fault");
    assert!(
        control
            .fail_at(&mut first, BuildPoint::View, 1, 101)
            .is_err()
    );
    control
        .fail_at(&mut first, BuildPoint::View, 1, 101)
        .expect("one injection");
    assert!(
        control.dispatch_destroyed(9),
        "a foreign event remains real"
    );
    assert!(
        control.dispatch_destroyed(1),
        "default mode delivers to the real stage"
    );
    assert_eq!(control.take_deferred_destroyed(), None);
}

#[test]
fn an_unreached_point_cannot_move_to_a_healthy_retry() {
    let mut control = FailureControl::<u8>::new(Some(FailureSpec::parse("view").expect("fixture")));
    let _first_that_failed_earlier = control.begin_build(1);
    let mut retry = control.begin_build(2);
    control
        .fail_at(&mut retry, BuildPoint::View, 2, 202)
        .expect("claimed fault was consumed");
    assert!(control.dispatch_destroyed(2));
}

#[test]
fn swallow_never_delivers_the_matching_event_or_posts_a_deferred_wake() {
    let mut control =
        FailureControl::<u8>::new(Some(FailureSpec::parse("hwnd:swallow").expect("fixture")));
    let mut attempt = control.begin_build(1);
    assert!(
        control
            .fail_at(&mut attempt, BuildPoint::Hwnd, 7, 707)
            .is_err()
    );
    for _ in 0..2 {
        assert!(!control.dispatch_destroyed(7));
        assert!(control.dispatch_destroyed(8));
        control
            .post_deferred_destroyed(true, true, || panic!("swallow never posts"))
            .expect("no wake");
        assert_eq!(control.take_deferred_destroyed(), None);
    }
}

#[test]
fn defer_posts_once_after_terminal_and_barrier_then_consumes_once() {
    let mut control =
        FailureControl::<u8>::new(Some(FailureSpec::parse("hwnd:defer").expect("fixture")));
    let mut attempt = control.begin_build(1);
    assert!(
        control
            .fail_at(&mut attempt, BuildPoint::Hwnd, 7, 707)
            .is_err()
    );
    control
        .post_deferred_destroyed(true, true, || panic!("no actual Destroyed yet"))
        .expect("not held");
    assert!(control.dispatch_destroyed(8));
    assert!(!control.dispatch_destroyed(7));
    for (terminal, ready) in [(false, false), (false, true), (true, false)] {
        control
            .post_deferred_destroyed(terminal, ready, || panic!("not eligible"))
            .expect("wait");
        assert_eq!(control.take_deferred_destroyed(), None);
    }
    let mut posts = 0;
    control
        .post_deferred_destroyed(true, true, || {
            posts += 1;
            true
        })
        .expect("eligible wake");
    assert!(
        !control.dispatch_destroyed(7),
        "duplicate actual event cannot bypass the next turn"
    );
    control
        .post_deferred_destroyed(true, true, || {
            posts += 1;
            true
        })
        .expect("one post");
    assert_eq!(posts, 1);
    assert_eq!(control.take_deferred_destroyed(), Some(7));
    assert_eq!(control.take_deferred_destroyed(), None);
    control
        .post_deferred_destroyed(true, true, || panic!("already consumed"))
        .expect("done");
}

#[test]
fn failed_deferred_post_is_an_error_without_a_retry_or_fake_consumption() {
    let mut control =
        FailureControl::<u8>::new(Some(FailureSpec::parse("hwnd:defer").expect("fixture")));
    let mut attempt = control.begin_build(1);
    assert!(
        control
            .fail_at(&mut attempt, BuildPoint::Hwnd, 7, 707)
            .is_err()
    );
    assert!(!control.dispatch_destroyed(7));
    let error = control
        .post_deferred_destroyed(true, true, || false)
        .expect_err("failed post");
    assert!(
        error
            .to_string()
            .contains("deferred Destroyed wake post failed"),
        "{error}"
    );
    assert_eq!(control.take_deferred_destroyed(), None);
    assert!(!control.dispatch_destroyed(7));
    control
        .post_deferred_destroyed(true, true, || panic!("one attempted post only"))
        .expect("failed attempt retained");
}

#[test]
fn injected_build_observation_starts_at_failure_and_survives_event_consumption() {
    for (value, deferred) in [("hwnd", false), ("hwnd:defer", true)] {
        let mut control =
            FailureControl::<u8>::new(Some(FailureSpec::parse(value).expect("fixture")));
        assert!(!control.observes_failed_build(41));
        let mut attempt = control.begin_build(41);
        assert!(
            !control.observes_failed_build(41),
            "a claim is not an injected fault"
        );
        control
            .fail_at(&mut attempt, BuildPoint::Webview, 7, 707)
            .expect("wrong point is not the fault");
        assert!(!control.observes_failed_build(41));
        assert!(
            control
                .fail_at(&mut attempt, BuildPoint::Hwnd, 7, 707)
                .is_err()
        );
        assert!(control.observes_failed_build(41));
        assert!(!control.observes_failed_build(42));
        if deferred {
            assert!(!control.dispatch_destroyed(7));
            control
                .post_deferred_destroyed(true, true, || true)
                .expect("wake");
            assert_eq!(control.take_deferred_destroyed(), Some(7));
        } else {
            assert!(control.dispatch_destroyed(7));
        }
        assert!(control.failed.is_none());
        assert!(
            control.observes_failed_build(41),
            "the later answer must still be observed"
        );
        let mut retry = control.begin_build(42);
        control
            .fail_at(&mut retry, BuildPoint::Hwnd, 8, 808)
            .expect("healthy retry");
        assert!(!control.observes_failed_build(42));
        assert!(control.observes_failed_build(41));
    }
}
