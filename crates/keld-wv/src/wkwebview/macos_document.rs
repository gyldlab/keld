//! GH542 single checked native document/attempt owner.
//!
//! This is a *state* authority, not an IPC, source, URL or file grant. One
//! `NativeDocumentOwner` belongs to one real `WKWebView` lifetime. The macOS native
//! navigation delegate supplies the actual main-frame commit evidence; Bun,
//! page-supplied URLs and bridge credentials cannot call these transitions.

use std::num::{NonZeroU32, NonZeroU64};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use objc2::Message;

use crate::WebviewId;

/// Opaque digest of a source already admitted by the frozen graph/Host owner.
/// A digest on its own grants no filesystem, IPC or navigation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VerifiedSourceId(pub(super) [u8; 32]);

/// One owner-issued expected host/native load. It is a correlation, not a
/// second navigation counter or a renderer-selected document principal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NativeAttempt {
    view: WebviewId,
    serial: NonZeroU64,
}

impl NativeAttempt {
    pub(super) const fn view(self) -> WebviewId {
        self.view
    }
}

/// Identity of the actual native `WKNavigation` returned by Host-owned
/// `WKWebView`.loadRequest. Never derive it from a page URL/JS value/message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NativeCommitIdentity(NonZeroU64);

/// The view keeps the actual Objective-C `WKNavigation` object retained from the
/// synchronous UI-owned loadRequest result through finish/fail. The state
/// owner stores only its opaque identity. Holding the object prevents pointer
/// reuse and proves that a later delegate callback matches this exact load.
#[derive(Debug)]
pub(super) struct RetainedNavigationWitness {
    pub(super) view: WebviewId,
    pub(super) attempt: NativeAttempt,
    navigation: objc2::rc::Retained<objc2_web_kit::WKNavigation>,
}

impl RetainedNavigationWitness {
    pub(super) fn from_host_load_result(
        view: WebviewId,
        attempt: NativeAttempt,
        navigation: &objc2_web_kit::WKNavigation,
    ) -> Option<Self> {
        (view == attempt.view()).then(|| Self {
            view,
            attempt,
            navigation: navigation.retain(),
        })
    }

    pub(super) fn matches(
        &self,
        view: WebviewId,
        navigation: &objc2_web_kit::WKNavigation,
    ) -> bool {
        self.view == view && std::ptr::eq(self.navigation.as_ref(), navigation)
    }

    pub(super) fn identity(&self) -> Option<NativeCommitIdentity> {
        // SAFETY: A non-null retained ObjC navigation object is kept alive
        // by this witness through each checked callback. The pointer alone
        // never admits a request without the same view/attempt/source stamp.
        let native = objc2::rc::Retained::as_ptr(&self.navigation) as usize as u64;
        NonZeroU64::new(native).map(NativeCommitIdentity)
    }
}

/// The only checked generation minted by a real, qualified native main-frame
/// commit. KEL142, GH484 and GH462 consume this immutable observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NativeCommittedStamp {
    pub(super) view: WebviewId,
    pub(super) generation: NonZeroU32,
    pub(super) attempt: NativeAttempt,
    pub(super) source: VerifiedSourceId,
    pub(super) native_commit: NativeCommitIdentity,
}

