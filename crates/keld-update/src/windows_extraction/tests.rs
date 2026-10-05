//! Real Windows filesystem acceptance for the T3b extraction boundary.

#![allow(unsafe_code)] // test-only Win32 mapping fixtures with local proofs
#![deny(unsafe_op_in_unsafe_fn)]

use std::fs::{self, File, OpenOptions};
use std::io::Cursor;
use std::os::windows::fs::{OpenOptionsExt as _, symlink_dir, symlink_file};
use std::os::windows::io::AsRawHandle as _;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use cap_std::ambient_authority;
use ed25519_dalek::{Signer as _, SigningKey};
use keld_guard::{
    ProfileDigest, validate_windows_owner_private_directory, validate_windows_owner_private_file,
};
use tempfile::TempDir;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::Memory::{CreateFileMappingW, PAGE_READONLY, PAGE_READWRITE};

use super::*;
use crate::tests::{
    append_required_policy, append_ustar_entry, digest_hex, expected_identity, finish_ustar,
    manifest_json, observation, release_json, signing_key,
};
use crate::{
    DirectInstallMode, DirectInstallationIdentity, InstallOwner, ManifestDecision, ProvenanceField,
    SigningKeyId, UpdateVerifier,
};

// This fixture was pinned independently with Python stdlib tarfile USTAR_FORMAT,
// including exact policy, complete parent directory entries and two terminal blocks.
const GOLDEN_TAR: &[u8] =
    include_bytes!("../../../keld-pack/tests/fixtures/windows-v0-content.tar");

struct Fixture {
    temp: TempDir,
    identity: DirectInstallationIdentity,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("isolated extraction fixture");
        let mut identity = expected_identity();
        identity.install_mode = DirectInstallMode::PerUserDirect;
        identity.install_root = temp.path().join("install");
        identity.update_root = identity.install_root.join("updates");
        let parent = cap_std::fs::Dir::open_ambient_dir(temp.path(), ambient_authority())
            .expect("retained fixture parent")
            .into_std_file();
        let install = crate::windows_fs::create_directory_relative(&parent, "install")
            .expect("guard-protected install root");
        let update = crate::windows_fs::create_directory_relative(&install, "updates")
            .expect("guard-protected update root");
        let versions = crate::windows_fs::create_directory_relative(&update, "versions")
            .expect("guard-protected versions root");
        validate_windows_owner_private_directory(&update).expect("exact update ACL");
        validate_windows_owner_private_directory(&versions).expect("exact versions ACL");
        Self { temp, identity }
    }

    fn versions(&self) -> PathBuf {
        self.identity.update_root.join("versions")
    }

    fn source(&self, bytes: &[u8]) -> PathBuf {
        let path = self.temp.path().join("source.tar");
        fs::write(&path, bytes).expect("write source fixture");
        path
    }

    fn admitted(&self, floor: &str) -> crate::AdmittedInstallation {
        admitted_for(self.identity.clone(), &signing_key(), floor)
    }

    fn receipt(&self, content: &[u8]) -> crate::VerifiedFull {
        authenticated_receipt(self.identity.clone(), &signing_key(), "1.0.0", content)
    }
}

#[test]
fn logical_uac_admission_cannot_open_a_protected_stage_without_writer_authority() {
    let fixture = Fixture::new();
    let mut identity = fixture.identity.clone();
    identity.install_mode = DirectInstallMode::MachineUacDirect;
    let admitted = admitted_for(identity, &signing_key(), "1.0.0");
    let error = admitted
        .open_windows_extraction_root()
        .expect_err("a logical observation is not an authenticated UAC writer capability");
    assert!(
        error.to_string().contains("exclusive writer lease"),
        "{error}"
    );
    assert_eq!(
        fs::read_dir(fixture.versions())
            .expect("per-user fixture versions remain present")
            .count(),
        0,
        "refusal precedes any protected stage creation"
    );
}

