//! Shipping public-main FS component. No renderer-action or Echo-forwarding acceptance.
use super::project::{Exercise, Policy, PublicFsProject};
use super::run_launch;

#[test]
fn public_app_fs_component_runs_without_synthetic_input_and_relaunches() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    let first = run_launch(&fixture, 0, Exercise::PublicAppComponent);
    let second = run_launch(&fixture, 1, Exercise::PublicAppComponent);
    let first_document = first
        .reports
        .iter()
        .find(|report| report["phase"] == "page-ready")
        .expect("first actual document identity");
    let second_document = second
        .reports
        .iter()
        .find(|report| report["phase"] == "page-ready")
        .expect("healthy relaunch document identity");
    for key in ["documentNonce", "javascriptToken"] {
        let before = first_document[key].as_str().expect("page-created identity");
        let after = second_document[key]
            .as_str()
            .expect("new page-created identity");
        assert_eq!(before.len(), 32);
        assert_eq!(after.len(), 32);
        assert!(before.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(after.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(
            before, after,
            "healthy launch reused old volatile document identity"
        );
    }
    eprintln!(
        "KELD_KEL140_PUBLIC_APP_COMPONENT_ACCEPTANCE launches=2 input_actions=0 renderer_action_acceptance=false echo_forwarding_acceptance=false"
    );
}

#[test]
fn public_app_explicit_empty_policy_denies_without_effect() {
    let fixture = PublicFsProject::new(Policy::Empty);
    run_launch(&fixture, 0, Exercise::PublicAppComponent);
}

#[test]
fn public_app_absent_policy_denies_without_effect() {
    let fixture = PublicFsProject::new(Policy::Absent);
    run_launch(&fixture, 0, Exercise::PublicAppComponent);
}
