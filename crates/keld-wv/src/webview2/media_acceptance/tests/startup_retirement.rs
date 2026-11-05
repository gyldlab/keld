//! Real zero-controller engine preparation; synthetic profile identity, not signed admission.

use super::profile_release::{quit_without_controller, raw_phase};
use super::{
    FIXTURE_DEADLINE, failure, profile_fixture_engine, profile_fixture_root, run_with_watchdog,
};
use crate::WvError;
use crate::engine::WebEngine as _;
use crate::profile::{ProfileIdentity, WebProfileSelection};
use crate::webview2 as backend;
use crate::webview2::startup_retirement::NativeViewBuild;
use std::cell::{Cell, RefCell};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Healthy,
    RefusedWitness,
    QueuedFault,
}

fn verify_persistent(outcome: Outcome) -> Result<Result<(), WvError>, WvError> {
    let root = profile_fixture_root("unstarted-persistent");
    std::fs::create_dir_all(&root).map_err(failure)?;
    println!("KELD_T3_PROFILE_ROOT {}", root.display());
    let identity =
        ProfileIdentity::from_host_verified_parts([93; 32], "dev.keld.unstarted-retirement")?;
    let selection = WebProfileSelection::Persistent(identity);
    let plan = backend::windows_profile_plan(&root, selection)?;
    let mut engine = profile_fixture_engine(&root, selection)?;
    let initial = raw_phase(&plan)?;
    let zero = engine.views.is_empty()
        && engine.view_build_attempt == NativeViewBuild::NotAttempted
        && !engine.browser_binding.load().is_bound();
    if outcome == Outcome::QueuedFault {
        let receipt = backend::browser_instance::browser_exit_receipt(
            engine.browser_binding.load(),
            1,
            false,
            backend::ReleaseSite::Host,
        )
        .ok_or_else(|| failure("owner-level abnormal receipt was not admitted"))?;
        engine
            .browser_exit_sender
            .upgrade()
            .ok_or_else(|| failure("native environment observer has no receipt sender"))?
            .send(receipt)
            .map_err(failure)?;
    }
    let calls = Cell::new(0_u32);
    let at_witness = RefCell::new(None);
    let expired = Cell::new(false);
    let result = engine.retire_unstarted_app(|deadline| {
        calls.set(calls.get() + 1);
        *at_witness.borrow_mut() = Some(raw_phase(&plan)?);
        expired.set(Instant::now() >= deadline);
        if outcome == Outcome::RefusedWitness {
            Err(failure("injected startup Host-witness refusal"))
        } else {
            Ok(())
        }
    });
    let sealed = engine.event_loop.is_none();
    let final_phase = raw_phase(&plan)?;
    let witness_phase = at_witness.into_inner();
    println!(
        "KELD_T3_PREPARED_RETIRE outcome={outcome:?} before={initial} at_witness={witness_phase:?} after={final_phase} calls={} zero={zero} sealed={sealed} result={result:?}",
        calls.get()
    );
    drop(engine);
    let lease = backend::open_control_file(
        &plan.control_dir.join(backend::PROFILE_LEASE),
        backend::ControlFileMode::Lease,
    )
    .map(drop);
    let cleanup = std::fs::remove_dir_all(&root).map_err(failure);
    cleanup?;
    lease?;
    println!(
        "KELD_T3_PREPARED_CLEANUP outcome={outcome:?} lease_reacquired=true root_removed=true"
    );
    if !zero || initial != "running" || !sealed || expired.get() {
        return Err(failure(
            "invalid zero-controller preparation or original witness budget",
        ));
    }
    let healthy = outcome == Outcome::Healthy;
    let expected_calls = u32::from(outcome != Outcome::QueuedFault);
    if result.is_ok() != healthy
        || calls.get() != expected_calls
        || final_phase != if healthy { "idle" } else { "stopping" }
        || (expected_calls == 1 && witness_phase.as_deref() != Some("stopping"))
    {
        return Ok(Err(failure(
            "prepared-engine retirement bypassed its first fault or Host witness",
        )));
    }
    if let Err(error) = result
        && !error.to_string().contains("KELD-WV-009")
    {
        return Ok(Err(failure(format!(
            "startup retirement lost its typed refusal: {error}"
        ))));
    }
    Ok(Ok(()))
}

#[test]
#[ignore = "real Windows unrun engine/profile/callback retirement; synthetic identity"]
fn windows_unstarted_facade_retirement_keeps_failure_first_and_host_witness() -> Result<(), WvError>
{
    run_with_watchdog(FIXTURE_DEADLINE, || {
        // Each case cleans its owned fixture before returning a semantic verdict.
        // A semantic failure does not suppress the following fresh healthy case.
        let mut first = None;
        for outcome in [
            Outcome::Healthy,
            Outcome::RefusedWitness,
            Outcome::Healthy,
            Outcome::QueuedFault,
            Outcome::Healthy,
        ] {
            if let Err(error) = verify_persistent(outcome)? {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    })
}

#[test]
#[ignore = "real Windows unstarted ephemeral profile deletion witness"]
fn windows_unstarted_facade_retirement_keeps_an_unproved_ephemeral_profile() -> Result<(), WvError>
{
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let mut first = None;
        for accept in [false, true] {
            let root = profile_fixture_root("unstarted-ephemeral");
            std::fs::create_dir_all(&root).map_err(failure)?;
            let profile = crate::profile::EphemeralProfile::from_host_random([94; 32])?;
            let selection = WebProfileSelection::ephemeral_dev(profile);
            let plan = backend::windows_profile_plan(&root, selection)?;
            let mut engine = profile_fixture_engine(&root, selection)?;
            let calls = Cell::new(0_u32);
            let result = engine.retire_unstarted_app(|_| {
                calls.set(calls.get() + 1);
                if accept {
                    Ok(())
                } else {
                    Err(failure("injected startup Host-witness refusal"))
                }
            });
            let retained = plan.control_dir.exists() && plan.user_data_dir.exists();
            drop(engine);
            std::fs::remove_dir_all(&root).map_err(failure)?;
            println!(
                "KELD_T3_PREPARED_EPHEMERAL accept={accept} retained={retained} calls={} root_removed=true result={result:?}",
                calls.get()
            );
            if calls.get() != 1 || retained == accept || result.is_ok() != accept {
                first.get_or_insert_with(|| {
                    failure("unstarted ephemeral deletion bypassed the Host witness")
                });
            }
        }
        first.map_or(Ok(()), Err)
    })
}

