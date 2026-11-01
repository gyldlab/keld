//! Terminal diagnostic adapters only; these tests do not establish native shutdown.
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::process::Output;
use std::sync::atomic::{AtomicU8, Ordering};

/// One wait recipe; the real process wait retains its existing custody owner.
pub(super) struct Wait<'a, W, E> {
    progress: &'a AtomicU8,
    collect_output: W,
    emit: E,
}

impl<'a, W, E> Wait<'a, W, E>
where
    W: FnOnce(&mut dyn FnMut()) -> Output,
    E: FnMut(&str),
{
    pub(super) fn new(progress: &'a AtomicU8, collect_output: W, emit: E) -> Self {
        Self {
            progress,
            collect_output,
            emit,
        }
    }

    /// Ordinary launches suppress Quit attribution when their observations failed.
    pub(super) fn public(self, observation_succeeded: bool) -> Output {
        self.finish(observation_succeeded, || {})
    }

    /// Retirement reaches this adapter only after successful observation assertions.
    pub(super) fn retirement(self, after_wait: impl FnOnce()) -> Output {
        self.finish(true, after_wait)
    }

    fn finish(self, observation_succeeded: bool, after_wait: impl FnOnce()) -> Output {
        let Self {
            progress,
            collect_output,
            mut emit,
        } = self;
        let mut last_live_http_quit_stage = 0_u8;
        let host_exit = catch_unwind(AssertUnwindSafe(|| {
            collect_output(&mut || {
                last_live_http_quit_stage = progress.load(Ordering::Acquire);
            })
        }));
        // Caller-owned peers remain held through the caught wait, but are released
        // before either failure diagnostics or the original panic is propagated.
        after_wait();
        host_exit.unwrap_or_else(|failure| {
            if observation_succeeded {
                let stage = match last_live_http_quit_stage {
                    0 => "no_http_quit_observed",
                    1 => "http_quit_received",
                    2 => "observer_response_written",
                    _ => "invalid_observer_stage",
                };
                // Fixed phase enum only; never reveal HTTP/IPC data or native IDs.
                emit(&format!(
                    "KELD_KEL140_NATIVE_QUIT_PHASE last_live_http_stage={stage}"
                ));
            }
            resume_unwind(failure)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Wait;
    use std::any::Any;
    use std::cell::RefCell;
    use std::os::unix::process::ExitStatusExt as _;
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
    use std::process::{ExitStatus, Output};
    use std::sync::atomic::{AtomicU8, Ordering};

    struct Peer<'a>(&'a RefCell<Vec<&'static str>>);

    impl Drop for Peer<'_> {
        fn drop(&mut self) {
            self.0.borrow_mut().push("drop_peer");
        }
    }

    fn failed_wait(
        retirement: bool,
        stage: u8,
        observation_succeeded: bool,
        sample: bool,
    ) -> (Vec<String>, Vec<&'static str>) {
        let progress = AtomicU8::new(stage);
        let events = RefCell::new(Vec::new());
        let mut lines = Vec::new();
        let failure: Box<dyn Any + Send> = Box::new("original wait failure");
        let original_payload = std::ptr::from_ref(failure.as_ref());
        let wait = Wait::new(
            &progress,
            |observe: &mut dyn FnMut()| -> Output {
                events.borrow_mut().push("wait");
                if sample {
                    observe();
                }
                // A later observer update must not replace the last callback sample.
                progress.store(2, Ordering::Release);
                resume_unwind(failure)
            },
            |line: &str| {
                events.borrow_mut().push("emit");
                lines.push(line.to_owned());
            },
        );
        let result = catch_unwind(AssertUnwindSafe(|| {
            if retirement {
                let peer = Peer(&events);
                wait.retirement(|| {
                    progress.store(u8::MAX, Ordering::Release);
                    drop(peer);
                })
            } else {
                wait.public(observation_succeeded)
            }
        }));
        let propagated = result.expect_err("the injected wait must remain a failure");
        assert!(
            std::ptr::eq(original_payload, std::ptr::from_ref(propagated.as_ref())),
            "the exact original panic payload must be resumed"
        );
        events.borrow_mut().push("caught");
        (lines, events.into_inner())
    }

    #[test]
    fn public_and_retirement_failures_emit_identical_fixed_lines() {
        for (stage, expected) in [
            (
                0,
                "KELD_KEL140_NATIVE_QUIT_PHASE last_live_http_stage=no_http_quit_observed",
            ),
            (
                1,
                "KELD_KEL140_NATIVE_QUIT_PHASE last_live_http_stage=http_quit_received",
            ),
            (
                2,
                "KELD_KEL140_NATIVE_QUIT_PHASE last_live_http_stage=observer_response_written",
            ),
            (
                u8::MAX,
                "KELD_KEL140_NATIVE_QUIT_PHASE last_live_http_stage=invalid_observer_stage",
            ),
        ] {
            let (public, public_events) = failed_wait(false, stage, true, true);
            let (retirement, retirement_events) = failed_wait(true, stage, true, true);
            assert_eq!(public, vec![expected.to_owned()]);
            assert_eq!(retirement, public, "both actual terminal adapters agree");
            assert_eq!(public_events, ["wait", "emit", "caught"]);
            assert_eq!(retirement_events, ["wait", "drop_peer", "emit", "caught"]);
        }
    }

    #[test]
    fn failed_public_observations_suppress_quit_attribution() {
        let (lines, events) = failed_wait(false, 2, false, true);
        assert_eq!(lines, Vec::<String>::new());
        assert_eq!(events, ["wait", "caught"]);
    }

    #[test]
    fn failure_before_first_callback_retains_the_initial_phase() {
        for retirement in [false, true] {
            let (lines, _) = failed_wait(retirement, 2, true, false);
            assert_eq!(
                lines,
                ["KELD_KEL140_NATIVE_QUIT_PHASE last_live_http_stage=no_http_quit_observed"]
            );
        }
    }

    #[test]
    fn successful_waits_preserve_output_and_emit_nothing() {
        for retirement in [false, true] {
            let progress = AtomicU8::new(2);
            let events = RefCell::new(Vec::new());
            let mut lines = Vec::new();
            let wait = Wait::new(
                &progress,
                |observe: &mut dyn FnMut()| {
                    events.borrow_mut().push("wait");
                    observe();
                    Output {
                        status: ExitStatus::from_raw(17 << 8),
                        stdout: b"retained stdout".to_vec(),
                        stderr: b"retained stderr".to_vec(),
                    }
                },
                |line: &str| lines.push(line.to_owned()),
            );
            let output = if retirement {
                let peer = Peer(&events);
                wait.retirement(|| drop(peer))
            } else {
                wait.public(true)
            };
            assert_eq!(output.status.code(), Some(17));
            assert_eq!(output.stdout, b"retained stdout");
            assert_eq!(output.stderr, b"retained stderr");
            assert_eq!(lines, Vec::<String>::new());
            let expected = if retirement {
                vec!["wait", "drop_peer"]
            } else {
                vec!["wait"]
            };
            assert_eq!(events.into_inner(), expected);
        }
    }
}
