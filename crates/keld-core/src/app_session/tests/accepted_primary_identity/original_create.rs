//! The selected Create producer uses real retained source and Runtime Init facts.

use super::{AcceptedRole, PrimaryRoleConfig, RoleOwner};
use crate::app_session::graph_contract::tests::prepared_original_window_source;
use crate::app_session::window_router::original::{
    OriginalWindowCreateRefusal, PreparedOriginalWindowCreate,
};
use crate::app_session::{
    AppWindowCommand, PlatformPrimaryOwnerHandle, PrimaryCallerContext, PrimaryOwnerUpdate,
    PrimaryRouter, SessionShutdownState,
};
use keld_ipc::CorrelationId;
use keld_ipc::window::WindowRequest;
use keld_wv::ApplicationNameForUserAgent;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

#[test]
fn windows_original_create_preparation_binds_exact_source_runtime_and_start_cut() {
    let _custody = keld_runtime::windows_job::install_host_death_job()
        .expect("install process-lifetime custody before starting a role");
    let expected_name = "fixture/1.0";
    let (_stage, prepared, selected) = prepared_original_window_source(
        ApplicationNameForUserAgent::new(expected_name.to_owned()).expect("valid boot field"),
    );
    let mut role = AcceptedRole::start_with_config(
        PrimaryRoleConfig::primary("bun")
            .arg("--define=__KELD_RUNTIME_PROCESS_FACTS__:true")
            .require_runtime_process_facts(),
    );
    let (bound, _control) = role.bind();
    let versions = bound
        .runtime_versions()
        .expect("actual Bun versions")
        .clone();
    assert!(versions.contains_key("bun"));
    let selection = prepared
        .bind_runtime(&bound)
        .expect("bind authenticated facts");
    selection
        .validate_runtime_versions(Some(&versions))
        .expect("identical Runtime facts preserve selection");
    let mut changed_versions = versions.clone();
    changed_versions.remove("bun");
    assert!(
        selection
            .validate_runtime_versions(Some(&changed_versions))
            .is_err()
    );
    let (guardian, _commands) = mpsc::channel();
    let (window, commands) = mpsc::channel();
    let router = PrimaryRouter::start_bound(
        bound,
        window,
        PlatformPrimaryOwnerHandle {
            command_tx: guardian,
        },
        SessionShutdownState::new(),
        None,
        Some(true),
        Some(Arc::new(selection)),
    )
    .expect("actual primary with prepared original source");
    super::expect_original_desktop_binding(&router, &commands);
    let handle = router.handle();
    let request = |path: &Path| WindowRequest::CreateWithPreload {
        width: 640,
        height: 480,
        x: Some(10),
        y: None,
        preload_absolute: path.to_str().expect("UTF-8 checked fixture").to_owned(),
        additional_arguments: vec!["--fixture-argument".to_owned()],
    };
    let started = Instant::now();
    let (caller, command) = {
        let _transition = handle.shutdown.transition_guard();
        assert!(
            handle
                .mark_window_registry_ready_guarded()
                .expect("serviceable registry")
        );
        let current = handle.current.lock().expect("current owner");
        let active = current.as_ref().expect("installed primary");
        let caller = PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        let mut session = handle
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("Registry");
        assert!(matches!(
            handle.prepare_original_window_create_locked(
                &mut session,
                &caller,
                CorrelationId(1),
                request(&selected.with_file_name("alternate.cjs")),
                started,
            ),
            Err(OriginalWindowCreateRefusal::Preload(error)) if error.to_string().contains("selector")
        ));
        assert!(matches!(
            handle.prepare_original_window_create_locked(
                &mut session,
                &caller,
                CorrelationId(2),
                request(&selected),
                started - Duration::from_secs(36),
            ),
            Err(OriginalWindowCreateRefusal::Deadline)
        ));
        let command = handle
            .prepare_original_window_create_locked(
                &mut session,
                &caller,
                CorrelationId(3),
                request(&selected),
                started,
            )
            .expect("first qualified selected Create");
        (caller, command)
    };
    let PreparedOriginalWindowCreate::Build(AppWindowCommand::CreateWindow {
        build,
        width: 640,
        height: 480,
        x: Some(10),
        y: None,
        original_source: Some(source),
        preload_image: Some(preload),
        original_role: Some(witness),
        original_application_name_for_user_agent: Some(name),
        original_create_start_by: Some(cut),
    }) = command
    else {
        panic!("qualified original Create did not prepare the exact native inputs");
    };
    assert_eq!(build, 1, "prior refusals cannot allocate a window identity");
    assert_eq!(source.window_key(), "editor");
    assert_eq!(preload.versions(), &versions);
    assert_eq!(preload.additional_arguments(), &["--fixture-argument"]);
    assert_eq!(name.as_str(), expected_name);
    assert_eq!(cut, started + Duration::from_secs(35));
    assert!(witness.matches(&caller.reader_stop));
    assert!(witness.try_admit_native(&mut || true));
    assert!(
        commands.try_recv().is_err(),
        "preparation owns no UI publication"
    );
    let mut unqualified_role = AcceptedRole::start(RoleOwner::Primary);
    let (unqualified, _unqualified_control) = unqualified_role.bind();
    assert!(unqualified.runtime_versions().is_none());
    assert!(prepared.bind_runtime(&unqualified).is_err());
    let refusal = handle
        .apply_generation_update(PrimaryOwnerUpdate::Bound(unqualified))
        .expect_err("authenticated successor lacks the selected Runtime facts");
    assert!(
        refusal
            .error
            .to_string()
            .contains("immutable preload Runtime process facts")
    );
    assert!(
        witness.try_admit_native(&mut || true),
        "refusal did not replace the current role"
    );
    router.shutdown().expect("router cleanup");
    for owner in [&mut role, &mut unqualified_role] {
        assert!(matches!(
            owner.finish(),
            Some(keld_runtime::SupervisorOutcome::Stopped)
        ));
    }
}
