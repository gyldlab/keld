//! Actual controller/guard/state paths, with literal state seeds only.
//! Literal seeds do not qualify login/provider/RNG completion.
//! The named single-flight socket case exercises the real SDK transport only.
use super::super::AuthProfileConfiguration;
use super::*;
use keld_guard::parse_manifest;

fn broker() -> AuthBroker {
    let mut configuration = AuthConfiguration::new();
    configuration.insert(
        "entra-common".to_owned(),
        AuthProfileConfiguration {
            client_id: Some("literal-client".to_owned()),
            fixed_redirect: None,
            as_redirect: None,
        },
    );
    AuthBroker {
        prepared_digest: [0; 32],
        configuration,
        as_port: None,
        registry: SessionRegistry::new(),
        owner: Arc::new(()),
        next_operation: 0,
        jobs: Vec::default(),
        test_token_endpoint: None,
    }
}
fn seed_account(broker: &mut AuthBroker) -> String {
    match broker
        .registry
        .register_completed_begin(
            "entra-common",
            [8; 32],
            Some("refresh-private".to_owned()),
            None,
            None,
            vec![],
        )
        .expect("literal state seed, not login")
    {
        AuthResponse::Begin { account_id, .. } => account_id,
        AuthResponse::Token { .. } => panic!("wrong state response"),
    }
}
fn granted_manifest() -> PermissionsManifest {
    parse_manifest(r#"{"app":{"auth":{"token":["https://login.microsoftonline.com/common"]}}}"#)
        .expect("actual parsed manifest")
}
#[test]
fn cancelled_current_token_job_emits_one_canonical_event_before_physical_drain() {
    let mut broker = broker();
    let account_id = seed_account(&mut broker);
    let manifest = granted_manifest();
    let first = broker.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Token {
            account_id: account_id.clone(),
            scopes: None,
        },
        || Some(()),
    );
    let operation = match first {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("expected admitted physical owner {other:?}"),
    };
    let same = broker.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Token {
            account_id,
            scopes: Some(vec![]),
        },
        || -> Option<()> { panic!("coalesced caller cannot admit twice") },
    );
    match same {
        HandleState::Pending {
            operation: same,
            admission: None,
        } => assert_eq!(same, operation),
        other => panic!("expected same native operation {other:?}"),
    }
    assert!(broker.cancel(&operation, CallError::generation_retired()));
    let mut events = vec![];
    assert_eq!(
        broker.poll(&operation, |event| events.push(event)),
        Progress::Drained
    );
    assert_eq!(
        events.len(),
        1,
        "current job removed for polling must still fan out its canonical result"
    );
    assert_eq!(events[0].operation, operation);
    let response = events
        .pop()
        .expect("actual event")
        .response
        .into_response()
        .expect_err("Core-owned physical cancellation");
    assert_eq!(response.code, "KELD-IPC-023");
    assert_eq!(
        broker.poll(&operation, |_| panic!("no second event")),
        Progress::Drained
    );
}

#[test]
fn foreign_operation_cannot_receipt_another_brokers_physical_drain() {
    let mut owner = broker();
    let account_id = seed_account(&mut owner);
    let manifest = granted_manifest();
    let operation = match owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Token {
            account_id,
            scopes: None,
        },
        || Some(()),
    ) {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("expected actual controller pending operation: {other:?}"),
    };
    let mut foreign = broker();
    let progress = foreign.poll(&operation, |_| {
        panic!("foreign worker cannot emit owner result")
    });
    let owner_retained_job = owner.jobs.len() == 1;
    let foreign_has_no_job = foreign.jobs.is_empty();
    // The actual-owner cleanup is independent of the foreign observation and
    // occurs BEFORE assertions, including under the baseline wrong receipt.
    let cancelled = owner.cancel(&operation, CallError::generation_retired());
    let mut events = vec![];
    let actual_drain = owner.poll(&operation, |event| events.push(event));
    assert!(
        matches!(progress, Progress::HostFault(HostFault::OwnerMismatch)),
        "foreign owner produced a drain receipt: {progress:?}"
    );
    assert!(
        owner_retained_job,
        "actual owner must retain its admitted job"
    );
    assert!(foreign_has_no_job);
    assert!(cancelled);
    assert_eq!(actual_drain, Progress::Drained);
    assert_eq!(events.len(), 1);
    assert_eq!(
        owner.poll(&operation, |_| panic!("retired id cannot reemit")),
        Progress::Drained
    );
    assert!(!foreign.cancel(&operation, CallError::generation_retired()));
}

