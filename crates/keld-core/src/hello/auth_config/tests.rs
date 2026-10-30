use super::auth_configuration_from_config_ts;

#[test]
fn static_auth_literal_preserves_credentials_and_redirect() {
    let config = auth_configuration_from_config_ts(
        r#"
            export default {
                name: "Legacy Title",
                /* not the key: auth: { injected: {} } */
                auth: { 'entra-common': {
                    clientId: 'operator-\u0063lient',
                    fixedRedirect: "http://localhost:4321/callback?note={auth}",
                }, },
            } as const;
        "#,
    )
    .expect("static configuration");
    let profile = config.get("entra-common").expect("profile carried");
    assert_eq!(profile.client_id.as_deref(), Some("operator-client"));
    assert_eq!(
        profile.fixed_redirect.as_deref(),
        Some("http://localhost:4321/callback?note={auth}")
    );
}

#[test]
fn static_auth_carries_as_return_without_registration_validation() {
    let config = auth_configuration_from_config_ts(
        "export default { auth: { 'entra-common': { asRedirect: 'not-a-valid-registration' } } };",
    )
    .expect("AS registration semantics remain post-Allow");
    let profile = config.get("entra-common").expect("closed profile");
    assert!(profile.client_id.is_none());
    assert!(profile.fixed_redirect.is_none());
    assert_eq!(
        profile.as_redirect.as_deref(),
        Some("not-a-valid-registration")
    );
}

#[test]
fn static_auth_ignores_comment_string_and_nested_mentions() {
    let config = auth_configuration_from_config_ts(
        r#"
            // auth: { "entra-common": { clientId: "not-config" } }
            export default {
                name: "auth: { pretend: {} }",
                other: { auth: { "entra-common": { clientId: "nested" } } },
            };
        "#,
    )
    .expect("no root auth");
    assert!(config.is_empty());
}

#[test]
fn static_auth_rejects_unknown_profile_fields_at_owning_dto() {
    for source in [
        "export default { auth: { 'entra-common': { clientID: 'operator-client' } } };",
        "export default { auth: { 'future-provider': { clientID: 'unused' } } };",
        "export default { auth: { 'entra-common': { clientId: 'operator-client', fixedRedirct: 'http://localhost:53161/callback' } } };",
    ] {
        let error = auth_configuration_from_config_ts(source)
            .expect_err("unknown inner auth field must refuse");
        assert!(error.to_string().starts_with("KELD-CORE-035"));
        assert!(
            error
                .to_string()
                .contains("invalid auth configuration fields")
        );
    }
}

#[test]
fn static_auth_keeps_unknown_profile_and_missing_client_for_native_owner() {
    let config = auth_configuration_from_config_ts(
            "export default { auth: { 'unknown-provider': { clientId: 'operator' }, 'entra-common': {} } };"
        ).expect("carrier does not filter provider policy");
    assert_eq!(config.len(), 2);
    assert_eq!(
        config
            .get("unknown-provider")
            .expect("unknown carried")
            .client_id
            .as_deref(),
        Some("operator")
    );
    assert!(
        config
            .get("entra-common")
            .expect("missing client preserved")
            .client_id
            .is_none()
    );
}

#[test]
fn static_auth_decodes_quotes_unicode_and_surrogate_pair() {
    let config = auth_configuration_from_config_ts(
        r#"
            export default { "auth": { "entra-common": {
                "clientId": 'a\'b"c\\d\uD83D\uDE00',
            } } }
        "#,
    )
    .expect("actual string decoding");
    assert_eq!(
        config
            .get("entra-common")
            .expect("profile")
            .client_id
            .as_deref(),
        Some("a'b\"c\\d😀")
    );
}

#[test]
fn static_auth_rejects_computed_spread_dynamic_and_malformed_input() {
    for source in [
        "export default { auth: buildAuth() };",
        "export default { auth: { ...settings } };",
        "export default { auth: { ['entra-common']: {} } };",
        "export default { ['auth']: {} };",
        "export default { auth: { 'entra-common': { clientId } } };",
        "export default { auth: { 'entra-common': { clientId: computed } } };",
        "export default { auth: { 'entra-common': { clientId: `template` } } };",
        "export default { auth: {} satisfies AuthConfiguration };",
        "export default { auth: { 'entra-common': { clientId: 'broken } } };",
        "export default { auth: { 'entra-common': { clientId: '\\u12XX' } } };",
        "export default { auth: {}",
        "export default { auth: {} ] };",
    ] {
        let error =
            auth_configuration_from_config_ts(source).expect_err("unsupported input must refuse");
        let rendered = error.to_string();
        assert!(rendered.starts_with("KELD-CORE-035:"), "{rendered}");
        assert!(rendered.contains("literal strings"), "{rendered}");
    }
}

