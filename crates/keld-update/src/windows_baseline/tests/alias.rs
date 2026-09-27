//! Loader integration proof for a same-volume DOS-device directory alias.

#![allow(unsafe_code)] // Test-only temporary DOS mapping, removed by exact target.
#![deny(unsafe_op_in_unsafe_fn)]

use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    DDD_EXACT_MATCH_ON_REMOVE, DDD_NO_BROADCAST_SYSTEM, DDD_RAW_TARGET_PATH, DDD_REMOVE_DEFINITION,
    DefineDosDeviceW, QueryDosDeviceW,
};

use super::support::trust_for;
use crate::windows_baseline::load_windows_baseline;

struct Mapping {
    name: Vec<u16>,
    target: Vec<u16>,
    active: bool,
}

impl Mapping {
    fn create(root: &Path) -> Self {
        let name = (b'M'..=b'Z')
            .rev()
            .find_map(|letter| {
                let name: Vec<u16> = format!("{}:", char::from(letter))
                    .encode_utf16()
                    .chain([0])
                    .collect();
                let mut buffer = [0_u16; 1024];
                // SAFETY: name is NUL-terminated and buffer is writable for its stated size.
                let result = unsafe { QueryDosDeviceW(name.as_ptr(), buffer.as_mut_ptr(), 1024) };
                (result == 0 && std::io::Error::last_os_error().raw_os_error() == Some(2))
                    .then_some(name)
            })
            .expect("an unused temporary drive letter; never replace an existing mapping");
        let target: Vec<u16> = format!(r"\??\{}", root.display())
            .encode_utf16()
            .chain([0])
            .collect();
        // SAFETY: both inputs are owned NUL-terminated UTF-16; the exact local fixture
        // target is retained for removal. No existing DOS mapping was selected.
        assert_ne!(
            unsafe {
                DefineDosDeviceW(
                    DDD_RAW_TARGET_PATH | DDD_NO_BROADCAST_SYSTEM,
                    name.as_ptr(),
                    target.as_ptr(),
                )
            },
            0,
            "create temporary test alias"
        );
        Self {
            name,
            target,
            active: true,
        }
    }

    fn prefix(&self) -> String {
        String::from_utf16(&self.name[..2]).expect("ASCII drive")
    }

    fn remove(&mut self) -> bool {
        // SAFETY: the original name and target remain live; exact-match removal
        // cannot remove a different target that another caller may have installed.
        let removed = unsafe {
            DefineDosDeviceW(
                DDD_REMOVE_DEFINITION
                    | DDD_EXACT_MATCH_ON_REMOVE
                    | DDD_RAW_TARGET_PATH
                    | DDD_NO_BROADCAST_SYSTEM,
                self.name.as_ptr(),
                self.target.as_ptr(),
            )
        };
        if removed != 0 {
            self.active = false;
        }
        removed != 0
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        if self.active {
            let _ = self.remove();
        }
    }
}

pub(super) fn run(root: &Path) {
    super::support::assert_ordinary_token();
    let trust = trust_for(&root.join("success"));
    let mut mapping = Mapping::create(root);
    let mut aliased = trust.clone();
    aliased.installation.install_root = format!(r"{}\success", mapping.prefix()).into();
    aliased.installation.update_root = aliased.installation.install_root.join("updates");
    let refused = load_windows_baseline(&aliased).err();
    // This ordinary-user mapping stays in the caller's local DOS namespace. The
    // protected record remains untouched. Removing actual-root validation reaches
    // identity mismatch instead, and therefore fails the exact error oracle below.
    assert!(mapping.remove(), "remove exact temporary test mapping");
    let error = refused.expect("same-volume subdirectory cannot become the drive anchor");
    assert!(matches!(
        error,
        crate::UpdateError::Baseline {
            step: "committed scaffold admission",
            ..
        }
    ));
    assert!(
        error.to_string().contains("volume root"),
        "the actual anchor predicate must decide: {error}"
    );
    drop(load_windows_baseline(&trust).expect("direct-root loader positive control"));
    println!("KELD_KEL266_ALIAS_REFUSED_AND_REMOVED");
}
