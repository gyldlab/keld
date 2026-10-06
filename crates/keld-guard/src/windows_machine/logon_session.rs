//! The initiating user's logon-session identity (KEL-270 T4d "Machine-UAC owner-loss
//! retirement" and "Machine-UAC bootstrap" item 6).
//!
//! At bootstrap the session must exist with a positive logon time, which the journal
//! records beside the session's LUID. At retirement the recorded session counts as
//! ended only when `LsaGetLogonSessionData` reports `STATUS_NO_SUCH_LOGON_SESSION`, or
//! when the session now holding that LUID has a different logon time, because a LUID is
//! unique only until restart. A live session, access denial and every other status halt.

// SAFETY policy: the LSA query and the release of its buffer are confined to this
// module; callers receive copied values only.
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::fmt;
use std::mem::{offset_of, size_of};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{LUID, NTSTATUS, STATUS_SUCCESS};
use windows_sys::Win32::Security::Authentication::Identity::{
    LsaFreeReturnBuffer, LsaGetLogonSessionData, SECURITY_LOGON_SESSION_DATA,
};

/// `STATUS_NO_SUCH_LOGON_SESSION` from `ntstatus.h`. `windows-sys` defines it only under
/// its `Win32_Security_Credentials` feature, which Keld does not enable; a real-Windows
/// test confirms that a freshly allocated LUID returns exactly this status.
const STATUS_NO_SUCH_LOGON_SESSION: NTSTATUS = 0xC000_005F_u32.cast_signed();
const FIX_QUERY: &str = "Retry the update or recovery as an administrator; Keld accepts \
     only success or STATUS_NO_SUCH_LOGON_SESSION from this query and halts on every other \
     status.";

/// A logon-session LUID, such as `TOKEN_STATISTICS.AuthenticationId`, as
/// `(HighPart << 32) | LowPart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowsLogonSessionId(u64);

impl WindowsLogonSessionId {
    /// Wraps a LUID already encoded as `(HighPart << 32) | LowPart`.
    #[must_use]
    pub const fn from_u64(value: u64) -> Self {
        Self(value)
    }

    /// The LUID as `(HighPart << 32) | LowPart`.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    const fn from_luid(luid: LUID) -> Self {
        let [l0, l1, l2, l3] = luid.LowPart.to_le_bytes();
        let [h0, h1, h2, h3] = luid.HighPart.to_le_bytes();
        Self(u64::from_le_bytes([l0, l1, l2, l3, h0, h1, h2, h3]))
    }

    const fn to_luid(self) -> LUID {
        let [l0, l1, l2, l3, h0, h1, h2, h3] = self.0.to_le_bytes();
        LUID {
            LowPart: u32::from_le_bytes([l0, l1, l2, l3]),
            HighPart: i32::from_le_bytes([h0, h1, h2, h3]),
        }
    }
}

impl fmt::Display for WindowsLogonSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// A positive `SECURITY_LOGON_SESSION_DATA.LogonTime`, the FILETIME at which a logon
/// session began. Zero and negative values are not logon times that Keld records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowsLogonTime(i64);

impl WindowsLogonTime {
    /// Accepts only a positive FILETIME value.
    #[must_use]
    pub const fn new(filetime: i64) -> Option<Self> {
        if filetime > 0 {
            Some(Self(filetime))
        } else {
            None
        }
    }

    /// The FILETIME value.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

/// A logon-session refusal. [`code`](Self::code) is its stable `KELD-GUARD*` code; the
/// `Display` text names the session and ends with the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowsLogonSessionError {
    /// `LsaGetLogonSessionData` returned a status other than success or
    /// `STATUS_NO_SUCH_LOGON_SESSION`, such as access denial (`KELD-GUARD022`).
    QueryStatus {
        /// The queried session.
        session: WindowsLogonSessionId,
        /// The returned NTSTATUS.
        status: NTSTATUS,
    },
    /// The query succeeded but its data was missing, out of bounds, named another
    /// session, or could not be released (`KELD-GUARD022`).
    Malformed {
        /// The queried session.
        session: WindowsLogonSessionId,
        /// Which bound the data broke.
        detail: &'static str,
    },
    /// The recorded session still exists with its recorded logon time (`KELD-GUARD023`).
    SessionLive {
        /// The queried session.
        session: WindowsLogonSessionId,
    },
    /// At bootstrap the initiating session does not exist or has no positive logon time
    /// (`KELD-GUARD024`).
    Unrecordable {
        /// The queried session.
        session: WindowsLogonSessionId,
        /// Why it cannot be recorded.
        detail: &'static str,
    },
}

