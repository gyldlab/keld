#[test]
fn strict_boot_schema_accepts_exact_v1() {
    let parsed = parse_boot_bytes(&valid_boot("src/main.ts", "index.html"))
        .expect("exact schema-v1 descriptor");
    assert_eq!(parsed.name, "Fixture");
    assert_eq!(parsed.entry, Path::new("src/main.ts"));
    assert!(
        matches!(parsed.initial, InitialWindow::Renderer(path) if path == Path::new("index.html"))
    );
    assert_eq!(parsed.permissions_digest.len(), 32);
}

#[cfg(target_os = "macos")]
#[test]
fn approved_schema_two_facade_boot_accepts_no_renderer() {
    let bytes = br#"{"schema":2,"name":"Fixture","entry":"src/main.ts","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#;
    let parsed = parse_boot_bytes(bytes).expect("schema-two facade descriptor without renderer");
    assert_eq!(parsed.name, "Fixture");
    assert_eq!(parsed.entry, Path::new("src/main.ts"));
    assert!(parsed.auth_configuration.is_empty());
}

#[test]
fn schema_one_still_requires_renderer() {
    let bytes = br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#;
    let error = must_err(
        parse_boot_bytes(bytes),
        "schema one still requires renderer",
    );
    assert_eq!(error.code(), "KELD-CORE-035");
    assert!(error.to_string().contains("renderer"), "{error}");
}

#[test]
fn schema_two_strictly_rejects_renderer_auth_profile_unknown_and_duplicate_fields() {
    const FIELDS: &str = r#""name":"Fixture","entry":"src/main.ts","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}"#;
    for injected in [
        r#","renderer":"index.html""#,
        r#","renderer":null"#,
        r#","auth":{}"#,
        r#","profile":"Legacy""#,
        r#","initial_window":"none""#,
        r#","name":"Duplicate""#,
        r#","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}"#,
    ] {
        let payload = format!(r#"{{"schema":2,{FIELDS}{injected}}}"#);
        let error = must_err(parse_boot_bytes(payload.as_bytes()), "strict schema two");
        assert_eq!(error.code(), "KELD-CORE-035", "{payload}");
    }
    let duplicate_schema = format!(r#"{{"schema":2,"schema":1,{FIELDS}}}"#);
    let error = must_err(
        parse_boot_bytes(duplicate_schema.as_bytes()),
        "duplicate discriminator",
    );
    assert_eq!(error.code(), "KELD-CORE-035");
}

#[cfg(target_os = "macos")]
#[test]
fn facade_stage_host_preflight_requires_no_renderer_file() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().expect("private staged root");
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).expect("private stage");
    fs::create_dir(root.path().join("src")).expect("source directory");
    fs::write(root.path().join("src/main.ts"), "export {};\n").expect("entry");
    fs::write(root.path().join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    fs::write(root.path().join(BOOT_FILE), br#"{"schema":2,"name":"Fixture","entry":"src/main.ts","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#).expect("facade boot");
    let selection = validate_from_root(root.path()).expect("exact facade preflight");
    assert!(matches!(
        selection.app.initial_window,
        ValidatedInitialWindow::None
    ));
    assert!(selection.app.auth_configuration.is_empty());
    assert_eq!(selection.app.name, "Fixture");
    assert_eq!(selection.app.entry_path, Path::new("src/main.ts"));
    assert!(!root.path().join("index.html").exists());
}

#[test]
fn strict_schema_v1_boot_carries_auth_without_filtering_profiles() {
    let bytes = br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"fixedRedirect":"http://localhost:53161/callback"},"future-provider":{}}}"#;
    let parsed = parse_boot_bytes(bytes).expect("schema-v1 auth carrier");
    assert_eq!(
        parsed.auth_configuration.len(),
        2,
        "Core must not filter provider keys"
    );
    let entra = parsed
        .auth_configuration
        .get("entra-common")
        .expect("known profile");
    assert_eq!(
        entra.client_id, None,
        "missing clientId remains data until Native post-Allow"
    );
    assert_eq!(
        entra.fixed_redirect.as_deref(),
        Some("http://localhost:53161/callback")
    );
    assert!(parsed.auth_configuration.contains_key("future-provider"));
}

