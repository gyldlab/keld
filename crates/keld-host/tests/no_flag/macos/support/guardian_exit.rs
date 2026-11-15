//! The one test-owned classifier for a source-attested original Guardian's
//! kernel exit event, shared by T1B `lifecycle` and the public-FS selector.
//!
//! `EVFILT_PROC` keeps a fired `NOTE_EXIT` queued until a read dequeues it, and
//! the event carries no timestamp. A dequeued event therefore proves only that
//! the exit happened before *that read*. An empty sealing read before Quit
//! orders later events after the seal, not after Quit: exit may occur between
//! the seal and the caller's release. An event read before receipt verification
//! is also known to precede that verification. Only an empty read after the
//! selected Bun's verified Quit-body receipt orders a later event after receipt.
//!
//! An absent event is never evidence of liveness. A refused registration, a
//! failed or interrupted read, or a read that finishes past the caller's
//! deadline never becomes a successful kernel-exit observation.

use crate::support::process::ProcessExitObserver;
use std::process::ExitStatus;
use std::time::Instant;

/// One native read of the armed observer; tests inject a fault here.
type NativeRead<'a> =
    dyn FnMut(&mut ProcessExitObserver) -> std::io::Result<Option<ExitStatus>> + 'a;

/// Fixed, payload-free classes. Markers never carry a PID or event data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuardianExitPhase {
    /// The kernel refused the nonchild subscription; no watch ever existed.
    RegistrationRefused,
    /// Registered, but not yet sealed by a read immediately before Quit.
    Registered,
    /// The sealing read found the exit already queued: exit preceded Quit.
    ObservedBeforeQuitRelease,
    /// Every read so far was empty. Never proof that the Guardian lives.
    NotObserved,
    /// Exit after the seal, unordered with Quit; no body-receipt fence (T1B).
    ObservedAfterSeal,
    /// Exit after the seal, unordered with Quit, read before receipt verification.
    ObservedAfterSealBeforeBodyReceipt,
    /// First read after receipt verification found the exit already queued,
    /// so it may have happened before or after that verification.
    ObservedUnorderedWithBodyReceipt,
    /// An empty read followed receipt verification, then the exit arrived.
    ObservedAfterBodyReceipt,
    /// A failed or interrupted read, or one that ended past the deadline.
    Unproved,
}

impl GuardianExitPhase {
    pub(crate) const fn marker(self) -> &'static str {
        match self {
            Self::RegistrationRefused => "guardian_exit_event_registration_refused",
            Self::Registered => "guardian_exit_event_registered_unsealed",
            Self::ObservedBeforeQuitRelease => "guardian_exit_event_observed_before_quit_release",
            Self::NotObserved => "guardian_exit_event_not_observed",
            Self::ObservedAfterSeal => "guardian_exit_event_observed_after_seal",
            Self::ObservedAfterSealBeforeBodyReceipt => {
                "guardian_exit_event_observed_after_seal_before_body_receipt"
            }
            Self::ObservedUnorderedWithBodyReceipt => {
                "guardian_exit_event_observed_not_ordered_after_body_receipt"
            }
            Self::ObservedAfterBodyReceipt => "guardian_exit_event_observed_after_body_receipt",
            Self::Unproved => "guardian_exit_event_unproved",
        }
    }
}

/// Whether, and how far, the optional body-receipt ordering fence has moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BodyReceiptFence {
    Untracked,
    Pending,
    /// Verified, but no empty read has happened since.
    Verified,
    /// An empty read happened strictly after verification.
    Ordered,
}

pub(crate) struct GuardianExitWitness {
    observer: Option<ProcessExitObserver>,
    phase: GuardianExitPhase,
    fence: BodyReceiptFence,
}

impl GuardianExitWitness {
    /// Register the original Guardian with no body-receipt fence (T1B).
    pub(crate) fn register(original_guardian: u32) -> Self {
        Self::register_with(original_guardian, BodyReceiptFence::Untracked)
    }

    /// Register the original Guardian and track the selected Bun's verified
    /// Quit-body receipt as a second ordering fence (public-FS).
    pub(crate) fn register_with_body_receipt_fence(original_guardian: u32) -> Self {
        Self::register_with(original_guardian, BodyReceiptFence::Pending)
    }

    fn register_with(original_guardian: u32, fence: BodyReceiptFence) -> Self {
        match ProcessExitObserver::try_arm(original_guardian) {
            Ok(observer) => Self {
                observer: Some(observer),
                phase: GuardianExitPhase::Registered,
                fence,
            },
            Err(_) => Self {
                observer: None,
                phase: GuardianExitPhase::RegistrationRefused,
                fence,
            },
        }
    }

