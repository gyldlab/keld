//! Native first-navigation witness (GH-814): the shipping `create_app` path's real
//! `NavigationStarting` COM subscription stamps the first-navigation diagnostic.
//!
//! This is not a media test. It lives under the opt-in `media-acceptance` fixture
//! because it needs that fixture's real `WebView2` engine, test-owned profile root and
//! watchdog; the contract it proves belongs to `webview2::navigation_diagnostics`.

use super::*;

/// Native Windows COM proof: the actual `create_app`/`NavigateToString`
/// subscription, not a direct call to the phase reducer, must observe
/// `WebView2`'s first `NavigationStarting` callback before `NavigationReady`.
/// Removing `add_NavigationStarting` must make this test RED while leaving
/// the ordinary renderer-ready path alive.
#[test]
#[ignore = "requires actual Windows WebView2 initial-navigation COM callbacks"]
fn windows_first_navigation_start_is_observed_by_real_com_callback() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        use crate::profile::{EphemeralProfile, WebProfileSelection};

        // A failed browser/profile release must remain inspectable. This
        // guard reports early setup errors as well as runtime errors;
        // only a separately proven successful release authorizes cleanup.
        struct FixtureRetention<'a> {
            path: &'a Path,
            cleaned: bool,
        }
        impl Drop for FixtureRetention<'_> {
            fn drop(&mut self) {
                if !self.cleaned && self.path.exists() {
                    eprintln!("KELD_FIRST_NAV_RETAINED {}", self.path.display());
                }
            }
        }
        let root = profile_fixture_root("first-navigation-start");
        std::fs::create_dir_all(&root).map_err(failure)?;
        let mut retention = FixtureRetention {
            path: &root,
            cleaned: false,
        };
        let selection =
            WebProfileSelection::ephemeral_dev(EphemeralProfile::from_host_random([88; 32])?);
        let plan = super::super::super::windows_profile_plan(&root, selection)?;
        let mut engine = profile_fixture_engine(&root, selection)?;
        let navigation = Arc::clone(&engine.navigation_diagnostic);
        let canary = format!("private-navigation-canary-{}", std::process::id());
        let spec = WebviewSpec {
            initial: NavTarget::Html(format!("<!doctype html><title>{canary}</title>")),
            ..blank_spec()
        };
        let (events_tx, events_rx) = mpsc::channel();
        engine.create_app(&spec, events_tx)?;

        // Only a real production NavigationReady authorizes the Quit.
        // A missing callback is not fixed by sleeping or relaxing the
        // 15-second shipping navigation deadline; it remains a test RED.
        let (commands_tx, commands_rx) = mpsc::channel();
        let readiness = thread::spawn(move || {
            let ready = matches!(
                events_rx.recv_timeout(Duration::from_secs(20)),
                Ok(crate::AppWindowEvent::NavigationReady)
            );
            let command = if ready {
                crate::AppWindowCommand::Quit
            } else {
                crate::AppWindowCommand::Fatal
            };
            commands_tx.send(command).is_ok() && ready
        });
        let run = engine.run_app_until_quit(commands_rx, mpsc::channel().0);
        let ready = readiness
            .join()
            .map_err(|_| failure("native navigation readiness owner panicked"))?;
        // Only reclaim this exact test-owned root after the shipping
        // engine has proved its profile release. Keep failed-release
        // fixtures for investigation; do not hide a browser/lease leak.
        // In particular, the deliberate missing-subscription RED still
        // cleans up because navigation and release succeeded independently.
        let released = !plan.control_dir.exists();
        if run.is_ok() && released {
            std::fs::remove_dir_all(&root).map_err(failure)?;
            retention.cleaned = true;
        }
        run?;
        if !released {
            return Err(failure(
                "successful real navigation did not release its profile",
            ));
        }
        if !ready {
            return Err(failure("no real initial NavigationReady was observed"));
        }
        let evidence = navigation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot();
        if !evidence.contains("navigation_start_at_ms=Some(") {
            return Err(failure(format!(
                "real COM NavigationStarting was not observed: {evidence}"
            )));
        }
        if !evidence.contains("first_loop_turn_at_ms=Some(") {
            return Err(failure(format!(
                "the real Tao event callback was never observed: {evidence}"
            )));
        }
        if evidence.contains(&canary) || evidence.contains("<title>") {
            return Err(failure("navigation diagnostics exposed test HTML content"));
        }
        println!("KELD_FIRST_NAV_START {evidence}");
        Ok(())
    })
}