// Append inside the existing controller_tests.rs module. Reuse broker() and
// seed_account(); no public test seam, new provider or SDK fixture is introduced.
// These call the actual private broker/registry/dispatcher and encode CallError,
// but do not prove authentication, socket publication, login or SDK effects.

// Independent literal postcard oracles from the frozen parent renderer tests.
// Expected bytes do not invoke TokenDenial.parts() or encode an expected DTO.
const CONTROLLER_TOKEN_001_WIRE: &[u8] = b"\x0dKELD-GUARD001\x78KELD-GUARD001: auth.token is not granted. Add the intended declared issuer to /app/auth/token in keld.permissions.jsonc.";
const CONTROLLER_TOKEN_002_WIRE: &[u8] = b"\x0dKELD-GUARD002\x7eKELD-GUARD002: auth.token is outside its grant. Add the intended declared issuer to /app/auth/token in keld.permissions.jsonc.";

fn permission_ready_error(state: HandleState<()>) -> Arc<CallError> {
    match state {
        HandleState::Ready(response) => response
            .into_response()
            .expect_err("the actual broker must return a denial/error"),
        other => panic!("expected actual ready error, got {other:?}"),
    }
}

fn permission_token_denial(
    owner: &mut AuthBroker,
    manifest: &PermissionsManifest,
    account_id: &str,
) -> Vec<u8> {
    let admissions = std::cell::Cell::new(0);
    let state = owner.handle_manifest_request(
        manifest,
        Principal::AppProcess,
        AuthRequest::Token {
            account_id: account_id.to_owned(),
            scopes: Some(vec!["private-requested-scope".to_owned()]),
        },
        || -> Option<()> {
            admissions.set(admissions.get() + 1);
            None // If Guard is bypassed, expose AtCapacity before any native I/O.
        },
    );
    let error = permission_ready_error(state);
    assert_eq!(admissions.get(), 0, "denial entered physical admission");
    assert!(owner.jobs.is_empty(), "denial installed a native job");
    assert_eq!(owner.next_operation, 0, "denial minted an operation id");
    for private in [account_id, "private-requested-scope", "\0unresolved"] {
        assert!(!error.message.contains(private), "{error:?}");
    }
    keld_ipc::codec::encode(error.as_ref()).expect("encode actual controller error")
}

#[test]
fn controller_begin_denial_precedes_request_fields_and_physical_admission() {
    let manifest = parse_manifest("{}").expect("actual default-deny manifest");
    let mut expected = None;
    for configured in [true, false] {
        for hint in [
            None,
            Some("valid@example.test"),
            Some("private-hint\r\nforged"),
        ] {
            let mut owner = broker();
            if !configured {
                owner.configuration.clear();
            }
            let admissions = std::cell::Cell::new(0);
            let state = owner.handle_manifest_request(
                &manifest,
                Principal::AppProcess,
                AuthRequest::Begin {
                    provider: "entra-common".to_owned(),
                    scopes: Some(vec!["openid".to_owned()]),
                    login_hint: hint.map(str::to_owned),
                },
                || -> Option<()> {
                    admissions.set(admissions.get() + 1);
                    None
                },
            );
            let error = permission_ready_error(state);
            assert_eq!(error.code, "KELD-GUARD001");
            assert!(!error.message.contains("private-hint"));
            assert_eq!(admissions.get(), 0);
            assert!(owner.jobs.is_empty());
            assert_eq!(owner.next_operation, 0);
            let bytes = keld_ipc::codec::encode(error.as_ref()).expect("actual Begin denial");
            if let Some(expected) = &expected {
                assert_eq!(
                    &bytes, expected,
                    "fields/configuration became a pre-Allow oracle"
                );
            } else {
                expected = Some(bytes);
            }
        }
    }
}

