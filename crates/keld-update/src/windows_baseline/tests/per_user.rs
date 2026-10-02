//! Native `PerUserDirect` installer/bootstrap acceptance.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use windows_permissions::constants::{SeObjectType, SecurityInformation};
use windows_permissions::wrappers::{ConvertSidToStringSid, SetSecurityInfo};
use windows_permissions::{LocalBox, SecurityDescriptor};

use super::support::{self, CASE_ENV, CUT_ENV, GOLDEN, ROOT_ENV};
use crate::windows_baseline::initialize::{
    BaselineBoundary, initialize_per_user_with_observer, initialize_per_user_with_token_check,
    initialize_windows_per_user_baseline,
};
use crate::windows_baseline::{WindowsBaselineTrust, load_windows_baseline};
use crate::{DirectInstallMode, InstallOwner, InstallProvenance, UpdateError};

const CRASH_HELPER: &str = "windows_baseline::tests::per_user::per_user_baseline_crash_helper";
const PER_USER_PROFILE: keld_guard::WindowsInstallProtectionProfile =
    keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate;

fn local_app_data() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("current Windows LocalAppData"))
}

fn fixture() -> (tempfile::TempDir, WindowsBaselineTrust, PathBuf) {
    let local = local_app_data();
    let temp = tempfile::tempdir_in(&local)
        .expect("isolated fixture beneath the current user's LocalAppData");
    let source = temp.path().join("baseline.tar");
    fs::write(&source, GOLDEN).expect("write authenticated baseline bytes");
    let trust = support::provision_per_user(temp.path(), "KeldPerUserFixture");
    assert!(
        trust.installation.install_root.starts_with(&local),
        "the fixture installation is beneath the owner's LocalAppData"
    );
    let install = support::directory(&trust.installation.install_root);
    assert_eq!(
        trust.volume_guid,
        crate::windows_fs::qualified_volume_root(&install)
            .expect("retain the actual LocalAppData volume identity"),
        "fixture trust must be bound to the volume containing LocalAppData"
    );
    (temp, trust, source)
}

fn assert_user_directory(path: &Path) {
    let directory = support::directory(path);
    keld_guard::validate_windows_install_directory(&directory, PER_USER_PROFILE)
        .expect("directory keeps the exact per-user owner-private profile");
}

fn assert_user_file(path: &Path) {
    let file = fs::File::open(path).expect("open per-user protected record");
    keld_guard::validate_windows_install_file(&file, PER_USER_PROFILE)
        .expect("file keeps the exact per-user owner-private profile");
}

#[test]
fn per_user_baseline_initialization_seeds_exact_state_and_reads_it_back() {
    support::assert_user_principal_token();
    assert_per_user_baseline_seed();
}

#[test]
#[ignore = "operator acceptance: run the default per-user installer as the unelevated owner"]
fn per_user_baseline_initialization_runs_without_elevation() {
    support::assert_ordinary_token();
    assert_per_user_baseline_seed();
}

fn assert_per_user_baseline_seed() {
    let (_temp, trust, source) = fixture();
    let verified = support::baseline(&trust);
    let receipt = initialize_windows_per_user_baseline(&verified, &source, &trust)
        .expect("initialize the signed baseline under the installing user's authority");
    let loaded = receipt.loaded();
    assert_eq!(loaded.identity(), &trust.installation);
    assert_eq!(loaded.version_floor(), "1.0.0");
    assert!(matches!(
        loaded.observation(),
        crate::ProvenanceObservation::Protected {
            record: InstallProvenance {
                identity,
                owner: InstallOwner::Direct,
            },
            version_floor: Some(floor),
        } if identity.install_mode == DirectInstallMode::PerUserDirect && floor == "1.0.0"
    ));

    let install = &trust.installation.install_root;
    let update = &trust.installation.update_root;
    let version = update.join("versions").join("1.0.0");
    for path in [
        install.as_path(),
        update.as_path(),
        &update.join("versions"),
        version.as_path(),
        &version.join("tree"),
        &version.join("tree/nest"),
    ] {
        assert_user_directory(path);
    }
    for path in [
        &install.join("install-provenance"),
        &update.join("bootstrap.lock"),
        &update.join("activation.lock"),
        &update.join("version-floor"),
        &update.join("current"),
        &update.join("last-known-good"),
        &version.join(".complete"),
        &version.join("content.tar"),
        &version.join("tree/nest/one"),
    ] {
        assert_user_file(path);
    }
    assert_eq!(
        fs::read(update.join("version-floor")).expect("read seeded trust floor"),
        crate::records::encode_floor("1.0.0").expect("canonical baseline floor")
    );
    assert_eq!(
        fs::read(update.join("current")).expect("read seeded current pointer"),
        crate::records::encode_pointer(crate::records::PointerKind::Current, verified.identity())
            .expect("canonical baseline current")
    );
    assert_eq!(
        fs::read(update.join("last-known-good")).expect("read seeded LKG pointer"),
        crate::records::encode_pointer(
            crate::records::PointerKind::LastKnownGood,
            verified.identity()
        )
        .expect("canonical baseline LKG")
    );
    assert_eq!(
        fs::read(version.join("content.tar")).expect("retained baseline archive"),
        GOLDEN
    );
    assert!(!update.join("previous-known-good").exists());
    assert!(!update.join("activation-journal").exists());

    let loaded_again = load_windows_baseline(&trust)
        .expect("a fresh read-only loader accepts the committed per-user baseline");
    assert_eq!(loaded_again.version_floor(), "1.0.0");
    assert_eq!(
        loaded_again.identity().install_mode,
        DirectInstallMode::PerUserDirect
    );
}

