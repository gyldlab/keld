//! Managed application-loop Close-result refusal and durable profile credit.
//! Synthetic persistent identity, not signed Host/Bun admission or Job acceptance.

use super::profile_release::raw_phase;
use super::{
    FIXTURE_DEADLINE, failure, profile_fixture_engine, profile_fixture_root, run_with_watchdog,
};
use crate::profile::{ProfileIdentity, WebProfileSelection};
use crate::webview2 as backend;
use crate::webview2::media_acceptance::adopted_close_control;
use crate::{AppWindowCommand, AppWindowEvent, BuildStage, WebviewId, WvError};
use std::cell::Cell;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Instant;

#[path = "adopted_close_native_support.rs"]
mod native_support;

#[derive(Debug, Default)]
struct DriverTrace {
    ready: u32,
    original_refused: u32,
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

fn create_command(build: u64, original_create_start_by: Option<Instant>) -> AppWindowCommand {
    AppWindowCommand::CreateWindow {
        build,
        width: 640,
        height: 480,
        x: None,
        y: None,
        original_source: None,
        preload_image: None,
        original_role: None,
        original_application_name_for_user_agent: None,
        original_create_start_by,
    }
}

fn drive(
    events: &Receiver<AppWindowEvent>,
    commands: &Sender<AppWindowCommand>,
    deadline: Instant,
) -> Result<DriverTrace, WvError> {
    let mut guard = DriverGuard {
        commands,
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
                    // An original-Create deadline without its selected inputs
                    // cannot fall through to an ordinary native window.
                    commands
                        .send(create_command(2, Some(deadline)))
                        .map_err(failure)?;
                }
            }
            AppWindowEvent::WindowBuildFailed {
                build: 2,
                stage: BuildStage::Webview,
            } if trace.original_refused == 0 && trace.built == 0 => {
                trace.original_refused += 1;
                commands.send(create_command(1, None)).map_err(failure)?;
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
    eprintln!("KELD_ADOPTED_CLOSE_DIAG phase=driver-eof trace={trace:?}");
    guard.completed = true;
    Ok(trace)
}

struct AppRun {
    outcome: Result<(), WvError>,
    trace: Result<DriverTrace, WvError>,
    witness_calls: u32,
}

fn run_managed(mut engine: backend::WebView2Engine, deadline: Instant) -> AppRun {
    let witness_calls = Cell::new(0_u32);
    let (commands, received) = mpsc::channel();
    let (events, received_events) = mpsc::channel();
    let (outcome, trace) = std::thread::scope(|threads| {
        let driver_commands = commands.clone();
        let driver = threads.spawn(move || drive(&received_events, &driver_commands, deadline));
        eprintln!("KELD_ADOPTED_CLOSE_DIAG phase=app-enter");
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
        eprintln!("KELD_ADOPTED_CLOSE_DIAG phase=app-return outcome={outcome:?}");
        drop(commands);
        eprintln!("KELD_ADOPTED_CLOSE_DIAG phase=driver-join-enter");
        let trace = driver
            .join()
            .map_err(|_| failure("managed Close driver panicked"))
            .and_then(std::convert::identity);
        eprintln!("KELD_ADOPTED_CLOSE_DIAG phase=driver-joined trace={trace:?}");
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
        || trace.original_refused != 1
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

fn expected_close_fault(outcome: &Result<(), WvError>) -> bool {
    outcome.as_ref().err().is_some_and(|error| {
        let message = error.to_string();
        message.contains("KELD-WV-009") && message.contains("returned an error from Close")
    })
}

fn require_product_verdict(
    withhold: bool,
    final_phase: &str,
    witness_calls: u32,
    outcome: &Result<(), WvError>,
) -> Result<(), WvError> {
    if withhold {
        if final_phase == "stopping" && witness_calls == 0 && expected_close_fault(outcome) {
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
    // Metadata only: keep the original callback until raw native cleanup ends.
    witness.prepare_after_app(expected_close_fault(&outcome), witness_calls);
    let facts = adopted_close_control::Scope::take_facts();
    let capture = facts.capture.as_ref().ok_or_else(|| {
        failure(format!(
            "native Close capture missing: {:?}",
            facts.capture_error
        ))
    })?;
    eprintln!(
        "KELD_ADOPTED_CLOSE_DIAG phase=cleanup-enter pid={}",
        capture.browser_pid
    );
    let cleanup = native_support::cleanup(capture, Some(&mut witness), deadline);
    eprintln!("KELD_ADOPTED_CLOSE_DIAG phase=cleanup-return result={cleanup:?}");
    let retired_observer = witness.retire(capture.browser_pid);
    let final_browser_exit = adopted_close_control::Scope::browser_exit_facts();
    let late = adopted_close_control::Scope::take_facts();
    drop(control);
    let already_retired = cleanup?;
    let retired_browser_exit = retired_observer?;
    if final_browser_exit != Some(retired_browser_exit) {
        return Err(failure(
            "raw native exit evidence changed during environment cleanup",
        ));
    }
    println!("KELD_ADOPTED_CLOSE_NATIVE_EXIT {retired_browser_exit:?}");
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

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn partial_build_case(withhold: bool) -> Result<(), WvError> {
    let root = profile_fixture_root("candidate-close");
    std::fs::create_dir_all(&root).map_err(failure)?;
    let deadline = Instant::now() + FIXTURE_DEADLINE;
    let _com = backend::initialize_com_sta()?;
    let control = adopted_close_control::Scope::enter(withhold);
    let identity = ProfileIdentity::from_host_verified_parts([98; 32], "dev.keld.candidate-close")?;
    let selection = WebProfileSelection::Persistent(identity);
    let plan = backend::windows_profile_plan(&root, selection)?;
    let mut engine = profile_fixture_engine(&root, selection)?;
    let proxy = engine.creation_event_loop()?.create_proxy();
    engine.managed_test_control =
        Some(backend::managed_test_control::TestControl::for_failed_view_fixture(proxy)?);
    let mut exit_witness = native_support::ExitWitness::new(&engine)?;
    let (commands, received) = mpsc::channel();
    let (events, driver_events) = mpsc::channel();
    let witness_calls = Cell::new(0);
    let (unretired_run, driver) = std::thread::scope(|scope| {
        let driver_commands = commands.clone();
        let driver = scope.spawn(move || -> Result<(), WvError> {
            let mut guard = DriverGuard {
                commands: &driver_commands,
                completed: false,
            };
            let mut sent = false;
            loop {
                let observed = driver_events
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .map_err(failure)?;
                match observed {
                    AppWindowEvent::HostInitialized { .. } if !sent => {
                        driver_commands
                            .send(create_command(1, None))
                            .map_err(failure)?;
                        sent = true;
                        if withhold {
                            break;
                        }
                    }
                    AppWindowEvent::WindowBuildFailed {
                        build: 1,
                        stage: BuildStage::Webview,
                    } if sent && !withhold => {
                        driver_commands
                            .send(AppWindowCommand::Quit)
                            .map_err(failure)?;
                        break;
                    }
                    AppWindowEvent::WindowBuilt { .. }
                    | AppWindowEvent::WindowBuildFailed { .. } => {
                        return Err(failure(
                            "fixture did not fail at the retained View build point",
                        ));
                    }
                    _ => {}
                }
            }
            guard.completed = true;
            Ok(())
        });
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
        (
            outcome,
            driver
                .join()
                .map_err(|_| failure("candidate driver panicked")),
        )
    });
    let retained = engine.retains_failed_build();
    let phase = raw_phase(&plan);
    let facts = adopted_close_control::Scope::take_facts();
    let capture = facts.capture.as_ref().ok_or_else(|| {
        failure(format!(
            "retained candidate Close capture unavailable: {:?}",
            facts.capture_error
        ))
    })?;
    // First sample the original HWND with the engine still owned by this
    // caller. The recorded native Close succeeded even in the withheld cell.
    let parent_live = native_support::is_window(capture.hwnd);
    exit_witness.prepare_after_app(expected_close_fault(&unretired_run), witness_calls.get());
    println!(
        "KELD_NATIVE_CANDIDATE_EXIT withhold={withhold} retained={retained} parent_live={parent_live} phase={phase:?} calls={} observed_success={} injected={} witnesses={} result={unretired_run:?}",
        facts.calls,
        facts.native_successes,
        facts.injected_errors,
        witness_calls.get(),
    );

    // Fixture cleanup follows the product verdict. The independent observation
    // recorded actual successful native Close, which permits ending test custody.
    drop(engine);
    native_support::cleanup(capture, Some(&mut exit_witness), deadline)?;
    exit_witness.retire(capture.browser_pid)?;
    let late = adopted_close_control::Scope::take_facts();
    drop(control);
    std::fs::remove_dir_all(&root).map_err(failure)?;

    driver??;
    if facts.calls != 1
        || facts.native_successes != 1
        || facts.injected_errors != u32::from(withhold)
        || late.calls != 0
    {
        return Err(failure(
            "candidate must observe one native Close and no retry",
        ));
    }
    if retained != withhold || parent_live != withhold {
        return Err(failure(
            "terminal return lost the original unproved native candidate",
        ));
    }
    require_product_verdict(withhold, &phase?, witness_calls.get(), &unretired_run)
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[test]
#[ignore = "actual managed construction candidate and one native Close on Windows"]
fn windows_failed_candidate_close_retains_owner_on_bounded_terminal_exit() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        partial_build_case(true)?;
        partial_build_case(false)
    })
}

fn pre_loop_build_case(withhold: bool) -> Result<(), WvError> {
    let root = profile_fixture_root("candidate-pre-loop-close");
    std::fs::create_dir_all(&root).map_err(failure)?;
    let deadline = Instant::now() + FIXTURE_DEADLINE;
    let _com = backend::initialize_com_sta()?;
    let control = adopted_close_control::Scope::enter_pre_loop(withhold);
    let identity =
        ProfileIdentity::from_host_verified_parts([99; 32], "dev.keld.candidate-pre-loop-close")?;
    let selection = WebProfileSelection::Persistent(identity);
    let plan = backend::windows_profile_plan(&root, selection)?;
    let mut engine = profile_fixture_engine(&root, selection)?;
    let (events, _observed) = mpsc::channel();
    let spec = crate::engine::WebviewSpec {
        initial: crate::engine::NavTarget::Html(String::from("refuse\0initial")),
        ..crate::engine::WebviewSpec::default()
    };
    let outcome = engine.create_app(&spec, events);
    let retained = engine.retains_failed_build();
    let rejected_loop = withhold.then(|| engine.run_until_closed());
    let phase = raw_phase(&plan);
    let facts = adopted_close_control::Scope::take_facts();
    let capture = facts.capture.as_ref().ok_or_else(|| {
        failure(format!(
            "pre-loop Close capture unavailable: {:?}",
            facts.capture_error
        ))
    })?;
    let parent_live = native_support::is_window(capture.hwnd);
    println!(
        "KELD_NATIVE_CANDIDATE_PRELOOP withhold={withhold} retained={retained} parent_live={parent_live} phase={phase:?} calls={} observed_success={} injected={} result={outcome:?} loop={rejected_loop:?}",
        facts.calls, facts.native_successes, facts.injected_errors,
    );

    // The test can drop this engine after independently observing successful
    // native Close. This pre-loop path never installed a managed browser binding.
    drop(engine);
    native_support::cleanup(capture, None, deadline)?;
    let late = adopted_close_control::Scope::take_facts();
    drop(control);
    std::fs::remove_dir_all(&root).map_err(failure)?;

    if facts.calls != 1
        || facts.native_successes != 1
        || facts.injected_errors != u32::from(withhold)
        || late.calls != 0
        || retained != withhold
        || (withhold && !parent_live)
        || phase? != "running"
        || !matches!(outcome, Err(WvError::Navigate(_)))
        || (withhold && !matches!(rejected_loop, Some(Err(WvError::ProfileSelection(_)))))
    {
        return Err(failure(
            "pre-loop return lost the native candidate or the original failure",
        ));
    }
    Ok(())
}

#[test]
#[ignore = "actual pre-loop Windows Create failure and one native Close on the creating STA"]
fn windows_failed_pre_loop_create_keeps_the_actual_candidate() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        pre_loop_build_case(true)?;
        pre_loop_build_case(false)
    })
}
