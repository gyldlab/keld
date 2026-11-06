//! GH1188 D6/D12: pending native teardown and deferred builds on the UI thread.
//!
//! Core remains the only identity, generation, admission and close-policy owner.
//! This owner retains an owed answer until native teardown is observed. It never
//! creates or drops a `View`/controller/`Window` and never latches a lifecycle fault.
//! The loop must park BEFORE dropping its existing native custodian; on a park
//! collision it still owns that custodian and must take the existing Fatal path.
//!
//! Loop order is part of D6: poll the barrier; publish already-staged releases;
//! expire entries into the barrier's existing first-fault cell; handle terminal
//! reactions/drain deferred requests; test the exit gate; dispatch this event;
//! then post an admitted deferred head and schedule the earliest returned deadline.
//! In particular, `stage_destroyed` is never followed by publication in that same
//! callback. Native `IsWindow` proof is still required on the subsequent callback.
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::{BTreeMap, VecDeque, btree_map::Entry};
use std::time::{Duration, Instant};

use tao::window::WindowId;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::IsWindow;

use super::{PROFILE_LAUNCH_DEADLINE, profile_failure_with};
use crate::engine::{AppWindowEvent, BuildStage};
use crate::profile::ProfileErrorKind;
use crate::{WebviewId, WvError};

pub(super) const RELEASING_DESTROYED_BOUND: Duration =
    PROFILE_LAUNCH_DEADLINE.saturating_add(Duration::from_secs(5));
pub(super) const STAGED_RELEASE_RECHECK: Duration = Duration::from_millis(10);

/// The answer already owed by the existing native command/build owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Releasing {
    Destroy(WebviewId),
    FailedBuild { build: u64, stage: BuildStage },
}

impl Releasing {
    fn event(self) -> AppWindowEvent {
        match self {
            Self::Destroy(view) => AppWindowEvent::WindowReleased { view },
            Self::FailedBuild { build, stage } => {
                AppWindowEvent::WindowBuildFailed { build, stage }
            }
        }
    }
}

/// Only the existing `CreateWindow` payload; no handle or new identity/policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DeferredCreate {
    pub(super) build: u64,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) x: Option<i32>,
    pub(super) y: Option<i32>,
}

