use crate::support::profile_evidence::parse_signed_identity_report;
use crate::support::profile_evidence::run_signed_purge_report;
use crate::support::signed_app::path_text;
use crate::support::signed_app::signed_host_executable;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) struct MacSecondUserPreflightCleanup {
    pub(crate) username: String,
    pub(crate) user_fixture_root: PathBuf,
    pub(crate) armed: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl Drop for MacSecondUserPreflightCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let removed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_as_local_user(
                &self.username,
                "/bin/rm",
                &["-rf", "--", path_text(&self.user_fixture_root)],
                &[],
            )
            .status
            .success()
        }));
        if matches!(removed, Ok(true)) {
            eprintln!(
                "KELD_KEL135_MACOS_SECOND_USER_SAFE_TO_DELETE_ACCOUNT preflight_no_webkit_store=true"
            );
        } else {
            eprintln!(
                "KELD_KEL135_MACOS_SECOND_USER_CLEANUP_INCOMPLETE preflight_test_root_retained=true"
            );
        }
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) struct MacSecondUserProfileCleanup {
    pub(crate) username: String,
    pub(crate) user_temp: PathBuf,
    pub(crate) user_fixture_root: PathBuf,
    pub(crate) user_profile_root: PathBuf,
    pub(crate) app: PathBuf,
    pub(crate) first_profile_root: PathBuf,
    pub(crate) shared_fixture_root: PathBuf,
    pub(crate) first_profile_purged: bool,
    pub(crate) second_profile_purged: bool,
    pub(crate) armed: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl Drop for MacSecondUserProfileCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let second_ok = if self.second_profile_purged {
                true
            } else {
                let second = run_signed_purge_report_as_user(
                    &self.username,
                    &self.user_temp,
                    &self.user_profile_root,
                    &self.app,
                );
                second.status.success()
                    && String::from_utf8_lossy(&second.stdout).contains("store_absent=true")
            };
            let first_ok = if self.first_profile_purged {
                true
            } else {
                run_signed_purge_report(&self.app, &self.first_profile_root)
                    .contains("store_absent=true")
            };
            let roots_removed = if second_ok && first_ok {
                let user_root_removed = run_as_local_user(
                    &self.username,
                    "/bin/rm",
                    &["-rf", "--", path_text(&self.user_fixture_root)],
                    &[],
                )
                .status
                .success();
                let first_root_removed = if self.first_profile_root.exists() {
                    fs::remove_dir_all(&self.first_profile_root).is_ok()
                } else {
                    true
                };
                let shared_root_removed = if self.shared_fixture_root.exists() {
                    fs::remove_dir_all(&self.shared_fixture_root).is_ok()
                } else {
                    true
                };
                user_root_removed && first_root_removed && shared_root_removed
            } else {
                false
            };
            second_ok && first_ok && roots_removed
        }));
        match cleanup {
            Ok(true) => eprintln!(
                "KELD_KEL135_MACOS_SECOND_USER_SAFE_TO_DELETE_ACCOUNT exact_purge_a=true exact_purge_b=true second_user_test_root_removed=true failure_cleanup=true"
            ),
            _ => eprintln!(
                "KELD_KEL135_MACOS_SECOND_USER_CLEANUP_INCOMPLETE retain_account_and_profile_metadata=true"
            ),
        }
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_signed_identity_report_as_user(
    username: &str,
    user_temp: &Path,
    app: &Path,
) -> std::collections::BTreeMap<String, String> {
    let executable = signed_host_executable(app);
    let environment = [("TMPDIR", user_temp.to_string_lossy().into_owned())];
    let output = run_as_local_user_command(
        username,
        executable.as_os_str(),
        &["--keld-profile-identity-fixture-v1"],
        &environment,
    )
    .output()
    .expect("run signed identity probe as second standard user");
    parse_signed_identity_report(output)
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn make_signed_fixture_readable_by_standard_users(app: &Path) {
    for directory in [
        app,
        &app.join("Contents"),
        &app.join("Contents/MacOS"),
        &app.join("Contents/_CodeSignature"),
    ] {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o755))
            .expect("make signed fixture directories traversable by the test account");
    }
    fs::set_permissions(
        &app.join("Contents/Info.plist"),
        fs::Permissions::from_mode(0o644),
    )
    .expect("make signed app metadata readable by the test account");
    fs::set_permissions(
        &signed_host_executable(app),
        fs::Permissions::from_mode(0o755),
    )
    .expect("make signed host executable readable by the test account");
    let verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .output()
        .expect("re-verify cross-user fixture after setting traversal permissions");
    assert!(
        verification.status.success(),
        "cross-user fixture signature did not survive permission setup: {verification:?}"
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_signed_purge_report_as_user(
    username: &str,
    user_temp: &Path,
    profile_root: &Path,
    app: &Path,
) -> Output {
    let executable = signed_host_executable(app);
    let environment = [
        ("TMPDIR", user_temp.to_string_lossy().into_owned()),
        (
            "KELD_PROFILE_TEST_ROOT",
            profile_root.to_string_lossy().into_owned(),
        ),
    ];
    run_as_local_user_command(
        username,
        executable.as_os_str(),
        &["--keld-profile-purge-fixture-v1"],
        &environment,
    )
    .output()
    .expect("run exact-identity purge as second standard user")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_as_local_user(
    username: &str,
    executable: &str,
    arguments: &[&str],
    environment: &[(&str, String)],
) -> Output {
    run_as_local_user_command(
        username,
        std::ffi::OsStr::new(executable),
        arguments,
        environment,
    )
    .output()
    .expect("run command as the second ordinary macOS user")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_as_local_user_command(
    username: &str,
    executable: &std::ffi::OsStr,
    arguments: &[&str],
    environment: &[(&str, String)],
) -> Command {
    assert!(
        !username.is_empty()
            && username
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
        "local test username has a rejected form"
    );
    let mut command = Command::new("/usr/bin/sudo");
    command.args(["-n", "-H", "-u", username, "--", "/usr/bin/env"]);
    for (name, value) in environment {
        command.arg(format!("{name}={value}"));
    }
    command.arg(executable).args(arguments);
    command
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn account_numeric_value(username: &str) -> u32 {
    let output = Command::new("/usr/bin/id")
        .args(["-u", username])
        .output()
        .expect("read macOS account UID");
    assert!(output.status.success(), "account UID lookup command failed");
    std::str::from_utf8(&output.stdout)
        .expect("account UID is UTF-8")
        .trim()
        .parse()
        .expect("account UID is numeric")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn current_account_numeric_id() -> u32 {
    let output = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .expect("read current macOS account UID");
    assert!(output.status.success(), "current UID lookup command failed");
    std::str::from_utf8(&output.stdout)
        .expect("current UID is UTF-8")
        .trim()
        .parse()
        .expect("current UID is numeric")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn account_groups(username: &str) -> String {
    let output = Command::new("/usr/bin/id")
        .args(["-Gn", username])
        .output()
        .expect("read macOS account groups");
    assert!(
        output.status.success(),
        "account group lookup command failed"
    );
    std::str::from_utf8(&output.stdout)
        .expect("account groups are UTF-8")
        .trim()
        .to_owned()
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn account_home(username: &str) -> PathBuf {
    assert!(
        !username.is_empty()
            && username
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
        "local test username has a rejected form"
    );
    let record = Command::new("/usr/bin/dscl")
        .args([
            ".",
            "-read",
            &format!("/Users/{username}"),
            "NFSHomeDirectory",
        ])
        .output()
        .expect("read macOS account home");
    assert!(
        record.status.success(),
        "account home lookup failed: {record:?}"
    );
    let text = String::from_utf8(record.stdout).expect("account home is UTF-8");
    let path = text
        .trim()
        .strip_prefix("NFSHomeDirectory: ")
        .expect("account record includes its home path");
    PathBuf::from(path)
}
