//! Pure source preparation and exact-reader observation. No native entry here.
use super::super::super::{DesktopEpoch, desktop_rect};
use super::super::{
    Arc, HostAppError, Instant, PrimaryRouterHandle, WindowEvent, WindowSession, WriterLink,
    app_detail, with_primary_caller_locked,
};
use keld_ipc::window::{DesktopFact, DesktopFacts};
use std::sync::atomic::Ordering;

/// Same source-stack metadata; the actual writer/link remains in its existing owner.
pub(super) struct DesktopSource {
    pub(super) target: Arc<WriterLink>,
    pub(super) deadline: Instant,
    capture: Option<keld_wv::AppDesktopCapture>,
}

pub(super) struct DesktopBefore {
    seen: bool,
    epoch: u64,
    latest: Option<Arc<WindowEvent>>,
}

impl DesktopBefore {
    pub(super) fn matches(&self, session: &WindowSession) -> bool {
        self.seen == session.desktop_seen
            && self.epoch == session.desktop_epoch.0
            && match (&self.latest, &session.desktop_latest) {
                (None, None) => true,
                (Some(before), Some(current)) => Arc::ptr_eq(before, current),
                _ => false,
            }
    }
}

impl DesktopSource {
    pub(super) fn capture(
        router: &PrimaryRouterHandle,
        capture: Option<keld_wv::AppDesktopCapture>,
        deadline: Instant,
    ) -> Result<Option<Self>, HostAppError> {
        let _transition = router.shutdown.transition_guard();
        if !router.shutdown.is_running() {
            return Ok(None);
        }
        let current = router
            .current
            .lock()
            .map_err(|_| app_detail("desktop source capture", "generation lock poisoned"))?;
        let Some(active) = current.as_ref() else {
            return Ok(None);
        };
        if !active.identity.is_primary()
            || active.reader_stop.load(Ordering::Acquire)
            || capture
                .as_ref()
                .is_some_and(|capture| !capture.matches(active.attempt, &active.reader_stop))
        {
            return Ok(None);
        }
        let Some(windows) = router.windows.as_ref() else {
            return Err(app_detail("desktop source capture", "Registry absent"));
        };
        let session = windows
            .lock()
            .map_err(|_| app_detail("desktop source capture", "Registry lock poisoned"))?;
        if !session.desktop_required {
            return Err(app_detail(
                "desktop source capture",
                "desktop capability unselected",
            ));
        }
        // Retirement closes the sole unscoped boot admission. A delayed initial
        // DTO cannot be relabelled with a replacement reader's identity.
        if capture.is_none() && session.initial_desktop_closed {
            return Ok(None);
        }
        let caller = crate::app_session::PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        let target = active
            .writer_link
            .as_ref()
            .filter(|target| target.matches(&caller))
            .ok_or_else(|| {
                app_detail(
                    "desktop source capture",
                    "exact owned Windows writer absent",
                )
            })?;
        let deadline = session
            .pending_desktop
            .as_ref()
            .filter(|pending| Arc::ptr_eq(&pending.target, target))
            .map_or(deadline, |pending| deadline.min(pending.deadline));
        Ok(Some(Self {
            target: Arc::clone(target),
            deadline,
            capture,
        }))
    }

    pub(super) fn initial(&self) -> bool {
        self.capture.is_none()
    }

    pub(super) fn matches(&self, session: &WindowSession) -> bool {
        session.desktop_required
            && self
                .capture
                .as_ref()
                .map_or(!session.initial_desktop_closed, |capture| {
                    capture.matches(
                        self.target.caller().attempt,
                        &self.target.caller().reader_stop,
                    )
                })
    }

    pub(super) fn before(
        &self,
        router: &PrimaryRouterHandle,
        turn: &super::super::WindowPublicationTurn<'_>,
    ) -> Result<Option<DesktopBefore>, HostAppError> {
        let mut before = None;
        with_primary_caller_locked(
            &router.current,
            &router.shutdown,
            self.target.caller(),
            |_| {
                let windows = router.windows.as_ref().ok_or_else(|| {
                    app_detail("desktop source observation", "Registry disappeared")
                })?;
                let session = windows.lock().map_err(|_| {
                    app_detail("desktop source observation", "Registry lock poisoned")
                })?;
                if !turn.owns(windows, &session) {
                    return Err(app_detail(
                        "desktop source observation",
                        "foreign source turn",
                    ));
                }
                if self.matches(&session) {
                    before = Some(DesktopBefore {
                        seen: session.desktop_seen,
                        epoch: session.desktop_epoch.0,
                        latest: session.desktop_latest.as_ref().map(Arc::clone),
                    });
                }
                Ok(())
            },
        )?;
        Ok(before)
    }
}

/// This is the existing bounded wire-shape check, not native geometry proof.
/// The approved Windows sampler independently qualifies its single primary.
fn qualified_desktop(facts: keld_wv::AppDesktopFacts) -> Option<DesktopFacts> {
    if facts.primary_id == 0
        || facts.displays.is_empty()
        || facts.displays.len() > 32
        || facts.displays.capacity() > 32
    {
        return None;
    }
    let mut displays: Vec<DesktopFact> = Vec::with_capacity(facts.displays.len());
    if displays.capacity() > 32 {
        return None;
    }
    let mut primary_count = 0;
    for display in facts.displays {
        if display.id == 0
            || displays.iter().any(|old| old.id == display.id)
            || display.bounds.width == 0
            || display.bounds.height == 0
            || display.work_area.width == 0
            || display.work_area.height == 0
            || !display.scale_factor.is_finite()
            || display.scale_factor <= 0.0
        {
            return None;
        }
        if display.id == facts.primary_id {
            primary_count += 1;
        }
        displays.push(DesktopFact {
            id: display.id,
            bounds: desktop_rect(display.bounds),
            work_area: desktop_rect(display.work_area),
            scale_factor: display.scale_factor,
        });
    }
    (primary_count == 1).then_some(DesktopFacts {
        primary_id: facts.primary_id,
        displays,
        should_use_dark_colors: facts.should_use_dark_colors,
    })
}

pub(super) fn prepare_snapshot(
    facts: Option<keld_wv::AppDesktopFacts>,
    before: &DesktopBefore,
) -> Result<Option<Arc<WindowEvent>>, HostAppError> {
    let snapshot = facts.and_then(qualified_desktop);
    if before.seen
        && before.latest.as_ref().is_some_and(|latest| {
            matches!(latest.as_ref(), WindowEvent::DesktopSnapshot {
                snapshot: old, ..
            } if old == &snapshot)
        })
    {
        return Ok(None);
    }
    let epoch = DesktopEpoch(before.epoch).advance()?;
    let event = WindowEvent::DesktopSnapshot { epoch, snapshot };
    // Count without encoding/allocation. The sole physical writer allocates the
    // encoded body only after its VALUE position has been reserved.
    let bytes = keld_ipc::codec::encoded_len(&event)
        .map_err(|error| crate::app_session::app_ipc("desktop snapshot count", &error))?;
    super::super::super::admit_desktop_payload_bytes(bytes)?;
    if bytes > keld_ipc::window::DESKTOP_EVENT_STORAGE_BYTES {
        return Err(app_detail(
            "desktop snapshot count",
            "qualified fixed shape exceeded storage",
        ));
    }
    Ok(Some(Arc::new(event)))
}
