//! Owns real manifest-source files, raw-byte SHA receipts and cleanup for the
//! public Native auth snapshot-boundary regression; no product policy oracle.
use keld_guard::verified_manifest::{VerifiedManifest, load_verified_manifest};
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub(super) struct OwnedSnapshotFiles {
    pub(super) root: PathBuf,
    directory: TempDir,
}
impl OwnedSnapshotFiles {
    pub(super) fn new() -> Self {
        let directory = tempfile::Builder::new()
            .prefix("keld-auth-snapshot-test-")
            .tempdir()
            .expect("create fresh privately owned snapshot fixture dir");
        Self {
            root: directory.path().to_path_buf(),
            directory,
        }
    }

    pub(super) fn finish(self) {
        self.directory
            .close()
            .expect("clean up verified snapshot fixtures");
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
