//! GH542 single checked native document/attempt owner.
//!
//! This is a *state* authority, not an IPC, source, URL or file grant. One
//! `NativeDocumentOwner` belongs to one real native `WebView` lifetime. The platform native
//! navigation adapter supplies the actual main-frame commit evidence; Bun,
//! page-supplied URLs and bridge credentials cannot call these transitions.

use std::num::{NonZeroU32, NonZeroU64};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::WebviewId;

/// Opaque digest of a source already admitted by the frozen graph/Host owner.
/// A digest on its own grants no filesystem, IPC or navigation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedSourceId(pub(crate) [u8; 32]);

/// One owner-issued expected host/native load. It is a correlation, not a
/// second navigation counter or a renderer-selected document principal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeAttempt {
    view: WebviewId,
    serial: NonZeroU64,
}

impl NativeAttempt {
    pub(crate) const fn view(self) -> WebviewId {
        self.view
    }
}

/// Identity observed on the native main-frame navigation callback. The adapter
/// corroborates it against the Host load before publishing a commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeCommitIdentity(pub(crate) NonZeroU64);

/// The only checked generation minted by a real, qualified native main-frame
/// commit. KEL142, GH484 and GH462 consume this immutable observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeCommittedStamp {
    pub(crate) view: WebviewId,
    pub(crate) generation: NonZeroU32,
    pub(crate) attempt: NativeAttempt,
    pub(crate) source: VerifiedSourceId,
    pub(crate) native_commit: NativeCommitIdentity,
}

/// Resource access may precede native commit but it cannot mint an IPC grant.
/// The same A lease remains valid across its own successful commit; B revokes A.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResourceLease {
    pub(crate) attempt: NativeAttempt,
    pub(crate) source: VerifiedSourceId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentFailure {
    Destroyed,
    Exhausted,
    WrongView,
    StaleAttempt,
    SourceMismatch,
    NativeMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Unbound {
        generation: u32,
    },
    AwaitingCommit {
        generation: NonZeroU32,
        attempt: NativeAttempt,
        source: VerifiedSourceId,
    },
    Live {
        stamp: NativeCommittedStamp,
    },
    FailedClosed {
        generation: u32,
    },
    Exhausted,
    Destroyed,
}

/// Monotonic loss of the selected physical Windows renderer. Already exported document
/// leases observe the same fence before queued Core or IPC publication.
#[cfg(windows)]
#[derive(Clone, Debug, Default)]
pub(crate) struct RendererLifetime(Arc<AtomicBool>);

