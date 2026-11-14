use std::cell::Cell;
use std::path::Path;
use std::sync::mpsc;

use super::{
    DevelopmentLaunchFacts, capture_development_launch_facts, capture_with_observer, native,
};
use crate::app_session::{
    SessionShutdownState, ValidatedAppIdentity, WindowsDevLeaseMonitor, WindowsDevLeaseObservation,
    WindowsProfileMode, publish_windows_lease_observation, select_windows_profile_mode,
};

fn prepared(name: &str, version: Option<&str>) -> DevelopmentLaunchFacts {
    DevelopmentLaunchFacts::from_validated_development(name, Path::new(r"\\?\D:\stage-λ"), version)
        .expect("valid explicitly selected metadata")
        .expect("local drive")
}

fn observation() -> native::Observation {
    native::Observation {
        app_data: Some(r"D:\Observed\Roaming".to_owned()),
        documents: Some(r"\\server\share\Documents".to_owned()),
        locale: Some("fr-CA".to_owned()),
    }
}

#[test]
fn schema_three_keeps_exact_explicit_metadata_and_captured_native_values() {
    let facts = prepared("Exact λ", Some("31.7.0-selected"))
        .with_observation(observation())
        .expect("closed captured record");
    let encoded = facts.private_argument().expect("JSON");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).expect("JSON"),
        serde_json::json!({
            "schema":3,"kind":"development","name":"Exact λ","root":r"\\?\D:\stage-λ",
            "version":"31.7.0-selected","locale":"fr-CA",
            "paths":{"appData":r"D:\Observed\Roaming","userData":r"D:\Observed\Roaming\Exact λ","documents":r"\\server\share\Documents"}
        }),
    );
    assert_eq!(
        facts.bun_define_argument().expect("initial"),
        facts.bun_define_argument().expect("recovered same record"),
    );
    let absent = prepared("NUL.txt", None)
        .with_observation(native::Observation {
            app_data: observation().app_data,
            ..native::Observation::default()
        })
        .expect("unavailable optional facts");
    let value =
        serde_json::from_str::<serde_json::Value>(&absent.private_argument().expect("JSON"))
            .expect("JSON");
    assert!(value.get("version").is_none());
    assert!(value.get("locale").is_none());
    assert_eq!(
        value["paths"],
        serde_json::json!({"appData":r"D:\Observed\Roaming"})
    );
}

#[test]
fn unsupported_stage_roots_are_absent_without_becoming_known_folder_roots() {
    for root in [
        r"D:\stage",
        r"\\server\share\stage",
        r"\\?\UNC\server\share\stage",
        r"\\.\D:\stage",
        r"\\?\D:stage",
        r"\\?\D:\a\..\b",
        r"\\?\D:\a/b",
        r"\\?\D:\a\\b",
        r"\\?\D:\a\",
        "/stage",
        "stage",
    ] {
        assert!(
            DevelopmentLaunchFacts::from_validated_development("app", Path::new(root), None)
                .expect("unsupported root")
                .is_none(),
            "{root}",
        );
    }
    for root in [r"\\?\D:\", r"\\?\d:\a b\λ"] {
        assert!(
            DevelopmentLaunchFacts::from_validated_development("app", Path::new(root), None)
                .expect("supported root")
                .is_some()
        );
    }
}

#[test]
fn selected_fields_and_escaped_whole_record_keep_typed_bounds() {
    let root = Path::new(r"\\?\D:\stage");
    for value in [
        String::new(),
        "nul\0value".to_owned(),
        "x".repeat(4097),
        "λ".repeat(2049),
    ] {
        for (name, version) in [("app", Some(value.as_str())), (value.as_str(), None)] {
            assert_eq!(
                DevelopmentLaunchFacts::from_validated_development(name, root, version)
                    .expect_err("invalid field")
                    .code(),
                "KELD-CORE-037"
            );
        }
    }
    assert!(
        DevelopmentLaunchFacts::from_validated_development("app", root, Some(&"λ".repeat(2048)))
            .expect("4096-byte version")
            .is_some()
    );
    assert_eq!(
        DevelopmentLaunchFacts::from_validated_development(&"\u{1}".repeat(4096), root, None)
            .expect_err("escaped record bound before native query")
            .code(),
        "KELD-CORE-037"
    );
    assert_eq!(
        DevelopmentLaunchFacts::from_validated_development(
            "app",
            Path::new("\\\\?\\D:\\stage\0suffix"),
            None
        )
        .expect_err("recognized root with NUL")
        .code(),
        "KELD-CORE-037"
    );
    let large = prepared(&"x".repeat(4096), Some(&"x".repeat(4096)));
    assert_eq!(
        large
            .with_observation(native::Observation {
                app_data: None,
                documents: Some(format!("D:\\{}", "x".repeat(4093))),
                locale: Some("x".repeat(4096)),
            })
            .expect_err("all captured fields share total record limit")
            .code(),
        "KELD-CORE-037"
    );
}

