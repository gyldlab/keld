//! Authenticate a same-execution local candidate with the existing baseline owner.
//! No key creation/signing, deployment trust construction, installation or launch.
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::PathBuf;

use keld_update::{BaselineVerifier, SelectedBaseline, ValidatedArchive, VerifiedBaseline};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

use super::{BuildError, local_archive::LocalArchiveCandidate};

const MAX_MANIFEST: usize = 1024 * 1024;
const MAX_SIGNATURE: usize = 89; // canonical base64 of64 bytes plus optional LF

/// Authenticated canonical content, not an installed baseline or OS trust receipt.
/// Keep both exact files and the original namespace custody alive. There is no
/// conversion to WindowsBaselineTrust and no initializer/loader call here.
pub(super) struct AuthenticatedLocalBaseline<'images> {
    verified: VerifiedBaseline,
    inventory: ValidatedArchive,
    content_path: PathBuf,
    _content: File,
    _candidate: LocalArchiveCandidate<'images>,
}

impl AuthenticatedLocalBaseline<'_> {
    pub(super) const fn verified(&self) -> &VerifiedBaseline { &self.verified }
    pub(super) const fn inventory(&self) -> &ValidatedArchive { &self.inventory }
    pub(super) fn content_path(&self) -> &std::path::Path { &self.content_path }
}

fn refuse(error: impl std::fmt::Display) -> BuildError {
    BuildError::input("local baseline authentication", error)
}

fn select(
    verifier: &BaselineVerifier,
    produced: &keld_pack::ProducedFull,
    manifest: &[u8],
    signature: &[u8],
) -> Result<SelectedBaseline, BuildError> {
    if manifest.is_empty() || manifest.len() > MAX_MANIFEST || signature.len() > MAX_SIGNATURE {
        return Err(refuse("manifest/signature exceeds local verification bounds"));
    }
    // The sole signature/schema/baseline selection owner authenticates literal
    // bytes. ProducedFull never mints this capability or selects its trust key.
    let selected = verifier.verify_manifest(manifest, signature).map_err(refuse)?;
    if selected.compressed_size() != produced.compressed_size()
        || selected.content_size() != produced.content_size()
        || selected.identity().content_blake3 != *produced.content_blake3()
    {
        return Err(refuse("signed baseline differs from this emitted candidate"));
    }
    Ok(selected)
}

/// `verifier` must come from genuine trusted installer configuration. In
/// particular, do not manufacture Strict identity/profile facts from CLI args,
/// the manifest, an Authenticode signature, or the local archive receipt.
/// Manifest and detached signature are immutable caller-owned byte snapshots.
/// Authentication precedes any new output creation. Errors leave diagnostics;
/// callers must not retry, install or report a successful baseline on failure.
pub(super) fn authenticate<'images>(
    candidate: LocalArchiveCandidate<'images>,
    verifier: &BaselineVerifier,
    manifest: &[u8],
    signature: &[u8],
) -> Result<AuthenticatedLocalBaseline<'images>, BuildError> {
    authenticate_inner(candidate, verifier, manifest, signature, #[cfg(test)] false)
}

