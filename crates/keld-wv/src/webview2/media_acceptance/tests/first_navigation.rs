//! Native first-navigation witness (GH-814): the shipping `create_app` path's real
//! `NavigationStarting` COM subscription stamps the first-navigation diagnostic.
//!
//! This is not a media test. It lives under the opt-in `media-acceptance` fixture
//! because it needs that fixture's real `WebView2` engine, test-owned profile root and
//! watchdog; the contract it proves belongs to `webview2::navigation_diagnostics`.

use std::path::Path;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use super::super::{FIXTURE_DEADLINE, blank_spec, failure};
use super::{profile_fixture_engine, profile_fixture_root, run_renderer_until_quit};
use crate::webview2::tests::run_with_watchdog;
use crate::{NavTarget, WebviewSpec, WvError};

/// Native Windows COM proof: the actual `create_app`/`NavigateToString`
/// subscription, not a direct call to the phase reducer, must observe
/// `WebView2`'s first `NavigationStarting` callback before `NavigationReady`.
/// Removing `add_NavigationStarting` must make this test RED while leaving
/// the ordinary renderer-ready path alive.
#[test]
#[ignore = "requires actual Windows WebView2 initial-navigation COM callbacks"]
fn windows_first_navigation_start_is_observed_by_real_com_callback() -> Result<(), WvError> {
    first_navigation_after(None)
}

#[test]
#[ignore = "requires a claimed Windows WebView2 native slot"]
fn initial_html_nul_refused_then_valid_navigation_ready() -> Result<(), WvError> {
    first_navigation_after(Some(NavTarget::Html(
        "<!doctype html><title>prefix</title>\0<p>suffix</p>".to_owned(),
    )))
}

#[test]
#[ignore = "requires a claimed Windows WebView2 native slot"]
fn initial_url_nul_refused_then_valid_navigation_ready() -> Result<(), WvError> {
    // A local data URL gives the broken implementation a valid, self-contained prefix.
    first_navigation_after(Some(NavTarget::Url(
        "data:text/html,%3Ctitle%3Eprefix%3C/title%3E\0suffix".to_owned(),
    )))
}

struct Creation {
    events: mpsc::Receiver<crate::AppWindowEvent>,
    refused_events: Option<mpsc::Receiver<crate::AppWindowEvent>>,
    defect: Option<WvError>,
}

fn start_case(
    engine: &mut crate::webview2::WebView2Engine,
    healthy: &WebviewSpec,
    invalid: Option<NavTarget>,
) -> Result<Creation, WvError> {
    let mut refused_events = None;
    let mut defect = None;
    if let Some(initial) = invalid {
        let (bad_tx, bad_rx) = mpsc::channel();
        let bad = WebviewSpec {
            initial,
            ..blank_spec()
        };
        match engine.create_app(&bad, bad_tx) {
            Ok(_) => {
                // Drain the wrongly accepted document normally before reporting failure.
                return Ok(Creation {
                    events: bad_rx,
                    refused_events: None,
                    defect: Some(failure("initial navigation accepted an embedded NUL")),
                });
            }
            Err(WvError::Navigate(reason))
                if reason
                    == "initial navigation contains a NUL character; remove it before loading" =>
            {
                println!("KELD_INITIAL_NUL_REFUSED KELD-WV-005: {reason}");
            }
            Err(other) => defect = Some(failure(format!("wrong initial refusal: {other}"))),
        }
        refused_events = Some(bad_rx);
    }
    // A separate channel prevents a callback from the rejected attempt from
    // serving as the healthy document's independent NavigationReady witness.
    let (healthy_tx, events) = mpsc::channel();
    engine.create_app(healthy, healthy_tx)?;
    Ok(Creation {
        events,
        refused_events,
        defect,
    })
}

fn first_navigation_after(invalid: Option<NavTarget>) -> Result<(), WvError> {
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
        let nul_case = invalid.is_some();
        if nul_case {
            println!("KELD_INITIAL_NUL_ROOT {}", root.display());
        }
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
        let Creation {
            events: events_rx,
            refused_events,
            defect,
        } = start_case(&mut engine, &spec, invalid)?;

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
        let run = run_renderer_until_quit(engine, commands_rx, mpsc::channel().0);
        let ready = readiness
            .join()
            .map_err(|_| failure("native navigation readiness owner panicked"))?;
        // Only reclaim this exact test-owned root after the shipping
        // engine has proved its profile release. Keep failed-release
        // fixtures for investigation; do not hide a browser/lease leak.
        // In particular, the deliberate missing-subscription RED still
        // cleans up because navigation and release succeeded independently.
        let released = matches!(
            std::fs::symlink_metadata(&plan.control_dir),
            Err(ref error) if error.kind() == std::io::ErrorKind::NotFound
        );
        if run.is_ok() && released {
            std::fs::remove_dir_all(&root).map_err(failure)?;
            retention.cleaned = true;
            if nul_case {
                println!("KELD_INITIAL_NUL_RELEASED {}", root.display());
            }
        }
        run?;
        if !released {
            return Err(failure(
                "successful real navigation did not release its profile",
            ));
        }
        if let Some(defect) = defect {
            return Err(defect);
        }
        if refused_events.is_some_and(|rx| {
            rx.try_iter()
                .any(|event| matches!(event, crate::AppWindowEvent::NavigationReady))
        }) {
            return Err(failure("refused initial content emitted NavigationReady"));
        }
        if !ready {
            return Err(failure("no real initial NavigationReady was observed"));
        }
        let evidence = navigation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot();
        check_first_navigation_evidence(&evidence, &canary)
    })
}

fn check_first_navigation_evidence(evidence: &str, canary: &str) -> Result<(), WvError> {
    // A failed observer registration is reported distinctly (#958), not as an
    // unobserved NavigationStarting callback.
    if !evidence.contains("navigation_start_observer=registered") {
        return Err(failure(format!(
            "the NavigationStarting observer was not registered: {evidence}"
        )));
    }
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
    if !evidence.contains("dom_content_loaded_at_ms=Some(") {
        return Err(failure(format!(
            "the actual WebView2 DOMContentLoaded callback for initial HTML was not observed: {evidence}"
        )));
    }
    if !evidence.contains("navigation_ids_match=Some(true)") {
        return Err(failure(format!(
            "first NavigationStarting and NavigationCompleted IDs did not correlate: {evidence}"
        )));
    }
    if evidence.contains(canary) || evidence.contains("<title>") {
        return Err(failure("navigation diagnostics exposed test HTML content"));
    }
    println!("KELD_FIRST_NAV_START {evidence}");
    Ok(())
}