#[test]
fn native_observation_is_once_outside_the_transition_and_recovery_does_not_recapture() {
    let shutdown = SessionShutdownState::new();
    let calls = Cell::new(0);
    let facts = capture_with_observer(prepared("Exact", Some("explicit")), &shutdown, || {
        let lock = shutdown
            .transition
            .try_lock()
            .expect("native call must hold no transition");
        drop(lock);
        calls.set(calls.get() + 1);
        Ok(observation())
    })
    .expect("running capture")
    .expect("retained facts");
    let first = facts.bun_define_argument().expect("first");
    for _ in 0..3 {
        assert_eq!(
            first,
            facts.bun_define_argument().expect("same recovered record")
        );
    }
    assert_eq!(calls.get(), 1);
}

#[test]
fn eof_during_native_observation_refuses_publication_and_preserves_terminal_owner() {
    let shutdown = SessionShutdownState::new();
    let (sender, _receiver) = mpsc::channel();
    let result = capture_with_observer(prepared("Exact", None), &shutdown, || {
        publish_windows_lease_observation(&shutdown, &sender, WindowsDevLeaseObservation::Eof);
        Ok(observation())
    })
    .expect("late observation loses the exact startup cut");
    assert!(result.is_none());
    assert!(
        capture_with_observer(prepared("Exact", None), &shutdown, || {
            panic!("pre-existing EOF must prevent another native query")
        })
        .expect("already stopped")
        .is_none()
    );
}

#[test]
fn eof_during_failed_native_observation_preserves_the_existing_terminal_cause() {
    let shutdown = SessionShutdownState::new();
    let (sender, receiver) = mpsc::channel();
    let result = capture_with_observer(prepared("Exact", None), &shutdown, || {
        publish_windows_lease_observation(&shutdown, &sender, WindowsDevLeaseObservation::Eof);
        Err(super::invalid(
            "typed native observation refusal after lease EOF",
        ))
    })
    .expect("the accepted EOF must precede the obsolete observation error");
    assert!(result.is_none());
    assert!(matches!(
        receiver.try_recv(),
        Ok(WindowsDevLeaseObservation::Eof)
    ));
    assert_eq!(
        shutdown.cause.load(std::sync::atomic::Ordering::Acquire),
        crate::app_session::SESSION_CLI_LEASE_LOST,
    );
}

#[test]
fn capture_still_requires_the_same_profile_and_actual_lease_owner() {
    // These are component ownership controls, not native lease admission proof.
    let (sender, receiver) = mpsc::channel();
    let lease = WindowsDevLeaseMonitor {
        observations: receiver,
        #[cfg(debug_assertions)]
        raw_handle_value: 0,
    };
    let shutdown = SessionShutdownState::new();
    let root = Path::new(r"\\?\D:\stage");
    let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.facts")
        .expect("component identity");
    let installed = select_windows_profile_mode(Some(identity), false).expect("installed mode");
    for (facade, profile, monitor) in [
        (false, WindowsProfileMode::EphemeralDev, Some(&lease)),
        (true, WindowsProfileMode::EphemeralDev, None),
        (true, installed, Some(&lease)),
    ] {
        assert!(
            capture_development_launch_facts("", root, None, facade, profile, monitor, &shutdown)
                .expect("ineligible capture")
                .is_none()
        );
    }
    publish_windows_lease_observation(&shutdown, &sender, WindowsDevLeaseObservation::Eof);
    assert!(
        capture_development_launch_facts(
            "",
            root,
            Some(""),
            true,
            WindowsProfileMode::EphemeralDev,
            Some(&lease),
            &shutdown,
        )
        .expect("prior EOF prevents invalid capture")
        .is_none()
    );
}
