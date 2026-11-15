//! Strict Win32-canonical executable path admission for the updater helper.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::path::{Component, Path, PathBuf, Prefix};

use super::{WindowsUpdaterHelperLaunchError, nul_terminated_wide};

/// A Win32-canonical drive-letter `.exe` path for
/// [`super::launch_elevated_updater_helper`].
///
/// `SHELLEXECUTEINFO` resolves a file name without a path against the current
/// directory, and hands a document file to its associated application. This
/// type admits a path only when all of these hold:
/// - it starts at a drive root (`C:\...`): no UNC, `\\?\` or `\\.\` prefix,
///   and no relative or drive-relative form;
/// - it names an `.exe`;
/// - Win32 would not rewrite it: no `.` or `..` component, only single `\`
///   separators, and `GetFullPathNameW` (through `std::path::absolute`)
///   returns it unchanged;
/// - no component ends in `.` or a space, which Win32 strips;
/// - no component holds `<>:"|?*`, NUL or another character below 0x20
///   (`:` would name an alternate data stream);
/// - no component is a reserved device name (`CON`, `PRN`, `AUX`, `NUL`,
///   `COM1`-`COM9`, `LPT1`-`LPT9` and their superscript-digit forms), with or
///   without an extension.
///
/// So the shell receives exactly the spelling the caller derived. The type
/// checks spelling only. It does not decide whether the drive is local (a
/// `subst` or mapped drive letter passes), resolve 8.3 aliases or reparse
/// points, or prove which file runs. The host's derivation from admitted
/// provenance (KEL-53 §6 S9a) and the helper's self-anchor (S9c) own those
/// facts (KEL-53 §4 "Helper launch and self-anchor").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsUpdaterHelperPath {
    pub(super) path: PathBuf,
    pub(super) file: Vec<u16>,
    pub(super) directory: Vec<u16>,
}

impl WindowsUpdaterHelperPath {
    /// Admits `path` if it is a Win32-canonical drive-letter `.exe` path.
    ///
    /// # Errors
    ///
    /// Returns [`WindowsUpdaterHelperLaunchError::HelperPath`] naming the first
    /// rule that `path` breaks.
    pub fn new(path: &Path) -> Result<Self, WindowsUpdaterHelperLaunchError> {
        let refuse = |rule| Err(WindowsUpdaterHelperLaunchError::HelperPath { rule });
        let mut components = path.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            return refuse("it does not start with a drive letter");
        };
        if !matches!(prefix.kind(), Prefix::Disk(_)) {
            return refuse("its prefix is not a plain drive letter");
        }
        if components.next() != Some(Component::RootDir) {
            return refuse("it is not absolute from the drive root");
        }
        let mut names = Vec::new();
        for component in components {
            let Component::Normal(name) = component else {
                return refuse("it has a `..` component");
            };
            if name.encode_wide().any(|unit| unit == u16::from(b':')) {
                return refuse("it names an alternate data stream");
            }
            if name.encode_wide().any(is_forbidden_name_unit) {
                return refuse("it holds a character that Win32 forbids in a name");
            }
            // Checked before the full-path check: Microsoft documents that
            // before Windows 11 that normalization turns a path naming a legacy
            // device into a `\\.\` device path, which would change the rule.
            if is_reserved_device_name(name) {
                return refuse("a component is a reserved device name");
            }
            names.push(name);
        }
        let Some(file_name) = names.last() else {
            return refuse("it names no file");
        };
        if !Path::new(file_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            return refuse("it does not name an `.exe` image");
        }
        // Component parsing drops `.` components and repeated or trailing
        // separators and reads `/` as a separator, so a path is canonical
        // exactly when its components rebuild the same text.
        if path.components().collect::<PathBuf>().as_os_str() != path.as_os_str() {
            return refuse(
                "it is not canonical: a `.` component or a `/`, doubled or trailing separator",
            );
        }
        // Win32's own normalization (GetFullPathNameW) also removes a single
        // trailing `.` from an inner component.
        if std::path::absolute(path)
            .ok()
            .as_deref()
            .map(Path::as_os_str)
            != Some(path.as_os_str())
        {
            return refuse("Win32 rewrites it to a different full path");
        }
        // GetFullPathNameW keeps an inner trailing space and a run of periods,
        // but Win32 file names must not end in either.
        for name in &names {
            if name
                .encode_wide()
                .last()
                .is_some_and(|unit| unit == u16::from(b'.') || unit == u16::from(b' '))
            {
                return refuse("a component ends in `.` or a space");
            }
        }
        let (Some(file), Some(directory)) = (
            nul_terminated_wide(path.as_os_str()),
            path.parent()
                .and_then(|parent| nul_terminated_wide(parent.as_os_str())),
        ) else {
            return refuse("it contains a NUL");
        };
        Ok(Self {
            path: path.to_path_buf(),
            file,
            directory,
        })
    }

    /// The admitted path, unchanged.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

/// Whether `unit` is one of the characters that Microsoft's "Naming Files,
/// Paths, and Namespaces" forbids in a file or directory name: `<>"/\|?*` or
/// a value from 0 through 31. `:` is checked separately, and the separators
/// never reach a component.
fn is_forbidden_name_unit(unit: u16) -> bool {
    unit < 0x20
        || b"<>\"|?*"
            .iter()
            .any(|forbidden| unit == u16::from(*forbidden))
}

/// Whether `name` is a reserved device name, alone or followed by an
/// extension: `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `COM¹`-`COM³`,
/// `LPT1`-`LPT9` or `LPT¹`-`LPT³`, in any case ("Naming Files, Paths, and
/// Namespaces": "NUL.txt and NUL.tar.gz are both equivalent to NUL"). Spaces
/// before the first `.` are ignored, so a spelling such as `NUL .exe` is also
/// refused.
fn is_reserved_device_name(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|digit| {
                matches!(
                    digit,
                    "1" | "2"
                        | "3"
                        | "4"
                        | "5"
                        | "6"
                        | "7"
                        | "8"
                        | "9"
                        | "\u{b9}"
                        | "\u{b2}"
                        | "\u{b3}"
                )
            })
}