impl WindowsLogonSessionError {
    /// Stable `KELD-GUARD*` code for this refusal.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::QueryStatus { .. } | Self::Malformed { .. } => "KELD-GUARD022",
            Self::SessionLive { .. } => "KELD-GUARD023",
            Self::Unrecordable { .. } => "KELD-GUARD024",
        }
    }
}

impl fmt::Display for WindowsLogonSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueryStatus { session, status } => write!(
                f,
                "KELD-GUARD022: the query for logon session {session} returned status \
                 0x{:08X}. {FIX_QUERY}",
                status.cast_unsigned()
            ),
            Self::Malformed { session, detail } => write!(
                f,
                "KELD-GUARD022: the query for logon session {session} is unusable: \
                 {detail}. {FIX_QUERY}"
            ),
            Self::SessionLive { session } => write!(
                f,
                "KELD-GUARD023: logon session {session} still exists with its recorded \
                 logon time. Sign out the initiating user or restart Windows, close every \
                 process that holds a token of that session, then run recovery again."
            ),
            Self::Unrecordable { session, detail } => write!(
                f,
                "KELD-GUARD024: initiating logon session {session} cannot be recorded: \
                 {detail}. Start the update again from a live interactive sign-in."
            ),
        }
    }
}

impl std::error::Error for WindowsLogonSessionError {}

/// Reads the positive logon time of the live initiating session, which the journal
/// records before `PublishPending`.
///
/// # Errors
/// [`WindowsLogonSessionError::Unrecordable`] when the session does not exist or its
/// logon time is zero or negative, and [`WindowsLogonSessionError::QueryStatus`] or
/// [`WindowsLogonSessionError::Malformed`] when the query fails.
pub fn windows_initiating_logon_time(
    session: WindowsLogonSessionId,
) -> Result<WindowsLogonTime, WindowsLogonSessionError> {
    initiating_logon_time(session, observe(session, query_logon_session(session))?)
}

/// Requires the recorded initiating session to have ended: no such logon session exists,
/// or the session now holding that LUID began at another time.
///
/// # Errors
/// [`WindowsLogonSessionError::SessionLive`] when the session still exists with
/// `recorded` as its logon time, and [`WindowsLogonSessionError::QueryStatus`] or
/// [`WindowsLogonSessionError::Malformed`] for every other query result, including
/// access denial.
pub fn require_windows_logon_session_ended(
    session: WindowsLogonSessionId,
    recorded: WindowsLogonTime,
) -> Result<(), WindowsLogonSessionError> {
    session_ended(
        session,
        recorded,
        observe(session, query_logon_session(session))?,
    )
}

/// What one `LsaGetLogonSessionData` call returned, copied out of the LSA buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LsaReply {
    status: NTSTATUS,
    session: Option<CopiedSession>,
    released: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CopiedSession {
    logon_id: WindowsLogonSessionId,
    logon_time: i64,
}

/// The one accepted reading of a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Present { logon_time: i64 },
    Absent,
}

fn query_logon_session(session: WindowsLogonSessionId) -> Result<LsaReply, &'static str> {
    let luid = session.to_luid();
    let mut data: *mut SECURITY_LOGON_SESSION_DATA = null_mut();
    // SAFETY: `luid` is a live LUID read for this call and `data` is writable output
    // storage for the LSA-allocated buffer pointer.
    let status = unsafe { LsaGetLogonSessionData(&raw const luid, &raw mut data) };
    if data.is_null() {
        return Ok(LsaReply {
            status,
            session: None,
            released: true,
        });
    }
    // Only a successful query documents the buffer as session data; any other buffer is
    // released unread.
    let copied = (status == STATUS_SUCCESS).then(|| {
        // SAFETY: the non-null buffer is the live LSA allocation that the successful query
        // just returned; the reader checks its self-reported size before LogonTime.
        unsafe { copy_session(data) }
    });
    // SAFETY: `data` is the LSA-allocated buffer returned above, released exactly once
    // and never read again.
    let released = unsafe { LsaFreeReturnBuffer(data.cast()) } == STATUS_SUCCESS;
    Ok(LsaReply {
        status,
        session: copied.transpose()?,
        released,
    })
}

