//! Pure sequences through the production observation gate; no native calls.
use super::ActivationObserver;
use windows::Win32::Foundation::HWND;

fn hwnd(value: usize) -> HWND {
    // A comparison token only. These handles never reach a Windows API.
    HWND(std::ptr::without_provenance_mut(value))
}

fn observe_current(
    observer: &ActivationObserver,
    active: HWND,
    live: &[(u32, HWND)],
    facts: &mut Vec<(u32, bool)>,
) {
    observer
        .observe_sample(active, |active| {
            facts.extend(live.iter().map(|(id, hwnd)| (*id, *hwnd == active)));
            Ok::<(), ()>(())
        })
        .expect("synthetic current-map observer");
}

#[test]
fn newer_direct_fact_invalidates_an_equal_outer_activation_sample() {
    let observer = ActivationObserver::default();
    let a = hwnd(1);
    let b = hwnd(2);
    let live = [(10, a), (20, b)];
    let mut facts = Vec::new();
    observe_current(&observer, a, &live, &mut facts);
    assert_eq!(facts, [(10, true), (20, false)]);
    facts.clear();

    // The direct callback observes a newer native transition before the next
    // outer message. Its invalidation must survive a return to the old HWND.
    observer.invalidate();
    facts.push((10, false));
    facts.push((20, true));
    observe_current(&observer, a, &live, &mut facts);
    assert_eq!(
        facts,
        [(10, false), (20, true), (10, true), (20, false)],
        "A -> direct B fact -> A must reconcile the current live map",
    );
}

#[test]
fn new_membership_at_a_reused_hwnd_is_observed_from_the_current_map() {
    let observer = ActivationObserver::default();
    let a = hwnd(1);
    let mut facts = Vec::new();
    observe_current(&observer, a, &[(10, a)], &mut facts);
    facts.clear();

    // A different live view owns the same raw handle. Its Created snapshot
    // sampled a transient inactive state; the hook must not keep the old gate.
    observer.invalidate();
    facts.push((30, false));
    observe_current(&observer, a, &[(30, a)], &mut facts);
    assert_eq!(
        facts,
        [(30, false), (30, true)],
        "a current view must be resampled without remembering the old ID",
    );
}

#[test]
fn unchanged_raw_sample_skips_work_until_an_edge_or_invalidation() {
    let observer = ActivationObserver::default();
    let a = hwnd(1);
    let mut facts = Vec::new();
    observe_current(&observer, a, &[(10, a)], &mut facts);
    observer
        .observe_sample(a, |_| -> Result<(), ()> {
            panic!("unchanged native sample must not rescan");
        })
        .expect("unchanged sample");
    observe_current(&observer, HWND::default(), &[(10, a)], &mut facts);
    observe_current(&observer, a, &[(10, a)], &mut facts);
    assert_eq!(facts, [(10, true), (10, false), (10, true)]);
}

#[test]
fn current_map_error_propagates_to_the_observer() {
    let observer = ActivationObserver::default();
    assert_eq!(
        observer.observe_sample(hwnd(1), |_| Err("current owner unavailable")),
        Err("current owner unavailable"),
    );
}
