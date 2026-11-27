//! Pure qualification of D-43's test-only fresh leaf encoding.
//! This mints neither an authenticated policy input nor native root custody.

use super::{UpdateError, invalid, validate_app};
use crate::Channel;

const FRESH_ROOT_DOMAIN: &[u8] = b"keld.windows.peruser.install-root/v2\0";
const WINDOWS_X64: &str = "windows-x64";

fn fresh_leaf(
    publisher_scope: &[u8; 32],
    app_id: &str,
    wire_channel: &str,
    target: &str,
) -> Result<String, UpdateError> {
    validate_app(app_id)?;
    let channel = Channel::parse(wire_channel).ok_or_else(|| invalid("unknown D-43 channel"))?;
    if target != WINDOWS_X64 {
        return Err(invalid("unsupported D-43 target"));
    }

    let mut hasher = blake3::Hasher::new();
    hasher.update(FRESH_ROOT_DOMAIN);
    hasher.update(publisher_scope);
    for field in [app_id, channel.as_str(), target] {
        let length = u16::try_from(field.len()).map_err(|_| invalid("D-43 field too long"))?;
        hasher.update(&length.to_be_bytes());
        hasher.update(field.as_bytes());
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[test]
fn exact_fresh_leaf_framing_and_lowercase_digest() {
    let frame = [
        &b"keld.windows.peruser.install-root/v2\x00"[..],
        &[0x42; 32],
        b"\x00\x08org.demo\x00\x06stable\x00\x0bwindows-x64",
    ]
    .concat();
    let expected = blake3::hash(&frame).to_hex().to_string();
    let actual =
        fresh_leaf(&[0x42; 32], "org.demo", "stable", "windows-x64").expect("valid wire fields");
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 64);
    assert!(
        actual
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
}

#[test]
fn every_validated_field_participates_without_claiming_commissioning() {
    let leaf = |scope, app, channel| {
        fresh_leaf(&[scope; 32], app, channel, WINDOWS_X64).expect("valid wire fields")
    };
    let stable = leaf(1, "org.demo", "stable");
    assert_ne!(stable, leaf(2, "org.demo", "stable"));
    assert_ne!(stable, leaf(1, "org.demo-2", "stable"));
    assert_ne!(stable, leaf(1, "org.demo", "beta"));
    assert_ne!(stable, leaf(1, "org.demo", "canary"));
}

#[test]
fn rejects_noncanonical_fields_without_normalizing() {
    let scope = [0; 32];
    for app in [
        "",
        "Org.demo",
        "org..demo",
        "org/demo",
        "órg.demo",
        "org.demo.",
    ] {
        assert!(fresh_leaf(&scope, app, "stable", WINDOWS_X64).is_err());
    }
    assert!(fresh_leaf(&scope, &"a".repeat(255), "stable", WINDOWS_X64).is_ok());
    assert!(fresh_leaf(&scope, &"a".repeat(256), "stable", WINDOWS_X64).is_err());
    for channel in ["Stable", "stable ", ""] {
        assert!(fresh_leaf(&scope, "org.demo", channel, WINDOWS_X64).is_err());
    }
    for target in ["WINDOWS-X64", "windows-arm64", "windows-x64 "] {
        assert!(fresh_leaf(&scope, "org.demo", "stable", target).is_err());
    }
}
