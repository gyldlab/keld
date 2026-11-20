use super::{
    Election, OriginalFacts, SelectedLaunch, native, prepare_if_running, prepare_selected,
};
use crate::app_session::graph_contract::tests::prepared_original_launch_source;
use crate::app_session::publish_windows_lease_observation;
use crate::app_session::windows_instance_election::tests::{elect_at_for_test, fixture};
use crate::app_session::{GRAPH_FILE, SessionShutdownState, WindowsDevLeaseObservation};
use sha2::{Digest as _, Sha256};
use std::{fs, sync::mpsc};

fn observation() -> native::Observation {
    native::Observation {
        app_data: Some(r"D:\Observed\Roaming".to_owned()),
        documents: Some(r"\\server\share\Documents".to_owned()),
        locale: Some(String::from("fr-CA")),
    }
}

#[test]
fn selected_record_serializes_exact_same_graph_policy_and_optional_facts() {
    let (_files, source, root, entry) = prepared_original_launch_source();
    let selected = SelectedLaunch {
        root: &root,
        entry,
        name: "Exact λ",
        version: Some("31.7.0-selected"),
        declaration: None,
        source: &source,
    };
    let mut facts = OriginalFacts::new(&selected).expect("prepared binding");
    facts.observe(observation()).expect("bounded observations");
    let input = facts.bun_argument().expect("parse-time argument");
    let encoded = input
        .strip_prefix("--define=__KELD_HOST_LAUNCH_FACTS__:")
        .expect("exact carrier");
    let value: serde_json::Value = serde_json::from_str(encoded).expect("closed record");
    let graph_bytes = fs::read(root.join(GRAPH_FILE)).expect("exact staged graph");
    let manifest_bytes =
        br#"{"profile":"legacy","windows":{"editor":{"web":{"csp":"original-editor-offline"}}}}"#;
    assert_eq!(
        value,
        serde_json::json!({
            "schema": 8,
            "kind": "development",
            "name": "Exact λ",
            "root": root.to_str().expect("verbatim root"),
            "version": "31.7.0-selected",
            "locale": "fr-CA",
            "windowsApplicationMenu": "absent-v1",
            "paths": {
                "appData": r"D:\Observed\Roaming",
                "userData": r"D:\Observed\Roaming\Exact λ",
                "documents": r"\\server\share\Documents",
            },
            "originalResourcePolicy": {
                "contract": "gh473-original-editor-offline/v1",
                "graphSha256": format!("sha256:{:x}", Sha256::digest(graph_bytes)),
                "policySha256": format!("sha256:{:x}", Sha256::digest(manifest_bytes)),
                "windowKey": "editor",
                "selector": "original-editor-offline",
            }
        }),
    );
    assert_eq!(input, facts.bun_argument().expect("retained same value"));
}

#[test]
fn another_root_or_invalid_record_refuses_before_native_capture_and_election() {
    let (_files, source, root, entry) = prepared_original_launch_source();
    let foreign = root.join("foreign");
    let invalid_name = "\u{1}".repeat(4096);
    for (selected_root, name, code) in [
        (foreign.as_path(), "valid", "KELD-CORE-036"),
        (root.as_path(), invalid_name.as_str(), "KELD-CORE-037"),
    ] {
        let selected = SelectedLaunch {
            root: selected_root,
            entry,
            name,
            version: None,
            declaration: None,
            source: &source,
        };
        let result = prepare_selected(
            selected,
            || panic!("foreign or invalid selection reached native capture"),
            |_| panic!("election must stay out of refused startup"),
        );
        assert_eq!(result.err().expect("typed refusal").code(), code);
    }
}