/// Copies the session identity out of an LSA buffer whose `Size` covers `LogonTime`.
///
/// # Safety
/// `data` must point to a live, readable `SECURITY_LOGON_SESSION_DATA` allocation of at
/// least the size it reports, and at least its `Size` field.
unsafe fn copy_session(
    data: *const SECURITY_LOGON_SESSION_DATA,
) -> Result<CopiedSession, &'static str> {
    // SAFETY: the caller guarantees that the `Size` field is readable.
    let size = unsafe { (&raw const (*data).Size).read_unaligned() } as usize;
    if size < offset_of!(SECURITY_LOGON_SESSION_DATA, LogonTime) + size_of::<i64>() {
        return Err("the returned data is shorter than its LogonTime field");
    }
    // SAFETY: the reported size covers LogonId and LogonTime, which precede the end of
    // LogonTime; the caller guarantees that the reported size is readable.
    let (logon_id, logon_time) = unsafe {
        (
            (&raw const (*data).LogonId).read_unaligned(),
            (&raw const (*data).LogonTime).read_unaligned(),
        )
    };
    Ok(CopiedSession {
        logon_id: WindowsLogonSessionId::from_luid(logon_id),
        logon_time,
    })
}

fn observe(
    session: WindowsLogonSessionId,
    reply: Result<LsaReply, &'static str>,
) -> Result<SessionState, WindowsLogonSessionError> {
    let malformed = |detail| WindowsLogonSessionError::Malformed { session, detail };
    let reply = reply.map_err(malformed)?;
    if !reply.released {
        return Err(malformed("its LSA buffer could not be released"));
    }
    match (reply.status, reply.session) {
        (STATUS_SUCCESS, Some(copied)) if copied.logon_id == session => Ok(SessionState::Present {
            logon_time: copied.logon_time,
        }),
        (STATUS_SUCCESS, Some(_)) => Err(malformed("the returned data names another session")),
        (STATUS_SUCCESS, None) => Err(malformed("success returned no session data")),
        (STATUS_NO_SUCH_LOGON_SESSION, _) => Ok(SessionState::Absent),
        (status, _) => Err(WindowsLogonSessionError::QueryStatus { session, status }),
    }
}

fn initiating_logon_time(
    session: WindowsLogonSessionId,
    state: SessionState,
) -> Result<WindowsLogonTime, WindowsLogonSessionError> {
    match state {
        SessionState::Present { logon_time } => {
            WindowsLogonTime::new(logon_time).ok_or(WindowsLogonSessionError::Unrecordable {
                session,
                detail: "its logon time is zero or negative",
            })
        }
        SessionState::Absent => Err(WindowsLogonSessionError::Unrecordable {
            session,
            detail: "no such logon session exists",
        }),
    }
}