#[cfg(windows)]
impl RendererLifetime {
    pub(crate) fn observe_exit(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub(crate) fn has_exited(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Read-only revocation witness minted only by the sole native document owner.
/// It does not authorize any operation after its Acquire observation.
#[derive(Clone, Debug)]
pub(crate) struct NativeDocumentLease {
    stamp: NativeCommittedStamp,
    live: Arc<AtomicBool>,
    #[cfg(windows)]
    renderer: Option<RendererLifetime>,
    source: Option<std::sync::Weak<crate::resource_source::AdmittedWebAppSource>>,
}
impl NativeDocumentLease {
    pub(crate) fn is_current(&self) -> bool {
        #[cfg(windows)]
        if self
            .renderer
            .as_ref()
            .is_some_and(RendererLifetime::has_exited)
        {
            return false;
        }
        self.live.load(Ordering::Acquire)
    }
    pub(crate) const fn stamp(&self) -> NativeCommittedStamp {
        self.stamp
    }
    pub(crate) fn belongs_to_source(
        &self,
        source: &Arc<crate::resource_source::AdmittedWebAppSource>,
    ) -> bool {
        self.source
            .as_ref()
            .is_some_and(|bound| bound.ptr_eq(&Arc::downgrade(source)))
    }
    pub(crate) fn same_binding(&self, other: &Self) -> bool {
        self.stamp == other.stamp && Arc::ptr_eq(&self.live, &other.live)
    }
}

/// Single UI-thread state machine. No timeout, role restart or page-provided
/// value changes its checked native document generation.
#[derive(Debug)]
pub(crate) struct NativeDocumentOwner {
    view: WebviewId,
    next_attempt: u64,
    phase: Phase,
    live_lease: Option<Arc<AtomicBool>>,
    #[cfg(windows)]
    renderer: Option<RendererLifetime>,
    source: Option<std::sync::Weak<crate::resource_source::AdmittedWebAppSource>>,
}

impl NativeDocumentOwner {
    pub(crate) const fn view(&self) -> WebviewId {
        self.view
    }

    pub(crate) fn lease_for(&self, expected: NativeCommittedStamp) -> Option<NativeDocumentLease> {
        #[cfg(windows)]
        if self.renderer_exited() {
            return None;
        }
        if self.current_stamp() != Some(expected) {
            return None;
        }
        let live = self.live_lease.as_ref()?;
        live.load(Ordering::Acquire).then(|| NativeDocumentLease {
            stamp: expected,
            live: Arc::clone(live),
            #[cfg(windows)]
            renderer: self.renderer.clone(),
            source: self.source.clone(),
        })
    }
    /// Bind the actual same-build source identity before any native load.
    pub(crate) fn bind_source(
        &mut self,
        source: &Arc<crate::resource_source::AdmittedWebAppSource>,
    ) -> Result<(), DocumentFailure> {
        if let Some(bound) = &self.source {
            return if bound.ptr_eq(&Arc::downgrade(source)) {
                Ok(())
            } else {
                Err(DocumentFailure::SourceMismatch)
            };
        }
        if !matches!(self.phase, Phase::Unbound { generation: 0 }) {
            return Err(DocumentFailure::SourceMismatch);
        }
        self.source = Some(Arc::downgrade(source));
        Ok(())
    }
    pub(crate) fn source_binding(
        &self,
    ) -> Option<Arc<crate::resource_source::AdmittedWebAppSource>> {
        self.source.as_ref()?.upgrade()
    }
    fn revoke_live_lease(&mut self) {
        if let Some(lease) = self.live_lease.take() {
            lease.store(false, Ordering::Release);
        }
    }

    #[cfg(windows)]
    pub(crate) fn for_windows_renderer(
        view: WebviewId,
        renderer: Option<RendererLifetime>,
    ) -> Self {
        let mut owner = Self::new(view);
        owner.renderer = renderer;
        owner
    }

    #[cfg(windows)]
    pub(crate) fn observes_renderer(&self) -> bool {
        self.renderer.is_some()
    }

    #[cfg(windows)]
    pub(crate) fn renderer_exited(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(RendererLifetime::has_exited)
    }

    pub(crate) const fn new(view: WebviewId) -> Self {
        Self {
            view,
            next_attempt: 0,
            phase: Phase::Unbound { generation: 0 },
            live_lease: None,
            #[cfg(windows)]
            renderer: None,
            source: None,
        }
    }

    /// Snapshot is usable by named IPC and eval only in the qualified Live
    /// phase. The caller still has to enforce final exact-channel Guard,
    /// current authenticated Bun role and native world/frame provenance.
    pub(crate) const fn current_stamp(&self) -> Option<NativeCommittedStamp> {
        match self.phase {
            Phase::Live { stamp } => Some(stamp),
            _ => None,
        }
    }

    /// GH473 may read static source before this attempt commits, or while
    /// the successfully committed same-document subresources finish loading.
    /// It has no right to mint an IPC principal from this lease.
    pub(crate) fn active_resource_lease(&self, source: VerifiedSourceId) -> Option<ResourceLease> {
        match self.phase {
            Phase::AwaitingCommit {
                attempt,
                source: expected,
                ..
            } if expected == source => Some(ResourceLease { attempt, source }),
            Phase::Live { stamp } if stamp.source == source => Some(ResourceLease {
                attempt: stamp.attempt,
                source,
            }),
            _ => None,
        }
    }

    pub(crate) const fn generation(&self) -> Option<u32> {
        match self.phase {
            Phase::Unbound { generation } | Phase::FailedClosed { generation } => Some(generation),
            Phase::AwaitingCommit { generation, .. } => Some(generation.get()),
            Phase::Live { stamp } => Some(stamp.generation.get()),
            Phase::Exhausted | Phase::Destroyed => None,
        }
    }

    /// Host-qualified full-document load: revoke the old commit BEFORE native
    /// engine loading begins. Checked increment belongs only to this owner.
    pub(crate) fn begin_host_load(
        &mut self,
        source: VerifiedSourceId,
    ) -> Result<NativeAttempt, DocumentFailure> {
        let generation = match self.phase {
            Phase::Destroyed => return Err(DocumentFailure::Destroyed),
            Phase::Exhausted => return Err(DocumentFailure::Exhausted),
            Phase::Unbound { generation } | Phase::FailedClosed { generation } => generation,
            Phase::AwaitingCommit { generation, .. } => generation.get(),
            Phase::Live { stamp } => stamp.generation.get(),
        };
        self.revoke_live_lease();
        let Some(next_gen) = generation.checked_add(1).and_then(NonZeroU32::new) else {
            self.phase = Phase::Exhausted;
            return Err(DocumentFailure::Exhausted);
        };
        let Some(serial) = self.next_attempt.checked_add(1).and_then(NonZeroU64::new) else {
            self.phase = Phase::Exhausted;
            return Err(DocumentFailure::Exhausted);
        };
        self.next_attempt = serial.get();
        let attempt = NativeAttempt {
            view: self.view,
            serial,
        };
        self.phase = Phase::AwaitingCommit {
            generation: next_gen,
            attempt,
            source,
        };
        Ok(attempt)
    }

    /// Initial document's source may be fetched during `PendingLoad`, and a
    /// committed same-document child may finish later. This lease is *never*
    /// accepted as a bridge credential or final Guard principal.
    pub(crate) fn resource_lease(
        &self,
        attempt: NativeAttempt,
        source: VerifiedSourceId,
    ) -> Option<ResourceLease> {
        if attempt.view.0 != self.view.0 {
            return None;
        }
        match self.phase {
            Phase::AwaitingCommit {
                attempt: expected,
                source: bound,
                ..
            } if expected.serial.get() == attempt.serial.get() && bound.0 == source.0 => {
                Some(ResourceLease { attempt, source })
            }
            Phase::Live { stamp }
                if stamp.attempt.serial.get() == attempt.serial.get()
                    && stamp.source.0 == source.0 =>
            {
                Some(ResourceLease { attempt, source })
            }
            _ => None,
        }
    }

    /// Recheck on the actual UI owner immediately before one native resource
    /// response. A may remain in flight after A commits but cannot survive B.
    pub(crate) fn is_current_lease(&self, lease: ResourceLease) -> bool {
        self.resource_lease(lease.attempt, lease.source).is_some()
    }

    /// Consume a qualified main-frame commit. The Windows adapter must first
    /// correlate the actual navigation ID and verified top-level response with
    /// this Host attempt. URL equality is insufficient.
    pub(crate) fn commit_admitted(
        &mut self,
        attempt: NativeAttempt,
        source: VerifiedSourceId,
        native_commit: NativeCommitIdentity,
    ) -> Result<NativeCommittedStamp, DocumentFailure> {
        if attempt.view != self.view {
            return Err(DocumentFailure::WrongView);
        }
        match self.phase {
            Phase::AwaitingCommit {
                generation,
                attempt: active,
                source: bound,
            } => {
                if active != attempt {
                    return Err(DocumentFailure::StaleAttempt);
                }
                if bound != source {
                    self.phase = Phase::FailedClosed {
                        generation: generation.get(),
                    };
                    return Err(DocumentFailure::SourceMismatch);
                }
                // GH542: a trusted Host full-document call advances once
                // before engine load, then its actual matching new-document
                // commit advances once again. No wrapping, no rollback.
                let Some(committed_generation) =
                    generation.get().checked_add(1).and_then(NonZeroU32::new)
                else {
                    self.phase = Phase::Exhausted;
                    return Err(DocumentFailure::Exhausted);
                };
                let stamp = NativeCommittedStamp {
                    view: self.view,
                    generation: committed_generation,
                    attempt,
                    source,
                    native_commit,
                };
                self.live_lease = Some(Arc::new(AtomicBool::new(true)));
                self.phase = Phase::Live { stamp };
                Ok(stamp)
            }
            Phase::Live { stamp } if stamp.attempt == attempt => {
                if stamp.source == source && stamp.native_commit == native_commit {
                    Ok(stamp) // idempotent duplicate callback only
                } else {
                    // A different commit, even with identical final URL, never
                    // reuses the old stamp or credentials.
                    self.unexpected_mainframe_commit();
                    Err(DocumentFailure::NativeMismatch)
                }
            }
            Phase::Destroyed => Err(DocumentFailure::Destroyed),
            Phase::Exhausted => Err(DocumentFailure::Exhausted),
            _ => Err(DocumentFailure::StaleAttempt),
        }
    }

    /// Only the callback's exact active attempt can fail this generation.
    /// Late A failure/cancel after trusted B begins/commits is an old-task
    /// cleanup operation; it cannot free or revoke B.
    pub(crate) fn fail_attempt(&mut self, attempt: NativeAttempt) -> bool {
        let generation = match self.phase {
            Phase::AwaitingCommit {
                generation,
                attempt: current,
                ..
            } if current == attempt => Some(generation),
            Phase::Live { stamp } if stamp.attempt == attempt => Some(stamp.generation),
            _ => None,
        };
        if let Some(generation) = generation {
            self.revoke_live_lease();
            self.phase = Phase::FailedClosed {
                generation: generation.get(),
            };
            true
        } else {
            false
        }
    }

    /// A separately *observed* uncorrelated actual main-frame replacement
    /// (not a stale A callback) closes the old document. It may not qualify
    /// its own new source merely by matching URL text.
    pub(crate) fn unexpected_mainframe_commit(&mut self) {
        self.revoke_live_lease();
        match self.phase {
            Phase::Destroyed | Phase::Exhausted => (),
            _ => {
                if self.generation().is_some_and(|n| n == u32::MAX) {
                    self.phase = Phase::Exhausted;
                } else {
                    self.phase = Phase::FailedClosed {
                        generation: self.generation().unwrap_or(0) + 1,
                    };
                }
            }
        }
    }

    /// Destroy the view's checked document and revoke every outstanding lease.
    pub(crate) fn destroy(&mut self) {
        self.revoke_live_lease();
        self.phase = Phase::Destroyed;
    }
}

impl Drop for NativeDocumentOwner {
    fn drop(&mut self) {
        self.revoke_live_lease();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod admission_lease_tests;