#[test]
fn static_auth_rejects_duplicate_keys_before_canonicalization() {
    for source in [
        "export default { auth: {}, auth: {} };",
        "export default { auth: { 'entra-common': {}, 'entra-common': {} } };",
        "export default { auth: { 'entra-common': { clientId: 'a', clientId: 'b' } } };",
        "export default { auth: { 'entra-common': { clientId: 'a', 'clientId': 'b' } } };",
    ] {
        let error = auth_configuration_from_config_ts(source).expect_err("duplicate must refuse");
        assert!(error.to_string().contains("duplicate"));
    }
}

#[test]
fn static_auth_legacy_templates_and_structural_limits() {
    for source in [
        "export default {}",
        "export default {\nname: \"hello\",\nentry: \"src/main.ts\",\nrenderer: \"index.html\",\n} as const;",
    ] {
        assert!(
            auth_configuration_from_config_ts(source)
                .expect("existing template")
                .is_empty()
        );
    }
    let oversized = format!(
        "export default {{ auth: {{ 'entra-common': {{ clientId: '{}' }} }} }};",
        "x".repeat(64 * 1024)
    );
    assert!(auth_configuration_from_config_ts(&oversized).is_err());
    let deep = format!(
        "export default {{ other: {}0{} }};",
        "[".repeat(200),
        "]".repeat(200)
    );
    assert!(
        auth_configuration_from_config_ts(&deep)
            .expect("unrelated depth remains compatible")
            .is_empty()
    );
}
#[test]
fn absent_auth_preserves_legacy_nonliteral_configurations() {
    for source in [
        "export default {\n name: \"legacy\",\n other: `template ${1 + 2}`,\n};",
        "import settings from './settings';\nconst x = 3;\nexport default {\n name: \"legacy\",\n other: settings,\n};",
        "const x = 6 / 2;\nexport default {\n name: \"legacy\",\n other: /auth: { pretend: {} }/,\n};",
        "export default {\n name: \"legacy\",\n other: 1, other: 2,\n};",
        "if (true) /export default { auth: {} }/.test('x');\nexport default {\n name: \"legacy\",\n};",
        "function helper() { return { auth: {} }; }\nexport default {\n name: \"legacy\",\n};",
    ] {
        assert_eq!(
            super::title_from_config_ts(source).as_deref(),
            Some("legacy")
        );
        assert!(
            auth_configuration_from_config_ts(source)
                .expect("no auth cannot restrict old config")
                .is_empty()
        );
    }
    let large = format!("/*{}*/ export default {{}}", "x".repeat(64 * 1024));
    assert!(
        auth_configuration_from_config_ts(&large)
            .expect("unrelated size")
            .is_empty()
    );
}

#[test]
fn auth_location_skips_templates_regex_division_and_unrelated_duplicate_keys() {
    let source = r"
            import settings from './settings';
            const ratio = {} / 2;
            const pattern = /export default { auth: { fake: {} } }/;
            export default {
                name: `auth: ${ { nested: `auth: ${1}` } }`,
                regex: /auth: { \/ }/,
                ratio: 8 / 2,
                other: 1, other: 2,
                nested: { auth: { fake: {} } },
                auth: { 'entra-common': { clientId: 'real-client' } },
            } as const;
        ";
    let config = auth_configuration_from_config_ts(source).expect("strict slice only");
    assert_eq!(config.len(), 1);
    assert_eq!(
        config
            .get("entra-common")
            .expect("actual root")
            .client_id
            .as_deref(),
        Some("real-client")
    );
}

#[test]
fn computed_auth_prefix_does_not_create_auth_property() {
    let source = "export default {\n name: \"legacy\",\n ['auth' + 'orship']: 'value',\n};";
    assert_eq!(
        super::title_from_config_ts(source).as_deref(),
        Some("legacy")
    );
    assert!(
        auth_configuration_from_config_ts(source)
            .expect("not literal root auth")
            .is_empty()
    );
    let actual = "export default { ['auth' /* key end */]: {} };";
    assert!(auth_configuration_from_config_ts(actual).is_err());
}

