//! One scalar cursor in the existing `WindowSession`, with no collected replay.
use super::super::publication_turn::{PendingWindowReplay, ReplayStage};
use super::emitter::fixed_window;
use super::{
    Arc, ControlMessage, CorrelationId, FrameKind, HostAppError, Instant, LIFECYCLE_CHANNEL,
    LifecycleResponse, PrimaryCallerContext, PrimaryRouterHandle, PublicationResult,
    RetainedWindowDocument, SubscribeRefusal, WINDOW_PAYLOAD_STORAGE_BYTES, WindowEmitter,
    WindowEvent, WindowSession, app_detail, subscription_refusal, with_primary_caller_locked,
};

enum ReplayStart {
    Waiting,
    Started,
    Conflict(CorrelationId),
}

enum ReplayItem {
    Desktop(Arc<WindowEvent>),
    Created(WindowEvent),
    Document(RetainedWindowDocument),
    Subscribed,
    Advance,
}

fn replay_matches(
    session: &WindowSession,
    emitter: &WindowEmitter<'_, '_>,
    correlation: CorrelationId,
) -> bool {
    session.publication.replay.as_ref().is_some_and(|pending| {
        Arc::ptr_eq(&pending.target, emitter.target)
            && pending.correlation == correlation
            && pending.deadline == emitter.deadline
    })
}

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn subscribe_window(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        channel: u16,
    ) -> Result<(), HostAppError> {
        self.subscribe_window_until(
            caller,
            correlation,
            channel,
            Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE,
        )
    }

    pub(super) fn subscribe_window_until(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        channel: u16,
        deadline: Instant,
    ) -> Result<(), HostAppError> {
        self.primary_writer_owner()?;
        let turn = self.claim_window_publication(deadline, Some(caller))?;
        let result = (|| {
            let Some(target) = self.primary_writer_target(caller)? else {
                return Ok(());
            };
            let entry = keld_ipc::channel_table::CHANNEL_TABLE
                .iter()
                .find(|entry| entry.wire_id() == channel);
            let mut refusal = entry.map_or(Some(SubscribeRefusal::UnknownChannel), |entry| {
                subscription_refusal(
                    entry.class(),
                    entry.authority(),
                    entry.id() == LIFECYCLE_CHANNEL,
                    caller.identity.is_primary(),
                )
            });
            if refusal.is_none()
                && (channel != keld_ipc::channel_table::WINDOW.wire_id() || turn.is_none())
            {
                refusal = Some(SubscribeRefusal::NotSubscribable);
            }
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
            let emitter = WindowEmitter {
                router: self,
                turn,
                target: &target,
                deadline,
            };
            match emitter.start_replay(caller, correlation)? {
                ReplayStart::Waiting => {}
                ReplayStart::Started => emitter.replay(correlation)?,
                ReplayStart::Conflict(pending_correlation) => {
                    emitter.desktop_pending_conflict(correlation, pending_correlation)?;
                }
            }
            Ok(())
        })();
        drop(turn);
        let result = self.finish_call_publication(result);
        self.finish_window_publication_service(result)
    }
}

impl WindowEmitter<'_, '_> {
    fn start_replay(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
    ) -> Result<ReplayStart, HostAppError> {
        let mut start = ReplayStart::Waiting;
        with_primary_caller_locked(&self.router.current, &self.router.shutdown, caller, |_| {
            let windows = self
                .router
                .windows
                .as_ref()
                .ok_or_else(|| app_detail("Window Subscribe", "Registry disappeared"))?;
            let mut session = windows
                .lock()
                .map_err(|_| app_detail("Window Subscribe", "Registry lock poisoned"))?;
            if !self.turn.owns(windows, &session) {
                return Err(app_detail("Window Subscribe", "foreign source turn"));
            }
            if session.desktop_required && session.subscribed != Some(self.owner()) {
                if let Some(pending) = session.pending_desktop.as_ref() {
                    if !Arc::ptr_eq(&pending.target, self.target) {
                        return Err(app_detail("desktop Subscribe", "foreign pending reader"));
                    }
                    if pending.correlation == correlation {
                        return Err(crate::app_session::app_ipc(
                            "desktop Subscribe correlation admission",
                            &crate::app_session::IpcError::Protocol {
                                detail: "desktop Subscribe reuses its pending correlation",
                            },
                        ));
                    }
                    start = ReplayStart::Conflict(pending.correlation);
                    return Ok(());
                }
                if !session.desktop_seen {
                    session.pending_desktop = Some(super::desktop::PendingDesktopSubscribe {
                        target: Arc::clone(self.target),
                        correlation,
                        deadline: self.deadline,
                    });
                    return Ok(());
                }
            }
            let stage = if session.subscribed == Some(self.owner()) {
                ReplayStage::Subscribed
            } else if session.desktop_required {
                // No adoption before the selected actual desktop has been
                // written. An unavailable sample returns only correlated 049.
                ReplayStage::Desktop
            } else {
                session.registry.subscribe_successor_for_caller(caller);
                session.subscribed = None;
                ReplayStage::Created
            };
            session.publication.replay = Some(PendingWindowReplay {
                target: Arc::clone(self.target),
                correlation,
                deadline: self.deadline,
                stage,
                after: 0,
            });
            start = ReplayStart::Started;
            Ok(())
        })?;
        Ok(start)
    }

