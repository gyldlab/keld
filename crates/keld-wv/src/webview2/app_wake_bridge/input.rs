//! Original receivers plus one current delivery, never an added payload queue.
#![forbid(unsafe_code)]

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::super::managed_test_control::outcome_hold::{EventKey, OutcomeHold};
use super::post::{PostState, post_staged};
use super::{AppWakeDelivery, AppWindowCommand, Failure, POLL_INTERVAL, RendererBridgeOutcome};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};

#[expect(
    clippy::struct_excessive_bools,
    reason = "Receiver liveness, terminal receipt, scheduling preference and current-slot entry are independent facts"
)]
pub(super) struct BridgeInput {
    commands: Receiver<AppWindowCommand>,
    outcomes: Option<Receiver<RendererBridgeOutcome>>,
    commands_open: bool,
    outcomes_open: bool,
    terminal_seen: bool,
    prefer_outcome: bool,
    current: Option<AppWakeDelivery>,
    attempted: bool,
    failure: Option<Failure>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    test_outcome_hold: Option<OutcomeHold>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    test_outcomes_held: bool,
}

enum Step {
    Post(Option<&'static str>),
    Wait,
    Finished,
}

impl BridgeInput {
    pub(super) fn new(
        commands: Receiver<AppWindowCommand>,
        outcomes: Option<Receiver<RendererBridgeOutcome>>,
    ) -> Self {
        Self {
            commands,
            outcomes_open: outcomes.is_some(),
            outcomes,
            commands_open: true,
            terminal_seen: false,
            prefer_outcome: false,
            current: None,
            attempted: false,
            failure: None,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_outcome_hold: None,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_outcomes_held: false,
        }
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn install_outcome_hold(&mut self, hold: OutcomeHold) -> bool {
        if self.test_outcome_hold.is_some() {
            return false;
        }
        self.test_outcome_hold = Some(hold);
        true
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) const fn test_outcomes_held(&self) -> bool {
        self.test_outcomes_held
    }

    #[cfg(test)]
    pub(super) fn has_current(&self) -> bool {
        self.current.is_some()
    }
    pub(super) fn failure(&self) -> Option<Failure> {
        self.failure
    }
    pub(super) fn take_current(&mut self) -> Option<AppWakeDelivery> {
        let current = self.current.take();
        if current.is_some() {
            self.attempted = false;
        }
        current
    }

    /// Additional queue/storage check after the original worker has returned
    /// and Core's actual producer/protocol owners proved their own drainage.
    /// Only payload-free wake hints may be disposed here. Every other delivery
    /// is attached to this same current slot and refuses release.
    pub(super) fn check_drained(&mut self) -> Result<(), crate::WvError> {
        loop {
            if let Some(delivery) = self.current.as_ref() {
                let hint = matches!(
                    delivery,
                    AppWakeDelivery::Command(
                        Some(AppWindowCommand::DialogWake | AppWindowCommand::HostWriterProgress)
                            | None
                    ) | AppWakeDelivery::Renderer(
                        None | Some(RendererBridgeOutcome::ControlCapacity)
                    )
                );
                if !hint {
                    return Err(super::error(
                        "application wake retains an actual queued delivery",
                    ));
                }
                let _ = self.current.take();
            }
            // No producer proof comes from this observation. The enclosing
            // actual shutdown cut already closed offers and drained records.
            if !self.receive_command() && !self.receive_outcome() {
                return Ok(());
            }
        }
    }

    #[expect(
        clippy::if_same_then_else,
        reason = "The short-circuit receive order alternates two real receivers and must remain observable"
    )]
    fn next(&mut self, terminal: &AtomicBool) -> Result<Step, Failure> {
        if let Some(fault) = self.failure {
            return Err(fault);
        }
        if self.current.is_some() {
            return Ok(if self.attempted {
                Step::Wait
            } else {
                Step::Post(self.command_name())
            });
        }
        // A single receive under this short guard attaches its actual result
        // immediately. No next receive can occur until the UI takes current.
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let allow_outcome = !self.test_outcomes_held;
        #[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
        let allow_outcome = true;
        // Gate only normal intake. check_drained still inspects the original
        // receiver and cannot treat a fixture hold as an empty queue.
        let received = if self.prefer_outcome {
            (allow_outcome && self.receive_outcome()) || self.receive_command()
        } else {
            self.receive_command() || (allow_outcome && self.receive_outcome())
        };
        if received {
            self.prefer_outcome = matches!(self.current, Some(AppWakeDelivery::Command(_)));
            if matches!(
                self.current,
                Some(AppWakeDelivery::Command(Some(
                    AppWindowCommand::Quit | AppWindowCommand::Fatal
                )))
            ) {
                self.terminal_seen = true;
                terminal.store(true, Ordering::Release);
            }
            return Ok(Step::Post(self.command_name()));
        }
        Ok(if !self.commands_open && !self.outcomes_open {
            Step::Finished
        } else {
            Step::Wait
        })
    }

