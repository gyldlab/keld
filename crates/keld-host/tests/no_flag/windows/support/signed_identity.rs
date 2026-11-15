//! Existing signed fixture namespace observation and selector owner.

use std::process::Command;

pub(crate) fn signed_fixture_profile_namespace(
    signed_identity: &std::ffi::OsStr,
    carrier: Option<&std::ffi::OsStr>,
) -> String {
    let mut command = Command::new(signed_identity);
    command.env_remove("KELD_KEL135_CARRIER_UNDER_TEST");
    if let Some(carrier) = carrier {
        command.env("KELD_KEL135_CARRIER_UNDER_TEST", carrier);
    }
    let output = command
        .args([
            "app_session::tests::kel135_signed_package_acceptance_fixture",
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("run signed identity fixture");
    let stdout = String::from_utf8(output.stdout).expect("signed identity stdout UTF-8");
    assert!(
        output.status.success(),
        "signed identity fixture failed: {stdout}"
    );
    stdout
        .lines()
        .find_map(|line| line.split("profile_namespace=").nth(1))
        .map(str::to_owned)
        .expect("signed identity fixture omitted profile namespace")
}
