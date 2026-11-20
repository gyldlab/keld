//! Channel adoption in an explicitly selected, independently digest-verified Legacy policy.
use std::fs::File;

use keld_guard::verified_manifest::{VerifiedManifest, load_verified_manifest as load_selected};
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

fn selected(text: &str) -> Result<VerifiedManifest, ManifestError> {
    let file = PolicyFile::new(text.as_bytes()).expect("create policy file");
    load_selected(
        File::open(&file.0).expect("retained handle"),
        file.0.clone(),
        Sha256::digest(text.as_bytes()).into(),
    )
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
                    selected(&text).expect_err("bad active list").code(),
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
                selected(&text)
                    .expect_err("invalid window policy or ambiguous input")
                    .code(),
                "KELD-GUARD005"
            );
        }
    }
}
