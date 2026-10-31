//! Strict authority-free Windows executable locator grammar.
//! Signed provenance and pinned filesystem handles stay with the parent.

use super::{PROVENANCE, binding};
use crate::{UpdateError, WindowsLocatedImage, records};
use std::path::{Component, Path, PathBuf, Prefix};

/// The installation layout a locator names; every component is checked lexically.
#[derive(Debug)]
pub(super) struct LocatedLayout {
    pub(super) install_root: PathBuf,
    pub(super) update_name: String,
    pub(super) version: String,
}

impl LocatedLayout {
    pub(super) fn parse(image: WindowsLocatedImage, locator: &Path) -> Result<Self, UpdateError> {
        let mut components = locator.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            return Err(binding(
                image,
                "locator",
                "not an absolute local drive path",
            ));
        };
        if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
            || components.next() != Some(Component::RootDir)
        {
            return Err(binding(
                image,
                "locator",
                "not an absolute local drive path",
            ));
        }
        let mut names = Vec::new();
        for component in components {
            let Component::Normal(name) = component else {
                return Err(binding(image, "locator", "contains a non-normal component"));
            };
            names.push(
                name.to_str()
                    .ok_or_else(|| binding(image, "locator", "not UTF-8"))?
                    .to_owned(),
            );
        }
        let [install @ .., update, versions, version, tree, file] = names.as_slice() else {
            return Err(binding(image, "locator", "too few components"));
        };
        if install.is_empty() {
            return Err(binding(image, "locator", "install root is a volume root"));
        }
        if file != image.file_name() || tree != "tree" || versions != "versions" {
            return Err(binding(
                image,
                "locator",
                format!(
                    "path must end in exactly versions\\<version>\\tree\\{}",
                    image.file_name()
                ),
            ));
        }
        if update.eq_ignore_ascii_case(PROVENANCE) {
            return Err(binding(
                image,
                "locator",
                "update root conflicts with provenance",
            ));
        }
        records::strict_version(version)
            .map_err(|_| binding(image, "locator", "version directory is not strict SemVer"))?;
        let mut install_root = PathBuf::from(prefix.as_os_str());
        install_root.push(Path::new(r"\"));
        for name in install {
            install_root.push(name);
        }
        Ok(Self {
            install_root,
            update_name: update.clone(),
            version: version.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::LocatedLayout;
    use crate::WindowsLocatedImage;

    #[test]
    fn strict_locator_rejects_case_aliased_provenance_root_and_wrong_image() {
        let alias = Path::new(r"C:\Keld\INSTALL-PROVENANCE\versions\1.2.3\tree\keld-host.exe");
        assert!(
            LocatedLayout::parse(WindowsLocatedImage::Host, alias).is_err(),
            "a case alias of the signed provenance leaf may never be the update root"
        );
        let wrong_image = Path::new(r"C:\Keld\update\versions\1.2.3\tree\keld-updater-helper.exe");
        assert!(
            LocatedLayout::parse(WindowsLocatedImage::Host, wrong_image).is_err(),
            "the closed host image choice may not be substituted"
        );
    }

    #[test]
    fn strict_locator_requires_semver_and_exact_literal_components() {
        for invalid in [
            r"C:\Keld\update\versions\1.2\tree\keld-host.exe",
            r"C:\Keld\update\VERSIONS\1.2.3\tree\keld-host.exe",
            r"C:\Keld\update\versions\1.2.3\Tree\keld-host.exe",
            r"\\server\share\Keld\update\versions\1.2.3\tree\keld-host.exe",
        ] {
            assert!(
                LocatedLayout::parse(WindowsLocatedImage::Host, Path::new(invalid)).is_err(),
                "invalid locator must fail closed: {invalid}"
            );
        }
        let valid = Path::new(r"C:\Keld\update\versions\1.2.3\tree\keld-host.exe");
        let layout = LocatedLayout::parse(WindowsLocatedImage::Host, valid)
            .expect("exact local drive shape and strict version");
        assert_eq!(layout.version, "1.2.3");
        assert_eq!(layout.update_name, "update");
        assert_eq!(layout.install_root, Path::new(r"C:\Keld"));
    }
}