    /// Call before releasing Quit: an event read here happened before release.
    /// An empty read orders later events after this seal, not after release;
    /// the Guardian can exit in the gap even when the caller releases next.
    pub(crate) fn seal_before_quit_release(&mut self) {
        self.seal_with(&mut ProcessExitObserver::try_read_exit);
    }

    fn seal_with(&mut self, read: &mut NativeRead<'_>) {
        if self.phase != GuardianExitPhase::Registered {
            return;
        }
        self.phase = match self.read_with(read) {
            Ok(Some(())) => GuardianExitPhase::ObservedBeforeQuitRelease,
            Ok(None) => GuardianExitPhase::NotObserved,
            Err(()) => GuardianExitPhase::Unproved,
        };
    }

    /// Record that the test has just seen the body receipt verified. Only the
    /// next empty read can order a later event after it.
    pub(crate) fn note_body_receipt_verified(&mut self) {
        if self.fence == BodyReceiptFence::Pending {
            self.fence = BodyReceiptFence::Verified;
        }
    }

    /// One zero-time read; the Host wait callback calls this only while the
    /// Host Child is live. A read ending at or past `deadline` is unproved.
    pub(crate) fn poll(&mut self, deadline: Instant) {
        self.poll_with(
            deadline,
            &mut Instant::now,
            &mut ProcessExitObserver::try_read_exit,
        );
    }

    /// `poll` with the clock and the native read injectable, so tests can place
    /// the deadline between the two clock reads without sleeping.
    fn poll_with(
        &mut self,
        deadline: Instant,
        now: &mut dyn FnMut() -> Instant,
        read: &mut NativeRead<'_>,
    ) {
        if self.phase != GuardianExitPhase::NotObserved {
            return;
        }
        if now() >= deadline {
            self.phase = GuardianExitPhase::Unproved;
            return;
        }
        let event = self.read_with(read);
        // A zero-time kernel read can still be scheduled past the Host clock.
        if now() >= deadline {
            self.phase = GuardianExitPhase::Unproved;
            return;
        }
        match event {
            Ok(None) => {
                if self.fence == BodyReceiptFence::Verified {
                    self.fence = BodyReceiptFence::Ordered;
                }
            }
            Ok(Some(())) => {
                self.phase = match self.fence {
                    BodyReceiptFence::Untracked => GuardianExitPhase::ObservedAfterSeal,
                    BodyReceiptFence::Pending => {
                        GuardianExitPhase::ObservedAfterSealBeforeBodyReceipt
                    }
                    BodyReceiptFence::Verified => {
                        GuardianExitPhase::ObservedUnorderedWithBodyReceipt
                    }
                    BodyReceiptFence::Ordered => GuardianExitPhase::ObservedAfterBodyReceipt,
                };
            }
            Err(()) => self.phase = GuardianExitPhase::Unproved,
        }
    }

    pub(crate) const fn phase(&self) -> GuardianExitPhase {
        self.phase
    }

