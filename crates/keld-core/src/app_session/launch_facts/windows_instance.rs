//! One pre-spawn election for an admitted Windows original development launch.

use std::fs::TryLockError;
use std::path::Path;

use keld_update::WindowsInstanceLeaseFile;

use super::HostAppError;
use crate::SingleInstanceDeclaration;

#[derive(Debug)]
pub(super) enum Election {
    Acquired(WindowsInstanceLeaseFile),
    Contended,
}

/// The launch owner calls this once after checking the live development lease,
/// prepared original source and Legacy Guard admission. Keep the actual winner
/// lease through recovery and teardown. Native I/O stays outside the session
/// transition, whose publication cut can still reject this outcome.
pub(super) fn acquire(declaration: &SingleInstanceDeclaration) -> Result<Election, HostAppError> {
    let root = keld_wv::webview2::native_local_app_data()
        .map_err(|error| election_error("single-instance LocalAppData lookup", error))?;
    acquire_at(&root, declaration)
}

fn acquire_at(
    root: &Path,
    declaration: &SingleInstanceDeclaration,
) -> Result<Election, HostAppError> {
    let lease = WindowsInstanceLeaseFile::open(root, declaration.id())
        .map_err(|error| election_error("single-instance lease admission", error))?;
    let acquired = match lease.file().try_lock() {
        Ok(()) => true,
        Err(TryLockError::WouldBlock) => false,
        Err(TryLockError::Error(error)) => {
            return Err(election_error("single-instance kernel lock", error));
        }
    };
    lease
        .validate()
        .map_err(|error| election_error("single-instance post-lock revalidation", error))?;
    Ok(if acquired {
        Election::Acquired(lease)
    } else {
        Election::Contended
    })
}

fn election_error(phase: &'static str, detail: impl std::fmt::Display) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-037",
        phase,
        detail.to_string(),
        "Verify the native lock directory owner and permissions. If another tool has the lease file open without sharing, close that handle. Preserve the persistent lease file.",
    )
}

#[cfg(test)]
#[path = "windows_instance/tests.rs"]
pub(super) mod tests;
