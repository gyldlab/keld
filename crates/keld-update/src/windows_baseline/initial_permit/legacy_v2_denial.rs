//! Read-only native identity check for the legacy initializer's v2 bypass.
//!
//! The legacy KEL-53 initializer accepts a caller-selected installation
//! identity. This check detects a retained installation ancestor that is the
//! actual reserved v2 directory, including paths that spell that object
//! differently. It never authorizes v1 initialization or creates an object.
//!
//! The `local_app_data` input MUST come from a D-42 admitted worker's
//! SHGetKnownFolderPath(FOLDERID_LocalAppData, 0, NULL) result, opened and
//! retained without reparse traversal. No such native issuer exists yet:
//! callers MUST NOT pass LOCALAPPDATA, a caller path, or a fabricated Dir as
//! evidence. The parent initializer must remain fail-closed for commissioned
//! v2 until that issuer is available and this check is called before effects.

use std::io;

use cap_fs_ext::{DirExt as _, MetadataExt as _};
use cap_std::fs::Dir;

/// Deny an installed root whose retained ancestry enters the exact canonical
/// LocalAppData/GYLDLAB/Keld/v2 directory object. No path-string comparison
/// participates in the decision: both sides are retained OS directory handles
/// and the comparison uses native volume and file identities.
///
/// A missing reserved subtree only proves this v2 directory was not observed.
/// It does not authenticate the legacy root, nor authorize any installation.
pub(in crate::windows_baseline) fn refuse_legacy_v2_ancestor(
    local_app_data: &Dir,
    retained_install_ancestors: &[Dir],
) -> io::Result<()> {
    if retained_install_ancestors.is_empty() {
        return Err(io::Error::other("no retained installation ancestry"));
    }

    let mut reserved = local_app_data.try_clone()?;
    for component in ["GYLDLAB", "Keld", "v2"] {
        reserved = match reserved.open_dir_nofollow(component) {
            Ok(directory) => directory,
            Err(cause) if cause.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(cause) => return Err(cause),
        };
    }

    let native_reserved = reserved.dir_metadata()?;
    if !native_reserved.is_dir() {
        return Err(io::Error::other("reserved v2 object is not a directory"));
    }
    for ancestor in retained_install_ancestors {
        let actual = ancestor.dir_metadata()?;
        if !actual.is_dir() {
            return Err(io::Error::other(
                "retained installation ancestor is not a directory",
            ));
        }
        if actual.dev() == native_reserved.dev() && actual.ino() == native_reserved.ino() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "legacy baseline initializer cannot enter the reserved native v2 installation root",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::Path;

    use cap_std::ambient_authority;

    use super::refuse_legacy_v2_ancestor;
    use crate::windows_extraction::open_ancestors;

    fn open_local(path: &Path) -> cap_std::fs::Dir {
        cap_std::fs::Dir::open_ambient_dir(path, ambient_authority())
            .expect("retain local-app-data fixture directory")
    }

    #[test]
    fn retained_reserved_v2_identity_refuses_before_any_install_effect() {
        let temp = tempfile::tempdir().expect("isolated namespace");
        let local = temp.path().join("UserData");
        let install = local.join("GYLDLAB").join("Keld").join("v2").join("leaf");
        fs::create_dir_all(install.join("updates").join("versions"))
            .expect("create an independent v2 fixture");
        let local = open_local(&local);
        let ancestors = open_ancestors(&install).expect("retain every real directory");

        let refusal = refuse_legacy_v2_ancestor(&local, &ancestors)
            .expect_err("native v2 ancestor cannot enter legacy initialization");
        assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);

        // The Windows canonicalized (normally verbatim-prefixed) spelling
        // references the same OS directory. This second independent open
        // proves the native file-identity comparison, not literal path text.
        let alternate_path = fs::canonicalize(&install).expect("Windows canonical path");
        let alternate_ancestors =
            open_ancestors(&alternate_path).expect("retain alternate spelling");
        let refusal = refuse_legacy_v2_ancestor(&local, &alternate_ancestors)
            .expect_err("a second spelling cannot evade native directory identity");
        assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn sibling_legacy_directory_is_a_distinct_native_object() {
        let temp = tempfile::tempdir().expect("isolated namespace");
        let local = temp.path().join("UserData");
        let legacy = local.join("GYLDLAB").join("Keld").join("legacy").join("leaf");
        fs::create_dir_all(local.join("GYLDLAB").join("Keld").join("v2"))
            .expect("create the canonical v2 namespace");
        fs::create_dir_all(&legacy).expect("create legacy fixture");
        let local = open_local(&local);
        let ancestors = open_ancestors(&legacy).expect("retain legacy ancestry");

        refuse_legacy_v2_ancestor(&local, &ancestors)
            .expect("a distinct sibling object is not the reserved v2 object");
    }

    #[test]
    fn unknown_or_empty_ancestry_never_proves_a_legacy_installation() {
        let temp = tempfile::tempdir().expect("isolated namespace");
        let local = temp.path().join("UserData");
        fs::create_dir(&local).expect("create root fixture");
        let local = open_local(&local);
        let err = refuse_legacy_v2_ancestor(&local, &[])
            .expect_err("absence of retained native ancestry refuses");
        assert_eq!(err.kind(), io::ErrorKind::Other);
    }

    #[test]
    fn malformed_reserved_directory_is_a_refusal_not_an_absence() {
        let temp = tempfile::tempdir().expect("isolated namespace");
        let local = temp.path().join("UserData");
        let legacy = local.join("Legacy").join("leaf");
        let reserved_parent = local.join("GYLDLAB").join("Keld");
        fs::create_dir_all(&legacy).expect("create legacy fixture");
        fs::create_dir_all(&reserved_parent).expect("create reserved parent");
        fs::write(reserved_parent.join("v2"), b"file-instead-of-directory")
            .expect("create invalid reserved leaf");
        let local = open_local(&local);
        let ancestors = open_ancestors(&legacy).expect("retain legacy ancestry");

        refuse_legacy_v2_ancestor(&local, &ancestors)
            .expect_err("a malformed existing v2 leaf is not a missing subtree");
    }
}