#[test]
fn controller_begin_allow_reaches_validation_then_capacity_without_sdk_work() {
    let manifest = parse_manifest(
        r#"{"app":{"auth":{"begin":["https://login.microsoftonline.com/common"]}}}"#,
    )
    .expect("actual declared-issuer grant");
    let mut owner = broker();
    let admissions = std::cell::Cell::new(0);
    let valid = owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: None,
        },
        || -> Option<()> {
            admissions.set(admissions.get() + 1);
            None
        },
    );
    assert!(matches!(valid, HandleState::AtCapacity));
    assert_eq!(
        admissions.get(),
        1,
        "actual Allow must enter the admission boundary once"
    );
    assert!(owner.jobs.is_empty());
    assert_eq!(owner.next_operation, 0);

    let invalid = owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: Some("private-allowed-hint\r\nforged".to_owned()),
        },
        || -> Option<()> { panic!("invalid post-Allow hint reached admission") },
    );
    let error = permission_ready_error(invalid);
    assert_eq!(error.code, "KELD-AUTH-010");
    assert!(!error.message.contains("private-allowed-hint"));
    owner.configuration.clear();
    let unconfigured = owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: None,
        },
        || -> Option<()> { panic!("missing post-Allow client reached admission") },
    );
    assert_eq!(permission_ready_error(unconfigured).code, "KELD-AUTH-015");
    assert!(owner.jobs.is_empty());
}

#[test]
fn controller_unknown_provider_is_the_only_normal_pre_allow_auth_error() {
    let mut owner = broker();
    owner.configuration.clear();
    let manifest = parse_manifest("{}").expect("no auth grants");
    let state = owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Begin {
            provider: "not-a-compiled-provider".to_owned(),
            scopes: None,
            login_hint: Some("private-unknown-hint\r\nforged".to_owned()),
        },
        || -> Option<()> { panic!("unknown provider entered admission") },
    );
    let error = permission_ready_error(state);
    assert_eq!(error.code, "KELD-AUTH-002");
    assert!(!error.message.contains("private-unknown-hint"));
    assert!(owner.jobs.is_empty());
    assert_eq!(owner.next_operation, 0);
}

#[test]
fn controller_live_and_missing_token_denials_use_literal_001_bytes() {
    for source in [
        r"{}",
        r#"{"app":{"auth":{"token":true}}}"#,
        r#"{"app":{"auth":{"token":[]}}}"#,
        r#"{"app":{"auth":{"token":["https://issuer-p.test",7]}}}"#,
    ] {
        let mut owner = broker();
        let live = seed_account(&mut owner);
        assert_eq!(
            owner.registry.issuer_for(&live),
            Some("https://login.microsoftonline.com/common")
        );
        let manifest = parse_manifest(source).expect("actual NotGranted shapes");
        let live_wire = permission_token_denial(&mut owner, &manifest, &live);
        let missing_wire =
            permission_token_denial(&mut owner, &manifest, "private-missing-selector");
        assert_eq!(live_wire, CONTROLLER_TOKEN_001_WIRE);
        assert_eq!(missing_wire, CONTROLLER_TOKEN_001_WIRE);
        assert_eq!(live_wire, missing_wire);
    }
}

