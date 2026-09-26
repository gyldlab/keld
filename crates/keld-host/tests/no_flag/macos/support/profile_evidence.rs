use crate::support::signed_app::signed_host_executable;
use std::path::Path;
use std::process::Command;
use std::process::Output;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn signed_media_executable_facts(executable: &Path) -> Option<(String, String)> {
    let bundle = executable
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))?;
    let verified = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle)
        .output()
        .expect("verify signed media fixture bundle before launch");
    assert!(
        verified.status.success(),
        "signed media fixture lost validity"
    );
    let digest = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(executable)
        .output()
        .expect("hash signed media fixture executable");
    assert!(digest.status.success(), "signed executable hash failed");
    let sha256 = String::from_utf8_lossy(&digest.stdout)
        .split_whitespace()
        .next()
        .expect("signed executable SHA-256")
        .to_owned();
    let details = Command::new("/usr/bin/codesign")
        .args(["-d", "--verbose=4"])
        .arg(bundle)
        .output()
        .expect("read signed media fixture CDHash");
    assert!(details.status.success(), "signed media CDHash read failed");
    let cdhash = String::from_utf8_lossy(&details.stderr)
        .lines()
        .find_map(|line| line.strip_prefix("CDHash="))
        .expect("signed media fixture CDHash")
        .to_owned();
    Some((sha256, cdhash))
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn attach_profile_run_evidence(
    report: &mut std::collections::BTreeMap<String, String>,
    output: &str,
    phase: &str,
    executable: &Path,
    signed_facts: Option<(String, String)>,
) {
    if let Some((sha256, cdhash)) = signed_facts {
        assert_eq!(
            signed_media_executable_facts(executable),
            Some((sha256.clone(), cdhash.clone())),
            "signed media executable changed during its fixture launch"
        );
        report.insert(String::from("host_sha256"), sha256.clone());
        report.insert(String::from("host_cdhash"), cdhash.clone());
        let identity = output
            .lines()
            .find(|line| line.starts_with("KELD_KEL135_SIGNED_IDENTITY "))
            .expect("current signed media host emitted its validated identity");
        for field in identity.split_whitespace().skip(1) {
            if let Some((key, value)) = field.split_once('=') {
                report.insert(format!("signed_{key}"), value.to_owned());
            }
        }
        eprintln!(
            "KELD_KEL135_SIGNED_MEDIA_RUN phase={phase} sha256={sha256} cdhash={cdhash} {identity}"
        );
    }
    let store = output
        .lines()
        .find(|line| line.starts_with("KELD_KEL135_STORE "))
        .expect("read selected WebKit store report");
    for field in store.split_whitespace().skip(1) {
        if let Some((key, value)) = field.split_once('=') {
            report.insert(format!("store_{key}"), value.to_owned());
        }
    }
    if phase.starts_with("media-") || phase.starts_with("query-") {
        let probe = output
            .lines()
            .find(|line| line.starts_with("KELD_KEL135_MEDIA_PROBE "))
            .expect("read signed-host AppKit/TCC media probe result");
        for field in probe.split_whitespace().skip(1) {
            if let Some((key, value)) = field.split_once('=') {
                report.insert(format!("probe_{key}"), value.to_owned());
            }
        }
        eprintln!("KELD_KEL135_MEDIA_PROBE_RESULT phase={phase} {probe}");
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn assert_store_report_matches(log: &str, store_uuid: &str) {
    let report = log
        .lines()
        .find(|line| line.contains("KELD_KEL135_STORE"))
        .expect("selected WKWebsiteDataStore report");
    assert!(
        report.contains(&format!("expected_store_uuid={store_uuid}")),
        "{report}"
    );
    assert!(
        report.contains(&format!("actual_identifier={store_uuid}")),
        "{report}"
    );
    assert!(report.contains("persistent=true"), "{report}");
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn assert_ephemeral_store_report(log: &str) {
    let report = log
        .lines()
        .find(|line| line.contains("KELD_KEL135_STORE"))
        .expect("selected ephemeral WKWebsiteDataStore report");
    assert!(report.contains("mode=ephemeral-dev"), "{report}");
    assert!(report.contains("actual_identifier=none"), "{report}");
    assert!(report.contains("persistent=false"), "{report}");
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn boot_uuid_from_log(log: &str) -> String {
    log.lines()
        .find_map(|line| line.strip_prefix("KELD_KEL135_LIFECYCLE boot_uuid_hex="))
        .expect("real current kern.bootsessionuuid evidence")
        .to_owned()
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn system_boot_uuid_hex() -> String {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.bootsessionuuid"])
        .output()
        .expect("read independent current boot UUID");
    assert!(output.status.success(), "sysctl boot UUID query failed");
    let value = std::str::from_utf8(&output.stdout)
        .expect("sysctl boot UUID is UTF-8")
        .trim()
        .replace('-', "")
        .to_ascii_lowercase();
    assert_eq!(value.len(), 32, "sysctl must return one canonical UUID");
    assert!(value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    value
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn system_boot_time_seconds() -> u64 {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["kern.boottime"])
        .output()
        .expect("read independent kernel boot time");
    assert!(
        output.status.success(),
        "sysctl boottime failed: {output:?}"
    );
    let report = String::from_utf8(output.stdout).expect("sysctl boottime is UTF-8");
    report
        .split_once("sec = ")
        .and_then(|(_, suffix)| {
            suffix
                .split(|character: char| !character.is_ascii_digit())
                .next()
        })
        .and_then(|value| value.parse::<u64>().ok())
        .expect("sysctl boottime has a seconds field")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_signed_purge_report(app: &Path, support_root: &Path) -> String {
    let output = Command::new(signed_host_executable(app))
        .arg("--keld-profile-purge-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", support_root)
        .output()
        .expect("launch signed exact-identity purge process");
    assert!(
        output.status.success(),
        "signed exact-identity purge failed: {output:?}"
    );
    String::from_utf8(output.stdout).expect("purge report is UTF-8")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_signed_purge_crash_probe(app: &Path, support_root: &Path) -> Output {
    Command::new(signed_host_executable(app))
        .arg("--keld-profile-purge-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", support_root)
        .env("KELD_PROFILE_TEST_PURGE_CRASH_AFTER_CALLBACK", "1")
        .output()
        .expect("launch purge recovery interruption fixture")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_signed_purge_phase_crash_probe(
    app: &Path,
    support_root: &Path,
    phase: &str,
) -> Output {
    Command::new(signed_host_executable(app))
        .arg("--keld-profile-purge-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", support_root)
        .env("KELD_PROFILE_TEST_PURGE_CRASH_AFTER_PHASE", phase)
        .output()
        .expect("launch fsynced purge phase interruption fixture")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_signed_store_presence_report(app: &Path) -> String {
    let output = Command::new(signed_host_executable(app))
        .arg("--keld-profile-presence-fixture-v1")
        .output()
        .expect("enumerate signed app's exact WebKit store registry");
    assert!(
        output.status.success(),
        "signed store enumeration failed: {output:?}"
    );
    String::from_utf8(output.stdout).expect("store presence report is UTF-8")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn webkit_version() -> String {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args([
            "-c",
            "Print:CFBundleVersion",
            "/System/Library/Frameworks/WebKit.framework/Versions/A/Resources/Info.plist",
        ])
        .output()
        .expect("read system WebKit framework version");
    assert!(output.status.success(), "read WebKit version: {output:?}");
    String::from_utf8(output.stdout)
        .expect("WebKit version is UTF-8")
        .trim()
        .to_owned()
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_signed_identity_report(app: &Path) -> std::collections::BTreeMap<String, String> {
    let output = Command::new(signed_host_executable(app))
        .arg("--keld-profile-identity-fixture-v1")
        .output()
        .expect("run signed current-process identity probe");
    parse_signed_identity_report(output)
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn parse_signed_identity_report(
    output: Output,
) -> std::collections::BTreeMap<String, String> {
    assert!(
        output.status.success(),
        "signed identity probe failed: {output:?}"
    );
    let report = String::from_utf8(output.stdout).expect("signed identity report is UTF-8");
    let line = report
        .lines()
        .find(|line| line.starts_with("KELD_KEL135_SIGNED_IDENTITY "))
        .expect("validated signed identity report marker");
    let fields: std::collections::BTreeMap<String, String> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|field| {
            field
                .split_once('=')
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
        })
        .collect();
    assert_eq!(
        fields
            .get("signature_validated_before_identity_read")
            .map(String::as_str),
        Some("true")
    );
    fields
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn sw_vers_value(key: &str) -> String {
    let output = Command::new("/usr/bin/sw_vers")
        .arg(key)
        .output()
        .expect("read current macOS version");
    assert!(output.status.success(), "sw_vers {key} failed: {output:?}");
    String::from_utf8(output.stdout)
        .expect("sw_vers output is UTF-8")
        .trim()
        .to_owned()
}
