//! Exact Windows machine-install protection, distinct from owner-private staging.

use std::fs::File;
use std::io;
use std::os::windows::fs::MetadataExt as _;

use windows_permissions::constants::{
    AccessRights, AceFlags, AceType, SeObjectType, SecurityInformation,
};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{GetSecurityInfo, SetSecurityInfo};
use windows_permissions::{LocalBox, SecurityDescriptor, Sid};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

const SYSTEM: &str = "S-1-5-18";
const ADMINISTRATORS: &str = "S-1-5-32-544";
const TRUSTED_INSTALLER: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";
const DIRECTORY: &str = "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)";
const FILE: &str = "O:SYD:P(A;;FA;;;SY)(A;;0x1200a9;;;BU)";

/// Requires the actual current process TokenUser to be LocalSystem.
///
/// # Errors
/// Refuses all other users, including elevated administrators, or token-query failure.
pub fn require_windows_system_token() -> io::Result<()> {
    let current = current_process_sid()?;
    let system: LocalBox<Sid> = SYSTEM.parse()?;
    if current.as_ref() != system.as_ref() {
        return Err(io::Error::other("initializer TokenUser is not LocalSystem"));
    }
    Ok(())
}

/// Validates the committed SYSTEM-owned, protected SYSTEM/Users-RX directory policy.
///
/// # Errors
/// Refuses a non-directory, reparse point, wrong owner, or any noncanonical ACE.
pub fn validate_windows_machine_directory(object: &File) -> io::Result<()> {
    validate(object, true)
}

/// Validates the committed SYSTEM-owned, protected SYSTEM/Users-RX regular-file policy.
///
/// # Errors
/// Refuses a non-file, reparse point, wrong owner, or any noncanonical ACE.
pub fn validate_windows_machine_file(object: &File) -> io::Result<()> {
    validate(object, false)
}

/// Seals an existing SYSTEM-private directory to the committed machine policy.
///
/// The caller must own a retained handle with WRITE_DAC. This never repairs an
/// unprotected or foreign object and never changes ownership.
///
/// # Errors
/// Refuses non-SYSTEM callers, a changed private descriptor, or setting/readback failure.
pub fn seal_windows_machine_directory(object: &mut File) -> io::Result<()> {
    require_windows_system_token()?;
    crate::validate_windows_owner_private_directory(object)?;
    seal(object, true)
}

/// Seals an existing inherited SYSTEM-private file to the committed machine policy.
///
/// The caller must own a retained handle with WRITE_DAC. This never repairs an
/// unprotected or foreign object and never changes ownership.
///
/// # Errors
/// Refuses non-SYSTEM callers, a changed private descriptor, or setting/readback failure.
pub fn seal_windows_machine_file(object: &mut File) -> io::Result<()> {
    require_windows_system_token()?;
    crate::validate_windows_owner_private_file(object)?;
    seal(object, false)
}

/// Checks the persistent namespace protection of a volume-root directory.
///
/// Only SYSTEM, Administrators and TrustedInstaller may own or mutate the anchor.
/// Other trustees may read/execute or create new directories, but cannot delete,
/// replace, change attributes, write data or change its security descriptor.
/// Inherit-only rules are ignored here; every descendant needs independent admission.
///
/// # Errors
/// Refuses unknown ACE forms, absent DACLs, untrusted owners or effective write rights.
pub fn validate_windows_machine_volume_anchor(object: &File) -> io::Result<()> {
    ensure_kind(object, true)?;
    let descriptor = descriptor(object)?;
    validate_anchor_descriptor(&descriptor)
}

