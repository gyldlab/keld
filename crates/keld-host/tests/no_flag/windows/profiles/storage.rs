//! Signed five-store profile isolation acceptance; requires source-bound fixtures.

use crate::support::product::ProductFixture;
use crate::support::profile_run::{SignedProfileStateCase, run_signed_profile_state_case};
use crate::support::profile_server::ProfileStateServer;
use crate::support::signed_identity::signed_fixture_profile_namespace;
use crate::{profile_state_run_nonce, profile_test_user_sid};
use std::env;
use std::net::TcpListener;

#[test]
#[ignore = "blocked on KEL-19 / KEL-254 T3 Part B (Windows persistent profiles need installed-root boot); requires signed KEL-135 host fixtures and WebView2 state acceptance"]
fn kel135_signed_host_profile_state_isolation() {
    let primary_carrier = env::var_os("KELD_KEL135_SIGNED_HOST_A_P1")
        .expect("KELD_KEL135_SIGNED_HOST_A_P1 must point to a signed host");
    let sibling_carrier = env::var_os("KELD_KEL135_SIGNED_HOST_B_P1")
        .expect("KELD_KEL135_SIGNED_HOST_B_P1 must point to a signed host");
    let alternate_publisher_carrier = env::var_os("KELD_KEL135_SIGNED_HOST_A_P2")
        .expect("KELD_KEL135_SIGNED_HOST_A_P2 must point to a signed host");
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind state control");
    let state_server = ProfileStateServer::new();
    let run_nonce = profile_state_run_nonce(&fixture);
    let primary_state = format!("{run_nonce}-a");
    let sibling_state = format!("{run_nonce}-b");
    let publisher_two_value = format!("{run_nonce}-p2");
    let identity_paths = [
        ("A/P1", "KELD_KEL135_SIGNED_IDENTITY_A_P1", &primary_carrier),
        ("B/P1", "KELD_KEL135_SIGNED_IDENTITY_B_P1", &sibling_carrier),
        (
            "A/P2",
            "KELD_KEL135_SIGNED_IDENTITY_A_P2",
            &alternate_publisher_carrier,
        ),
    ];
    let identities = identity_paths.map(|(label, variable, carrier)| {
        let path = env::var_os(variable).expect("matching signed identity fixture is required");
        let namespace = signed_fixture_profile_namespace(&path, Some(carrier));
        assert_eq!(namespace.len(), 64, "{label} profile namespace width");
        assert!(namespace.bytes().all(|byte| byte.is_ascii_hexdigit()));
        (label, namespace)
    });
    assert_ne!(
        identities[0].1, identities[1].1,
        "A and B must have distinct authenticated identities"
    );
    assert_ne!(
        identities[0].1, identities[2].1,
        "publisher scopes must produce distinct identities"
    );
    println!(
        "KELD_KEL135_IDENTITIES {}",
        serde_json::json!({
            "user_sid": profile_test_user_sid(),
            "identities": identities,
            "origin": format!("http://{}", state_server.address()),
        })
    );
    let cases: [(&str, &std::ffi::OsStr, &str, &str); 9] = [
        ("a-seed", &primary_carrier, "", &primary_state),
        (
            "a-restart",
            &primary_carrier,
            &primary_state,
            &primary_state,
        ),
        ("b-isolated", &sibling_carrier, "", &sibling_state),
        (
            "a-after-b",
            &primary_carrier,
            &primary_state,
            &primary_state,
        ),
        (
            "a-p2-isolated",
            &alternate_publisher_carrier,
            "",
            &publisher_two_value,
        ),
        ("a-final", &primary_carrier, &primary_state, &primary_state),
        ("a-cleanup", &primary_carrier, &primary_state, ""),
        ("b-cleanup", &sibling_carrier, &sibling_state, ""),
        (
            "p2-cleanup",
            &alternate_publisher_carrier,
            &publisher_two_value,
            "",
        ),
    ];
    for (name, host, before, after) in cases {
        let case = SignedProfileStateCase {
            name,
            host,
            before,
            after,
        };
        run_signed_profile_state_case(
            &fixture,
            &control_listener,
            &state_server,
            &run_nonce,
            &case,
        );
    }
}
