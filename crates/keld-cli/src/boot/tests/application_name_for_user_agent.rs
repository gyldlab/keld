//! Cold producer tests use the real staging entry and never execute staged code.
use super::{fixture, fixture_dev_root};
use crate::boot::stage_dev_boot;
use std::fs;

fn config(member: &str) -> String {
    format!(
        "export default {{ name: \"Stage Fixture\", entry: \"src/main.ts\", renderer: \"index.html\", {member} }} as const;\n"
    )
}

#[test]
fn application_name_for_user_agent_absence_preserves_exact_descriptor_bytes() {
    let (_temp, project, host) = fixture();
    let stage = stage_dev_boot(&project, &host).expect("old default remains usable");
    let bytes = fs::read(stage.root().join("keld.boot.json")).expect("descriptor bytes");
    assert_eq!(bytes, br#"{"entry":"src/main.ts","name":"Stage Fixture","permissions":{"content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356","file":"keld.permissions.jsonc"},"renderer":"index.html","schema":1}"#);
}

#[test]
fn application_name_for_user_agent_literal_is_carried_or_explicitly_refused() {
    for requested in [
        String::from("FirstPartyA/1 FrameworkMetadata"),
        "x".repeat(256),
    ] {
        let (_temp, project, host) = fixture();
        fs::write(
            project.join("keld.config.ts"),
            config(&format!("applicationNameForUserAgent: {requested:?},")),
        )
        .expect("literal config");
        let staged = stage_dev_boot(&project, &host);
        #[cfg(target_os = "macos")]
        {
            let stage = staged.expect("macOS producer accepts validated metadata");
            let descriptor: serde_json::Value = serde_json::from_slice(
                &fs::read(stage.root().join("keld.boot.json")).expect("descriptor"),
            )
            .expect("closed JSON");
            assert_eq!(descriptor["applicationNameForUserAgent"], requested);
            assert_eq!(descriptor["schema"], 1);
            assert_eq!(
                fs::read(stage.root().join("keld.permissions.jsonc")).expect("policy bytes"),
                b"{}\n"
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            let error = staged.expect_err("unsupported backend must preserve/refuse request");
            assert!(
                error.to_string().contains("applicationNameForUserAgent"),
                "{error}"
            );
            assert!(error.to_string().contains("macOS"), "{error}");
            assert!(
                !fixture_dev_root(&project).exists(),
                "refusal precedes stage resources"
            );
        }
    }
}

#[test]
fn application_name_for_user_agent_bad_literals_refuse_before_staging() {
    let overlong = format!("applicationNameForUserAgent: {:?},", "x".repeat(257));
    let members = [
        "applicationNameForUserAgent: null,",
        "applicationNameForUserAgent: 7,",
        "applicationNameForUserAgent: false,",
        "applicationNameForUserAgent: \"\",",
        "applicationNameForUserAgent: \" leading\",",
        "applicationNameForUserAgent: \"trailing \",",
        "applicationNameForUserAgent: \"line\\nfeed\",",
        "applicationNameForUserAgent: \"tab\\tvalue\",",
        "applicationNameForUserAgent: \"nul\\0value\",",
        "applicationNameForUserAgent: \"del\\x7fvalue\",",
        "applicationNameForUserAgent: \"café\",",
        "applicationNameForUserAgent: \"A\" + \"B\",",
        "applicationNameForUserAgent: `Template`,",
        "applicationNameForUserAgent: runtimeValue,",
        "applicationNameForUserAgent: \"A\", applicationNameForUserAgent: \"B\",",
        "applicationNameForUserAgent: \"A\", \"applicationNameForUser\\u0041gent\": \"B\",",
        "[\"applicationNameForUserAgent\"]: \"A\",",
        "get applicationNameForUserAgent() { return \"A\"; },",
        "...runtimeConfig,",
        &overlong,
    ];
    for member in members {
        let (_temp, project, host) = fixture();
        fs::write(project.join("keld.config.ts"), config(member)).expect("invalid config fixture");
        assert!(
            stage_dev_boot(&project, &host).is_err(),
            "accepted {member}"
        );
        assert!(
            !fixture_dev_root(&project).exists(),
            "invalid literal acquired stage resources: {member}"
        );
    }
}