fn descriptor(object: &File) -> io::Result<LocalBox<SecurityDescriptor>> {
    GetSecurityInfo(
        object,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
    .map_err(io::Error::other)
}

fn ensure_kind(object: &File, directory: bool) -> io::Result<()> {
    let metadata = object.metadata()?;
    if (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(io::Error::other(
            "machine object has wrong type or reparse data",
        ));
    }
    Ok(())
}

fn validate(object: &File, directory: bool) -> io::Result<()> {
    ensure_kind(object, directory)?;
    let descriptor = descriptor(object)?;
    validate_descriptor(&descriptor, directory)
}

fn validate_descriptor(actual: &SecurityDescriptor, directory: bool) -> io::Result<()> {
    let expected: LocalBox<SecurityDescriptor> =
        if directory { DIRECTORY } else { FILE }.parse()?;
    if actual.owner() != expected.owner() || !actual.as_sddl()?.to_string_lossy().contains("D:P") {
        return Err(io::Error::other("machine owner or DACL protection differs"));
    }
    let actual_acl = actual
        .dacl()
        .ok_or_else(|| io::Error::other("machine DACL absent"))?;
    let expected_acl = expected
        .dacl()
        .ok_or_else(|| io::Error::other("canonical DACL absent"))?;
    if actual_acl.len() != expected_acl.len() {
        return Err(io::Error::other(
            "machine DACL must contain exactly two rules",
        ));
    }
    for index in 0..expected_acl.len() {
        let actual = actual_acl
            .get_ace(index)
            .ok_or_else(|| io::Error::other("unreadable machine ACE"))?;
        let expected = expected_acl
            .get_ace(index)
            .ok_or_else(|| io::Error::other("unreadable canonical ACE"))?;
        if actual.ace_type() != expected.ace_type()
            || actual.flags() != expected.flags()
            || actual.mask() != expected.mask()
            || actual.sid() != expected.sid()
        {
            return Err(io::Error::other(
                "machine access rule differs from SYSTEM/full and Users/RX",
            ));
        }
    }
    Ok(())
}

fn seal(object: &mut File, directory: bool) -> io::Result<()> {
    let expected: LocalBox<SecurityDescriptor> =
        if directory { DIRECTORY } else { FILE }.parse()?;
    SetSecurityInfo(
        object,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        expected.dacl(),
        None,
    )?;
    validate(object, directory)
}

fn trusted(sid: &Sid) -> io::Result<bool> {
    for text in [SYSTEM, ADMINISTRATORS, TRUSTED_INSTALLER] {
        let expected: LocalBox<Sid> = text.parse()?;
        if sid == expected.as_ref() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_anchor_descriptor(descriptor: &SecurityDescriptor) -> io::Result<()> {
    let owner = descriptor
        .owner()
        .ok_or_else(|| io::Error::other("volume anchor owner absent"))?;
    if !trusted(owner)? {
        return Err(io::Error::other("volume anchor has an untrusted owner"));
    }
    let dacl = descriptor
        .dacl()
        .ok_or_else(|| io::Error::other("volume anchor DACL absent"))?;
    // FILE_ADD_SUBDIRECTORY = 4. FILE_ADD_FILE aliases WRITE_DATA and is excluded.
    let permitted = (AccessRights::FileGenericRead | AccessRights::FileGenericExecute).bits() | 4;
    let ordinary_flags = AceFlags::ContainerInherit
        | AceFlags::ObjectInherit
        | AceFlags::NoPropagateInherit
        | AceFlags::InheritOnly
        | AceFlags::Inherited;
    for index in 0..dacl.len() {
        let ace = dacl
            .get_ace(index)
            .ok_or_else(|| io::Error::other("unreadable volume anchor ACE"))?;
        if !matches!(
            ace.ace_type(),
            AceType::ACCESS_ALLOWED_ACE_TYPE | AceType::ACCESS_DENIED_ACE_TYPE
        ) || (ace.flags().bits() & !ordinary_flags.bits()) != 0
        {
            return Err(io::Error::other("unsupported volume anchor ACE form"));
        }
        let sid = ace
            .sid()
            .ok_or_else(|| io::Error::other("volume anchor ACE SID absent"))?;
        if ace.flags().contains(AceFlags::InheritOnly)
            || ace.ace_type() == AceType::ACCESS_DENIED_ACE_TYPE
        {
            continue;
        }
        if !trusted(sid)? && ace.mask().bits() & !permitted != 0 {
            return Err(io::Error::other(format!(
                "volume anchor grants mutation to {sid}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_descriptor_fields_have_independent_falsifiers() {
        for (sddl, directory) in [
            ("O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)", true),
            ("O:SYD:P(A;;FA;;;SY)(A;;0x1200a9;;;BU)", false),
        ] {
            let descriptor: LocalBox<SecurityDescriptor> =
                sddl.parse().expect("literal descriptor");
            validate_descriptor(&descriptor, directory).expect("literal machine contract");
            assert!(validate_descriptor(&descriptor, !directory).is_err());
        }
        for sddl in [
            "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)",
            "O:SYD:(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)",
            "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BU)",
            "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;AU)",
            "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)(A;;FW;;;BU)",
        ] {
            let descriptor: LocalBox<SecurityDescriptor> = sddl.parse().expect("mutated literal");
            assert!(validate_descriptor(&descriptor, true).is_err(), "{sddl}");
        }
    }

    #[test]
    fn anchor_rejects_parent_replacement_and_unknown_rights() {
        let accepted: LocalBox<SecurityDescriptor> =
            "O:SYD:P(A;OICI;FA;;;SY)(A;;0x1200ad;;;BU)(A;OICIIO;FA;;;AU)"
                .parse()
                .expect("anchor");
        validate_anchor_descriptor(&accepted).expect("read/create-directory only");
        for mask in ["0x40", "SD", "WD", "WO", "0x2", "0x10", "0x100", "GW", "GA"] {
            let descriptor: LocalBox<SecurityDescriptor> =
                format!("O:SYD:P(A;;FA;;;SY)(A;;{mask};;;BU)")
                    .parse()
                    .expect("single fault");
            assert!(validate_anchor_descriptor(&descriptor).is_err(), "{mask}");
        }
        let owner: LocalBox<SecurityDescriptor> =
            "O:BUD:P(A;;FA;;;SY)".parse().expect("owner fault");
        assert!(validate_anchor_descriptor(&owner).is_err());
    }
}
