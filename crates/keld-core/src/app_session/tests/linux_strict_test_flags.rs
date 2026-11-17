// Offer every name: exactly the reviewed value-only flags may cross
// `--clearenv`, so growing the list fails here and forces that review.
#[cfg(all(target_os = "linux", target_arch = "x86_64", debug_assertions))]
#[test]
fn linux_strict_forwards_exactly_the_value_only_test_flags() {
    use std::ffi::OsString;
    let forwarded =
        super::linux_strict_test_flags(|key| Some(OsString::from(format!("{key}-value"))));
    let keys: Vec<&str> = forwarded.iter().map(|(key, _)| *key).collect();
    assert_eq!(
        keys,
        ["KELD_T2_EXIT_ON_LINK_EOF", "KELD_T3_EXIT_AFTER_HELLO"]
    );
    for named_object in [
        "KELD_T1B_CONTROL",
        "KELD_T3_PRE_READY_MARKER",
        "KELD_T3_SUPERVISOR_ATTEMPTS",
        "KELD_T4_GENERATION_MARKER",
    ] {
        assert!(
            !keys.contains(&named_object),
            "{named_object} names an OS object"
        );
    }
    for (key, value) in &forwarded {
        assert_eq!(*value, OsString::from(format!("{key}-value")));
    }
    assert_eq!(
        super::linux_strict_test_flags(|_| None),
        [] as [(&str, OsString); 0]
    );
}

/// The inherited opt-in is a value, never a filename. Even an unrelated
/// same-user 0600 file cannot become the supervisor's append destination.
#[cfg(all(target_os = "linux", target_arch = "x86_64", debug_assertions))]
#[test]
fn stage_debug_witness_rejects_external_private_file_and_preexisting_hardlink() {
    use std::ffi::OsStr;
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().expect("private stage root");
    let unrelated = root.path().join("unrelated-private.log");
    std::fs::write(&unrelated, b"keep this data").expect("seed unrelated file");
    std::fs::set_permissions(&unrelated, std::fs::Permissions::from_mode(0o600))
        .expect("private unrelated file");

    assert!(
        super::create_stage_debug_attempt_witness(root.path(), Some(unrelated.as_os_str()))
            .is_err(),
        "the inherited value must never select arbitrary same-user files"
    );
    assert_eq!(
        std::fs::read(&unrelated).expect("read unrelated"),
        b"keep this data"
    );

    let stage_witness = root.path().join(".keld-debug-primary-attempts");
    std::fs::hard_link(&unrelated, &stage_witness).expect("preplant same-user hardlink");
    assert!(
        super::create_stage_debug_attempt_witness(root.path(), Some(OsStr::new("1"))).is_err(),
        "exclusive creation must refuse even a same-user private hardlink"
    );
    assert_eq!(
        std::fs::read(&unrelated).expect("read unrelated"),
        b"keep this data"
    );
    std::fs::remove_file(&stage_witness).expect("remove planted link");
    assert_eq!(
        super::create_stage_debug_attempt_witness(root.path(), Some(OsStr::new("1")))
            .expect("create fresh stage-owned witness"),
        Some(stage_witness.clone())
    );
    let metadata = std::fs::metadata(&stage_witness).expect("witness metadata");
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    assert!(
        super::create_stage_debug_attempt_witness(root.path(), Some(OsStr::new("1"))).is_err(),
        "the same stage path cannot be recreated or overwritten"
    );
}
