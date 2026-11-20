//! Subscribe owns a scalar cursor and commits only after the final physical receipt.

use super::super::super::primary_writer::ControlMessage;
use super::super::{
    CorrelationId, HostAppError, LifecycleResponse, PrimaryCallerContext, PrimaryRouterHandle,
    app_detail, window_subscribe_refusal,
};
use super::desktop::DesktopSubscribe;
use super::{PublicationResult, WindowEmitter};
use keld_ipc::window::WindowEvent;
use std::sync::Arc;
use std::time::Instant;

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn subscribe_window_ordered(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        channel: u16,
    ) -> Result<(), HostAppError> {
        self.primary_writer_owner()?;
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let refusal = window_subscribe_refusal(
            channel,
            caller.identity.is_primary(),
            self.windows.is_some(),
        );
        if refusal.is_none() && self.selected_desktop_enabled()? {
            let Some(target) = self.primary_writer_target(caller)? else {
                return Ok(());
            };
            match self.defer_selected_desktop_subscribe(&target, correlation)? {
                DesktopSubscribe::Ordinary => {}
                DesktopSubscribe::Deferred => {
                    return self
                        .primary_writer_owner()?
                        .completed_source_progress()
                        .map_err(|error| error.into_host_error("desktop Subscribe progress"));
                }
                DesktopSubscribe::Competing => {
                    let Some(turn) = self.claim_window_publication(deadline, Some(caller))? else {
                        return Ok(());
                    };
                    let emitter = WindowEmitter {
                        router: self,
                        turn: &turn,
                        target: &target,
                        deadline,
                    };
                    let refusal = emitter.desktop_error(
                        correlation, "KELD-IPC-005",
                        "KELD-IPC-005: a selected desktop Subscribe already owns another correlation",
                        |_| true,
                    ).map(drop);
                    drop(turn);
                    return self.finish_call_publication(refusal);
                }
            }
        }
        let turn = self.claim_window_publication(deadline, Some(caller))?;
        let result = (|| {
            let Some(target) = self.primary_writer_target(caller)? else {
                return Ok(());
            };
            if let Some(reason) = refusal {
                let frame = self.prepare_primary_control(
                    caller,
                    Arc::clone(&target),
                    ControlMessage::LifecycleReply {
                        correlation,
                        response: LifecycleResponse::SubscribeRefused { reason },
                    },
                    deadline,
                    "Window Subscribe refusal preparation",
                )?;
                if let Some(receipt) = self.admit_primary_frame(caller, frame)? {
                    self.observe_publication(
                        &target,
                        receipt,
                        deadline,
                        "Window Subscribe refusal completion",
                    )?;
                }
                return Ok(());
            }
            let turn = turn
                .as_ref()
                .ok_or_else(|| app_detail("Window Subscribe", "accepted Registry disappeared"))?;
            WindowEmitter {
                router: self,
                turn,
                target: &target,
                deadline,
            }
            .replay(correlation, false)
            .map(drop)
        })();
        drop(turn);
        self.finish_call_publication(result)
    }
}

impl WindowEmitter<'_, '_> {
    pub(super) fn replay(
        &self,
        correlation: CorrelationId,
        selected: bool,
    ) -> PublicationResult<bool> {
        let owner = self.owner();
        let Some(replay_created) =
            self.with_registry("Window Subscribe admission", |session, _| {
                let replay = session.subscribed != Some(owner);
                if replay {
                    session.registry.subscribe_successor(owner);
                    session.subscribed = None;
                }
                Ok(replay)
            })?
        else {
            return Ok(false);
        };

        if replay_created {
            let mut after = 0;
            while let Some(fact) = self
                .with_registry("Window Created replay", |session, _| {
                    Ok(session.registry.next_created_after(owner, after))
                })?
                .flatten()
            {
                let WindowEvent::Created { window, .. } = &fact else {
                    return Err(app_detail("Window Created replay", "foreign cursor fact").into());
                };
                let frame = self.prepare_fact(&fact)?;
                let check = |session: &super::super::WindowSession| {
                    session.subscribed.is_none()
                        && session.registry.matches_publication_fact(owner, &fact)
                };
                let phase = "Window Created replay completion";
                if selected {
                    if !self.admit_by_deadline(frame, check, phase)? {
                        return Ok(false);
                    }
                } else if !self.admit(frame, check, phase)? {
                    return Ok(false);
                }
                after = window.id.0;
            }

            // Finish the entire Created census before exposing any selected
            // associations. The Registry holds the only retained physical view.
            let mut after = 0;
            while let Some((window, view)) = self
                .with_registry("Window binding replay", |session, _| {
                    Ok(session.registry.next_original_view_after(owner, after))
                })?
                .flatten()
            {
                let frame = self.prepare_original_view_binding(window, view)?;
                let check = |session: &super::super::WindowSession| {
                    session.subscribed.is_none()
                        && session
                            .registry
                            .original_view_for_publication(owner, window)
                            == Some(view)
                };
                let phase = "Window binding replay completion";
                if selected {
                    if !self.admit_by_deadline(frame, check, phase)? {
                        return Ok(false);
                    }
                } else if !self.admit(frame, check, phase)? {
                    return Ok(false);
                }
                after = window.id.0;
            }
        }

        let message = ControlMessage::LifecycleReply {
            correlation,
            response: LifecycleResponse::Subscribed,
        };
        let check = |session: &super::super::WindowSession| {
            if replay_created {
                session.subscribed.is_none()
            } else {
                session.subscribed == Some(owner)
            }
        };
        let phase = "Window Subscribe completion";
        let accepted = if selected {
            let frame = self.router.prepare_primary_control(
                self.caller(),
                Arc::clone(self.target),
                message,
                self.deadline,
                phase,
            )?;
            self.admit_by_deadline(frame, check, phase)?
        } else {
            self.control(message, check, phase)?
        };
        if accepted {
            return Ok(self
                .with_registry("Window Subscribe commit", |session, _| {
                    session.subscribed = Some(owner);
                    Ok(())
                })?
                .is_some());
        }
        Ok(false)
    }
}
