//! Channel adoption in an explicitly selected, independently digest-verified Legacy policy.
use std::{error::Error, fs::File};

use keld_guard::verified_manifest::{VerifiedManifest, load_selected_windows_legacy_manifest};
use keld_guard::{
    Decision, DenyReason, ManifestError, Principal, RendererChannelPermit, RendererChannelQuery,
    evaluate, evaluate_query,
};
use sha2::{Digest as _, Sha256};

use super::{HEADER, PolicyFile};

const SOURCE: &str = r#"{"profile":"legacy","$keldWindowChannelGrants":1,"app":{"fs":{"read":["/notes/**"]}},"windows":{"editor":{"channels":["el:open"],"web":{"csp":"original-editor-offline"}},"viewer":{"channels":["el:about"]}}}"#;
const MARKER: &str = r#""$keldWindowChannelGrants":1,"#;
const INACTIVE: &[&str] = &[
    "", "null", r#""1""#, "true", "false", "0", "2", "1.0", "1e0", "{}", "[]",
];

fn policy(marker: &str) -> String {
    if marker.is_empty() {
        SOURCE.replace(MARKER, "")
    } else {
        SOURCE.replace(MARKER, &format!(r#""$keldWindowChannelGrants":{marker},"#))
    }
}

fn selected(text: &str) -> Result<VerifiedManifest, Box<dyn Error>> {
    let file = PolicyFile::new(text.as_bytes())?;
    Ok(load_selected_windows_legacy_manifest(
        File::open(&file.0)?,
        file.0.clone(),
        Sha256::digest(text.as_bytes()).into(),
    )?)
}

fn policy_error(text: &str) -> Result<ManifestError, Box<dyn Error>> {
    match selected(text) {
        Ok(_) => Err("invalid policy was accepted".into()),
        Err(error) => error.downcast::<ManifestError>().map(|error| *error),
    }
}

fn channel(policy: &VerifiedManifest, key: &str, topic: &str) -> Decision<RendererChannelPermit> {
    let manifest = policy.manifest();
    evaluate_query(
        manifest,
        Principal::Webview {
            id: 1,
            generation: 1,
        },
        RendererChannelQuery {
            channel: topic,
            current_window: manifest.window_channel_grants(key),
        },
    )
}

fn assert_miss(policy: &VerifiedManifest, key: &str, topic: &str) {
    let decision = channel(policy, key, topic);
    assert_eq!(
        decision,
        Decision::Deny(DenyReason::ChannelForbidden {
            channel: format!("el:{topic}")
        })
    );
    let Decision::Deny(reason) = decision else {
        unreachable!()
    };
    assert_eq!(reason.code(), "KELD-GUARD003");
}

#[test]
fn only_the_marked_policy_grants_its_own_exact_literal() {
    for marker in INACTIVE.iter().copied().chain(["1"]) {
        let text = policy(marker);
        let verified = selected(&text).expect("digest-verified selected fixture");
        let csp = verified.resolve_window_csp("editor").expect("verified CSP");
        assert_eq!(csp.response_header(), HEADER);
        assert_eq!(
            csp.verified_sha256(),
            <[u8; 32]>::from(Sha256::digest(text.as_bytes()))
        );
        assert!(matches!(
            evaluate(
                verified.manifest(),
                Principal::AppProcess,
                "fs.read",
                "/notes/a"
            ),
            Decision::Allow(_)
        ));
        assert!(matches!(
            evaluate(
                verified.manifest(),
                Principal::AppProcess,
                "fs.read",
                "/other/a"
            ),
            Decision::Deny(_)
        ));
        for (key, topic) in [
            ("editor", "openX"),
            ("editor", "Open"),
            ("editor", "about"),
            ("viewer", "open"),
            ("missing", "open"),
        ] {
            assert_miss(&verified, key, topic);
        }
        if marker == "1" {
            assert!(matches!(
                channel(&verified, "editor", "open"),
                Decision::Allow(_)
            ));
            assert!(matches!(
                channel(&verified, "viewer", "about"),
                Decision::Allow(_)
            ));
        } else {
            assert_miss(&verified, "editor", "open");
            assert_miss(&verified, "viewer", "about");
        }
    }
}

#[test]
fn inactive_malformed_channels_preserve_csp_and_active_malformed_channels_refuse_the_policy() {
    for bad in [
        "null",
        "true",
        r#""el:open""#,
        r#"["el:open", 1]"#,
        r#"["el:*"]"#,
    ] {
        for marker in INACTIVE.iter().copied().chain(["1"]) {
            let text = policy(marker).replace(r#"["el:open"]"#, bad);
            if marker == "1" {
                assert_eq!(
                    policy_error(&text).expect("bad active list").code(),
                    "KELD-GUARD005"
                );
            } else {
                let verified = selected(&text).expect("inactive list is inert");
                assert_eq!(
                    verified
                        .resolve_window_csp("editor")
                        .expect("valid CSP")
                        .response_header(),
                    HEADER
                );
                assert_miss(&verified, "editor", "open");
            }
        }
    }
}

#[test]
fn independent_window_structure_and_csp_errors_refuse_every_marker() {
    for marker in INACTIVE.iter().copied().chain(["1"]) {
        for text in [
            policy(marker).replace("original-editor-offline", "unknown"),
            policy(marker).replace(r#"{"channels":["el:about"]}"#, "42"),
            policy(marker).replace(
                r#""channels":["el:open"]"#,
                r#""channels":[],"channels":["el:open"]"#,
            ),
        ] {
            assert_eq!(
                policy_error(&text)
                    .expect("invalid window policy or ambiguous input")
                    .code(),
                "KELD-GUARD005"
            );
        }
    }
}

#[test]
fn ordinary_loaders_keep_their_prior_behavior_and_only_explicit_legacy_can_be_selected() {
    for profile in [r#""profile":"strict","#, "", r#""profile":"legacy","#] {
        let text = policy("").replace(r#""profile":"legacy","#, profile);
        let file = PolicyFile::new(text.as_bytes()).expect("ordinary policy");
        let ordinary = file
            .verified(text.as_bytes())
            .expect("ordinary verified policy");
        assert!(matches!(
            channel(&ordinary, "editor", "open"),
            Decision::Allow(_)
        ));
        let bad = text.replace(r#"["el:open"]"#, "null");
        assert!(keld_guard::parse_manifest(&bad).is_err());
        let file = PolicyFile::new(bad.as_bytes()).expect("malformed ordinary policy");
        assert!(file.verified(bad.as_bytes()).is_err());
        if profile != r#""profile":"legacy","# {
            let marked = SOURCE.replace(r#""profile":"legacy","#, profile);
            assert_eq!(
                policy_error(&marked)
                    .expect("outside selected profile")
                    .code(),
                "KELD-GUARD005"
            );
        }
    }
}

#[test]
fn selected_loader_enforces_the_verified_byte_and_size_boundary() {
    let stripped = policy("");
    let file = PolicyFile::new(stripped.as_bytes()).expect("stripped policy");
    let error = load_selected_windows_legacy_manifest(
        File::open(&file.0).expect("retained stripped handle"),
        file.0.clone(),
        Sha256::digest(SOURCE.as_bytes()).into(),
    )
    .expect_err("digest from marked source must refuse the stripped source");
    assert_eq!(error.code(), "KELD-GUARD016");
    let exact = SOURCE.to_owned() + &" ".repeat(64 * 1024 - SOURCE.len());
    assert!(selected(&exact).is_ok());
    assert_eq!(
        policy_error(&(exact + " ")).expect("over cap").code(),
        "KELD-GUARD017"
    );
}

#[test]
fn marked_literal_bounds_use_the_original_utf8_suffix_bytes() {
    let topic = "é".repeat(128);
    let source = SOURCE.replace("el:open", &format!(r#"el:{topic}","el:é"#));
    let admitted = selected(&source).expect("256 byte suffix");
    assert!(matches!(
        channel(&admitted, "editor", &topic),
        Decision::Allow(_)
    ));
    assert!(matches!(
        channel(&admitted, "editor", "é"),
        Decision::Allow(_)
    ));
    assert_miss(&admitted, "editor", "e\u{301}");
    assert_miss(&admitted, "editor", "%C3%A9");
    assert_eq!(
        policy_error(&source.replace(&topic, &(topic.clone() + "a")))
            .expect("257 byte suffix")
            .code(),
        "KELD-GUARD005"
    );
}
