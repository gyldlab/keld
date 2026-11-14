//! Pure progress of the SAME retained callback/receiver, not a new native owner.

use std::cell::{Cell, OnceCell};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED};

use super::super::{
    ControllerCreationError, ProfileErrorKind, check_launch_deadline, profile_failure_with,
};
use crate::WvError;

enum CallPhase {
    Fresh,
    NeverEntered,
    InCall,
    Returned(windows::core::Result<()>),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum CallbackPhase {
    Absent,
    Entered,
    Returned,
    Unproved,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum WaitPhase {
    Fresh,
    InPump,
    PumpFailed,
    Refused,
    Succeeded,
}

pub(super) struct Completion<C> {
    status: windows::core::Result<()>,
    controller: Option<C>,
}
impl<C> Completion<C> {
    #[cfg(test)]
    pub(super) fn status(&self) -> &windows::core::Result<()> {
        &self.status
    }
    #[cfg(test)]
    pub(super) fn controller(&self) -> Option<&C> {
        self.controller.as_ref()
    }
}
struct CallbackRecord<C> {
    completion: OnceCell<Completion<C>>,
    phase: Cell<CallbackPhase>,
}

/// Exactly one private non-clone port goes into the existing `FnOnce` handler.
/// No other method can initialize its `OnceCell` or mint another completion.
pub(super) struct CompletionPort<C> {
    record: Rc<CallbackRecord<C>>,
    notification: Sender<()>,
}
impl<C> CompletionPort<C> {
    pub(super) fn complete(
        self,
        status: windows::core::Result<()>,
        controller: Option<C>,
        observe: impl FnOnce(&Completion<C>),
    ) -> windows::core::Result<()> {
        // The private one-shot port is the sole initializer. Retain the actual
        // value before any observer, error formatting, notification or native work.
        let retained = self
            .record
            .completion
            .get_or_init(|| Completion { status, controller });
        self.record.phase.set(CallbackPhase::Entered);
        let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observe(retained)));
        // A failed observer still wakes its existing waiter to inspect the stored
        // refusal. The unit is never a completion or release receipt by itself.
        // Keep even notification failure/unwind inside the same native boundary.
        let notified =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.notification.send(())));
        let returned = observed.is_ok() && matches!(notified, Ok(Ok(())));
        self.record.phase.set(if returned {
            CallbackPhase::Returned
        } else {
            CallbackPhase::Unproved
        });
        if returned {
            Ok(())
        } else {
            Err(windows::core::Error::from(E_UNEXPECTED))
        }
    }
}

pub(super) enum Retirement<'a, C> {
    NeverEntered,
    Resolved(Option<&'a C>),
    Unproved,
}

pub(super) struct RequestCore<C> {
    // This exact receiver remains alive even after an immediate native Err,
    // timeout, pump unwind or a callback that has not yet arrived.
    receiver: Receiver<()>,
    record: Rc<CallbackRecord<C>>,
    deadline: Instant,
    call: CallPhase,
    wait: WaitPhase,
}
impl<C> RequestCore<C> {
    pub(super) fn new(deadline: Instant) -> (Self, CompletionPort<C>) {
        let (notification, receiver) = mpsc::channel();
        let record = Rc::new(CallbackRecord {
            completion: OnceCell::new(),
            phase: Cell::new(CallbackPhase::Absent),
        });
        let port = CompletionPort {
            record: Rc::clone(&record),
            notification,
        };
        (
            Self {
                receiver,
                record,
                deadline,
                call: CallPhase::Fresh,
                wait: WaitPhase::Fresh,
            },
            port,
        )
    }

    pub(super) fn begin_with(
        &mut self,
        native: impl FnOnce() -> windows::core::Result<()>,
        after_return: impl FnOnce(),
    ) -> Result<(), ControllerCreationError> {
        if !matches!(self.call, CallPhase::Fresh) {
            return Err(refused("controller creation request was already attempted"));
        }
        self.call = CallPhase::NeverEntered;
        check_launch_deadline(self.deadline).map_err(ControllerCreationError::Pump)?;
        // Set before the existing native entry. Unwind keeps InCall, never an
        // invented native return or permission to discard the callback receiver.
        self.call = CallPhase::InCall;
        self.call = CallPhase::Returned(native());
        after_return();
        self.call_result()
    }

