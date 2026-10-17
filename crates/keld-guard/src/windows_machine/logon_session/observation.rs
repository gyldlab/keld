//! Fail-closed interpretation of copied Windows LSA logon-session replies.
//!
//! The parent retains raw LSA FFI and buffer-release ownership. This module
//! cannot dereference a kernel allocation or grant process authority.

use super::{
    STATUS_NO_SUCH_LOGON_SESSION, WindowsLogonSessionError, WindowsLogonSessionId, WindowsLogonTime,
};
use windows_sys::Win32::Foundation::{NTSTATUS, STATUS_SUCCESS};

/// What one `LsaGetLogonSessionData` call returned, copied out of the LSA buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LsaReply {
    pub(super) status: NTSTATUS,
    pub(super) session: Option<CopiedSession>,
    pub(super) released: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CopiedSession {
    pub(super) logon_id: WindowsLogonSessionId,
    pub(super) logon_time: i64,
}

/// The one accepted reading of a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionState {
    Present { logon_time: i64 },
    Absent,
}

pub(super) fn observe(
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

pub(super) fn initiating_logon_time(
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

pub(super) fn session_ended(
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
