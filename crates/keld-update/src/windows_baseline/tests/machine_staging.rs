//! Real loaded-state handoff to SYSTEM-only, owner-private incomplete extraction.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Cursor;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use super::support::{self, GOLDEN, baseline, provision, trust_for};
use crate::tests::{digest_hex, manifest_json, release_json, sign, signing_key};
use crate::{
    LoadedWindowsBaseline, ManifestDecision, ProvenanceField, SigningKeyId, UpdateError,
    UpdateVerifier, VerifiedFull, WindowsBaselineTrust, WindowsExtractionRoot,
    initialize_windows_baseline, load_windows_baseline,
};

pub(super) fn run(root: &Path) {
    let trust = provision(root, "machine-staging");
    drop(
        initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
            .expect("real installed baseline"),
    );
    let unchanged = protected_bytes(&trust);
    refuse_changed_conversion_descriptor(&trust);
    let loaded = load_windows_baseline(&trust).expect("real protected loader");
    let candidate = higher_release(&loaded);
    let mut extraction = loaded
        .into_windows_extraction_root()
        .expect("SYSTEM consumes actual loader");
    assert_metadata_pins(&trust, true);
    refuse_context_and_floor(&mut extraction, &candidate, root);
    refuse_changed_descriptor(&mut extraction, &candidate, root, &trust);
    assert_eq!(versions(&trust), BTreeSet::from([PathBuf::from("1.0.0")]));
    let stage = extraction
        .extract(&candidate, &root.join("source.tar"))
        .expect("same T3b extraction accepts actual loaded machine root");
    assert_eq!(stage.identity(), candidate.identity());
    let name = stage.name().to_owned();
    let stage_path = trust.installation.update_root.join("versions").join(&name);
    assert_eq!(
        fs::read(stage_path.join("content.tar")).expect("staged archive"),
        GOLDEN
    );
    assert_eq!(
        fs::read(stage_path.join("tree/nest/one")).expect("staged payload"),
        b"!"
    );
    assert!(!stage_path.join(".complete").exists());
    drop(stage);
    drop(extraction);
    assert_metadata_pins(&trust, false);
    keld_guard::validate_windows_owner_private_directory(&support::directory(&stage_path))
        .expect("stage stays SYSTEM-private after all root handles close");
    keld_guard::validate_windows_owner_private_file(
        &fs::File::open(stage_path.join("content.tar")).expect("private archive handle"),
    )
    .expect("private stage file policy unchanged");
    assert_eq!(
        versions(&trust),
        BTreeSet::from([PathBuf::from("1.0.0"), PathBuf::from(name)])
    );
    assert_eq!(
        protected_bytes(&trust),
        unchanged,
        "staging changes no baseline record or byte"
    );
    assert!(
        !trust
            .installation
            .update_root
            .join("versions/2.0.0")
            .exists()
    );
    drop(
        load_windows_baseline(&trust)
            .expect("diagnostic incomplete sibling does not become active"),
    );
    println!("KELD_KEL266_MACHINE_STAGING_PASSED");
}

fn higher_release(loaded: &LoadedWindowsBaseline) -> VerifiedFull {
    let updater = UpdateVerifier::new(
        loaded.identity().clone(),
        signing_key().verifying_key().to_bytes(),
    )
    .expect("configured update verifier");
    let admitted = updater
        .admit(loaded.observation())
        .expect("admit actual protected observation");
    let compressed =
        zstd::stream::encode_all(Cursor::new(GOLDEN), 0).expect("full release transport");
    let release = release_json(
        "2.0.0",
        &compressed.len().to_string(),
        &digest_hex(&compressed),
        &GOLDEN.len().to_string(),
        &digest_hex(GOLDEN),
        "",
    );
    let manifest = manifest_json(&release);
    let ManifestDecision::Update(selected) = admitted
        .verify_manifest(&manifest, &sign(&manifest))
        .expect("authenticate higher release")
    else {
        panic!("higher release must be selected")
    };
    let mut output = Vec::new();
    let receipt = selected
        .verify_full(&mut Cursor::new(compressed), &mut output)
        .expect("authenticate complete package bytes");
    assert_eq!(output, GOLDEN);
    receipt
}

fn refuse_context_and_floor(
    root: &mut WindowsExtractionRoot,
    receipt: &VerifiedFull,
    suite: &Path,
) {
    // These one-field controls deliberately mutate private receipt storage. The
    // public authenticated API cannot create these contradictory capabilities.
    for field in [
        ProvenanceField::InstallMode,
        ProvenanceField::InstallRoot,
        ProvenanceField::SigningKey,
        ProvenanceField::Profile,
    ] {
        let mut changed = receipt.clone();
        match field {
            ProvenanceField::InstallMode => {
                changed.installation.install_mode = crate::DirectInstallMode::PerUserDirect;
            }
            ProvenanceField::InstallRoot => changed.installation.install_root.push("foreign"),
            ProvenanceField::SigningKey => {
                changed.installation.signing_key_id = SigningKeyId::from_public_key(&[0x44; 32]);
            }
            ProvenanceField::Profile => changed.installation.profile_digest.0[0] ^= 1,
            _ => unreachable!("literal context-control fields"),
        }
        assert!(
            matches!(root.extract(&changed, &suite.join("absent.tar")),
            Err(UpdateError::ProvenanceMismatch { field: actual, .. }) if actual == field),
            "context refusal must precede source I/O: {field:?}"
        );
    }
    let mut equal = receipt.clone();
    equal.content.identity.version = "1.0.0".to_owned();
    assert!(
        matches!(
            root.extract(&equal, &suite.join("absent.tar")),
            Err(UpdateError::ProvenanceMismatch {
                field: ProvenanceField::VersionFloor,
                ..
            })
        ),
        "nonhigher receipt must refuse before source I/O"
    );
}