#[test]
fn controller_grant_p_live_closed_q_and_missing_use_literal_002_bytes() {
    // Q is the existing compiled entra-common profile. P is only a manifest
    // literal unequal to Q; no second provider/profile/schema is introduced.
    let mut owner = broker();
    let live_q = seed_account(&mut owner);
    let manifest = parse_manifest(r#"{"app":{"auth":{"token":["https://issuer-p.test"]}}}"#)
        .expect("present nonempty all-string different-issuer grant");
    let live_wire = permission_token_denial(&mut owner, &manifest, &live_q);
    let missing_wire = permission_token_denial(&mut owner, &manifest, "private-unresolved-q");
    assert_eq!(live_wire, CONTROLLER_TOKEN_002_WIRE);
    assert_eq!(missing_wire, CONTROLLER_TOKEN_002_WIRE);
    assert_eq!(live_wire, missing_wire);
    assert_ne!(CONTROLLER_TOKEN_001_WIRE, CONTROLLER_TOKEN_002_WIRE);
}

#[test]
fn controller_real_serde_sentinel_allow_reports_only_002_and_host_fault() {
    let text = r#"{"app":{"auth":{"token":["\u0000unresolved"]}}}"#;
    assert!(
        parse_manifest(text).is_err(),
        "shared load must still reject sentinel grant"
    );
    let manifest: PermissionsManifest =
        serde_json::from_str(text).expect("existing public Deserialize seam, not a new bypass");
    let mut owner = broker();
    let missing = "private-forced-unresolved-selector";
    assert!(owner.registry.issuer_for(missing).is_none());
    let admissions = std::cell::Cell::new(0);
    let state = owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Token {
            account_id: missing.to_owned(),
            scopes: None,
        },
        || -> Option<()> {
            admissions.set(admissions.get() + 1);
            None
        },
    );
    // This variant is reachable only after actual dispatch returned Allow
    // while the actual registry lookup was None. No DenyReason is fabricated.
    let (response, fault) = match state {
        HandleState::DeniedInvariant { response, fault } => (response, fault),
        other => panic!("actual Allow-with-None defense not exercised: {other:?}"),
    };
    assert_eq!(fault, HostFault::UnresolvedAfterAllow);
    let error = response
        .into_response()
        .expect_err("no token may be minted");
    assert_eq!(
        keld_ipc::codec::encode(error.as_ref()).expect("actual synthetic wire DTO"),
        CONTROLLER_TOKEN_002_WIRE
    );
    assert_eq!(admissions.get(), 0);
    assert!(owner.jobs.is_empty());
    assert_eq!(owner.next_operation, 0);
    assert!(owner.registry.issuer_for(missing).is_none());
    assert!(!error.message.contains(missing));
    assert!(!error.message.contains("unresolved"));
}

#[path = "controller_tests/singleflight.rs"]
mod singleflight;

#[test]
fn native_singleflight_real_sdk_one_exchange_and_healthy_cache() {
    super::super::macos_http::tests::isolated_selector(
        "native_singleflight_real_sdk_one_exchange_and_healthy_cache",
        "auth::controller::tests::native_singleflight_real_sdk_one_exchange_and_healthy_cache",
        singleflight::run_sdk_case,
    );
}

