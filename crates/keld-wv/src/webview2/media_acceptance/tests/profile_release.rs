//! Real `WebView2` profile release controls, with synthetic identity, not signed-app admission.

use super::{
    FIXTURE_DEADLINE, WebView2Engine, failure, profile_fixture_engine, profile_fixture_root,
    run_with_watchdog,
};
use crate::WvError;
use crate::profile::{ProfileIdentity, WebProfileSelection};
use crate::webview2 as backend;
use std::cell::{Cell, RefCell};
use std::sync::{atomic::Ordering, mpsc};
use std::time::Instant;

fn raw_phase(plan: &backend::WindowsProfilePlan) -> Result<String, WvError> {
    let bytes = std::fs::read(plan.control_dir.join("profile.lifecycle.v1")).map_err(failure)?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(failure)?;
    json.get("phase")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| failure("durable record has no independent phase field"))
}

#[test]
#[ignore = "real Windows zero-controller persistent profile state"]
fn windows_zero_controller_constructor_is_running() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let root = profile_fixture_root("zero-controller-running");
        std::fs::create_dir_all(&root).map_err(failure)?;
        println!("KELD_T2_PROFILE_ROOT {}", root.display());
        let identity =
            ProfileIdentity::from_host_verified_parts([83; 32], "dev.keld.t2-running-control")?;
        let selection = WebProfileSelection::Persistent(identity);
        let plan = backend::windows_profile_plan(&root, selection)?;
        let result = (|| {
            let engine = profile_fixture_engine(&root, selection)?;
            let phase = raw_phase(&plan)?;
            let controllers = engine.views.len();
            println!("KELD_T2_ZERO_CONTROLLER phase={phase} controllers={controllers}");
            drop(engine);
            if phase != "running" || controllers != 0 {
                return Err(failure(format!(
                    "expected durable running before any controller; observed phase={phase} controllers={controllers}"
                )));
            }
            Ok(())
        })();
        std::fs::remove_dir_all(&root).map_err(failure)?;
        result
    })
}

fn quit_without_controller(
    engine: WebView2Engine,
    witness: impl FnOnce(Instant) -> Result<(), WvError>,
) -> Result<(), WvError> {
    let (commands, received) = mpsc::channel();
    let (events, received_events) = mpsc::channel();
    commands
        .send(crate::AppWindowCommand::Quit)
        .map_err(failure)?;
    let result = engine.run_app_until_quit(
        received,
        events,
        backend::AppReadyTrigger::HostInitialized,
        || {},
        witness,
    );
    let ready_count = received_events
        .try_iter()
        .filter(|event| matches!(event, crate::AppWindowEvent::HostInitialized { .. }))
        .count();
    if ready_count != 1 {
        return Err(failure(format!(
            "zero-controller session published {ready_count} HostInitialized events"
        )));
    }
    result
}

