//! Owns real manifest-source files, raw-byte SHA receipts and cleanup for the
//! public Native auth snapshot-boundary regression; no product policy oracle.
use keld_guard::verified_manifest::{VerifiedManifest, load_verified_manifest};
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub(super) struct OwnedSnapshotFiles {
    pub(super) root: PathBuf,
    cleaned: bool,
}
impl OwnedSnapshotFiles {
    pub(super) fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("test-only filename seed is after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "keld-auth-snapshot-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create fresh privately owned snapshot fixture dir");
        Self {
            root,
            cleaned: false,
        }
    }

    pub(super) fn finish(mut self) {
        fs::remove_dir_all(&self.root).expect("clean up verified snapshot fixtures");
        self.cleaned = true;
    }
}
impl Drop for OwnedSnapshotFiles {
    fn drop(&mut self) {
        if !self.cleaned
            && let Err(error) = fs::remove_dir_all(&self.root)
        {
            eprintln!("emergency snapshot fixture cleanup failed: {error}");
        }
    }
}

pub(super) fn verified(root: &Path, name: &str, bytes: &[u8]) -> VerifiedManifest {
    let path = root.join(name);
    fs::write(&path, bytes).expect("write actual privately owned manifest source");
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    load_verified_manifest(
        File::open(&path).expect("open same owned manifest"),
        path,
        digest,
    )
    .expect("actual retained-handle bytes independently matched SHA-256")
}
