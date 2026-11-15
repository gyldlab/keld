//! Actual ordinary-host LPAC denial on the independently initialized installation.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use windows_permissions::constants::{SeObjectType, SecurityInformation};
use windows_permissions::wrappers::GetSecurityInfo;

use super::support;

struct Snapshot {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
    descriptor: String,
}

fn descriptor(object: &File) -> String {
    GetSecurityInfo(
        object,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
    .expect("parent observes retained-object security")
    .as_sddl()
    .expect("descriptor text")
    .to_string_lossy()
    .into_owned()
}

fn snapshot(install: &Path) -> Vec<Snapshot> {
    let mut observed = Vec::new();
    for relative in [
        "",
        "updates",
        "updates/versions",
        "updates/versions/1.0.0",
        "updates/versions/1.0.0/tree",
    ] {
        let path = install.join(relative);
        let object = support::directory(&path);
        keld_guard::validate_windows_machine_directory(&object)
            .expect("committed directory exists and is protected");
        observed.push(Snapshot {
            path,
            bytes: None,
            descriptor: descriptor(&object),
        });
    }
    for relative in [
        "install-provenance",
        "updates/activation.lock",
        "updates/version-floor",
        "updates/current",
        "updates/last-known-good",
        "updates/versions/1.0.0/.complete",
        "updates/versions/1.0.0/content.tar",
        "updates/versions/1.0.0/tree/nest/one",
    ] {
        let path = install.join(relative);
        let object = File::open(&path).expect("parent confirms target exists before role denial");
        keld_guard::validate_windows_machine_file(&object).expect("committed file protection");
        observed.push(Snapshot {
            bytes: Some(fs::read(&path).expect("protected bytes before role")),
            descriptor: descriptor(&object),
            path,
        });
    }
    observed // Every temporary File is dropped; snapshots retain values only.
}

#[test]
#[ignore = "requires SYSTEM-created baseline; execute from an ordinary host, never SYSTEM"]
fn real_lpac_cannot_mutate_committed_baseline() {
    support::assert_ordinary_token();
    let root = support::suite_root();
    assert!(root.join("system-finished.txt").is_file());
    let install = root.join("success");
    let trust = support::trust_for(&install);
    drop(crate::load_windows_baseline(&trust).expect("real coherent baseline admission"));
    let before = snapshot(&install);
    let private_fixture = tempfile::tempdir().expect("ordinary-owned LPAC runtime/control fixture");
    let version = install.join("updates/versions/1.0.0");
    let output = crate::windows_extraction::lpac_probe::run_lpac_probe(
        private_fixture.path(),
        &version,
        Some(&install),
    );
    assert!(output.contains("KELD_266_LPAC_BASELINE protected_files=8 write_denied=true delete_access_denied=true dac_denied=true owner_denied=true create_denied=true"),
        "committed-state probe must actually run: {output}");
    let after = snapshot(&install);
    assert_eq!(after.len(), before.len());
    for (before, after) in before.iter().zip(&after) {
        assert_eq!(before.path, after.path);
        assert_eq!(
            before.bytes, after.bytes,
            "protected bytes changed: {:?}",
            before.path
        );
        assert_eq!(
            before.descriptor, after.descriptor,
            "descriptor changed: {:?}",
            before.path
        );
    }
    for unexpected in [
        install.join("lpac-new"),
        install.join("updates/lpac-new"),
        version.join("tree/new.txt"),
        version.join("tree/renamed.txt"),
        version.join("tree/reparse.txt"),
    ] {
        assert!(
            !unexpected.exists(),
            "role mutation left an entry: {unexpected:?}"
        );
    }
    println!("KELD_KEL266_REAL_LPAC_BASELINE_DENIAL_PASSED");
}
