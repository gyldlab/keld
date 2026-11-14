//! Synchronous producers service only completed owned receipts while at capacity.
//! No native operation, new publication, or recovery-owner wait runs in this hook.
use super::super::PrimaryRouterHandle;
use super::{
    Arc, Class, ControlMessage, HostAppError, Instant, PreparedFrame, PrimaryCallerContext,
    Reservation, WriterLink, WriterRefusal,
};
use std::sync::atomic::Ordering;

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn reserve_primary_value(
        &self,
        caller: &PrimaryCallerContext,
        deadline: Instant,
        phase: &'static str,
    ) -> Result<Reservation, HostAppError> {
        self.reserve_primary_serviced(
            Class::Value,
            deadline,
            || caller.reader_stop.load(Ordering::Acquire) || !self.shutdown.is_running(),
            phase,
        )
    }

    pub(in crate::app_session) fn prepare_primary_control(
        &self,
        caller: &PrimaryCallerContext,
        target: Arc<WriterLink>,
        message: ControlMessage,
        deadline: Instant,
        phase: &'static str,
    ) -> Result<PreparedFrame, HostAppError> {
        let reservation = self.reserve_primary_serviced(
            Class::Control,
            deadline,
            || caller.reader_stop.load(Ordering::Acquire) || !self.shutdown.is_running(),
            phase,
        )?;
        message.encode(reservation, target)
    }

    pub(in crate::app_session) fn reserve_primary_retirement_control(
        &self,
        caller: &PrimaryCallerContext,
        deadline: Instant,
        phase: &'static str,
    ) -> Result<Reservation, HostAppError> {
        self.reserve_primary_serviced(
            Class::Control,
            deadline,
            || caller.reader_stop.load(Ordering::Acquire),
            phase,
        )
    }

    /// Storage only: the final exact accepted-cause/link fence remains mandatory.
    pub(in crate::app_session) fn reserve_primary_tail_control(
        &self,
        deadline: Instant,
        phase: &'static str,
    ) -> Result<Reservation, HostAppError> {
        self.reserve_primary_serviced(Class::Control, deadline, || false, phase)
    }

    fn reserve_primary_serviced(
        &self,
        class: Class,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        phase: &'static str,
    ) -> Result<Reservation, HostAppError> {
        self.primary_writer_owner()?
            .reserve_waiting_serviced(class, true, deadline, cancelled, || {
                self.settle_completed_primary_sources()
            })
            .map_err(|error| error.into_host_error(phase))
    }

    fn settle_completed_primary_sources(&self) -> Result<(), WriterRefusal> {
        let dialog = self.settle_dialog_primary_receipt();
        let compat = self.settle_compat_primary_receipt();
        if matches!(dialog, Ok(true)) || matches!(compat, Ok(true)) {
            // Both source guards ended. A current UI marker may already have
            // been acknowledged before this same-thread capacity wait began.
            self.primary_writer
                .as_ref()
                .ok_or(WriterRefusal::Disconnected)?
                .completed_source_progress()?;
        }
        dialog?;
        compat?;
        Ok(())
    }
}