#[test]
fn strict_schema_v1_boot_rejects_unknown_auth_profile_fields() {
    let cases = [
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientID":"operator-client"}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientId":"operator-client","fixedRedirct":"http://localhost:53161/callback"}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"future-provider":{"clientID":"unused"}}}"#.as_slice(),
        ];
    for bytes in cases {
        let error = must_err(
            parse_boot_bytes(bytes),
            "unknown inner auth field must refuse",
        );
        assert_eq!(error.code(), "KELD-CORE-035");
    }
}

#[test]
fn strict_schema_v1_auth_null_fields_preserve_absence() {
    let bytes = br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientId":null,"fixedRedirect":null}}}"#;
    let parsed = parse_boot_bytes(bytes).expect("existing Option null compatibility");
    let profile = parsed
        .auth_configuration
        .get("entra-common")
        .expect("known profile");
    assert!(profile.client_id.is_none());
    assert!(profile.fixed_redirect.is_none());
}

#[test]
fn strict_schema_v1_carries_optional_as_return_data() {
    let mut value = serde_json::json!({
        "schema": 1, "name": "Fixture", "entry": "src/main.ts",
        "renderer": "index.html", "permissions": {
            "file": "keld.permissions.jsonc",
            "content_sha256": "sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"
        }, "auth": { "entra-common": { "asRedirect": "not-a-valid-registration" } }
    });
    for binding in [
        serde_json::json!("not-a-valid-registration"),
        serde_json::Value::Null,
    ] {
        value["auth"]["entra-common"]["asRedirect"] = binding.clone();
        let bytes = serde_json::to_vec(&value).expect("boot fixture bytes");
        let parsed = parse_boot_bytes(&bytes).expect("AS binding is boot data");
        let profile = parsed
            .auth_configuration
            .get("entra-common")
            .expect("profile");
        assert_eq!(profile.as_redirect.as_deref(), binding.as_str());
        assert!(profile.client_id.is_none());
        assert!(profile.fixed_redirect.is_none());
    }
}

#[test]
fn strict_schema_v1_boot_without_auth_defaults_to_empty() {
    let bytes = br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#;
    let parsed = parse_boot_bytes(bytes).expect("legacy schema-v1 boot descriptor");
    assert!(parsed.auth_configuration.is_empty());
}

#[test]
fn strict_schema_v1_boot_rejects_duplicate_auth_shape() {
    let cases = [
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{},"auth":{"entra-common":{}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientId":"first","clientId":"second"}}}"#.as_slice(),
            // The map owner must reject repeated decoded profile keys as well
            // as duplicate struct fields. These are independent raw JSON literals.
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"clientId":"first"},"entra-common":{"clientId":"second"}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{},"entra\u002dcommon":{}}}"#.as_slice(),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"unknown-profile":{},"unknown-profile":{}}}"#.as_slice(),
        ];
    for bytes in cases {
        let error = must_err(parse_boot_bytes(bytes), "duplicate auth shape must fail");
        assert_eq!(error.code(), "KELD-CORE-035");
    }
}

#[cfg(unix)]
#[test]
fn schema_v1_auth_carrier_survives_actual_root_validation() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = tempfile::tempdir().expect("temporary parent");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage root");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("private mode");
    fs::create_dir(root.join("src")).expect("source directory");
    fs::write(root.join("src/main.ts"), b"export {};\n").expect("entry");
    fs::write(root.join("index.html"), b"<p>fixture</p>\n").expect("renderer");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    // Independent literal: this is the exact serialized schema-v1 boot carrier.
    fs::write(
            root.join(BOOT_FILE),
            br#"{"schema":1,"name":"Fixture","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"auth":{"entra-common":{"fixedRedirect":"http://localhost:53161/callback"},"future-provider":{}}}"#,
        )
        .expect("boot descriptor");

    let selection = validate_from_root(&root).expect("validate real staged boot files");
    assert_eq!(selection.app.auth_configuration.len(), 2);
    let entra = selection
        .app
        .auth_configuration
        .get("entra-common")
        .expect("configured profile");
    assert_eq!(
        entra.client_id, None,
        "clientId remains for Native post-Allow handling"
    );
    assert_eq!(
        entra.fixed_redirect.as_deref(),
        Some("http://localhost:53161/callback")
    );
    assert!(
        selection
            .app
            .auth_configuration
            .contains_key("future-provider")
    );
}

