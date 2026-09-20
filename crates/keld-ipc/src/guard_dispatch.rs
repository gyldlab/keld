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

use keld_guard::{
    Decision, DenyReason, PermissionsManifest, Principal, ScopePermit, evaluate, json_pointer_for,
    validate_fs_component,
};

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
///
/// The result type cannot borrow the callback-only permit:
///
/// ```compile_fail
/// # use keld_guard::{parse_manifest, Principal};
/// # use keld_ipc::guard_dispatch::dispatch_privileged;
/// # let manifest = parse_manifest(r#"{"app":{"fs":{"read":["/tmp/**"]}}}"#)?;
/// let escaped = dispatch_privileged(
///     &manifest,
///     Principal::AppProcess,
///     "fs.read",
///     "/tmp/file",
///     |permit| permit,
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn dispatch_privileged<T>(
    manifest: &PermissionsManifest,
    principal: Principal,
    operation: &str,
    path: &str,
    handler: impl FnOnce(&ScopePermit) -> T,
) -> Result<T, DenyReason> {
    match evaluate(manifest, principal, operation, path) {
        Decision::Allow(permit) if filesystem_dispatch_path_is_valid(operation, path) => {
            Ok(handler(&permit))
        }
        Decision::Allow(_) => Err(DenyReason::OutOfScope {
            capability: operation.to_owned(),
            scope: "absolute normalized filesystem request".to_owned(),
            json_pointer: json_pointer_for(operation),
            requested: path.to_owned(),
        }),
        Decision::Deny(reason) => Err(reason),
    }
}

fn filesystem_dispatch_path_is_valid(operation: &str, path: &str) -> bool {
    if !matches!(operation, "fs.read" | "fs.write") {
        return true;
    }
    if path.is_empty() || path.contains(['\0', '\\']) {
        return false;
    }
    #[cfg(windows)]
    let remainder = {
        let bytes = path.as_bytes();
        if bytes.len() < 3 || !bytes[0].is_ascii_uppercase() || bytes[1] != b':' || bytes[2] != b'/'
        {
            return false;
        }
        &path[3..]
    };
    #[cfg(not(windows))]
    let Some(remainder) = path.strip_prefix('/') else {
        return false;
    };

    remainder.is_empty()
        || remainder
            .split('/')
            .all(|component| validate_fs_component(component).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keld_guard::parse_manifest;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn manifest_granting_fs_read() -> PermissionsManifest {
        parse_manifest(r#"{"app":{"fs":{"read":["/appdata/**"]}}}"#).expect("manifest")
    }

    #[test]
    fn allow_runs_the_handler_and_observes_its_side_effect() {
        let manifest = manifest_granting_fs_read();
        let ran = AtomicBool::new(false);
        let result = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            "fs.read",
            "/appdata/notes.txt",
            |_| {
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
    fn allow_lends_the_first_matching_grant_index() {
        let manifest = parse_manifest(
            r#"{"app":{"fs":{"read":["/unmatched/**","/matched/**","/matched/file"]}}}"#,
        )
        .expect("manifest");
        let selected = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            "fs.read",
            "/matched/file",
            keld_guard::ScopePermit::grant_index,
        );
        assert_eq!(selected, Ok(1), "first matching grant remains final");
    }

    #[test]
    fn deny_never_runs_the_handler() {
        let manifest = manifest_granting_fs_read();
        let ran = AtomicBool::new(false);
        // Out-of-scope path: granted only /appdata/**.
        let result = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            "fs.read",
            "/documents/secret.txt",
            |_| {
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
    fn invalid_filesystem_grammar_never_runs_the_handler() {
        let manifest = manifest_granting_fs_read();
        let ran = AtomicBool::new(false);
        for invalid in ["/appdata//notes.txt", "/appdata/./notes.txt"] {
            let result =
                dispatch_privileged(&manifest, Principal::AppProcess, "fs.read", invalid, |_| {
                    ran.store(true, Ordering::SeqCst);
                });
            assert!(
                matches!(result, Err(DenyReason::OutOfScope { .. })),
                "{invalid}: {result:?}"
            );
            assert!(!ran.load(Ordering::SeqCst));
        }
        let variable_manifest = parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#)
            .expect("literal variable manifest");
        let result = dispatch_privileged(
            &variable_manifest,
            Principal::AppProcess,
            "fs.read",
            "$APPDATA/notes.txt",
            |_| ran.store(true, Ordering::SeqCst),
        );
        assert!(matches!(result, Err(DenyReason::OutOfScope { .. })));
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[test]
    fn principal_denial_precedes_invalid_filesystem_grammar_without_scope_disclosure() {
        let manifest = manifest_granting_fs_read();
        for principal in [
            Principal::Webview {
                id: 9,
                generation: 2,
            },
            Principal::Plugin { id: 4 },
        ] {
            let ran = AtomicBool::new(false);
            let result =
                dispatch_privileged(&manifest, principal, "fs.read", "/appdata//secret", |_| {
                    ran.store(true, Ordering::SeqCst);
                });
            let reason = result.expect_err("non-app principal must deny before grammar");
            assert_eq!(reason.code(), "KELD-GUARD006");
            assert!(matches!(reason, DenyReason::NotAppProcess { .. }));
            assert!(!reason.to_string().contains("/appdata"));
            assert!(!ran.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn missing_capability_is_not_granted_and_does_not_run_handler() {
        let manifest = parse_manifest("{}").expect("empty manifest");
        let ran = AtomicBool::new(false);
        let result = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            "fs.write",
            "/appdata/x",
            |_| {
                ran.store(true, Ordering::SeqCst);
            },
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
            "/appdata/notes.txt", // in scope for AppProcess
            |_| ran.store(true, Ordering::SeqCst),
        );
        assert!(
            matches!(result, Err(DenyReason::NotAppProcess { .. })),
            "{result:?}"
        );
        assert!(!ran.load(Ordering::SeqCst));
        assert!(result.unwrap_err().to_string().contains("KELD-GUARD006"));
    }
}
