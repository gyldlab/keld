//! Fixtures for the KEL-254 T3 Part B native installed-boot rows in keld-host.
//!
//! They build real `PerUserDirect` installations around an operator-signed host through
//! the production per-user baseline initializer. They are fixture construction, not
//! acceptance oracles. Run unelevated:
//!
//! 1. once per app id, `kel254_operator_embeds_the_fixture_expectation` with
//!    `KELD_KEL254_UNSIGNED_HOST` naming a built `keld-host.exe`, a new
//!    `KELD_KEL254_EMBEDDED_HOST` path and, optionally, `KELD_KEL254_APP_ID` (default:
//!    the fixture app id);
//! 2. sign each output once with a KEL-135 acceptance publisher and a
//!    `keld.app-id/v1:` description (signtool, outside these tests);
//! 3. the keld-host installed rows run `kel254_per_user_install_fixture` from this test
//!    executable, which they name `KELD_KEL254_INSTALLER_FIXTURE`, once per launch with
//!    `KELD_KEL254_SIGNED_HOST`, a `KELD_KEL254_BOOT_SOURCE` holding the compiled boot
//!    files and a new, empty `KELD_KEL254_INSTALL_PARENT` beneath the user's
//!    `LocalAppData`. The record names the app id embedded in the signed host and that
//!    host's own publisher, unless `KELD_KEL254_RECORD_PUBLISHER_HOST` names another
//!    signed host whose publisher it records instead (the wrong-publisher control).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::locate::{host_path, open_image};
use super::signed_host::fixture_payload;
use super::support;
use crate::ExpectedAppIdentity;
use crate::windows_baseline::{
    initialize_windows_per_user_baseline, select_active_package_for_executable,
};

const UNSIGNED_HOST_ENV: &str = "KELD_KEL254_UNSIGNED_HOST";
const EMBEDDED_HOST_ENV: &str = "KELD_KEL254_EMBEDDED_HOST";
const APP_ID_ENV: &str = "KELD_KEL254_APP_ID";
const SIGNED_HOST_ENV: &str = "KELD_KEL254_SIGNED_HOST";
const RECORD_PUBLISHER_HOST_ENV: &str = "KELD_KEL254_RECORD_PUBLISHER_HOST";
const BOOT_SOURCE_ENV: &str = "KELD_KEL254_BOOT_SOURCE";
const INSTALL_PARENT_ENV: &str = "KELD_KEL254_INSTALL_PARENT";
const LABEL: &str = "KeldPerUserFixture";
const HOST: &str = "keld-host.exe";
const HELPER: &str = "keld-updater-helper.exe";

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} must be set")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write as _;
        write!(&mut text, "{byte:02x}").expect("format hex");
        text
    })
}

/// Every file and directory beneath `source` by its `/`-separated package name.
fn collect(source: &Path, prefix: &str, entries: &mut BTreeMap<String, Option<Vec<u8>>>) {
    for entry in fs::read_dir(source).expect("read boot source directory") {
        let entry = entry.expect("boot source entry");
        let name = entry
            .file_name()
            .into_string()
            .expect("UTF-8 boot source name");
        let package_name = format!("{prefix}{name}");
        let kind = entry.file_type().expect("boot source entry kind");
        if kind.is_dir() {
            entries.insert(package_name.clone(), None);
            collect(&entry.path(), &format!("{package_name}/"), entries);
        } else {
            assert!(kind.is_file(), "boot source holds a non-file object");
            let bytes = fs::read(entry.path()).expect("read boot source file");
            entries.insert(package_name, Some(bytes));
        }
    }
}

/// Canonical package content holding the boot source, `host` as `keld-host.exe` and a
/// fixture `keld-updater-helper.exe`, which these host rows never start.
fn installed_tree_content(source: &Path, host: &[u8]) -> Vec<u8> {
    let mut tree = BTreeMap::new();
    collect(source, "", &mut tree);
    assert!(
        !tree.contains_key(HOST) && !tree.contains_key(HELPER) && !tree.contains_key(".keld"),
        "the boot source holds no host, no updater helper and no producer-owned .keld policy"
    );
    tree.insert(HOST.to_owned(), Some(host.to_vec()));
    tree.insert(
        HELPER.to_owned(),
        Some(b"keld-updater-helper fixture image".to_vec()),
    );
    // `String` orders by bytes, the producer's strict member order.
    let mut inputs: Vec<(&str, Option<&[u8]>)> = tree
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_deref()))
        .collect();
    let mut entries: Vec<keld_pack::PackageEntry<'_>> = inputs
        .iter_mut()
        .map(|(name, bytes)| match bytes {
            None => keld_pack::PackageEntry::Directory { name },
            Some(input) => keld_pack::PackageEntry::File {
                name,
                size: u64::try_from(input.len()).expect("file length fits u64"),
                input,
            },
        })
        .collect();
    let mut compressed = Vec::new();
    keld_pack::produce_windows_v0(&mut entries, &mut compressed).expect("native producer");
    zstd::stream::decode_all(compressed.as_slice()).expect("canonical package content")
}

