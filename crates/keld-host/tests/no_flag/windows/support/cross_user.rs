//! Exact current-user SID, request, coordinator and remote observation helpers.

use super::control::read_control_line;
use super::profile_run::{SignedProfileStateCase, record_profile_state_case};
use super::profile_server::ProfileStateServer;
use crate::{CONTROL_LINE_LIMIT, PRODUCT_DEADLINE};
use serde_json::Value;
use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::Instant;

pub(crate) fn profile_test_user_sid() -> String {
    let identifiers = profile_test_token_sids("/user");
    assert_eq!(identifiers.len(), 1, "whoami must report one user SID");
    identifiers
        .into_iter()
        .next()
        .expect("one Windows user SID")
}

pub(crate) fn profile_test_token_sids(kind: &str) -> Vec<String> {
    let output = Command::new("whoami.exe")
        .args([kind, "/fo", "csv", "/nh"])
        .output()
        .expect("observe the fixture process's actual Windows user SID");
    assert!(output.status.success(), "whoami user observation failed");
    // The SID is ASCII even when the account-name column uses the local code page.
    let text = String::from_utf8_lossy(&output.stdout);
    text.split(|character: char| character == ',' || character == '"' || character.is_whitespace())
        .filter(|field| field.starts_with("S-1-"))
        .map(str::to_owned)
        .collect()
}

pub(crate) fn profile_request_address(request: &Value, field: &str) -> SocketAddr {
    let address: SocketAddr = request[field]
        .as_str()
        .expect("loopback address field")
        .parse()
        .expect("socket address");
    assert!(
        address.ip().is_loopback() && address.port() != 0,
        "fixture only contacts a live loopback endpoint"
    );
    address
}

pub(crate) fn validate_profile_second_user(
    greeting: &Value,
    nonce: &str,
    namespace: &str,
    first_sid: &str,
    address: SocketAddr,
) -> Result<String, String> {
    if greeting["kind"] != "hello"
        || greeting["nonce"] != nonce
        || greeting["namespace"] != namespace
        || greeting["server"] != address.to_string()
    {
        return Err("second-user context does not match the live request".to_owned());
    }
    let sid = greeting["user_sid"]
        .as_str()
        .ok_or("second-user SID is missing")?;
    if sid == first_sid || !sid.starts_with("S-1-") || greeting["administrator"] != false {
        return Err("second-user context is not a distinct ordinary user".to_owned());
    }
    Ok(sid.to_owned())
}

pub(crate) fn run_remote_profile_state_case(
    peer: &mut BufReader<TcpStream>,
    server: &ProfileStateServer,
    nonce: &str,
    case: &SignedProfileStateCase<'_>,
) {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    server.expect_case(case.name, deadline);
    write_profile_coordinator(
        peer.get_mut(),
        &serde_json::json!({
            "kind": "run", "case": case.name, "before": case.before, "after": case.after,
        }),
    );
    let ready = read_profile_coordinator(peer);
    assert_eq!(ready["kind"], "ready");
    assert_eq!(ready["case"], case.name);
    let pid = |field: &str| {
        u32::try_from(ready[field].as_u64().expect("observed native PID")).expect("PID width")
    };
    let observation = server.wait_for_case(case.name, deadline);
    record_profile_state_case(
        &observation,
        case,
        nonce,
        server.address(),
        pid("host_pid"),
        pid("bun_pid"),
    );
    write_profile_coordinator(peer.get_mut(), &serde_json::json!({"kind": "finish"}));
    let finished = read_profile_coordinator(peer);
    assert_eq!(finished["kind"], "finished");
    assert_eq!(finished["case"], case.name);
}

pub(crate) fn read_profile_coordinator(reader: &mut BufReader<TcpStream>) -> Value {
    serde_json::from_str(&read_control_line(reader)).expect("bounded coordinator JSON")
}

pub(crate) fn write_profile_coordinator(stream: &mut TcpStream, message: &Value) {
    let mut bytes = serde_json::to_vec(message).expect("coordinator JSON");
    bytes.push(b'\n');
    assert!(bytes.len() <= CONTROL_LINE_LIMIT);
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    let mut pending = bytes.as_slice();
    while !pending.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("coordinator write deadline");
        stream
            .set_write_timeout(Some(remaining))
            .expect("coordinator write timeout");
        let written = stream.write(pending).expect("write coordinator message");
        assert_ne!(written, 0, "coordinator stopped accepting bytes");
        pending = &pending[written..];
    }
}
