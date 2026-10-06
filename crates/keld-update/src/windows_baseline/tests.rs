//! Native baseline contracts; privileged scenarios require explicit operator execution.

mod alias;
mod capture;
mod installed_host_operator;
mod locate;
mod locate_operator;
mod machine_staging;
mod machine_uac;
mod per_user;
mod qualification;
mod reader;
mod role;
mod selection;
mod signed_host;
mod substitutions;
mod support;
mod transaction;
mod writer;

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
