//! The initiating user's token, taken from the host process that the KEL-270 D2 bootstrap
//! pinned, the rule that admits only a non-elevated Medium initiating token, its two
//! duplicates and the token-based thread impersonation that pins the staged source
//! ("Machine-UAC bootstrap" items 4 and 5).
//!
//! The token comes only from that process object: never from a thread or pipe
//! impersonation token, a token another process supplies, or a session-token API.
//! Nothing here enables a privilege or changes a token's owner or DACL.

// SAFETY policy: the token open, duplication and thread-token FFI is confined to this
// module; callers receive owned token values and a scoped impersonation, never a raw
// handle, and every path closes its handles and reverts the thread.
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::io;
use std::os::windows::io::{
    AsHandle, AsRawHandle as _, BorrowedHandle, FromRawHandle as _, OwnedHandle,
};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Security::{
    DuplicateTokenEx, RevertToSelf, SecurityImpersonation, TOKEN_ACCESS_MASK, TOKEN_ASSIGN_PRIMARY,
    TOKEN_DUPLICATE, TOKEN_IMPERSONATE, TOKEN_QUERY, TOKEN_TYPE, TokenImpersonation, TokenPrimary,
};
use windows_sys::Win32::System::SystemServices::SECURITY_MANDATORY_MEDIUM_RID;
use windows_sys::Win32::System::Threading::{OpenProcessToken, SetThreadToken};

use super::uac_token::{WindowsTokenError, thread_is_impersonating};

/// Rights on the token opened from the host process: query, and the duplicate and
/// assign-primary rights that `CreateProcessWithTokenW` also requires of its token.
const INITIATING_TOKEN_ACCESS: TOKEN_ACCESS_MASK =
    TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY;
/// `SetThreadToken` requires only `TOKEN_IMPERSONATE` on its token.
const IMPERSONATION_TOKEN_ACCESS: TOKEN_ACCESS_MASK = TOKEN_IMPERSONATE;

/// Requires the initiating token's facts to be those of a non-elevated Medium token
/// ("Machine-UAC bootstrap" item 4). The helper reads `elevated` and `integrity_rid` from
/// [`WindowsInitiatingToken`] with the single token-fact reader,
/// `keld_ipc::query_windows_peer_token_facts`, and calls this before any protected write.
///
/// # Errors
/// [`WindowsTokenError::InitiatingTokenProfile`] for an elevated token or for any
/// integrity level other than exactly Medium.
pub fn require_windows_initiating_token_profile(
    elevated: bool,
    integrity_rid: u32,
) -> Result<(), WindowsTokenError> {
    if !elevated && integrity_rid == SECURITY_MANDATORY_MEDIUM_RID.cast_unsigned() {
        Ok(())
    } else {
        Err(WindowsTokenError::InitiatingTokenProfile {
            elevated,
            integrity_rid,
        })
    }
}

/// The primary token of the host process that the D2 bootstrap pinned, opened with
/// `TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY` and used only to make the two
/// duplicates below.
///
/// It lends its handle only by borrow, for the token-fact reader; the handle closes when
/// this value drops.
#[derive(Debug)]
pub struct WindowsInitiatingToken {
    token: OwnedHandle,
}

impl AsHandle for WindowsInitiatingToken {
    fn as_handle(&self) -> BorrowedHandle<'_> {
        self.token.as_handle()
    }
}

impl WindowsInitiatingToken {
    /// Opens the primary token of `host_process`, a handle to the verified host process
    /// with at least `PROCESS_QUERY_LIMITED_INFORMATION` access.
    ///
    /// # Errors
    /// [`WindowsTokenError::Os`] when Windows refuses the open, for example for a
    /// process handle without query access.
    pub fn open(host_process: BorrowedHandle<'_>) -> Result<Self, WindowsTokenError> {
        let mut raw = null_mut();
        // SAFETY: `host_process` is a live process handle borrowed for this call; `raw` is
        // writable output storage and the access mask names exactly the three rights.
        let opened = unsafe {
            OpenProcessToken(
                host_process.as_raw_handle().cast(),
                INITIATING_TOKEN_ACCESS,
                &raw mut raw,
            )
        };
        if opened == 0 {
            return Err(WindowsTokenError::os(
                "OpenProcessToken(TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY)",
                &io::Error::last_os_error(),
            ));
        }
        Ok(Self {
            token: owned_token(raw, "OpenProcessToken")?,
        })
    }