/// Resource access may precede native commit but it cannot mint an IPC grant.
/// The same A lease remains valid across its own successful commit; B revokes A.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ResourceLease {
    pub(super) attempt: NativeAttempt,
    pub(super) source: VerifiedSourceId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DocumentFailure {
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

/// Read-only revocation witness minted only by the sole native document owner.
/// It does not authorize any operation after its Acquire observation.
#[derive(Clone, Debug)]
pub(super) struct NativeDocumentLease {
    stamp: NativeCommittedStamp,
    live: Arc<AtomicBool>,
    source: Option<std::sync::Weak<super::macos_resource_source::AdmittedWebAppSource>>,
}
impl NativeDocumentLease {
    pub(super) fn is_current(&self) -> bool {
        self.live.load(Ordering::Acquire)
    }
    pub(super) const fn stamp(&self) -> NativeCommittedStamp {
        self.stamp
    }
    pub(super) fn belongs_to_source(
        &self,
        source: &Arc<super::macos_resource_source::AdmittedWebAppSource>,
    ) -> bool {
        self.source
            .as_ref()
            .is_some_and(|bound| bound.ptr_eq(&Arc::downgrade(source)))
    }
    pub(super) fn same_binding(&self, other: &Self) -> bool {
        self.stamp == other.stamp && Arc::ptr_eq(&self.live, &other.live)
    }
}

/// Single UI-thread state machine. No timeout, role restart or page-provided
/// value changes its checked native document generation.
#[derive(Debug)]
pub(super) struct NativeDocumentOwner {
    view: WebviewId,
    next_attempt: u64,
    phase: Phase,
    live_lease: Option<Arc<AtomicBool>>,
    source: Option<std::sync::Weak<super::macos_resource_source::AdmittedWebAppSource>>,
}

impl NativeDocumentOwner {
    pub(super) fn lease_for(&self, expected: NativeCommittedStamp) -> Option<NativeDocumentLease> {
        if self.current_stamp() != Some(expected) {
            return None;
        }
        let live = self.live_lease.as_ref()?;
        live.load(Ordering::Acquire).then(|| NativeDocumentLease {
            stamp: expected,
            live: Arc::clone(live),
            source: self.source.clone(),
        })
    }
    /// Bind the actual same-build source identity before any native load.
    pub(super) fn bind_source(
        &mut self,
        source: &Arc<super::macos_resource_source::AdmittedWebAppSource>,
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
    pub(super) fn source_binding(
        &self,
    ) -> Option<Arc<super::macos_resource_source::AdmittedWebAppSource>> {
        self.source.as_ref()?.upgrade()
    }
    fn revoke_live_lease(&mut self) {
        if let Some(lease) = self.live_lease.take() {
            lease.store(false, Ordering::Release);
        }
    }

    pub(super) const fn new(view: WebviewId) -> Self {
        Self {
            view,
            next_attempt: 0,
            phase: Phase::Unbound { generation: 0 },
            live_lease: None,
            source: None,
        }
    }

    /// Snapshot is usable by named IPC and eval only in the qualified Live
    /// phase. The caller still has to enforce final exact-channel Guard,
    /// current authenticated Bun role and native world/frame provenance.
    pub(super) const fn current_stamp(&self) -> Option<NativeCommittedStamp> {
        match self.phase {
            Phase::Live { stamp } => Some(stamp),
            _ => None,
        }
    }

    /// GH473 may read static source before this attempt commits, or while
    /// the successfully committed same-document subresources finish loading.
    /// It has no right to mint an IPC principal from this lease.
    pub(super) fn active_resource_lease(&self, source: VerifiedSourceId) -> Option<ResourceLease> {
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

    pub(super) const fn generation(&self) -> Option<u32> {
        match self.phase {
            Phase::Unbound { generation } | Phase::FailedClosed { generation } => Some(generation),
            Phase::AwaitingCommit { generation, .. } => Some(generation.get()),
            Phase::Live { stamp } => Some(stamp.generation.get()),
            Phase::Exhausted | Phase::Destroyed => None,
        }
    }

    /// Host-qualified full-document load: revoke the old commit BEFORE native
    /// `WebKit` loading begins. Checked increment belongs only to this owner.
    pub(super) fn begin_host_load(
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
    pub(super) fn resource_lease(
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
    pub(super) fn is_current_lease(&self, lease: ResourceLease) -> bool {
        self.resource_lease(lease.attempt, lease.source).is_some()
    }

    /// Consume evidence from the *real* native main-frame commit callback.
    /// Caller must corroborate the retained loadRequest `WKNavigation` object
    /// with `WKWebView`, final committed URL, main frame and verified source.
    pub(super) fn commit_admitted(
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
    pub(super) fn fail_attempt(&mut self, attempt: NativeAttempt) -> bool {
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
    pub(super) fn unexpected_mainframe_commit(&mut self) {
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

    /// Hash/history changes and Bun-only recovery preserve exactly this
    /// checked generation and native resource owner. No hidden rebind/replay.
    pub(super) fn destroy(&mut self) {
        self.revoke_live_lease();
        self.phase = Phase::Destroyed;
    }
}

impl Drop for NativeDocumentOwner {
    fn drop(&mut self) {
        self.revoke_live_lease();
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod recovery_fixture;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub use recovery_fixture::MacNativeDocumentFixture;

#[cfg(test)]
mod tests {
    use super::*;

    fn source(v: u8) -> VerifiedSourceId {
        VerifiedSourceId([v; 32])
    }

    fn native(v: u64) -> NativeCommitIdentity {
        NativeCommitIdentity(NonZeroU64::new(v).expect("test native identity"))
    }

    #[test]
    fn pending_first_source_has_zero_bridge_authority_until_actual_commit() {
        let mut doc = NativeDocumentOwner::new(WebviewId(5));
        assert_eq!(doc.generation(), Some(0));
        assert_eq!(doc.current_stamp(), None);
        let a = doc.begin_host_load(source(7)).expect("pending A");
        let lease = doc.resource_lease(a, source(7)).expect("resource only");
        assert!(doc.is_current_lease(lease));
        assert_eq!(doc.current_stamp(), None);
        assert_eq!(doc.resource_lease(a, source(8)), None);
        let committed = doc
            .commit_admitted(a, source(7), native(10))
            .expect("commit");
        // GH542: trusted Host load revokes +1 BEFORE native call. Matching
        // real cross-document commit adds another checked +1 (0->1->2).
        assert_eq!(committed.generation.get(), 2);
        assert_eq!(doc.current_stamp(), Some(committed));
        assert!(doc.is_current_lease(lease));
        assert_eq!(doc.commit_admitted(a, source(7), native(10)), Ok(committed));
        assert_eq!(doc.generation(), Some(2));
    }

    #[test]
    fn late_a_fail_and_duplicate_after_b_begin_or_commit_leave_b_usable() {
        let mut doc = NativeDocumentOwner::new(WebviewId(7));
        let a = doc.begin_host_load(source(1)).expect("A");
        let lease_a = doc.resource_lease(a, source(1)).expect("A lease");
        let b = doc.begin_host_load(source(2)).expect("B");
        let lease_b = doc.resource_lease(b, source(2)).expect("B lease");
        assert!(!doc.fail_attempt(a));
        assert!(!doc.is_current_lease(lease_a));
        assert!(doc.is_current_lease(lease_b));
        let committed_b = doc
            .commit_admitted(b, source(2), native(20))
            .expect("B commit");
        assert!(!doc.fail_attempt(a));
        assert!(!doc.fail_attempt(a));
        assert_eq!(doc.current_stamp(), Some(committed_b));
        assert!(doc.is_current_lease(lease_b));
        assert_eq!(
            doc.commit_admitted(a, source(1), native(10)),
            Err(DocumentFailure::StaleAttempt)
        );
    }

    #[test]
    fn matching_b_failure_after_commit_revokes_b_but_late_a_failure_does_not() {
        let mut doc = NativeDocumentOwner::new(WebviewId(4));
        let a = doc.begin_host_load(source(1)).expect("A");
        let b = doc.begin_host_load(source(2)).expect("B");
        let b_lease = doc.resource_lease(b, source(2)).expect("B lease");
        let b_stamp = doc.commit_admitted(b, source(2), native(88)).expect("B");
        assert!(!doc.fail_attempt(a), "late A never revokes B");
        assert_eq!(doc.current_stamp(), Some(b_stamp));
        assert!(doc.is_current_lease(b_lease));
        assert!(
            doc.fail_attempt(b),
            "matching live B failed navigation must revoke B"
        );
        assert_eq!(doc.current_stamp(), None);
        assert!(!doc.is_current_lease(b_lease));
        assert!(!doc.fail_attempt(b), "duplicate failure is inert");
        assert!(!doc.fail_attempt(a), "old A still inert");
        let c = doc
            .begin_host_load(source(3))
            .expect("fresh C after failure");
        assert_ne!(c, b);
        assert_eq!(doc.generation(), Some(4));
    }

    #[test]
    fn matching_current_failure_does_not_restore_old_document() {
        let mut doc = NativeDocumentOwner::new(WebviewId(7));
        let a = doc.begin_host_load(source(1)).expect("A");
        doc.commit_admitted(a, source(1), native(30))
            .expect("A commit");
        let b = doc.begin_host_load(source(2)).expect("B");
        assert_eq!(doc.current_stamp(), None);
        assert!(doc.fail_attempt(b));
        assert_eq!(doc.current_stamp(), None);
        assert_eq!(doc.resource_lease(b, source(2)), None);
        assert!(!doc.fail_attempt(a));
        let c = doc.begin_host_load(source(3)).expect("C");
        assert_ne!(b, c);
        assert_eq!(doc.generation(), Some(4));
    }

    #[test]
    fn source_identity_and_native_callback_identity_are_both_required() {
        let mut doc = NativeDocumentOwner::new(WebviewId(7));
        let a = doc.begin_host_load(source(1)).expect("A");
        assert_eq!(
            doc.commit_admitted(a, source(2), native(22)),
            Err(DocumentFailure::SourceMismatch)
        );
        assert_eq!(doc.current_stamp(), None);
        let b = doc.begin_host_load(source(1)).expect("B");
        let stamp = doc
            .commit_admitted(b, source(1), native(33))
            .expect("B commit");
        assert_eq!(
            doc.commit_admitted(b, source(1), native(34)),
            Err(DocumentFailure::NativeMismatch)
        );
        assert_eq!(doc.current_stamp(), None);
        assert!(doc.generation().expect("closed gen") > stamp.generation.get());
    }

    #[test]
    fn dropping_the_only_native_owner_revokes_its_existing_lease() {
        let mut owner = NativeDocumentOwner::new(WebviewId(17));
        let attempt = owner.begin_host_load(source(1)).expect("one model attempt");
        let stamp = owner
            .commit_admitted(attempt, source(1), native(1))
            .expect("one model commit");
        let lease = owner.lease_for(stamp).expect("existing model lease");
        assert!(lease.is_current());
        drop(owner);
        assert!(!lease.is_current());
    }

    #[test]
    fn actual_unexpected_replacement_revokes_checked_stamp() {
        let mut doc = NativeDocumentOwner::new(WebviewId(10));
        let a = doc.begin_host_load(source(1)).expect("A");
        let stamp = doc
            .commit_admitted(a, source(1), native(1))
            .expect("A commit");
        assert_eq!(doc.current_stamp(), Some(stamp));
        doc.unexpected_mainframe_commit();
        assert_eq!(doc.current_stamp(), None);
        assert_eq!(doc.generation(), Some(3));
    }

    #[test]
    fn exhausted_and_destroyed_are_terminal_no_counter_wrap() {
        let mut doc = NativeDocumentOwner {
            view: WebviewId(2),
            next_attempt: 42,
            live_lease: None,
            source: None,
            phase: Phase::FailedClosed {
                generation: u32::MAX,
            },
        };
        assert_eq!(
            doc.begin_host_load(source(1)),
            Err(DocumentFailure::Exhausted)
        );
        assert_eq!(
            doc.begin_host_load(source(1)),
            Err(DocumentFailure::Exhausted)
        );
        let mut serial = NativeDocumentOwner {
            view: WebviewId(2),
            next_attempt: u64::MAX,
            live_lease: None,
            source: None,
            phase: Phase::Unbound { generation: 0 },
        };
        assert_eq!(
            serial.begin_host_load(source(1)),
            Err(DocumentFailure::Exhausted)
        );
        let mut dead = NativeDocumentOwner::new(WebviewId(1));
        dead.destroy();
        assert_eq!(dead.current_stamp(), None);
        assert_eq!(
            dead.begin_host_load(source(1)),
            Err(DocumentFailure::Destroyed)
        );
    }

    #[test]
    fn foreign_view_and_stale_source_never_qualify() {
        let mut first = NativeDocumentOwner::new(WebviewId(1));
        let mut second = NativeDocumentOwner::new(WebviewId(2));
        let a = first.begin_host_load(source(1)).expect("first");
        let b = second.begin_host_load(source(1)).expect("second");
        assert_eq!(second.resource_lease(a, source(1)), None);
        assert_eq!(
            second.commit_admitted(a, source(1), native(22)),
            Err(DocumentFailure::WrongView)
        );
        assert_eq!(second.current_stamp(), None);
        assert_eq!(
            second
                .commit_admitted(b, source(1), native(33))
                .unwrap()
                .view,
            WebviewId(2)
        );
    }

    #[test]
    fn unexpected_native_replacement_retires_pending_source_lease() {
        let mut doc = NativeDocumentOwner::new(WebviewId(13));
        let source_a = source(7);
        let attempt = doc.begin_host_load(source_a).expect("pending load");
        let lease = doc
            .resource_lease(attempt, source_a)
            .expect("pending lease");
        assert_eq!(doc.current_stamp(), None);
        assert!(doc.is_current_lease(lease));

        // A native commit/finish without its retained navigation witness
        // must use this existing fail-closed transition, never a guessed
        // current attempt. This model proves revocation, not callback wiring.
        doc.unexpected_mainframe_commit();

        assert_eq!(doc.current_stamp(), None);
        assert!(!doc.is_current_lease(lease));
        assert_eq!(doc.active_resource_lease(source_a), None);
        assert_eq!(
            doc.commit_admitted(attempt, source_a, native(71)),
            Err(DocumentFailure::StaleAttempt)
        );
        let next = doc.begin_host_load(source(8)).expect("fresh host load");
        assert_ne!(attempt, next);
        assert!(doc.resource_lease(next, source(8)).is_some());
        assert!(!doc.is_current_lease(lease));
    }

    #[test]
    fn unexpected_native_replacement_retires_live_stamp_and_source_lease() {
        let mut doc = NativeDocumentOwner::new(WebviewId(14));
        let source_a = source(9);
        let attempt = doc.begin_host_load(source_a).expect("host load");
        let stamp = doc
            .commit_admitted(attempt, source_a, native(91))
            .expect("qualified native commit");
        let lease = doc.resource_lease(attempt, source_a).expect("live lease");
        assert_eq!(doc.current_stamp(), Some(stamp));
        assert!(doc.is_current_lease(lease));

        doc.unexpected_mainframe_commit();

        assert_eq!(doc.current_stamp(), None);
        assert!(!doc.is_current_lease(lease));
        assert_eq!(doc.active_resource_lease(source_a), None);
        assert_eq!(doc.generation(), Some(3));
        assert_eq!(
            doc.commit_admitted(attempt, source_a, native(91)),
            Err(DocumentFailure::StaleAttempt)
        );
        doc.destroy();
        doc.unexpected_mainframe_commit();
        assert_eq!(doc.generation(), None);
        assert_eq!(doc.current_stamp(), None);
        assert_eq!(
            doc.begin_host_load(source_a),
            Err(DocumentFailure::Destroyed)
        );
    }
}

#[cfg(test)]
mod admission_lease_tests {
    use super::*;
    fn source(n: u8) -> VerifiedSourceId {
        VerifiedSourceId([n; 32])
    }
    fn native(n: u64) -> NativeCommitIdentity {
        NativeCommitIdentity(NonZeroU64::new(n).expect("native"))
    }
    #[test]
    fn duplicate_commit_reuses_one_lease_and_host_load_revokes_before_commit() {
        let mut owner = NativeDocumentOwner::new(WebviewId(1));
        let a = owner.begin_host_load(source(1)).expect("A");
        let stamp = owner
            .commit_admitted(a, source(1), native(1))
            .expect("commit A");
        let lease = owner.lease_for(stamp).expect("A lease");
        assert_eq!(owner.commit_admitted(a, source(1), native(1)), Ok(stamp));
        let duplicate = owner.lease_for(stamp).expect("duplicate lease");
        assert!(Arc::ptr_eq(&lease.live, &duplicate.live));
        assert!(lease.is_current());
        let _b = owner.begin_host_load(source(2)).expect("B");
        assert!(!lease.is_current());
        assert!(owner.lease_for(stamp).is_none());
    }
    #[test]
    fn stale_a_fail_cannot_revoke_b_but_matching_fail_destroy_and_replacement_do() {
        for terminal in 0..3 {
            let mut owner = NativeDocumentOwner::new(WebviewId(1));
            let a = owner.begin_host_load(source(1)).expect("A");
            let b = owner.begin_host_load(source(2)).expect("B");
            let stamp = owner
                .commit_admitted(b, source(2), native(2))
                .expect("commit B");
            let lease = owner.lease_for(stamp).expect("B lease");
            assert!(!owner.fail_attempt(a));
            assert!(lease.is_current());
            match terminal {
                0 => {
                    assert!(owner.fail_attempt(b));
                }
                1 => owner.destroy(),
                _ => owner.unexpected_mainframe_commit(),
            }
            assert!(!lease.is_current());
        }
    }
}
