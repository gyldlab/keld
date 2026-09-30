//! Actual SYSTEM transaction and process-crash acceptance. Not a power-loss test.

use std::fs;
use std::io::Write as _;

use super::support::{self, CASE_ENV, CUT_ENV, baseline, child, provision, trust_for};
use crate::windows_baseline::initialize::{BaselineBoundary, initialize_with_observer};
use crate::windows_baseline::{initialize_windows_baseline, load_windows_baseline};
use crate::windows_extraction::{StageProtection, open_source, populate_stage};

const CRASH_CHILD: &str = "windows_baseline::tests::qualification::system_crash_child";
const COMPETING_CHILD: &str = "windows_baseline::tests::qualification::system_competing_child";

#[test]
#[ignore = "requires reviewed operator helper running this exact selector as LocalSystem"]
fn system_baseline_qualification() {
    let root = support::create_suite();
    system_machine_uac_baseline_loader(&root);
    provision_and_qualify_committed_baseline(&root);

    super::substitutions::run(&root);
    super::machine_staging::run(&root);

    competing_initializer_is_refused(&root);

    let cuts = [
        BaselineBoundary::LockCreated,
        BaselineBoundary::ActivationLockCreated,
        BaselineBoundary::StageCreated,
        BaselineBoundary::StagePopulated,
        BaselineBoundary::CompletePublished,
        BaselineBoundary::VersionPublished,
        BaselineBoundary::FloorPublished,
        BaselineBoundary::CurrentPublished,
        BaselineBoundary::LastKnownGoodPublished,
        BaselineBoundary::RootsVerified,
        BaselineBoundary::ProvenancePublished,
    ];
    for (index, cut) in cuts.into_iter().enumerate() {
        let case = format!("cut-{index}");
        let cut_trust = provision(&root, &case);
        let output = child(CRASH_CHILD, &root, &case, &format!("{cut:?}"), 91);
        assert!(
            output.contains("KELD_KEL266_CRASH_CUT"),
            "child reached requested boundary"
        );
        let provenance = cut_trust
            .installation
            .install_root
            .join("install-provenance");
        if cut == BaselineBoundary::ProvenancePublished {
            assert!(provenance.is_file());
            let loaded = load_windows_baseline(&cut_trust)
                .expect("provenance-last cut has committed records");
            assert_eq!(loaded.version_floor(), "1.0.0");
            super::super::load::validate_initial_seed(&loaded.roots)
                .expect("complete initial seed after final cut");
        } else {
            assert!(
                !provenance.exists(),
                "precommit cut never publishes provenance"
            );
            assert!(
                load_windows_baseline(&cut_trust).is_err(),
                "partial state is never admitted"
            );
        }
        assert!(
            initialize_windows_baseline(
                &baseline(&cut_trust),
                &root.join("source.tar"),
                &cut_trust
            )
            .is_err(),
            "neither stale lock nor partial state triggers automatic repair"
        );
        println!("KELD_KEL266_CUT_OK={cut:?}");
    }
    // Preserve the explicit success fixture for the separate ordinary-user process.
    // Its read/write denial cannot be inferred from this SYSTEM process's success.
    fs::write(
        root.join("system-finished.txt"),
        b"SYSTEM transaction and process crash cuts passed\n",
    )
    .expect("qualification marker");
    println!("KELD_KEL266_SYSTEM_FINISHED={}", root.display());
}

fn provision_and_qualify_committed_baseline(root: &std::path::Path) -> crate::WindowsBaselineTrust {
    let trust = provision(root, "success");
    let receipt = initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
        .expect("SYSTEM initializes exact baseline");
    assert_eq!(receipt.loaded().version_floor(), "1.0.0");
    assert_eq!(receipt.loaded().publisher_scope(), &[0x26; 32]);
    assert_eq!(receipt.loaded().identity(), &trust.installation);
    drop(receipt);
    let loaded = load_windows_baseline(&trust).expect("fresh production read-only load");
    assert_eq!(loaded.version_floor(), "1.0.0");
    assert!(
        fs::read(trust.installation.update_root.join("activation.lock"))
            .expect("persistent activation lock")
            .is_empty()
    );
    let update = loaded
        .roots
        .update
        .try_clone()
        .expect("retain update directory across reader release");
    drop(loaded);
    assert_activation_lease_sharing(&update);
    assert_missing_activation_lock_refuses(root, &trust);
    assert!(
        initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust).is_err(),
        "committed installation cannot be reseeded"
    );
    let mut wrong = trust.clone();
    wrong.publisher_scope[0] ^= 1;
    assert!(load_windows_baseline(&wrong).is_err(), "publisher mismatch");
    wrong = trust.clone();
    wrong.volume_guid.replace_range(11..12, "0");
    if wrong.volume_guid == trust.volume_guid {
        wrong.volume_guid.replace_range(11..12, "1");
    }
    assert!(
        load_windows_baseline(&wrong).is_err(),
        "trusted volume mismatch"
    );
    trust
}

