//! One Windows owner-private ACL policy for staged directories and files.

use std::fs::File;
use std::io;
use std::os::windows::fs::MetadataExt as _;

use windows_permissions::constants::{
    AccessRights, AceFlags, AceType, SeObjectType, SecurityInformation,
};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{ConvertSidToStringSid, GetSecurityInfo};
use windows_permissions::{LocalBox, SecurityDescriptor};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

/// Builds the canonical protected current-user directory descriptor for atomic creation.
///
/// # Errors
/// Returns an I/O error when the current token user cannot be read or the descriptor
/// cannot be built.
pub fn windows_owner_private_directory_security() -> io::Result<LocalBox<SecurityDescriptor>> {
    let current = current_process_sid()?;
    let sid = ConvertSidToStringSid(&current)?;
    format!(
        "O:{}D:P(A;OICI;FA;;;{})",
        sid.to_string_lossy(),
        sid.to_string_lossy()
    )
    .parse()
}

/// Checks the exact owner-private directory object and descriptor by retained handle.
///
/// # Errors
/// Returns an I/O error if the handle is not a real directory or its current-user
/// owner and protected, single-ACE ACL do not match the canonical policy.
pub fn validate_windows_owner_private_directory(directory: &File) -> io::Result<()> {
    validate(directory, ObjectKind::Directory)
}

/// Checks an inherited owner-private regular file by retained handle.
///
/// # Errors
/// Returns an I/O error if the handle is not a real regular file or its current-user
/// owner and single inherited full-control ACE do not match the canonical policy.
pub fn validate_windows_owner_private_file(file: &File) -> io::Result<()> {
    validate(file, ObjectKind::File)
}

#[derive(Clone, Copy)]
enum ObjectKind {
    Directory,
    File,
}

fn validate(object: &File, kind: ObjectKind) -> io::Result<()> {
    let metadata = object.metadata()?;
    let expected_type = match kind {
        ObjectKind::Directory => metadata.is_dir(),
        ObjectKind::File => metadata.is_file(),
    };
    if !expected_type || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other(
            "object has the wrong type or is a reparse point",
        ));
    }
    let descriptor = GetSecurityInfo(
        object,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
    .map_err(io::Error::other)?;
    validate_descriptor(&descriptor, kind)
}