#[test]
fn per_user_baseline_initializer_refuses_wrong_mode_before_path_admission() {
    support::assert_user_principal_token();
    let temp = tempfile::tempdir_in(local_app_data())
        .expect("isolated wrong-mode fixture beneath LocalAppData");
    let install = temp.path().join("must-not-exist");
    let mut trust = support::trust_for(&install);
    trust.installation.install_mode = DirectInstallMode::MachineUacDirect;
    let verified = support::baseline(&trust);
    let error =
        initialize_windows_per_user_baseline(&verified, &temp.path().join("absent.tar"), &trust)
            .expect_err("the per-user initializer cannot choose a machine mode");
    assert!(matches!(
        error,
        UpdateError::Baseline {
            step: "installer mode",
            ..
        }
    ));
    assert!(
        !install.exists(),
        "mode refusal precedes all path admission"
    );
}

#[test]
fn per_user_baseline_initializer_refuses_token_query_failure_before_path_admission() {
    support::assert_user_principal_token();
    let (_temp, trust, source) = fixture();
    let absent_archive = source.with_file_name("missing-authenticated-baseline.tar");
    let error = initialize_per_user_with_token_check(
        &support::baseline(&trust),
        &absent_archive,
        &trust,
        || Err(std::io::Error::other("injected token query failure")),
    )
    .expect_err("unknown token identity must refuse before filesystem admission");
    assert!(matches!(
        error,
        UpdateError::Baseline {
            step: "installer authority",
            ..
        }
    ));
    assert!(
        error.to_string().contains("injected token query failure"),
        "the guard query failure remains diagnosable: {error}"
    );
    let update = &trust.installation.update_root;
    assert!(!update.join("bootstrap.lock").exists());
    assert!(!update.join("activation.lock").exists());
    assert!(!update.join("version-floor").exists());
    assert!(!update.join("current").exists());
    assert!(!update.join("last-known-good").exists());
    assert!(
        !trust
            .installation
            .install_root
            .join("install-provenance")
            .exists()
    );
    assert!(!absent_archive.exists());

    let mut invalid_trust = trust.clone();
    let missing_install = trust
        .installation
        .install_root
        .with_file_name("missing-install-root");
    invalid_trust.installation.install_root = missing_install.clone();
    invalid_trust.installation.update_root = missing_install.join("updates");
    let invalid_error = initialize_per_user_with_token_check(
        &support::baseline(&invalid_trust),
        &absent_archive,
        &invalid_trust,
        || Err(std::io::Error::other("injected token query failure")),
    )
    .expect_err("token query failure must precede even root admission");
    assert!(matches!(
        invalid_error,
        UpdateError::Baseline {
            step: "installer authority",
            ..
        }
    ));
    assert!(
        !missing_install.exists(),
        "failed token observation must not create or admit the install root"
    );
}

#[test]
fn per_user_baseline_initializer_refuses_managed_owner_before_path_admission() {
    support::assert_user_principal_token();
    let temp = tempfile::tempdir_in(local_app_data())
        .expect("isolated managed-owner fixture beneath LocalAppData");
    let install = temp.path().join("must-not-exist");
    let mut trust = support::trust_for(&install);
    trust.installation.install_mode = DirectInstallMode::PerUserDirect;
    trust.owner = InstallOwner::Managed {
        mechanism: "MSIX".to_owned(),
    };
    let verified = support::baseline(&trust);
    let error =
        initialize_windows_per_user_baseline(&verified, &temp.path().join("absent.tar"), &trust)
            .expect_err("managed deployment owner remains the only updater authority");
    assert!(matches!(error, UpdateError::ManagedInstall { .. }));
    assert!(
        !install.exists(),
        "managed refusal precedes filesystem admission"
    );
}

