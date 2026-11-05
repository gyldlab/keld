//! Digest-verified, single-handle permissions-manifest loading.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::{
    ManifestError, PermissionsManifest, ResolvedWindowCsp, WindowCspProfile, parse_manifest_at,
    read_manifest_bytes,
};

/// Parsed permissions paired with the SHA-256 verified over its exact source bytes.
///
/// The fields have no public constructor or mutable/raw-byte accessor, so callers
/// cannot independently substitute a manifest or digest after verification.
#[derive(Clone)]
pub struct VerifiedManifest {
    manifest: PermissionsManifest,
    verified_sha256: [u8; 32],
}

impl std::fmt::Debug for VerifiedManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifiedManifest")
            .field("verified_sha256", &self.verified_sha256)
            .finish_non_exhaustive()
    }
}

impl VerifiedManifest {
    /// Returns the parsed permissions whose source bytes passed verification.
    #[must_use]
    pub const fn manifest(&self) -> &PermissionsManifest {
        &self.manifest
    }

    /// Returns the SHA-256 computed over the exact bytes supplied to the parser.
    #[must_use]
    pub const fn verified_sha256(&self) -> [u8; 32] {
        self.verified_sha256
    }

    /// Resolves one Host-selected key through the sole immutable policy catalog.
    ///
    /// Directly parsed data cannot mint this verified result:
    ///
    /// ```compile_fail
    /// if let Ok(data) = keld_guard::parse_manifest("{}") {
    ///     let _policy = data.resolve_window_csp("main");
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns `KELD-GUARD005` when the key has no selected CSP. This never
    /// synthesizes a default policy or derives a response from `/app/net`.
    pub fn resolve_window_csp(&self, key: &str) -> Result<ResolvedWindowCsp, ManifestError> {
        let profile = self.manifest.windows.get(key).and_then(|window| window.web.csp)
            .ok_or_else(|| ManifestError::Parse {
                path: None,
                detail: format!("window {key:?} has no verified CSP selector; set windows.<key>.web.csp to the qualified named policy before loading content"),
            })?;
        Ok(ResolvedWindowCsp::new(key, self.verified_sha256, profile))
    }

    /// Resolves the sole key for a Host-selected resource policy class.
    ///
    /// The caller supplies a closed profile, never an application-selected key.
    /// Every relevant native view must still bind this key through its current
    /// Core registry association before loading content.
    ///
    /// # Errors
    /// Returns `KELD-GUARD005` for zero or multiple matching keys. A missing or
    /// ambiguous mapping never guesses `main`, picks the first key or defaults.
    pub fn resolve_unique_window_csp(
        &self,
        profile: WindowCspProfile,
    ) -> Result<ResolvedWindowCsp, ManifestError> {
        let mut keys = self
            .manifest
            .windows
            .iter()
            .filter(|(_, window)| window.web.csp == Some(profile))
            .map(|(key, _)| key);
        let key = keys.next().ok_or_else(|| ManifestError::Parse {
            path: None,
            detail: format!("no window selects {}; declare exactly one trusted window key before loading original resources", profile.as_str()),
        })?;
        if keys.next().is_some() {
            return Err(ManifestError::Parse {
                path: None,
                detail: format!(
                    "multiple windows select {}; declare exactly one trusted window key for this resource class",
                    profile.as_str()
                ),
            });
        }
        self.resolve_window_csp(key)
    }
}

/// Reads, verifies, UTF-8 decodes, and parses one already-validated file handle.
///
/// `display_path` is used only in diagnostics and is never opened. The function
/// consumes `file`, owns the sole byte buffer, verifies those bytes before UTF-8
/// decoding, and parses the same buffer without exposing it.
///
/// # Errors
///
/// Returns [`ManifestError::Read`] when the retained handle cannot be read,
/// [`ManifestError::TooLarge`] when it exceeds the shared 64 KiB ceiling,
/// [`ManifestError::IntegrityMismatch`] when its exact bytes do not match
/// `expected_sha256`, [`ManifestError::InvalidUtf8`] for non-UTF-8 bytes, or
/// [`ManifestError::Parse`] for malformed JSONC or reserved window CSP metadata, or
/// [`ManifestError::InvalidAuthScope`] for a malformed auth issuer grant.
pub fn load_verified_manifest(
    file: File,
    display_path: PathBuf,
    expected_sha256: [u8; 32],
) -> Result<VerifiedManifest, ManifestError> {
    load_verified_reader(file, display_path, expected_sha256)
}

fn load_verified_reader(
    reader: impl Read,
    display_path: PathBuf,
    expected_sha256: [u8; 32],
) -> Result<VerifiedManifest, ManifestError> {
    let bytes = read_manifest_bytes(reader, &display_path)?;
    let actual: [u8; 32] = Sha256::digest(&bytes).into();
    if actual != expected_sha256 {
        return Err(ManifestError::IntegrityMismatch {
            path: display_path,
            expected: expected_sha256,
            actual,
        });
    }
    let text = std::str::from_utf8(&bytes).map_err(|source| ManifestError::InvalidUtf8 {
        path: display_path.clone(),
        detail: source.to_string(),
    })?;
    let manifest = parse_manifest_at(text, Some(&display_path))?;
    Ok(VerifiedManifest {
        manifest,
        verified_sha256: actual,
    })
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _bytes: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected retained-handle failure"))
        }
    }

    #[test]
    fn retained_handle_read_failure_is_guard004() {
        let path = PathBuf::from("/diagnostic/keld.permissions.jsonc");
        let error = load_verified_reader(FailingReader, path.clone(), [0_u8; 32])
            .expect_err("read failure must fail closed");
        assert_eq!(error.code(), "KELD-GUARD004");
        assert!(
            matches!(error, ManifestError::Read { path: ref actual, .. } if actual == &path),
            "{error:?}"
        );
        assert!(error.to_string().contains("Check the path"), "{error}");
    }

    #[test]
    fn digest_verified_policy_still_rejects_auth_control_sentinel() {
        let bytes = br#"{"app":{"auth":{"token":["\u0000unresolved"]}}}"#;
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        let path = PathBuf::from("/diagnostic/keld.permissions.jsonc");
        let error = load_verified_reader(io::Cursor::new(bytes), path.clone(), digest)
            .expect_err("matching digest must not bypass auth grant validation");
        assert!(
            matches!(
                error,
                ManifestError::InvalidAuthScope { path: Some(ref actual), ref capability }
                    if actual == &path && capability == "auth.token"
            ),
            "{error:?}"
        );
        assert_eq!(error.code(), "KELD-GUARD005");
    }
}
