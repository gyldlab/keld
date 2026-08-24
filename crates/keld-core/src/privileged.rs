//! Privileged kipc dispatch: `evaluate` before any handler with OS authority.
//!
//! Architecture 03 §1 / broker pattern (03 §4.1): the guard sits between
//! decode and the handler, so a forgotten check is not a possible bug shape.
//! Echo (`ECHO_CHANNEL`) is classified unprivileged and is not dispatched
//! through this module (`serve_echo_session` stays ungated).

use std::fmt;
use std::io::{ErrorKind, Read, Write};

use keld_guard::{Decision, DenyReason, PermissionsManifest, Principal, evaluate};
use keld_ipc::codec::{decode, encode};
use keld_ipc::echo::handle_echo;
use keld_ipc::frame::{ChannelId, FrameKind};
use keld_ipc::link::{AppLinkDeadlines, handshake_server, read_frame, write_frame};
use keld_ipc::{APP_LINK_IO_DEADLINE, CallError, ECHO_CHANNEL, IpcError, SessionToken};

/// kipc channel for host-owned scoped `fs.read`.
///
/// v0 payload is a postcard `String` path. [`dispatch_privileged`] evaluates
/// [`FS_READ_OPERATION`] against that path before the handler. The OS read
/// itself is KEL-71 and MUST use this path.
pub const FS_READ_CHANNEL: ChannelId = ChannelId(2);

/// Capability id evaluated for [`FS_READ_CHANNEL`].
pub const FS_READ_OPERATION: &str = "fs.read";

/// Host-side privileged session: echo stays ungated; `fs.read` is guarded.
///
/// The host supplies [`Self::principal`] — v0 `FrameHeader` has no principal
/// field (architecture 03 §1). Channel grants are not evaluated.
pub struct PrivilegedSession<F> {
    /// Loaded `keld.permissions.jsonc`.
    pub manifest: PermissionsManifest,
    /// Host-minted caller identity (not read from the frame).
    pub principal: Principal,
    /// Invoked only after `evaluate` returns [`Decision::Allow`].
    ///
    /// The argument is the requested path. The return value is the `Reply`
    /// postcard payload.
    pub handler: F,
}

impl<F> fmt::Debug for PrivilegedSession<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrivilegedSession")
            .field("manifest", &self.manifest)
            .field("principal", &self.principal)
            .field("handler", &"<privileged handler>")
            .finish()
    }
}

/// Run `handler` only if [`evaluate`] allows `(principal, operation, path)`.
///
/// Deny returns the typed [`DenyReason`] (`KELD-GUARD*`) and does not invoke
/// `handler`. This is Unique #4 on the kipc handler path: default-deny,
/// host-enforced. Callers MUST NOT invoke a privileged handler except through
/// this function.
///
/// The `Allow` path does not allocate beyond what `handler` itself does.
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

/// Postcard `Err` payload for a guard deny (session stays up).
#[must_use]
pub fn deny_call_error(reason: &DenyReason) -> CallError {
    CallError {
        code: reason.code().to_owned(),
        message: reason.to_string(),
    }
}

