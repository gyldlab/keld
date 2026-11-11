//! One bounded signature response from an already authorized external key session.
//! This module never creates a key or starts a signer. The current build, signed
//! Host, candidate and literal emitted manifest stay retained while it waits.
use super::{BuildError, local_archive::LocalArchiveCandidate, local_source};
use sha2::{Digest as _, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Completion {
    event: String,
    manifest_sha256: String,
    signature_sha256: String,
    public_hex: String,
}
const WAIT: Duration = Duration::from_secs(300);
fn refuse(e: impl std::fmt::Display) -> BuildError {
    BuildError::input("local manifest signing session", e)
}
/// Only a verified response to this execution's literal manifest constructs this.
/// Not a catalog, release authorization, installation or Strict capability.
pub(super) struct SignedManifest<'candidate, 'images> {
    candidate: &'candidate LocalArchiveCandidate<'images>,
    manifest: PathBuf,
    signature: PathBuf,
    _completion: local_source::PinnedFile,
    _manifest: File,
    _signature: local_source::PinnedFile,
    _request: File,
}
impl SignedManifest<'_, '_> {
    pub(super) fn require_candidate(
        &self,
        candidate: &LocalArchiveCandidate<'_>,
    ) -> Result<(), BuildError> {
        if !std::ptr::eq(self.candidate, candidate) {
            return Err(refuse(
                "signature belongs to a different retained transaction",
            ));
        }
        Ok(())
    }
    pub(super) fn manifest(&self) -> &Path {
        &self.manifest
    }
    pub(super) fn signature(&self) -> &Path {
        &self.signature
    }
}
fn signing_request(manifest: &Path, signature: &Path, bytes: &[u8]) -> Result<Vec<u8>, BuildError> {
    if !manifest.is_absolute()
        || !signature.is_absolute()
        || bytes.is_empty()
        || bytes.len() > 1024 * 1024
    {
        return Err(refuse("bounded absolute signing request required"));
    }
    // Exact existing update_key_session.ts request, no command/key/provider fields.
    let wire=serde_json::to_vec(&serde_json::json!({"manifest":manifest,"sha256":local_source::observed_hex(&Sha256::digest(bytes)),"signatureOut":signature})).map_err(refuse)?;
    if wire.len() > 8192 {
        return Err(refuse("signing wire exceeds existing session bound"));
    }
    Ok(wire)
}
pub(super) fn wait<'candidate, 'images>(
    candidate: &'candidate LocalArchiveCandidate<'images>,
    manifest_file: &File,
) -> Result<SignedManifest<'candidate, 'images>, BuildError> {
    let pair = candidate.signed_pair()?;
    let root = candidate
        .path()
        .parent()
        .ok_or_else(|| refuse("candidate parent absent"))?;
    let manifest = root.join("app.keld.manifest.json");
    let signature = root.join("app.keld.manifest.sig");
    let completion = root.join("manifest-signing-complete.json");
    // Never reuse a stale signature, including a dangling link.
    for output in [&signature, &completion] {
        match std::fs::symlink_metadata(output) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(refuse(e)),
            Ok(_) => return Err(refuse("signature output already exists")),
        }
    }
    let mut retained = manifest_file.try_clone().map_err(refuse)?;
    retained.seek(SeekFrom::Start(0)).map_err(refuse)?;
    let mut bytes = Vec::new();
    (&mut retained)
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(refuse)?;
    let request_bytes = signing_request(&manifest, &signature, &bytes)?;
    let request_path = root.join("manifest-signing-request.json");
    let mut request = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(&request_path)
        .map_err(refuse)?;
    request.write_all(&request_bytes).map_err(refuse)?;
    request.sync_all().map_err(refuse)?;
    let request = super::local_archive::reopen_emitted_archive(
        request,
        &request_path,
        request_bytes.len() as u64,
    )?;
    // Controller relays this file's literal single JSON request into the existing
    // one-use session. stdout is explicitly flushed before waiting. No secrets.
    println!(
        "{}",
        serde_json::json!({"event":"manifest-signing-request","request":request_path,"publicHex":local_source::observed_hex(&pair.release_public_key),"timeoutSeconds":WAIT.as_secs()})
    );
    std::io::stdout().flush().map_err(refuse)?;
    let start = Instant::now();
    loop {
        match std::fs::symlink_metadata(&completion) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(refuse(e)),
        }
        if start.elapsed() >= WAIT {
            return Err(refuse(
                "signature session timed out; no retry, rebuild or installation performed",
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut complete = local_source::pin_file(&completion, 1024)?;
    let mut complete_bytes = Vec::new();
    complete
        .file
        .read_to_end(&mut complete_bytes)
        .map_err(refuse)?;
    let receipt: Completion = serde_json::from_slice(&complete_bytes).map_err(refuse)?;
    if receipt.manifest_sha256 != local_source::observed_hex(&Sha256::digest(&bytes))
        || receipt.public_hex != local_source::observed_hex(&pair.release_public_key)
        || receipt.event != "manifest-session-complete"
    {
        return Err(refuse(
            "completion does not match this manifest and captured public key",
        ));
    }
    // Publication must be closed/complete before visibility. An incomplete or
    // still-write-open response refuses rather than retrying a partial signature.
    let mut response = local_source::pin_file(&signature, 89)?;
    let mut signature_bytes = Vec::new();
    response
        .file
        .read_to_end(&mut signature_bytes)
        .map_err(refuse)?;
    if receipt.signature_sha256 != local_source::observed_hex(&Sha256::digest(&signature_bytes)) {
        return Err(refuse("signature completion digest mismatch"));
    }
    keld_update::verify_literal_release_signature(
        pair.release_public_key,
        &bytes,
        &signature_bytes,
    )
    .map_err(refuse)?;
    Ok(SignedManifest {
        candidate,
        _completion: complete,
        manifest,
        signature,
        _manifest: retained,
        _signature: response,
        _request: request,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signer_wire_matches_existing_one_use_protocol_and_exact_bytes() {
        let bytes = b"literal generated manifest";
        let wire = signing_request(
            Path::new("D:/same/manifest"),
            Path::new("D:/same/signature"),
            bytes,
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&wire).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 3);
        assert_eq!(
            json["sha256"],
            local_source::observed_hex(&Sha256::digest(bytes))
        );
        assert_eq!(json["manifest"], "D:/same/manifest");
        assert_eq!(json["signatureOut"], "D:/same/signature");
        assert!(signing_request(Path::new("relative"), Path::new("D:/s"), bytes).is_err());
        assert!(signing_request(Path::new("D:/m"), Path::new("D:/s"), b"").is_err());
        assert!(
            signing_request(
                Path::new("D:/m"),
                Path::new("D:/s"),
                &vec![0; 1024 * 1024 + 1]
            )
            .is_err()
        );
    }
}