fn system_machine_uac_baseline_loader(root: &std::path::Path) {
    let trust = support::provision_machine_uac(root, "uac-mode");
    let verified = baseline(&trust);
    let mut source = open_source(&root.join("source.tar")).expect("lock UAC baseline source");
    let validated = verified
        .validate_windows_archive(&mut source)
        .expect("validate baseline content for UAC fixture");
    let profile = keld_guard::WindowsInstallProtectionProfile::MachineUac;
    let version_name = &trust.installation.baseline.version;
    let versions = cap_std::fs::Dir::from_std_file(support::directory(
        &trust.installation.update_root.join("versions"),
    ));
    let versions_parent = versions
        .try_clone()
        .expect("versions handle")
        .into_std_file();
    let version = crate::windows_fs::create_directory_relative_with_profile(
        &versions_parent,
        version_name,
        profile,
    )
    .expect("create protected UAC baseline version");
    let (directories, files) = populate_stage(
        version,
        version_name,
        &validated,
        &mut source,
        StageProtection::MachineUac,
        &mut |_, _| Ok::<(), std::io::Error>(()),
    )
    .expect("populate the UAC baseline with exact ACLs from object creation");
    let complete = crate::records::encode_complete(verified.identity(), verified.content_size())
        .expect("canonical UAC baseline completion record");
    let mut complete_file = crate::windows_fs::create_file_relative_with_profile(
        &directories[0]
            .try_clone()
            .expect("baseline version handle")
            .into_std_file(),
        ".complete",
        profile,
    )
    .expect("create protected UAC completion marker");
    complete_file
        .write_all(&complete)
        .expect("write completion marker");
    complete_file.sync_all().expect("flush completion marker");
    drop(complete_file);
    drop(files);
    drop(directories);

    let update = support::directory(&trust.installation.update_root);
    write_uac_record(&update, "activation.lock", b"");
    write_uac_record(
        &update,
        "version-floor",
        &crate::records::encode_floor(&verified.identity().version).expect("version floor"),
    );
    write_uac_record(
        &update,
        "current",
        &crate::records::encode_pointer(crate::records::PointerKind::Current, verified.identity())
            .expect("current pointer"),
    );
    write_uac_record(
        &update,
        "last-known-good",
        &crate::records::encode_pointer(
            crate::records::PointerKind::LastKnownGood,
            verified.identity(),
        )
        .expect("last-known-good pointer"),
    );
    let provenance = crate::records::encode_provenance(
        &crate::InstallProvenance {
            identity: trust.installation.clone(),
            owner: crate::InstallOwner::Direct,
        },
        &trust.publisher_scope,
        &trust.volume_guid,
    )
    .expect("mode-bound UAC provenance");
    let install = support::directory(&trust.installation.install_root);
    write_uac_record(&install, "install-provenance", &provenance);
    drop(install);
    drop(update);

    let loaded = load_windows_baseline(&trust)
        .expect("read-only loader accepts exact Machine-UAC owner/DACL records");
    assert_eq!(
        loaded.identity().install_mode,
        crate::DirectInstallMode::MachineUacDirect
    );
    assert_eq!(loaded.version_floor(), version_name);
    drop(loaded);

    assert_uac_writer_snapshot(&trust, version_name);
}

fn assert_uac_writer_snapshot(
    trust: &crate::windows_baseline::WindowsBaselineTrust,
    version: &str,
) {
    let updater = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier matches the protected installation");
    let update = support::directory(&trust.installation.update_root);
    write_uac_record(&update, "activation-journal", b"pending-test-journal");
    assert!(
        crate::windows_baseline::load::load_windows_activation_write_snapshot_for_test(
            trust, &updater,
        )
        .is_err(),
        "writer snapshot refuses pending recovery without a process-family observation"
    );
    std::fs::remove_file(trust.installation.update_root.join("activation-journal"))
        .expect("remove isolated pending-journal negative control");
    drop(update);

    let writer = crate::windows_baseline::load::load_windows_activation_write_snapshot_for_test(
        trust, &updater,
    )
    .expect("exclusive writer snapshot validates the exact Machine-UAC state");
    assert_eq!(writer.version_floor(), version);
    assert_eq!(writer.current().version, version);
    assert_eq!(writer.last_known_good().version, version);
    assert!(writer.previous_known_good().is_none());
    assert!(
        load_windows_baseline(trust).is_err(),
        "exclusive writer snapshot excludes coherent readers"
    );
    assert!(
        crate::windows_baseline::load::load_windows_activation_write_snapshot_for_test(
            trust, &updater,
        )
        .is_err(),
        "exclusive writer snapshot excludes a competing writer"
    );
    let root = writer
        .open_extraction_root()
        .expect("only the protected writer snapshot opens UAC staging root");
    assert!(
        load_windows_baseline(trust).is_err(),
        "extraction root retains exclusive writer lease"
    );
    drop(root);
    load_windows_baseline(trust).expect("dropping the writer root releases the lease for readers");
}