    fn receive_command(&mut self) -> bool {
        if !self.commands_open {
            return false;
        }
        match self.commands.try_recv() {
            Ok(command) => {
                self.current = Some(AppWakeDelivery::Command(Some(command)));
                true
            }
            Err(TryRecvError::Disconnected) => {
                self.commands_open = false;
                if self.terminal_seen {
                    false
                } else {
                    self.current = Some(AppWakeDelivery::Command(Some(AppWindowCommand::Fatal)));
                    true
                }
            }
            Err(TryRecvError::Empty) => false,
        }
    }
    fn receive_outcome(&mut self) -> bool {
        if !self.outcomes_open {
            return false;
        }
        let Some(outcomes) = &self.outcomes else {
            return false;
        };
        match outcomes.try_recv() {
            Ok(outcome) => {
                self.current = Some(AppWakeDelivery::Renderer(Some(outcome)));
                true
            }
            Err(TryRecvError::Disconnected) => {
                self.outcomes_open = false;
                false
            }
            Err(TryRecvError::Empty) => false,
        }
    }
    #[cfg_attr(
        not(all(feature = "profile-test-hooks", debug_assertions)),
        expect(
            clippy::unused_self,
            reason = "Debug observation reads this same retained current delivery only when hooks are enabled"
        )
    )]
    fn command_name(&self) -> Option<&'static str> {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(AppWakeDelivery::Command(Some(command))) = &self.current {
            return Some(super::super::t2_test_observation::bridge_command_name(
                command,
            ));
        }
        None
    }
}
impl PostState for BridgeInput {
    fn begin_post(&mut self) -> Result<bool, Failure> {
        if let Some(fault) = self.failure {
            return Err(fault);
        }
        if self.current.is_none() || self.attempted {
            return Ok(false);
        }
        self.attempted = true;
        Ok(true)
    }
    fn post_failed(&mut self, fault: Failure) {
        self.failure.get_or_insert(fault);
    }
}

pub(super) fn run(
    input: &Arc<Mutex<BridgeInput>>,
    stop: &AtomicBool,
    terminal: &AtomicBool,
    post: &mut impl FnMut() -> Result<(), ()>,
    posted: &mut impl FnMut(Option<&'static str>),
) -> Result<(), Failure> {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    let mut outcome_hold = input
        .lock()
        .map_err(|_| Failure::InputPoisoned)?
        .test_outcome_hold
        .take();
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        // Only fixed metadata is inspected here, outside the input/native/Core
        // guards. The existing worker park is the only wait or poll cadence.
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(hold) = &mut outcome_hold {
            hold.poll().map_err(|error| {
                eprintln!("KELD_X10_OUTCOME_HOLD phase=failed detail={error}");
                Failure::TestControlFailed
            })?;
        }
        let delivery_step = {
            let mut input = input.lock().map_err(|_| Failure::InputPoisoned)?;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if let Some(hold) = &mut outcome_hold {
                hold.before_turn(input.current.is_none());
                input.test_outcomes_held = hold.is_holding();
            }
            let next_action = input.next(terminal)?;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if let Some(hold) = &mut outcome_hold {
                let event = match &input.current {
                    Some(AppWakeDelivery::Renderer(Some(RendererBridgeOutcome::Event {
                        webview,
                        navigation,
                        request,
                        ..
                    }))) => Some(EventKey {
                        webview: webview.0,
                        navigation: *navigation,
                        delivery: *request,
                    }),
                    _ => None,
                };
                hold.selected_event(event);
                input.test_outcomes_held = hold.is_holding();
            }
            next_action
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(hold) = &mut outcome_hold {
            hold.observe();
        }
        match delivery_step {
            Step::Post(name) => {
                if stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                if post_staged(input, &mut *post)? {
                    posted(name);
                }
            }
            Step::Wait => {
                // No input/native/Core guard crosses the existing bounded poll.
                // The UI's actual take unparks this same worker immediately.
                std::thread::park_timeout(POLL_INTERVAL);
            }
            Step::Finished => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
