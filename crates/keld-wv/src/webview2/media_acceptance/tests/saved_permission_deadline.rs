//! Real profile API regression: a spent construction budget cannot authorize continuation.
use super::{
    FIXTURE_DEADLINE, blank_spec, failure, profile_fixture_engine, profile_fixture_root,
    run_renderer_until_quit, run_with_watchdog,
};
use crate::profile::{EphemeralProfile, ProfileErrorKind, WebProfileSelection};
use crate::webview2::saved_media_permissions::{self as saved, SavedPermission};
use crate::webview2::{self as backend, WebView2Engine};
use crate::{WebEngine as _, WebviewId, WvError};
use saved::test_control::{Fault, Site};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
    COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY,
};
fn fresh_deadline() -> Instant {
    Instant::now() + backend::PROFILE_LAUNCH_DEADLINE
}
fn with_live_profile(
    work: impl FnOnce(&mut WebView2Engine, WebviewId) -> Result<(), WvError>,
) -> Result<(), WvError> {
    with_engine(|engine| {
        let id = engine.create(&blank_spec())?;
        work(engine, id)
    })
}

pub(super) fn with_engine(
    work: impl FnOnce(&mut WebView2Engine) -> Result<(), WvError>,
) -> Result<(), WvError> {
    let root = profile_fixture_root("permission-deadline");
    std::fs::create_dir_all(root.parent().expect("fixture has a parent")).map_err(failure)?;
    std::fs::create_dir(&root).map_err(failure)?;
    println!("KELD_PERMISSION_DEADLINE_ROOT {}", root.display());
    let result = (|| {
        let profile = EphemeralProfile::from_host_random([88; 32])?;
        let mut engine =
            profile_fixture_engine(&root, WebProfileSelection::ephemeral_dev(profile))?;
        let observed = work(&mut engine);
        let (commands, receiver) = mpsc::channel();
        commands
            .send(crate::AppWindowCommand::Quit)
            .map_err(failure)?;
        let release = run_renderer_until_quit(engine, receiver, mpsc::channel().0);
        if let Err(error) = &release {
            eprintln!("KELD_PERMISSION_RELEASE_ERROR {error}");
        }
        // Execute real release even for a failing oracle; never replace the
        // original product verdict with a secondary fixture failure.
        observed.and(release)
    })();
    let cleanup = std::fs::remove_dir_all(&root).map_err(failure);
    match &cleanup {
        Ok(()) => println!("KELD_PERMISSION_DEADLINE_CLEANED {}", root.display()),
        Err(error) => eprintln!("KELD_PERMISSION_CLEANUP_ERROR {}: {error}", root.display()),
    }
    result.and(cleanup)
}
fn require_deadline_error<T>(result: Result<T, WvError>) -> Result<(), WvError> {
    match result {
        Err(WvError::ProfileSelection(error))
            if error.kind() == ProfileErrorKind::LifecycleUnproven
                && error.to_string().contains(backend::LAUNCH_DEADLINE_EXPIRED) =>
        {
            Ok(())
        }
        Err(error) => Err(failure(format!(
            "expected original typed deadline error, got {error}"
        ))),
        Ok(_) => Err(failure("expired permission budget returned success")),
    }
}
#[test]
#[ignore = "real Windows profile permission API and ordered release"]
fn windows_saved_permission_reconciliation_refuses_a_spent_budget() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        with_live_profile(|engine, id| {
            let webview = &engine.view(id)?.webview;
            let profile = saved::webview_profile(webview)?;
            let origin = "https://keld-permission-deadline.invalid/";
            for kind in [
                COREWEBVIEW2_PERMISSION_KIND_CAMERA,
                COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
            ] {
                let permission = SavedPermission {
                    kind,
                    state: COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                    origin: origin.to_owned(),
                };
                saved::set_profile_permission_state(
                    &profile,
                    &permission,
                    COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                    fresh_deadline(),
                )?;
            }
            let before = saved::profile_permission_settings(&profile, fresh_deadline())?;
            for setting in &before {
                println!(
                    "KELD_PERMISSION_SEED kind={} state={} origin={}",
                    setting.kind.0, setting.state.0, setting.origin
                );
            }
            let allows = before
                .iter()
                .filter(|p| p.origin == origin && p.state == COREWEBVIEW2_PERMISSION_STATE_ALLOW)
                .count();
            if allows != 2 {
                return Err(failure("native saved-Allow seed was not observed"));
            }
            let result = saved::reconcile_saved_media_permissions(webview, Instant::now());
            println!("KELD_PERMISSION_EXPIRED seeded={allows} result={result:?}");
            require_deadline_error(result)?;
            let unchanged = saved::profile_permission_settings(&profile, fresh_deadline())?;
            if unchanged
                .iter()
                .filter(|p| p.origin == origin && p.state == COREWEBVIEW2_PERMISSION_STATE_ALLOW)
                .count()
                != 2
            {
                return Err(failure(
                    "expired admission issued a native permission write",
                ));
            }
            saved::reconcile_saved_media_permissions(webview, fresh_deadline())?;
            let after = saved::profile_permission_settings(&profile, fresh_deadline())?;
            if after
                .iter()
                .filter(|p| p.origin == origin && p.state == COREWEBVIEW2_PERMISSION_STATE_DENY)
                .count()
                != 2
            {
                return Err(failure(
                    "fresh reconciliation did not deny both native saved grants",
                ));
            }
            println!("KELD_PERMISSION_FRESH camera=deny microphone=deny");
            Ok(())
        })
    })
}