    /// Duplicates the token to a new primary token for the candidate launch, with the
    /// rights that `CreateProcessWithTokenW` requires: query, duplicate and assign-primary.
    ///
    /// # Errors
    /// [`WindowsTokenError::Os`] when `DuplicateTokenEx` fails.
    pub fn duplicate_primary(&self) -> Result<WindowsInitiatingPrimaryToken, WindowsTokenError> {
        Ok(WindowsInitiatingPrimaryToken {
            token: self.duplicate(INITIATING_TOKEN_ACCESS, TokenPrimary)?,
        })
    }

    /// Duplicates the token to a new impersonation-level token for source pinning,
    /// with only the impersonate right.
    ///
    /// # Errors
    /// [`WindowsTokenError::Os`] when `DuplicateTokenEx` fails.
    pub fn duplicate_impersonation(
        &self,
    ) -> Result<WindowsInitiatingImpersonationToken, WindowsTokenError> {
        Ok(WindowsInitiatingImpersonationToken {
            token: self.duplicate(IMPERSONATION_TOKEN_ACCESS, TokenImpersonation)?,
        })
    }

    fn duplicate(
        &self,
        access: TOKEN_ACCESS_MASK,
        token_type: TOKEN_TYPE,
    ) -> Result<OwnedHandle, WindowsTokenError> {
        let mut raw = null_mut();
        // SAFETY: the source is the live token this value owns, opened with
        // TOKEN_DUPLICATE. Null attributes give the new token a default descriptor and
        // a non-inheritable handle; `raw` is writable output storage.
        let duplicated = unsafe {
            DuplicateTokenEx(
                self.token.as_raw_handle().cast(),
                access,
                null(),
                SecurityImpersonation,
                token_type,
                &raw mut raw,
            )
        };
        if duplicated == 0 {
            return Err(WindowsTokenError::os(
                "DuplicateTokenEx",
                &io::Error::last_os_error(),
            ));
        }
        owned_token(raw, "DuplicateTokenEx")
    }
}

/// A primary duplicate of the initiating token for `CreateProcessWithTokenW`, with query,
/// duplicate and assign-primary rights.
///
/// It lends its handle only by borrow; the handle closes when this value drops.
#[derive(Debug)]
pub struct WindowsInitiatingPrimaryToken {
    token: OwnedHandle,
}

impl AsHandle for WindowsInitiatingPrimaryToken {
    fn as_handle(&self) -> BorrowedHandle<'_> {
        self.token.as_handle()
    }
}

/// An impersonation-level duplicate of the initiating token, used only through
/// [`impersonate`](Self::impersonate).
#[derive(Debug)]
pub struct WindowsInitiatingImpersonationToken {
    token: OwnedHandle,
}

impl WindowsInitiatingImpersonationToken {
    /// Runs `work` on the calling thread while that thread impersonates this token, then
    /// reverts the thread before returning, including when `work` unwinds.
    ///
    /// A failed `RevertToSelf` terminates the process, because the thread would otherwise
    /// keep running under the initiating user's security context.
    ///
    /// # Errors
    /// [`WindowsTokenError::ThreadImpersonating`] when the thread already impersonates,
    /// and [`WindowsTokenError::Os`] when the thread-token query or `SetThreadToken`
    /// fails. `work` does not run on either path.
    pub fn impersonate<R>(&self, work: impl FnOnce() -> R) -> Result<R, WindowsTokenError> {
        self.impersonate_with(revert_to_self, work)
    }

    fn impersonate_with<R>(
        &self,
        revert: fn() -> bool,
        work: impl FnOnce() -> R,
    ) -> Result<R, WindowsTokenError> {
        if thread_is_impersonating()
            .map_err(|source| WindowsTokenError::os("OpenThreadToken(TOKEN_QUERY)", &source))?
        {
            return Err(WindowsTokenError::ThreadImpersonating);
        }
        // Armed before the call, so a failed SetThreadToken still reverts the thread.
        let revert_guard = RevertOnDrop { revert };
        // SAFETY: a null thread pointer selects the calling thread; the token is the live
        // impersonation-level token this value owns, opened with TOKEN_IMPERSONATE.
        let assigned = unsafe { SetThreadToken(null(), self.token.as_raw_handle().cast()) };
        if assigned == 0 {
            let source = io::Error::last_os_error();
            drop(revert_guard);
            return Err(WindowsTokenError::os("SetThreadToken", &source));
        }
        let result = work();
        drop(revert_guard);
        Ok(result)
    }
}

