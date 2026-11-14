//! Actual reader EOF/connection-loss facts keep their captured caller through
//! the existing direct owner. They never claim a physical write outcome.

use std::io;
use std::sync::mpsc;
use std::time::Instant;

mod initial;
pub(super) use initial::PendingInitialReadFailure;

use super::primary_writer::FailureDisposition;
use super::{
    DirectPrimaryOwnerCommand, HostAppError, PrimaryCallerContext, PrimaryRouterHandle, app_detail,
    append_app_cleanup, receive_direct_owner_reply_until, with_primary_caller_locked,
};

pub(super) struct PrimaryReadFailure {
    caller: PrimaryCallerContext,
    kind: io::ErrorKind,
    deadline: Instant,
}
impl PrimaryReadFailure {
    /// Called only from the actual reader's existing connection-loss branch.
    /// Protocol/Guard refusals are not reclassified as recoverable EOF.
    pub(super) fn observed(caller: &PrimaryCallerContext, kind: io::ErrorKind) -> Option<Self> {
        matches!(
            kind,
            io::ErrorKind::UnexpectedEof
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
        )
        .then(|| Self {
            caller: caller.clone(),
            kind,
            deadline: Instant::now() + super::DIRECT_PRIMARY_OWNER_REPLY_DEADLINE,
        })
    }
}

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn report_primary_read_failure(
        &self,
        failure: PrimaryReadFailure,
    ) -> Result<(), HostAppError> {
        // This is the same pending-Ready owner/wait. It rechecks the exact caller
        // after every wait and never lets an old reader latch a successor's Ready.
        self.latch_ready_writer_failure_for(&failure.caller);
        let deadline = failure.deadline;
        let (reply, response) = mpsc::sync_channel(1);
        self.guardian
            .command_tx
            .send(DirectPrimaryOwnerCommand::FailPrimaryReader(failure, reply))
            .map_err(|_| app_detail("primary reader recovery", "primary owner stopped"))?;
        receive_direct_owner_reply_until(&response, "primary reader recovery", false, deadline)
    }

    /// The actual supervisor request and pending Echo settlement occur inside
    /// the same final exact-caller cut. No numeric producer precheck is authority.
    pub(in crate::app_session) fn apply_primary_read_failure(
        &self,
        failure: &PrimaryReadFailure,
        restart: impl FnOnce(u32),
    ) -> Result<FailureDisposition, String> {
        if !matches!(
            failure.kind,
            io::ErrorKind::UnexpectedEof
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
        ) {
            return Err(String::from(
                "reader failure is not an actual connection-loss kind",
            ));
        }
        let mut restart = Some(restart);
        let mut disposition = FailureDisposition::Ignored;
        with_primary_caller_locked(&self.current, &self.shutdown, &failure.caller, |_| {
            if Instant::now() >= failure.deadline {
                return Err(app_detail(
                    "primary reader recovery",
                    "the original connection-loss deadline expired",
                ));
            }
            self.fail_pending_echo_for_attempt(
                failure.caller.attempt,
                "application call ended because its original app link disconnected",
            )?;
            if let Some(restart) = restart.take() {
                restart(failure.caller.attempt);
            }
            disposition = FailureDisposition::RestartRequested;
            Ok(())
        })
        .map_err(|error| error.to_string())?;
        Ok(disposition)
    }

    /// Late reader cleanup cannot clear B's Echo or send a global Fatal using
    /// A's equal numeric attempt. Accepted terminal errors remain observable.
    pub(in crate::app_session) fn finish_primary_reader_result(
        &self,
        caller: &PrimaryCallerContext,
        result: Result<(), HostAppError>,
    ) -> Result<(), HostAppError> {
        let Err(primary) = result else {
            return Ok(());
        };
        let mut owned = false;
        let cleanup = with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            owned = true;
            let pending = self.fail_pending_echo_for_attempt(
                caller.attempt,
                "application call ended with its original primary reader",
            );
            let _ = self.window_commands.send(keld_wv::AppWindowCommand::Fatal);
            pending
        });
        if !owned && cleanup.is_ok() && self.shutdown.is_running() {
            return Ok(());
        }
        Err(append_app_cleanup(primary, [cleanup]))
    }
}