#[test]
fn owner_private_root_refuses_install_profile_and_topology_substitution() {
    let fixture = Fixture::new();
    let install = fixture.temp.path().join("unprotected-install");
    let update = install.join("updates");
    fs::create_dir(&install).expect("ordinary inherited install fixture");
    fs::create_dir(&update).expect("ordinary inherited update fixture");
    fs::create_dir(update.join("versions")).expect("ordinary inherited versions fixture");

    let mut identity = fixture.identity.clone();
    identity.install_root = install;
    identity.update_root = update.clone();
    let admitted = admitted_for(identity.clone(), &signing_key(), "1.0.0");
    assert!(
        admitted.open_windows_extraction_root().is_err(),
        "matching topology cannot compensate for a missing owner-private install profile"
    );

    identity.update_root = fixture.temp.path().join("sibling-updates");
    let admitted = admitted_for(identity, &signing_key(), "1.0.0");
    assert!(
        admitted.open_windows_extraction_root().is_err(),
        "a sibling update root cannot substitute for the trusted direct child"
    );
}

#[test]
fn owner_private_stage_cannot_publish_a_version_without_the_writer_lease() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = authenticated_receipt(
        fixture.identity.clone(),
        &signing_key(),
        "1.0.0",
        GOLDEN_TAR,
    );
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("owner-private preflight root");
    let stage = root
        .extract(&receipt, &source)
        .expect("verified incomplete stage");
    let stage_name = stage.name().to_owned();
    let error = stage
        .complete()
        .expect_err("owner-private staging is not the activation writer");
    assert_eq!(error.code(), "KELD-UPDATE-015");
    assert!(matches!(
        &error,
        UpdateError::VersionPublication {
            version,
            stage_name: observed_stage,
            outcome: VersionPublicationOutcome::StageRetained,
            ..
        } if version == "2.0.0" && observed_stage == &stage_name
    ));
    let stage_path = fixture.versions().join(stage_name);
    assert!(stage_path.is_dir(), "the diagnostic stage is retained");
    assert!(
        !stage_path.join(".complete").exists(),
        "refusal precedes the durable completion marker"
    );
    assert!(
        !fixture.versions().join("2.0.0").exists(),
        "no final version directory was published"
    );
    assert_eq!(
        fs::read_dir(&fixture.identity.update_root)
            .expect("update root remains available")
            .count(),
        1,
        "no floor, pointer, lease, journal or provenance was created"
    );
}

#[test]
#[ignore = "operator runs from the elevated installing administrator token"]
fn elevated_uac_creator_applies_profile_before_payload_write() {
    assert!(
        keld_guard::require_windows_system_token().is_err(),
        "Machine-UAC stage qualification uses an elevated administrator, not SYSTEM"
    );
    let fixture = Fixture::new();
    let mut identity = fixture.identity.clone();
    identity.install_mode = DirectInstallMode::MachineUacDirect;
    let receipt = authenticated_receipt(identity, &signing_key(), "1.1.0", GOLDEN_TAR);
    let source_path = fixture.source(GOLDEN_TAR);
    let mut source = open_source(&source_path).expect("lock ordinary-user package cache input");
    let validated = receipt
        .validate_windows_archive(&mut source)
        .expect("elevated helper revalidates exact signed package bytes");
    let parent = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(fixture.temp.path())
        .expect("retain user cache parent");
    let stage = crate::windows_fs::create_directory_relative_with_profile(
        &parent,
        "machine-uac-stage",
        keld_guard::WindowsInstallProtectionProfile::MachineUac,
    )
    .expect("create Administrators/SYSTEM-owned stage before payload");
    keld_guard::validate_windows_admin_machine_directory(&stage)
        .expect("stage root has exact profile before copy");
    let mut before_payload_write = 0_usize;
    let (directories, files) = populate_stage(
        stage,
        "machine-uac-stage",
        &validated,
        &mut source,
        StageProtection::MachineUac,
        &mut |event, diagnostic| {
            if event == ExtractionEvent::BeforePayloadWrite {
                let relative = if diagnostic == "content.tar" {
                    PathBuf::from("content.tar")
                } else {
                    PathBuf::from("tree").join(diagnostic)
                };
                let path = fixture.temp.path().join("machine-uac-stage").join(relative);
                let file = File::open(path).expect("destination exists before its flush");
                keld_guard::validate_windows_admin_machine_file(&file).expect(
                    "exact Machine-UAC file profile is present before the first payload write",
                );
                assert_eq!(
                    file.metadata().expect("inspect new destination").len(),
                    0,
                    "before-write observation must precede every payload byte"
                );
                before_payload_write += 1;
            }
            Ok(())
        },
    )
    .expect("shared extraction/readback succeeds under Machine-UAC profile");
    assert_eq!(
        before_payload_write,
        files.len(),
        "every destination file must pass the pre-write control"
    );
    for directory in &directories {
        keld_guard::validate_windows_admin_machine_directory(
            &directory
                .try_clone()
                .expect("directory handle")
                .into_std_file(),
        )
        .expect("every output directory has Machine-UAC profile");
    }
    for file in &files {
        keld_guard::validate_windows_admin_machine_file(
            &file.try_clone().expect("file handle").into_std(),
        )
        .expect("every output file has Machine-UAC profile");
    }
    assert_eq!(
        fs::read(fixture.temp.path().join("machine-uac-stage/content.tar"))
            .expect("read copied package"),
        GOLDEN_TAR
    );
}