    pub(super) fn wait_with(
        &mut self,
        pump: impl FnOnce(&Receiver<()>, Instant) -> Result<(), WvError>,
    ) -> Result<(), ControllerCreationError> {
        if self.wait != WaitPhase::Fresh {
            return Err(refused("controller creation wait was already attempted"));
        }
        self.call_result()?;
        self.wait = WaitPhase::InPump;
        if let Err(error) = check_launch_deadline(self.deadline) {
            self.wait = WaitPhase::PumpFailed;
            return Err(ControllerCreationError::Pump(error));
        }
        if self.record.completion.get().is_none()
            && let Err(error) = pump(&self.receiver, self.deadline)
        {
            self.wait = WaitPhase::PumpFailed;
            return Err(ControllerCreationError::Pump(error));
        }
        // The original absolute bound still applies if one pumped message ran
        // longer than the clock. Late callback storage is custody, not success.
        if let Err(error) = check_launch_deadline(self.deadline) {
            self.wait = WaitPhase::PumpFailed;
            return Err(ControllerCreationError::Pump(error));
        }
        self.wait = WaitPhase::Refused;
        if self.record.phase.get() != CallbackPhase::Returned {
            return Err(refused(
                "controller callback did not return with proved custody",
            ));
        }
        let completion =
            self.record.completion.get().ok_or_else(|| {
                refused("controller notification has no retained native completion")
            })?;
        completion
            .status
            .as_ref()
            .map_err(|error| ControllerCreationError::Windows(error.clone()))?;
        if completion.controller.is_none() {
            return Err(ControllerCreationError::Windows(
                windows::core::Error::from(E_POINTER),
            ));
        }
        self.wait = WaitPhase::Succeeded;
        Ok(())
    }

    fn call_result(&self) -> Result<(), ControllerCreationError> {
        match &self.call {
            CallPhase::Returned(result) => result
                .as_ref()
                .copied()
                .map_err(|error| ControllerCreationError::Windows(error.clone())),
            _ => Err(refused(
                "controller creation has no actual native call return",
            )),
        }
    }

    pub(super) fn completed_controller(&self) -> Option<&C> {
        if self.wait != WaitPhase::Succeeded
            || !matches!(&self.call, CallPhase::Returned(Ok(())))
            || self.record.phase.get() != CallbackPhase::Returned
        {
            return None;
        }
        self.record.completion.get()?.controller.as_ref()
    }

    pub(super) fn entered_native(&self) -> bool {
        matches!(self.call, CallPhase::InCall | CallPhase::Returned(_))
    }
    pub(super) fn unproved_wait(&self) -> bool {
        matches!(self.wait, WaitPhase::InPump | WaitPhase::PumpFailed)
            || self.record.phase.get() == CallbackPhase::Unproved
    }
    pub(super) fn retirement(&self) -> Retirement<'_, C> {
        match self.call {
            CallPhase::Fresh | CallPhase::NeverEntered => Retirement::NeverEntered,
            CallPhase::InCall => Retirement::Unproved,
            CallPhase::Returned(_) => {
                if self.wait == WaitPhase::InPump
                    || self.record.phase.get() != CallbackPhase::Returned
                {
                    return Retirement::Unproved;
                }
                self.record
                    .completion
                    .get()
                    .map_or(Retirement::Unproved, |completion| {
                        Retirement::Resolved(completion.controller.as_ref())
                    })
            }
        }
    }
}

fn refused(detail: &'static str) -> ControllerCreationError {
    ControllerCreationError::Pump(profile_failure_with(
        ProfileErrorKind::LifecycleUnproven,
        detail,
    ))
}