    pub(crate) const fn marker(&self) -> &'static str {
        self.phase.marker()
    }

    /// Only a completed empty read is evidence. Every error, including an
    /// interrupted read that may not have scanned the queue, is unproved.
    fn read_with(&mut self, read: &mut NativeRead<'_>) -> Result<Option<()>, ()> {
        let observer = self.observer.as_mut().ok_or(())?;
        read(observer)
            .map(|status| status.map(|_| ()))
            .map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::{GuardianExitPhase, GuardianExitWitness};
    use crate::support::PROCESS_DEADLINE;
    use crate::support::process::ProcessExitObserver;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::thread;
    use std::time::Instant;

    struct OwnedShell(Child);

    impl Drop for OwnedShell {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    impl OwnedShell {
        /// A real `/bin/sh` blocked in its built-in read on test-owned stdin;
        /// closing that stdin is the only thing that lets it exit 23.
        fn status23() -> Self {
            Self(
                Command::new("/bin/sh")
                    .args(["-c", "IFS= read -r line; exit 23"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("start owned native status23 shell"),
            )
        }

        fn assert_live(&mut self) {
            assert!(
                self.0
                    .try_wait()
                    .expect("inspect retained original child")
                    .is_none(),
                "the original shell exited before the test released stdin"
            );
        }

        fn release(&mut self) {
            drop(self.0.stdin.take().expect("release only owned shell stdin"));
        }

        fn reap_status23(mut self) {
            assert_eq!(
                self.0.wait().expect("reap exact owned shell").code(),
                Some(23),
                "kernel event must correspond to the original exiting process"
            );
        }
    }

    /// Independent oracle: a second kqueue, never the witness under test,
    /// proves the exit has happened. It neither reaps nor signals.
    fn await_exit_independently(oracle: &mut ProcessExitObserver) {
        let deadline = Instant::now() + PROCESS_DEADLINE;
        loop {
            if let Some(status) = oracle.try_poll_exit().expect("independent oracle read") {
                assert_eq!(status.code(), Some(23), "oracle saw the original exit");
                return;
            }
            assert!(
                Instant::now() < deadline,
                "independent oracle missed the native exit before the process deadline"
            );
            thread::yield_now();
        }
    }

    fn poll_until_observed(witness: &mut GuardianExitWitness, deadline: Instant) {
        loop {
            witness.poll(deadline);
            if witness.phase() != GuardianExitPhase::NotObserved {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "native status23 event missing within the original process deadline"
            );
            thread::yield_now();
        }
    }

    /// Exit after an empty sealing read is ordered only after that seal. A
    /// constant `Observed` fails at the live-child read; a dropped event fails
    /// the deadline.
    #[test]
    fn native_exit_after_empty_seal_is_ordered_after_seal() {
        let mut child = OwnedShell::status23();
        let mut witness = GuardianExitWitness::register(child.0.id());
        assert_eq!(witness.phase(), GuardianExitPhase::Registered);
        witness.seal_before_quit_release();
        assert_eq!(witness.phase(), GuardianExitPhase::NotObserved);
        let deadline = Instant::now() + PROCESS_DEADLINE;
        child.assert_live();
        witness.poll(deadline);
        assert_eq!(
            witness.phase(),
            GuardianExitPhase::NotObserved,
            "a live original child must not produce a native exit event"
        );
        child.release();
        poll_until_observed(&mut witness, deadline);
        assert_eq!(witness.phase(), GuardianExitPhase::ObservedAfterSeal);
        child.reap_status23();
    }

    /// A real exit in the empty-seal-to-Quit gap is not ordered after Quit.
    /// The independent observer confirms exit before the logical Quit channel
    /// is released; neither witness is polled until after that release.
    #[test]
    fn native_exit_between_empty_seal_and_quit_release_is_only_after_seal() {
        let deadline = Instant::now() + PROCESS_DEADLINE;
        let mut child = OwnedShell::status23();
        let mut oracle = ProcessExitObserver::arm(child.0.id());
        let mut untracked = GuardianExitWitness::register(child.0.id());
        let mut pending = GuardianExitWitness::register_with_body_receipt_fence(child.0.id());
        let (quit_release, quit_released) = std::sync::mpsc::sync_channel(1);
        child.assert_live();
        untracked.seal_before_quit_release();
        pending.seal_before_quit_release();
        assert_eq!(untracked.phase(), GuardianExitPhase::NotObserved);
        assert_eq!(pending.phase(), GuardianExitPhase::NotObserved);
        child.release();
        loop {
            let status = oracle.try_poll_exit().expect("independent gap exit read");
            assert!(
                Instant::now() < deadline,
                "gap exit exceeded original deadline"
            );
            if let Some(status) = status {
                assert_eq!(status.code(), Some(23), "original child exited in the gap");
                break;
            }
            thread::yield_now();
        }
        quit_release
            .send(())
            .expect("release logical Quit after exit");
        quit_released.try_recv().expect("logical Quit was released");
        untracked.poll(deadline);
        pending.poll(deadline);
        assert_eq!(
            (untracked.marker(), pending.marker()),
            (
                "guardian_exit_event_observed_after_seal",
                "guardian_exit_event_observed_after_seal_before_body_receipt",
            ),
            "an exit proved before Quit must not receive an after-Quit label"
        );
        child.reap_status23();
    }

    /// The exit happens after registration but
    /// before Quit release. The sealing read must classify it as pre-Quit,
    /// and no later poll may relabel it as a post-Quit exit.
    #[test]
    fn native_exit_before_quit_release_is_never_reported_after_release() {
        let mut child = OwnedShell::status23();
        let mut oracle = ProcessExitObserver::arm(child.0.id());
        let mut witness = GuardianExitWitness::register(child.0.id());
        child.assert_live();
        child.release();
        await_exit_independently(&mut oracle);
        witness.seal_before_quit_release();
        witness.poll(Instant::now() + PROCESS_DEADLINE);
        assert_eq!(
            witness.phase(),
            GuardianExitPhase::ObservedBeforeQuitRelease,
            "an exit that preceded Quit release must not read as post-Quit"
        );
        child.reap_status23();
    }

    /// Exit after Quit release but before the body receipt is verified, first
    /// dequeued after verification: it must stay unordered, never post-receipt.
    #[test]
    fn native_exit_before_body_receipt_is_not_ordered_after_receipt() {
        let mut child = OwnedShell::status23();
        let mut oracle = ProcessExitObserver::arm(child.0.id());
        let mut witness = GuardianExitWitness::register_with_body_receipt_fence(child.0.id());
        child.assert_live();
        witness.seal_before_quit_release();
        assert_eq!(witness.phase(), GuardianExitPhase::NotObserved);
        child.release();
        await_exit_independently(&mut oracle);
        // Simulated receipt verification after the real exit, before any read.
        witness.note_body_receipt_verified();
        witness.poll(Instant::now() + PROCESS_DEADLINE);
        assert_eq!(
            witness.phase(),
            GuardianExitPhase::ObservedUnorderedWithBodyReceipt,
            "an event first read after verification is not ordered after it"
        );
        child.reap_status23();
    }

    /// The same exit dequeued before verification is classified as before the
    /// receipt, and a later verification cannot relabel it.
    #[test]
    fn native_exit_read_before_body_receipt_stays_before_receipt() {
        let mut child = OwnedShell::status23();
        let mut oracle = ProcessExitObserver::arm(child.0.id());
        let mut witness = GuardianExitWitness::register_with_body_receipt_fence(child.0.id());
        witness.seal_before_quit_release();
        child.release();
        await_exit_independently(&mut oracle);
        let deadline = Instant::now() + PROCESS_DEADLINE;
        poll_until_observed(&mut witness, deadline);
        witness.note_body_receipt_verified();
        witness.poll(deadline);
        assert_eq!(
            witness.phase(),
            GuardianExitPhase::ObservedAfterSealBeforeBodyReceipt
        );
        child.reap_status23();
    }

    /// Only an empty read after verification orders a later real exit after
    /// the body receipt.
    #[test]
    fn native_exit_after_empty_post_receipt_read_is_after_receipt() {
        let mut child = OwnedShell::status23();
        let mut witness = GuardianExitWitness::register_with_body_receipt_fence(child.0.id());
        witness.seal_before_quit_release();
        let deadline = Instant::now() + PROCESS_DEADLINE;
        child.assert_live();
        witness.note_body_receipt_verified();
        witness.poll(deadline);
        assert_eq!(witness.phase(), GuardianExitPhase::NotObserved);
        child.release();
        poll_until_observed(&mut witness, deadline);
        assert_eq!(witness.phase(), GuardianExitPhase::ObservedAfterBodyReceipt);
        child.reap_status23();
    }

    /// A kernel-refused subscription (the reaped PID no longer exists) is its
    /// own class, distinct from a registered watch whose read is unproved.
    #[test]
    fn refused_registration_is_distinct_from_unproved() {
        let mut gone = Command::new("/usr/bin/true")
            .spawn()
            .expect("start owned short-lived process");
        let pid = gone.id();
        assert!(gone.wait().expect("reap owned process").success());
        let mut witness = GuardianExitWitness::register(pid);
        witness.seal_before_quit_release();
        witness.poll(Instant::now() + PROCESS_DEADLINE);
        assert_eq!(witness.phase(), GuardianExitPhase::RegistrationRefused);
    }

    /// A poll that starts at or past the caller's deadline is unproved even
    /// when the real exit event is already queued. This reaches only the check
    /// before the read; the check after it is pinned by the next test.
    #[test]
    fn native_exit_read_past_deadline_is_unproved() {
        let mut child = OwnedShell::status23();
        let mut oracle = ProcessExitObserver::arm(child.0.id());
        let mut witness = GuardianExitWitness::register(child.0.id());
        witness.seal_before_quit_release();
        child.release();
        await_exit_independently(&mut oracle);
        witness.poll(Instant::now());
        assert_eq!(witness.phase(), GuardianExitPhase::Unproved);
        child.reap_status23();
    }

    /// A read that starts before the deadline but ends at it is unproved even
    /// though the real queued exit was dequeued. The injected clock is before
    /// the deadline for the pre-read check and at it for the post-read check;
    /// without the post-read check this reads as `ObservedAfterSeal`.
    #[test]
    fn native_exit_read_ending_at_deadline_is_unproved() {
        let mut child = OwnedShell::status23();
        let mut oracle = ProcessExitObserver::arm(child.0.id());
        let mut witness = GuardianExitWitness::register(child.0.id());
        child.assert_live();
        witness.seal_before_quit_release();
        assert_eq!(witness.phase(), GuardianExitPhase::NotObserved);
        child.release();
        await_exit_independently(&mut oracle);
        let before_deadline = Instant::now();
        let deadline = before_deadline + PROCESS_DEADLINE;
        let mut clock = [before_deadline, deadline].into_iter();
        let mut reads = 0_u8;
        witness.poll_with(
            deadline,
            &mut || clock.next().expect("poll reads the clock exactly twice"),
            &mut |observer| {
                reads += 1;
                observer.try_read_exit()
            },
        );
        assert_eq!(reads, 1, "the pre-read check let the native read run");
        assert_eq!(
            witness.phase(),
            GuardianExitPhase::Unproved,
            "a dequeued exit from a read that ended at the deadline is unproved"
        );
        assert_eq!(
            clock.next(),
            None,
            "both deadline checks consulted the clock"
        );
        child.reap_status23();
    }

    fn interrupted(_: &mut ProcessExitObserver) -> std::io::Result<Option<ExitStatus>> {
        Err(std::io::Error::from(std::io::ErrorKind::Interrupted))
    }

    /// An interrupted read may not have scanned the queue, so it is never a
    /// proven-empty read: neither the seal nor the post-receipt fence may
    /// advance on it. Mapping `Interrupted` to "no event" fails both asserts.
    #[test]
    fn interrupted_native_read_is_unproved_not_empty() {
        let mut child = OwnedShell::status23();
        let mut sealed = GuardianExitWitness::register(child.0.id());
        let mut fenced = GuardianExitWitness::register_with_body_receipt_fence(child.0.id());
        child.assert_live();
        sealed.seal_with(&mut interrupted);
        assert_eq!(
            sealed.phase(),
            GuardianExitPhase::Unproved,
            "an interrupted sealing read must not order later exits after the seal"
        );
        fenced.seal_before_quit_release();
        assert_eq!(fenced.phase(), GuardianExitPhase::NotObserved);
        fenced.note_body_receipt_verified();
        fenced.poll_with(
            Instant::now() + PROCESS_DEADLINE,
            &mut Instant::now,
            &mut interrupted,
        );
        assert_eq!(
            fenced.phase(),
            GuardianExitPhase::Unproved,
            "an interrupted post-receipt read must not order the fence"
        );
        child.release();
        child.reap_status23();
    }

    /// The kernel's own EINTR (`kevent64` -1), injected inside the real read
    /// and reached only through `seal_before_quit_release` and `poll`, stays
    /// unproved. Wiring either back to `try_poll_exit`, or letting
    /// `try_read_exit` map EINTR to no event, makes these reads empty.
    #[test]
    fn kernel_interrupted_witness_read_is_unproved_not_empty() {
        let mut child = OwnedShell::status23();
        let mut sealed = GuardianExitWitness::register(child.0.id());
        let mut fenced = GuardianExitWitness::register_with_body_receipt_fence(child.0.id());
        let mut direct = ProcessExitObserver::arm(child.0.id());
        child.assert_live();
        direct.interrupt_next_read();
        assert_eq!(
            direct.try_read_exit().map_err(|error| error.kind()),
            Err(std::io::ErrorKind::Interrupted),
            "the native read keeps the kernel's EINTR as an error"
        );
        direct.interrupt_next_read();
        assert_eq!(
            direct.try_poll_exit().map_err(|error| error.kind()),
            Ok(None),
            "retry loops still read EINTR as no event"
        );
        sealed
            .observer
            .as_mut()
            .expect("registered live child")
            .interrupt_next_read();
        sealed.seal_before_quit_release();
        assert_eq!(
            sealed.phase(),
            GuardianExitPhase::Unproved,
            "a kernel-interrupted sealing read must not order later exits after the seal"
        );
        fenced.seal_before_quit_release();
        assert_eq!(fenced.phase(), GuardianExitPhase::NotObserved);
        fenced.note_body_receipt_verified();
        fenced
            .observer
            .as_mut()
            .expect("registered live child")
            .interrupt_next_read();
        child.assert_live();
        fenced.poll(Instant::now() + PROCESS_DEADLINE);
        assert_eq!(
            fenced.phase(),
            GuardianExitPhase::Unproved,
            "a kernel-interrupted post-receipt read must not order the fence"
        );
        child.release();
        child.reap_status23();
    }
}
