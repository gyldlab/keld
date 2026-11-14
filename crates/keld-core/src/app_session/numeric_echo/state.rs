//! Storage phases of the SAME pending record. No phase grants document or role
//! authority. A read's cancellation marker cannot dispose its storage.
use super::{Body, HostAppError, PrimaryEchoReply, ProgressFailure, SyncSender, TrySendError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Notice {
    Needed,
    Sent,
    Closed,
}

enum Phase<S> {
    Prepared(S),
    Awaiting(S),
    Reading {
        storage: S,
        cancelled: Option<&'static str>,
    },
    ReadEnded {
        storage: S,
        detail: &'static str,
    },
    Ready {
        body: Result<Body, &'static str>,
        storage: S,
        notice: Notice,
    },
}

pub(super) struct State<S> {
    phase: Option<Phase<S>>,
    progress_failure: Option<ProgressFailure>,
}

impl<S> State<S> {
    pub(super) fn prepared(storage: S) -> Self {
        Self {
            phase: Some(Phase::Prepared(storage)),
            progress_failure: None,
        }
    }

    pub(super) fn progress_failure(&self) -> Option<ProgressFailure> {
        self.progress_failure
    }

    pub(super) fn record_progress_failure(&mut self, failure: ProgressFailure) {
        self.progress_failure.get_or_insert(failure);
    }

    pub(super) fn storage(&self) -> Option<&S> {
        match self.phase.as_ref()? {
            Phase::Prepared(storage)
            | Phase::Awaiting(storage)
            | Phase::Reading { storage, .. }
            | Phase::ReadEnded { storage, .. }
            | Phase::Ready { storage, .. } => Some(storage),
        }
    }

    pub(super) fn admitted(&mut self) {
        self.phase = match self.phase.take() {
            Some(Phase::Prepared(storage)) => Some(Phase::Awaiting(storage)),
            other => other,
        };
    }

    pub(super) fn begin_read(&mut self) -> bool {
        match self.phase.take() {
            Some(Phase::Awaiting(storage)) => {
                self.phase = Some(Phase::Reading {
                    storage,
                    cancelled: None,
                });
                true
            }
            other => {
                self.phase = other;
                false
            }
        }
    }

    pub(super) fn retire(&mut self, detail: &'static str) {
        match self.phase.as_mut() {
            Some(Phase::Reading { cancelled, .. }) => {
                cancelled.get_or_insert(detail);
            }
            Some(Phase::Prepared(_) | Phase::Awaiting(_)) => {
                self.phase = match self.phase.take() {
                    Some(Phase::Prepared(storage) | Phase::Awaiting(storage)) => {
                        Some(Phase::ReadEnded { storage, detail })
                    }
                    other => other,
                };
            }
            // A settled result keeps its exact buffers; a later role's policy
            // cannot rewrite it. Native lease/role checks still govern delivery.
            _ => {}
        }
    }

    pub(super) fn read_ended(&mut self, detail: &'static str) {
        self.phase = match self.phase.take() {
            Some(Phase::Reading { storage, cancelled }) => Some(Phase::ReadEnded {
                storage,
                detail: cancelled.unwrap_or(detail),
            }),
            other => other,
        };
    }

    pub(super) fn finish(&mut self, body: Body) -> Result<(), Body> {
        match self.phase.take() {
            Some(Phase::Reading { storage, cancelled }) => {
                if let Some(detail) = cancelled {
                    self.phase = Some(Phase::Ready {
                        body: Err(detail),
                        storage,
                        notice: Notice::Needed,
                    });
                    // The owner is already restored before disposing late bytes.
                    drop(body);
                } else {
                    self.phase = Some(Phase::Ready {
                        body: Ok(body),
                        storage,
                        notice: Notice::Needed,
                    });
                }
                Ok(())
            }
            other => {
                self.phase = other;
                Err(body)
            }
        }
    }

    pub(super) fn materialize(&mut self) {
        self.phase = match self.phase.take() {
            Some(Phase::ReadEnded { storage, detail }) => Some(Phase::Ready {
                body: Err(detail),
                storage,
                notice: Notice::Needed,
            }),
            other => other,
        };
    }

    pub(super) fn send_notice(
        &mut self,
        reply: &SyncSender<Result<PrimaryEchoReply, HostAppError>>,
    ) {
        if self.progress_failure.is_some() {
            return;
        }
        self.materialize();
        if !self.needs_notice() {
            return;
        }
        // The actual body and storage never enter this fallible notification.
        let notice = match reply.try_send(Ok(PrimaryEchoReply::NumericReady)) {
            Ok(()) => Notice::Sent,
            Err(TrySendError::Full(_)) => Notice::Needed,
            Err(TrySendError::Disconnected(_)) => Notice::Closed,
        };
        self.set_notice(notice);
    }

    pub(super) fn restore_ready(&mut self, body: Result<Body, &'static str>, storage: S) {
        self.phase = Some(Phase::Ready {
            body,
            storage,
            notice: Notice::Needed,
        });
    }

    #[cfg(test)]
    pub(super) fn reply_address_for_test(&self) -> Option<*const u8> {
        match self.phase.as_ref() {
            Some(Phase::Ready {
                body: Ok(Body::Reply(bytes)),
                ..
            }) => Some(bytes.as_ptr()),
            _ => None,
        }
    }

    pub(super) fn is_ready(&self) -> bool {
        matches!(self.phase, Some(Phase::Ready { .. }))
    }

    pub(super) fn ready_failure(&self) -> Option<&'static str> {
        match self.phase.as_ref() {
            Some(Phase::Ready {
                body: Err(detail), ..
            }) => Some(*detail),
            _ => None,
        }
    }

    pub(super) fn needs_notice(&self) -> bool {
        matches!(
            self.phase,
            Some(Phase::Ready {
                notice: Notice::Needed,
                ..
            })
        )
    }

    pub(super) fn set_notice(&mut self, result: Notice) {
        if let Some(Phase::Ready { notice, .. }) = self.phase.as_mut() {
            *notice = result;
        }
    }

    pub(super) fn take_ready(&mut self) -> Option<(Result<Body, &'static str>, S)> {
        if self.progress_failure.is_some() {
            return None;
        }
        match self.phase.take() {
            Some(Phase::Ready { body, storage, .. }) => Some((body, storage)),
            other => {
                self.phase = other;
                None
            }
        }
    }
}
