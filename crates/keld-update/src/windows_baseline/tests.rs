//! Native baseline contracts; privileged scenarios require explicit operator execution.

mod alias;
mod machine_staging;
mod qualification;
mod reader;
mod role;
mod substitutions;
mod support;

use support::{baseline, trust_for};

#[test]
fn ordinary_process_cannot_initialize_even_with_verified_package() {
    assert!(
        keld_guard::require_windows_system_token().is_err(),
        "run the ordinary test gate as an ordinary developer, not SYSTEM"
    );
    let empty = tempfile::tempdir().expect("isolated ordinary caller fixture");
    let trust = trust_for(&empty.path().join("install"));
    let verified = baseline(&trust);
    let error =
        super::initialize_windows_baseline(&verified, &empty.path().join("absent.tar"), &trust)
            .expect_err("verified bytes cannot mint installer authority");
    assert!(matches!(
        error,
        crate::UpdateError::Baseline {
            step: "installer authority",
            ..
        }
    ));
    assert!(!trust.installation.install_root.exists());
    assert_eq!(
        std::fs::read_dir(empty.path())
            .expect("unchanged parent")
            .count(),
        0
    );
}