impl DeferredCreate {
    pub(super) fn failure_event(self) -> AppWindowEvent {
        AppWindowEvent::WindowBuildFailed {
            build: self.build,
            stage: BuildStage::Webview,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub(super) enum CreateAdmission {
    Start(DeferredCreate),
    Queued,
    Refused(DeferredCreate),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub(super) enum StageResult {
    Unknown,
    AlreadyStaged,
    Staged { wake_posted: bool },
}

#[derive(Debug)]
struct ReleasingEntry {
    answer: Releasing,
    hwnd: isize,
    parked_at: Instant,
    staged: bool,
}

impl ReleasingEntry {
    fn expires_at(&self) -> Instant {
        self.parked_at + RELEASING_DESTROYED_BOUND
    }
}

/// Production uses Tao `WindowId`. The key parameter is only a pure-test seam:
/// integer keys let the same state transitions run without inventing Tao handles.
#[derive(Debug)]
pub(super) struct ManagedCustody<K = WindowId> {
    releasing: BTreeMap<K, ReleasingEntry>,
    deferred: VecDeque<DeferredCreate>,
    deferred_wake_outstanding: bool,
}

impl<K> Default for ManagedCustody<K> {
    fn default() -> Self {
        Self {
            releasing: BTreeMap::new(),
            deferred: VecDeque::new(),
            deferred_wake_outstanding: false,
        }
    }
}

impl<K: Ord + Copy> ManagedCustody<K> {
    /// Records custody before the caller drops its `View` or standalone `Window`.
    ///
    /// A collision replaces nothing and consumes no native custodian. Both
    /// owners remain the caller's responsibility; fail the session, never retry
    /// this key with another answer or silently lose the original parked entry.
    pub(super) fn park(
        &mut self,
        key: K,
        hwnd: isize,
        answer: Releasing,
        now: Instant,
    ) -> Result<(), WvError> {
        match self.releasing.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(ReleasingEntry {
                    answer,
                    hwnd,
                    parked_at: now,
                    staged: false,
                });
                Ok(())
            }
            Entry::Occupied(_) => Err(profile_failure_with(
                ProfileErrorKind::LifecycleUnproven,
                "a native window release key already has a pending owner",
            )),
        }
    }

    pub(super) fn contains(&self, key: K) -> bool {
        self.releasing.contains_key(&key)
    }

    #[cfg(any(test, all(feature = "profile-test-hooks", debug_assertions)))]
    pub(super) fn len(&self) -> usize {
        self.releasing.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.releasing.is_empty()
    }

    /// Read-only input for the loop's existing observation/fault hooks.
    #[cfg(any(test, all(feature = "profile-test-hooks", debug_assertions)))]
    pub(super) fn entry(&self, key: K) -> Option<(isize, Releasing)> {
        self.releasing
            .get(&key)
            .map(|entry| (entry.hwnd, entry.answer))
    }

    /// Stages once before attempting one `ProfileReleaseWake`. Failure is covered
    /// by `next_deadline`, with no re-post and no answer from `WM_DESTROY`'s stack.
    pub(super) fn stage_destroyed(&mut self, key: K, post: impl FnOnce() -> bool) -> StageResult {
        let Some(entry) = self.releasing.get_mut(&key) else {
            return StageResult::Unknown;
        };
        if entry.staged {
            return StageResult::AlreadyStaged;
        }
        entry.staged = true;
        StageResult::Staged {
            wake_posted: post(),
        }
    }

    /// Each expired entry is removed once. The caller records its HWND and maps
    /// a nonzero result to the existing barrier's `ReleasingExpired` first fault.
    /// Publication precedes this check, so a completed release wins at the bound.
    pub(super) fn expire(&mut self, now: Instant, mut report: impl FnMut(K, isize)) -> usize {
        let mut expired = 0;
        self.releasing.retain(|key, entry| {
            if now < entry.expires_at() {
                return true;
            }
            report(*key, entry.hwnd);
            expired += 1;
            false
        });
        expired
    }

    /// Remains applicable after normal browser credit or a barrier fault.
    pub(super) fn next_deadline(&self, now: Instant) -> Option<Instant> {
        self.releasing
            .values()
            .map(|entry| {
                let expiry = entry.expires_at();
                if entry.staged {
                    expiry.min(now + STAGED_RELEASE_RECHECK)
                } else {
                    expiry
                }
            })
            .min()
    }

    /// Terminal/fault handling drains deferred creates before calling this.
    /// A staged release is still pending, even if the browser barrier is ready.
    pub(super) fn exit_ready(&self, terminal: bool, barrier_ready: bool) -> bool {
        terminal && barrier_ready && self.is_empty()
    }

    /// ending is supplied by the loop from terminal intent/terminal/barrier fault;
    /// it is never copied into a second lifecycle state here.
    pub(super) fn admit(&mut self, create: DeferredCreate, ending: bool) -> CreateAdmission {
        if ending {
            return CreateAdmission::Refused(create);
        }
        if self.is_empty() && self.deferred.is_empty() {
            return CreateAdmission::Start(create);
        }
        // Core registry admission bounds this queue. Do not duplicate its limit.
        self.deferred.push_back(create);
        CreateAdmission::Queued
    }

    /// Called only at the bottom of a loop callback. Some(deadline) means posting
    /// failed: keep the mark clear and schedule a recheck even with no releases.
    pub(super) fn post_deferred_wake(
        &mut self,
        ending: bool,
        now: Instant,
        post: impl FnOnce() -> bool,
    ) -> Option<Instant> {
        if ending || !self.is_empty() || self.deferred.is_empty() || self.deferred_wake_outstanding
        {
            return None;
        }
        self.deferred_wake_outstanding = post();
        (!self.deferred_wake_outstanding).then_some(now + STAGED_RELEASE_RECHECK)
    }

    /// Only the dedicated `DeferredBuildWake` arm calls this. A `ProfileReleaseWake`
    /// never consumes a request. Clear its mark even if a buffered Destroy/terminal
    /// has since revoked admission; a later bottom-of-callback check may re-post.
    pub(super) fn take_deferred_wake(&mut self, ending: bool) -> Option<DeferredCreate> {
        let outstanding = std::mem::take(&mut self.deferred_wake_outstanding);
        if !outstanding || ending || !self.is_empty() {
            return None;
        }
        self.deferred.pop_front()
    }

    /// Terminal/fault reaction. Pop before sending, drain the whole FIFO even
    /// when the coordinator is gone, and propagate the first send failure.
    /// The caller's send closure records `BUILD_REFUSED` before its actual send.
    pub(super) fn refuse_deferred<E>(
        &mut self,
        mut send: impl FnMut(AppWindowEvent) -> Result<(), E>,
    ) -> Result<(), E> {
        self.deferred_wake_outstanding = false;
        let mut first_error = None;
        while let Some(create) = self.deferred.pop_front() {
            if let Err(error) = send(create.failure_event())
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// The injected observer is for pure unit tests. The native entry below is
    /// the production caller and supplies the real Win32 `IsWindow` read.
    fn publish_staged_with<E>(
        &mut self,
        mut is_window: impl FnMut(isize) -> bool,
        mut send: impl FnMut(isize, Releasing, AppWindowEvent) -> Result<(), E>,
    ) -> Result<usize, E> {
        let mut published = 0;
        let mut first_error = None;
        self.releasing.retain(|_, entry| {
            if first_error.is_some() || !entry.staged || is_window(entry.hwnd) {
                return true;
            }
            // Retain the entry through the send. It leaves after either result;
            // a failed send requires the loop's existing Fatal reaction and must
            // never cause another attempt of this same owed answer.
            match send(entry.hwnd, entry.answer, entry.answer.event()) {
                Ok(()) => published += 1,
                Err(error) => first_error = Some(error),
            }
            false
        });
        first_error.map_or(Ok(published), Err)
    }
}

impl ManagedCustody<WindowId> {
    /// Call at the top of a later loop callback, before expiry and exit checks.
    pub(super) fn publish_staged<E>(
        &mut self,
        send: impl FnMut(isize, Releasing, AppWindowEvent) -> Result<(), E>,
    ) -> Result<usize, E> {
        self.publish_staged_with(
            |hwnd| {
                // SAFETY: the UI owner captured this HWND before dropping its
                // own native custodian. This is a value-only query, never a
                // dereference or destroy. Handle reuse can only retain custody
                // until the bound; builds cannot run while any release is pending.
                // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindow
                unsafe { IsWindow(Some(HWND(hwnd as _))).as_bool() }
            },
            send,
        )
    }
}

#[cfg(test)]
#[path = "tests/managed_window_release.rs"]
mod release_tests;

#[cfg(test)]
#[path = "tests/managed_window_queue.rs"]
mod queue_tests;