fn admitted_for(
    identity: DirectInstallationIdentity,
    key: &SigningKey,
    floor: &str,
) -> crate::AdmittedInstallation {
    UpdateVerifier::new(identity.clone(), key.verifying_key().to_bytes())
        .expect("fixture verifier")
        .admit(&observation(identity, InstallOwner::Direct, Some(floor)))
        .expect("authenticated fixture admission")
}

fn authenticated_receipt(
    identity: DirectInstallationIdentity,
    key: &SigningKey,
    floor: &str,
    content: &[u8],
) -> crate::VerifiedFull {
    let admitted = admitted_for(identity, key, floor);
    let compressed = zstd::stream::encode_all(Cursor::new(content), 0).expect("fixture zstd");
    let release = release_json(
        "2.0.0",
        &compressed.len().to_string(),
        &digest_hex(&compressed),
        &content.len().to_string(),
        &digest_hex(content),
        "",
    );
    let manifest = manifest_json(&release);
    let mut signature = [0_u8; 88];
    let signature_len = BASE64_STANDARD
        .encode_slice(key.sign(&manifest).to_bytes(), &mut signature)
        .expect("88 bytes hold a 64-byte Ed25519 signature in base64");
    let selected = match admitted
        .verify_manifest(&manifest, &signature[..signature_len])
        .expect("signed fixture manifest")
    {
        ManifestDecision::Update(selected) => selected,
        ManifestDecision::NoUpdate => panic!("candidate must be above fixture floor"),
    };
    selected
        .verify_full(&mut Cursor::new(compressed), &mut Vec::new())
        .expect("signed full receipt")
}

#[test]
fn machine_copy_refuses_an_ordinary_process_before_readback() {
    assert!(
        keld_guard::require_windows_system_token().is_err(),
        "ordinary gate requires a non-SYSTEM process"
    );
    let fixture = Fixture::new();
    let source_path = fixture.source(b"payload");
    let mut source = open_source(&source_path).expect("locked source");
    let parent = cap_std::fs::Dir::open_ambient_dir(fixture.versions(), ambient_authority())
        .expect("private fixture parent");
    let mut readback = false;
    let result = copy_read_back(
        &parent,
        "payload",
        "payload",
        &mut source,
        CopyRange { offset: 0, size: 7 },
        StageProtection::Machine,
        &mut |event, _| {
            readback |= event == ExtractionEvent::BeforeReadback;
            Ok(())
        },
    );
    assert!(
        result.is_err(),
        "ordinary process cannot seal a machine payload"
    );
    assert!(!readback, "authority refusal cannot produce final readback");
    assert!(
        !fixture.versions().join("payload").exists(),
        "machine-copy authority must precede object creation"
    );
    assert!(!fixture.versions().join(".complete").exists());
}