fn write_uac_record(parent: &std::fs::File, name: &str, bytes: &[u8]) {
    let mut output = crate::windows_fs::create_file_relative_with_profile(
        parent,
        name,
        keld_guard::WindowsInstallProtectionProfile::MachineUac,
    )
    .expect("create protected UAC local record");
    output.write_all(bytes).expect("write UAC local record");
    output.sync_all().expect("flush UAC local record");
}

fn assert_missing_activation_lock_refuses(
    root: &std::path::Path,
    trust: &crate::windows_baseline::WindowsBaselineTrust,
) {
    let activation_lock = trust.installation.update_root.join("activation.lock");
    let displaced_lock = root.join("saved-activation.lock");
    fs::rename(&activation_lock, &displaced_lock).expect("temporarily remove lease fixture");
    assert!(
        load_windows_baseline(trust).is_err(),
        "missing lease refuses"
    );
    fs::rename(&displaced_lock, &activation_lock).expect("restore persistent lease fixture");
}

fn assert_activation_lease_sharing(update: &cap_std::fs::Dir) {
    let profile = keld_guard::WindowsInstallProtectionProfile::MachineSystem;
    let first_reader = super::super::open_activation_lease(update, profile, false)
        .expect("first coherent snapshot reader");
    let second_reader = super::super::open_activation_lease(update, profile, false)
        .expect("concurrent snapshot reader");
    assert!(
        super::super::open_activation_lease(update, profile, true).is_err(),
        "read pins exclude the writer"
    );
    drop(first_reader);
    drop(second_reader);

    let writer = super::super::open_activation_lease(update, profile, true)
        .expect("one exclusive writer after every snapshot closes");
    assert!(
        super::super::open_activation_lease(update, profile, false).is_err(),
        "writer excludes a new snapshot"
    );
    assert!(
        super::super::open_activation_lease(update, profile, true).is_err(),
        "exclusive writer excludes a competing writer"
    );
    drop(writer);
    super::super::open_activation_lease(update, profile, false)
        .expect("persistent lock file remains reusable after handle release");
}

fn competing_initializer_is_refused(root: &std::path::Path) {
    let concurrent = provision(root, "concurrent");
    initialize_with_observer(
        &baseline(&concurrent),
        &root.join("source.tar"),
        &concurrent,
        |boundary| {
            if boundary == BaselineBoundary::LockCreated {
                let output = child(COMPETING_CHILD, root, "concurrent", "", 0);
                assert!(output.contains("KELD_KEL266_COMPETITOR_REFUSED"));
            }
            Ok(())
        },
    )
    .expect("first initializer succeeds while second actual process is refused");
}

#[test]
#[ignore = "private subprocess endpoint of SYSTEM qualification"]
fn system_crash_child() {
    keld_guard::require_windows_system_token().expect("actual SYSTEM child");
    let root = support::suite_root();
    let case = std::env::var(CASE_ENV).expect("private scenario selector");
    assert!(case.starts_with("cut-") && !case.contains(['/', '\\']));
    let trust = trust_for(&root.join(case));
    let cut = std::env::var(CUT_ENV).expect("requested persisted boundary");
    let result = initialize_with_observer(
        &baseline(&trust),
        &root.join("source.tar"),
        &trust,
        |boundary| {
            if format!("{boundary:?}") == cut {
                use std::io::Write as _;
                println!("KELD_KEL266_CRASH_CUT={cut}");
                std::io::stdout()
                    .flush()
                    .expect("cut observation before process exit");
                std::process::exit(91);
            }
            Ok(())
        },
    );
    panic!("requested crash boundary was not reached: {result:?}");
}

#[test]
#[ignore = "private subprocess endpoint of SYSTEM qualification"]
fn system_competing_child() {
    keld_guard::require_windows_system_token().expect("actual SYSTEM competitor");
    let root = support::suite_root();
    assert_eq!(
        std::env::var(CASE_ENV).expect("private scenario"),
        "concurrent"
    );
    let trust = trust_for(&root.join("concurrent"));
    let error = initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
        .expect_err("second process must not enter active initializer");
    assert!(matches!(
        error,
        crate::UpdateError::Baseline {
            step: "fresh update" | "exclusive bootstrap lock",
            ..
        }
    ));
    println!("KELD_KEL266_COMPETITOR_REFUSED");
}
