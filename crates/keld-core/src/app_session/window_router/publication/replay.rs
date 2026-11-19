//! Subscribe owns a scalar cursor and commits only after the final physical receipt.

use super::super::super::primary_writer::ControlMessage;
use super::super::{
    CorrelationId, HostAppError, LifecycleResponse, PrimaryCallerContext, PrimaryRouterHandle,
    app_detail, window_subscribe_refusal,
};
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
        let turn = self.claim_window_publication(deadline, Some(caller))?;
        let result = (|| {
            let Some(target) = self.primary_writer_target(caller)? else {
                return Ok(());
            };
            if let Some(reason) =
                window_subscribe_refusal(channel, caller.identity.is_primary(), turn.is_some())
            {
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
            .replay(correlation)
        })();
        drop(turn);
        self.finish_call_publication(result)
    }
}

impl WindowEmitter<'_, '_> {
    fn replay(&self, correlation: CorrelationId) -> PublicationResult<()> {
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
            return Ok(());
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
                self.admit(
                    self.prepare_fact(&fact)?,
                    |session| {
                        session.subscribed.is_none()
                            && session.registry.matches_publication_fact(owner, &fact)
                    },
                    "Window Created replay completion",
                )?;
                after = window.id.0;
            }
        }

        if self.control(
            ControlMessage::LifecycleReply {
                correlation,
                response: LifecycleResponse::Subscribed,
            },
            |session| {
                if replay_created {
                    session.subscribed.is_none()
                } else {
                    session.subscribed == Some(owner)
                }
            },
            "Window Subscribe completion",
        )? {
            self.with_registry("Window Subscribe commit", |session, _| {
                session.subscribed = Some(owner);
                Ok(())
            })?;
        }
        Ok(())
    }
}