#[test]
#[ignore = "requires the reviewed operator helper to run this exact selector as LocalSystem"]
fn system_copy_seals_before_final_writer_flush() {
    keld_guard::require_windows_system_token().expect("actual SYSTEM copy qualification");
    let fixture = Fixture::new();
    let source_path = fixture.source(b"payload");
    let mut source = open_source(&source_path).expect("locked source");
    let parent = cap_std::fs::Dir::open_ambient_dir(fixture.versions(), ambient_authority())
        .expect("private fixture parent");
    let mut observed = Vec::new();
    let stopped = copy_read_back(
        &parent,
        "stopped",
        "stopped",
        &mut source,
        CopyRange { offset: 0, size: 7 },
        StageProtection::Machine,
        &mut |event, name| {
            observed.push(event);
            if event == ExtractionEvent::BeforeFileFlush {
                let object = File::open(fixture.versions().join(name))
                    .expect("inspect original-writer object");
                keld_guard::validate_windows_machine_file(&object)
                    .expect("actual final DACL already applied before final flush");
                return Err(io::Error::other("stop before final writable-handle flush"));
            }
            Ok(())
        },
    );
    assert!(
        stopped.is_err(),
        "pre-flush failure cannot return a receipt"
    );
    assert_eq!(observed, [ExtractionEvent::BeforeFileFlush]);
    assert!(!fixture.versions().join(".complete").exists());

    observed.clear();
    let (retained, _) = copy_read_back(
        &parent,
        "complete-copy",
        "complete-copy",
        &mut source,
        CopyRange { offset: 0, size: 7 },
        StageProtection::Machine,
        &mut |event, name| {
            observed.push(event);
            if event == ExtractionEvent::BeforeFileFlush {
                let object = File::open(fixture.versions().join(name))
                    .expect("inspect original-writer object");
                keld_guard::validate_windows_machine_file(&object)
                    .expect("final DACL precedes flush");
            }
            Ok(())
        },
    )
    .expect("sealed, flushed and read-back machine copy");
    assert_eq!(
        observed,
        [
            ExtractionEvent::BeforeFileFlush,
            ExtractionEvent::BeforeReadback
        ]
    );
    keld_guard::validate_windows_machine_file(&retained.into_std())
        .expect("final readback retains machine protection");
    assert_eq!(
        fs::read(fixture.versions().join("complete-copy")).expect("read-back copy bytes"),
        b"payload"
    );
    println!("KELD_KEL266_COPY_SEAL_FLUSH_PASSED");
}

fn assert_empty_versions(versions: &Path) {
    assert_eq!(
        fs::read_dir(versions).expect("read versions").count(),
        0,
        "refusal before stage creation must leave versions empty"
    );
}

#[test]
fn signed_golden_extracts_only_incomplete_stage_with_exact_bytes() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("exact protected root");
    let stage = root
        .extract(&receipt, &source)
        .expect("extract signed golden");
    assert_eq!(stage.identity().version, "2.0.0");
    let stage_path = fixture.versions().join(stage.name());
    assert_eq!(
        fs::read(stage_path.join("content.tar")).expect("retained tar"),
        GOLDEN_TAR
    );
    assert_eq!(
        fs::read(stage_path.join("tree/nest/one")).expect("one-byte file"),
        b"!"
    );
    let multi: Vec<u8> = (0u8..=255).cycle().take(1024).chain(*b"Z").collect();
    assert_eq!(
        fs::read(stage_path.join("tree/nest/multi")).expect("multiblock file"),
        multi
    );
    assert_eq!(
        fs::read(stage_path.join("tree/.keld/update-policy.v1")).expect("authenticated policy"),
        b"{\"schema\":1,\"dataMigration\":\"none\"}\n"
    );
    assert!(stage_path.join("tree/empty").is_dir());
    assert!(!stage_path.join(".complete").exists());
    assert!(!fixture.versions().join("2.0.0").exists());
}

#[test]
fn tilde_in_existing_update_root_preserves_authenticated_extraction() {
    let mut fixture = Fixture::new();
    let renamed = fixture.identity.install_root.join("updates~stable");
    fs::rename(&fixture.identity.update_root, &renamed)
        .expect("rename preserves protected directory descriptors");
    fixture.identity.update_root = renamed;
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("existing filesystem roots permit a literal tilde");
    let stage = root
        .extract(&receipt, &source)
        .expect("authenticated extraction beneath existing tilde root");
    let stage_path = fixture.versions().join(stage.name());
    assert_eq!(
        fs::read(stage_path.join("content.tar")).expect("exact retained content"),
        GOLDEN_TAR
    );
    assert_eq!(
        fs::read(stage_path.join("tree/nest/one")).expect("extracted payload"),
        b"!"
    );
    assert!(!stage_path.join(".complete").exists());
}

