//! Existing macOS descriptor census binds one connected application socket pair.
use super::private_diagnostics::assert_private_eq;
use crate::support::unix_descriptors::{UnixDescriptor, unix_descriptors};
use std::collections::BTreeSet;

pub(super) fn assert_single_application_link(host: u32, bun: u32, endpoint: &str) {
    let host_rows = unix_descriptors(host);
    let bun_rows = unix_descriptors(bun);
    // Bound listeners and accepted FD clones can share a pathname. A Bun
    // connected-peer identity selects the actual host socket object instead.
    let pairs = application_socket_pairs(&host_rows, &bun_rows, endpoint);
    // Diagnostic counts are not socket authority. Never print the inspected
    // PID, private endpoint, descriptor ID or kernel socket address: only the
    // scalar counts the #1009 source guard admits reach test stderr.
    eprintln!(
        "KELD_KEL140_APPLICATION_SOCKET_CENSUS host_fd_count={} bun_fd_count={} connected_pairs={}",
        host_rows.len(),
        bun_rows.len(),
        pairs.len()
    );
    assert_eq!(pairs.len(), 1, "one connected application socket pair");
    let (server_socket, client_socket) = pairs.first().expect("one actual connected pair");
    let client_peer = format!("->{server_socket}");
    for row in bun_rows.iter().filter(|row| &row.socket == client_socket) {
        // Kernel socket addresses stay out of every failure text.
        assert_private_eq(
            &row.identity,
            &client_peer,
            "client FD clone changed its connected peer",
        );
    }
    for row in host_rows.iter().filter(|row| &row.socket == server_socket) {
        if let Some(peer) = row.identity.strip_prefix("->") {
            assert_private_eq(peer, client_socket.as_str(), "host connected-peer symmetry");
        } else {
            assert_private_eq(
                row.identity.as_str(),
                endpoint,
                "accepted host socket lost the declared endpoint",
            );
        }
    }
}

fn application_socket_pairs(
    host_rows: &[UnixDescriptor],
    bun_rows: &[UnixDescriptor],
    endpoint: &str,
) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for client in bun_rows {
        if let Some(peer) = client.identity.strip_prefix("->")
            && host_rows
                .iter()
                .any(|server| server.socket == peer && server.identity == endpoint)
        {
            pairs.insert((peer.to_owned(), client.socket.clone()));
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::super::private_diagnostics::private_failure_message;
    use super::*;

    #[test]
    fn application_link_census_preserves_private_pairing_without_leaking_identity() {
        const PRIVATE: &str = "KELD_TEST_PRIVATE_ENDPOINT_TOKEN_PATH";
        let host_rows = [UnixDescriptor {
            descriptor: PRIVATE.to_owned(),
            socket: "0xHOST".to_owned(),
            identity: PRIVATE.to_owned(),
        }];
        let bun_rows = [UnixDescriptor {
            descriptor: PRIVATE.to_owned(),
            socket: "0xBUN".to_owned(),
            identity: "->0xHOST".to_owned(),
        }];
        let pairs = application_socket_pairs(&host_rows, &bun_rows, PRIVATE);
        assert!(
            pairs.len() == 1,
            "actual connected peer relation must still match"
        );
        // The census line itself is count-only by construction (pinned by the
        // callsite test below). The identity predicates go through the #1009
        // diagnostics owner: a mismatch still fails, naming only the reason,
        // never the private endpoint or a kernel socket address.
        for (left, right, reason) in [
            (
                host_rows[0].identity.as_str(),
                "0xOTHER",
                "accepted host socket lost the declared endpoint",
            ),
            (
                bun_rows[0].identity.as_str(),
                "->0xOTHER",
                "client FD clone changed its connected peer",
            ),
            ("0xHOST", "0xBUN", "host connected-peer symmetry"),
        ] {
            let message = private_failure_message(|| {
                assert_private_eq(left, right, reason);
            });
            assert!(message.contains(reason), "fixed reason missing");
            for identity in [PRIVATE, "0xHOST", "0xBUN", "0xOTHER"] {
                assert!(
                    !message.contains(identity),
                    "private endpoint or kernel socket identity leaked"
                );
            }
        }
    }

    #[test]
    fn application_link_stderr_callsite_must_remain_count_only() {
        let (implementation, _) = include_str!("application_link.rs")
            .split_once("#[cfg(test)]\nmod tests")
            .expect("source-binding regression owns adjacent test boundary");
        let compact: String = implementation
            .chars()
            .filter(|char| !char.is_whitespace())
            .collect();
        assert!(
            compact.contains(
                "eprintln!(\"KELD_KEL140_APPLICATION_SOCKET_CENSUShost_fd_count={}\
                 bun_fd_count={}connected_pairs={}\",\
                 host_rows.len(),bun_rows.len(),pairs.len());"
            ),
            "actual socket census stderr callsite must stay count-only"
        );
        assert!(
            implementation.matches("eprintln!(").count() == 1,
            "socket census must not add a second unreviewed diagnostic sink"
        );
        // Raw endpoint/pair interpolation anywhere in public_fs/ is rejected
        // by the #1009 source guard (which would flag those literals here);
        // the inspected PIDs are not guard bindings, so they stay checked.
        assert!(
            !implementation.contains("host={host}") && !implementation.contains("bun={bun}"),
            "raw socket census PID interpolation must not return"
        );
    }
}
