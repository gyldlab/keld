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
