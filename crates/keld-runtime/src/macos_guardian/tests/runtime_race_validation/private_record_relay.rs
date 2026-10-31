use std::io::{self, Write as _};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use super::records_and_cut_types::{
    COLLECTION, ControlFields, RelayCommand, RelayReceipt, receive_record,
};
use super::{
    CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARED, CONTROL_REVOKE, RuntimeError,
    lifecycle_error, runtime_io_kind, write_generation_control,
};

pub(super) fn private_record_relay(
    mut host: UnixStream,
    mut guardian: UnixStream,
    commands: &Receiver<RelayCommand>,
    receipts: &Sender<RelayReceipt>,
) -> Result<(), RuntimeError> {
    loop {
        let request = match receive_record(&mut guardian) {
            Ok(record) => record,
            Err(error) if runtime_io_kind(&error) == Some(io::ErrorKind::UnexpectedEof) => {
                // Actual preparer/Supervisor control closure drives the
                // real host owner EOF; the relay fabricates no record.
                receipts
                    .send(RelayReceipt::GuardianControlEof)
                    .map_err(|_| {
                        lifecycle_error(
                            "test relay actual EOF receipt",
                            io::Error::other("custodian ended"),
                        )
                    })?;
                return host.shutdown(Shutdown::Both).map_err(|source| {
                    lifecycle_error("test relay observed control closure", source)
                });
            }
            Err(error) => return Err(error),
        };
        write_generation_control(&mut host, &request)?;
        let response = receive_record(&mut host)?;
        if response.kind == CONTROL_PREPARED && response.attempt == 2 {
            let mut encoded = Vec::new();
            write_generation_control(&mut encoded, &response)?;
            receipts
                .send(RelayReceipt::PreparedTwo(response))
                .map_err(|_| {
                    lifecycle_error(
                        "test relay Prepared receipt",
                        io::Error::other("custodian ended"),
                    )
                })?;
            match commands.recv_timeout(COLLECTION).map_err(|source| {
                lifecycle_error("test relay delivery release", io::Error::other(source))
            })? {
                RelayCommand::Full => guardian.write_all(&encoded).map_err(|source| {
                    lifecycle_error("test relay complete Prepared write", source)
                })?,
                RelayCommand::Prefix => {
                    // One canonical byte is a valid prefix, not a
                    // malformed substitute response or model timeout.
                    let began = Instant::now();
                    guardian.write_all(&encoded[..1]).map_err(|source| {
                        lifecycle_error("test relay Prepared prefix write", source)
                    })?;
                    receipts
                        .send(RelayReceipt::PrefixSent(began))
                        .map_err(|_| {
                            lifecycle_error(
                                "test relay prefix receipt",
                                io::Error::other("custodian ended"),
                            )
                        })?;
                    match commands.recv_timeout(COLLECTION).map_err(|source| {
                        lifecycle_error(
                            "test relay partial outcome collection",
                            io::Error::other(source),
                        )
                    })? {
                        RelayCommand::AwaitClosure => {}
                        RelayCommand::Full | RelayCommand::Prefix => {
                            return Err(lifecycle_error(
                                "test relay prefix command",
                                io::Error::other("unexpected duplicate delivery"),
                            ));
                        }
                    }
                }
                RelayCommand::AwaitClosure => {}
            }
        } else {
            write_generation_control(&mut guardian, &response)?;
            // These facts come from the actual requests and actual
            // host replies, not the shared Revoked event that EOF can
            // also emit. Normal lease cleanup must earn its own oracle.
            let receipt = match (request.kind, request.attempt) {
                (CONTROL_REVOKE, 2) => Some(RelayReceipt::NormalRevokeTwo {
                    request: ControlFields::capture(&request),
                    response: ControlFields::capture(&response),
                }),
                (CONTROL_CLEAR, 2) => Some(RelayReceipt::NormalClearTwo {
                    request: ControlFields::capture(&request),
                    response: ControlFields::capture(&response),
                }),
                _ => None,
            };
            if let Some(receipt) = receipt {
                receipts.send(receipt).map_err(|_| {
                    lifecycle_error(
                        "test relay normal cleanup receipt",
                        io::Error::other("custodian ended"),
                    )
                })?;
            }
            if response.kind == CONTROL_CLEARED && response.attempt == 1 {
                receipts.send(RelayReceipt::ClearedOne).map_err(|_| {
                    lifecycle_error(
                        "test relay Clear receipt",
                        io::Error::other("custodian ended"),
                    )
                })?;
            }
        }
    }
}