fn pending_completion(site: Site) -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        with_live_profile(|engine, id| {
            let profile = saved::webview_profile(&engine.view(id)?.webview)?;
            let permission = SavedPermission {
                kind: COREWEBVIEW2_PERMISSION_KIND_CAMERA,
                state: COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                origin: "https://keld-permission-callback.invalid/".to_owned(),
            };
            saved::set_profile_permission_state(
                &profile,
                &permission,
                COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                fresh_deadline(),
            )?;
            let control = saved::profile_permission_settings(&profile, fresh_deadline())?;
            if !control.iter().any(|p| {
                p.kind == permission.kind
                    && p.origin == permission.origin
                    && p.state == COREWEBVIEW2_PERMISSION_STATE_ALLOW
            }) {
                return Err(failure(
                    "at-risk saved grant missing before callback suppression",
                ));
            }
            let fault = Fault::arm(Some(site));
            let started = Instant::now();
            let deadline = started + Duration::from_secs(1);
            let result = match site {
                Site::Read => saved::profile_permission_settings(&profile, deadline).map(|_| ()),
                Site::Write => saved::set_profile_permission_state(
                    &profile,
                    &permission,
                    COREWEBVIEW2_PERMISSION_STATE_DENY,
                    deadline,
                ),
            };
            let trace = fault.finish();
            println!(
                "KELD_PERMISSION_CALLBACK site={site:?} elapsed_ms={} native_callbacks={:?} result={result:?}",
                started.elapsed().as_millis(),
                trace.completed
            );
            require_deadline_error(result)?;
            if trace.calls != vec![(site, deadline)] || trace.completed != vec![(site, true)] {
                return Err(failure(
                    "suppression must intercept one actual successful native completion",
                ));
            }
            if started.elapsed() > Duration::from_secs(3) {
                return Err(failure(
                    "callback refusal escaped its supplied one-second wait budget",
                ));
            }
            // Timeout is not native cancellation: the suppressed write may already be
            // committed. Query the actual profile, then prove fresh healthy operations.
            let actual = saved::profile_permission_settings(&profile, fresh_deadline())?;
            let actual = actual
                .iter()
                .find(|p| p.kind == permission.kind && p.origin == permission.origin)
                .ok_or_else(|| failure("the live native profile lost its seeded permission"))?;
            let expected = if site == Site::Write {
                COREWEBVIEW2_PERMISSION_STATE_DENY
            } else {
                COREWEBVIEW2_PERMISSION_STATE_ALLOW
            };
            if actual.state != expected {
                return Err(failure(
                    "native write/callback result was incorrectly treated as cancelled",
                ));
            }
            saved::set_profile_permission_state(
                &profile,
                &permission,
                COREWEBVIEW2_PERMISSION_STATE_DENY,
                fresh_deadline(),
            )?;
            saved::reconcile_saved_media_permissions(&engine.view(id)?.webview, fresh_deadline())?;
            println!("KELD_PERMISSION_CALLBACK_RECOVERY site={site:?} healthy=true");
            Ok(())
        })
    })
}