#[test]
fn tilde_archive_member_still_refuses_before_stage_creation() {
    let fixture = Fixture::new();
    let mut content = Vec::new();
    append_required_policy(&mut content);
    append_ustar_entry(&mut content, "payload~1.bin", b'0', b"forbidden alias");
    finish_ustar(&mut content);
    let source = fixture.source(&content);
    let receipt = fixture.receipt(&content);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let error = root
        .extract(&receipt, &source)
        .expect_err("package tilde prohibition remains in force");
    assert_eq!(error.code(), "KELD-UPDATE-011");
    assert!(matches!(error, UpdateError::ArchiveInvalid { .. }));
    assert_empty_versions(&fixture.versions());
}

#[test]
fn different_authenticated_contexts_and_floor_refuse_before_stage_creation() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");

    let mut cases: Vec<(DirectInstallationIdentity, SigningKey, ProvenanceField)> = Vec::new();
    let mut changed = fixture.identity.clone();
    changed.install_root = fixture.temp.path().join("other-install");
    cases.push((changed, signing_key(), ProvenanceField::InstallRoot));
    let mut changed = fixture.identity.clone();
    changed.update_root = fixture.temp.path().join("other-updates");
    cases.push((changed, signing_key(), ProvenanceField::UpdateRoot));
    let mut changed = fixture.identity.clone();
    changed.baseline.content_blake3[0] ^= 1;
    cases.push((changed, signing_key(), ProvenanceField::Baseline));
    let mut changed = fixture.identity.clone();
    changed.profile_digest = ProfileDigest([3_u8; 32]);
    cases.push((changed, signing_key(), ProvenanceField::Profile));
    let other_key = SigningKey::from_bytes(&[8_u8; 32]);
    let mut changed = fixture.identity.clone();
    changed.signing_key_id = SigningKeyId::from_public_key(&other_key.verifying_key().to_bytes());
    cases.push((changed, other_key, ProvenanceField::SigningKey));

    for (identity, key, expected_field) in cases {
        let receipt = authenticated_receipt(identity, &key, "1.0.0", GOLDEN_TAR);
        let error = root
            .extract(&receipt, &source)
            .expect_err("foreign admitted context extracted");
        assert!(
            matches!(&error, UpdateError::ProvenanceMismatch { field, .. } if *field == expected_field),
            "foreign {expected_field:?}: {error:?}"
        );
        assert_empty_versions(&fixture.versions());
    }

    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut higher_floor_root = fixture
        .admitted("2.0.0")
        .open_windows_extraction_root()
        .expect("protected root with higher floor");
    let error = higher_floor_root
        .extract(&receipt, &source)
        .expect_err("candidate at floor extracted");
    assert_eq!(error.code(), "KELD-UPDATE-003");
    assert!(matches!(
        error,
        UpdateError::ProvenanceMismatch {
            field: ProvenanceField::VersionFloor,
            ..
        }
    ));
    assert_empty_versions(&fixture.versions());
}

fn expect_extraction(error: UpdateError, expected_incomplete: bool) -> Option<String> {
    assert_eq!(error.code(), "KELD-UPDATE-012", "{error:?}");
    match error {
        UpdateError::Extraction {
            incomplete_stage,
            step,
            ..
        } => {
            assert_eq!(incomplete_stage.is_some(), expected_incomplete, "{step}");
            incomplete_stage
        }
        other => panic!("expected typed extraction failure, got {other:?}"),
    }
}

#[test]
fn missing_or_noncanonical_protected_root_refuses_without_creating_stage() {
    let missing = Fixture::new();
    fs::remove_dir(missing.versions()).expect("remove unused versions root");
    let error = missing
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("missing versions accepted");
    expect_extraction(error, false);

    let permissive = Fixture::new();
    fs::remove_dir(permissive.versions()).expect("remove protected versions root");
    fs::create_dir(permissive.versions()).expect("ordinary inherited versions ACL");
    let error = permissive
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("permissive versions accepted");
    expect_extraction(error, false);
    assert_empty_versions(&permissive.versions());

    let permissive = Fixture::new();
    fs::remove_dir(permissive.versions()).expect("remove protected versions root");
    fs::remove_dir(&permissive.identity.update_root).expect("remove protected update root");
    fs::create_dir(&permissive.identity.update_root).expect("ordinary inherited update ACL");
    fs::create_dir(permissive.versions()).expect("ordinary inherited versions ACL");
    let error = permissive
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("permissive update root accepted");
    expect_extraction(error, false);
    assert_empty_versions(&permissive.versions());
}

