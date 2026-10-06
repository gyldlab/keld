//! AC2: the writer's output from the real unsigned release host is loadable (§7 row 2).

use super::{
    TRUST_E_NOSIGNATURE, WIN_VERIFY_TRUST_REJECTED, env_path, fixture_payload, sha256_hex,
};
use keld_pack::embed_host_identity;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::process::{Command, Stdio};

/// Launched with no dev lease, the embedded release host exits with the KEL-135 identity
/// refusal before any listener, child or window.
#[test]
#[ignore = "needs KELD_PACK_REAL_HOST (release keld-host.exe) and a new KELD_PACK_EMBEDDED_HOST path"]
fn ac2_embedded_release_host_launches_to_the_identity_refusal() {
    let input_path = env_path("KELD_PACK_REAL_HOST");
    let embedded_path = env_path("KELD_PACK_EMBEDDED_HOST");
    let host = std::fs::read(&input_path).expect("read the release host");
    let embedded =
        embed_host_identity(&host, &fixture_payload()).expect("the release host is admissible");
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&embedded_path)
            .expect("KELD_PACK_EMBEDDED_HOST must be a new file in an existing directory");
        file.write_all(&embedded).expect("write the embedded host");
        file.sync_all().expect("flush the embedded host");
    } // The writer handle is closed before launch, so nothing else holds the image.

    // A plain lease-less launch: no dev lease and no launcher start gate.
    let output = Command::new(&embedded_path)
        .current_dir(
            embedded_path
                .parent()
                .expect("the embedded host has a parent"),
        )
        .env_remove("KELD_DEV_LEASE")
        .env_remove("KELD_WINDOWS_LAUNCH_GATE")
        .stdin(Stdio::null())
        .output()
        .expect("CreateProcess must accept the embedded image; a loader rejection fails AC2");
    let stderr = String::from_utf8(output.stderr).expect("host stderr is UTF-8");
    let stdout = String::from_utf8(output.stdout).expect("host stdout is UTF-8");
    println!(
        "KELD_PACK_AC2 input_sha256={} input_bytes={} embedded_sha256={} embedded_bytes={} \
         exit={:?}",
        sha256_hex(&host),
        host.len(),
        sha256_hex(&embedded),
        embedded.len(),
        output.status.code()
    );
    println!("KELD_PACK_AC2 stdout={stdout:?}");
    println!("KELD_PACK_AC2 stderr={stderr:?}");

    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    // WinVerifyTrust reports the embedded image as unsigned rather than as an unknown
    // subject, so the refusal is the KEL-135 identity check, reached with zero startup
    // resources.
    assert!(
        stderr.starts_with(
            "KELD-WV-009: no-flag host failed during Windows authenticated app identity — "
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "{WIN_VERIFY_TRUST_REJECTED}{TRUST_E_NOSIGNATURE:08x}."
        )),
        "{stderr}"
    );
    assert!(
        stderr
            .trim_end()
            .ends_with("[startup-resource-attempts listener=0 child=0 window=0]"),
        "{stderr}"
    );
}