#[test]
fn real_election_drives_the_record_and_retains_only_the_winner() {
    let (_files, source, root, entry) = prepared_original_launch_source();
    let (namespace, declaration) = fixture();
    let selected = SelectedLaunch {
        root: &root,
        entry,
        name: "Exact",
        version: None,
        declaration: Some(&declaration),
        source: &source,
    };
    let mut winner = OriginalFacts::new(&selected).expect("prepared source");
    let actual = elect_at_for_test(namespace.path(), &declaration).expect("real kernel call");
    let lease = winner.set_election(Some(actual)).expect("winner lease");
    let result: serde_json::Value =
        serde_json::from_str(&winner.encode().expect("record")).expect("exact verified JSON");
    assert_eq!(
        result["singleInstance"],
        serde_json::json!({"verdict":"acquired"})
    );
    let mut contender = OriginalFacts::new(&selected).expect("same prepared source");
    let actual = elect_at_for_test(namespace.path(), &declaration).expect("real contender");
    assert!(contender.set_election(Some(actual)).is_none());
    let result: serde_json::Value =
        serde_json::from_str(&contender.encode().expect("record")).expect("exact verified JSON");
    assert_eq!(
        result["singleInstance"],
        serde_json::json!({"verdict":"contended"})
    );
    drop(lease);
    assert!(matches!(
        elect_at_for_test(namespace.path(), &declaration).expect("released kernel lock"),
        Election::Acquired(_)
    ));
}

#[test]
fn eof_beats_provisional_command_and_releases_its_real_winner() {
    let (_files, source, root, entry) = prepared_original_launch_source();
    let (namespace, declaration) = fixture();
    let shutdown = SessionShutdownState::new();
    let (send, _receive) = mpsc::channel();
    let result = prepare_if_running(&shutdown, || {
        let prepared = prepare_selected(
            SelectedLaunch {
                root: &root,
                entry,
                name: "Exact",
                version: None,
                declaration: Some(&declaration),
                source: &source,
            },
            || {
                let lock = shutdown
                    .transition
                    .try_lock()
                    .expect("observation holds no cut");
                drop(lock);
                Ok(observation())
            },
            |declared| elect_at_for_test(namespace.path(), declared),
        )?;
        assert!(prepared.winner_lease.is_some());
        assert!(matches!(
            elect_at_for_test(namespace.path(), &declaration)?,
            Election::Contended
        ));
        publish_windows_lease_observation(&shutdown, &send, WindowsDevLeaseObservation::Eof);
        Ok(prepared)
    })
    .expect("accepted EOF");
    assert!(result.is_none(), "EOF forbids publication");
    assert!(matches!(
        elect_at_for_test(namespace.path(), &declaration).expect("provisional lock released"),
        Election::Acquired(_)
    ));
    let stopped = prepare_if_running::<()>(&shutdown, || {
        panic!("already stopped launch must make no queries")
    })
    .expect("unchanged stop");
    assert!(stopped.is_none());
}

#[test]
fn present_native_failure_cannot_replace_accepted_terminal_cause() {
    let shutdown = SessionShutdownState::new();
    let (send, _receive) = mpsc::channel();
    assert!(
        prepare_if_running::<()>(&shutdown, || {
            publish_windows_lease_observation(&shutdown, &send, WindowsDevLeaseObservation::Eof);
            Err(super::invalid("native operation failed after EOF"))
        })
        .expect("EOF wins")
        .is_none()
    );
}

#[test]
fn app_session_binds_only_the_boot_selected_prepared_graph() {
    use crate::app_session::WindowsGraphSelection;
    use std::sync::Arc;

    let (_files, source, root, entry) = prepared_original_launch_source();
    let (_other_files, other, _, _) = prepared_original_launch_source();
    assert_eq!(
        source.launch_graph().graph_sha256(),
        other.launch_graph().graph_sha256(),
        "the separately admitted graph has the same expected bytes"
    );
    let declaration = serde_json::from_str(r#"{"id":"con.example"}"#).expect("boot declaration");
    let validated = WindowsGraphSelection {
        graph: Arc::clone(source.launch_graph()),
        version: Some(String::from("31.7.0-selected")),
        single_instance: Some(declaration),
        application_name_for_user_agent: None,
    };
    let selected = validated
        .launch_selection(&source, &root, entry, "Exact")
        .expect("same admitted graph");
    assert_eq!(selected.version, Some("31.7.0-selected"));
    assert_eq!(
        selected.declaration.expect("actual boot declaration").id(),
        "con.example"
    );
    assert!(std::ptr::eq(selected.source, &source));
    assert!(
        validated
            .launch_selection(&other, &root, entry, "Exact")
            .is_err(),
        "an independently prepared graph cannot supply the selection"
    );
}