#[test]
#[ignore = "operator fixture (KEL-254 T3 Part B): needs KELD_KEL254_UNSIGNED_HOST, a new KELD_KEL254_EMBEDDED_HOST path and optionally KELD_KEL254_APP_ID"]
fn kel254_operator_embeds_the_fixture_expectation() {
    let unsigned = fs::read(env_path(UNSIGNED_HOST_ENV)).expect("read the unsigned host");
    let output = env_path(EMBEDDED_HOST_ENV);
    let app_id =
        std::env::var(APP_ID_ENV).unwrap_or_else(|_| crate::tests::expected_identity().app_id);
    let embedded = keld_pack::embed_host_identity(&unsigned, &fixture_payload(&app_id))
        .expect("the unsigned host is an admissible prebuilt image");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .expect("create the embedded host; never overwrite an existing file");
    file.write_all(&embedded).expect("write the embedded host");
    file.sync_all().expect("flush the embedded host");
    println!(
        "KELD_KEL254_EMBEDDED_HOST path={} sign_description=keld.app-id/v1:{app_id}",
        output.display()
    );
}

#[test]
#[ignore = "fixture for the keld-host installed rows (KEL-254 T3 Part B): run unelevated with KELD_KEL254_SIGNED_HOST, KELD_KEL254_BOOT_SOURCE and a new KELD_KEL254_INSTALL_PARENT beneath LocalAppData"]
fn kel254_per_user_install_fixture() {
    support::assert_ordinary_token();
    let signed_host = env_path(SIGNED_HOST_ENV);
    let publisher_host = std::env::var_os(RECORD_PUBLISHER_HOST_ENV)
        .map_or_else(|| signed_host.clone(), PathBuf::from);
    let publisher = keld_guard::WindowsAuthenticodeImage::open(&publisher_host)
        .and_then(keld_guard::WindowsAuthenticodeImage::verify)
        .expect("the recorded publisher's host verifies through the KEL-135 owner");
    let publisher_scope = *publisher.identity().publisher_scope();
    drop(publisher);
    // An installer records the app its signed host was built for.
    let app_id = ExpectedAppIdentity::from_signed_image(&open_image(&signed_host))
        .expect("the signed host carries one canonical expectation")
        .app_id;

    let parent = env_path(INSTALL_PARENT_ENV);
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LocalAppData"));
    assert!(
        parent.starts_with(&local),
        "a per-user installation lives beneath the owner's LocalAppData"
    );
    assert!(
        fs::read_dir(&parent)
            .expect("read install parent")
            .next()
            .is_none(),
        "use a new, empty install parent; never repair an existing fixture"
    );
    let host = fs::read(&signed_host).expect("read the signed host");
    let content = installed_tree_content(&env_path(BOOT_SOURCE_ENV), &host);
    let mut trust = support::provision_per_user_with(&parent, LABEL, &content);
    trust.publisher_scope = publisher_scope;
    trust.installation.app_id.clone_from(&app_id);
    trust.installation.baseline.app_id.clone_from(&app_id);
    let archive = parent.join("baseline.tar");
    fs::write(&archive, &content).expect("write the authenticated baseline bytes");
    let receipt = initialize_windows_per_user_baseline(
        &support::baseline_with(&trust, &content),
        &archive,
        &trust,
    )
    .expect("the production per-user initializer seeds the signed host's package");
    drop(receipt);
    fs::remove_file(&archive).expect("remove the baseline source");

    // The installed host's own container selects its installation, as the host will.
    // This fixture check presents the recorded signer; the keld-host rows present the
    // signer that verification of the installed host proves.
    let installed = host_path(&trust, &trust.installation.baseline.version);
    let image = open_image(&installed);
    let expected = ExpectedAppIdentity::from_signed_image(&image)
        .expect("the installed host carries its expectation");
    let selection = select_active_package_for_executable(
        crate::WindowsLocatedImage::Host,
        &installed,
        &image,
        &expected,
        &publisher_scope,
        &app_id,
    )
    .expect("the installed host selects its installation");
    assert_eq!(selection.publisher_scope(), &publisher_scope);
    assert_eq!(selection.install_identity().app_id, app_id);
    drop((selection, image));
    println!(
        "KELD_KEL254_INSTALLED_HOST {}",
        serde_json::json!({
            "host": installed,
            "app_id": app_id,
            "publisher_scope": hex(&publisher_scope),
        })
    );
}
