//! Pure control-state checks only; no native Event/Close receipt is modeled.
use super::*;

#[test]
fn arming_never_bypasses_an_existing_current_delivery() {
    let mut hold = OutcomeHold::new(PathBuf::from(r"D:\unused-x10"), HoldMode::BeforeIntake);
    hold.phase = Phase::Arming;
    hold.before_turn(false);
    assert_eq!(hold.phase, Phase::Arming);
    assert!(!hold.is_holding());
    hold.before_turn(true);
    assert!(hold.is_holding());
    assert!(hold.just_armed && hold.just_held);
}

#[test]
fn after_event_selects_once_and_keeps_exact_scalar_identity() {
    let mut hold = OutcomeHold::new(PathBuf::from(r"D:\unused-x10"), HoldMode::AfterNextEvent);
    hold.phase = Phase::Arming;
    hold.before_turn(true);
    assert_eq!(hold.phase, Phase::Armed);
    hold.selected_event(None);
    assert!(!hold.is_holding());
    let first = EventKey {
        webview: 11,
        navigation: 7,
        delivery: 19,
    };
    hold.selected_event(Some(first));
    assert!(hold.is_holding());
    hold.selected_event(Some(EventKey {
        webview: 22,
        navigation: 8,
        delivery: 20,
    }));
    assert_eq!(hold.event, Some(first));
}

#[test]
fn expired_budget_fails_without_implicitly_releasing_the_hold() {
    let mut hold = OutcomeHold::new(PathBuf::from(r"D:\unused-x10"), HoldMode::BeforeIntake);
    hold.phase = Phase::Holding;
    hold.deadline = Some(Instant::now());
    let original = hold.deadline;
    assert!(hold.poll().is_err());
    assert!(hold.is_holding());
    assert_eq!(hold.deadline, original);
    assert!(!hold.just_released);
}

fn directory(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("keld-x10-{label}-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&path).expect("unique control fixture directory");
    path
}

#[test]
fn real_arm_and_release_files_select_one_finite_hold() {
    let directory = directory("files");
    let mut hold = OutcomeHold::new(directory.clone(), HoldMode::BeforeIntake);
    hold.poll().expect("absent markers are idle");
    let idle = hold.phase;
    std::fs::write(directory.join("arm"), b"arm\n").expect("arm marker");
    hold.poll().expect("actual arm");
    let deadline = hold.deadline;
    hold.before_turn(true);
    let was_holding = hold.is_holding();
    std::fs::write(directory.join("release"), b"release\n").expect("release marker");
    hold.poll().expect("actual release before deadline");
    let released = hold.phase;
    hold.poll().expect("released control cannot rearm");
    std::fs::remove_file(directory.join("arm")).expect("remove arm");
    std::fs::remove_file(directory.join("release")).expect("remove release");
    std::fs::remove_dir(directory).expect("remove control directory");
    assert_eq!(idle, Phase::Waiting);
    assert!(was_holding && deadline.is_some());
    assert_eq!(released, Phase::Released);
    assert_eq!(hold.phase, Phase::Released);
    assert_eq!(hold.deadline, deadline, "the budget never resets");
}

#[test]
fn early_release_and_nonfile_markers_cannot_grant_a_cut() {
    let directory = directory("invalid");
    let mut early = OutcomeHold::new(directory.clone(), HoldMode::BeforeIntake);
    std::fs::write(directory.join("release"), b"early\n").expect("early release");
    let refused_early = early.poll().is_err();
    std::fs::remove_file(directory.join("release")).expect("remove early marker");
    std::fs::create_dir(directory.join("arm")).expect("non-file arm");
    let mut malformed = OutcomeHold::new(directory.clone(), HoldMode::BeforeIntake);
    let refused_nonfile = malformed.poll().is_err();
    std::fs::remove_dir(directory.join("arm")).expect("remove non-file arm");
    std::fs::remove_dir(directory).expect("remove control directory");
    assert!(refused_early && refused_nonfile);
    assert_eq!(early.phase, Phase::Waiting);
    assert_eq!(malformed.phase, Phase::Waiting);
    assert!(!early.is_holding() && !malformed.is_holding());
}
