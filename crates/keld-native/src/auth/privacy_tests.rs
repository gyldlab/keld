use super::*;
// External proposal: append these cases to the owning auth tests after Root
// integration. These prove renderer + actual guard dispatch, not registry
// resolution, token exchange, host reporting, or actual caller wire routing.

use keld_guard::{DenyReason, PermissionsManifest, Principal, parse_manifest};
use keld_ipc::CallError;
use keld_ipc::codec::{decode, encode};
use keld_ipc::guard_dispatch::dispatch_privileged;

const LIVE_Q: &str = "https://issuer-q.test";
const UNRESOLVED: &str = "\0unresolved";

// Literal postcard struct bytes: code length13, code, message length120/126,
// then exact UTF-8 message. Neither expected vector references production
// constants, parts(), an encoder-produced expected value, or a length helper.
const TOKEN_NOT_GRANTED: &[u8] = b"\x0dKELD-GUARD001\x78KELD-GUARD001: auth.token is not granted. Add the intended declared issuer to /app/auth/token in keld.permissions.jsonc.";
const TOKEN_OUT_OF_SCOPE: &[u8] = b"\x0dKELD-GUARD002\x7eKELD-GUARD002: auth.token is outside its grant. Add the intended declared issuer to /app/auth/token in keld.permissions.jsonc.";

fn actual_denial(manifest: &PermissionsManifest, resource: &str) -> Result<(), DenyReason> {
    let effects = std::cell::Cell::new(0);
    let result = dispatch_privileged(
        manifest,
        Principal::AppProcess,
        "auth.token",
        resource,
        |_| {
            effects.set(effects.get() + 1);
        },
    );
    assert_eq!(effects.get(), 0, "deny fixture reached an effect callback");
    result
}

#[test]
fn token_ungranted_live_and_unresolved_are_the_same_literal_bytes() {
    for source in [
        r"{}",
        r#"{"app":{"auth":{"token":true}}}"#,
        r#"{"app":{"auth":{"token":[]}}}"#,
        r#"{"app":{"auth":{"token":["https://issuer-p.test",7]}}}"#,
    ] {
        let manifest = parse_manifest(source).expect("pre-scope NotGranted fixture");
        let live = actual_denial(&manifest, LIVE_Q)
            .expect_err("actual guard must deny this token resource");
        let unresolved = actual_denial(&manifest, UNRESOLVED)
            .expect_err("actual guard must deny this token resource");
        assert!(matches!(live, DenyReason::NotGranted { .. }));
        assert!(matches!(unresolved, DenyReason::NotGranted { .. }));
        let live_wire = encode(&CallError::from(
            TokenDenial::from_guard(&live).expect("001 category"),
        ))
        .expect("live deny encode");
        let unresolved_wire = encode(&CallError::from(
            TokenDenial::from_guard(&unresolved).expect("001 category"),
        ))
        .expect("unresolved deny encode");
        assert_eq!(live_wire, TOKEN_NOT_GRANTED);
        assert_eq!(unresolved_wire, TOKEN_NOT_GRANTED);
        assert_eq!(live_wire, unresolved_wire);
    }
}

#[test]
fn token_scoped_live_and_unresolved_are_the_same_literal_bytes() {
    // Present, nonempty, all-string, different issuer. Empty list is 001,
    // and therefore cannot stand in for a scoped 002 fixture.
    let manifest = parse_manifest(r#"{"app":{"auth":{"token":["https://issuer-p.test"]}}}"#)
        .expect("different literal issuer grant");
    let live =
        actual_denial(&manifest, LIVE_Q).expect_err("actual guard must deny this token resource");
    let unresolved = actual_denial(&manifest, UNRESOLVED)
        .expect_err("actual guard must deny this token resource");
    assert!(matches!(live, DenyReason::OutOfScope { .. }));
    assert!(matches!(unresolved, DenyReason::OutOfScope { .. }));
    let live_wire = encode(&CallError::from(
        TokenDenial::from_guard(&live).expect("002 category"),
    ))
    .expect("live deny encode");
    let unresolved_wire = encode(&CallError::from(
        TokenDenial::from_guard(&unresolved).expect("002 category"),
    ))
    .expect("unresolved deny encode");
    assert_eq!(live_wire, TOKEN_OUT_OF_SCOPE);
    assert_eq!(unresolved_wire, TOKEN_OUT_OF_SCOPE);
    assert_eq!(live_wire, unresolved_wire);
    assert_ne!(TOKEN_NOT_GRANTED, TOKEN_OUT_OF_SCOPE);
}

#[test]
fn token_denial_literal_vectors_decode_with_code_and_fix() {
    for (bytes, code) in [
        (TOKEN_NOT_GRANTED, "KELD-GUARD001"),
        (TOKEN_OUT_OF_SCOPE, "KELD-GUARD002"),
    ] {
        let error: CallError = decode(bytes).expect("decode independent literal vector");
        assert_eq!(error.code, code);
        assert!(error.message.starts_with(code));
        assert!(error.message.contains("auth.token"));
        assert!(error.message.contains("/app/auth/token"));
        assert!(error.message.contains("Add the intended declared issuer"));
        assert_eq!(error.to_string(), error.message);
        for secret in [
            "https://issuer-p.test",
            LIVE_Q,
            UNRESOLVED,
            "private-account-selector",
        ] {
            assert!(!error.message.contains(secret));
        }
    }
}

#[test]
fn ordinary_guard_conversion_retains_full_display_and_requested_path() {
    let manifest = parse_manifest("{}").expect("empty ordinary guard manifest");
    #[cfg(windows)]
    let requested = "C:/private-ordinary-fs-oracle";
    #[cfg(not(windows))]
    let requested = "/private-ordinary-fs-oracle";
    let reason = dispatch_privileged(
        &manifest,
        Principal::AppProcess,
        "fs.read",
        requested,
        |_| {
            panic!("ordinary deny fixture reached effect callback");
        },
    )
    .expect_err("ordinary guard denial");
    assert!(TokenDenial::from_guard(&reason).is_none());
    let error = CallError::from(&reason);
    assert_eq!(error.code, reason.code());
    assert_eq!(error.message, reason.to_string());
    assert!(error.message.contains(requested));
}

// No fabricated policy-denial fixture is proposed for AC13(d). A pure renderer
// equality check alone is insufficient: Root's actual dispatcher test must
// use direct Deserialize to bypass the shared manifest loader, actually Allow
// UNRESOLVED, resolve None, emit TOKEN_OUT_OF_SCOPE through write_call_error,
// surface a Native host-only fault and assert zero token/broker/listener/exchange.
// Place that integration control in an existing test owner with serde_json
// already available (e.g. keld-core), rather than adding a Native dependency.
