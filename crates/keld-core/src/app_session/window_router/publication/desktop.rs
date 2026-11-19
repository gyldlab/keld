//! The selected desktop sample and one pending Subscribe stay in the Window owner.

use super::super::super::primary_writer::WriterLink;
use super::super::{
    CallError, CorrelationId, FrameKind, HostAppError, LIFECYCLE_CHANNEL, PrimaryCallerContext,
    PrimaryRouterHandle, WindowOwner, WindowSession, app_detail, app_ipc,
    with_primary_caller_locked,
};
use super::desktop_time::DesktopSubscribeClock;
use super::{NativePublicationDisposition, PublicationResult, WindowEmitter};
use keld_ipc::IpcError;
use keld_ipc::window::{DesktopFact, DesktopFacts, Rect, WindowEvent};
use keld_wv::{AppDesktopCapture, AppDesktopFacts};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(in crate::app_session::window_router) struct SelectedDesktop {
    owner: Option<WindowOwner>,
    initial_closed: bool,
    init_seen: bool,
    epoch: u64,
    sample: Option<DesktopSample>,
    pending: Option<PendingDesktopSubscribe>,
}

struct DesktopSample {
    event: Arc<WindowEvent>,
    validated_at: Instant,
    published: bool,
}

struct PendingDesktopSubscribe {
    target: Arc<WriterLink>,
    correlation: CorrelationId,
    clock: DesktopSubscribeClock,
}

pub(super) enum DesktopSubscribe {
    Ordinary,
    Deferred,
    Competing { original: CorrelationId },
}

pub(in crate::app_session) enum DesktopStartup {
    Waiting,
    Ready {
        target: Arc<WriterLink>,
        deadline: Instant,
    },
    FatalQueued(HostAppError),
}

impl SelectedDesktop {
    pub(in crate::app_session::window_router) fn retire(&mut self, owner: WindowOwner) {
        if self.owner == Some(owner) {
            self.owner = None;
            self.initial_closed = true;
            self.sample = None;
            self.pending = None;
        }
    }

    fn observation(
        &mut self,
        result: Option<DesktopFacts>,
        now: Instant,
    ) -> Result<(), HostAppError> {
        let epoch = self.epoch.checked_add(1).ok_or_else(|| {
            app_detail(
                "desktop observation",
                "the Host session desktop epoch exhausted",
            )
        })?;
        // This is a checked native sample or explicitly unavailable. The sole
        // primary writer owns all serialized output bytes.
        self.sample = Some(DesktopSample {
            event: Arc::new(WindowEvent::DesktopSnapshot {
                epoch,
                snapshot: result,
            }),
            validated_at: now,
            published: false,
        });
        self.epoch = epoch;
        Ok(())
    }

    fn matches_pending(&self, target: &Arc<WriterLink>, correlation: CorrelationId) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            Arc::ptr_eq(&pending.target, target) && pending.correlation == correlation
        })
    }
}

struct DesktopSource {
    target: Arc<WriterLink>,
    event: Arc<WindowEvent>,
    correlation: Option<CorrelationId>,
    deadline: Instant,
    caller_deadline: Option<Instant>,
}

fn unavailable() -> HostAppError {
    HostAppError::new(
        "KELD-CORE-049",
        "selected desktop Subscribe",
        "The Host's native desktop display or appearance snapshot is unavailable",
        "Correct the native display or appearance capture error reported by the Host and relaunch.",
    )
}

