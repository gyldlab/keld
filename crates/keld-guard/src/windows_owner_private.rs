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

/// Builds the canonical inherited current-user regular-file descriptor.
///
/// This is suitable only beneath a validated owner-private directory.
///
/// # Errors
/// Returns an I/O error when the current token user cannot be read or the descriptor
/// cannot be built.
pub fn windows_owner_private_file_security() -> io::Result<LocalBox<SecurityDescriptor>> {
    let current = current_process_sid()?;
    let sid = ConvertSidToStringSid(&current)?;
    format!(
        "O:{}D:AI(A;ID;FA;;;{})",
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
    use std::fs::{self, OpenOptions};
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_permissions::wrappers::SetNamedSecurityInfo;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    struct OwnedDirectory {
        path: std::path::PathBuf,
        retained: Option<File>,
    }
    impl Drop for OwnedDirectory {
        fn drop(&mut self) {
            // No unpinned name-based ACL changes or deletion if the native
            // directory HANDLE was never successfully retained.
            if self.retained.is_none() {
                return;
            }
            // FILE_SHARE_DELETE was deliberately denied at original open.
            // The retained kernel object prevents a rename/reparse swap
            // while the test-owned DACL and exact marker are restored.
            if let Ok(canonical) = windows_owner_private_directory_security() {
                let _ = SetNamedSecurityInfo(
                    &self.path,
                    SeObjectType::SE_FILE_OBJECT,
                    SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
                    None,
                    None,
                    canonical.dacl(),
                    None,
                );
            }
            let marker = self.path.join("marker");
            // Preserve an independently substituted marker even while
            // the parent directory remains bound to our original HANDLE.
            if fs::read(&marker).is_ok_and(|bytes| bytes == b"retained object stays live") {
                let _ = fs::remove_file(&marker);
            }
            drop(self.retained.take());
            // The final removal is nonrecursive and refuses reparse paths:
            // a substituted directory with foreign contents stays intact.
            if fs::symlink_metadata(&self.path).is_ok_and(|meta| {
                meta.is_dir() && meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
            }) {
                let _ = fs::remove_dir(&self.path);
            }
        }
    }

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

        let file = windows_owner_private_file_security().expect("inherited current-user file");
        validate_descriptor(&file, ObjectKind::File).expect("canonical inherited file");
        assert!(validate_descriptor(&file, ObjectKind::Directory).is_err());

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

    #[test]
    fn retained_directory_observes_actual_post_open_dacl_mutation() {
        let scratch = std::env::temp_dir().join(format!(
            "keld-guard-retained-acl-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir(&scratch).expect("unique native ACL fixture");
        let mut fixture = OwnedDirectory {
            path: scratch,
            retained: None,
        };
        let canonical = windows_owner_private_directory_security().expect("canonical DACL");
        SetNamedSecurityInfo(
            &fixture.path,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
            None,
            None,
            canonical.dacl(),
            None,
        )
        .expect("publish canonical real Windows DACL");
        fs::write(fixture.path.join("marker"), b"retained object stays live")
            .expect("native test marker");
        let held = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&fixture.path)
            .expect("retain the same real directory object");
        fixture.retained = Some(
            held.try_clone()
                .expect("fixture owns original kernel HANDLE"),
        );
        validate_windows_owner_private_directory(&held)
            .expect("canonical protected native directory admits by held HANDLE");
        let swapped = fixture.path.with_extension("renamed");
        if fs::rename(&fixture.path, &swapped).is_ok() {
            fs::rename(&swapped, &fixture.path).expect("rescue test-owned original path");
            panic!("non-DELETE-shared original directory HANDLE must refuse rename");
        }

        // Independently mutate the exact live DACL after opening and retaining
        // the directory HANDLE. No pathname reopen is used by the validator.
        let current = current_process_sid().expect("TokenUser");
        let sid = ConvertSidToStringSid(&current).expect("SID text");
        let sid = sid.to_string_lossy();
        let injected: LocalBox<SecurityDescriptor> =
            format!("O:{sid}D:P(A;OICI;FA;;;{sid})(A;OICI;FR;;;WD)")
                .parse()
                .expect("independent two-rule Windows descriptor");
        SetNamedSecurityInfo(
            &fixture.path,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
            None,
            None,
            injected.dacl(),
            None,
        )
        .expect("apply extra Everyone read ACE through independent path");
        let observed = GetSecurityInfo(
            &held,
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Owner | SecurityInformation::Dacl,
        )
        .expect("observe actual mutated DACL through retained HANDLE");
        assert_eq!(
            observed.dacl().expect("native DACL").len(),
            2,
            "the kernel, not a string fixture, contains the extra ACE"
        );
        let error = validate_windows_owner_private_directory(&held)
            .expect_err("extra Everyone read ACE must invalidate the retained directory");
        assert!(
            error.to_string().contains("expected one access rule"),
            "refusal must be live ACL admission, not a missing or reopened object: {error}"
        );
        assert_eq!(
            fs::read(fixture.path.join("marker")).expect("marker after refusal"),
            b"retained object stays live"
        );
        drop(held);
        let owned_path = fixture.path.clone();
        drop(fixture);
        assert!(
            !owned_path.exists(),
            "the pinned test fixture must leave no orphan after restoring its own DACL"
        );
    }
}