#[test]
fn reparse_update_or_versions_root_refuses_before_stage_creation() {
    let versions_link = Fixture::new();
    let target = versions_link.temp.path().join("other-versions");
    fs::create_dir(&target).expect("reparse target");
    fs::remove_dir(versions_link.versions()).expect("replace versions with link");
    symlink_dir(&target, versions_link.versions())
        .expect("real Windows directory reparse fixture requires symlink privilege");
    let error = versions_link
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("reparse versions root accepted");
    expect_extraction(error, false);
    assert_empty_versions(&target);

    let update_link = Fixture::new();
    let target = update_link.temp.path().join("other-update");
    fs::create_dir(&target).expect("reparse target");
    fs::remove_dir(update_link.versions()).expect("empty protected update root");
    fs::remove_dir(&update_link.identity.update_root).expect("replace update with link");
    symlink_dir(&target, &update_link.identity.update_root)
        .expect("real Windows directory reparse fixture requires symlink privilege");
    let error = update_link
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect_err("reparse update root accepted");
    expect_extraction(error, false);
    assert_empty_versions(&target);
}

#[test]
fn locked_and_hardlinked_sources_refuse_before_stage_creation() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");

    let lock = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&source)
        .expect("exclusive source lock");
    let error = root
        .extract(&receipt, &source)
        .expect_err("locked source extracted");
    expect_extraction(error, false);
    assert_empty_versions(&fixture.versions());
    drop(lock);

    let alias = fixture.temp.path().join("source-link.tar");
    fs::hard_link(&source, &alias).expect("create real NTFS hardlink");
    let error = root
        .extract(&receipt, &source)
        .expect_err("hardlinked source extracted");
    expect_extraction(error, false);
    assert_empty_versions(&fixture.versions());
}

#[test]
fn reparse_source_refuses_before_stage_creation() {
    let fixture = Fixture::new();
    let target = fixture.source(GOLDEN_TAR);
    let link = fixture.temp.path().join("source-link-symlink.tar");
    symlink_file(&target, &link)
        .expect("real Windows source reparse fixture requires symlink privilege");
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let error = root
        .extract(&receipt, &link)
        .expect_err("reparse source extracted");
    expect_extraction(error, false);
    assert_empty_versions(&fixture.versions());
}

struct Mapping(windows_sys::Win32::Foundation::HANDLE);

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: this owns exactly one successful CreateFileMappingW handle.
        unsafe { CloseHandle(self.0) };
    }
}

fn source_mapping(path: &Path, writable: bool) -> Mapping {
    let file = OpenOptions::new()
        .read(true)
        .write(writable)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(path)
        .expect("mapping source handle");
    // SAFETY: `file` is a live regular-file handle, the optional pointers are
    // null by contract, and the nonzero source size supplies the mapping length.
    let handle = unsafe {
        CreateFileMappingW(
            file.as_raw_handle().cast(),
            std::ptr::null(),
            if writable {
                PAGE_READWRITE
            } else {
                PAGE_READONLY
            },
            0,
            0,
            std::ptr::null(),
        )
    };
    assert!(
        !handle.is_null(),
        "CreateFileMappingW: {}",
        std::io::Error::last_os_error()
    );
    drop(file); // The mapping, not the original file handle, is the negative control.
    Mapping(handle)
}

#[test]
fn writable_mapping_blocks_source_admission_but_readonly_mapping_is_valid() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let writable = source_mapping(&source, true);
    let error = root
        .extract(&receipt, &source)
        .expect_err("writable mapped source extracted");
    expect_extraction(error, false);
    assert_empty_versions(&fixture.versions());
    drop(writable);

    let readonly = source_mapping(&source, false);
    let stage = root
        .extract(&receipt, &source)
        .expect("readonly mapping is safe");
    assert_eq!(
        fs::read(fixture.versions().join(stage.name()).join("content.tar"))
            .expect("protected content"),
        GOLDEN_TAR
    );
    drop(stage);
    drop(readonly);
}