#[test]
fn as_registration_is_validated_only_after_actual_begin_allow() {
    let mut broker = broker();
    broker
        .configuration
        .get_mut("entra-common")
        .expect("profile config")
        .as_redirect = Some("malformed-secret-free-fixture".to_owned());
    let request = || AuthRequest::Begin {
        provider: "entra-common".to_owned(),
        scopes: None,
        login_hint: None,
    };
    let denied = parse_manifest(r#"{"app":{}}"#).expect("default deny");
    let result = broker.handle_manifest_request(
        &denied,
        Principal::AppProcess,
        request(),
        || -> Option<()> {
            panic!("denial cannot admit");
        },
    );
    let HandleState::Ready(response) = result else {
        panic!("guard denial must be ready");
    };
    assert_eq!(
        response.into_response().expect_err("actual denial").code,
        "KELD-GUARD001"
    );
    let allowed = parse_manifest(
        r#"{"app":{"auth":{"begin":["https://login.microsoftonline.com/common"]}}}"#,
    )
    .expect("literal begin grant");
    let result = broker.handle_manifest_request(
        &allowed,
        Principal::AppProcess,
        request(),
        || -> Option<()> {
            panic!("malformed binding cannot admit");
        },
    );
    let HandleState::Ready(response) = result else {
        panic!("post-Allow configuration error must be ready");
    };
    assert_eq!(
        response.into_response().expect_err("invalid binding").code,
        "KELD-AUTH-015"
    );
    assert!(broker.jobs.is_empty());
}

#[test]
fn known_compliance_class_uses_actual_guard_before_typed_no_effect_deny() {
    use std::cell::Cell;
    let mut owner = broker();
    // Deliberately missing/invalid setup proves the known-class cut precedes
    // client/redirect processing, rather than selecting AS or browser fallback.
    let configuration = owner
        .configuration
        .get_mut("entra-common")
        .expect("profile config");
    configuration.client_id = None;
    configuration.as_redirect = Some("invalid-fixture-binding".to_owned());
    configuration.fixed_redirect = Some("invalid-fixture-loopback".to_owned());
    let request = || AuthRequest::Begin {
        provider: "entra-common".to_owned(),
        scopes: None,
        login_hint: None,
    };
    let admissions = Cell::new(0_u32);
    let admit = || {
        admissions.set(admissions.get() + 1);
        Some(())
    };
    let denied = parse_manifest(r#"{"app":{}}"#).expect("actual default deny");
    let denied = owner.handle_classified_manifest_request(
        &denied,
        Principal::AppProcess,
        request(),
        TrustedFlowClass::KnownRequiresDeviceCompliance,
        admit,
    );
    let HandleState::Ready(response) = denied else {
        panic!("Guard denial must remain ready");
    };
    assert_eq!(
        response
            .into_response()
            .expect_err("actual Guard denial")
            .code,
        "KELD-GUARD001"
    );
    let allowed = parse_manifest(
        r#"{"app":{"auth":{"begin":["https://login.microsoftonline.com/common"]}}}"#,
    )
    .expect("actual literal grant");
    let denied = owner.handle_classified_manifest_request(
        &allowed,
        Principal::AppProcess,
        request(),
        TrustedFlowClass::KnownRequiresDeviceCompliance,
        admit,
    );
    let HandleState::Ready(response) = denied else {
        panic!("known class must be denied before effects");
    };
    let error = response
        .into_response()
        .expect_err("conditional unit predicate");
    assert_eq!(error.code, "KELD-AUTH-006");
    assert!(error.message.contains("Intune Company Portal"));
    assert!(error.message.contains("Enterprise SSO"));
    assert!(error.message.contains("no device-compliance claim"));
    assert!(
        error
            .message
            .contains("broker-capable managed-device application")
    );
    assert_eq!(admissions.get(), 0);
    assert_eq!(owner.next_operation, 0);
    assert!(owner.jobs.is_empty());
    // Public v0 input cannot mint the trusted class: same actual granted request
    // remains Unclassified and hits its original configuration producer.
    let ordinary = owner.handle_manifest_request(&allowed, Principal::AppProcess, request(), admit);
    let HandleState::Ready(response) = ordinary else {
        panic!("missing client is resource-free");
    };
    assert_eq!(
        response
            .into_response()
            .expect_err("unclassified public route")
            .code,
        "KELD-AUTH-015"
    );
    assert_eq!(admissions.get(), 0);
}

#[test]
fn as_pending_stop_fences_do_not_generate_their_own_wake_loop() {
    let operation = broker().next_id().expect("existing opaque identity owner");
    super::super::macos_session::assert_stop_wake_contract(operation);
}

#[test]
fn canceled_login_completes_once_without_restart_and_allows_explicit_begin() {
    use std::cell::Cell;
    // A forbidden automatic T3 fallback returns its existing config error before
    // any socket/browser effect, so the negative control remains resource-free.
    let (mut owner, port, manifest, account) = as_begin_fixture("invalid-fallback-trap");
    let request = || AuthRequest::Begin {
        provider: "entra-common".to_owned(),
        scopes: None,
        login_hint: None,
    };
    let admissions = Cell::new(0_u32);
    let admit = || {
        admissions.set(admissions.get() + 1);
        Some(())
    };
    let begin = |owner: &mut AuthBroker| {
        owner.handle_manifest_request(&manifest, Principal::AppProcess, request(), admit)
    };
    let operation = match begin(&mut owner) {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("actual explicit Begin must be admitted: {other:?}"),
    };
    let mut events = Vec::new();
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Pending
    );
    assert!(
        events.is_empty(),
        "submitted operation has no premature result"
    );
    super::super::macos_session::test_deliver_canceled_login(&port, &operation);
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Drained
    );
    assert_eq!(events.len(), 1, "one actual canceled-login completion");
    assert_eq!(events[0].operation, operation);
    let error = events
        .pop()
        .expect("actual canceled-login event")
        .response
        .into_response()
        .expect_err("canceled-login is a typed non-success reply");
    assert_eq!(error.code, "KELD-AUTH-007");
    assert!(error.message.starts_with("KELD-AUTH-007:"));
    assert!(error.message.contains("fresh user-initiated auth.begin"));
    for _ in 0..8 {
        assert_eq!(
            owner.poll(&operation, |event| events.push(event)),
            Progress::Drained
        );
    }
    assert!(
        events.is_empty(),
        "retired operation cannot emit again or restart"
    );
    assert!(owner.jobs.is_empty(), "no spontaneous native job");
    assert_eq!(owner.next_operation, 1, "no spontaneous operation identity");
    assert_eq!(admissions.get(), 1, "no automatic readmission");
    assert!(
        owner.registry.admission_open(),
        "007 leaves the broker viable"
    );
    assert_eq!(
        owner.registry.issuer_for(&account),
        Some("https://login.microsoftonline.com/common")
    );
    let retry = match begin(&mut owner) {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("a new explicit Begin must remain viable: {other:?}"),
    };
    assert_ne!(retry, operation);
    assert_eq!(admissions.get(), 2);
    assert_eq!(owner.next_operation, 2);
    assert_eq!(owner.jobs.len(), 1);
    assert!(owner.cancel(&retry, CallError::generation_retired()));
    assert_eq!(
        owner.poll(&retry, |event| events.push(event)),
        Progress::Drained
    );
    assert_eq!(
        events.len(),
        1,
        "explicit retry has its own single cleanup result"
    );
    assert!(owner.jobs.is_empty());
}