/// Reverts the calling thread when dropped, and terminates the process if that fails.
struct RevertOnDrop {
    revert: fn() -> bool,
}

impl Drop for RevertOnDrop {
    fn drop(&mut self) {
        abort_if_revert_failed((self.revert)());
    }
}

fn revert_to_self() -> bool {
    // SAFETY: RevertToSelf acts on the calling thread only and takes no pointers.
    unsafe { RevertToSelf() != 0 }
}

fn abort_if_revert_failed(reverted: bool) {
    if !reverted {
        // The thread would otherwise continue under the initiating user's token.
        std::process::abort();
    }
}

fn owned_token(
    raw: windows_sys::Win32::Foundation::HANDLE,
    operation: &'static str,
) -> Result<OwnedHandle, WindowsTokenError> {
    if raw.is_null() {
        return Err(WindowsTokenError::os(
            operation,
            &io::Error::other("returned a null token handle"),
        ));
    }
    // SAFETY: the successful call returned one new owned token handle, transferred once.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;
    use std::os::windows::io::AsHandle as _;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::process::{Command, Stdio};

    use std::mem::size_of;

    use windows_sys::Win32::Security::{
        ImpersonateSelf, SID_AND_ATTRIBUTES, SecurityImpersonation, SetTokenInformation,
        TOKEN_ADJUST_DEFAULT, TOKEN_MANDATORY_LABEL, TokenIntegrityLevel, TokenPrimary,
    };
    use windows_sys::Win32::System::SystemServices::SE_GROUP_INTEGRITY;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, OpenProcess, PROCESS_SYNCHRONIZE,
    };

    use super::super::uac_token::test_support::{
        TokenFacts, current_process_token, current_thread_facts, process_token, token_facts,
    };
    use super::*;

    const HOST_CHILD_ENV: &str = "KELD_TEST_INITIATING_TOKEN_HOST_CHILD";
    const REVERT_CHILD_ENV: &str = "KELD_TEST_INITIATING_TOKEN_REVERT_FAILURE_CHILD";
    const UNDER_IMPERSONATION_MARKER: &str = "KELD_TEST_RAN_UNDER_TOKEN_IMPERSONATION";
    const AFTER_REVERT_MARKER: &str = "KELD_TEST_AFTER_FAILED_TOKEN_REVERT";
    /// `STATUS_STACK_BUFFER_OVERRUN`, the exit status of Rust's `process::abort` on Windows.
    const ABORT_EXIT_CODE: i32 = 0xC000_0409_u32.cast_signed();
    /// Mandatory-label RIDs as `winnt.h` spells them, independent of the windows-sys
    /// constant the rule uses.
    const UNTRUSTED_RID: u32 = 0x0000;
    const LOW_RID: u32 = 0x1000;
    const MEDIUM_RID: u32 = 0x2000;
    const MEDIUM_PLUS_RID: u32 = 0x2100;
    const HIGH_RID: u32 = 0x3000;
    const SYSTEM_RID: u32 = 0x4000;
    const PROTECTED_PROCESS_RID: u32 = 0x5000;
    /// `ERROR_BAD_TOKEN_TYPE` from `winerror.h`.
    const ERROR_BAD_TOKEN_TYPE: i32 = 1349;

    fn current_process() -> BorrowedHandle<'static> {
        // SAFETY: the current-process pseudo-handle is valid for the life of the process
        // and is never closed.
        unsafe { BorrowedHandle::borrow_raw(GetCurrentProcess()) }
    }

    fn own_impersonation_token() -> WindowsInitiatingImpersonationToken {
        WindowsInitiatingToken::open(current_process())
            .expect("open this process token")
            .duplicate_impersonation()
            .expect("duplicate an impersonation token")
    }

    /// The facts both duplicates must keep: the logon session, elevation type,
    /// elevation, integrity and session of the token they came from.
    fn identity(facts: TokenFacts) -> (u64, i32, bool, u32, u32) {
        (
            facts.authentication_id,
            facts.elevation_type,
            facts.elevated,
            facts.integrity_rid,
            facts.session_id,
        )
    }

    #[test]
    fn initiating_token_comes_from_the_host_process_and_its_duplicates_keep_its_identity() {
        if std::env::var_os(HOST_CHILD_ENV).is_some() {
            // The child stands in for the host until the parent closes its stdin.
            let mut sink = Vec::new();
            std::io::stdin()
                .read_to_end(&mut sink)
                .expect("read stdin to end");
            return;
        }
        let mut host = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "windows_machine::initiating_token::tests::initiating_token_comes_from_the_host_process_and_its_duplicates_keep_its_identity",
                "--nocapture",
            ])
            .env(HOST_CHILD_ENV, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .expect("start the stand-in host process");
        let host_before = token_facts(process_token(host.as_handle()).as_handle());
        let own = token_facts(current_process_token().as_handle());
        assert_ne!(
            host_before.token_id, own.token_id,
            "the host has its own token"
        );

        let initiating = WindowsInitiatingToken::open(host.as_handle()).expect("open host token");
        let opened = token_facts(initiating.as_handle());
        assert_eq!(
            opened.token_id, host_before.token_id,
            "the token is the host process's own token object"
        );
        let profile =
            require_windows_initiating_token_profile(opened.elevated, opened.integrity_rid);
        if opened.elevated {
            assert_eq!(
                profile,
                Err(WindowsTokenError::InitiatingTokenProfile {
                    elevated: true,
                    integrity_rid: opened.integrity_rid,
                }),
                "an elevated stand-in host is refused"
            );
        } else {
            assert_eq!(
                opened.integrity_rid, MEDIUM_RID,
                "a non-elevated shell is Medium"
            );
            assert_eq!(
                profile,
                Ok(()),
                "the non-elevated Medium host token is admitted"
            );
        }

        let primary = initiating
            .duplicate_primary()
            .expect("duplicate a primary token");
        let primary_facts = token_facts(primary.as_handle());
        assert_eq!(primary_facts.token_type, TokenPrimary);
        assert_ne!(primary_facts.token_id, host_before.token_id, "a new token");
        assert_eq!(identity(primary_facts), identity(host_before));

        let impersonation = initiating
            .duplicate_impersonation()
            .expect("duplicate an impersonation token");
        assert_eq!(current_thread_facts(), None);
        let thread = impersonation
            .impersonate(current_thread_facts)
            .expect("impersonate the initiating token")
            .expect("the thread holds the impersonation token");
        assert_eq!(current_thread_facts(), None, "the thread was reverted");
        assert_eq!(thread.token_type, TokenImpersonation);
        assert_eq!(thread.impersonation_level, SecurityImpersonation);
        assert_ne!(thread.token_id, host_before.token_id, "a new token");
        assert_ne!(thread.token_id, primary_facts.token_id, "a new token");
        assert_eq!(identity(thread), identity(host_before));

        let host_after = token_facts(process_token(host.as_handle()).as_handle());
        assert_eq!(host_after, host_before, "the host token is unmodified");

        drop(host.stdin.take());
        assert!(host.wait().expect("wait for the host").success());
    }

    #[test]
    fn impersonation_token_has_no_query_right() {
        let impersonation = own_impersonation_token();
        let mut elevation = 0u32;
        let mut returned = 0;
        // SAFETY: the token handle is live; both outputs are writable storage.
        let ok = unsafe {
            windows_sys::Win32::Security::GetTokenInformation(
                impersonation.token.as_raw_handle().cast(),
                windows_sys::Win32::Security::TokenElevation,
                (&raw mut elevation).cast(),
                4,
                &raw mut returned,
            )
        };
        assert_eq!(ok, 0, "TOKEN_IMPERSONATE alone must not allow TOKEN_QUERY");
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(5));
    }

    #[test]
    fn open_refuses_a_process_handle_without_query_access() {
        // SAFETY: OpenProcess takes no pointers; the result is checked before ownership.
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, GetCurrentProcessId()) };
        assert!(!raw.is_null(), "open a synchronize-only process handle");
        // SAFETY: OpenProcess returned one new owned handle.
        let synchronize_only = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
        match WindowsInitiatingToken::open(synchronize_only.as_handle()) {
            Err(WindowsTokenError::Os { operation, detail }) => {
                assert_eq!(
                    operation,
                    "OpenProcessToken(TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY)"
                );
                assert!(detail.ends_with("(os error 5)"), "{detail}");
            }
            other => panic!("expected an access-denied refusal, got {other:?}"),
        }
    }

    #[test]
    fn impersonation_reverts_after_success_error_and_unwind() {
        let impersonation = own_impersonation_token();
        let process_identity = identity(token_facts(current_process_token().as_handle()));

        let inside = impersonation
            .impersonate(current_thread_facts)
            .expect("impersonate");
        assert_eq!(identity(inside.expect("thread token")), process_identity);
        assert_eq!(current_thread_facts(), None, "reverted after success");

        let failed: Result<Result<(), &str>, _> =
            impersonation.impersonate(|| Err("source pinning refused"));
        assert_eq!(failed, Ok(Err("source pinning refused")));
        assert_eq!(current_thread_facts(), None, "reverted after a refused pin");

        let unwound = catch_unwind(AssertUnwindSafe(|| {
            impersonation.impersonate(|| panic!("source pinning panicked"))
        }));
        assert!(unwound.is_err());
        assert_eq!(current_thread_facts(), None, "reverted while unwinding");
    }

    #[test]
    fn nested_impersonation_is_refused_and_leaves_the_outer_one_in_place() {
        let impersonation = own_impersonation_token();
        let (nested, still_impersonating) = impersonation
            .impersonate(|| {
                let nested = impersonation.impersonate(|| unreachable!("nested work ran"));
                (nested, current_thread_facts().is_some())
            })
            .expect("outer impersonation");
        assert_eq!(nested.unwrap_err(), WindowsTokenError::ThreadImpersonating);
        assert!(
            still_impersonating,
            "the refusal must not revert the outer token"
        );
        assert_eq!(current_thread_facts(), None);

        // SAFETY: ImpersonateSelf acts on this test thread and is reverted below.
        assert_ne!(unsafe { ImpersonateSelf(SecurityImpersonation) }, 0);
        let refused = impersonation.impersonate(|| unreachable!("work ran"));
        let still_self = current_thread_facts().is_some();
        assert!(revert_to_self(), "restore the test thread");
        assert_eq!(refused.unwrap_err(), WindowsTokenError::ThreadImpersonating);
        assert!(
            still_self,
            "the refusal must not revert the existing impersonation"
        );
    }

    fn injected_revert_failure() -> bool {
        false
    }

    #[test]
    fn failed_revert_terminates_the_process_before_any_later_work() {
        if std::env::var_os(REVERT_CHILD_ENV).is_some() {
            let impersonation = own_impersonation_token();
            let _ = impersonation.impersonate_with(injected_revert_failure, || {
                println!("{UNDER_IMPERSONATION_MARKER}");
            });
            println!("{AFTER_REVERT_MARKER}");
            return;
        }
        let output = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "windows_machine::initiating_token::tests::failed_revert_terminates_the_process_before_any_later_work",
                "--nocapture",
            ])
            .env(REVERT_CHILD_ENV, "1")
            .output()
            .expect("run the fail-closed child");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(ABORT_EXIT_CODE), "{stdout}");
        assert!(stdout.contains(UNDER_IMPERSONATION_MARKER), "{stdout}");
        assert!(!stdout.contains(AFTER_REVERT_MARKER), "{stdout}");
    }

    /// A duplicate of this process's token with `access` and `token_type`, made with
    /// test-only FFI so that it can carry rights the wrapper never requests.
    fn own_token_duplicate(access: TOKEN_ACCESS_MASK, token_type: TOKEN_TYPE) -> OwnedHandle {
        let own = WindowsInitiatingToken::open(current_process()).expect("open this process token");
        let mut raw = null_mut();
        // SAFETY: the source is a live token opened with TOKEN_DUPLICATE; null attributes
        // and writable output storage.
        let ok = unsafe {
            DuplicateTokenEx(
                own.as_handle().as_raw_handle().cast(),
                access,
                null(),
                SecurityImpersonation,
                token_type,
                &raw mut raw,
            )
        };
        assert_ne!(ok, 0, "DuplicateTokenEx: {}", io::Error::last_os_error());
        // SAFETY: the successful duplication returned one new owned handle.
        unsafe { OwnedHandle::from_raw_handle(raw.cast()) }
    }

    /// Lowers `token`'s mandatory label to `S-1-16-<rid>`; lowering needs no privilege.
    fn lower_integrity(token: &OwnedHandle, rid: u32) {
        // Revision 1, one sub-authority, SECURITY_MANDATORY_LABEL_AUTHORITY {0,0,0,0,0,16}.
        let mut sid = [1u8, 1, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0];
        sid[8..].copy_from_slice(&rid.to_le_bytes());
        let label = TOKEN_MANDATORY_LABEL {
            Label: SID_AND_ATTRIBUTES {
                Sid: sid.as_mut_ptr().cast(),
                Attributes: SE_GROUP_INTEGRITY.cast_unsigned(),
            },
        };
        let length = u32::try_from(size_of::<TOKEN_MANDATORY_LABEL>() + sid.len())
            .expect("label length fits u32");
        // SAFETY: the token was opened with TOKEN_ADJUST_DEFAULT; `label` and the SID it
        // points to outlive the call.
        let ok = unsafe {
            SetTokenInformation(
                token.as_raw_handle().cast(),
                TokenIntegrityLevel,
                (&raw const label).cast(),
                length,
            )
        };
        assert_ne!(ok, 0, "SetTokenInformation: {}", io::Error::last_os_error());
    }

    #[test]
    fn initiating_token_profile_admits_only_a_non_elevated_medium_token() {
        assert_eq!(
            require_windows_initiating_token_profile(false, MEDIUM_RID),
            Ok(())
        );
        for integrity_rid in [
            UNTRUSTED_RID,
            LOW_RID,
            MEDIUM_RID - 1,
            MEDIUM_RID + 1,
            MEDIUM_PLUS_RID,
            HIGH_RID,
            SYSTEM_RID,
            PROTECTED_PROCESS_RID,
            u32::MAX,
        ] {
            assert_eq!(
                require_windows_initiating_token_profile(false, integrity_rid),
                Err(WindowsTokenError::InitiatingTokenProfile {
                    elevated: false,
                    integrity_rid,
                }),
                "integrity 0x{integrity_rid:04X}"
            );
        }
        for integrity_rid in [MEDIUM_RID, HIGH_RID] {
            assert_eq!(
                require_windows_initiating_token_profile(true, integrity_rid),
                Err(WindowsTokenError::InitiatingTokenProfile {
                    elevated: true,
                    integrity_rid,
                }),
                "an elevated token at 0x{integrity_rid:04X}"
            );
        }
    }

    #[test]
    fn a_real_low_integrity_token_is_refused() {
        let low = own_token_duplicate(TOKEN_QUERY | TOKEN_ADJUST_DEFAULT, TokenPrimary);
        lower_integrity(&low, LOW_RID);
        let facts = token_facts(low.as_handle());
        assert_eq!(facts.integrity_rid, LOW_RID, "the label was lowered");
        assert_eq!(
            require_windows_initiating_token_profile(facts.elevated, facts.integrity_rid),
            Err(WindowsTokenError::InitiatingTokenProfile {
                elevated: facts.elevated,
                integrity_rid: LOW_RID,
            })
        );
    }

    #[test]
    #[ignore = "operator row: run from an elevated prompt with `cargo test -p keld-guard --lib \
                windows_machine::initiating_token::tests::an_elevated_initiating_token_is_refused \
                -- --ignored --exact`"]
    fn an_elevated_initiating_token_is_refused() {
        let token =
            WindowsInitiatingToken::open(current_process()).expect("open this process token");
        let facts = token_facts(token.as_handle());
        assert!(
            facts.elevated,
            "run this operator row from an elevated prompt"
        );
        assert_eq!(facts.integrity_rid, HIGH_RID);
        assert_eq!(
            require_windows_initiating_token_profile(facts.elevated, facts.integrity_rid),
            Err(WindowsTokenError::InitiatingTokenProfile {
                elevated: true,
                integrity_rid: HIGH_RID,
            })
        );
    }

    #[test]
    fn a_refused_set_thread_token_runs_no_work_and_leaves_no_impersonation() {
        // SetThreadToken takes only an impersonation token: a primary token that carries
        // the impersonate right passes the access check and fails the type check.
        let primary = WindowsInitiatingImpersonationToken {
            token: own_token_duplicate(TOKEN_IMPERSONATE, TokenPrimary),
        };
        let mut ran = false;
        match primary.impersonate(|| ran = true) {
            Err(WindowsTokenError::Os { operation, detail }) => {
                assert_eq!(operation, "SetThreadToken");
                assert!(
                    detail.ends_with(&format!("(os error {ERROR_BAD_TOKEN_TYPE})")),
                    "{detail}"
                );
            }
            other => panic!("expected a SetThreadToken refusal, got {other:?}"),
        }
        assert!(!ran, "work must not run after a refused SetThreadToken");
        assert_eq!(current_thread_facts(), None, "no impersonation remains");

        let after = own_impersonation_token()
            .impersonate(current_thread_facts)
            .expect("the thread still impersonates normally");
        assert!(after.is_some());
        assert_eq!(current_thread_facts(), None);
    }
}