    fn commit_subscribed(&self, correlation: CorrelationId) -> Result<(), HostAppError> {
        with_primary_caller_locked(
            &self.router.current,
            &self.router.shutdown,
            self.caller(),
            |_| {
                let windows =
                    self.router.windows.as_ref().ok_or_else(|| {
                        app_detail("Window Subscribe commit", "Registry disappeared")
                    })?;
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("Window Subscribe commit", "Registry lock poisoned"))?;
                if !self.turn.owns(windows, &session)
                    || !replay_matches(&session, self, correlation)
                {
                    return Err(app_detail(
                        "Window Subscribe commit",
                        "original replay record changed",
                    ));
                }
                session.subscribed = Some(self.owner());
                session.publication.replay = None;
                Ok(())
            },
        )
    }

    fn next_replay(&self, correlation: CorrelationId) -> Result<Option<ReplayItem>, HostAppError> {
        let mut item = None;
        with_primary_caller_locked(
            &self.router.current,
            &self.router.shutdown,
            self.caller(),
            |_| {
                let windows = self
                    .router
                    .windows
                    .as_ref()
                    .ok_or_else(|| app_detail("Window replay", "Registry disappeared"))?;
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("Window replay", "Registry lock poisoned"))?;
                if !self.turn.owns(windows, &session)
                    || !replay_matches(&session, self, correlation)
                {
                    return Err(app_detail(
                        "Window replay",
                        "original replay record changed",
                    ));
                }
                let pending = session
                    .publication
                    .replay
                    .as_ref()
                    .ok_or_else(|| app_detail("Window replay", "cursor disappeared"))?;
                let (stage, after) = (pending.stage, pending.after);
                item = Some(match stage {
                    ReplayStage::Desktop => ReplayItem::Desktop(
                        session
                            .desktop_latest
                            .as_ref()
                            .map(Arc::clone)
                            .ok_or_else(|| {
                                app_detail("desktop replay", "selected sample disappeared")
                            })?,
                    ),
                    ReplayStage::Created => {
                        if let Some(fact) = session.registry.next_created_after(self.owner(), after)
                        {
                            ReplayItem::Created(fact)
                        } else {
                            let pending =
                                session.publication.replay.as_mut().ok_or_else(|| {
                                    app_detail("Window replay", "cursor disappeared")
                                })?;
                            pending.stage = ReplayStage::Documents;
                            pending.after = 0;
                            ReplayItem::Advance
                        }
                    }
                    ReplayStage::Documents => {
                        if let Some(document) = session
                            .registry
                            .next_recovered_document_after(self.owner(), after)?
                        {
                            ReplayItem::Document(document)
                        } else {
                            let pending =
                                session.publication.replay.as_mut().ok_or_else(|| {
                                    app_detail("Window replay", "cursor disappeared")
                                })?;
                            pending.stage = ReplayStage::Subscribed;
                            pending.after = 0;
                            ReplayItem::Advance
                        }
                    }
                    ReplayStage::Subscribed => ReplayItem::Subscribed,
                });
                Ok(())
            },
        )?;
        Ok(item)
    }

    fn advance_desktop_replay(
        &self,
        correlation: CorrelationId,
        snapshot: &Arc<WindowEvent>,
    ) -> Result<(), HostAppError> {
        with_primary_caller_locked(
            &self.router.current,
            &self.router.shutdown,
            self.caller(),
            |_| {
                let windows =
                    self.router.windows.as_ref().ok_or_else(|| {
                        app_detail("desktop replay advance", "Registry disappeared")
                    })?;
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("desktop replay advance", "Registry lock poisoned"))?;
                if !self.turn.owns(windows, &session)
                    || !replay_matches(&session, self, correlation)
                    || !super::desktop::retains_snapshot(&session, snapshot)
                    || !session
                        .publication
                        .replay
                        .as_ref()
                        .is_some_and(|pending| pending.stage == ReplayStage::Desktop)
                {
                    return Err(app_detail(
                        "desktop replay advance",
                        "original source changed",
                    ));
                }
                session
                    .registry
                    .subscribe_successor_for_caller(self.caller());
                session.subscribed = None;
                let pending =
                    session.publication.replay.as_mut().ok_or_else(|| {
                        app_detail("desktop replay advance", "cursor disappeared")
                    })?;
                pending.stage = ReplayStage::Created;
                pending.after = 0;
                Ok(())
            },
        )
    }

    fn advance_replay(&self, correlation: CorrelationId, after: u32) -> Result<(), HostAppError> {
        with_primary_caller_locked(
            &self.router.current,
            &self.router.shutdown,
            self.caller(),
            |_| {
                let windows =
                    self.router.windows.as_ref().ok_or_else(|| {
                        app_detail("Window replay cursor", "Registry disappeared")
                    })?;
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("Window replay cursor", "Registry lock poisoned"))?;
                if !self.turn.owns(windows, &session)
                    || !replay_matches(&session, self, correlation)
                {
                    return Err(app_detail(
                        "Window replay cursor",
                        "original replay record changed",
                    ));
                }
                let pending = session
                    .publication
                    .replay
                    .as_mut()
                    .ok_or_else(|| app_detail("Window replay cursor", "cursor disappeared"))?;
                pending.after = after;
                Ok(())
            },
        )
    }

    pub(super) fn replay(&self, correlation: CorrelationId) -> PublicationResult<()> {
        while let Some(item) = self.next_replay(correlation)? {
            match item {
                ReplayItem::Advance => {}
                ReplayItem::Desktop(snapshot) => {
                    let WindowEvent::DesktopSnapshot {
                        snapshot: facts, ..
                    } = snapshot.as_ref()
                    else {
                        return Err(app_detail("desktop replay", "foreign snapshot variant").into());
                    };
                    if facts.is_none() {
                        self.desktop_unavailable(correlation, &snapshot, |session| {
                            replay_matches(session, self, correlation)
                        })?;
                        return Ok(());
                    }
                    if !self.desktop_snapshot(&snapshot, |session| {
                        replay_matches(session, self, correlation)
                    })? {
                        return Ok(());
                    }
                    self.advance_desktop_replay(correlation, &snapshot)?;
                }
                ReplayItem::Created(fact) => {
                    let window = fixed_window(&fact)
                        .ok_or_else(|| app_detail("Window replay", "non-Created cursor fact"))?;
                    let reservation = self.router.reserve_primary_value(
                        self.caller(),
                        self.deadline,
                        "Window Created replay reservation",
                    )?;
                    let frame = reservation.encode_value(
                        Arc::clone(self.target),
                        FrameKind::Event,
                        keld_ipc::channel_table::WINDOW.id(),
                        CorrelationId(0),
                        &fact,
                        WINDOW_PAYLOAD_STORAGE_BYTES,
                    )?;
                    self.admit(
                        frame,
                        |session| {
                            replay_matches(session, self, correlation)
                                && session
                                    .registry
                                    .retains_caller_window(self.caller(), window)
                                && session
                                    .registry
                                    .matches_created_snapshot(self.owner(), &fact)
                        },
                        "Window Created replay completion",
                    )?;
                    self.advance_replay(correlation, window.id.0)?;
                }
                ReplayItem::Document(document) => {
                    self.document(&document, true)?;
                    self.advance_replay(correlation, document.window.id.0)?;
                }
                ReplayItem::Subscribed => {
                    let written = self.control(
                        ControlMessage::LifecycleReply {
                            correlation,
                            response: LifecycleResponse::Subscribed,
                        },
                        |session| {
                            replay_matches(session, self, correlation)
                                && session
                                    .publication
                                    .replay
                                    .as_ref()
                                    .is_some_and(|p| p.stage == ReplayStage::Subscribed)
                        },
                        "Window Subscribed completion",
                    )?;
                    if written {
                        self.commit_subscribed(correlation)?;
                    }
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}
