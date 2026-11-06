//! Real persistent profile lifecycle plus labelled post-receipt fault injection.

use super::profile_release::raw_phase;
use super::{
    FIXTURE_DEADLINE, WebEngine, blank_spec, failure, profile_fixture_engine, profile_fixture_root,
    run_renderer_until_quit, run_with_watchdog,
};
use crate::profile::{ProfileIdentity, WebProfileSelection};
use crate::webview2::{
    media_acceptance::release_receipt_control::{Mode, Scope},
    windows_profile_plan,
};
use crate::{AppWindowCommand, WvError};
use std::sync::mpsc;

#[derive(Clone, Copy, Debug)]
enum ExitPath {
    AppQuit,
    AppFault,
    HelloFault,
}

/// Receive an actual native exit before a loop sees its queued injected fault.
fn precredit_native_exit(
    engine: &mut super::WebView2Engine,
    id: crate::WebviewId,
) -> Result<(), WvError> {
    use crate::webview2::{
        PROFILE_LAUNCH_DEADLINE, check_launch_deadline, milliseconds_until, pump_message_or_handle,
        remember_browser_process_id,
    };
    let _browser = remember_browser_process_id(&engine.views, &engine.browser_binding)?;
    engine.destroy(id)?;
    let deadline = std::time::Instant::now() + PROFILE_LAUNCH_DEADLINE;
    while Scope::observations().0 == 0 {
        check_launch_deadline(deadline)?;
        pump_message_or_handle(None, milliseconds_until(deadline)?)?;
    }
    Ok(())
}

fn actual_profile_exit_with_injected_receipt(mode: Mode, path: ExitPath) -> Result<(), WvError> {
    let root = profile_fixture_root("release-receipts");
    std::fs::create_dir_all(&root).map_err(failure)?;
    println!("KELD_RELEASE_RECEIPT_ROOT {}", root.display());
    let result = (|| {
        let _scope = Scope::enter(mode);
        let identity = ProfileIdentity::from_host_verified_parts(
            [91; 32],
            "dev.keld.release-receipt-control",
        )?;
        let selection = WebProfileSelection::Persistent(identity);
        let plan = windows_profile_plan(&root, selection)?;
        let mut engine = profile_fixture_engine(&root, selection)?;
        let id = engine.create(&blank_spec())?;
        let initial = raw_phase(&plan)?;
        if !matches!(path, ExitPath::AppQuit) {
            precredit_native_exit(&mut engine, id)?;
        }
        let (commands, receiver) = mpsc::channel();
        let (events, _keep_events_alive) = mpsc::channel();
        let outcome = match path {
            ExitPath::AppQuit => {
                commands.send(AppWindowCommand::Quit).map_err(failure)?;
                run_renderer_until_quit(engine, receiver, events)
            }
            // The live sender prevents a disconnected queue from supplying Stop.
            ExitPath::AppFault => run_renderer_until_quit(engine, receiver, events),
            ExitPath::HelloFault => engine.run_until_closed(),
        };
        let phase = raw_phase(&plan)?;
        let (native_receipts, faults_sent) = Scope::observations();
        println!(
            "KELD_RELEASE_RECEIPT_RESULT mode={mode:?} path={path:?} before={initial} after={phase} native={native_receipts} injected={faults_sent} outcome={outcome:?}"
        );
        if initial != "running" || native_receipts != 1 {
            return Err(failure(
                "the fixture did not observe one actual native browser exit from a Running profile",
            ));
        }
        if mode == Mode::Healthy {
            if outcome.is_err() || phase != "idle" || faults_sent != 0 {
                return Err(failure(
                    "healthy native release did not complete Idle without injected fault",
                ));
            }
        } else {
            let message = outcome
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_default();
            if faults_sent != 1
                || phase != "stopping"
                || !message.contains("KELD-WV-009")
                || !message.contains("the host browser process failed or exited abnormally")
            {
                return Err(failure(
                    "an injected queued failure after a real native normal receipt was hidden or committed Idle",
                ));
            }
        }
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root).map_err(failure);
    if let Err(error) = &cleanup {
        eprintln!(
            "KELD_RELEASE_RECEIPT_CLEANUP_ERROR {} {error}",
            root.display()
        );
    } else {
        println!("KELD_RELEASE_RECEIPT_CLEANED {}", root.display());
    }
    result.and(cleanup)
}

#[test]
#[ignore = "real Windows browser/profile exit and labelled same-PID post-receipt fault"]
fn windows_native_exit_credit_cannot_hide_a_queued_fault() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        actual_profile_exit_with_injected_receipt(Mode::Healthy, ExitPath::AppQuit)?;
        actual_profile_exit_with_injected_receipt(Mode::SamePidFault, ExitPath::AppQuit)?;
        actual_profile_exit_with_injected_receipt(Mode::SamePidFault, ExitPath::AppFault)?;
        actual_profile_exit_with_injected_receipt(Mode::SamePidFault, ExitPath::HelloFault)
    })
}

#[test]
#[ignore = "real Windows browser/profile exit and labelled foreign-PID abnormal environment event"]
fn windows_foreign_abnormal_receipt_cannot_be_discarded_as_stale() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        actual_profile_exit_with_injected_receipt(Mode::ForeignPidFault, ExitPath::AppQuit)
    })
}