#[test]
fn bounded_boot_rejects_zero_limit_plus_one_and_non_utf8() {
    assert!(parse_boot_bytes(&[]).is_err());
    let exact = vec![b' '; MAX_BOOT_BYTES];
    let exact_error = must_err(parse_boot_bytes(&exact), "spaces are not JSON");
    assert_eq!(exact_error.code(), "KELD-CORE-035");
    let over = vec![b' '; MAX_BOOT_BYTES + 1];
    let over_error = must_err(parse_boot_bytes(&over), "64 KiB + 1 must fail");
    assert!(over_error.to_string().contains("64 KiB"), "{over_error}");
    let utf8_error = must_err(parse_boot_bytes(&[0xff]), "non-UTF-8 must fail");
    assert!(utf8_error.to_string().contains("UTF-8"), "{utf8_error}");
}

#[test]
fn duplicate_unknown_version_name_and_permissions_fields_fail_closed() {
    let cases = [
        r#"{"schema":1,"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"x","entry":"a","renderer":"b","unknown":1,"permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":5,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"wrong.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
        r#"{"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"SHA256:CA3D163BAB055381827226140568F3BEF7EAAC187CEBD76878E0B63E9E442356"}}"#,
    ];
    for bytes in cases {
        assert!(
            parse_boot_bytes(bytes.as_bytes()).is_err(),
            "accepted: {bytes}"
        );
    }
}

#[test]
fn kipc_transport_sidecar_admission_skips_absent_and_accepts_regular_file() {
    let temp = tempfile::tempdir().expect("sidecar root");
    let sidecar = temp.path().join("kipc-transport.ts");
    assert!(
        !admit_kipc_transport_sidecar(&sidecar).expect("absent sidecar"),
        "NotFound must allow self-contained entries"
    );
    fs::write(&sidecar, "export {}\n").expect("regular sidecar");
    assert!(
        admit_kipc_transport_sidecar(&sidecar).expect("regular sidecar"),
        "regular file must be eligible to bind"
    );
}

#[test]
fn kipc_transport_sidecar_admission_rejects_directory() {
    let temp = tempfile::tempdir().expect("sidecar root");
    let sidecar = temp.path().join("kipc-transport.ts");
    fs::create_dir(&sidecar).expect("directory sidecar");
    let error = must_err(
        admit_kipc_transport_sidecar(&sidecar),
        "directory sidecar must fail closed",
    );
    assert!(error.to_string().contains("not a regular file"), "{error}");
}

#[test]
fn fs_route_keeps_guard_evaluation_out_of_core_production_source() {
    let source = include_str!("../../app_session.rs");
    let production = source
        .split_once("\n#[cfg(test)]\nmod tests")
        .map(|(production, _)| production)
        .expect("tests module marker");
    let forbidden_dispatch = ["dispatch", "_privileged("].concat();
    let forbidden_evaluate = ["keld_guard::", "evaluate("].concat();
    assert!(
        !production.contains(&forbidden_dispatch),
        "keld-core must route to native rather than call the privileged dispatcher"
    );
    assert!(
        !production.contains(&forbidden_evaluate),
        "keld-core must never become a second guard evaluator"
    );
}

#[test]
fn kipc_transport_sidecar_admission_uses_lstat_not_follow() {
    let source = include_str!("../../app_session.rs");
    let start = source
        .find("fn admit_kipc_transport_sidecar")
        .expect("admission helper");
    let body = source
        .get(start..start.saturating_add(900))
        .expect("admission helper body");
    assert!(
        body.contains("symlink_metadata"),
        "admission must lstat so a sidecar symlink is not followed into --ro-bind"
    );
    assert!(
        !body.contains(".metadata()"),
        "Path::metadata follows symlinks and would reintroduce CWE-59"
    );
    assert!(
        body.contains("is_symlink()"),
        "admission must reject a symlink before readonly_runtime"
    );
}

