//! At most the first reader's original command waits for actual router attach.
//! This holds no payload and makes no recovery decision before that attachment.

use super::{Instant, PrimaryReadFailure, PrimaryRouterHandle, mpsc};

#[derive(Default)]
pub(in crate::app_session) struct PendingInitialReadFailure {
    command: Option<(PrimaryReadFailure, mpsc::SyncSender<Result<(), String>>)>,
}
impl PendingInitialReadFailure {
    pub(in crate::app_session) fn retain(
        &mut self,
        failure: PrimaryReadFailure,
        reply: mpsc::SyncSender<Result<(), String>>,
    ) {
        if self.command.is_some() {
            let _ = reply.try_send(Err(String::from(
                "an initial reader failure is already awaiting router attachment",
            )));
        } else if Instant::now() >= failure.deadline {
            let _ = reply.try_send(Err(String::from(
                "initial reader failure expired before router attachment",
            )));
        } else {
            self.command = Some((failure, reply));
        }
    }

    pub(in crate::app_session) fn expire(&mut self) {
        self.expire_at(Instant::now());
    }

    fn expire_at(&mut self, now: Instant) {
        if self
            .command
            .as_ref()
            .is_some_and(|(failure, _)| now >= failure.deadline)
        {
            self.stop("initial reader failure expired before router attachment");
        }
    }

    pub(in crate::app_session) fn attached(
        &mut self,
        router: &PrimaryRouterHandle,
        restart: impl FnOnce(u32),
    ) {
        let Some((failure, reply)) = self.command.take() else {
            return;
        };
        let result = if Instant::now() >= failure.deadline {
            Err(String::from(
                "initial reader failure expired before router attachment",
            ))
        } else {
            router
                .apply_primary_read_failure(&failure, restart)
                .map(|_| ())
        };
        let _ = reply.try_send(result);
    }

    pub(in crate::app_session) fn stop(&mut self, detail: &'static str) {
        if let Some((_, reply)) = self.command.take() {
            let _ = reply.try_send(Err(String::from(detail)));
        }
    }
}
impl Drop for PendingInitialReadFailure {
    fn drop(&mut self) {
        self.stop("primary owner ended before evaluating initial reader failure");
    }
}

#[cfg(test)]
mod tests;
