//! Retained Windows primary-generation families for the GH1188 release witness.
//!
//! Job policy and native calls stay in `windows_job`; this owner only serializes
//! enrollment with observation and bounds retained generation bookkeeping.

#![forbid(unsafe_code)] // Native assignment/query policy belongs to the existing Job owner.

use std::fmt;
use std::process::Child;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use super::{WindowsHostDeathJob, WindowsHostJobError, WindowsProcessJob};

/// Maximum concurrently retained nonempty generations, not a lifetime restart cap.
const MAX_RETAINED_PRIMARY_FAMILIES: usize = 64;

#[derive(Default)]
struct FamiliesState {
    jobs: Vec<WindowsProcessJob>,
    enrollment_pending: bool,
    sealed: bool,
}

/// Session-owned primary Jobs, including descendants of retired generations.
///
/// Clones share one bounded owner. The role supervisor reserves enrollment before
/// spawning, assigns its retained `Child` before publishing `Spawned`, and retains
/// the Job beyond that generation's authority lease. Only native-proved empty Jobs
/// are pruned. A descendant started before the post-spawn assignment is not claimed
/// as family; the host-death census must still observe it as non-family.
///
/// This is cleanup ownership, not containment of the primary's filesystem authority.
#[derive(Clone, Default)]
pub struct WindowsPrimaryFamilies {
    state: Arc<Mutex<FamiliesState>>,
}

impl fmt::Debug for WindowsPrimaryFamilies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowsPrimaryFamilies")
            .finish_non_exhaustive()
    }
}

impl WindowsPrimaryFamilies {
    /// Creates one empty family owner for a single application session.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seals generation admission and borrows every retained family for release.
    ///
    /// The host supplies its actual retained capability and an absolute deadline.
    /// Contention or an in-flight spawn refuses immediately, never reports a clean
    /// census or waits on a mutex beyond the release budget. Sealing is irreversible;
    /// a later application session must use a new family owner.
    ///
    /// # Errors
    ///
    /// Refuses incomplete/contended/poisoned enrollment, a failed native query, or
    /// expiration of the supplied deadline. No process is terminated by this call.
    pub fn observe_non_family_exit(
        &self,
        host: &WindowsHostDeathJob,
        deadline: Instant,
    ) -> Result<(), WindowsHostJobError> {
        let mut state = self.try_state("primary family release")?;
        state.sealed = true;
        if state.enrollment_pending {
            return Err(family_error(
                "primary family release",
                "generation enrollment is incomplete",
            ));
        }
        let families = state.jobs.iter().collect::<Vec<_>>();
        host.observe_non_family_exit(&families, deadline)
    }

    pub(crate) fn begin_generation(
        &self,
    ) -> Result<WindowsGenerationEnrollment, WindowsHostJobError> {
        let mut state = self.try_state("primary family enrollment")?;
        if state.sealed || state.enrollment_pending {
            return Err(family_error(
                "primary family enrollment",
                "session admission is sealed or another generation is pending",
            ));
        }
        // Query every retained Job before changing the inventory. A failed query
        // preserves custody; direct-child exit alone never proves family retirement.
        let alive = state
            .jobs
            .iter()
            .map(|job| job.active_processes().map(|n| n != 0))
            .collect::<Result<Vec<_>, _>>()?;
        let mut next = alive.into_iter();
        state.jobs.retain(|_| next.next().unwrap_or(true));
        if state.jobs.len() >= MAX_RETAINED_PRIMARY_FAMILIES {
            return Err(family_error(
                "primary family enrollment",
                "64 nonempty generation families are still retained; no new child was spawned",
            ));
        }
        let job = WindowsProcessJob::create()?;
        state.enrollment_pending = true;
        Ok(WindowsGenerationEnrollment {
            families: self.clone(),
            job: Some(job),
        })
    }

    fn try_state(
        &self,
        phase: &'static str,
    ) -> Result<MutexGuard<'_, FamiliesState>, WindowsHostJobError> {
        self.state.try_lock().map_err(|_| {
            family_error(
                phase,
                "family ownership is contended or poisoned; release is unproved",
            )
        })
    }
}

/// One pre-spawn reservation. Aborting it never invents family membership.
pub(crate) struct WindowsGenerationEnrollment {
    families: WindowsPrimaryFamilies,
    job: Option<WindowsProcessJob>,
}

impl WindowsGenerationEnrollment {
    pub(crate) fn bind_child(&mut self, child: &Child) -> Result<(), WindowsHostJobError> {
        let mut state = self.families.try_state("primary family binding")?;
        if state.sealed || !state.enrollment_pending {
            return Err(family_error(
                "primary family binding",
                "generation admission is no longer live",
            ));
        }
        let job = self.job.as_mut().ok_or_else(|| {
            family_error("primary family binding", "generation was already bound")
        })?;
        // Reuse the sole retained-handle assignment/read-back owner. This is before
        // authenticated admission, not before the child's first instruction.
        job.assign_child(child)?;
        let job = self.job.take().ok_or_else(|| {
            family_error("primary family binding", "the assigned Job owner is absent")
        })?;
        state.jobs.push(job);
        state.enrollment_pending = false;
        Ok(())
    }
}

impl Drop for WindowsGenerationEnrollment {
    fn drop(&mut self) {
        if self.job.is_some()
            && let Ok(mut state) = self.families.state.try_lock()
        {
            state.enrollment_pending = false;
        }
        // On contention/poison the pending bit remains set: availability may fail,
        // but an incomplete generation can never be silently excluded from release.
        // The uncommitted Job drops here; generic supervisor cleanup owns the Child.
    }
}

fn family_error(phase: &'static str, detail: &'static str) -> WindowsHostJobError {
    WindowsHostJobError::contract(phase, detail)
}
