//! Refuse untrusted HTTP route data without echoing it into test diagnostics.
//! The first test uses a real owned TCP connection, not a mock stream.
use super::RecoveryObserver;
use crate::public_fs::private_diagnostics::route_summary;
use std::net::{TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;

#[test]
fn native_unexpected_recovery_route_does_not_echo_private_request_path() {
    const PRIVATE: &str = "KELD_SYNTHETIC_PRIVATE_ROUTE_PATH_TOKEN_SENTINEL";
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("owned loopback listener");
    let client = TcpStream::connect(listener.local_addr().expect("local bound address"))
        .expect("real native recovery fixture client");
    let (accepted, _) = listener.accept().expect("accept native HTTP peer");

    let (_commands, inbox) = mpsc::channel();
    let mut observer = RecoveryObserver::new(inbox);
    let route = format!("/unexpected/{PRIVATE}?token={PRIVATE}");
    let rejected = catch_unwind(AssertUnwindSafe(|| observer.accept_wait(&route, accepted)));
    let payload = rejected.expect_err("unexpected route must still fail closed");
    let fixed = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&'static str>().copied());
    // The #1009 diagnostics owner: a fixed marker plus the route's fixed kind
    // and byte count, never the route bytes themselves.
    let expected = format!(
        "KELD_KEL140_RECOVERY_UNEXPECTED_ROUTE {}",
        route_summary(&route)
    );
    assert!(
        fixed.is_some_and(|diagnostic| diagnostic == expected && !diagnostic.contains(PRIVATE)),
        "untrusted route must not be interpolated into diagnostic panic"
    );
    drop(client);
}

#[test]
fn public_observer_fallback_has_same_fixed_route_panic() {
    // Source-binding the other dispatch branch prevents a future raw route
    // interpolation from bypassing the native recovery-path regression. The
    // negative half (no raw `path` capture anywhere in the fixture) is owned
    // by the #1009 source guard in `private_diagnostics/source_guard.rs`.
    let source = include_str!("../server.rs");
    assert!(
        source.contains(
            "None => panic!(\n                    \"KELD_KEL140_OBSERVER_UNEXPECTED_ROUTE {}\",\n                    route_summary(path)\n                ),"
        ),
        "observer's non-recovery dispatch must use the fixed route diagnostic owner"
    );
}