#[test]
fn named_auth_accessors_are_unsupported_not_silently_absent() {
    for source in [
        "export default { get auth() { return {}; } };",
        "export default { set auth(value) {} };",
        "export default { get 'auth'() { return {}; } };",
        "export default { get ['auth']() { return {}; } };",
        "export default { auth: {}, get auth() { return {}; } };",
        "export default { get auth() { return {}; }, auth: {} };",
    ] {
        let error = auth_configuration_from_config_ts(source)
            .expect_err("actual root accessor must refuse");
        assert!(error.to_string().starts_with("KELD-CORE-035:"));
    }
    for source in [
        "export default { get: { auth: {} } };",
        "export default { get authorship() { return { auth: {} }; } };",
        "export default { get ['auth' + 'orship']() { return {}; } };",
    ] {
        assert!(
            auth_configuration_from_config_ts(source)
                .expect("unrelated getter/key")
                .is_empty()
        );
    }
}

#[test]
fn config_line_comments_end_at_cr_or_unicode_line_terminator() {
    for end in ["\r", "\r\n", "\u{2028}", "\u{2029}"] {
        let source = format!(
            "export default {{ // root comment{end}auth: {{ // auth comment{end}'entra-common': {{ clientId: 'visible' }} }} }};"
        );
        let config =
            auth_configuration_from_config_ts(&source).expect("line comment cannot hide auth");
        assert_eq!(
            config
                .get("entra-common")
                .expect("profile")
                .client_id
                .as_deref(),
            Some("visible")
        );
    }
}

#[test]
fn trailing_statements_do_not_restrict_static_auth_extraction() {
    let source = "export default { auth: { 'entra-common': { clientId: 'kept' } } }; doSomething(); throw new Error('not executed');";
    let config = auth_configuration_from_config_ts(source)
        .expect("same literal extraction with opaque tail");
    assert_eq!(
        config
            .get("entra-common")
            .expect("profile")
            .client_id
            .as_deref(),
        Some("kept")
    );
}

// GH #588: the manifest is the sole security-profile authority; finding the
// exact decoded root member must precede any auth-value parsing.
#[test]
fn root_profile_is_refused_at_literal_and_method_heads() {
    for source in [
        "export default { profile: 'legacy' };",
        "export default { 'profile': 'strict' };",
        "export default { profile };",
        "export default { profile() {} };",
        "export default { profile<T>() {} };",
        "export default { get profile() { return 'legacy' } };",
        "export default { set profile(value) {} };",
        "export default { async profile() {} };",
        "export default { *profile() {} };",
        "export default { async *profile() {} };",
        "export default { get ['profile']() { return 'legacy' } };",
        "export default { async ['profile']() {} };",
        r"export default { pro\u0066ile: 'legacy' };",
        r"export default { \u{70}rofile: 'legacy' };",
        r"export default { 'pro\u0066ile': 'legacy' };",
        r"export default { 'pro\u{66}ile': 'legacy' };",
        r"export default { 'pro\x66ile': 'legacy' };",
        r"export default { 'pr\ofile': 'legacy' };",
        r#"export default { ["pro\u0066ile"]: 'legacy' };"#,
        "export default { auth: {}, profile: 'legacy' };",
        "export default { profile: 'legacy', auth: {} };",
    ] {
        let error = auth_configuration_from_config_ts(source)
            .expect_err("root security profile must not be silently ignored");
        let rendered = error.to_string();
        assert!(
            rendered.starts_with("KELD-CORE-035:"),
            "{source}: {rendered}"
        );
        assert!(
            rendered.contains("keld.permissions.jsonc"),
            "{source}: {rendered}"
        );
    }
}

#[test]
fn root_initial_window_uses_the_existing_facade_owner_after_profile_composition() {
    for source in [
        "export default { initialWindow: 'none' };",
        r#"export default { "initialWindow": "none" };"#,
    ] {
        assert!(super::initial_window_from_config_ts(source).expect("approved literal facade mode"));
        assert!(auth_configuration_from_config_ts(source).expect("no auth field").is_empty());
    }
    for source in [
        "export default { initialWindow: makeWindow() };",
        r"export default { ['initial\u0057indow']: 'none' };",
        "export default { get initialWindow() { return 'none' } };",
        "export default { async *initialWindow() {} };",
        "export default { auth: {}, initialWindow: 'none' };",
    ] {
        assert!(super::initial_window_from_config_ts(source).is_err(), "{source}");
    }
}