#[test]
#[ignore = "real Windows prepared engine; attempted-build metadata is an owner-level refusal control"]
fn windows_attempted_build_refusal_preserves_engine_for_ordinary_retirement() -> Result<(), WvError>
{
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let root = profile_fixture_root("unstarted-attempt-refusal");
        std::fs::create_dir_all(&root).map_err(failure)?;
        let identity =
            ProfileIdentity::from_host_verified_parts([95; 32], "dev.keld.unstarted-refusal")?;
        let selection = WebProfileSelection::Persistent(identity);
        let plan = backend::windows_profile_plan(&root, selection)?;
        let mut engine = profile_fixture_engine(&root, selection)?;
        // This is a direct custody-fact refusal control, not an actual SDK failure.
        // All native resources are the real zero-controller fixture's resources.
        engine.view_build_attempt = NativeViewBuild::Attempted;
        let called = Cell::new(false);
        let refused = engine.retire_unstarted_app(|_| {
            called.set(true);
            Ok::<(), WvError>(())
        });
        let phase = raw_phase(&plan)?;
        let preserved = engine.event_loop.is_some()
            && engine.browser_exit.is_some()
            && engine.browser_exit_token.is_some()
            && engine.profile.is_some();
        let ordinary = if engine.event_loop.is_some() {
            quit_without_controller(engine, |_| Ok::<(), WvError>(()))
        } else {
            drop(engine);
            Ok(())
        };
        let final_phase = raw_phase(&plan)?;
        let lease = backend::open_control_file(
            &plan.control_dir.join(backend::PROFILE_LEASE),
            backend::ControlFileMode::Lease,
        )
        .map(drop);
        std::fs::remove_dir_all(&root).map_err(failure)?;
        lease?;
        println!(
            "KELD_T3_PREPARED_MISUSE preserved={preserved} before_cleanup={phase} after_cleanup={final_phase} witness_called={} cleanup={ordinary:?} root_removed=true refusal={refused:?}",
            called.get()
        );
        if refused.is_ok() || called.get() {
            return Err(failure(
                "an attempted build was admitted to zero-controller pre-run retirement",
            ));
        }
        ordinary?;
        if !preserved || phase != "running" || final_phase != "idle" {
            return Err(failure(
                "refused pre-run retirement consumed native custody or prevented ordinary cleanup",
            ));
        }
        Ok(())
    })
}

#[test]
#[ignore = "real Windows retired preparation; no native construction may restart its lifecycle"]
fn windows_retired_preparation_refuses_create_before_state_changes() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let root = profile_fixture_root("unstarted-sealed-create");
        std::fs::create_dir_all(&root).map_err(failure)?;
        let identity =
            ProfileIdentity::from_host_verified_parts([96; 32], "dev.keld.retired-preparation")?;
        let selection = WebProfileSelection::Persistent(identity);
        let plan = backend::windows_profile_plan(&root, selection)?;
        let mut engine = profile_fixture_engine(&root, selection)?;
        let retired = engine.retire_unstarted_app(|_| Ok::<(), WvError>(()));
        let before = raw_phase(&plan)?;
        let profile_released = engine.profile.is_none();
        // Local sentinels expose create_app's pre-call mutation, not native Ready.
        engine
            .navigation_ready
            .store(true, std::sync::atomic::Ordering::Release);
        engine
            .navigation_failed
            .store(true, std::sync::atomic::Ordering::Release);
        let plain = engine.create(&super::blank_spec());
        let app = engine.create_app(&super::blank_spec(), std::sync::mpsc::channel().0);
        let after = raw_phase(&plan)?;
        let unchanged = engine.view_build_attempt == NativeViewBuild::NotAttempted
            && engine
                .navigation_ready
                .load(std::sync::atomic::Ordering::Acquire)
            && engine
                .navigation_failed
                .load(std::sync::atomic::Ordering::Acquire);
        let refused = matches!((&plain, &app),
            (Err(WvError::EventLoop(first)), Err(WvError::EventLoop(second)))
            if first == "run loop already started; create webviews before run_until_closed"
                && first == second);
        println!(
            "KELD_T3_PREPARED_SEALED before={before} after={after} profile_released={profile_released} unchanged={unchanged} refused={refused} plain={plain:?} app={app:?}"
        );
        drop(engine);
        std::fs::remove_dir_all(&root).map_err(failure)?;
        retired?;
        if before != "idle" || after != "idle" || !profile_released || !unchanged || !refused {
            return Err(failure(
                "a sealed engine mutated creation or durable profile state",
            ));
        }
        Ok(())
    })
}