#[test]
#[ignore = "real Windows zero-controller persistent release witness"]
fn windows_zero_controller_quit_gates_idle_on_the_release_witness() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        for accept in [false, true] {
            let root = profile_fixture_root("zero-controller-witness");
            std::fs::create_dir_all(&root).map_err(failure)?;
            println!("KELD_T2_PROFILE_ROOT {}", root.display());
            let identity =
                ProfileIdentity::from_host_verified_parts([84; 32], "dev.keld.t2-witness-control")?;
            let selection = WebProfileSelection::Persistent(identity);
            let plan = backend::windows_profile_plan(&root, selection)?;
            let engine = profile_fixture_engine(&root, selection)?;
            let initial_phase = raw_phase(&plan)?;
            let controller_count = engine.views.len();
            let calls = Cell::new(0_u32);
            let phase_at_witness = RefCell::new(None);
            let result = quit_without_controller(engine, |deadline| {
                calls.set(calls.get() + 1);
                *phase_at_witness.borrow_mut() = Some(raw_phase(&plan)?);
                if Instant::now() >= deadline {
                    return Err(failure("witness received an already-expired grace"));
                }
                if accept {
                    Ok(())
                } else {
                    Err(failure("injected non-family release refusal"))
                }
            });
            let final_phase = raw_phase(&plan)?;
            let observed_phase = phase_at_witness.into_inner();
            println!(
                "KELD_T2_PERSISTENT accept={accept} before={initial_phase} witness={observed_phase:?} after={final_phase} calls={} controllers={controller_count} result={result:?}",
                calls.get()
            );
            std::fs::remove_dir_all(&root).map_err(failure)?;
            if initial_phase != "running"
                || controller_count != 0
                || calls.get() != 1
                || observed_phase.as_deref() != Some("stopping")
                || final_phase != if accept { "idle" } else { "stopping" }
                || result.is_ok() != accept
            {
                return Err(failure(
                    "zero-controller durable release order disagrees with witness outcome",
                ));
            }
            if let Err(error) = result
                && !error.to_string().contains("KELD-WV-009")
            {
                return Err(failure(format!(
                    "witness refusal lost the profile failure code: {error}"
                )));
            }
        }
        Ok(())
    })
}

#[test]
#[ignore = "real Windows zero-controller ephemeral deletion witness"]
fn windows_ephemeral_quit_keeps_the_directory_when_release_is_unproved() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        for accept in [false, true] {
            let root = profile_fixture_root("ephemeral-witness");
            std::fs::create_dir_all(&root).map_err(failure)?;
            println!("KELD_T2_PROFILE_ROOT {}", root.display());
            let profile = crate::profile::EphemeralProfile::from_host_random([85; 32])?;
            let selection = WebProfileSelection::ephemeral_dev(profile);
            let plan = backend::windows_profile_plan(&root, selection)?;
            let engine = profile_fixture_engine(&root, selection)?;
            let calls = Cell::new(0_u32);
            let result = quit_without_controller(engine, |_| {
                calls.set(calls.get() + 1);
                if accept {
                    Ok(())
                } else {
                    Err(failure("injected non-family release refusal"))
                }
            });
            let retained = plan.control_dir.exists() && plan.user_data_dir.exists();
            println!(
                "KELD_T2_EPHEMERAL accept={accept} retained={retained} calls={} result={result:?}",
                calls.get()
            );
            std::fs::remove_dir_all(&root).map_err(failure)?;
            if calls.get() != 1 || retained == accept || result.is_ok() != accept {
                return Err(failure(
                    "ephemeral deletion did not follow the release witness",
                ));
            }
        }
        Ok(())
    })
}

#[test]
#[ignore = "real Windows environment with owner-level missing-receipt negative control"]
fn windows_release_witness_does_not_replace_the_browser_receipt() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let root = profile_fixture_root("receipt-before-witness");
        std::fs::create_dir_all(&root).map_err(failure)?;
        println!("KELD_T2_PROFILE_ROOT {}", root.display());
        let identity =
            ProfileIdentity::from_host_verified_parts([86; 32], "dev.keld.t2-receipt-control")?;
        let selection = WebProfileSelection::Persistent(identity);
        let plan = backend::windows_profile_plan(&root, selection)?;
        let mut engine = profile_fixture_engine(&root, selection)?;
        // Owner-level rule control, not a claim of an actual browser process:
        // a nonzero binding requires its normal receipt before any witness call.
        engine.expected_browser_pid.store(1, Ordering::Release);
        let called = Cell::new(false);
        let result = engine.finish_profile_release_with(false, || {
            called.set(true);
            Ok(())
        });
        let phase = raw_phase(&plan)?;
        drop(engine);
        std::fs::remove_dir_all(&root).map_err(failure)?;
        if result.is_ok() || called.get() || phase != "running" {
            return Err(failure(
                "a census witness replaced the required browser receipt",
            ));
        }
        Ok(())
    })
}
