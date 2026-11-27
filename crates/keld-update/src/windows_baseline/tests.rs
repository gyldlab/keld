//! Native baseline contracts; privileged scenarios require explicit operator execution.

mod alias;
mod capture;
mod claimant;
mod coordinator_image_operator;
mod helper;
mod installed_host_operator;
mod locate;
mod locate_operator;
mod machine_staging;
mod machine_uac;
mod per_user;
mod qualification;
mod reader;
mod recovery_required;
mod recovery_required_operator;
mod repair_source;
mod repository_source_reader;
mod repository_writer_scope;
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

// This synthetic LocalAppData root is ONLY a negative-oracle input for the
// private retained-identity predicate. It is not the missing D-42 worker or
// known-folder issuer and cannot authorize the legacy initializer.
#[test]
fn legacy_v2_wrapper_checks_retained_ancestry_and_both_scaffold_parents() {
    use cap_std::ambient_authority;
    use cap_std::fs::Dir;
    use std::fs;

    use super::initial_permit::legacy_v2_denial::refuse_legacy_v2_roots;

    fn retain_fixture_roots(install: &std::path::Path) -> super::Roots {
        let open = |path: &std::path::Path| {
            Dir::open_ambient_dir(path, ambient_authority())
                .expect("retain actual fixture directory")
        };
        super::Roots {
            trust: trust_for(install),
            ancestors: crate::windows_extraction::open_ancestors(install)
                .expect("retain native installation ancestors"),
            update: open(&install.join("updates")),
            versions: open(&install.join("updates/versions")),
        }
    }

    let fixture = tempfile::tempdir().expect("isolated reserved-root fixture");
    let local_path = fixture.path().join("UserData");
    let namespace = local_path.join("GYLDLAB/Keld");
    let reserved_path = namespace.join("v2");
    let legacy_path = namespace.join("legacy");
    for install in [&reserved_path, &legacy_path] {
        fs::create_dir_all(install.join("updates/versions"))
            .expect("create isolated protected-shape fixture");
    }
    let local = Dir::open_ambient_dir(&local_path, ambient_authority())
        .expect("retain synthetic LocalAppData fixture");
    let reserved = Dir::open_ambient_dir(&reserved_path, ambient_authority())
        .expect("retain original native v2 identity");

    let reserved_roots = retain_fixture_roots(&reserved_path);
    let error = refuse_legacy_v2_roots(&local, &reserved_roots)
        .expect_err("reserved v2 ancestor must refuse through the actual wrapper");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);

    let mut legacy_roots = retain_fixture_roots(&legacy_path);
    refuse_legacy_v2_roots(&local, &legacy_roots)
        .expect("distinct native legacy root must remain eligible for this deny-only predicate");
    // The two scaffold-parent roles must each independently participate in
    // the wrapper; replacing either with the same reserved object must refuse.
    legacy_roots.update = reserved
        .try_clone()
        .expect("retain reserved update sentinel");
    let error = refuse_legacy_v2_roots(&local, &legacy_roots)
        .expect_err("reserved update parent must refuse through wrapper");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);

    let mut legacy_roots = retain_fixture_roots(&legacy_path);
    legacy_roots.versions = reserved;
    let error = refuse_legacy_v2_roots(&local, &legacy_roots)
        .expect_err("reserved versions parent must refuse through wrapper");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(
        !legacy_path.join("updates/bootstrap.lock").exists()
            && !reserved_path.join("updates/bootstrap.lock").exists(),
        "read-only wrapper must leave both independent fixture trees unchanged"
    );
}
