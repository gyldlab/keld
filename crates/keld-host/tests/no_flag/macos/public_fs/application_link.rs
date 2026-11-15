//! Existing macOS descriptor census binds one connected application socket pair.
use crate::support::unix_descriptors::{UnixDescriptor, unix_descriptors};
use serde_json::json;
use std::collections::BTreeSet;

pub(super) fn assert_single_application_link(host: u32, bun: u32, endpoint: &str) {
    let host_rows = unix_descriptors(host);
    let bun_rows = unix_descriptors(bun);
    let describe = |rows: &[UnixDescriptor]| {
        rows.iter()
            .map(|row| {
                json!({
                    "descriptor":row.descriptor,"socket":row.socket,"identity":row.identity,
                })
            })
            .collect::<Vec<_>>()
    };
    eprintln!(
        "KELD_KEL140_APPLICATION_SOCKET_CENSUS host={host} bun={bun} endpoint={endpoint} host_rows={:?} bun_rows={:?}",
        describe(&host_rows),
        describe(&bun_rows)
    );
    // Bound listeners and accepted FD clones can share a pathname. A Bun
    // connected-peer identity selects the actual host socket object instead.
    let pairs = application_socket_pairs(&host_rows, &bun_rows, endpoint);
    eprintln!("KELD_KEL140_APPLICATION_SOCKET_PAIRS {pairs:?}");
    assert_eq!(pairs.len(), 1, "one connected application socket pair");
    let (server_socket, client_socket) = pairs.first().expect("one actual connected pair");
    let client_peer = format!("->{server_socket}");
    for row in bun_rows.iter().filter(|row| &row.socket == client_socket) {
        assert_eq!(
            row.identity, client_peer,
            "client FD clone changed its connected peer"
        );
    }
    for row in host_rows.iter().filter(|row| &row.socket == server_socket) {
        if let Some(peer) = row.identity.strip_prefix("->") {
            assert_eq!(peer, client_socket.as_str(), "host connected-peer symmetry");
        } else {
            assert_eq!(
                row.identity, endpoint,
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
