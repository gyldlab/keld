//! Existing macOS descriptor census binds one connected application socket pair.
use crate::support::unix_descriptors::{UnixDescriptor, unix_descriptors};
use std::collections::BTreeSet;

pub(super) fn assert_single_application_link(host: u32, bun: u32, endpoint: &str) {
    let host_rows = unix_descriptors(host);
    let bun_rows = unix_descriptors(bun);
    // Counts only: never echo raw process IDs, socket identities or the
    // operator's private app-link endpoint into hosted failing stderr.
    eprintln!(
        "KELD_KEL140_APPLICATION_SOCKET_CENSUS host_count={} bun_count={}",
        host_rows.len(),
        bun_rows.len()
    );
    assert_application_link_rows(&host_rows, &bun_rows, endpoint);
}

fn assert_application_link_rows(
    host_rows: &[UnixDescriptor],
    bun_rows: &[UnixDescriptor],
    endpoint: &str,
) {
    // Bound listeners and accepted FD clones can share a pathname. A Bun
    // connected-peer identity selects the actual host socket object instead.
    let pairs = application_socket_pairs(host_rows, bun_rows, endpoint);
    eprintln!(
        "KELD_KEL140_APPLICATION_SOCKET_PAIRS pair_count={}",
        pairs.len()
    );
    assert!(pairs.len() == 1, "one connected application socket pair");
    let (server_socket, client_socket) = pairs.first().expect("one actual connected pair");
    let client_peer = format!("->{server_socket}");
    for row in bun_rows.iter().filter(|row| &row.socket == client_socket) {
        assert!(
            row.identity == client_peer,
            "client FD clone changed its connected peer"
        );
    }
    for row in host_rows.iter().filter(|row| &row.socket == server_socket) {
        if let Some(peer) = row.identity.strip_prefix("->") {
            assert!(peer == client_socket, "host connected-peer symmetry");
        } else {
            assert!(
                row.identity == endpoint,
                "accepted host socket lost the declared endpoint"
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
mod redacted_link_diagnostics_tests {
    use super::{UnixDescriptor, assert_application_link_rows};

    const PRIVATE_SENTINEL: &str = "KELD_PRIVATE_APP_LINK_PATH_DO_NOT_LOG";
    const ENDPOINT: &str = "/private/test/real-app-link.sock";

    fn socket(socket: &str, identity: &str) -> UnixDescriptor {
        UnixDescriptor {
            descriptor: String::from("5"),
            socket: socket.to_owned(),
            identity: identity.to_owned(),
        }
    }

    fn assert_fixed_panic(host: &[UnixDescriptor], bun: &[UnixDescriptor], expected: &str) {
        let Err(panic) = std::panic::catch_unwind(|| {
            assert_application_link_rows(host, bun, ENDPOINT);
        }) else {
            panic!("invalid private application socket must be rejected");
        };
        let diagnostic = if let Some(text) = panic.downcast_ref::<String>() {
            text.as_str()
        } else if let Some(text) = panic.downcast_ref::<&str>() {
            text
        } else {
            panic!("unexpected nontext application socket panic");
        };
        assert_eq!(diagnostic, expected, "unexpected redacted failure stage");
        assert!(
            !diagnostic.contains(PRIVATE_SENTINEL),
            "application socket failure leaked a private identity"
        );
    }

    #[test]
    fn socket_census_peer_errors_reject_without_raw_paths() {
        let host = vec![socket("0xA", ENDPOINT)];
        let bun = vec![socket("0xB", "->0xA")];
        assert_application_link_rows(&host, &bun, ENDPOINT);

        let mut bad_bun = bun;
        bad_bun.push(socket("0xB", PRIVATE_SENTINEL));
        assert_fixed_panic(
            &host,
            &bad_bun,
            "client FD clone changed its connected peer",
        );

        let mut bad_host_peer = host;
        bad_host_peer.push(socket("0xA", &format!("->{PRIVATE_SENTINEL}")));
        assert_fixed_panic(
            &bad_host_peer,
            &bad_bun[..1],
            "host connected-peer symmetry",
        );

        let mut bad_host_endpoint = bad_host_peer;
        bad_host_endpoint.retain(|row| !row.identity.starts_with("->"));
        bad_host_endpoint.push(socket("0xA", PRIVATE_SENTINEL));
        assert_fixed_panic(
            &bad_host_endpoint,
            &bad_bun[..1],
            "accepted host socket lost the declared endpoint",
        );

        assert_fixed_panic(&[], &bad_bun[..1], "one connected application socket pair");
    }
}
