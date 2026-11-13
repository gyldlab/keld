//! Fixed Windows lifecycle-keeper transcript: challenge, ACK and one-use receipt.
//!
//! The caller separately pins the named-pipe peer process, validates its token and
//! owns the handshake deadline. These safe record operations never confer OS
//! authority or mint a process identity.

use std::io::{self, Read as _, Write as _};

use super::{
    WindowsLifecycleBinding, WindowsLifecycleBindingKnowledge, WindowsLifecycleExpectation,
    WindowsLifecyclePurpose, WindowsNamedPipeBootstrapStream,
};
use crate::token::SessionToken;

const LIFECYCLE_BINDING_RECORD_LEN: usize = 8 + 1 + (32 * 3) + (32 * 2) + (4 * 2);

pub(super) fn lifecycle_binding_record(
    magic: [u8; 8],
    binding: WindowsLifecycleBinding,
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    client_pid: u32,
    server_pid: u32,
) -> [u8; LIFECYCLE_BINDING_RECORD_LEN] {
    let mut record = [0; LIFECYCLE_BINDING_RECORD_LEN];
    let mut cursor = 0;
    record[cursor..cursor + magic.len()].copy_from_slice(&magic);
    cursor += magic.len();
    record[cursor] = binding.purpose as u8;
    cursor += 1;
    for identity in [
        binding.installation_id,
        binding.attempt_id,
        binding.lifecycle_channel_id,
        *client_nonce.as_bytes(),
        *server_nonce.as_bytes(),
    ] {
        record[cursor..cursor + identity.len()].copy_from_slice(&identity);
        cursor += identity.len();
    }
    for pid in [client_pid, server_pid] {
        record[cursor..cursor + 4].copy_from_slice(&pid.to_le_bytes());
        cursor += 4;
    }
    debug_assert_eq!(cursor, record.len());
    record
}

pub(super) fn authenticate_lifecycle_binding_server(
    stream: &mut WindowsNamedPipeBootstrapStream,
    binding: WindowsLifecycleBinding,
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    client_pid: u32,
    server_pid: u32,
) -> io::Result<[u8; LIFECYCLE_BINDING_RECORD_LEN]> {
    let challenge = lifecycle_binding_record(
        *b"KELD-LC1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    );
    stream.write_all(&challenge)?;
    let mut acknowledgement = [0; LIFECYCLE_BINDING_RECORD_LEN];
    stream.read_exact(&mut acknowledgement)?;
    let expected_acknowledgement = lifecycle_binding_record(
        *b"KELD-LA1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    );
    if acknowledgement != expected_acknowledgement {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "lifecycle acknowledgement does not match the authenticated install/attempt/peer",
        ));
    }
    Ok(lifecycle_binding_record(
        *b"KELD-LR1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    ))
}

pub(super) fn authenticate_lifecycle_binding_client(
    stream: &mut WindowsNamedPipeBootstrapStream,
    expectation: WindowsLifecycleExpectation,
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    client_pid: u32,
    server_pid: u32,
) -> io::Result<(WindowsLifecycleBinding, WindowsLifecycleBindingKnowledge)> {
    let mut received = [0; LIFECYCLE_BINDING_RECORD_LEN];
    stream.read_exact(&mut received)?;
    let binding = decode_lifecycle_challenge(
        &received,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    )?;
    let knowledge = expectation.accepts(binding).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "keeper lifecycle challenge changed install, attempt, channel or purpose",
        )
    })?;
    let acknowledgement = lifecycle_binding_record(
        *b"KELD-LA1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    );
    stream.write_all(&acknowledgement)?;
    let expected_receipt = lifecycle_binding_record(
        *b"KELD-LR1",
        binding,
        client_nonce,
        server_nonce,
        client_pid,
        server_pid,
    );
    let mut receipt = [0; LIFECYCLE_BINDING_RECORD_LEN];
    stream.read_exact(&mut receipt)?;
    if receipt != expected_receipt {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "lifecycle listener did not confirm one-use context consumption",
        ));
    }
    Ok((binding, knowledge))
}

pub(super) fn decode_lifecycle_challenge(
    record: &[u8; LIFECYCLE_BINDING_RECORD_LEN],
    client_nonce: SessionToken,
    server_nonce: SessionToken,
    client_pid: u32,
    server_pid: u32,
) -> io::Result<WindowsLifecycleBinding> {
    if record[..8] != *b"KELD-LC1"
        || record[105..137] != *client_nonce.as_bytes()
        || record[137..169] != *server_nonce.as_bytes()
        || record[169..173] != client_pid.to_le_bytes()
        || record[173..177] != server_pid.to_le_bytes()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "keeper challenge has wrong protocol, nonce or connected-process transcript",
        ));
    }
    let purpose = match record[8] {
        1 => WindowsLifecyclePurpose::CoordinatorToKeeper,
        2 => WindowsLifecyclePurpose::KeeperToSuccessor,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "keeper challenge has an unknown lifecycle purpose",
            ));
        }
    };
    let installation_id = record[9..41]
        .try_into()
        .map_err(|_| io::Error::other("installation ID record range changed"))?;
    let attempt_id = record[41..73]
        .try_into()
        .map_err(|_| io::Error::other("attempt ID record range changed"))?;
    let lifecycle_channel_id = record[73..105]
        .try_into()
        .map_err(|_| io::Error::other("lifecycle channel record range changed"))?;
    WindowsLifecycleBinding::new(installation_id, attempt_id, lifecycle_channel_id, purpose)
}