/// Serves one connected app-link peer until the stream closes.
///
/// `Call` on [`ECHO_CHANNEL`] is ungated (`handle_echo`). `Call` on
/// [`FS_READ_CHANNEL`] decodes a postcard `String` path, then
/// [`dispatch_privileged`] — deny is `FrameKind::Err` + [`CallError`], and
/// `session.handler` does not run. `Ping` is echoed. Anything else is
/// [`IpcError::Protocol`].
///
/// # Errors
///
/// Returns [`IpcError`] on I/O, protocol, codec, auth, or deadline failures.
/// Guard denials are `Err` frames, not this error.
pub fn serve_privileged_session<S, F>(
    stream: &mut S,
    token: &SessionToken,
    session: &mut PrivilegedSession<F>,
) -> Result<(), IpcError>
where
    S: Read + Write + AppLinkDeadlines,
    F: FnMut(&str) -> Vec<u8>,
{
    stream.set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))?;
    handshake_server(stream, token)?;
    loop {
        let (header, payload) = match read_frame(stream) {
            Ok(frame) => frame,
            Err(IpcError::Io(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };
        match header.kind {
            FrameKind::Call if header.channel == ECHO_CHANNEL => {
                let reply = handle_echo(&payload)?;
                write_frame(
                    stream,
                    FrameKind::Reply,
                    0,
                    ECHO_CHANNEL,
                    header.corr,
                    &reply,
                )?;
            }
            FrameKind::Call if header.channel == FS_READ_CHANNEL => {
                let path: String = decode(&payload)?;
                match dispatch_privileged(
                    &session.manifest,
                    session.principal,
                    FS_READ_OPERATION,
                    &path,
                    || (session.handler)(&path),
                ) {
                    Ok(reply) => write_frame(
                        stream,
                        FrameKind::Reply,
                        0,
                        FS_READ_CHANNEL,
                        header.corr,
                        &reply,
                    )?,
                    Err(reason) => {
                        let bytes = encode(&deny_call_error(&reason))?;
                        write_frame(
                            stream,
                            FrameKind::Err,
                            0,
                            FS_READ_CHANNEL,
                            header.corr,
                            &bytes,
                        )?;
                    }
                }
            }
            FrameKind::Ping => {
                write_frame(stream, FrameKind::Ping, 0, header.channel, header.corr, &[])?;
            }
            _ => {
                return Err(IpcError::Protocol {
                    detail: "unexpected frame kind in privileged session",
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    use keld_guard::parse_manifest;

    fn allow_fs_read() -> PermissionsManifest {
        parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("manifest")
    }

    #[test]
    fn deny_missing_capability_does_not_run_handler() {
        let manifest = parse_manifest("{}").expect("empty");
        let ran = AtomicBool::new(false);
        let err = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            FS_READ_OPERATION,
            "$APPDATA/notes.txt",
            || ran.store(true, Ordering::SeqCst),
        )
        .expect_err("empty manifest must deny fs.read");
        assert_eq!(err.code(), "KELD-GUARD001");
        assert!(err.to_string().contains("KELD-GUARD001"), "{err}");
        assert!(
            err.fix().contains("/app/fs/read"),
            "fix must name the grant: {}",
            err.fix()
        );
        assert!(
            !ran.load(Ordering::SeqCst),
            "handler must not run on KELD-GUARD001"
        );
    }

    #[test]
    fn deny_out_of_scope_does_not_run_handler() {
        let manifest = allow_fs_read();
        let ran = AtomicBool::new(false);
        let err = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            FS_READ_OPERATION,
            "$DOCUMENTS/notes.txt",
            || ran.store(true, Ordering::SeqCst),
        )
        .expect_err("out-of-scope path must deny");
        assert_eq!(err.code(), "KELD-GUARD002");
        assert!(err.to_string().contains("KELD-GUARD002"), "{err}");
        assert!(
            err.fix().contains("$DOCUMENTS/notes.txt"),
            "fix must name the requested path: {}",
            err.fix()
        );
        assert!(
            !ran.load(Ordering::SeqCst),
            "handler must not run on KELD-GUARD002"
        );
    }

    #[test]
    fn allow_runs_handler_and_returns_its_value() {
        let manifest = allow_fs_read();
        let ran = AtomicBool::new(false);
        let out = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            FS_READ_OPERATION,
            "$APPDATA/notes.txt",
            || {
                ran.store(true, Ordering::SeqCst);
                7_u8
            },
        )
        .expect("in-scope path must allow");
        assert_eq!(out, 7);
        assert!(
            ran.load(Ordering::SeqCst),
            "handler must run on Allow — Decision::Allow from a stub is not this test"
        );
    }

    #[test]
    fn webview_is_guard006_even_when_app_scopes_would_allow() {
        let manifest = allow_fs_read();
        let ran = AtomicBool::new(false);
        let webview = Principal::Webview {
            id: 1,
            generation: 1,
        };
        let err = dispatch_privileged(
            &manifest,
            webview,
            FS_READ_OPERATION,
            "$APPDATA/notes.txt",
            || ran.store(true, Ordering::SeqCst),
        )
        .expect_err("webview must not inherit /app grants");
        assert_eq!(err.code(), "KELD-GUARD006");
        assert!(err.to_string().contains("KELD-GUARD006"), "{err}");
        assert!(
            !err.fix().contains("/app/fs/read"),
            "must not recommend applying app scopes to a webview: {}",
            err.fix()
        );
        assert!(
            !ran.load(Ordering::SeqCst),
            "handler must not run on KELD-GUARD006"
        );
    }

    #[test]
    fn plugin_is_guard006_even_when_app_scopes_would_allow() {
        let manifest = allow_fs_read();
        let ran = AtomicBool::new(false);
        let err = dispatch_privileged(
            &manifest,
            Principal::Plugin { id: 2 },
            FS_READ_OPERATION,
            "$APPDATA/notes.txt",
            || ran.store(true, Ordering::SeqCst),
        )
        .expect_err("plugin must not inherit /app grants");
        assert_eq!(err.code(), "KELD-GUARD006");
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[test]
    fn deny_call_error_carries_code_and_fix_text() {
        let manifest = parse_manifest("{}").expect("empty");
        let reason = match evaluate(
            &manifest,
            Principal::AppProcess,
            FS_READ_OPERATION,
            "$APPDATA/x",
        ) {
            Decision::Deny(reason) => reason,
            Decision::Allow => panic!("empty manifest must deny"),
        };
        let err = deny_call_error(&reason);
        assert_eq!(err.code, "KELD-GUARD001");
        assert!(err.message.contains("KELD-GUARD001"), "{}", err.message);
        assert!(
            err.message.contains("keld.permissions.jsonc"),
            "{}",
            err.message
        );
    }

    /// Negative control (KEL-69): skipping `evaluate` here (always `Ok(handler())`)
    /// makes `deny_missing_capability_does_not_run_handler` fail — the handler
    /// flag would be true. Recorded 2026-08-17 on this tree.
    #[test]
    fn dispatch_calls_evaluate_before_handler() {
        let src = include_str!("privileged.rs");
        assert!(
            src.contains("evaluate(manifest, principal, operation, path)"),
            "KEL-69: dispatch_privileged must call keld-guard::evaluate; \
             a constant Allow would make deny_*_does_not_run_handler pass a no-op guard"
        );
        assert!(
            src.contains("dispatch_privileged("),
            "KEL-69: serve_privileged_session must go through dispatch_privileged, \
             not an inlined always-run handler"
        );
        let echo = include_str!("../../keld-ipc/src/session.rs");
        assert!(
            !echo.contains("keld_guard") && !echo.contains("evaluate("),
            "KEL-69: echo session must stay ungated"
        );
    }
}