fn authenticate_inner<'images>(
    mut candidate: LocalArchiveCandidate<'images>,
    verifier: &BaselineVerifier,
    manifest: &[u8], signature: &[u8],
    #[cfg(test)] corrupt_verified_content: bool,
) -> Result<AuthenticatedLocalBaseline<'images>, BuildError> {
    let selected = select(verifier, candidate.receipt(), manifest, signature)?;
    let root = candidate.path().parent().ok_or_else(|| refuse("candidate parent absent"))?;
    // The candidate owns retained directory guards for this exact private root.
    // Create-new refuses collision; existing files are never overwritten.
    let content_path = root.join("authenticated-baseline.tar.candidate");
    let mut content = OpenOptions::new().read(true).write(true).create_new(true)
        .share_mode(FILE_SHARE_READ).open(&content_path).map_err(refuse)?;
    let verified = {
        let mut compressed = candidate.retained_reader().map_err(refuse)?;
        selected.verify_full(&mut compressed, &mut content).map_err(refuse)?
    };
    content.sync_all().map_err(refuse)?;
    content.seek(SeekFrom::Start(0)).map_err(refuse)?;
    #[cfg(test)]
    if corrupt_verified_content {
        use std::io::Write as _;
        // Fault after genuine full verification, before the actual canonical
        // consumer. This is neither a second parser nor a forged auth receipt.
        content.write_all(&[0xff]).map_err(refuse)?;
        content.seek(SeekFrom::Start(0)).map_err(refuse)?;
    }
    let inventory = verified.validate_windows_archive(&mut content).map_err(refuse)?;
    // Native installation remains a separate operation with real provisioned
    // roots, trusted publisher/profile/volume facts and qualified Runtime.
    Ok(AuthenticatedLocalBaseline {
        verified, inventory, content_path, _content: content, _candidate: candidate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use keld_guard::ProfileDigest;
    use keld_update::{ArtifactIdentity, Channel, DirectInstallationIdentity, DirectInstallMode,
        PrincipalModel, SigningKeyId};

    // Public key of Update's existing [7;32] synthetic test signing fixture.
    // No fixture identity is ever used to install state or construct production trust.
    pub(super) const PUBLIC: [u8; 32] = [234,74,108,99,226,156,82,10,190,245,80,123,19,46,197,249,
        149,71,118,174,190,190,123,146,66,30,234,105,20,70,210,44];
    pub(super) const MANIFEST: &[u8] = include_bytes!("test-only-manifest.json");
    const SIGNATURE: &[u8] = include_bytes!("test-only-manifest.sig");

    pub(super) fn identity() -> DirectInstallationIdentity {
        DirectInstallationIdentity {
            install_mode: DirectInstallMode::PerUserDirect,
            app_id: "dev.keld.fixture".into(), channel: Channel::Stable,
            target: "windows-x64".into(), install_root: PathBuf::from(r"C:\test-only-never-installed"),
            update_root: PathBuf::from(r"C:\test-only-never-installed-state"),
            signing_key_id: SigningKeyId::from_public_key(&PUBLIC),
            baseline: ArtifactIdentity { app_id: "dev.keld.fixture".into(), channel: Channel::Stable,
                target: "windows-x64".into(), version: "1.0.0".into(), content_blake3: [1;32] },
            profile_digest: ProfileDigest([2;32]),
            principal_model: PrincipalModel::StrictDistinctOsPrincipals,
        }
    }
    fn verifier() -> BaselineVerifier {
        BaselineVerifier::new(identity(), PUBLIC).expect("synthetic policy-only verifier")
    }
    fn produced() -> keld_pack::ProducedFull {
        let bytes = b"synthetic helper, never executed";
        let mut input = Cursor::new(bytes);
        let mut entries = [keld_pack::PackageEntry::File {
            name: "keld-updater-helper.exe", size: bytes.len() as u64, input: &mut input,
        }];
        keld_pack::produce_windows_v0(&mut entries, &mut Vec::new()).expect("synthetic emitted package")
    }
    #[test]
    fn genuine_test_signature_does_not_authorize_a_different_produced_candidate() {
        let owner = verifier();
        owner.verify_manifest(MANIFEST, SIGNATURE).expect("positive signature/selection control");
        let error = select(&owner, &produced(), MANIFEST, SIGNATURE).expect_err("candidate differs");
        assert!(error.to_string().contains("signed baseline differs"));
    }
    #[test]
    fn changed_literal_manifest_refuses_before_candidate_consistency() {
        let mut changed = MANIFEST.to_vec(); changed.push(b' ');
        let error = select(&verifier(), &produced(), &changed, SIGNATURE).expect_err("literal bytes changed");
        assert!(error.to_string().contains("signature does not match literal manifest bytes"));
    }
    #[test]
    fn oversized_manifest_refuses_before_authentication() {
        let error = select(&verifier(), &produced(), &vec![0;MAX_MANIFEST+1], SIGNATURE)
            .expect_err("bounded manifest");
        assert!(error.to_string().contains("local verification bounds"));
    }
}


#[cfg(test)]
mod chain_tests {
    use super::*;
    use std::io::{Cursor, Write as _};

    fn package() -> (Vec<u8>, keld_pack::ProducedFull) {
        let bytes = b"synthetic helper, never executed";
        let mut input = Cursor::new(bytes);
        let mut entries = [keld_pack::PackageEntry::File {
            name: "keld-updater-helper.exe", size: bytes.len() as u64, input: &mut input,
        }];
        let mut bytes = Vec::new();
        let receipt = keld_pack::produce_windows_v0(&mut entries, &mut bytes).expect("real Pack fixture");
        (bytes, receipt)
    }

    #[test]
    #[ignore = "explicit native fixture emission, no production signing or trust creation"]
    fn emit_test_only_manifest_for_existing_update_fixture_key() {
        let (_, produced) = package();
        let path = std::env::var_os("KELD_BASELINE_FIXTURE_MANIFEST_OUT").expect("explicit fixture output");
        let mut value: serde_json::Value = serde_json::from_slice(super::tests::MANIFEST).expect("existing fixture schema");
        let full = &mut value["releases"][0]["full"];
        full["size"] = produced.compressed_size().into();
        full["contentSize"] = produced.content_size().into();
        full["blake3"] = super::super::local_source::observed_hex(produced.compressed_blake3()).into();
        full["contentBlake3"] = super::super::local_source::observed_hex(produced.content_blake3()).into();
        let bytes = serde_json::to_vec(&value).expect("fixture literal bytes");
        let mut file = OpenOptions::new().write(true).create_new(true).open(path).expect("fresh fixture output only");
        file.write_all(&bytes).expect("fixture output"); file.sync_all().expect("fixture durability");
        println!("TEST_ONLY_BASELINE_MANIFEST_WRITTEN=1");
    }

    fn inputs() -> (BaselineVerifier, Vec<u8>, Vec<u8>) {
        // Explicit reviewed synthetic fixture files only. No live credentials.
        let manifest = std::fs::read(std::env::var_os("KELD_BASELINE_TEST_MANIFEST").expect("signed matching test fixture")).expect("test manifest");
        let signature = std::fs::read(std::env::var_os("KELD_BASELINE_TEST_SIGNATURE").expect("test signature")).expect("test signature bytes");
        let (_, produced) = package();
        let mut identity = super::tests::identity();
        identity.baseline.content_blake3 = *produced.content_blake3();
        let verifier = BaselineVerifier::new(identity, super::tests::PUBLIC).expect("existing public synthetic fixture key");
        (verifier, manifest, signature)
    }

    fn candidate(parent: &std::path::Path, name: &str, truncate: bool) -> LocalArchiveCandidate<'static> {
        let (mut bytes, produced) = package();
        if truncate { bytes.pop().expect("nonempty compressed fixture"); }
        LocalArchiveCandidate::synthetic_candidate(&parent.join(name), &bytes, produced).expect("synthetic candidate custody")
    }

    #[test]
    #[ignore = "requires reviewed matching signed synthetic manifest; explicit native qualification"]
    fn actual_authentication_chain_has_positive_and_stream_canonical_falsifiers() {
        let parent = tempfile::tempdir().expect("owned fixture parent");
        let (verifier, manifest, signature) = inputs();
        let positive = authenticate(candidate(parent.path(), "positive", false), &verifier, &manifest, &signature).expect("real full+canonical chain");
        assert_eq!(positive.verified().content_size(), positive.inventory().content_size());
        assert!(positive.inventory().entries().iter().any(|entry| entry.name() == "keld-updater-helper.exe"));
        assert!(positive.content_path().starts_with(parent.path().join("positive")));
        drop(positive);
        let Err(short) = authenticate(candidate(parent.path(), "short", true), &verifier, &manifest, &signature) else { panic!("short compressed bytes admitted"); };
        assert!(short.to_string().contains("KELD-UPDATE-008: compressed artifact length mismatch"));
        let Err(changed) = authenticate_inner(candidate(parent.path(), "changed", false), &verifier, &manifest, &signature, true) else { panic!("changed verified content bypassed canonical validation"); };
        assert!(changed.to_string().contains("KELD-UPDATE-011"));
        let collision = candidate(parent.path(), "collision", false);
        std::fs::write(parent.path().join("collision/authenticated-baseline.tar.candidate"), b"occupied").expect("owned collision");
        assert!(authenticate(collision, &verifier, &manifest, &signature).is_err());
        assert_eq!(std::fs::read(parent.path().join("collision/authenticated-baseline.tar.candidate")).unwrap(), b"occupied");
    }
    #[test]
    #[ignore = "requires reviewed signed synthetic Pack fixture; never installs state"]
    fn installer_source_seal_preserves_custody_and_revalidates_bytes() {
        let parent = tempfile::tempdir().expect("owned fixture parent");
        let (verifier, manifest, signature) = inputs();
        let baseline = authenticate(candidate(parent.path(), "sealed", false), &verifier, &manifest, &signature)
            .expect("genuine baseline authentication");
        let path = baseline.content_path().to_path_buf();
        let before = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(&path)
            .expect_err("writable canonical sink conflicts with installer read sharing");
        assert_eq!(before.raw_os_error(), Some(32));
        let sealed = baseline.seal_for_installer().expect("read-only handoff");
        let mut reader = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(&path)
            .expect("existing installer sharing can coexist with sealed owner");
        sealed.verified().validate_windows_archive(&mut reader).expect("same authenticated content");
        let denied = OpenOptions::new().write(true).open(&path).expect_err("sealed writer denied");
        assert_eq!(denied.raw_os_error(), Some(32));
        let denied = std::fs::rename(&path, path.with_extension("moved")).expect_err("sealed rename denied");
        assert_eq!(denied.raw_os_error(), Some(32));
        drop(reader);
        drop(sealed);
        let writable = OpenOptions::new().write(true).open(&path).expect("write access restored after all pins drop");
        drop(writable);

        let altered = authenticate(candidate(parent.path(), "changed-at-close", false), &verifier, &manifest, &signature)
            .expect("genuine authentication before race injection");
        let result = altered.seal_for_installer_inner(|path| {
            let mut writer = OpenOptions::new().write(true).open(path)?;
            writer.write_all(&[0xff])?;
            writer.sync_all()
        });
        let Err(error) = result else { panic!("changed content admitted at handoff"); };
        assert!(error.to_string().contains("KELD-UPDATE-011"));
        let changed_payload = authenticate(candidate(parent.path(), "payload-at-close", false), &verifier, &manifest, &signature)
            .expect("genuine authentication before payload injection");
        let result = changed_payload.seal_for_installer_inner(|path| {
            let bytes = std::fs::read(path)?;
            let marker = b"synthetic helper, never executed";
            let matches: Vec<_> = bytes.windows(marker.len()).enumerate()
                .filter_map(|(index, value)| (value == marker).then_some(index)).collect();
            assert_eq!(matches.len(), 1, "exact known fixture payload marker");
            let offset = u64::try_from(matches[0]).expect("fixture offset fits u64");
            let mut writer = OpenOptions::new().write(true).open(path)?;
            writer.seek(SeekFrom::Start(offset))?;
            writer.write_all(b"S")?;
            writer.sync_all()
        });
        let Err(error) = result else { panic!("changed canonical payload admitted at handoff"); };
        assert!(error.to_string().contains("KELD-UPDATE-009: decompressed content BLAKE3 mismatch"));
        parent.close().expect("remove owned fixture after custody drops");
    }
}

