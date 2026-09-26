use crate::support::PROCESS_DEADLINE;
use crate::support::profile_evidence::run_signed_purge_report;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) struct MacProfilePurgeCleanup {
    pub(crate) apps: Vec<PathBuf>,
    pub(crate) support_root: PathBuf,
    pub(crate) fixture_root: PathBuf,
    pub(crate) armed: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl Drop for MacProfilePurgeCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut all_purged = true;
            for app in &self.apps {
                let purged = std::panic::catch_unwind(|| {
                    run_signed_purge_report(app, &self.support_root).contains("store_absent=true")
                })
                .unwrap_or(false);
                all_purged &= purged;
            }
            if !all_purged {
                return false;
            }
            fs::remove_dir_all(&self.fixture_root).is_ok()
        }));
        match cleanup {
            Ok(true) => eprintln!(
                "KELD_KEL135_MACOS_MEDIA_FAILURE_CLEANUP exact_purge=true fixture_removed=true"
            ),
            _ => eprintln!(
                "KELD_KEL135_MACOS_MEDIA_FAILURE_CLEANUP incomplete=true retained_root={}",
                self.fixture_root.display()
            ),
        }
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn valid_macos_codesign_hashes() -> Vec<String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .expect("query local code-signing identities");
    assert!(
        output.status.success(),
        "security find-identity failed: {output:?}"
    );
    String::from_utf8(output.stdout)
        .expect("valid signing identity output is UTF-8")
        .lines()
        .filter(|line| !line.contains("CSSMERR_"))
        .filter_map(|line| {
            let rest = line.split_once(") ")?.1;
            let candidate = rest.split_whitespace().next()?;
            (candidate.len() == 40
                && candidate
                    .as_bytes()
                    .iter()
                    .all(|byte| byte.is_ascii_hexdigit()))
            .then(|| candidate.to_owned())
        })
        .collect()
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn build_signed_profile_app(
    stage_root: &Path,
    app_parent: &Path,
    app_name: &str,
    bundle_id: &str,
    signer: &str,
) -> PathBuf {
    let app = app_parent.join(format!("{app_name}.app"));
    fs::create_dir(&app).expect("create signed host fixture directory");
    fs::set_permissions(&app, fs::Permissions::from_mode(0o700))
        .expect("protect signed host fixture directory");
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos).expect("create signed app executable directory");
    fs::set_permissions(&contents, fs::Permissions::from_mode(0o700))
        .expect("protect signed app contents directory");
    fs::set_permissions(&macos, fs::Permissions::from_mode(0o700))
        .expect("protect signed app executable directory");
    fs::copy(stage_root.join("keld-host"), macos.join("keld-host"))
        .expect("copy KEL-135 host executable into app bundle");
    let signed_executable = signed_host_executable(&app);
    fs::set_permissions(&signed_executable, fs::Permissions::from_mode(0o700))
        .expect("make signed app executable runnable");
    let info = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>CFBundleExecutable</key><string>keld-host</string><key>CFBundleIdentifier</key><string>{bundle_id}</string><key>CFBundleName</key><string>{app_name}</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>1</string><key>CFBundleShortVersionString</key><string>1.0</string><key>NSCameraUsageDescription</key><string>KEL-135 acceptance fixture validates saved camera grant isolation.</string><key>NSMicrophoneUsageDescription</key><string>KEL-135 acceptance fixture validates saved microphone grant isolation.</string></dict></plist>"
    );
    fs::write(contents.join("Info.plist"), info).expect("write signed app bundle metadata");
    let signature = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", signer, "--timestamp=none"])
        .arg("--identifier")
        .arg(bundle_id)
        .arg(&app)
        .output()
        .expect("sign KEL-135 host fixture app");
    assert!(signature.status.success(), "codesign failed: {signature:?}");
    let verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&app)
        .output()
        .expect("verify KEL-135 host app");
    assert!(
        verification.status.success(),
        "signed host executable did not verify: {verification:?}"
    );
    app
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn signed_host_executable(app: &Path) -> PathBuf {
    app.join("Contents/MacOS/keld-host")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn configure_profile_media_mode(
    command: &mut Command,
    mode: Option<&str>,
    phase: &str,
    address: &str,
    log_name: &str,
) {
    match mode {
        Some("allow" | "allow-reuse") => {
            command.env("KELD_PROFILE_TEST_MEDIA_SEED_ALLOW", "1");
            if mode == Some("allow-reuse") {
                eprintln!(
                    "KELD_KEL135_MEDIA_ALLOW_READY phase={phase} action=if-macOS-TCC-prompts-click-Allow deadline_seconds=120"
                );
            }
        }
        Some("prompt" | "prompt-reuse") => {
            if let Some(kind) = phase.strip_prefix("media-seed-") {
                eprintln!(
                    "KELD_KEL135_MEDIA_PROMPT_READY kind={kind} action=click-Allow-in-profile-fixture-window deadline_seconds=120"
                );
            } else {
                assert!(
                    phase.starts_with("query-"),
                    "unexpected prompt fixture phase"
                );
            }
            command
                .env("KELD_PROFILE_TEST_MEDIA_SEED_ALLOW", "1")
                .env("KELD_PROFILE_TEST_MEDIA_SEED_PROMPT", "1");
        }
        None => {}
        Some(_) => panic!("unknown KEL-135 media fixture mode"),
    }
    if matches!(mode, Some("prompt-reuse" | "allow-reuse")) {
        command.env(
            "KELD_PROFILE_FIXTURE_SECOND_URL",
            format!("http://{address}/media-nonce-reuse?run={log_name}"),
        );
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn refresh_signed_profile_app(stage_root: &Path, app: &Path, signer: &str) {
    let executable = signed_host_executable(app);
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("make signed reboot fixture executable replaceable");
    fs::copy(stage_root.join("keld-host"), &executable)
        .expect("update reboot fixture executable with the current Keld build");
    let signature = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", signer, "--timestamp=none"])
        .arg(app)
        .output()
        .expect("re-sign reboot fixture with its same code identity");
    assert!(
        signature.status.success(),
        "re-sign fixture failed: {signature:?}"
    );
    let verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .output()
        .expect("verify refreshed reboot fixture signature");
    assert!(
        verification.status.success(),
        "refreshed fixture signature failed: {verification:?}"
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn spawn_profile_host(
    executable: PathBuf,
    support_root: &Path,
    address: &str,
    phase: &str,
    log_name: &str,
) -> Child {
    let log = fs::File::create(support_root.join(format!("{log_name}.log")))
        .expect("create profile evidence log");
    let stderr = log.try_clone().expect("clone evidence log");
    Command::new(executable)
        .arg("--keld-profile-webview-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", support_root)
        .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
        .env_remove("KELD_PROFILE_TEST_BOOT_UUID")
        .env_remove("KELD_PROFILE_FIXTURE_FATAL_ON_STDIN")
        .env(
            "KELD_PROFILE_FIXTURE_URL",
            format!("http://{address}/{phase}?run={log_name}"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("launch signed profile owner")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn stop_profile_host(child: &mut Child) {
    drop(child.stdin.take());
    let deadline = Instant::now() + PROCESS_DEADLINE;
    loop {
        match child.try_wait().expect("wait for signed profile owner") {
            Some(status) => {
                assert!(status.success(), "signed profile owner failed: {status}");
                return;
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            None => {
                let _ = child.kill();
                let status = child.wait().expect("reap stuck signed profile owner");
                panic!("signed profile owner did not stop cleanly: {status}");
            }
        }
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn kill_profile_host(child: &mut Child) -> std::process::ExitStatus {
    child.kill().expect("SIGKILL active profile host");
    child.wait().expect("reap crashed profile host")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn path_text(path: &Path) -> &str {
    path.to_str().expect("macOS test fixture path is UTF-8")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn copy_profile_fixture_tree(source: &Path, destination: &Path) {
    for entry in fs::read_dir(source).expect("list signed fixture source") {
        let entry = entry.expect("signed fixture directory entry");
        let source = entry.path();
        let target = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source).expect("inspect signed fixture source");
        if metadata.file_type().is_symlink() {
            panic!("test fixture contains a symlink: {source:?}");
        }
        if metadata.is_dir() {
            fs::create_dir(&target).expect("create signed fixture directory");
            fs::set_permissions(&target, fs::Permissions::from_mode(0o700))
                .expect("protect signed fixture directory");
            copy_profile_fixture_tree(&source, &target);
        } else if metadata.is_file() {
            fs::copy(&source, &target).expect("copy signed fixture file");
            fs::set_permissions(&target, metadata.permissions())
                .expect("preserve staged fixture file permissions");
        } else {
            panic!("unsupported signed fixture node: {source:?}");
        }
    }
}