#[test]
fn per_user_baseline_initializer_refuses_noncanonical_root_acl_before_writes() {
    support::assert_user_principal_token();
    let (_temp, trust, source) = fixture();
    let verified = support::baseline(&trust);
    let current =
        windows_permissions::utilities::current_process_sid().expect("actual owner TokenUser");
    let sid = ConvertSidToStringSid(&current)
        .expect("owner SID text")
        .to_string_lossy()
        .into_owned();
    let altered = format!("O:{sid}D:P(A;OICI;FA;;;{sid})(A;OICI;0x1200a9;;;BU)");
    set_per_user_dacl(&trust.installation.install_root, &altered);

    let error = initialize_windows_per_user_baseline(&verified, &source, &trust)
        .expect_err("extra Builtin Users access is not the per-user profile");
    assert!(matches!(
        error,
        UpdateError::Baseline {
            step: "private scaffold admission",
            ..
        }
    ));
    let update = &trust.installation.update_root;
    assert!(!update.join("bootstrap.lock").exists());
    assert!(!update.join("activation.lock").exists());
    assert!(
        !trust
            .installation
            .install_root
            .join("install-provenance")
            .exists()
    );

    let exact = format!("O:{sid}D:P(A;OICI;FA;;;{sid})");
    set_per_user_dacl(&trust.installation.install_root, &exact);
    initialize_windows_per_user_baseline(&verified, &source, &trust)
        .expect("exact owner-private restoration is the positive control");
}

fn set_per_user_dacl(path: &Path, sddl: &str) {
    let descriptor: LocalBox<SecurityDescriptor> =
        sddl.parse().expect("per-user DACL mutation fixture parses");
    let mut object = support::dacl_handle(path);
    SetSecurityInfo(
        &mut object,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        descriptor.dacl(),
        None,
    )
    .expect("mutate only the isolated per-user fixture root DACL");
}

#[test]
fn real_lpac_role_cannot_mutate_per_user_baseline_or_updater_state() {
    support::assert_user_principal_token();
    let (temp, trust, source) = fixture();
    let verified = support::baseline(&trust);
    drop(
        initialize_windows_per_user_baseline(&verified, &source, &trust)
            .expect("initialize exact per-user baseline before role test"),
    );
    let version = trust
        .installation
        .update_root
        .join("versions")
        .join("1.0.0");
    let observed = crate::windows_extraction::lpac_probe::run_lpac_probe(
        temp.path(),
        &version,
        Some(&trust.installation.install_root),
    );
    assert!(
        observed.contains("KELD_266_LPAC_BASELINE protected_files=8 write_denied=true"),
        "real restricted-role probe must test baseline records: {observed}"
    );
    assert_eq!(
        fs::read(version.join("content.tar")).expect("owner still reads the baseline archive"),
        GOLDEN
    );
}

#[test]
fn per_user_baseline_initialization_is_provenance_last_at_every_crash_cut() {
    support::assert_user_principal_token();
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
        let (temp, _fixture_trust, source) = fixture();
        let case = format!("cut-{index}");
        let case_install = temp.path().join(&case);
        let per_user = support::provision_per_user(temp.path(), &case);
        assert_eq!(per_user.installation.install_root, case_install);
        let stdout = support::child(CRASH_HELPER, temp.path(), &case, &format!("{cut:?}"), 91);
        assert!(
            stdout.contains(&format!("KELD_T4C_CRASH_CUT={cut:?}")),
            "child reports the exact durable seed boundary: {stdout}"
        );
        let provenance = per_user
            .installation
            .install_root
            .join("install-provenance");
        if cut == BaselineBoundary::ProvenancePublished {
            assert!(provenance.is_file(), "final boundary publishes provenance");
            let loaded = load_windows_baseline(&per_user)
                .expect("provenance-last boundary admits the complete per-user baseline");
            assert_eq!(loaded.version_floor(), "1.0.0");
            crate::windows_baseline::load::validate_initial_seed(&loaded.roots)
                .expect("exact per-user baseline/floor/current/LKG seed");
        } else {
            assert!(!provenance.exists(), "precommit boundary has no provenance");
            assert!(
                load_windows_baseline(&per_user).is_err(),
                "partial per-user seed is never admitted"
            );
        }
        let error =
            initialize_windows_per_user_baseline(&support::baseline(&per_user), &source, &per_user)
                .expect_err("interrupted or committed baseline cannot be automatically reseeded");
        assert!(matches!(error, UpdateError::Baseline { .. }));
    }
}

#[test]
#[ignore = "private T4c subprocess endpoint for per-user baseline crash cuts"]
fn per_user_baseline_crash_helper() {
    support::assert_user_principal_token();
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("fixture root"));
    let case = std::env::var(CASE_ENV).expect("fixture case");
    let cut = std::env::var(CUT_ENV).expect("requested baseline cut");
    assert!(case.starts_with("cut-") && !case.contains(['/', '\\']));
    let mut trust = support::trust_for(&root.join(&case));
    trust.installation.install_mode = DirectInstallMode::PerUserDirect;
    let verified = support::baseline(&trust);
    let result = initialize_per_user_with_observer(
        &verified,
        &root.join("baseline.tar"),
        &trust,
        |boundary| {
            if format!("{boundary:?}") == cut {
                println!("KELD_T4C_CRASH_CUT={cut}");
                std::io::stdout()
                    .flush()
                    .expect("flush exact crash-boundary observation");
                std::process::exit(91);
            }
            Ok(())
        },
    );
    panic!("requested per-user crash boundary was not reached: {result:?}");
}