fn qualified_sample(facts: AppDesktopFacts) -> Option<DesktopFacts> {
    if facts.displays.len() != 1 || facts.displays.capacity() > 32 || facts.should_use_dark_colors {
        return None;
    }
    let display = facts.displays.into_iter().next()?;
    if display.id == 0
        || display.id != facts.primary_id
        || display.bounds.x != 0
        || display.bounds.y != 0
        || display.bounds.width == 0
        || display.bounds.height == 0
        || display.work_area.width == 0
        || display.work_area.height == 0
        || !display.scale_factor.is_finite()
        || display.scale_factor <= 0.0
    {
        return None;
    }
    let bounds = Rect {
        x: display.bounds.x,
        y: display.bounds.y,
        width: display.bounds.width,
        height: display.bounds.height,
    };
    let work_area = Rect {
        x: display.work_area.x,
        y: display.work_area.y,
        width: display.work_area.width,
        height: display.work_area.height,
    };
    Some(DesktopFacts {
        primary_id: display.id,
        displays: vec![DesktopFact {
            id: display.id,
            bounds,
            work_area,
            scale_factor: display.scale_factor,
        }],
        should_use_dark_colors: facts.should_use_dark_colors,
    })
}

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn selected_desktop_enabled(&self) -> Result<bool, HostAppError> {
        self.windows.as_ref().map_or(Ok(false), |windows| {
            windows
                .lock()
                .map(|session| session.desktop.is_some())
                .map_err(|_| app_detail("selected desktop", "Registry lock poisoned"))
        })
    }

    /// The installer already holds the canonical transition and current lock.
    pub(in crate::app_session) fn bind_selected_desktop(
        &self,
        caller: &PrimaryCallerContext,
    ) -> Result<(), HostAppError> {
        let Some(windows) = self.windows.as_ref() else {
            return Ok(());
        };
        let mut session = windows
            .lock()
            .map_err(|_| app_detail("selected desktop", "Registry lock poisoned"))?;
        let Some(desktop) = session.desktop.as_mut() else {
            return Ok(());
        };
        self.window_commands
            .send(keld_wv::AppWindowCommand::WindowsDesktopProducer {
                capture: AppDesktopCapture::from_current(
                    caller.attempt,
                    Arc::clone(&caller.reader_stop),
                ),
            })
            .map_err(|_| app_detail("selected desktop", "native producer owner ended"))?;
        desktop.owner = Some(WindowOwner::new(caller.identity, caller.attempt));
        Ok(())
    }

    pub(super) fn defer_selected_desktop_subscribe(
        &self,
        target: &Arc<WriterLink>,
        correlation: CorrelationId,
    ) -> Result<DesktopSubscribe, HostAppError> {
        let caller = target.caller();
        let mut outcome = DesktopSubscribe::Deferred;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            let Some(windows) = self.windows.as_ref() else {
                outcome = DesktopSubscribe::Ordinary;
                return Ok(());
            };
            let mut session = windows
                .lock()
                .map_err(|_| app_detail("desktop Subscribe", "Registry lock poisoned"))?;
            let owner = WindowOwner::new(caller.identity, caller.attempt);
            if session.subscribed == Some(owner) || session.desktop.is_none() {
                outcome = DesktopSubscribe::Ordinary;
                return Ok(());
            }
            let Some(desktop) = session
                .desktop
                .as_mut()
                .filter(|desktop| desktop.owner == Some(owner))
            else {
                return Ok(());
            };
            if let Some(pending) = desktop.pending.as_ref() {
                if Arc::ptr_eq(&pending.target, target) && pending.correlation != correlation {
                    outcome = DesktopSubscribe::Competing {
                        original: pending.correlation,
                    };
                    return Ok(());
                }
                return Err(app_ipc(
                    "desktop Subscribe correlation",
                    &IpcError::Protocol {
                        detail: "desktop Subscribe reuses its pending correlation",
                    },
                ));
            }
            desktop.pending = Some(PendingDesktopSubscribe {
                target: Arc::clone(target),
                correlation,
                clock: DesktopSubscribeClock::new(Instant::now()),
            });
            Ok(())
        })?;
        Ok(outcome)
    }

    pub(in crate::app_session) fn observe_selected_desktop(
        &self,
        capture: Option<AppDesktopCapture>,
        facts: Option<AppDesktopFacts>,
    ) -> Result<(), HostAppError> {
        let qualified = facts.and_then(qualified_sample);
        let _transition = self.shutdown.transition_guard();
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("selected desktop observation", "current owner poisoned"))?;
        let Some(active) = current.as_ref().filter(|active| {
            self.shutdown.is_running()
                && active.identity.is_primary()
                && !active.reader_stop.load(Ordering::Acquire)
                && capture
                    .as_ref()
                    .is_none_or(|capture| capture.matches(active.attempt, &active.reader_stop))
        }) else {
            return Ok(());
        };
        let Some(windows) = self.windows.as_ref() else {
            return Ok(());
        };
        let mut session = windows
            .lock()
            .map_err(|_| app_detail("selected desktop observation", "Registry lock poisoned"))?;
        let owner = WindowOwner::new(active.identity, active.attempt);
        let subscribed = session.subscribed == Some(owner);
        let Some(desktop) = session
            .desktop
            .as_mut()
            .filter(|desktop| desktop.owner == Some(owner))
        else {
            return Ok(());
        };
        if capture.is_none() && desktop.initial_closed {
            return Ok(());
        }
        desktop.initial_closed = true;
        if desktop.sample.as_ref().is_some_and(|previous| {
            !subscribed
                || matches!(
                    previous.event.as_ref(),
                    WindowEvent::DesktopSnapshot { snapshot, .. } if snapshot == &qualified
                )
        }) {
            return Ok(());
        }
        desktop.observation(qualified, Instant::now())
    }

    pub(in crate::app_session) fn mark_selected_desktop_initialized(
        &self,
        loop_entry: Instant,
        observed_at: Instant,
    ) -> Result<(), HostAppError> {
        let _transition = self.shutdown.transition_guard();
        let Some(windows) = self.windows.as_ref() else {
            return Ok(());
        };
        let mut session = windows
            .lock()
            .map_err(|_| app_detail("selected desktop Init", "Registry lock poisoned"))?;
        if let Some(desktop) = session.desktop.as_mut() {
            if desktop
                .pending
                .as_ref()
                .is_some_and(|pending| observed_at >= pending.clock.init_deadline(loop_entry))
            {
                return Err(app_detail(
                    "selected desktop Init",
                    "the native UI loop did not complete Init before its deadline",
                ));
            }
            desktop.init_seen = true;
            desktop.initial_closed = true;
            if desktop.sample.is_none() {
                desktop.observation(None, Instant::now())?;
            }
        }
        Ok(())
    }

    /// Runs on the existing Core coordinator, including before native entry.
    pub(in crate::app_session) fn selected_desktop_startup_deadline(
        &self,
        loop_entry: Option<Instant>,
        now: Instant,
    ) -> Result<(), HostAppError> {
        let _transition = self.shutdown.transition_guard();
        let Some(windows) = self.windows.as_ref() else {
            return Ok(());
        };
        let session = windows
            .lock()
            .map_err(|_| app_detail("desktop startup deadline", "Registry lock poisoned"))?;
        let Some(desktop) = session.desktop.as_ref() else {
            return Ok(());
        };
        let Some(pending) = desktop.pending.as_ref().filter(|pending| {
            !pending.target.caller().reader_stop.load(Ordering::Acquire)
                && self.shutdown.is_running()
        }) else {
            return Ok(());
        };
        if pending.clock.ui_entry_expired(loop_entry, now) {
            return Err(app_detail(
                "selected desktop UI entry",
                "the native UI loop did not enter within 10 s of Subscribe",
            ));
        }
        if desktop.init_seen && now >= pending.clock.caller_deadline() {
            return Err(app_ipc("desktop Subscribe deadline", &IpcError::Timeout));
        }
        Ok(())
    }

    fn desktop_source(&self) -> Result<Option<DesktopSource>, HostAppError> {
        let Some(target) = self.current_primary_writer_target()? else {
            return Ok(None);
        };
        let mut source = None;
        with_primary_caller_locked(&self.current, &self.shutdown, target.caller(), |_| {
            let Some(windows) = self.windows.as_ref() else {
                return Ok(());
            };
            let session = windows
                .lock()
                .map_err(|_| app_detail("desktop settlement", "Registry lock poisoned"))?;
            let Some(desktop) = session.desktop.as_ref().filter(|desktop| desktop.init_seen) else {
                return Ok(());
            };
            let Some(sample) = desktop.sample.as_ref().filter(|sample| !sample.published) else {
                return Ok(());
            };
            let owner = WindowOwner::new(target.caller().identity, target.caller().attempt);
            if let Some(pending) = desktop
                .pending
                .as_ref()
                .filter(|pending| Arc::ptr_eq(&pending.target, &target))
            {
                source = Some(DesktopSource {
                    target: Arc::clone(&target),
                    event: Arc::clone(&sample.event),
                    correlation: Some(pending.correlation),
                    deadline: pending.clock.settlement_deadline(sample.validated_at),
                    caller_deadline: Some(pending.clock.caller_deadline()),
                });
            } else if session.subscribed == Some(owner) {
                source = Some(DesktopSource {
                    target: Arc::clone(&target),
                    event: Arc::clone(&sample.event),
                    correlation: None,
                    deadline: Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE,
                    caller_deadline: None,
                });
            }
            Ok(())
        })?;
        Ok(source)
    }

    pub(in crate::app_session) fn service_selected_desktop(
        &self,
    ) -> Result<DesktopStartup, HostAppError> {
        let Some(source) = self.desktop_source()? else {
            return Ok(DesktopStartup::Waiting);
        };
        let caller = source.target.caller();
        let turn = self.claim_window_publication(source.deadline, Some(caller));
        let result: PublicationResult<bool> = (|| {
            let Some(turn) = turn
                .as_ref()
                .map_err(|error| app_detail("desktop settlement", error.to_string()))?
            else {
                return Ok(false);
            };
            let emitter = WindowEmitter {
                router: self,
                turn,
                target: &source.target,
                deadline: source.deadline,
            };
            emitter.settle_desktop(&source)
        })();
        drop(turn);
        match result {
            Ok(true) => Ok(source
                .caller_deadline
                .map_or(DesktopStartup::Waiting, |deadline| DesktopStartup::Ready {
                    target: source.target,
                    deadline,
                })),
            Ok(_) => Ok(DesktopStartup::Waiting),
            Err(error) => match self.finish_native_publication(
                caller,
                |session| source.matches(session),
                Err(error),
            )? {
                NativePublicationDisposition::Handled => Ok(DesktopStartup::Waiting),
                NativePublicationDisposition::FatalQueued(error) => {
                    Ok(DesktopStartup::FatalQueued(error))
                }
            },
        }
    }
}

