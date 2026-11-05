//! Guard-before-effect/preflight atom, with independent callback effect counters.
//! These are policy/dispatch tests, not physical native authentication or Bun proof.
use std::cell::Cell;

use keld_guard::{
    DenyReason, NativeCapabilityQuery, Principal, RendererChannelQuery, ScopePermit, parse_manifest,
};
use keld_ipc::call_error::CallError;
use keld_ipc::guard_dispatch::{dispatch_privileged_query, preflight_privileged_query};

fn webview() -> Principal {
    Principal::Webview {
        id: 7,
        generation: 3,
    }
}

#[test]
fn no_grant_means_no_callback_and_preserves_the_frozen_guard003_wire_error()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = parse_manifest(r#"{"windows":{"editor":{"channels":[]}}}"#)?;
    let query = RendererChannelQuery {
        channel: "rendererReq",
        current_window: policy.window_channel_grants("editor"),
    };
    let effects = Cell::new(0);
    let preflight = preflight_privileged_query(&policy, webview(), query);
    let final_error = dispatch_privileged_query(&policy, webview(), query, |_| {
        effects.set(effects.get() + 1);
    });
    assert_eq!(preflight, final_error);
    assert!(final_error.is_err());
    let final_error = final_error.err().ok_or("expected final denial")?;
    assert_eq!(effects.get(), 0);
    assert_eq!(
        final_error,
        DenyReason::ChannelForbidden {
            channel: "el:rendererReq".to_owned()
        }
    );
    assert_eq!(final_error.code(), "KELD-GUARD003");
    assert_eq!(final_error.kind(), "channel_forbidden");
    let wire = CallError::from(&final_error);
    assert_eq!(wire.code, "KELD-GUARD003");
    assert_eq!(
        wire.message,
        "KELD-GUARD003: channel `el:rendererReq` is not granted to this principal. Add `el:rendererReq` to this principal's channels list in keld.permissions.jsonc."
    );
    Ok(())
}

#[test]
fn successful_preflight_is_not_a_cached_permit_for_a_different_final_topic()
-> Result<(), Box<dyn std::error::Error>> {
    let policy = parse_manifest(r#"{"windows":{"editor":{"channels":["el:rendererReq"]}}}"#)?;
    let current_window = policy.window_channel_grants("editor");
    preflight_privileged_query(
        &policy,
        webview(),
        RendererChannelQuery {
            channel: "rendererReq",
            current_window,
        },
    )
    .expect("exact grant permits preflight");
    let effects = Cell::new(0);
    let denied = dispatch_privileged_query(
        &policy,
        webview(),
        RendererChannelQuery {
            channel: "rendererReqX",
            current_window,
        },
        |_| effects.set(effects.get() + 1),
    );
    assert!(matches!(denied, Err(DenyReason::ChannelForbidden { .. })));
    assert_eq!(effects.get(), 0);
    dispatch_privileged_query(
        &policy,
        webview(),
        RendererChannelQuery {
            channel: "rendererReq",
            current_window,
        },
        |_| effects.set(effects.get() + 1),
    )
    .expect("exact grant permits final dispatch");
    assert_eq!(effects.get(), 1);
    Ok(())
}

#[test]
fn independently_inferred_native_and_renderer_queries_keep_separate_permits()
-> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    let (scope, requested) = ("C:/safe/**", "C:/safe/file");
    #[cfg(not(windows))]
    let (scope, requested) = ("/safe/**", "/safe/file");
    let policy = parse_manifest(&format!(
        r#"{{"app":{{"fs":{{"read":["{scope}"]}}}},"windows":{{"editor":{{"channels":["el:fs.read"]}}}}}}"#
    ))?;
    let native = dispatch_privileged_query(
        &policy,
        Principal::AppProcess,
        NativeCapabilityQuery {
            operation: "fs.read",
            path: requested,
        },
        ScopePermit::grant_index,
    );
    assert_eq!(native, Ok(0));
    let channel = dispatch_privileged_query(
        &policy,
        webview(),
        RendererChannelQuery {
            channel: "fs.read",
            current_window: policy.window_channel_grants("editor"),
        },
        |_channel_permit| 17,
    );
    assert_eq!(channel, Ok(17));
    Ok(())
}
