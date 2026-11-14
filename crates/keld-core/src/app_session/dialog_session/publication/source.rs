//! Temporary ownership of the one existing settled dialog result.
use super::{
    DialogFault, DialogOwner, DialogPublication, DialogReplyV1, DialogSession, DialogState,
    HostAppError, Instant, PrimaryCallerContext, app_detail,
};

pub(super) struct DialogSource<'a> {
    pub(super) session: &'a DialogSession,
    pub(super) owner: DialogOwner,
    pub(super) caller: PrimaryCallerContext,
    pub(super) deadline: Instant,
    pub(super) result: Option<Result<DialogReplyV1, DialogFault>>,
}

impl<'a> DialogSource<'a> {
    pub(super) fn take(session: &'a DialogSession) -> Result<Option<Self>, HostAppError> {
        let mut state = session
            .state
            .lock()
            .map_err(|_| app_detail("dialog settled result", "custody lock poisoned"))?;
        let Some(record) = state.publication.as_ref() else {
            return Ok(None);
        };
        if record.preparing
            || record.receipt.is_some()
            || state.custody.original_terminal(record.owner).is_none()
        {
            return Ok(None);
        }
        let Some(record) = state.publication.as_mut() else {
            return Ok(None);
        };
        let Some(result) = record.result.take() else {
            return Ok(None);
        };
        record.preparing = true;
        Ok(Some(Self {
            session,
            owner: record.owner,
            caller: record.caller.clone(),
            deadline: record.deadline,
            result: Some(result),
        }))
    }

    pub(super) fn matches(&self, record: &DialogPublication) -> bool {
        record.owner == self.owner && record.preparing && record.receipt.is_none()
    }

    pub(super) fn restore(&mut self, state: &mut DialogState) {
        if !state
            .publication
            .as_ref()
            .is_some_and(|record| self.matches(record))
        {
            return;
        }
        if state.custody.original_terminal(self.owner).is_none() {
            // Retirement canceled this never-admitted source. Free its actual
            // moved paths before exposing the same source slot to another call.
            drop(self.result.take());
            state.publication = None;
            return;
        }
        if let Some(record) = state.publication.as_mut() {
            record.result = self.result.take();
            record.preparing = false;
        }
    }
}

impl Drop for DialogSource<'_> {
    fn drop(&mut self) {
        if self.result.is_none() {
            return;
        }
        let session = self.session;
        if let Ok(mut state) = session.state.lock() {
            self.restore(&mut state);
        }
    }
}
