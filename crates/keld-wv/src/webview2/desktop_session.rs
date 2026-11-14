//! Native desktop observations for the selected Windows facade only. The UI
//! thread owns the sampler; Core remains the sole writer of public desktop state.
use super::desktop_sample::DesktopSampler;
use crate::{AppDesktopCapture, AppDesktopFacts, AppWindowEvent, WvError};
use std::sync::mpsc::Sender;
use std::time::Instant;

/// Production always uses its actual `DesktopSampler`. Only pure source-bound
/// controls substitute OS acquisition; the producer orchestration is unchanged.
pub(super) trait Capture {
    fn capture(&mut self) -> Result<AppDesktopFacts, WvError>;
}

impl Capture for DesktopSampler {
    fn capture(&mut self) -> Result<AppDesktopFacts, WvError> {
        DesktopSampler::capture(self)
    }
}

#[derive(Default)]
pub(super) struct DesktopSession<S = DesktopSampler> {
    sampler: S,
    initialized: bool,
    producer: Option<AppDesktopCapture>,
}

impl<S: Capture> DesktopSession<S> {
    fn sample(&mut self) -> Option<AppDesktopFacts> {
        self.sampler
            .capture()
            .map_err(|error| {
                eprintln!("KELD_WV_DESKTOP_UNAVAILABLE {error}");
            })
            .ok()
    }

    /// Called only after the genuine first Init predicate has succeeded. If the
    /// generation was already bound, the same observation also belongs to it.
    pub(super) fn first_init(
        &mut self,
        events: &Sender<AppWindowEvent>,
        ending: impl Fn() -> bool,
    ) -> bool {
        if ending() {
            return true;
        }
        let capture = self.producer.as_ref().filter(|p| p.is_live()).cloned();
        let facts = self.sample();
        // Capture consumes the existing Init budget. Queueing the observation
        // must not move the timestamp in either direction.
        let observed_at = Instant::now();
        if ending() {
            return true;
        }
        let refresh = capture.map(|capture| AppWindowEvent::WindowsDesktopObserved {
            capture,
            facts: facts.clone(),
        });
        if events
            .send(AppWindowEvent::DesktopObserved { facts })
            .is_err()
            || !super::report_host_initialized(events, observed_at)
        {
            return false;
        }
        self.initialized = true;
        refresh.is_none_or(|event| events.send(event).is_ok())
    }

    pub(super) fn bind(
        &mut self,
        capture: AppDesktopCapture,
        events: &Sender<AppWindowEvent>,
    ) -> Result<(), WvError> {
        if capture.is_live() {
            self.producer = Some(capture);
            self.refresh(events)?;
        }
        Ok(())
    }

    pub(super) fn refresh(&mut self, events: &Sender<AppWindowEvent>) -> Result<(), WvError> {
        if !self.initialized {
            return Ok(());
        }
        let Some(capture) = self.producer.as_ref().filter(|p| p.is_live()).cloned() else {
            return Ok(());
        };
        let facts = self.sample();
        events
            .send(AppWindowEvent::WindowsDesktopObserved { capture, facts })
            .map_err(|_| WvError::EventLoop("desktop observation receiver closed".to_owned()))
    }
}

#[cfg(test)]
mod tests;
