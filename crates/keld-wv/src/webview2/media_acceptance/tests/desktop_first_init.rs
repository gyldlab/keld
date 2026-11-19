//! Real pre-window Windows observation and same-host current-reader resampling.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};
use std::time::Instant;

use super::{
    FIXTURE_DEADLINE, failure, profile_fixture_engine, profile_fixture_root, run_with_watchdog,
};
use crate::profile::{ProfileIdentity, WebProfileSelection};
use crate::webview2::AppReadyTrigger;
use crate::{AppDesktopCapture, AppDesktopFacts, AppWindowCommand, AppWindowEvent, WvError};

fn receive(
    events: &Receiver<AppWindowEvent>,
    deadline: Instant,
) -> Result<AppWindowEvent, WvError> {
    events
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(failure)
}

fn bind_current(
    sequence: u32,
    commands: &Sender<AppWindowCommand>,
    events: &Receiver<AppWindowEvent>,
    deadline: Instant,
) -> Result<(Arc<AtomicBool>, AppDesktopFacts), WvError> {
    let reader = Arc::new(AtomicBool::new(false));
    commands
        .send(AppWindowCommand::WindowsDesktopProducer {
            capture: AppDesktopCapture::from_current(sequence, Arc::clone(&reader)),
        })
        .map_err(failure)?;
    match receive(events, deadline)? {
        AppWindowEvent::WindowsDesktopObserved {
            capture,
            facts: Some(facts),
        } if capture.matches(sequence, &reader) => Ok((reader, facts)),
        other => Err(failure(format!(
            "missing matching actual reader observation: {other:?}"
        ))),
    }
}

fn observe_without_app_window(
    commands: &Sender<AppWindowCommand>,
    events: &Receiver<AppWindowEvent>,
    deadline: Instant,
) -> Result<(), WvError> {
    let AppWindowEvent::DesktopObserved {
        facts: Some(initial),
    } = receive(events, deadline)?
    else {
        return Err(failure(
            "first native observation unavailable or not before HostInitialized",
        ));
    };
    if !matches!(
        receive(events, deadline)?,
        AppWindowEvent::HostInitialized { .. }
    ) {
        return Err(failure(
            "the genuine Init milestone did not follow the initial observation",
        ));
    }
    if !matches!(initial.displays.as_slice(), [display] if display.id == initial.primary_id)
        || initial.primary_id == 0
        || initial.should_use_dark_colors
    {
        return Err(failure(
            "fixture did not observe the approved single light primary target",
        ));
    }
    let (old_reader, first) = bind_current(7, commands, events, deadline)?;
    old_reader.store(true, Ordering::Release);
    let (current_reader, replacement) = bind_current(8, commands, events, deadline)?;
    if first.primary_id != initial.primary_id || replacement.primary_id != initial.primary_id {
        return Err(failure(
            "stable physical target changed its Host-session identity during reader replacement",
        ));
    }
    current_reader.store(true, Ordering::Release);
    println!(
        "KELD_NATIVE_DESKTOP_PREWINDOW primary={} displays={} light=true observed_reader_replacements=2",
        initial.primary_id,
        initial.displays.len()
    );
    Ok(())
}

#[test]
#[ignore = "real selected Windows desktop sampling on a single light PMv2 display"]
fn windows_selected_desktop_precedes_init_and_resamples_current_reader() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let root = profile_fixture_root("selected-desktop-init");
        std::fs::create_dir_all(&root).map_err(failure)?;
        let identity =
            ProfileIdentity::from_host_verified_parts([102; 32], "dev.keld.desktop-init")?;
        let mut engine = profile_fixture_engine(&root, WebProfileSelection::Persistent(identity))?;
        engine.select_desktop_observation()?;
        let deadline = Instant::now() + FIXTURE_DEADLINE;
        let (commands, received) = mpsc::channel();
        let (events, captured) = mpsc::channel();
        let (native_result, driver_result) = std::thread::scope(|threads| {
            let commands = commands.clone();
            let driver = threads.spawn(move || {
                let outcome = observe_without_app_window(&commands, &captured, deadline);
                let terminal = if outcome.is_ok() {
                    AppWindowCommand::Quit
                } else {
                    AppWindowCommand::Fatal
                };
                outcome.and(commands.send(terminal).map_err(failure))
            });
            let result = engine.run_app_until_quit(
                received,
                events,
                AppReadyTrigger::HostInitialized,
                || {},
                |_| Ok::<(), WvError>(()),
            );
            (
                result,
                driver
                    .join()
                    .map_err(|_| failure("desktop fixture reader panicked")),
            )
        });
        let retained = engine.retains_failed_build();
        drop(engine);
        if !retained {
            std::fs::remove_dir_all(&root).map_err(failure)?;
        }
        driver_result??;
        native_result
    })
}
