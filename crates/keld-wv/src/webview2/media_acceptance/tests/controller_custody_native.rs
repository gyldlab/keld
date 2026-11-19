//! Accepted native controller requests that outlive their production wait.
//! The anchor is a fixture-owned real controller, not a managed application view.
//! It supplies a live browser handle and a calibrated native Close control.

use super::{
    FIXTURE_DEADLINE, failure, profile_fixture_engine, profile_fixture_root, run_with_watchdog,
};
use crate::WvError;
use crate::profile::{EphemeralProfile, WebProfileSelection};
use crate::webview2 as backend;
use crate::webview2::media_acceptance::{CONTROLLER_CUSTODY_EVIDENCE, ControllerCustodyEvidence};
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{E_ABORT, HANDLE, HWND, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{GetCurrentThreadId, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::IsWindow;
use windows::core::BOOL;

#[derive(Clone, Copy, Debug)]
enum Mode {
    Healthy,
    LateSuccess,
    ParentCancelled,
}

#[derive(Debug, PartialEq, Eq)]
struct Facts {
    calls: u32,
    returns: u32,
    callbacks: u32,
    completions: Vec<(i32, bool, u32)>,
    closes: Vec<i32>,
}

fn facts() -> Facts {
    CONTROLLER_CUSTODY_EVIDENCE.with_borrow(|e| Facts {
        calls: e.calls,
        returns: e.returns,
        callbacks: e.callbacks,
        completions: e.completions.clone(),
        closes: e.close_results.clone(),
    })
}

fn retained() -> Option<backend::ICoreWebView2Controller> {
    CONTROLLER_CUSTODY_EVIDENCE.with_borrow(|e| e.retained.clone())
}

fn visible_hresult(controller: &backend::ICoreWebView2Controller) -> i32 {
    let mut visible = BOOL(0);
    // SAFETY: the fixture retains the real interface on its creating STA.
    // The healthy anchor calibrates this query before and after actual Close.
    // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller
    unsafe { controller.IsVisible(&raw mut visible) }
        .err()
        .map_or(0, |error| error.code().0)
}

fn close(controller: &backend::ICoreWebView2Controller) -> Result<(), WvError> {
    // SAFETY: only this fixture's adopted/retained controller, on its STA.
    // This cleanup/control call is separate from the production custody counter.
    // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2controller#close
    unsafe { controller.Close() }.map_err(failure)
}

fn is_window(hwnd: HWND) -> bool {
    // SAFETY: a read-only native check of a captured fixture HWND.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindow
    unsafe { IsWindow(Some(hwnd)) }.as_bool()
}

fn process_state(process: &OwnedHandle) -> windows::Win32::Foundation::WAIT_EVENT {
    // SAFETY: the existing helper acquired this exact SYNCHRONIZE handle while
    // the anchor's browser was alive. Never reopen a potentially reused PID.
    // https://learn.microsoft.com/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject
    unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 0) }
}

fn pump_until(
    deadline: Instant,
    process: Option<&OwnedHandle>,
    predicate: impl Fn() -> bool,
) -> Result<(), WvError> {
    while !predicate() {
        if Instant::now() >= deadline {
            return Err(failure(
                "native custody probe incomplete at cleanup deadline",
            ));
        }
        // Remaining budget is a kill switch, never a negative oracle.
        let handles = process
            .filter(|process| process_state(process) == WAIT_TIMEOUT)
            .map(|process| [HANDLE(process.as_raw_handle())]);
        backend::pump_message_or_handle(
            handles.as_ref().map(<[HANDLE; 1]>::as_slice),
            backend::milliseconds_until(deadline)?,
        )?;
    }
    Ok(())
}

