//! Signed second-user storage isolation and active context-binding contract.

use crate::profile_state_run_nonce;
use crate::support::control::accept_control_until;
use crate::support::cross_user::{
    profile_test_user_sid, read_profile_coordinator, run_remote_profile_state_case,
    validate_profile_second_user, write_profile_coordinator,
};
use crate::support::product::ProductFixture;
use crate::support::profile_run::{SignedProfileStateCase, run_signed_profile_state_case};
use crate::support::profile_server::ProfileStateServer;
use crate::support::signed_identity::signed_fixture_profile_namespace;
use serde_json::Value;
use std::env;
use std::io::BufReader;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
#[ignore = "blocked on KEL-19 / KEL-254 T3 Part B (Windows persistent profiles need installed-root boot); requires an operator-authenticated second ordinary user and shared signed fixtures"]
fn kel135_signed_host_cross_user_storage_isolation() {
    let host = env::var_os("KELD_KEL135_SIGNED_HOST_A_P1").expect("signed A/P1 host");
    let identity =
        env::var_os("KELD_KEL135_SIGNED_IDENTITY_A_P1").expect("signed A/P1 identity fixture");
    let shared = env::var_os("KELD_KEL135_SHARED_DIRECTORY")
        .expect("owned directory readable by the second user");
    let namespace = signed_fixture_profile_namespace(&identity, Some(&host));
    let first_sid = profile_test_user_sid();
    let fixture = ProductFixture::new();
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("first-user control");
    let server = ProfileStateServer::new();
    let nonce = profile_state_run_nonce(&fixture);
    let first_value = format!("{nonce}-u1");
    let second_value = format!("{nonce}-u2");
    let seed = SignedProfileStateCase {
        name: "u1-seed",
        host: &host,
        before: "",
        after: &first_value,
    };
    run_signed_profile_state_case(&fixture, &control, &server, &nonce, &seed);
    let coordinator = TcpListener::bind(("127.0.0.1", 0)).expect("cross-user coordinator");
    let mut request = tempfile::Builder::new()
        .prefix("kel135-user2-")
        .suffix(".json")
        .tempfile_in(shared)
        .expect("unique cross-user request file");
    serde_json::to_writer(
        request.as_file_mut(),
        &serde_json::json!({
            "coordinator": coordinator.local_addr().expect("coordinator address").to_string(),
            "server": server.address().to_string(), "nonce": nonce,
            "host": Path::new(&host), "identity": Path::new(&identity),
            "controller": env::current_exe().expect("current acceptance controller"),
        }),
    )
    .expect("write cross-user request");
    request
        .as_file_mut()
        .flush()
        .expect("publish complete request");
    println!(
        "KELD_KEL135_SECOND_USER_REQUEST {}",
        request.path().display()
    );
    std::io::stdout()
        .flush()
        .expect("publish operator action before waiting");
    let stream = accept_control_until(&coordinator, None, Instant::now() + Duration::from_mins(10));
    let mut peer = BufReader::new(stream);
    let greeting = read_profile_coordinator(&mut peer);
    let second_sid =
        validate_profile_second_user(&greeting, &nonce, &namespace, &first_sid, server.address())
            .expect("second user must be distinct, ordinary and bound to the same app/origin");
    println!(
        "KELD_KEL135_CROSS_USER {}",
        serde_json::json!({
            "first_sid": first_sid, "second_sid": second_sid,
            "namespace": namespace, "origin": format!("http://{}", server.address()),
        })
    );
    for (name, before, after) in [
        ("u2-isolated", "", second_value.as_str()),
        ("u2-restart", second_value.as_str(), second_value.as_str()),
        ("u2-cleanup", second_value.as_str(), ""),
    ] {
        let case = SignedProfileStateCase {
            name,
            host: &host,
            before,
            after,
        };
        run_remote_profile_state_case(&mut peer, &server, &nonce, &case);
    }
    write_profile_coordinator(peer.get_mut(), &serde_json::json!({"kind": "stop"}));
    assert_eq!(read_profile_coordinator(&mut peer)["kind"], "stopped");
    for (name, before, after) in [
        ("u1-after-u2", first_value.as_str(), first_value.as_str()),
        ("u1-cleanup", first_value.as_str(), ""),
    ] {
        let case = SignedProfileStateCase {
            name,
            host: &host,
            before,
            after,
        };
        run_signed_profile_state_case(&fixture, &control, &server, &nonce, &case);
    }
}

#[test]
fn profile_second_user_requires_distinct_ordinary_context_and_the_same_origin() {
    let address: SocketAddr = "127.0.0.1:12345".parse().expect("fixture address");
    let good = serde_json::json!({
        "kind": "hello", "nonce": "run1", "namespace": "namespace-a",
        "user_sid": "S-1-5-21-2000", "administrator": false, "server": "127.0.0.1:12345",
    });
    assert_eq!(
        validate_profile_second_user(&good, "run1", "namespace-a", "S-1-5-21-1000", address)
            .expect("different ordinary user at the same origin"),
        "S-1-5-21-2000"
    );
    for (field, value) in [
        ("user_sid", Value::String("S-1-5-21-1000".to_owned())),
        ("administrator", Value::Bool(true)),
        ("administrator", Value::Null),
        ("namespace", Value::String("namespace-b".to_owned())),
        ("nonce", Value::String("old-run".to_owned())),
        ("server", Value::String("127.0.0.1:12346".to_owned())),
    ] {
        let mut wrong = good.clone();
        wrong[field] = value;
        assert!(
            validate_profile_second_user(&wrong, "run1", "namespace-a", "S-1-5-21-1000", address)
                .is_err(),
            "{field} mismatch cannot count as cross-user acceptance"
        );
    }
}
