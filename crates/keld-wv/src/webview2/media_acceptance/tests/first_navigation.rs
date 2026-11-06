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

use webview2_com::DocumentTitleChangedEventHandler;
use windows::core::PWSTR;

use super::super::WebEngine;
use super::super::{FIXTURE_DEADLINE, blank_spec, failure};
use super::{profile_fixture_engine, profile_fixture_root, run_renderer_until_quit};
use crate::webview2::tests::run_with_watchdog;
use crate::{NavTarget, WebviewId, WebviewSpec, WvError};

/// Native Windows COM proof: the actual `create_app`/`NavigateToString`
/// subscription, not a direct call to the phase reducer, must observe
/// `WebView2`'s first `NavigationStarting` callback before `NavigationReady`.
/// Removing `add_NavigationStarting` must make this test RED while leaving
/// the ordinary renderer-ready path alive.
#[test]
#[ignore = "requires actual Windows WebView2 initial-navigation COM callbacks"]
fn windows_first_navigation_start_is_observed_by_real_com_callback() -> Result<(), WvError> {
    first_navigation_after(None, None)
}

#[test]
#[ignore = "requires a claimed Windows WebView2 native slot"]
fn initial_html_nul_refused_then_valid_navigation_ready() -> Result<(), WvError> {
    first_navigation_after(
        Some(NavTarget::Html(
            "<!doctype html><title>prefix</title>\0<p>suffix</p>".to_owned(),
        )),
        None,
    )
}

#[test]
#[ignore = "requires a claimed Windows WebView2 native slot"]
fn initial_url_nul_refused_then_valid_navigation_ready() -> Result<(), WvError> {
    // A local data URL gives the broken implementation a valid, self-contained prefix.
    first_navigation_after(
        Some(NavTarget::Url(
            "data:text/html,%3Ctitle%3Eprefix%3C/title%3E\0suffix".to_owned(),
        )),
        None,
    )
}

/// Actual subsequent HTML navigation must refuse NUL without poisoning the
/// existing window's independently observed first navigation and release.
#[test]
#[ignore = "requires a claimed Windows WebView2 native slot"]
fn later_html_nul_refused_then_valid_navigation_ready() -> Result<(), WvError> {
    first_navigation_after(None, Some(LaterNulCase::Html))
}

/// The URL route has its own native COM call and typed refusal.
#[test]
#[ignore = "requires a claimed Windows WebView2 native slot"]
fn later_url_nul_refused_then_valid_navigation_ready() -> Result<(), WvError> {
    first_navigation_after(None, Some(LaterNulCase::Url))
}

/// `ExecuteScript` must refuse truncation before the native COM call.
#[test]
#[ignore = "requires a claimed Windows WebView2 native slot"]
fn later_eval_nul_refused_then_valid_navigation_ready() -> Result<(), WvError> {
    first_navigation_after(None, Some(LaterNulCase::Eval))
}

#[derive(Clone, Copy)]
enum LaterNulCase {
    Html,
    Url,
    Eval,
}

/// The native window exists before checking later-call error precedence.
/// A mutation that removes only the later guard must fail this exact selector.
fn later_nul_defect(
    engine: &mut crate::webview2::WebView2Engine,
    id: WebviewId,
    case: LaterNulCase,
) -> Option<WvError> {
    let bad_html = NavTarget::Html("<title>prefix</title>\0private-suffix".to_owned());
    let bad_url = NavTarget::Url("data:text/html,%3Ctitle%3Eprefix\0private-suffix".to_owned());
    let bad_script = "globalThis.__keldTest = 'prefix';\0globalThis.__keldTest = 'suffix'";
    let rejected = match case {
        LaterNulCase::Html => engine.navigate(id, bad_html.clone()),
        LaterNulCase::Url => engine.navigate(id, bad_url.clone()),
        LaterNulCase::Eval => engine.eval(id, bad_script),
    };
    let correct = match (case, &rejected) {
        (LaterNulCase::Html | LaterNulCase::Url, Err(WvError::Navigate(reason))) => {
            reason == "navigation contains a NUL character; remove it before loading"
        }
        (LaterNulCase::Eval, Err(WvError::Script(reason))) => {
            reason == "script contains a NUL character; remove it before evaluation"
        }
        _ => false,
    };

    // The lookup must precede argument conversion even for malformed input.
    let stale = WebviewId(u32::MAX);
    let stale_reply = match case {
        LaterNulCase::Html => engine.navigate(stale, bad_html),
        LaterNulCase::Url => engine.navigate(stale, bad_url),
        LaterNulCase::Eval => engine.eval(stale, bad_script),
    };
    if !matches!(stale_reply, Err(WvError::UnknownWebview { .. })) {
        return Some(failure(format!(
            "later NUL operation lost stale-view precedence: {stale_reply:?}"
        )));
    }
    if !correct {
        return Some(failure(format!(
            "later NUL operation did not return its exact typed refusal: {rejected:?}"
        )));
    }
    println!("KELD_LATER_NUL_REFUSED known-view and stale-view controls passed");
    None
}