#[test]
fn retained_source_blocks_writer_after_complete_preflight() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut observed = false;
    let stage = root
        .extract_inner(&receipt, &source, |event, _| {
            if event == ExtractionEvent::PreCreate {
                observed = true;
                assert!(
                    OpenOptions::new().write(true).open(&source).is_err(),
                    "retained source must exclude a concurrent writer"
                );
            }
            Ok(())
        })
        .expect("source remains valid after denied writer");
    assert!(observed, "hook ran after complete preflight");
    assert_eq!(
        fs::read(fixture.versions().join(stage.name()).join("content.tar"))
            .expect("protected content"),
        GOLDEN_TAR
    );
}

#[test]
fn stage_fault_is_named_and_incomplete_without_publication() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut created_name = String::new();
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                created_name = name.to_owned();
                return Err(std::io::Error::other("injected after stage creation"));
            }
            Ok(())
        })
        .expect_err("injected failure returned a stage");
    assert!(!created_name.is_empty());
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(created_name.as_str())
    );
    let stage_path = fixture.versions().join(&created_name);
    assert!(
        stage_path.is_dir(),
        "incomplete stage remains diagnostic state"
    );
    assert!(!stage_path.join(".complete").exists());
    assert!(!fixture.versions().join("2.0.0").exists());
}

#[test]
fn precreate_stage_name_collision_refuses_without_reusing_existing_object() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut colliding_name = String::new();
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::PreCreate {
                colliding_name = name.to_owned();
                fs::create_dir(fixture.versions().join(name))?;
            }
            Ok(())
        })
        .expect_err("colliding stage reused");
    assert!(!colliding_name.is_empty());
    assert!(matches!(
        &error,
        UpdateError::Extraction {
            incomplete_stage: Some(name),
            step: "stage creation (outcome unconfirmed)",
            ..
        } if name == &colliding_name
    ));
    assert_eq!(error.code(), "KELD-UPDATE-012");
    let collision = fixture.versions().join(colliding_name);
    assert!(collision.is_dir());
    assert_eq!(
        fs::read_dir(&collision)
            .expect("collision untouched")
            .count(),
        0
    );
}

#[test]
fn existing_child_collision_refuses_before_writing_that_member() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut planted = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeMember && name == "nest" {
                fs::write(
                    fixture.versions().join(&stage_name).join("tree/nest"),
                    b"collision",
                )?;
                planted = true;
            }
            Ok(())
        })
        .expect_err("existing file ancestor accepted");
    assert!(planted, "hook reached the contested directory");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    let stage_path = fixture.versions().join(&stage_name);
    assert_eq!(
        fs::read(stage_path.join("tree/nest")).expect("collision survives"),
        b"collision"
    );
    assert!(!stage_path.join("tree/nest/one").exists());
    assert!(!stage_path.join(".complete").exists());
}

#[test]
fn preplanted_leaf_reparse_cannot_redirect_extracted_file() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let outside = fixture.temp.path().join("outside-marker.txt");
    fs::write(&outside, b"outside original").expect("outside sentinel");
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut planted = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeMember && name == "nest/one" {
                let leaf = fixture.versions().join(&stage_name).join("tree/nest/one");
                symlink_file(&outside, leaf)?;
                planted = true;
            }
            Ok(())
        })
        .expect_err("leaf reparse redirected extraction");
    assert!(planted, "fixture planted a real leaf reparse");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    assert_eq!(
        fs::read(&outside).expect("outside sentinel remains"),
        b"outside original"
    );
    assert!(
        !fixture
            .versions()
            .join(stage_name)
            .join(".complete")
            .exists()
    );
}

#[test]
fn preplanted_ancestor_reparse_cannot_redirect_extracted_tree() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let outside = fixture.temp.path().join("outside-dir");
    fs::create_dir(&outside).expect("outside directory");
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut planted = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeMember && name == "nest" {
                let ancestor = fixture.versions().join(&stage_name).join("tree/nest");
                symlink_dir(&outside, ancestor)?;
                planted = true;
            }
            Ok(())
        })
        .expect_err("ancestor reparse redirected extraction");
    assert!(planted, "fixture planted a real directory reparse");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    assert_eq!(
        fs::read_dir(&outside)
            .expect("outside stays readable")
            .count(),
        0
    );
    assert!(
        !fixture
            .versions()
            .join(stage_name)
            .join(".complete")
            .exists()
    );
}

