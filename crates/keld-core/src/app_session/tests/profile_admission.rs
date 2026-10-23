#[test]
fn renderer_profile_boot_uses_schema_three_without_repurposing_facade_schema_two() {
    let bytes = String::from_utf8(valid_boot("src/main.ts", "index.html"))
        .expect("existing independent fixture bytes");
    let version_three = bytes.replace("\"schema\":1", "\"schema\":3");
    assert_ne!(
        version_three, bytes,
        "fixture must actually exercise version3"
    );
    let parsed = parse_boot_bytes(version_three.as_bytes()).expect("renderer/profile schema3");
    assert_eq!(parsed.renderer, Path::new("index.html"));
    for unsupported in [2, 4, 5, 255] {
        let input = bytes.replace("\"schema\":1", &format!("\"schema\":{unsupported}"));
        let error = must_err(
            parse_boot_bytes(input.as_bytes()),
            "not an implemented renderer version",
        );
        assert_eq!(error.code(), "KELD-CORE-035", "{unsupported}");
    }
}

#[test]
fn compiled_boot_positional_arrays_cannot_select_a_mode_or_profile_contract() {
    let input = format!(
        r#"[1,"Fixture","src/main.ts","index.html",{{"file":"keld.permissions.jsonc","content_sha256":"{DIGEST}"}},{{}}]"#
    );
    let error = must_err(
        parse_boot_bytes(input.as_bytes()),
        "compiled boot must be an object",
    );
    assert_eq!(error.code(), "KELD-CORE-035");
}

#[test]
fn renderer_profile_boot_keeps_closed_nested_object_and_duplicate_boundaries() {
    let valid =
        String::from_utf8(valid_boot("src/main.ts", "index.html")).expect("independent fixture");
    for version in [1, 3] {
        let bytes = valid.replace("\"schema\":1", &format!("\"schema\":{version}"));
        for input in [
            format!(
                r#"{{"schema":{version},"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":["keld.permissions.jsonc","{DIGEST}"]}}"#
            ),
            bytes.replace("\"schema\":", "\"schema\":1,\"schema\":"),
            bytes.replace("\"renderer\":\"index.html\"", "\"renderer\":null"),
            format!("{bytes} {{}}"),
        ] {
            assert_ne!(input, bytes, "every negative input must change the fixture");
            assert_eq!(
                must_err(parse_boot_bytes(input.as_bytes()), "closed boot input").code(),
                "KELD-CORE-035"
            );
        }
    }
}

#[test]
fn verified_profile_preflight_checks_both_version_and_entry_before_admission() {
    use keld_guard::ProfileState;
    use sha2::{Digest as _, Sha256};

    let root = tempfile::tempdir().expect("owned policy files");
    for (name, bytes, explicit) in [
        ("default", b"{}".as_slice(), false),
        ("legacy", br#"{"profile":"legacy"}"#.as_slice(), true),
        ("strict", br#"{"profile":"strict"}"#.as_slice(), true),
    ] {
        let path = root.path().join(name);
        std::fs::write(&path, bytes).expect("owned exact policy");
        let verified = load_verified_manifest(
            File::open(&path).expect("retained file"),
            path,
            Sha256::digest(bytes).into(),
        )
        .expect("real digest-verified policy");
        let expected_schema = if explicit { 3 } else { 1 };
        for schema in [1, 2, 3, 4, 255] {
            for diagnostic in [false, true] {
                let result = super::profile_preflight::validate_renderer_profile(
                    schema, &verified, diagnostic,
                );
                if schema != expected_schema || (explicit && diagnostic) {
                    assert_eq!(
                        result.expect_err("preflight mismatch").code(),
                        "KELD-CORE-035",
                        "{name}/{schema}/{diagnostic}"
                    );
                } else if !explicit {
                    assert_eq!(
                        result.expect("existing no-profile behavior"),
                        ProfileState::Unverified
                    );
                } else if !cfg!(target_os = "macos") {
                    assert_eq!(
                        result.expect_err("unsupported explicit target").code(),
                        "KELD-CORE-034"
                    );
                } else if name == "strict" {
                    let error = result.expect_err("known-uncontained Strict refusal");
                    assert_eq!(error.code(), "KELD-GUARD008");
                    assert!(
                        std::error::Error::source(&error).is_some(),
                        "preserve typed Guard cause"
                    );
                } else {
                    assert_eq!(
                        result.expect("explicit Mac primary decision"),
                        ProfileState::Legacy
                    );
                }
            }
        }
    }
}

#[test]
fn selected_profile_preflight_never_reopens_a_replaced_policy_path() {
    use sha2::{Digest as _, Sha256};

    let root = tempfile::tempdir().expect("owned policy");
    let path = root.path().join("policy.jsonc");
    let bytes = br#"{"profile":"legacy"}"#;
    std::fs::write(&path, bytes).expect("initial policy");
    let verified = load_verified_manifest(
        File::open(&path).expect("retained file"),
        path.clone(),
        Sha256::digest(bytes).into(),
    )
    .expect("actual initial verified bytes");
    std::fs::rename(&path, root.path().join("old-policy.jsonc"))
        .expect("replace only path identity");
    std::fs::write(&path, br#"{"profile":"strict"}"#).expect("new source request");
    assert_eq!(
        verified.verified_sha256(),
        <[u8; 32]>::from(Sha256::digest(bytes))
    );
    let result = super::profile_preflight::validate_renderer_profile(3, &verified, false);
    if cfg!(target_os = "macos") {
        assert_eq!(
            result.expect("retained selection"),
            keld_guard::ProfileState::Legacy
        );
    } else {
        assert_eq!(
            result.expect_err("unsupported target stays refused").code(),
            "KELD-CORE-034"
        );
    }
}
