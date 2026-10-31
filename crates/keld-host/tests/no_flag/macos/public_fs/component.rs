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
    for document in [first_document, second_document] {
        assert_eq!(
            document.get("button"),
            Some(&serde_json::Value::Null),
            "public-main-only page must not expose renderer activation"
        );
    }
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

#[test]
fn explicit_empty_policy_independently_denies_seeded_public_read() {
    let fixture = PublicFsProject::new(Policy::Empty);
    run_launch(&fixture, 0, Exercise::PublicAppNaturalCases);
    eprintln!("KELD_KEL140_NATURAL_READ_DENY policy=explicit-empty input_actions=0 echo_calls=0");
}

#[test]
fn absent_policy_independently_denies_seeded_public_read() {
    let fixture = PublicFsProject::new(Policy::Absent);
    run_launch(&fixture, 0, Exercise::PublicAppNaturalCases);
    eprintln!("KELD_KEL140_NATURAL_READ_DENY policy=absent input_actions=0 echo_calls=0");
}

#[test]
fn public_app_static_links_and_native_content_limits_leave_healthy_session() {
    let fixture = PublicFsProject::new(Policy::Narrow);
    run_launch(&fixture, 0, Exercise::PublicAppNaturalCases);
    eprintln!(
        "KELD_KEL140_NATURAL_LINKS_LIMITS input_actions=0 echo_calls=0 race_acceptance=false deadline_acceptance=false blocked_syscall_acceptance=false"
    );
}

#[test]
fn public_app_native_traversal_boundaries_preserve_sentinels_and_healthy_session() {
    let fixture = PublicFsProject::with_traversal_boundaries();
    run_launch(&fixture, 0, Exercise::PublicAppNaturalCases);
    eprintln!(
        "KELD_KEL140_NATURAL_TRAVERSAL_BOUNDS input_actions=0 echo_calls=0 race_acceptance=false deadline_acceptance=false blocked_syscall_acceptance=false"
    );
}

/// KEL-118: actual public Quit with the measured Python-peer fork shape.
#[test]
fn public_quit_reaps_a_stdout_inheriting_descendant_after_bun_exit_zero() {
    let mut descendant = super::descendant_quit::DescendantQuit::bind();
    let mut fixture = PublicFsProject::new(Policy::Narrow);
    fixture.set_descendant_quit(descendant.configuration());
    super::run_launch_with_descendant_quit(
        &fixture,
        0,
        Exercise::PublicAppComponent,
        Some(&mut descendant),
    );
    // Healthy next launch retains the existing ordinary component/FS/policy route.
    fixture.set_descendant_quit(serde_json::Value::Null);
    run_launch(&fixture, 1, Exercise::PublicAppComponent);
}