fn as_begin_fixture(
    fixed_redirect: &str,
) -> (AuthBroker, AsWorkerPort, PermissionsManifest, String) {
    let mut owner = broker();
    let account = seed_account(&mut owner);
    let port = super::super::macos_session::test_as_cancellation_port();
    owner.as_port = Some(port.clone());
    let configuration = owner
        .configuration
        .get_mut("entra-common")
        .expect("profile config");
    configuration.as_redirect = Some("msalliteral-client://auth".to_owned());
    configuration.fixed_redirect = Some(fixed_redirect.to_owned());
    let manifest = parse_manifest(
        r#"{"app":{"auth":{"begin":["https://login.microsoftonline.com/common"]}}}"#,
    )
    .expect("actual parsed begin grant");
    (owner, port, manifest, account)
}

fn anchor_domain_fixture() -> (AuthBroker, AsWorkerPort, PermissionsManifest, String) {
    let fixture = as_begin_fixture("http://localhost/callback");
    // This existing syntactically valid missing-port fixture is exactly003.
    let profile = provider_profile("entra-common").expect("closed profile");
    let error = RedirectPlan::new(profile, Some("http://localhost/callback"))
        .err()
        .expect("fixed mode requires a nonzero explicit port");
    assert!(matches!(error, PrimitiveError::RedirectMode));
    assert_eq!(error.code(), "KELD-AUTH-003");
    fixture
}