fn validate_descriptor(descriptor: &SecurityDescriptor, kind: ObjectKind) -> io::Result<()> {
    let current = current_process_sid().map_err(io::Error::other)?;
    if descriptor.owner() != Some(&current) {
        return Err(io::Error::other(
            "owner does not equal the current process TokenUser SID",
        ));
    }
    let sddl = descriptor.as_sddl().map_err(io::Error::other)?;
    let protected = sddl.to_string_lossy().contains("D:P");
    if protected != matches!(kind, ObjectKind::Directory) {
        return Err(io::Error::other("DACL inheritance protection is incorrect"));
    }
    let dacl = descriptor
        .dacl()
        .ok_or_else(|| io::Error::other("security descriptor contains no DACL"))?;
    if dacl.len() != 1 {
        return Err(io::Error::other(format!(
            "expected one access rule, found {}",
            dacl.len()
        )));
    }
    let ace = dacl
        .get_ace(0)
        .ok_or_else(|| io::Error::other("the one access rule is unreadable"))?;
    let required_flags = match kind {
        ObjectKind::Directory => AceFlags::ContainerInherit | AceFlags::ObjectInherit,
        ObjectKind::File => AceFlags::Inherited,
    };
    if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE
        || ace.mask() != AccessRights::FileAllAccess
        || ace.sid() != Some(&current)
        || ace.flags() != required_flags
    {
        return Err(io::Error::other(
            "expected one current-user full-control rule with the required inheritance",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid_strings() -> (String, &'static str) {
        let current = current_process_sid().expect("TokenUser");
        let sid = ConvertSidToStringSid(&current).expect("SID text");
        let own = sid.to_string_lossy().into_owned();
        let other = if own == "S-1-5-18" {
            "S-1-5-11"
        } else {
            "S-1-5-18"
        };
        (own, other)
    }

    fn assert_descriptor_rejected(sddl: &str, kind: ObjectKind, expected: &str) {
        let descriptor: LocalBox<SecurityDescriptor> =
            sddl.parse().expect("independent SDDL fixture must parse");
        let error = validate_descriptor(&descriptor, kind)
            .expect_err("single-predicate mutation must be refused");
        assert!(error.to_string().contains(expected), "{sddl}: {error}");
    }

    #[test]
    fn canonical_directory_descriptor_and_inherited_file_rule() {
        let directory = windows_owner_private_directory_security().expect("current-user policy");
        validate_descriptor(&directory, ObjectKind::Directory).expect("canonical directory");
        assert!(validate_descriptor(&directory, ObjectKind::File).is_err());

        let current = current_process_sid().expect("TokenUser");
        let sid = ConvertSidToStringSid(&current).expect("SID text");
        let file: LocalBox<SecurityDescriptor> = format!(
            "O:{}D:AI(A;ID;FA;;;{})",
            sid.to_string_lossy(),
            sid.to_string_lossy()
        )
        .parse()
        .expect("inherited file descriptor");
        validate_descriptor(&file, ObjectKind::File).expect("one inherited file ACE");
        assert!(validate_descriptor(&file, ObjectKind::Directory).is_err());
    }

    #[test]
    fn directory_descriptor_predicates_have_independent_falsifiers() {
        let (own, other) = sid_strings();
        let directory = format!("O:{own}D:P(A;OICI;FA;;;{own})");
        let descriptor: LocalBox<SecurityDescriptor> =
            directory.parse().expect("canonical directory SDDL");
        validate_descriptor(&descriptor, ObjectKind::Directory).expect("baseline descriptor");

        // Each fixture changes exactly one field of the accepted descriptor.
        for (sddl, expected) in [
            (
                format!("O:{own}D:(A;OICI;FA;;;{own})"),
                "DACL inheritance protection",
            ),
            (
                format!("O:{other}D:P(A;OICI;FA;;;{own})"),
                "owner does not equal",
            ),
            (
                format!("O:{own}D:P(A;OICI;FA;;;{own})(A;OICI;FA;;;{own})"),
                "expected one access rule",
            ),
            (
                format!("O:{own}D:P(A;OICI;FA;;;{other})"),
                "expected one current-user",
            ),
            (
                format!("O:{own}D:P(D;OICI;FA;;;{own})"),
                "expected one current-user",
            ),
            (
                format!("O:{own}D:P(A;OICI;FR;;;{own})"),
                "expected one current-user",
            ),
            (
                format!("O:{own}D:P(A;OI;FA;;;{own})"),
                "expected one current-user",
            ),
        ] {
            assert_descriptor_rejected(&sddl, ObjectKind::Directory, expected);
        }
    }

    #[test]
    fn inherited_file_protection_and_ace_flags_are_separate_predicates() {
        let (own, _) = sid_strings();
        let file = format!("O:{own}D:AI(A;ID;FA;;;{own})");
        let descriptor: LocalBox<SecurityDescriptor> =
            file.parse().expect("canonical inherited file SDDL");
        validate_descriptor(&descriptor, ObjectKind::File).expect("baseline descriptor");

        assert_descriptor_rejected(
            &format!("O:{own}D:PAI(A;ID;FA;;;{own})"),
            ObjectKind::File,
            "DACL inheritance protection",
        );
        assert_descriptor_rejected(
            &format!("O:{own}D:AI(A;;FA;;;{own})"),
            ObjectKind::File,
            "expected one current-user",
        );
    }

    #[test]
    fn retained_file_cannot_pass_directory_validation() {
        let executable = std::env::current_exe().expect("current test executable");
        let file = File::open(executable).expect("open test executable");
        assert!(validate_windows_owner_private_directory(&file).is_err());
    }
}