#[test]
fn unrelated_unicode_dynamic_and_nested_keys_stay_opaque() {
    for source in [
        "export default { profileπ: 'legacy', initialWindowTitle: 'none' };",
        "export default { πprofile: 'legacy', profile_: 'legacy' };",
        r"export default { profile\u{0301}: 'legacy' };",
        r"export default { pro\u{66}ileSuffix: 'legacy' };",
        r"export default { 'pro\x66ileSuffix': 'legacy' };",
        r"export default { 'pro\ofile': 'legacy' };",
        r"export default { ['pro' + 'file']: 'computed' };",
        "export default { other: { profile: 'legacy', initialWindow: 'none' } };",
        "export default { get: { profile: 'legacy' }, async: { initialWindow: 'none' } };",
        "export default { get authorship() { return { profile: 'legacy' }; } };",
        "export default { async profiled() { return 'legacy'; } };",
        "export default { *profiled() { return 'legacy'; } };",
        "export default { other: /profile: 'legacy'/.test('x') };",
        r#"export default { name: "profile: 'legacy'; initialWindow: 'none'" };"#,
    ] {
        assert!(
            auth_configuration_from_config_ts(source)
                .expect("unrelated legacy configuration remains unrestricted")
                .is_empty(),
            "{source}"
        );
    }
    // ECMAScript U+FEFF is white space, even inside a property declaration.
    let bom = "export default { profile\u{feff}: 'legacy' };";
    let error = auth_configuration_from_config_ts(bom)
        .expect_err("BOM must separate tokens, not change the key name");
    assert!(error.to_string().contains("keld.permissions.jsonc"));
    // Escaped line continuation and identity-escape string keys decode to
    // the same exact spelling of profile.
    let continued = "export default { 'pro\\\nfile': 'legacy' };";
    assert!(
        auth_configuration_from_config_ts(continued)
            .expect_err("string key line continuation")
            .to_string()
            .contains("keld.permissions.jsonc")
    );
}

#[test]
fn generic_type_commas_are_not_root_keys_but_relational_commas_are() {
    for source in [
        "export default { other: id<A, auth, profile>(7) };",
        "export default { other: id<A, profile<B>, C>(7) };",
        "export default { other: id<{ auth: string, profile: string }, Other>(7) };",
        "export default { other: <T, profile>(x: T) => x };",
        "export default { other<T, profile>(value: T) { return value; } };",
        "export default { other: id<(x: number) => string, profile>(7) };",
    ] {
        assert!(
            auth_configuration_from_config_ts(source)
                .expect("comma inside bounded TS generic remains unrelated")
                .is_empty(),
            "{source}"
        );
    }
    let auth_source = "export default { other: id<A, auth, profile>(7), auth: { 'entra-common': { clientId: 'kept' } } };";
    let parsed = auth_configuration_from_config_ts(auth_source)
        .expect("real root auth is extracted after generic arguments");
    assert_eq!(
        parsed
            .get("entra-common")
            .expect("actual root auth")
            .client_id
            .as_deref(),
        Some("kept")
    );
    for source in [
        "export default { other: a < b, profile: 'legacy' };",
        "export default { other: id<A, profile>(7), profile: 'legacy' };",
    ] {
        let error = auth_configuration_from_config_ts(source)
            .expect_err("genuine root key after expression must be rejected");
        assert!(error.to_string().starts_with("KELD-CORE-035:"), "{source}");
    }
    let relational_auth =
        "export default { other: a < b, auth: { 'entra-common': { clientId: 'root' } } };";
    let parsed = auth_configuration_from_config_ts(relational_auth)
        .expect("relational less-than leaves real root auth visible");
    assert_eq!(
        parsed
            .get("entra-common")
            .expect("real auth")
            .client_id
            .as_deref(),
        Some("root")
    );
}

#[test]
fn escaped_root_auth_still_uses_the_original_literal_value_parser() {
    let source =
        r"export default { a\u0075th: { 'entra-common': { clientId: 'still-literal' } } };";
    let configuration = auth_configuration_from_config_ts(source)
        .expect("escaped auth identifier denotes the original auth key");
    assert_eq!(
        configuration
            .get("entra-common")
            .expect("root auth")
            .client_id
            .as_deref(),
        Some("still-literal")
    );
    let invalid = r"export default { a\u0075th: { 'entra-common': { clientId: compute() } } };";
    assert!(
        auth_configuration_from_config_ts(invalid)
            .expect_err("escaped auth name does not relax literal-value grammar")
            .to_string()
            .starts_with("KELD-CORE-035:")
    );
}
