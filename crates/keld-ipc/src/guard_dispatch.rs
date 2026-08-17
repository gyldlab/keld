//! Guard-before-handler dispatch for privileged kipc `Call`s (KEL-69).
//!
//! Architecture 03 §1's destination: the guard evaluates
//! `(principal, channel, args)` before any handler with OS authority runs.
//! [`dispatch_privileged`] is the single sanctioned entry point for that —
//! a future privileged handler (host `fs.read`/`fs.write`, KEL-71; other
//! native modules) MUST call it instead of running its body directly, so a
//! deny can never be bypassed by a handler that forgets to check.
//!
//! The echo channel (KEL-30) is deliberately NOT routed through this: it is
//! an unprivileged demo, not an operation with OS authority
//! (`crate::session::serve_echo_session` stays ungated).

use keld_guard::{Decision, DenyReason, PermissionsManifest, Principal, evaluate};

/// Evaluates `(principal, operation, path)` against `manifest`; only calls
/// `handler` on [`Decision::Allow`]. On [`Decision::Deny`], `handler`'s
/// OS/side-effect never runs — the caller gets the typed [`DenyReason`]
/// (`KELD-GUARD*`, with fix text) instead.
///
/// `path` is the resource argument the manifest's scopes are checked
/// against (e.g. a filesystem path); it is not itself a kipc frame field —
/// callers extract it from the decoded request.
///
/// # Errors
///
/// Returns the [`DenyReason`] `evaluate` produced when the decision is
/// [`Decision::Deny`] — `handler` never runs in that case.
pub fn dispatch_privileged<T>(
    manifest: &PermissionsManifest,
    principal: Principal,
    operation: &str,
    path: &str,
    handler: impl FnOnce() -> T,
) -> Result<T, DenyReason> {
    match evaluate(manifest, principal, operation, path) {
        Decision::Allow => Ok(handler()),
        Decision::Deny(reason) => Err(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keld_guard::parse_manifest;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn manifest_granting_fs_read() -> PermissionsManifest {
        parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("manifest")
    }

    #[test]
    fn allow_runs_the_handler_and_observes_its_side_effect() {
        let manifest = manifest_granting_fs_read();
        let ran = AtomicBool::new(false);
        let result = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            "fs.read",
            "$APPDATA/notes.txt",
            || {
                // Real side effect, not `Decision::Allow` from a unit stub —
                // observed via the flag after `dispatch_privileged` returns.
                ran.store(true, Ordering::SeqCst);
                42
            },
        );
        assert_eq!(result, Ok(42));
        assert!(ran.load(Ordering::SeqCst), "handler must have run on Allow");
    }

    #[test]
    fn deny_never_runs_the_handler() {
        let manifest = manifest_granting_fs_read();
        let ran = AtomicBool::new(false);
        // Out-of-scope path: granted only $APPDATA/**.
        let result = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            "fs.read",
            "$DOCUMENTS/secret.txt",
            || {
                ran.store(true, Ordering::SeqCst);
            },
        );
        assert!(
            matches!(result, Err(DenyReason::OutOfScope { .. })),
            "{result:?}"
        );
        assert!(
            !ran.load(Ordering::SeqCst),
            "handler's side-effect must not occur on Deny"
        );
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("KELD-GUARD002"), "{msg}");
    }

    #[test]
    fn missing_capability_is_not_granted_and_does_not_run_handler() {
        let manifest = parse_manifest("{}").expect("empty manifest");
        let ran = AtomicBool::new(false);
        let result = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            "fs.write",
            "$APPDATA/x",
            || ran.store(true, Ordering::SeqCst),
        );
        assert!(
            matches!(result, Err(DenyReason::NotGranted { .. })),
            "{result:?}"
        );
        assert!(!ran.load(Ordering::SeqCst));
        assert!(result.unwrap_err().to_string().contains("KELD-GUARD001"));
    }

    #[test]
    fn non_app_process_principal_is_denied_even_with_an_in_scope_path() {
        // KEL-69 AC: a webview/plugin must not inherit /app grants on this path.
        let manifest = manifest_granting_fs_read();
        let ran = AtomicBool::new(false);
        let webview = Principal::Webview {
            id: 1,
            generation: 1,
        };
        let result = dispatch_privileged(
            &manifest,
            webview,
            "fs.read",
            "$APPDATA/notes.txt", // in scope for AppProcess
            || ran.store(true, Ordering::SeqCst),
        );
        assert!(
            matches!(result, Err(DenyReason::NotAppProcess { .. })),
            "{result:?}"
        );
        assert!(!ran.load(Ordering::SeqCst));
        assert!(result.unwrap_err().to_string().contains("KELD-GUARD006"));
    }
}