#[test]
fn changed_retained_tar_is_rejected_on_readback() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut tampered = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeReadback && name == "content.tar" {
                fs::write(
                    fixture.versions().join(&stage_name).join("content.tar"),
                    b"changed",
                )?;
                tampered = true;
            }
            Ok(())
        })
        .expect_err("changed retained tar accepted");
    assert!(tampered, "hook reached content.tar after writer close");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    assert_eq!(fs::read(source).expect("source retained"), GOLDEN_TAR);
    assert!(
        !fixture
            .versions()
            .join(stage_name)
            .join(".complete")
            .exists()
    );
}

#[test]
fn same_length_readback_tamper_fails_the_digest_predicate() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let mut stage_name = String::new();
    let mut tampered = false;
    let error = root
        .extract_inner(&receipt, &source, |event, name| {
            if event == ExtractionEvent::AfterStageCreate {
                stage_name = name.to_owned();
            }
            if event == ExtractionEvent::BeforeReadback && name == "content.tar" {
                let mut changed = GOLDEN_TAR.to_vec();
                changed[0] ^= 1;
                fs::write(
                    fixture.versions().join(&stage_name).join("content.tar"),
                    &changed,
                )?;
                tampered = true;
            }
            Ok(())
        })
        .expect_err("same-length readback tamper accepted");
    assert!(tampered, "hook reached content.tar after writer close");
    assert_eq!(
        expect_extraction(error, true).as_deref(),
        Some(stage_name.as_str())
    );
    let retained = fixture.versions().join(&stage_name).join("content.tar");
    assert_eq!(
        fs::metadata(&retained).expect("same object remains").len(),
        GOLDEN_TAR.len() as u64
    );
    assert_eq!(
        fs::read(&retained).expect("tampered stage copy")[0],
        GOLDEN_TAR[0] ^ 1
    );
    assert_eq!(
        fs::read(source).expect("authenticated source intact"),
        GOLDEN_TAR
    );
    assert!(
        !fixture
            .versions()
            .join(stage_name)
            .join(".complete")
            .exists()
    );
}

#[test]
fn retained_directory_handles_pin_stage_until_receipt_is_dropped() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let stage = root
        .extract(&receipt, &source)
        .expect("extract signed golden");
    let stage_path = fixture.versions().join(stage.name());
    let renamed = fixture.versions().join("after-release");
    assert!(
        fs::rename(&stage_path, &renamed).is_err(),
        "live stage handle must deny delete sharing"
    );
    drop(stage);
    drop(root);
    fs::rename(&stage_path, &renamed).expect("released stage may be renamed by its owner");
    assert!(renamed.is_dir());
}

#[test]
#[ignore = "private real-LPAC subprocess entry point"]
fn lpac_stage_mutation_child() {
    super::lpac_probe::mutation_child();
}

#[test]
fn real_lpac_role_cannot_mutate_released_protected_stage() {
    let fixture = Fixture::new();
    let source = fixture.source(GOLDEN_TAR);
    let receipt = fixture.receipt(GOLDEN_TAR);
    let mut root = fixture
        .admitted("1.0.0")
        .open_windows_extraction_root()
        .expect("protected root");
    let stage = root
        .extract(&receipt, &source)
        .expect("real protected stage");
    let stage_path = fixture.versions().join(stage.name());
    drop(stage);
    drop(root); // Avoid confusing LPAC ACL denial with delete-sharing pins.

    super::lpac_probe::run_lpac_probe(fixture.temp.path(), &stage_path, None);
    assert_eq!(
        fs::read(stage_path.join("tree/nest/one")).expect("protected file"),
        b"!"
    );
    assert!(!stage_path.join("tree/new.txt").exists());
    assert!(!stage_path.join("tree/renamed.txt").exists());
    assert!(!stage_path.join("tree/reparse.txt").exists());
    assert!(
        !fs::metadata(stage_path.join("tree/nest/one"))
            .expect("protected attributes")
            .permissions()
            .readonly()
    );
    let stage_dir = cap_std::fs::Dir::open_ambient_dir(&stage_path, ambient_authority())
        .expect("stage directory")
        .into_std_file();
    validate_windows_owner_private_directory(&stage_dir).expect("stage ACL unchanged");
    validate_windows_owner_private_file(
        &File::open(stage_path.join("tree/nest/one")).expect("protected file"),
    )
    .expect("file ACL unchanged");
}