#[test]
fn local_anchor_close_before_submit_preserves_auth_and_resolves_prestart_fallback() {
    use super::super::macos_session::{AsMainStop, test_apply_main_stop};
    use std::cell::Cell;
    let (mut owner, port, manifest, account) = anchor_domain_fixture();
    let admissions = Cell::new(0_u64);
    let mut events = Vec::new();
    for expected in 1..=2 {
        let request = AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: None,
        };
        let operation =
            match owner.handle_manifest_request(&manifest, Principal::AppProcess, request, || {
                admissions.set(admissions.get() + 1);
                Some(())
            }) {
                HandleState::Pending {
                    operation,
                    admission: Some(()),
                } => operation,
                other => panic!("actual granted Begin must remain viable: {other:?}"),
            };
        assert!(test_apply_main_stop(&port, AsMainStop::AnchorClosing));
        assert_eq!(
            owner.poll(&operation, |event| events.push(event)),
            Progress::Pending
        );
        assert!(events.is_empty());
        assert!(test_apply_main_stop(&port, AsMainStop::AnchorClosing));
        assert_eq!(
            owner.poll(&operation, |event| events.push(event)),
            Progress::Drained
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].operation, operation);
        let error = events
            .pop()
            .expect("actual prestart result")
            .response
            .into_response()
            .expect_err("existing invalid T3 redirect proves fallback without I/O");
        assert_eq!(error.code, "KELD-AUTH-003");
        assert!(owner.registry.admission_open());
        assert_eq!(
            owner.registry.issuer_for(&account),
            Some("https://login.microsoftonline.com/common")
        );
        assert!(owner.jobs.is_empty());
        assert_eq!(owner.next_operation, expected);
        assert_eq!(admissions.get(), expected);
    }
}

#[test]
fn local_anchor_close_stops_active_owner_without_fallback_or_revoking_accounts() {
    let (mut owner, port, manifest, account) = anchor_domain_fixture();
    let operation = match owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: None,
        },
        || Some(()),
    ) {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("actual granted Begin: {other:?}"),
    };
    let mut events = Vec::new();
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Pending
    );
    super::super::macos_session::test_active_anchor_retirement(&port, &operation);
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Drained
    );
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].operation, operation);
    let error = events
        .pop()
        .expect("actual stopped owner result")
        .response
        .into_response()
        .expect_err("local presentation stop is not successful login");
    assert_eq!(
        error.code, "KELD-AUTH-016",
        "accepted work must not fallback to trap003"
    );
    assert!(owner.registry.admission_open());
    assert_eq!(
        owner.registry.issuer_for(&account),
        Some("https://login.microsoftonline.com/common")
    );
    assert!(owner.jobs.is_empty());
    assert_eq!(owner.next_operation, 1);
}

#[test]
fn accepted_as_context_callback_error_does_not_replay_loopback() {
    let (mut owner, port, manifest, account) = anchor_domain_fixture();
    let operation = match owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: None,
        },
        || Some(()),
    ) {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("actual granted Begin: {other:?}"),
    };
    assert_eq!(owner.poll(&operation, |_| {}), Progress::Pending);
    let mut events = Vec::new();
    super::super::macos_session::test_deliver_callback_error(&port, &operation, 2);
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Drained
    );
    assert_eq!(events.len(), 1);
    let error = events
        .pop()
        .expect("one terminal callback error")
        .response
        .into_response()
        .expect_err("presentation completion error cannot be a successful login");
    assert_eq!(
        error.code, "KELD-AUTH-017",
        "a callback after session start must not enter the trapped loopback fallback"
    );
    assert!(owner.registry.admission_open());
    assert_eq!(
        owner.registry.issuer_for(&account),
        Some("https://login.microsoftonline.com/common")
    );
    assert!(owner.jobs.is_empty());
    assert_eq!(owner.next_operation, 1);
}