#[test]
#[ignore = "real Windows saved-permission collection callback withheld after native completion"]
fn windows_saved_permission_read_callback_is_bounded() -> Result<(), WvError> {
    pending_completion(Site::Read)
}

#[test]
#[ignore = "real Windows saved-permission write callback withheld after native completion"]
fn windows_saved_permission_write_callback_is_bounded() -> Result<(), WvError> {
    pending_completion(Site::Write)
}

#[test]
#[ignore = "real Windows construction with withheld list callback and unmodified production deadline"]
fn windows_saved_permission_create_keeps_one_budget_and_retries() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        with_engine(|engine| {
            let original_id = engine.next_id;
            let fault = Fault::arm(Some(Site::Read));
            let started = Instant::now();
            let result = engine.create(&blank_spec());
            let trace = fault.finish();
            println!(
                "KELD_PERMISSION_CREATE elapsed_ms={} native_callbacks={:?} reconciled={} views={} result={result:?}",
                started.elapsed().as_millis(),
                trace.completed,
                engine.saved_permissions_reconciled,
                engine.views.len()
            );
            let observed = (|| {
                require_deadline_error(result)?;
                let build = trace
                    .build
                    .ok_or_else(|| failure("no native construction budget was recorded"))?;
                if trace.calls != vec![(Site::Read, build)]
                    || trace.completed != vec![(Site::Read, true)]
                {
                    return Err(failure(
                        "saved reconciliation did not retain the original construction deadline",
                    ));
                }
                if engine.saved_permissions_reconciled
                    || !engine.views.is_empty()
                    || engine.next_id != original_id
                {
                    return Err(failure(
                        "failed reconciliation published a successful/native-navigable view",
                    ));
                }
                Ok(())
            })();
            // Always perform the same healthy retry after observing the failed
            // attempt, including under a deliberately broken budget mutation.
            let recovery = (|| {
                let id = engine.create(&blank_spec())?;
                if !engine.saved_permissions_reconciled
                    || id.0 != original_id
                    || engine.views.len() != 1
                {
                    return Err(failure(
                        "healthy retry failed to perform reconciliation before first view publication",
                    ));
                }
                println!("KELD_PERMISSION_CREATE_RETRY reconciled=true views=1");
                Ok(())
            })();
            if let Err(error) = &recovery {
                eprintln!("KELD_PERMISSION_RETRY_ERROR {error}");
            }
            observed.and(recovery)
        })
    })
}

#[test]
#[ignore = "real Windows empty permission collection with expired admission budget"]
fn windows_empty_saved_permission_collection_still_checks_budget() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        with_live_profile(|engine, id| {
            let webview = &engine.view(id)?.webview;
            let profile = saved::webview_profile(webview)?;
            if !saved::profile_permission_settings(&profile, fresh_deadline())?.is_empty() {
                return Err(failure(
                    "fresh profile was not an independent empty-collection control",
                ));
            }
            require_deadline_error(saved::reconcile_saved_media_permissions(
                webview,
                Instant::now(),
            ))?;
            saved::reconcile_saved_media_permissions(webview, fresh_deadline())?;
            println!("KELD_PERMISSION_EMPTY expired=refused fresh=accepted");
            Ok(())
        })
    })
}