fn refuse_changed_conversion_descriptor(trust: &WindowsBaselineTrust) {
    let loaded = load_windows_baseline(trust).expect("qualified state before descriptor drift");
    let directory = trust.installation.update_root.join("versions");
    super::substitutions::set_dacl(
        &directory,
        "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)(A;;0x2;;;BU)",
    );
    assert!(
        matches!(
            loaded.into_windows_extraction_root(),
            Err(UpdateError::Extraction {
                step: "machine versions protection",
                incomplete_stage: None,
                ..
            })
        ),
        "conversion rechecks the exact committed descriptor"
    );
    super::substitutions::set_dacl(&directory, "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)");
}

fn refuse_changed_descriptor(
    root: &mut WindowsExtractionRoot,
    receipt: &VerifiedFull,
    suite: &Path,
    trust: &WindowsBaselineTrust,
) {
    let directory = trust.installation.update_root.join("versions");
    super::substitutions::set_dacl(
        &directory,
        "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)(A;;0x2;;;BU)",
    );
    assert!(
        matches!(
            root.extract(receipt, &suite.join("source.tar")),
            Err(UpdateError::Extraction {
                step: "versions protection",
                incomplete_stage: None,
                ..
            })
        ),
        "machine descriptor is checked again immediately before creation"
    );
    super::substitutions::set_dacl(&directory, "O:SYD:P(A;OICI;FA;;;SY)(A;OICI;0x1200a9;;;BU)");
}

fn assert_metadata_pins(trust: &WindowsBaselineTrust, retained: bool) {
    let update = &trust.installation.update_root;
    let writer = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(update.join("activation.lock"));
    if retained {
        assert!(
            writer.is_err(),
            "retained read owner keeps the shared lease"
        );
    } else {
        drop(writer.expect("released read owner permits a writer"));
    }
    let version = update.join("versions/1.0.0");
    for path in [
        trust.installation.install_root.join("install-provenance"),
        update.join("version-floor"),
        update.join("current"),
        update.join("last-known-good"),
        version.join(".complete"),
    ] {
        let writer = OpenOptions::new().write(true).open(&path);
        if retained {
            assert_eq!(
                writer
                    .expect_err("retained metadata denies writer")
                    .raw_os_error(),
                Some(32)
            );
        } else {
            drop(writer.expect("released read pins allow SYSTEM writer without changing bytes"));
        }
    }
    let deletion = OpenOptions::new()
        .access_mode(DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(version);
    if retained {
        assert_eq!(
            deletion
                .expect_err("retained baseline directory denies rename authority")
                .raw_os_error(),
            Some(32)
        );
    } else {
        drop(deletion.expect("released directory permits SYSTEM DELETE open without mutation"));
    }
}

fn versions(trust: &WindowsBaselineTrust) -> BTreeSet<PathBuf> {
    fs::read_dir(trust.installation.update_root.join("versions"))
        .expect("version census")
        .map(|entry| PathBuf::from(entry.expect("directory entry").file_name()))
        .collect()
}

fn protected_bytes(trust: &WindowsBaselineTrust) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    for relative in [
        "install-provenance",
        "updates/bootstrap.lock",
        "updates/activation.lock",
        "updates/version-floor",
        "updates/current",
        "updates/last-known-good",
    ] {
        let path = trust.installation.install_root.join(relative);
        files.insert(
            path.clone(),
            fs::read(path).expect("baseline metadata bytes"),
        );
    }
    collect_files(
        &trust.installation.update_root.join("versions/1.0.0"),
        &mut files,
    );
    files
}

fn collect_files(directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(directory).expect("baseline tree census") {
        let entry = entry.expect("baseline entry");
        if entry.file_type().expect("baseline type").is_dir() {
            collect_files(&entry.path(), files);
        } else {
            files.insert(
                entry.path(),
                fs::read(entry.path()).expect("baseline file bytes"),
            );
        }
    }
}

#[test]
#[ignore = "requires actual SYSTEM-created baseline and a separate ordinary-user process"]
fn ordinary_loaded_conversion_refuses_before_staging() {
    support::assert_ordinary_token();
    let root = support::suite_root();
    assert!(
        root.join("system-finished.txt").is_file(),
        "SYSTEM suite completed"
    );
    for case in ["success", "machine-staging"] {
        let trust = trust_for(&root.join(case));
        let names = versions(&trust);
        let unchanged = protected_bytes(&trust);
        let loaded = load_windows_baseline(&trust)
            .expect("ordinary metadata load, including private diagnostic sibling");
        assert!(
            matches!(
                loaded.into_windows_extraction_root(),
                Err(UpdateError::Extraction {
                    step: "machine staging authority",
                    incomplete_stage: None,
                    ..
                })
            ),
            "ordinary token cannot convert real loaded state into staging authority"
        );
        assert_eq!(versions(&trust), names);
        assert_eq!(protected_bytes(&trust), unchanged);
    }
    println!("KELD_KEL266_ORDINARY_MACHINE_CONVERSION_REFUSED");
}
