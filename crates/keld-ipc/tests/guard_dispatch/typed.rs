//! External source compatibility and callback isolation for the typed dispatcher.

use keld_guard::{NativeCapabilityQuery, Principal, ScopePermit, parse_manifest};
use keld_ipc::guard_dispatch::{dispatch_privileged, typed};

#[test]
fn typed_native_and_existing_five_argument_calls_agree() {
    let policy =
        parse_manifest(r#"{"app":{"net":{"fetch":["example.test"]}}}"#).expect("native grant");
    let query = NativeCapabilityQuery {
        operation: "net.fetch",
        path: "example.test",
    };
    let from_native = dispatch_privileged::<usize>(
        &policy,
        Principal::AppProcess,
        query.operation,
        query.path,
        ScopePermit::grant_index,
    );
    let inferred = typed::dispatch_privileged(
        &policy,
        Principal::AppProcess,
        query,
        ScopePermit::grant_index,
    );
    assert_eq!(from_native, Ok(0));
    assert_eq!(inferred, from_native);
}

#[cfg(windows)]
#[test]
fn selected_renderer_misses_keep_the_wire_error_and_never_call_the_handler() {
    use keld_guard::{
        RendererChannelPermit, RendererChannelQuery,
        verified_manifest::load_selected_windows_legacy_manifest,
    };
    use keld_ipc::CallError;
    use sha2::{Digest as _, Sha256};
    use std::fs;

    for (marker, channel) in [
        ("", "open"),
        (r#""$keldWindowChannelGrants":2,"#, "open"),
        (r#""$keldWindowChannelGrants":1,"#, "openX"),
    ] {
        let source = format!(
            r#"{{"profile":"legacy",{marker}"windows":{{"editor":{{"channels":["el:open"],"web":{{"csp":"original-editor-offline"}}}}}}}}"#
        );
        let dir = super::tmp_dir("selected-policy");
        let path = dir.join("keld.permissions.jsonc");
        fs::write(&path, &source).expect("write fixture bytes");
        let verified = load_selected_windows_legacy_manifest(
            fs::File::open(&path).expect("retain source"),
            path.clone(),
            Sha256::digest(source.as_bytes()).into(),
        )
        .expect("verified selected policy");
        fs::remove_file(&path).expect("remove fixture");
        fs::remove_dir(&dir).expect("remove fixture directory");

        let policy = verified.manifest();
        let query = RendererChannelQuery {
            channel,
            current_window: policy.window_channel_grants("editor"),
        };
        let mut called = 0;
        let reason = typed::dispatch_privileged(
            policy,
            Principal::Webview {
                id: 1,
                generation: 1,
            },
            query,
            |_: &RendererChannelPermit| called += 1,
        )
        .expect_err("current request must deny");
        assert_eq!(called, 0);
        assert_eq!(reason.kind(), "channel_forbidden");
        let wire = CallError::from(&reason);
        assert_eq!(wire.code, "KELD-GUARD003");
        assert_eq!(
            wire.message,
            format!(
                "KELD-GUARD003: channel \x60el:{channel}\x60 is not granted to this principal. Add \x60el:{channel}\x60 to this principal's channels list in keld.permissions.jsonc."
            )
        );
    }
}
