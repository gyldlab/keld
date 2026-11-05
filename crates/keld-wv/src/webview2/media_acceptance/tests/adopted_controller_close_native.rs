//! Managed application-loop Close-result refusal and durable profile credit.
//! Synthetic persistent identity, not signed Host/Bun admission or Job acceptance.

use super::profile_release::raw_phase;
use super::{
    FIXTURE_DEADLINE, failure, profile_fixture_engine, profile_fixture_root, run_with_watchdog,
};
use crate::profile::{ProfileIdentity, WebProfileSelection};
use crate::webview2 as backend;
use crate::webview2::media_acceptance::adopted_close_control;
use crate::{AppWindowCommand, AppWindowEvent, WebviewId, WvError};
use std::cell::Cell;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Instant;

#[path = "adopted_close_native_support.rs"]
mod native_support;

#[derive(Debug, Default)]
struct DriverTrace {
    ready: u32,
    built: u32,
    released: u32,
    view: Option<WebviewId>,
    error: Option<String>,
}

struct DriverGuard<'a> {
    commands: &'a Sender<AppWindowCommand>,
    completed: bool,
}
impl Drop for DriverGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.commands.send(AppWindowCommand::Fatal);
        }
    }
}

fn drive(
    events: Receiver<AppWindowEvent>,
    commands: Sender<AppWindowCommand>,
    deadline: Instant,
) -> Result<DriverTrace, WvError> {
    let mut guard = DriverGuard {
        commands: &commands,
        completed: false,
    };
    let mut trace = DriverTrace::default();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = match events.recv_timeout(remaining) {
            Ok(event) => event,
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                return Err(failure(
                    "managed Close driver deadline; not a semantic verdict",
                ));
            }
        };
        match event {
            AppWindowEvent::HostInitialized { .. } => {
                trace.ready += 1;
                if trace.ready == 1 {
                    commands
                        .send(AppWindowCommand::CreateWindow {
                            build: 1,
                            width: 640,
                            height: 480,
                            x: None,
                            y: None,
                        })
                        .map_err(failure)?;
                }
            }
            AppWindowEvent::WindowBuilt { build, view, .. } => {
                trace.built += 1;
                if trace.view.replace(view).is_some() || build != 1 {
                    trace.error = Some(String::from("unexpected managed build identity"));
                }
                commands
                    .send(AppWindowCommand::DestroyWindow { view })
                    .map_err(failure)?;
                // The same existing bridge preserves command FIFO. Destroy parks
                // native custody before Quit asks the same release owner to end.
                commands.send(AppWindowCommand::Quit).map_err(failure)?;
            }
            AppWindowEvent::WindowReleased { view } => {
                trace.released += 1;
                if trace.view != Some(view) {
                    trace.error = Some(String::from("release did not match the built view"));
                }
            }
            AppWindowEvent::WindowBuildFailed { .. }
            | AppWindowEvent::WindowCloseRequested { .. } => {
                trace.error = Some(format!("unexpected native fixture event {event:?}"));
                commands.send(AppWindowCommand::Quit).map_err(failure)?;
            }
            _ => {}
        }
    }
    guard.completed = true;
    Ok(trace)
}

struct AppRun {
    outcome: Result<(), WvError>,
    trace: Result<DriverTrace, WvError>,
    witness_calls: u32,
}

fn run_managed(engine: backend::WebView2Engine, deadline: Instant) -> AppRun {
    let witness_calls = Cell::new(0_u32);
    let (commands, received) = mpsc::channel();
    let (events, received_events) = mpsc::channel();
    let (outcome, trace) = std::thread::scope(|threads| {
        let driver_commands = commands.clone();
        let driver = threads.spawn(move || drive(received_events, driver_commands, deadline));
        let outcome = engine.run_app_until_quit(
            received,
            events,
            backend::AppReadyTrigger::HostInitialized,
            || {},
            |_| {
                witness_calls.set(witness_calls.get() + 1);
                Ok::<(), WvError>(())
            },
        );
        drop(commands);
        let trace = driver
            .join()
            .map_err(|_| failure("managed Close driver panicked"))
            .and_then(std::convert::identity);
        (outcome, trace)
    });
    AppRun {
        outcome,
        trace,
        witness_calls: witness_calls.get(),
    }
}