#[test]
#[cfg(unix)]
fn kipc_transport_sidecar_admission_rejects_symlink_to_regular_file() {
    let temp = tempfile::tempdir().expect("sidecar root");
    let outside = temp.path().join("outside.ts");
    fs::write(&outside, "export const steal = 1;\n").expect("external target");
    let sidecar = temp.path().join("kipc-transport.ts");
    std::os::unix::fs::symlink(&outside, &sidecar).expect("sidecar symlink");
    // `metadata()` follows and would admit this as a regular file — CWE-59.
    assert!(
        fs::metadata(&sidecar).expect("follow").is_file(),
        "negative control: following the symlink sees a regular file"
    );
    let error = must_err(
        admit_kipc_transport_sidecar(&sidecar),
        "symlink sidecar must fail closed before --ro-bind",
    );
    assert!(error.to_string().contains("symbolic link"), "{error}");
}

#[test]
fn portable_relative_paths_reject_normalized_escape_and_empty_components() {
    for path in [
        "",
        ".",
        "..",
        "/abs",
        "a/../b",
        "a/./b",
        "a//b",
        "a/",
        "C:\\boot.ts",
        "C:/boot.ts",
        "\\\\server\\share",
        "a\\b",
    ] {
        let error = must_err(
            parse_boot_bytes(&valid_boot(path, "index.html")),
            "unsafe entry path must fail",
        );
        assert!(error.to_string().contains("entry"), "{path}: {error}");
    }
}

#[test]
#[cfg(target_os = "macos")]
fn validated_root_opens_same_handle_targets_and_rejects_symlink_escape() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    fs::create_dir(root.join("src")).expect("src");
    fs::write(root.join("src/main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join("index.html"), "<p id=fixture>exact</p>\n").expect("renderer");
    fs::write(root.join("keld.permissions.jsonc"), b"{}\n").expect("permissions");
    fs::write(
        root.join("keld.boot.json"),
        valid_boot("src/main.ts", "index.html"),
    )
    .expect("boot");

    let selection = validate_from_root(&root).expect("validated selection");
    assert!(matches!(
        &selection.app.initial_window,
        ValidatedInitialWindow::Renderer(bytes) if bytes == b"<p id=fixture>exact</p>\n"
    ));
    assert_eq!(selection.app.name, "Fixture");

    let mut substituted: serde_json::Value =
        serde_json::from_slice(&valid_boot("src/main.ts", "index.html"))
            .expect("substitute boot JSON");
    substituted["name"] = serde_json::Value::String("Substituted".to_owned());
    fs::write(
        root.join("keld.boot.json"),
        serde_json::to_vec(&substituted).expect("substitute boot bytes"),
    )
    .expect("replace sidecar after selection");

    let outside = temp.path().join("outside.html");
    fs::write(&outside, "outside").expect("outside");
    fs::remove_file(root.join("index.html")).expect("remove renderer");
    symlink(&outside, root.join("index.html")).expect("escape symlink");
    assert_eq!(
        match &selection.app.initial_window {
            ValidatedInitialWindow::Renderer(bytes) => bytes.as_slice(),
            ValidatedInitialWindow::None => panic!("renderer selection became facade"),
        },
        b"<p id=fixture>exact</p>\n",
        "post-selection renderer substitution changed consumed bytes"
    );
    assert_eq!(
        selection.app.name, "Fixture",
        "post-selection sidecar substitution changed owned fields"
    );
    let error = must_err(validate_from_root(&root), "symlink escape must fail");
    assert!(error.to_string().contains("renderer"), "{error}");
}

#[test]
#[cfg(target_os = "macos")]
fn guarded_preflight_preserves_read_error_before_resources() {
    use std::fs::OpenOptions;

    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join("index.html"), "<p>guarded</p>\n").expect("renderer");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");

    let mut selection = validate_from_root(&root).expect("validated selection");
    selection.permissions_file = OpenOptions::new()
        .write(true)
        .open(root.join(PERMISSIONS_FILE))
        .expect("write-only retained handle");
    let before = startup_resource_snapshot();

    let error = run_guarded(selection, |_| -> ! { unreachable!() })
        .expect_err("read failure must fail preflight");
    assert_eq!(error.code(), "KELD-GUARD004");
    assert_eq!(error.resources, before, "preflight advanced app resources");
    let message = error.to_string();
    assert!(message.contains("Check the path"), "{message}");
    assert!(
        message.contains("rebuild the staged boot artifact"),
        "{message}"
    );
    let source = std::error::Error::source(&error).expect("manifest source");
    assert!(source.downcast_ref::<ManifestError>().is_some());
    assert_eq!(
        source.to_string(),
        error.manifest_source.as_ref().unwrap().to_string()
    );
}
