//! Shared typed guardian lifecycle failure mapping and aggregation.
//! One error owner; child modules retain exact existing error surfaces.

use crate::RuntimeError;
use nix::errno::Errno;
use std::io;

pub(super) fn nix_io_error(source: Errno) -> io::Error {
    io::Error::from_raw_os_error(source as i32)
}

pub(super) fn lifecycle_error(phase: &'static str, source: io::Error) -> RuntimeError {
    RuntimeError::Lifecycle { phase, source }
}

pub(super) fn collapse_failures(mut failures: Vec<RuntimeError>) -> RuntimeError {
    if failures.len() == 1
        && let Some(error) = failures.pop()
    {
        return error;
    }
    let details = failures
        .into_iter()
        .map(|error| error.to_string())
        .collect::<Vec<_>>()
        .join("; ");
    lifecycle_error("macOS guardian cleanup", io::Error::other(details))
}
