//! Read-only native identity check for the legacy initializer's v2 bypass.
//!
//! The legacy KEL-53 initializer accepts a caller-selected installation
//! identity. This check detects the actual reserved v2 directory in any
//! retained installation ancestor or scaffold creation parent. It never
//! authorizes v1 initialization or creates an object.
//!
//! The `local_app_data` input MUST come from a D-42 admitted worker's
//! `SHGetKnownFolderPath(FOLDERID_LocalAppData, 0, NULL)` result, opened and
//! retained without reparse traversal. No such native issuer exists yet:
//! callers MUST NOT pass LOCALAPPDATA, a caller path, or a fabricated Dir as
//! evidence. The parent initializer must remain fail-closed for commissioned
//! v2 until that issuer is available and this check is called before effects.

use std::io;

use cap_fs_ext::{DirExt as _, MetadataExt as _};
use cap_std::fs::Dir;

use crate::windows_baseline::Roots;

/// Check every retained KEL-53 creation parent, not only install ancestry.
/// The eventual legacy initializer must pass the same `Roots` that owns its
/// bootstrap and publication effects, so it cannot omit the update or versions
/// directories when invoking this denial.
pub(in crate::windows_baseline) fn refuse_legacy_v2_roots(
    local_app_data: &Dir,
    roots: &Roots,
) -> io::Result<()> {
    refuse_legacy_v2_creation_parents(
        local_app_data,
        &roots.ancestors,
        [&roots.update, &roots.versions],
    )
}

/// Deny an installation creation parent that is the exact canonical
/// LocalAppData/GYLDLAB/Keld/v2 directory object. Both sides use retained OS
/// directory handles and native volume and file identities, not path strings.
///
/// A missing reserved subtree only proves this v2 directory was not observed.
/// It does not authenticate the legacy root, nor authorize any installation.
fn refuse_legacy_v2_creation_parents(
    local_app_data: &Dir,
    retained_install_ancestors: &[Dir],
    retained_scaffold_parents: [&Dir; 2],
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
    for parent in retained_install_ancestors
        .iter()
        .chain(retained_scaffold_parents)
    {
        let actual = parent.dir_metadata()?;
        if !actual.is_dir() {
            return Err(io::Error::other(
                "retained installation creation parent is not a directory",
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

    use super::refuse_legacy_v2_creation_parents;
    use crate::windows_extraction::open_ancestors;

    fn open_local(path: &Path) -> cap_std::fs::Dir {
        cap_std::fs::Dir::open_ambient_dir(path, ambient_authority())
            .expect("retain local-app-data fixture directory")
    }

    fn open_scaffold(install: &Path) -> [cap_std::fs::Dir; 2] {
        let update = install.join("updates");
        [open_local(&update), open_local(&update.join("versions"))]
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
        let scaffold = open_scaffold(&install);

        let refusal =
            refuse_legacy_v2_creation_parents(&local, &ancestors, [&scaffold[0], &scaffold[1]])
                .expect_err("native v2 ancestor cannot enter legacy initialization");
        assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);

        // The Windows canonicalized (normally verbatim-prefixed) spelling
        // references the same OS directory. This second independent open
        // proves the native file-identity comparison, not literal path text.
        let alternate_path = fs::canonicalize(&install).expect("Windows canonical path");
        let alternate_ancestors =
            open_ancestors(&alternate_path).expect("retain alternate spelling");
        let refusal = refuse_legacy_v2_creation_parents(
            &local,
            &alternate_ancestors,
            [&scaffold[0], &scaffold[1]],
        )
        .expect_err("a second spelling cannot evade native directory identity");
        assert_eq!(refusal.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn sibling_legacy_directory_is_a_distinct_native_object() {
        let temp = tempfile::tempdir().expect("isolated namespace");
        let local = temp.path().join("UserData");
        let legacy = local
            .join("GYLDLAB")
            .join("Keld")
            .join("legacy")
            .join("leaf");
        fs::create_dir_all(local.join("GYLDLAB").join("Keld").join("v2"))
            .expect("create the canonical v2 namespace");
        fs::create_dir_all(legacy.join("updates").join("versions")).expect("create legacy fixture");
        let local = open_local(&local);
        let ancestors = open_ancestors(&legacy).expect("retain legacy ancestry");
        let scaffold = open_scaffold(&legacy);

        refuse_legacy_v2_creation_parents(&local, &ancestors, [&scaffold[0], &scaffold[1]])
            .expect("a distinct sibling object is not the reserved v2 object");
    }

    #[test]
    fn every_retained_creation_parent_participates_in_reserved_root_denial() {
        let temp = tempfile::tempdir().expect("isolated namespace");
        let local_path = temp.path().join("UserData");
        let reserved_path = local_path.join("GYLDLAB").join("Keld").join("v2");
        let legacy = local_path.join("Legacy");
        fs::create_dir_all(&reserved_path).expect("create independent reserved fixture");
        fs::create_dir_all(legacy.join("updates").join("versions"))
            .expect("create independent legacy fixture");
        let local = open_local(&local_path);
        let ancestors = open_ancestors(&legacy).expect("retain legacy ancestry");
        let reserved = open_local(&reserved_path);
        let scaffold = open_scaffold(&legacy);

        // The retained objects, not their claimed roles or pathname strings,
        // decide this deny-only predicate. Neither scaffold slot may be omitted.
        for parents in [[&reserved, &scaffold[1]], [&scaffold[0], &reserved]] {
            let error = refuse_legacy_v2_creation_parents(&local, &ancestors, parents)
                .expect_err("a reserved identity in either creation parent must refuse");
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        }
        refuse_legacy_v2_creation_parents(&local, &ancestors, [&scaffold[0], &scaffold[1]])
            .expect("the unchanged legacy creation parents remain distinct");
    }

    #[test]
    fn unknown_or_empty_ancestry_never_proves_a_legacy_installation() {
        let temp = tempfile::tempdir().expect("isolated namespace");
        let local = temp.path().join("UserData");
        fs::create_dir(&local).expect("create root fixture");
        let local = open_local(&local);
        let err = refuse_legacy_v2_creation_parents(&local, &[], [&local, &local])
            .expect_err("absence of retained native ancestry refuses");
        assert_eq!(err.kind(), io::ErrorKind::Other);
    }

    #[test]
    fn malformed_reserved_directory_is_a_refusal_not_an_absence() {
        let temp = tempfile::tempdir().expect("isolated namespace");
        let local = temp.path().join("UserData");
        let legacy = local.join("Legacy").join("leaf");
        let reserved_parent = local.join("GYLDLAB").join("Keld");
        fs::create_dir_all(legacy.join("updates").join("versions")).expect("create legacy fixture");
        fs::create_dir_all(&reserved_parent).expect("create reserved parent");
        fs::write(reserved_parent.join("v2"), b"file-instead-of-directory")
            .expect("create invalid reserved leaf");
        let local = open_local(&local);
        let ancestors = open_ancestors(&legacy).expect("retain legacy ancestry");
        let scaffold = open_scaffold(&legacy);

        refuse_legacy_v2_creation_parents(&local, &ancestors, [&scaffold[0], &scaffold[1]])
            .expect_err("a malformed existing v2 leaf is not a missing subtree");
    }
}
