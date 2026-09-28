//! One signed package-purge fixture invocation and independent success receipt.

use std::process::Command;

pub(crate) fn run_signed_purge_fixture(
    signed_purge: &std::ffi::OsStr,
    crash_after_prepared: bool,
) -> std::process::Output {
    let mut command = Command::new(signed_purge);
    command.args([
        "app_session::tests::kel135_signed_package_purge_acceptance_fixture",
        "--ignored",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ]);
    if crash_after_prepared {
        command.env("KELD_KEL135_PURGE_CRASH_AFTER_PREPARED", "1");
    } else {
        command.env_remove("KELD_KEL135_PURGE_CRASH_AFTER_PREPARED");
    }
    command
        .output()
        .expect("run signed authenticated package-purge fixture")
}

pub(crate) fn assert_signed_purge_success(output: std::process::Output) {
    let stdout = String::from_utf8(output.stdout).expect("signed purge stdout UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("signed purge stderr UTF-8");
    assert!(
        output.status.success(),
        "signed package purge failed with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains("KELD_KEL135_SIGNED_PURGE profile_namespace="),
        "signed package purge omitted its identity receipt: {stdout}"
    );
}
