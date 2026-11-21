//! Public application-ID syntax admission for packaging callers.
//! Parsing a name is not publisher authentication or profile authorization.

use keld_wv::profile::{ProfileErrorKind, validate_canonical_app_id};

#[test]
fn canonical_app_id_export_accepts_normal_and_maximum_length_names() {
    let maximum = format!("app.{}", "x".repeat(251));
    for id in [
        "a",
        "com.example.drawio",
        "com.example.editor-7",
        "app9.package-2",
        maximum.as_str(),
    ] {
        let validated = validate_canonical_app_id(id);
        assert_eq!(validated, Ok(()), "valid package identifier {id:?}");
    }
}

#[test]
fn canonical_app_id_export_refuses_invalid_names_with_typed_error() {
    let excessive = "a".repeat(256);
    for id in [
        "",
        ".app",
        "app.",
        "com..app",
        "com.-app",
        "com.app-",
        "Com.app",
        "com.app_name",
        "com/app",
        "com.%2e%2e",
        "com.é",
        excessive.as_str(),
    ] {
        let error = validate_canonical_app_id(id).expect_err("noncanonical name must refuse");
        assert_eq!(
            error.kind(),
            ProfileErrorKind::InvalidAppId,
            "wrong typed refusal for {id:?}"
        );
        assert!(
            error.to_string().starts_with("KELD-WV-009:"),
            "unregistered error for {id:?}: {error}"
        );
    }
}
