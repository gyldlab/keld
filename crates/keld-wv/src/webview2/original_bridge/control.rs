//! Single charged startup CONTROL work on the current native position.
use super::current::Action;
use super::post::{PostAuthority, PostResult, PreparedControl};
use super::startup::BoundEntry;
use super::state::State;
use super::{Instant, OriginalBridge, WvError, failure};
use crate::renderer_bridge::{NativeControlReservation, RendererRoleWitness};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::wv_link::HostRendererMessage;

#[derive(Clone, Copy)]
enum StartupKind {
    Bound(BoundEntry),
    Registered,
}

impl StartupKind {
    const fn carrier(self) -> HostRendererMessage<'static> {
        match self {
            Self::Bound(_) => HostRendererMessage::Bound { ready: false },
            Self::Registered => HostRendererMessage::AppRegistered,
        }
    }
}

pub(super) struct StartupControl {
    kind: StartupKind,
    document: CheckedNativeDocumentLease,
    role: RendererRoleWitness,
    epoch: [u8; 32],
    prepared: Option<PreparedControl>,
    charge: Option<NativeControlReservation>,
}

impl StartupControl {
    pub(super) fn dispose_if_quiescent(&mut self) -> Result<Action, WvError> {
        if let Some(prepared) = self.prepared.take() {
            match prepared.into_charge_after_close() {
                Ok(charge) => self.charge = Some(charge),
                Err(retained) => {
                    self.prepared = Some(retained);
                    return Ok(Action::Keep);
                }
            }
        }
        Ok(Action::Dispose)
    }

    fn drive(&mut self, bridge: &OriginalBridge) -> Result<Action, WvError> {
        if bridge.retired.get()
            || bridge.faulted.get()
            || !self.document.is_current()
            || !self.role.is_live()
        {
            return self.dispose_if_quiescent();
        }
        if self.prepared.is_none() {
            let charge = self
                .charge
                .take()
                .ok_or_else(|| failure("startup publication lost its Core CONTROL charge"))?;
            let epoch = std::str::from_utf8(&self.epoch)
                .map_err(|_| failure("startup epoch is not UTF-8"))?;
            match PreparedControl::prepare(
                &self.kind.carrier(),
                epoch,
                self.document.clone(),
                PostAuthority::Current(self.role.clone()),
                charge,
            ) {
                Ok(prepared) => self.prepared = Some(prepared),
                Err(error) => {
                    self.charge = Some(error.charge);
                    return Err(failure(error.detail));
                }
            }
        }
        let kind = self.kind;
        let prepared = self
            .prepared
            .as_mut()
            .ok_or_else(|| failure("prepared startup CONTROL disappeared"))?;
        let result = prepared.post_with_entry(&bridge.resource, &mut |lease, epoch, owner| {
            let Ok(mut state) = bridge.state.try_borrow_mut() else {
                return false;
            };
            if bridge.retired.get()
                || bridge.faulted.get()
                || !state.matches(owner, lease, epoch, matches!(kind, StartupKind::Registered))
            {
                return false;
            }
            match kind {
                StartupKind::Bound(entry) => {
                    let State {
                        startup, binding, ..
                    } = &mut *state;
                    startup.as_mut().is_some_and(|receipt| {
                        receipt.begin_bound(entry, binding, owner, Instant::now())
                    })
                }
                StartupKind::Registered => {
                    if !state.registration_ack_pending
                        || state.binding.registration_notice().is_some()
                    {
                        return false;
                    }
                    state.registration_ack_pending = false;
                    true
                }
            }
        });
        match result {
            PostResult::Returned(Ok(())) | PostResult::AlreadyReturned => {
                self.dispose_if_quiescent()
            }
            PostResult::Returned(Err(_)) => {
                Err(failure("native startup CONTROL returned a failure"))
            }
            PostResult::NotAdmitted => self.dispose_if_quiescent(),
            PostResult::OutcomeUnknown => Err(failure(
                "native startup CONTROL return is unproved; custody retained",
            )),
        }
    }
}

struct DriveTurn<'a>(&'a OriginalBridge);
impl Drop for DriveTurn<'_> {
    fn drop(&mut self) {
        self.0.driving.set(false);
    }
}

impl OriginalBridge {
    pub(super) fn drive_on_ui(&self) -> Result<(), WvError> {
        if self.callback_depth.get().is_none() {
            self.fail("original bridge lost callback depth");
            return Err(failure("original bridge lost callback depth"));
        }
        if self.callback_depth.get() != Some(0) || self.capturing.get() || self.installing.get() {
            return Ok(());
        }
        if self.driving.replace(true) {
            self.fail("selected startup service reentered");
            return Err(failure("selected startup service reentered"));
        }
        let _turn = DriveTurn(self);
        let result = (|| {
            if self.retired.get() {
                self.clear_closed_registrations();
                if !self.current.is_empty() {
                    self.current.drive(StartupControl::dispose_if_quiescent)?;
                }
                return Ok(());
            }
            if self.faulted.get() {
                return Err(failure("selected startup bridge is faulted"));
            }
            self.synchronize_commit()?;
            self.flush_registration()?;
            self.stage_control()?;
            if !self.current.is_empty() {
                self.current.drive(|control| control.drive(self))?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.fail("selected startup service failed");
        }
        result
    }

    fn stage_control(&self) -> Result<(), WvError> {
        if !self.current.is_empty() {
            return Ok(());
        }
        let (kind, document, role, epoch) = {
            let nav = self
                .navigation
                .try_borrow()
                .map_err(|_| failure("startup owner borrowed"))?;
            let state = self
                .state
                .try_borrow()
                .map_err(|_| failure("startup state borrowed"))?;
            let Some(document) = nav.current_document() else {
                return Ok(());
            };
            let Some(startup) = state.startup.as_ref() else {
                return Ok(());
            };
            let now = Instant::now();
            if startup.expired(&state.binding, nav.document(), now) {
                return Err(failure(
                    "actual selected document startup receipt timed out",
                ));
            }
            let kind = if state.registration_ack_pending
                && state.binding.registration_notice().is_none()
            {
                Some(StartupKind::Registered)
            } else {
                startup
                    .next_bound(&state.binding, nav.document(), now)
                    .map(StartupKind::Bound)
            };
            let Some(kind) = kind else {
                return Ok(());
            };
            let role = nav
                .host_role()
                .filter(|role| role.is_live())
                .cloned()
                .ok_or_else(|| failure("startup lost its actual load caller"))?;
            (kind, document, role, state.epoch()?)
        };
        let text =
            std::str::from_utf8(&epoch).map_err(|_| failure("startup epoch is not UTF-8"))?;
        let encoded = kind
            .carrier()
            .encoded_len(text)
            .map_err(|_| failure("invalid canonical startup control"))?;
        let Some(charge) =
            role.try_reserve_native_control(encoded, 1024 + std::mem::size_of::<StartupControl>())
        else {
            return Ok(());
        };
        let mut pending = Some(StartupControl {
            kind,
            document,
            role,
            epoch,
            prepared: None,
            charge: Some(charge),
        });
        if !self.current.attach(&mut pending, |value| value) {
            return Err(failure("startup control position became occupied"));
        }
        Ok(())
    }
}