/// After a malformed later call is refused, prove a distinct valid call changed
/// the actual WebView2 document. The original NavigationReady cannot satisfy
/// this oracle: only a newly registered native title-change callback can.
fn prove_later_recovery(
    engine: &mut crate::webview2::WebView2Engine,
    id: WebviewId,
    case: LaterNulCase,
) -> Result<(), WvError> {
    let webview = engine.view(id)?.webview.clone();
    let (changed_tx, changed_rx) = mpsc::channel();
    let callback = DocumentTitleChangedEventHandler::create(Box::new(move |_, _| {
        let _ = changed_tx.send(());
        Ok(())
    }));
    let mut token = 0_i64;
    // SAFETY: the live WebView2, handler and token stay on the owning STA.
    unsafe { webview.add_DocumentTitleChanged(&callback, &raw mut token) }
        .map_err(failure)?;

    let marker = match case {
        LaterNulCase::Html => "keld-later-html-recovered",
        LaterNulCase::Url => "keld-later-url-recovered",
        LaterNulCase::Eval => "keld-later-eval-recovered",
    };
    let outcome = (|| -> Result<(), WvError> {
        match case {
            LaterNulCase::Html => engine.navigate(
                id,
                NavTarget::Html(format!("<!doctype html><title>{marker}</title>")),
            )?,
            LaterNulCase::Url => engine.navigate(
                id,
                NavTarget::Url(format!("data:text/html,%3Ctitle%3E{marker}%3C/title%3E")),
            )?,
            LaterNulCase::Eval => engine.eval(id, &format!("document.title = '{marker}';"))?,
        }
        // WebView2 fires this for a real changed document title, not for a
        // successful Rust return or the already-observed initial navigation.
        super::super::wait_with_pump(changed_rx).map_err(failure)?;
        let mut title = PWSTR::null();
        // SAFETY: the COM getter writes a caller-owned PWSTR on its creating STA.
        unsafe { webview.DocumentTitle(&raw mut title) }.map_err(failure)?;
        let observed = super::super::owned_string(title)?;
        if observed != marker {
            return Err(failure(format!(
                "post-refusal native title mismatch: expected {marker:?}, got {observed:?}"
            )));
        }
        println!("KELD_LATER_NUL_RECOVERED title={marker}");
        Ok(())
    })();
    // SAFETY: remove the test observer on the same STA, even after a failed
    // post-refusal operation; the production observer remains installed.
    unsafe { webview.remove_DocumentTitleChanged(token) }.map_err(failure)?;
    outcome
}
struct Creation {
    events: Option<mpsc::Receiver<crate::AppWindowEvent>>,
    ready_before_run: bool,
    refused_events: Option<mpsc::Receiver<crate::AppWindowEvent>>,
    defect: Option<WvError>,
}

fn start_case(
    engine: &mut crate::webview2::WebView2Engine,
    healthy: &WebviewSpec,
    invalid: Option<NavTarget>,
    later: Option<LaterNulCase>,
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
                    events: Some(bad_rx),
                    ready_before_run: false,
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
    let live_id = engine.create_app(healthy, healthy_tx)?;
    let (events, ready_before_run) = if let Some(case) = later {
        // Pump COM until the first real navigation completes. Otherwise a
        // later navigation could cancel startup and its old Ready could be
        // mistaken for post-refusal recovery.
        let initial = super::super::wait_with_pump(events).map_err(failure)?;
        if !matches!(initial, crate::AppWindowEvent::NavigationReady) {
            defect = Some(failure(format!("first native navigation was not ready: {initial:?}")));
        }
        let late_defect = later_nul_defect(engine, live_id, case);
        if defect.is_none() {
            defect = late_defect;
        }
        if defect.is_none() {
            defect = prove_later_recovery(engine, live_id, case).err();
        }
        (None, matches!(initial, crate::AppWindowEvent::NavigationReady))
    } else {
        (Some(events), false)
    };
    Ok(Creation {
        events,
        ready_before_run,
        refused_events,
        defect,
    })
}

fn first_navigation_after(
    invalid: Option<NavTarget>,
    later: Option<LaterNulCase>,
) -> Result<(), WvError> {
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
        let nul_case = invalid.is_some() || later.is_some();
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
            ready_before_run,
            refused_events,
            defect,
        } = start_case(&mut engine, &spec, invalid, later)?;

        // Only a real production NavigationReady authorizes the Quit.
        // A missing callback is not fixed by sleeping or relaxing the
        // 15-second shipping navigation deadline; it remains a test RED.
        let (commands_tx, commands_rx) = mpsc::channel();
        let readiness = thread::spawn(move || {
            let ready = ready_before_run
                || matches!(
                    events_rx.and_then(|rx| rx.recv_timeout(Duration::from_secs(20)).ok()),
                    Some(crate::AppWindowEvent::NavigationReady)
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
