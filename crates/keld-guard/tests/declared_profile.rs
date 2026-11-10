//! KEL-78 explicit-profile syntax through the existing public JSONC boundary.

use keld_guard::{DeclaredProfile, ManifestError, ProfileState, admit_uncontained, parse_manifest};

#[test]
fn known_uncontained_admission_never_promotes_absence_or_starts_strict() {
    for (input, requested, expected) in [
        ("{}", ProfileState::Unverified, ProfileState::Unverified),
        (
            r#"{"profile":"legacy"}"#,
            ProfileState::Legacy,
            ProfileState::Legacy,
        ),
    ] {
        let manifest = parse_manifest(input).expect("valid request");
        let actual = manifest
            .declared_profile()
            .map_or(ProfileState::Unverified, ProfileState::from);
        assert_eq!(actual, requested);
        assert_eq!(
            admit_uncontained(actual).expect("uncontained policy decision"),
            expected
        );
    }
    let error = admit_uncontained(ProfileState::Strict).expect_err("Strict cannot launch here");
    assert_eq!(error.code(), "KELD-GUARD008");
    assert!(error.to_string().contains("Do not start"));
}

#[test]
fn explicit_profile_null_is_not_an_absent_declaration() {
    let error =
        parse_manifest(r#"{"profile":null}"#).expect_err("null cannot select default policy");
    assert!(matches!(error, ManifestError::Parse { .. }), "{error:?}");
    assert_eq!(error.code(), "KELD-GUARD005");
}

#[test]
fn explicit_profile_values_are_exact_strings_not_coerced() {
    for value in [
        "false",
        "true",
        "0",
        "1",
        "[]",
        "{}",
        "\"\"",
        r#""Legacy""#,
        r#"" strict""#,
        r#""strict ""#,
        r#""unverified""#,
        r#""STRICT""#,
    ] {
        let input = format!(r#"{{"profile":{value}}}"#);
        let error = parse_manifest(&input).expect_err("invalid declared profile must be refused");
        assert!(
            matches!(error, ManifestError::Parse { .. }),
            "{input}: {error:?}"
        );
        assert_eq!(error.code(), "KELD-GUARD005", "{input}");
    }
}

#[test]
fn explicit_profile_enum_object_syntax_is_not_authoring_syntax() {
    for value in [r#"{"strict":null}"#, r#"{"legacy":null}"#] {
        let input = format!(r#"{{"profile":{value}}}"#);
        let error = parse_manifest(&input).expect_err("only a string may declare the profile");
        assert!(
            matches!(error, ManifestError::Parse { .. }),
            "{input}: {error:?}"
        );
    }
}

#[test]
fn declared_profile_accessor_preserves_absence_and_exact_decoded_request() {
    for (input, expected) in [
        ("{}", None),
        (r#"{"profile":"strict"}"#, Some(DeclaredProfile::Strict)),
        (r#"{"profile":"legacy"}"#, Some(DeclaredProfile::Legacy)),
        (
            r#"{"pro\u0066ile":"le\u0067acy","app":{},"unknown":true}"#,
            Some(DeclaredProfile::Legacy),
        ),
        (
            "{/* explicit selection */\"profile\":\"strict\"// comment\n}",
            Some(DeclaredProfile::Strict),
        ),
    ] {
        assert_eq!(
            parse_manifest(input)
                .expect("valid manifest")
                .declared_profile(),
            expected
        );
    }
    assert_eq!(
        ProfileState::from(DeclaredProfile::Strict),
        ProfileState::Strict
    );
    assert_eq!(
        ProfileState::from(DeclaredProfile::Legacy),
        ProfileState::Legacy
    );
}

#[test]
fn misplaced_profile_is_rejected_at_each_reserved_location() {
    for input in [
        r#"{"app":{"profile":"legacy"}}"#,
        r#"{"app":{"profile":null}}"#,
        r#"{"windows":{"main":{"profile":"strict"}}}"#,
        r#"{"roles":{"primary":{"profile":"legacy"}}}"#,
        r#"{"roles":{"primary":{"pro\u0066ile":false}}}"#,
        r#"{"profile":"legacy","windows":{"main":{"profile":"strict"}}}"#,
    ] {
        let error = parse_manifest(input).expect_err("per-location fallback is forbidden");
        assert!(
            matches!(error, ManifestError::Parse { .. }),
            "{input}: {error:?}"
        );
        assert_eq!(error.code(), "KELD-GUARD005", "{input}");
    }
}

#[test]
fn positional_arrays_cannot_act_as_permissions_objects() {
    for input in ["[]", "[{}]", r#"[{},"legacy"]"#] {
        let error = parse_manifest(input).expect_err("the manifest must be a JSON object");
        assert!(
            matches!(error, ManifestError::Parse { .. }),
            "{input}: {error:?}"
        );
        assert_eq!(error.code(), "KELD-GUARD005", "{input}");
    }
}

#[test]
fn duplicate_decoded_profile_keys_stay_ambiguous() {
    for input in [
        r#"{"profile":"legacy","profile":"legacy"}"#,
        r#"{"profile":"legacy","profile":"strict"}"#,
        r#"{"profile":"legacy","pro\u0066ile":"strict"}"#,
    ] {
        let error = parse_manifest(input).expect_err("no first/last-key-wins policy");
        assert!(
            matches!(error, ManifestError::Parse { .. }),
            "{input}: {error:?}"
        );
        assert_eq!(error.code(), "KELD-GUARD005", "{input}");
    }
}

#[test]
fn unrelated_profile_names_and_missing_declaration_remain_accepted() {
    for input in [
        "{}",
        r#"{"sandbox":false,"appSandbox":"off"}"#,
        r#"{"windows":{"profile":{}},"roles":{"profile":{}}}"#,
        r#"{"app":{"auth":{"profile":"provider-private"}}}"#,
        r#"{"unknown":{"profile":"not-a-security-declaration"}}"#,
    ] {
        parse_manifest(input).expect("unrelated fields and identifiers must not become overrides");
    }
}