#[test]
fn host_tail_closes_auth_and_native_admission_permanently() {
    use super::super::macos_session::{AsMainStop, test_apply_main_stop};
    let (mut owner, port, manifest, _) = anchor_domain_fixture();
    let request = || AuthRequest::Begin {
        provider: "entra-common".to_owned(),
        scopes: None,
        login_hint: None,
    };
    let operation = match owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        request(),
        || Some(()),
    ) {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("actual granted Begin: {other:?}"),
    };
    let mut events = Vec::new();
    owner.stop_for_host_tail(|event| events.push(event));
    assert!(!test_apply_main_stop(&port, AsMainStop::HostTail));
    assert!(!test_apply_main_stop(&port, AsMainStop::AnchorClosing));
    assert!(!test_apply_main_stop(&port, AsMainStop::Running));
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Drained
    );
    assert_eq!(events.len(), 1, "same global terminal emitted once");
    assert!(!owner.registry.admission_open());
    let stopped = permission_ready_error(owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        request(),
        || -> Option<()> {
            panic!("global tail must not readmit");
        },
    ));
    assert_eq!(stopped.code, "KELD-AUTH-016");
    assert!(owner.jobs.is_empty());
    assert_eq!(owner.next_operation, 1);
}

#[test]
fn host_tail_native_stop_before_submit_finishes_begin_stopping_result() {
    use super::super::macos_session::{AsMainStop, test_apply_main_stop};
    let (mut owner, port, manifest, _) = anchor_domain_fixture();
    let operation = match owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: None,
        },
        || Some(()),
    ) {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("actual granted Begin: {other:?}"),
    };
    assert!(!test_apply_main_stop(&port, AsMainStop::HostTail));
    let mut events = Vec::new();
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Drained,
        "a submit rejected by the already-closed host port is a normal stop result"
    );
    assert_eq!(events.len(), 1);
    let error = events
        .pop()
        .expect("one terminal Begin result")
        .response
        .into_response()
        .expect_err("host tail cannot complete sign-in");
    assert_eq!(error.code, "KELD-AUTH-016");
    assert!(owner.jobs.is_empty());
}

#[test]
fn contended_anchor_stop_registers_before_claim_and_survives_running_drive() {
    let (mut owner, port, manifest, account) = anchor_domain_fixture();
    let operation = match owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: None,
        },
        || Some(()),
    ) {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("actual granted Begin: {other:?}"),
    };
    let mut events = Vec::new();
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Pending
    );
    super::super::macos_session::test_contended_anchor_stop(&port, &operation);
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Drained
    );
    assert_eq!(events.len(), 1);
    let error = events
        .pop()
        .expect("actual prestart reply")
        .response
        .into_response()
        .expect_err("fixed port absence stops fallback before I/O");
    assert_eq!(error.code, "KELD-AUTH-003");
    assert!(owner.registry.admission_open());
    assert_eq!(
        owner.registry.issuer_for(&account),
        Some("https://login.microsoftonline.com/common")
    );
    assert!(owner.jobs.is_empty());
}

#[test]
fn global_tail_overrides_unclaimed_local_unavailable() {
    use super::super::macos_session::{AsMainStop, test_apply_main_stop};
    let (mut owner, port, manifest, _) = anchor_domain_fixture();
    let operation = match owner.handle_manifest_request(
        &manifest,
        Principal::AppProcess,
        AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: None,
        },
        || Some(()),
    ) {
        HandleState::Pending {
            operation,
            admission: Some(()),
        } => operation,
        other => panic!("actual granted Begin: {other:?}"),
    };
    let mut events = Vec::new();
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Pending
    );
    assert!(test_apply_main_stop(&port, AsMainStop::AnchorClosing));
    owner.stop_for_host_tail(|event| events.push(event));
    assert!(!test_apply_main_stop(&port, AsMainStop::HostTail));
    super::super::macos_session::test_assert_unclaimed_host_priority(&port, &operation);
    assert_eq!(
        owner.poll(&operation, |event| events.push(event)),
        Progress::Drained
    );
    assert_eq!(events.len(), 1);
    let error = events
        .pop()
        .expect("actual global tail reply")
        .response
        .into_response()
        .expect_err("canonical host tail wins");
    assert_eq!(error.code, "KELD-AUTH-016");
    assert!(!owner.registry.admission_open());
    assert!(owner.jobs.is_empty());
}