// Drop retained COM witnesses before the engine's apartment on unwind too.
struct ObservationScope;
impl Drop for ObservationScope {
    fn drop(&mut self) {
        let controller = CONTROLLER_CUSTODY_EVIDENCE.with_borrow_mut(|e| {
            let retained = e.retained.take();
            *e = ControllerCustodyEvidence::default();
            retained
        });
        if let Some(controller) = controller
            && visible_hresult(&controller) == 0
        {
            let _ = close(&controller);
        }
    }
}

// Outer errors mean cleanup or eligibility was not proved; the inner result is the frozen verdict.
#[allow(
    clippy::too_many_lines,
    reason = "Keep native acquisition, frozen observations and cleanup order in one auditable case."
)]
fn case(mode: Mode) -> Result<Result<(), WvError>, WvError> {
    let root = profile_fixture_root("native-controller-custody");
    std::fs::create_dir_all(&root).map_err(failure)?;
    println!("KELD_CUSTODY_ROOT mode={mode:?} {}", root.display());
    let deadline = Instant::now() + FIXTURE_DEADLINE;
    CONTROLLER_CUSTODY_EVIDENCE.with_borrow_mut(|e| {
        assert!(!e.active, "one custody fixture per STA");
        *e = ControllerCustodyEvidence {
            active: true,
            ..ControllerCustodyEvidence::default()
        };
    });
    let mut engine = None;
    let mut anchor_window = None;
    let mut anchor = None;
    let mut target_window = None;
    let mut target_hwnd = None;
    let mut browser = None;
    let mut browser_pid = 0;
    let mut target_closed_hresult = None;
    let mut target_was_adopted = None;
    // Keep the existing first-fault owner alive through DestroyWindow and the
    // real late callback. It intentionally starts unbound, exercising only the
    // zero-controller exemption of this owner, not a durable application run.
    let release = Rc::new(backend::ProfileReleaseWait::new(
        None,
        Arc::new(backend::BrowserBinding::new()),
        None,
        backend::ReleaseSite::Host,
    ));
    let observation = ObservationScope;
    let outcome = (|| -> Result<(), WvError> {
        engine = Some(profile_fixture_engine(
            &root,
            WebProfileSelection::ephemeral_dev(EphemeralProfile::from_host_random([95; 32])?),
        )?);
        let env = &engine.as_ref().expect("engine initialized").environment;
        anchor_window = Some(backend::RecoveryWindow::new()?);
        let anchor_hwnd = anchor_window.as_ref().expect("anchor window").0;
        anchor = Some(
            backend::create_controller_observed_until(env, anchor_hwnd, deadline)
                .map_err(backend::ControllerCreationError::into_wv_error)?,
        );
        let controller = anchor.as_ref().expect("anchor controller");
        if visible_hresult(controller) != 0 || !is_window(anchor_hwnd) {
            return Err(failure("ineligible: healthy anchor is not live"));
        }
        // SAFETY: read the real live anchor on its creating STA.
        let webview = unsafe { controller.CoreWebView2() }.map_err(failure)?;
        browser_pid = backend::webview_browser_process_id(&webview)
            .ok_or_else(|| failure("ineligible: anchor browser PID is unavailable"))?;
        drop(webview);
        browser = backend::open_browser_process(browser_pid);
        if browser.as_ref().map(process_state) != Some(WAIT_TIMEOUT) {
            return Err(failure(
                "ineligible: no independently live anchor browser handle",
            ));
        }
        target_window = Some(backend::RecoveryWindow::new()?);
        let hwnd = target_window.as_ref().expect("target window").0;
        target_hwnd = Some(hwnd);
        if !is_window(hwnd) {
            return Err(failure("ineligible: target parent is not live"));
        }
        // Observe only the target after the healthy anchor completed.
        let request_deadline = match mode {
            Mode::Healthy => deadline,
            _ => Instant::now() + Duration::from_millis(200),
        };
        CONTROLLER_CUSTODY_EVIDENCE.with_borrow_mut(|e| {
            *e = ControllerCustodyEvidence {
                active: true,
                expire_after_call: (!matches!(mode, Mode::Healthy)).then_some(request_deadline),
                ..ControllerCustodyEvidence::default()
            };
        });
        let result = backend::create_controller_owned_until(
            env,
            hwnd,
            request_deadline,
            Some(Rc::downgrade(&release)),
        );
        if matches!(mode, Mode::Healthy) {
            let adopted = result
                .map_err(backend::ControllerCreationError::into_wv_error)?
                .adopt()?;
            if visible_hresult(&adopted) != 0 || release.fault().is_some() {
                return Err(failure(
                    "healthy adoption faulted or returned a closed controller",
                ));
            }
            let completed = facts();
            // SAFETY: a pure current-thread identity query.
            let thread = unsafe { GetCurrentThreadId() };
            if completed.calls != 1
                || completed.returns != 1
                || completed.callbacks != 1
                || completed.completions != [(0, true, thread)]
                || !completed.closes.is_empty()
            {
                return Err(failure(format!(
                    "healthy adoption evidence disagrees: {completed:?}"
                )));
            }
            close(&adopted)?;
            target_closed_hresult = Some(visible_hresult(&adopted));
            target_was_adopted = Some(adopted);
        } else {
            let expired_wait = matches!(
                &result,
                Err(backend::ControllerCreationError::Pump(error))
                    if error.to_string().contains(backend::LAUNCH_DEADLINE_EXPIRED)
            );
            // A Pump deadline plus call/return and no callback proves this was
            // accepted by the real SDK and abandoned by the production wait.
            // Entry expiry, SDK refusal and synchronous completion are ineligible.
            let accepted = facts();
            if !expired_wait
                || accepted.calls != 1
                || accepted.returns != 1
                || accepted.callbacks != 0
                || !accepted.completions.is_empty()
            {
                return Err(failure(format!("ineligible late request: {accepted:?}")));
            }
            // Freeze refusal before any later callback can supply another fault.
            let fault_before_callback = release.fault();
            let zero_release = backend::wait_for_browser_exit(&release);
            let zero_was_refused = matches!(
                &zero_release,
                Err(error) if error.to_string().contains("KELD-WV-009")
            );
            println!(
                "KELD_CUSTODY_ABANDONED mode={mode:?} accepted=true callbacks=0 fault={fault_before_callback:?} zero_refused={zero_was_refused}"
            );
            if matches!(mode, Mode::ParentCancelled) {
                // RecoveryWindow owns a same-thread synchronous DestroyWindow.
                // The SDK promises pending creation's completion before return.
                drop(target_window.take());
            } else {
                pump_until(deadline, None, || facts().callbacks != 0)?;
            }
            let completed = facts();
            // SAFETY: current-thread identity is a pure query.
            let thread = unsafe { GetCurrentThreadId() };
            let expected = match mode {
                Mode::LateSuccess => (0, true, thread),
                Mode::ParentCancelled => (E_ABORT.0, false, thread),
                Mode::Healthy => unreachable!(),
            };
            if completed.callbacks != 1 || completed.completions != [expected] {
                return Err(failure(format!(
                    "ineligible completion shape for {mode:?}: {completed:?}"
                )));
            }
            if matches!(mode, Mode::LateSuccess) {
                let controller = retained()
                    .ok_or_else(|| failure("native nonnull callback lost its retained witness"))?;
                target_closed_hresult = Some(visible_hresult(&controller));
                if completed.closes != [0] {
                    return Err(failure(format!(
                        "unclaimed successful callback must Close once: {completed:?}"
                    )));
                }
            } else if !completed.closes.is_empty() || is_window(hwnd) {
                return Err(failure(format!(
                    "cancelled null completion closed a nonexistent controller or retained HWND: {completed:?}"
                )));
            }
            if fault_before_callback != Some(backend::ProfileReleaseFault::RebindUnproven)
                || !zero_was_refused
                || release.fault() != fault_before_callback
                || release.observed.get()
            {
                return Err(failure(
                    "accepted unresolved request obtained zero-controller credit or lost its first fault",
                ));
            }
        }
        println!("KELD_CUSTODY_NATIVE mode={mode:?} facts={:?}", facts());
        Ok(())
    })();

    let committed_facts = facts();
    // Freeze all semantic observations before fallback cleanup. A no-Close
    // mutant retains a live real interface; this fixture closes it explicitly
    // before reporting the intended native-state failure.
    let cleanup = (|| -> Result<i32, WvError> {
        if let Some(controller) = retained()
            && visible_hresult(&controller) == 0
        {
            println!("KELD_CUSTODY_CLEANUP explicit-target-close=true");
            close(&controller)?;
        }
        drop(target_window.take());
        if target_hwnd.is_some_and(is_window) {
            return Err(failure("target HWND cleanup is unproved"));
        }
        let anchor = anchor
            .as_ref()
            .ok_or_else(|| failure("setup failed before a native anchor could be observed"))?;
        close(anchor)?;
        let closed_hresult = visible_hresult(anchor);
        let anchor_hwnd = anchor_window.as_ref().map(|window| window.0);
        drop(anchor_window.take());
        if anchor_hwnd.is_some_and(is_window) {
            return Err(failure("anchor HWND cleanup is unproved"));
        }
        let process = browser
            .as_ref()
            .ok_or_else(|| failure("browser cleanup lacks its captured native handle"))?;
        pump_until(deadline, Some(process), || {
            let receipt = CONTROLLER_CUSTODY_EVIDENCE
                .with_borrow(|e| e.browser_exits.contains(&(browser_pid, true)));
            receipt && process_state(process) == WAIT_OBJECT_0
        })?;
        println!(
            "KELD_CUSTODY_CLEANED mode={mode:?} pid={browser_pid} process_signaled=true native_normal_receipt=true target_hwnd=false anchor_hwnd=false"
        );
        Ok(closed_hresult)
    })();
    let final_facts = facts();
    drop(target_was_adopted.take());
    drop(observation);
    drop(target_window);
    drop(anchor);
    drop(anchor_window);
    drop(engine);
    if let Err(error) = &cleanup {
        return Err(failure(format!(
            "native custody cleanup unproved: {error}; outcome={outcome:?}; root={}",
            root.display()
        )));
    }
    std::fs::remove_dir_all(&root).map_err(failure)?;
    let anchor_closed_hresult = cleanup?;
    // Cleanup eligibility is independent of an earlier semantic RED. Preserve
    // that verdict only after callbacks and the native Close control stayed valid.
    if final_facts != committed_facts {
        return Err(failure(
            "controller callbacks or custody Close continued after the frozen verdict",
        ));
    }
    if anchor_closed_hresult >= 0 {
        return Err(failure(
            "ineligible: native Close control did not invalidate IsVisible",
        ));
    }
    Ok(outcome.and_then(|()| {
        if !matches!(mode, Mode::ParentCancelled)
            && target_closed_hresult != Some(anchor_closed_hresult)
        {
            return Err(failure(format!(
                "retained target did not reach the calibrated native closed state: target={target_closed_hresult:?} control={anchor_closed_hresult}"
            )));
        }
        Ok(())
    }))
}

#[test]
#[ignore = "real Windows accepted controller request completed after its wait is abandoned"]
fn windows_late_controller_completion_closes_once_without_zero_controller_credit()
-> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let verdict = case(Mode::LateSuccess)?;
        case(Mode::Healthy)??;
        verdict
    })
}

#[test]
#[ignore = "real Windows pending controller cancellation during parent DestroyWindow"]
fn windows_parent_destroy_finishes_cancelled_null_completion_without_zero_credit()
-> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let verdict = case(Mode::ParentCancelled)?;
        case(Mode::Healthy)??;
        verdict
    })
}
