//! Protected Windows metadata record preparation, flush, and atomic publication.
//!
//! The baseline owner retains trust/lease/transaction sequencing; this private
//! module validates a bounded pending sibling and verifies its published bytes.

use super::{error, random_leaf_name, read_record};
use crate::UpdateError;
use crate::windows_extraction::ensure_regular;
use crate::windows_fs::{create_file_relative_with_profile, publish_new};
use cap_std::fs::{Dir, File as CapFile};
use std::io::Write as _;

/// Prepares one protected record sibling, then publishes it only at an absent target.
///
/// The same owner handles installer-baseline records and completed package markers;
/// the caller selects only the already-admitted directory profile and fixed leaf.
pub(crate) fn publish_new_record(
    parent: &Dir,
    leaf: &str,
    bytes: &[u8],
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> Result<(), UpdateError> {
    let temporary = prepare_record(parent, bytes, profile)?;
    publish_prepared_record(
        parent,
        &temporary,
        RecordTarget::Absent(leaf),
        bytes,
        profile,
    )
}

/// Where a prepared protected record sibling is published.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RecordTarget<'leaf> {
    /// A fixed leaf that must not exist yet.
    Absent(&'leaf str),
    /// One fixed activation record slot, replaced (or created) under the writer lease.
    Replace(crate::windows_fs::RecordSlot),
}

pub(super) fn prepare_record(
    parent: &Dir,
    bytes: &[u8],
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> Result<String, UpdateError> {
    if bytes.len() > crate::records::MAX_LOCAL_RECORD_BYTES {
        return Err(error("record creation", "record exceeds 64 KiB"));
    }
    let temporary = random_leaf_name("pending").map_err(|cause| error("record identity", cause))?;
    let retained_parent = parent
        .try_clone()
        .map_err(|cause| error("record parent", cause))?
        .into_std_file();
    let mut output = CapFile::from_std(
        create_file_relative_with_profile(&retained_parent, &temporary, profile)
            .map_err(|cause| error("record creation", cause))?,
    );
    ensure_regular(
        &output
            .metadata()
            .map_err(|cause| error("record metadata", cause))?,
    )
    .map_err(|cause| error("record kind", cause))?;
    keld_guard::validate_windows_install_file(
        &output
            .try_clone()
            .map_err(|cause| error("record handle", cause))?
            .into_std(),
        profile,
    )
    .map_err(|cause| error("record protection", cause))?;
    output
        .write_all(bytes)
        .map_err(|cause| error("record write", cause))?;
    let output = output.into_std();
    output
        .sync_all()
        .map_err(|cause| error("record flush", cause))?;
    drop(output);
    let (_, observed) = read_record(parent, &temporary, profile)?;
    if observed != bytes {
        return Err(error("record readback", "flushed bytes differ"));
    }
    Ok(temporary)
}

pub(super) fn publish_prepared_record(
    parent: &Dir,
    temporary: &str,
    target: RecordTarget<'_>,
    bytes: &[u8],
    profile: keld_guard::WindowsInstallProtectionProfile,
) -> Result<(), UpdateError> {
    let retained_parent = parent
        .try_clone()
        .map_err(|cause| error("record parent", cause))?
        .into_std_file();
    let leaf = match target {
        RecordTarget::Absent(leaf) => {
            publish_new(&retained_parent, temporary, leaf)
                .map_err(|cause| error("record publication", cause))?;
            leaf
        }
        RecordTarget::Replace(slot) => {
            crate::windows_fs::replace_record_slot(&retained_parent, temporary, slot)
                .map_err(|cause| error("record replacement", cause))?;
            slot.leaf()
        }
    };
    let (_, observed) = read_record(parent, leaf, profile)?;
    if observed != bytes {
        return Err(error("published record readback", "published bytes differ"));
    }
    Ok(())
}