fn require_native_eligibility(
    withhold: bool,
    initial_windows: usize,
    initial_phase: &str,
    trace: &DriverTrace,
    facts: &adopted_close_control::Facts,
    late: &adopted_close_control::Facts,
    already_retired: bool,
) -> Result<(), WvError> {
    // All prerequisites are independent of the disputed fault-latch assertion.
    // A missing callback/control/normal receipt is not a valid negative control.
    if !already_retired
        || initial_windows != 0
        || initial_phase != "running"
        || trace.ready != 1
        || trace.built != 1
        || trace.released != 1
        || trace.error.is_some()
        || facts.calls != 1
        || facts.native_successes != 1
        || facts.results != [0]
        || facts.injected_errors != u32::from(withhold)
        || facts.capture_error.is_some()
        || late.calls != 0
        || !late.results.is_empty()
    {
        return Err(failure(
            "managed Close control eligibility/retirement failed; no semantic credit",
        ));
    }
    Ok(())
}

fn require_product_verdict(
    withhold: bool,
    final_phase: &str,
    witness_calls: u32,
    outcome: &Result<(), WvError>,
) -> Result<(), WvError> {
    if withhold {
        let message = outcome
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default();
        if final_phase == "stopping"
            && witness_calls == 0
            && message.contains("KELD-WV-009")
            && message.contains("returned an error from Close")
        {
            Ok(())
        } else {
            Err(failure(
                "a managed Close error received normal shutdown or durable Idle credit",
            ))
        }
    } else if outcome.is_ok() && final_phase == "idle" && witness_calls == 1 {
        Ok(())
    } else {
        Err(failure(
            "healthy managed Close did not preserve ordinary Idle release",
        ))
    }
}

// Outer error: setup/eligibility/cleanup was not proved. Inner error: frozen
// product verdict after native cleanup and owned profile-fixture removal.
fn case(withhold: bool) -> Result<Result<(), WvError>, WvError> {
    let root = profile_fixture_root("managed-adopted-close");
    std::fs::create_dir_all(&root).map_err(failure)?;
    println!(
        "KELD_ADOPTED_CLOSE_ROOT withhold={withhold} {}",
        root.display()
    );
    let deadline = Instant::now() + FIXTURE_DEADLINE;
    // The read-only native observer remains alive after the consumed engine.
    // Its COM references must remain within this extra balanced STA lifetime.
    let _com = backend::initialize_com_sta()?;
    let control = adopted_close_control::Scope::enter(withhold);
    let identity =
        ProfileIdentity::from_host_verified_parts([96; 32], "dev.keld.managed-close-result")?;
    let selection = WebProfileSelection::Persistent(identity);
    let plan = backend::windows_profile_plan(&root, selection)?;
    let engine = profile_fixture_engine(&root, selection)?;
    let initial_phase = raw_phase(&plan)?;
    let initial_windows = engine.views.len();
    let mut witness = native_support::ExitWitness::new(&engine)?;
    let AppRun {
        outcome,
        trace,
        witness_calls,
    } = run_managed(engine, deadline);
    // Capture the actual durable product result before any test cleanup.
    let final_phase = raw_phase(&plan);
    let facts = adopted_close_control::Scope::take_facts();
    let capture = facts.capture.as_ref().ok_or_else(|| {
        failure(format!(
            "native Close capture missing: {:?}",
            facts.capture_error
        ))
    })?;
    let cleanup = native_support::cleanup(capture, &mut witness, deadline);
    let retired_observer = witness.retire();
    let late = adopted_close_control::Scope::take_facts();
    drop(control);
    let already_retired = cleanup?;
    retired_observer?;
    let trace = trace?;
    let final_phase = final_phase?;
    println!(
        "KELD_ADOPTED_CLOSE_RESULT withhold={withhold} before={initial_phase} after={final_phase} calls={} native_successes={} injected={} results={:?} witnesses={witness_calls} trace={trace:?} result={outcome:?}",
        facts.calls, facts.native_successes, facts.injected_errors, facts.results,
    );
    require_native_eligibility(
        withhold,
        initial_windows,
        &initial_phase,
        &trace,
        &facts,
        &late,
        already_retired,
    )?;
    // Native normal exit plus the exact process handle and captured HWND tree
    // are proved before deleting this test-owned persistent fixture.
    drop(facts);
    std::fs::remove_dir_all(&root).map_err(failure)?;
    if root.exists() {
        return Err(failure("managed Close fixture root was not removed"));
    }
    println!("KELD_ADOPTED_CLOSE_PROFILE_CLEANED {}", root.display());
    Ok(require_product_verdict(
        withhold,
        &final_phase,
        witness_calls,
        &outcome,
    ))
}

#[test]
#[ignore = "real managed Windows App loop and a failure of one successful native Close result"]
fn windows_managed_close_result_failure_refuses_idle_after_native_retirement() -> Result<(), WvError>
{
    run_with_watchdog(FIXTURE_DEADLINE, || {
        case(false)??;
        let verdict = case(true)?;
        // A prior semantic RED does not suppress the fresh healthy recovery.
        // Unproved cleanup is an outer error and prevents another native fixture.
        case(false)??;
        verdict
    })
}