impl AuthenticatedLocalBaseline<'_> {
    /// Converts the canonical sink into a read-only installation source.
    ///
    /// The directory owner survives the close/reopen boundary. This does not
    /// assume the reopened file is the earlier object: the actual Update owner
    /// revalidates its complete authenticated canonical bytes before return.
    /// No installation, scaffold provisioning, or trust construction occurs.
    pub(super) fn seal_for_installer(self) -> Result<Self, BuildError> {
        self.seal_for_installer_inner(#[cfg(test)] |_| Ok(()))
    }

    fn seal_for_installer_inner(
        self,
        #[cfg(test)] after_close: impl FnOnce(&std::path::Path) -> std::io::Result<()>,
    ) -> Result<Self, BuildError> {
        use std::os::windows::fs::MetadataExt as _;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        let Self { verified, inventory: _, content_path, _content: writable_content, _candidate: candidate } = self;
        // A surviving writable handle conflicts with Update's read-sharing
        // source open. Release only this sink, never the namespace owner.
        drop(writable_content);
        #[cfg(test)]
        after_close(&content_path).map_err(refuse)?;
        let mut content = OpenOptions::new().read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&content_path).map_err(refuse)?;
        let metadata = content.metadata().map_err(refuse)?;
        if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(refuse("installation source is not a regular non-reparse file"));
        }
        let inventory = verified.validate_windows_archive(&mut content).map_err(refuse)?;
        content.seek(SeekFrom::Start(0)).map_err(refuse)?;
        Ok(Self { verified, inventory, content_path, _content: content, _candidate: candidate })
    }
}