impl DesktopSource {
    fn matches(&self, session: &WindowSession) -> bool {
        session.desktop.as_ref().is_some_and(|desktop| {
            desktop
                .sample
                .as_ref()
                .is_some_and(|sample| Arc::ptr_eq(&sample.event, &self.event))
                && self
                    .correlation
                    .is_none_or(|correlation| desktop.matches_pending(&self.target, correlation))
        })
    }
}

impl WindowEmitter<'_, '_> {
    fn settle_desktop(&self, source: &DesktopSource) -> PublicationResult<bool> {
        let phase = "desktop Subscribe settlement";
        if Instant::now() >= self.deadline {
            return Err(app_ipc(phase, &IpcError::Timeout).into());
        }
        let missing = matches!(
            &*source.event,
            WindowEvent::DesktopSnapshot { snapshot: None, .. }
        );
        if let Some(correlation) = source.correlation
            && missing
        {
            let error = unavailable();
            if self.desktop_error(correlation, error.code(), &error.to_string(), |session| {
                source.matches(session)
            })? {
                return Err(error.into());
            }
            return Ok(false);
        }
        let frame = self
            .router
            .reserve_primary_value(self.caller(), self.deadline, phase)?
            .encode_value(
                Arc::clone(self.target),
                FrameKind::Event,
                keld_ipc::channel_table::WINDOW.id(),
                CorrelationId(0),
                &*source.event,
                keld_ipc::window::DESKTOP_EVENT_STORAGE_BYTES,
            )?;
        if !self.admit_by_deadline(frame, |session| source.matches(session), phase)? {
            return Ok(false);
        }
        if let Some(correlation) = source.correlation
            && !self.replay(correlation, true)?
        {
            return Ok(false);
        }
        Ok(self
            .with_registry(phase, |session, _| {
                let Some(desktop) = session.desktop.as_mut() else {
                    return Ok(false);
                };
                if let Some(sample) = desktop
                    .sample
                    .as_mut()
                    .filter(|sample| Arc::ptr_eq(&sample.event, &source.event))
                {
                    sample.published = true;
                }
                if source
                    .correlation
                    .is_some_and(|correlation| desktop.matches_pending(self.target, correlation))
                {
                    desktop.pending = None;
                }
                Ok(true)
            })?
            .unwrap_or(false))
    }

    pub(super) fn desktop_error(
        &self,
        correlation: CorrelationId,
        code: &str,
        message: &str,
        check: impl FnOnce(&WindowSession) -> bool,
    ) -> PublicationResult<bool> {
        let phase = "desktop Subscribe refusal";
        let frame = self
            .router
            .reserve_primary_value(self.caller(), self.deadline, phase)?
            .encode_error_value(
                Arc::clone(self.target),
                LIFECYCLE_CHANNEL,
                correlation,
                &CallError {
                    code: code.to_owned(),
                    message: message.to_owned(),
                },
            )?;
        self.admit_by_deadline(frame, check, phase)
    }
}
