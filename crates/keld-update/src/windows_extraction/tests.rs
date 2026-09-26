//! Real Windows filesystem acceptance for the T3b extraction boundary.

#![allow(unsafe_code)] // test-only hostile Win32 DACL mutation attempt with local proof
#![deny(unsafe_op_in_unsafe_fn)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read as _, Seek as _, SeekFrom};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::{OpenOptionsExt as _, symlink_dir, symlink_file};
use std::os::windows::io::{AsHandle as _, AsRawHandle as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use cap_std::ambient_authority;
use ed25519_dalek::{Signer as _, SigningKey};
use keld_guard::{
    ProfileDigest, validate_windows_owner_private_directory, validate_windows_owner_private_file,
};
use keld_runtime::windows_lpac::{WindowsLpacPathAccess, WindowsLpacProfile, WindowsLpacStdio};
use tempfile::TempDir;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_READONLY, SetFileAttributesW};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::Memory::{CreateFileMappingW, PAGE_READONLY, PAGE_READWRITE};

use super::*;
use crate::tests::{
    append_required_policy, append_ustar_entry, digest_hex, expected_identity, finish_ustar,
    manifest_json, observation, release_json, signing_key,
};
use crate::{
    DirectInstallationIdentity, InstallOwner, ManifestDecision, ProvenanceField, SigningKeyId,
    UpdateVerifier,
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
        identity.install_root = temp.path().join("install");
        identity.update_root = temp.path().join("updates");
        fs::create_dir(&identity.install_root).expect("fixture install root");
        let parent = cap_std::fs::Dir::open_ambient_dir(temp.path(), ambient_authority())
            .expect("retained fixture parent")
            .into_std_file();
        let update = crate::windows_fs::create_directory_relative(&parent, "updates")
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
    let renamed = fixture.temp.path().join("updates~stable");
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

const LPAC_HELPER_ENV: &str = "KELD_265_EXTRACTION_LPAC_HELPER";
const LPAC_HELPER_TEST: &str = "windows_extraction::tests::lpac_stage_mutation_child";

#[test]
#[ignore = "private real-LPAC subprocess entry point"]
fn lpac_stage_mutation_child() {
    if env::var(LPAC_HELPER_ENV).as_deref() != Ok("probe") {
        return;
    }
    let stage = PathBuf::from(env::var_os("KELD_265_STAGE").expect("host-provided stage path"));
    let private = PathBuf::from(env::var_os("KELD_265_PRIVATE").expect("role-private path"));
    let private_file = private.join("allowed.txt");
    fs::write(&private_file, b"role-owned").expect("granted role-private write");
    let mut private_permissions = fs::metadata(&private_file)
        .expect("role-private metadata")
        .permissions();
    private_permissions.set_readonly(true);
    fs::set_permissions(&private_file, private_permissions)
        .expect("granted role-private attribute write");

    let create_denied = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join("tree/new.txt"))
        .is_err();
    let rename_denied = fs::rename(&private_file, stage.join("tree/renamed.txt")).is_err();
    let reparse_denied = symlink_file(&private_file, stage.join("tree/reparse.txt")).is_err();
    let victim = stage.join("tree/nest/one");
    let wide: Vec<u16> = victim
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `wide` is a live NUL-terminated UTF-16 path and the attribute
    // constant is a Win32 value. The host checks the original attribute afterward.
    let attribute_denied = unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_READONLY) }
        == 0
        && std::io::Error::last_os_error().raw_os_error() == Some(5);
    // SAFETY: `wide` is a live NUL-terminated UTF-16 path; this hostile call passes
    // no pointers to caller-owned security objects. Success would be caught below
    // and the entire fixture is inside a disposable private temporary tree.
    // This denial proves the protected stage DACL did not change. RolePrivate
    // does not grant WRITE_DAC either, so it is not an operation-matched proof
    // that LPAC could edit some other ACL. The host rechecks the exact stage ACL.
    let acl_status = unsafe {
        SetNamedSecurityInfoW(
            wide.as_ptr().cast_mut(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    let acl_denied = acl_status == 5;
    println!(
        "KELD_265_LPAC private_create=true private_attribute=true create_denied={create_denied} \
         rename_denied={rename_denied} reparse_denied={reparse_denied} \
         attribute_denied={attribute_denied} acl_denied={acl_denied}"
    );
    assert!(create_denied && rename_denied && reparse_denied);
    assert!(attribute_denied && acl_denied);
}

fn run_lpac_stage_probe(fixture: &Fixture, stage_path: &Path) {
    let runtime = fixture.temp.path().join("lpac-runtime");
    let private = fixture.temp.path().join("lpac-private");
    fs::create_dir(&runtime).expect("runtime ACL fixture");
    fs::create_dir(&private).expect("role-private ACL fixture");
    let program = runtime.join("lpac-extraction-probe.exe");
    fs::copy(env::current_exe().expect("test executable"), &program)
        .expect("copy real subprocess fixture");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let profile_name = format!("keld-265-{}-{nonce}", std::process::id());
    let profile = WindowsLpacProfile::create(OsStr::new(&profile_name))
        .expect("fresh zero-capability LPAC profile");
    profile
        .grant_path(fixture.temp.path(), WindowsLpacPathAccess::Traverse)
        .expect("fixture ancestor traversal only");
    profile
        .grant_path(&runtime, WindowsLpacPathAccess::ReadExecute)
        .expect("runtime executable ACL");
    profile
        .grant_path(&private, WindowsLpacPathAccess::RolePrivate)
        .expect("role-private control ACL");

    let mut output = tempfile::tempfile_in(&private).expect("captured LPAC output");
    let input = File::open("NUL").expect("null LPAC stdin");
    let mut environment = vec![
        (OsString::from(LPAC_HELPER_ENV), OsString::from("probe")),
        (
            OsString::from("KELD_265_STAGE"),
            stage_path.as_os_str().to_owned(),
        ),
        (
            OsString::from("KELD_265_PRIVATE"),
            private.clone().into_os_string(),
        ),
        (OsString::from("TEMP"), private.clone().into_os_string()),
        (OsString::from("TMP"), private.clone().into_os_string()),
    ];
    for key in ["SystemRoot", "WINDIR", "USERPROFILE", "LOCALAPPDATA"] {
        if let Some(value) = env::var_os(key) {
            environment.push((OsString::from(key), value));
        }
    }
    let args: Vec<OsString> = ["--exact", LPAC_HELPER_TEST, "--ignored", "--nocapture"]
        .into_iter()
        .map(OsString::from)
        .collect();
    let mut child = profile
        .spawn_suspended(
            &program,
            &args,
            &environment,
            Some(&private),
            Some(WindowsLpacStdio {
                stdin: input.as_handle(),
                stdout: output.as_handle(),
                stderr: output.as_handle(),
            }),
            &[],
        )
        .expect("suspended LPAC fixture");
    let token = child.observe_token().expect("real LPAC token");
    assert!(token.is_app_container);
    assert!(token.all_application_packages_opt_out_configured);
    assert_eq!(token.capability_count, 0);
    child.resume().expect("resume inspected LPAC fixture");
    let exit = child.wait(10_000).expect("bounded LPAC fixture exit");
    output.seek(SeekFrom::Start(0)).expect("rewind LPAC output");
    let mut observed = String::new();
    output
        .read_to_string(&mut observed)
        .expect("read LPAC output");
    assert_eq!(exit, 0, "LPAC child failed: {observed}");
    assert!(
        observed.contains(
            "KELD_265_LPAC private_create=true private_attribute=true \
                           create_denied=true rename_denied=true reparse_denied=true \
                           attribute_denied=true acl_denied=true"
        ),
        "unexpected LPAC observation: {observed}"
    );
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

    run_lpac_stage_probe(&fixture, &stage_path);
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