fn session_ended(
    session: WindowsLogonSessionId,
    recorded: WindowsLogonTime,
    state: SessionState,
) -> Result<(), WindowsLogonSessionError> {
    match state {
        SessionState::Absent => Ok(()),
        SessionState::Present { logon_time } if logon_time != recorded.get() => Ok(()),
        SessionState::Present { .. } => Err(WindowsLogonSessionError::SessionLive { session }),
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::io::AsHandle as _;

    use windows_sys::Win32::Foundation::STATUS_ACCESS_DENIED;
    use windows_sys::Win32::Security::AllocateLocallyUniqueId;

    use super::super::uac_token::test_support::{current_process_token, luid_u64, token_facts};
    use super::*;

    /// The `LocalSystem` logon session (`SYSTEM_LUID` in `winnt.h`).
    const SYSTEM_LOGON_SESSION: WindowsLogonSessionId = WindowsLogonSessionId::from_u64(0x3e7);
    const SESSION: WindowsLogonSessionId = WindowsLogonSessionId::from_u64(0x0000_0001_2345_6789);
    const OTHER_SESSION: WindowsLogonSessionId =
        WindowsLogonSessionId::from_u64(0x0000_0001_2345_678a);
    const RECORDED_TIME: i64 = 134_355_029_402_464_382;

    fn own_logon_session() -> WindowsLogonSessionId {
        WindowsLogonSessionId::from_u64(
            token_facts(current_process_token().as_handle()).authentication_id,
        )
    }

    fn fresh_luid() -> WindowsLogonSessionId {
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        // SAFETY: `luid` is writable output storage.
        assert_ne!(unsafe { AllocateLocallyUniqueId(&raw mut luid) }, 0);
        WindowsLogonSessionId::from_u64(luid_u64(luid))
    }

    fn reply(status: NTSTATUS, session: Option<(WindowsLogonSessionId, i64)>) -> LsaReply {
        LsaReply {
            status,
            session: session.map(|(logon_id, logon_time)| CopiedSession {
                logon_id,
                logon_time,
            }),
            released: true,
        }
    }

    fn recorded() -> WindowsLogonTime {
        WindowsLogonTime::new(RECORDED_TIME).expect("positive")
    }

    #[test]
    fn logon_session_id_is_high_part_then_low_part() {
        let luid = LUID {
            LowPart: 0x89ab_cdef,
            HighPart: 0x0123_4567,
        };
        let id = WindowsLogonSessionId::from_luid(luid);
        assert_eq!(id.as_u64(), 0x0123_4567_89ab_cdef);
        assert_eq!(id.to_string(), "0123456789abcdef");
        let negative = WindowsLogonSessionId::from_luid(LUID {
            LowPart: 1,
            HighPart: -1,
        });
        assert_eq!(negative.as_u64(), 0xffff_ffff_0000_0001);
        let back = negative.to_luid();
        assert_eq!((back.LowPart, back.HighPart), (1, -1));
        assert_eq!(SYSTEM_LOGON_SESSION.to_string(), "00000000000003e7");
    }

    #[test]
    fn logon_time_accepts_only_positive_filetimes() {
        for rejected in [0, -1, i64::MIN] {
            assert_eq!(WindowsLogonTime::new(rejected), None);
        }
        for accepted in [1, RECORDED_TIME, i64::MAX] {
            assert_eq!(
                WindowsLogonTime::new(accepted).map(WindowsLogonTime::get),
                Some(accepted)
            );
        }
    }

    #[test]
    fn logon_session_refusals_have_exact_codes_and_fix_text() {
        let denied = WindowsLogonSessionError::QueryStatus {
            session: SYSTEM_LOGON_SESSION,
            status: STATUS_ACCESS_DENIED,
        };
        assert_eq!(denied.code(), "KELD-GUARD022");
        assert_eq!(
            denied.to_string(),
            "KELD-GUARD022: the query for logon session 00000000000003e7 returned status \
             0xC0000022. Retry the update or recovery as an administrator; Keld accepts only \
             success or STATUS_NO_SUCH_LOGON_SESSION from this query and halts on every other \
             status."
        );
        let malformed = WindowsLogonSessionError::Malformed {
            session: SESSION,
            detail: "success returned no session data",
        };
        assert_eq!(malformed.code(), "KELD-GUARD022");
        assert_eq!(
            malformed.to_string(),
            "KELD-GUARD022: the query for logon session 0000000123456789 is unusable: \
             success returned no session data. Retry the update or recovery as an \
             administrator; Keld accepts only success or STATUS_NO_SUCH_LOGON_SESSION from \
             this query and halts on every other status."
        );
        let live = WindowsLogonSessionError::SessionLive { session: SESSION };
        assert_eq!(live.code(), "KELD-GUARD023");
        assert_eq!(
            live.to_string(),
            "KELD-GUARD023: logon session 0000000123456789 still exists with its recorded \
             logon time. Sign out the initiating user or restart Windows, close every \
             process that holds a token of that session, then run recovery again."
        );
        let unrecordable = WindowsLogonSessionError::Unrecordable {
            session: SESSION,
            detail: "its logon time is zero or negative",
        };
        assert_eq!(unrecordable.code(), "KELD-GUARD024");
        assert_eq!(
            unrecordable.to_string(),
            "KELD-GUARD024: initiating logon session 0000000123456789 cannot be recorded: \
             its logon time is zero or negative. Start the update again from a live \
             interactive sign-in."
        );
    }

    #[test]
    fn own_logon_session_reads_back_its_luid_and_a_stable_positive_logon_time() {
        let own = own_logon_session();
        let raw = query_logon_session(own).expect("query the own logon session");
        assert_eq!(raw.status, STATUS_SUCCESS);
        assert!(raw.released, "the LSA buffer was released");
        let copied = raw.session.expect("session data");
        assert_eq!(copied.logon_id, own, "the data names the queried session");

        let logon_time = windows_initiating_logon_time(own).expect("record the logon time");
        assert_eq!(logon_time.get(), copied.logon_time);
        assert!(logon_time.get() > 0);
        assert_eq!(windows_initiating_logon_time(own), Ok(logon_time), "stable");

        assert_eq!(
            require_windows_logon_session_ended(own, logon_time),
            Err(WindowsLogonSessionError::SessionLive { session: own }),
            "a live session with its recorded logon time halts"
        );
        let other_time = WindowsLogonTime::new(logon_time.get() - 1).expect("positive");
        assert_eq!(
            require_windows_logon_session_ended(own, other_time),
            Ok(()),
            "the LUID now names a session that began at another time"
        );
    }

    #[test]
    fn fresh_luid_is_no_such_logon_session_on_real_windows() {
        let fresh = fresh_luid();
        let raw = query_logon_session(fresh).expect("query a fresh LUID");
        assert_eq!(raw.status, STATUS_NO_SUCH_LOGON_SESSION);
        assert_eq!(raw.session, None);
        assert_eq!(
            require_windows_logon_session_ended(fresh, recorded()),
            Ok(())
        );
        assert_eq!(
            windows_initiating_logon_time(fresh),
            Err(WindowsLogonSessionError::Unrecordable {
                session: fresh,
                detail: "no such logon session exists",
            })
        );
    }

    #[test]
    fn system_logon_session_is_denied_unless_the_caller_is_elevated() {
        if token_facts(current_process_token().as_handle()).elevated {
            // A local administrator may read any logon session. For LocalSystem the
            // returned data is documented only as "zero", so assert the access decision
            // alone, not a status or data shape the documentation does not state.
            let reply = query_logon_session(SYSTEM_LOGON_SESSION);
            assert!(
                !matches!(
                    reply,
                    Ok(LsaReply {
                        status: STATUS_ACCESS_DENIED,
                        ..
                    })
                ),
                "an administrator must not be denied: {reply:?}"
            );
            return;
        }
        let denied = WindowsLogonSessionError::QueryStatus {
            session: SYSTEM_LOGON_SESSION,
            status: STATUS_ACCESS_DENIED,
        };
        assert_eq!(
            require_windows_logon_session_ended(SYSTEM_LOGON_SESSION, recorded()),
            Err(denied.clone()),
            "denial halts recovery"
        );
        assert_eq!(
            windows_initiating_logon_time(SYSTEM_LOGON_SESSION),
            Err(denied),
            "denial refuses the bootstrap"
        );
    }

    #[test]
    fn seam_injected_statuses_other_than_success_and_no_such_session_halt() {
        for status in [
            STATUS_ACCESS_DENIED,
            0xC000_0001_u32.cast_signed(), // STATUS_UNSUCCESSFUL
            0x0000_0103,                   // STATUS_PENDING, a success-class status
            0x8000_0005_u32.cast_signed(), // STATUS_BUFFER_OVERFLOW, a warning
        ] {
            let halted: Result<SessionState, _> = Err(WindowsLogonSessionError::QueryStatus {
                session: SESSION,
                status,
            });
            assert_eq!(observe(SESSION, Ok(reply(status, None))), halted);
            let with_data = observe(SESSION, Ok(reply(status, Some((SESSION, RECORDED_TIME)))));
            assert_eq!(with_data, halted);
        }
    }

    #[test]
    fn seam_injected_session_results_resolve_both_rules() {
        let absent = observe(SESSION, Ok(reply(STATUS_NO_SUCH_LOGON_SESSION, None)))
            .expect("no such logon session");
        assert_eq!(absent, SessionState::Absent);
        assert_eq!(session_ended(SESSION, recorded(), absent), Ok(()));
        assert!(matches!(
            initiating_logon_time(SESSION, absent),
            Err(WindowsLogonSessionError::Unrecordable { .. })
        ));

        let reused = observe(
            SESSION,
            Ok(reply(STATUS_SUCCESS, Some((SESSION, RECORDED_TIME + 1)))),
        )
        .expect("a session holds the LUID");
        assert_eq!(
            session_ended(SESSION, recorded(), reused),
            Ok(()),
            "LUID reuse with a different logon time"
        );

        let live = observe(
            SESSION,
            Ok(reply(STATUS_SUCCESS, Some((SESSION, RECORDED_TIME)))),
        )
        .expect("the recorded session");
        assert_eq!(
            session_ended(SESSION, recorded(), live),
            Err(WindowsLogonSessionError::SessionLive { session: SESSION })
        );
        assert_eq!(initiating_logon_time(SESSION, live), Ok(recorded()));

        for unrecordable in [0, -1, i64::MIN] {
            let state = observe(
                SESSION,
                Ok(reply(STATUS_SUCCESS, Some((SESSION, unrecordable)))),
            )
            .expect("a session with an unusable logon time");
            assert_eq!(
                initiating_logon_time(SESSION, state),
                Err(WindowsLogonSessionError::Unrecordable {
                    session: SESSION,
                    detail: "its logon time is zero or negative",
                })
            );
            assert_eq!(session_ended(SESSION, recorded(), state), Ok(()));
        }
    }

    #[test]
    fn seam_injected_malformed_replies_halt() {
        let malformed = |detail| {
            Err(WindowsLogonSessionError::Malformed {
                session: SESSION,
                detail,
            })
        };
        assert_eq!(
            observe(
                SESSION,
                Ok(reply(STATUS_SUCCESS, Some((OTHER_SESSION, RECORDED_TIME))))
            ),
            malformed("the returned data names another session")
        );
        assert_eq!(
            observe(SESSION, Ok(reply(STATUS_SUCCESS, None))),
            malformed("success returned no session data")
        );
        let mut unreleased = reply(STATUS_SUCCESS, Some((SESSION, RECORDED_TIME)));
        unreleased.released = false;
        assert_eq!(
            observe(SESSION, Ok(unreleased)),
            malformed("its LSA buffer could not be released")
        );
        assert_eq!(
            observe(
                SESSION,
                Err("the returned data is shorter than its LogonTime field")
            ),
            malformed("the returned data is shorter than its LogonTime field")
        );
    }

    #[test]
    fn copied_session_requires_its_size_to_cover_logon_time() {
        let covering = offset_of!(SECURITY_LOGON_SESSION_DATA, LogonTime) + size_of::<i64>();
        let mut data = SECURITY_LOGON_SESSION_DATA {
            LogonId: SESSION.to_luid(),
            LogonTime: RECORDED_TIME,
            ..SECURITY_LOGON_SESSION_DATA::default()
        };
        for (size, accepted) in [
            (0, false),
            (covering - 1, false),
            (covering, true),
            (size_of::<SECURITY_LOGON_SESSION_DATA>(), true),
        ] {
            data.Size = u32::try_from(size).expect("size fits u32");
            // SAFETY: `data` is a complete live structure, so every reported size up to
            // its own is readable.
            let copied = unsafe { copy_session(&raw const data) };
            if accepted {
                assert_eq!(
                    copied,
                    Ok(CopiedSession {
                        logon_id: SESSION,
                        logon_time: RECORDED_TIME
                    })
                );
            } else {
                assert_eq!(
                    copied,
                    Err("the returned data is shorter than its LogonTime field")
                );
            }
        }
    }
}
