//! Opt-in Windows fixture. This module is absent from default shipping builds.
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use keld_guard::{Decision, PermissionsManifest, Principal};
use tao::platform::windows::EventLoopBuilderExtWindows;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
    COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE,
    COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DEFAULT,
    COREWEBVIEW2_PERMISSION_STATE_DENY, ICoreWebView2PermissionRequestedEventArgs,
};
use webview2_com::WebMessageReceivedEventHandler;
use windows::Win32::Foundation::{E_UNEXPECTED, LPARAM, WPARAM};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_NULL};
use windows::core::PWSTR;

use super::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoreWebView2EnvironmentOptions, E_POINTER,
    EventLoopBuilder, NavTarget, PermissionRequestedEventHandler, WebEngine, WebView2Engine,
    WebviewSpec, WindowsLoopEvent, WvError, create_environment_with_options, matching_browser_exit,
    runtime_version, wait_with_pump, webview_media_principal,
};
use crate::{LogicalSize, media::manifest_fingerprint};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;

const FIXTURE_DEADLINE: Duration = Duration::from_mins(1);
const WATCHDOG_PROBE_DEADLINE: Duration = Duration::from_millis(100);
const PARENT_DEADLINE_MARGIN: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Evidence {
    active: bool,
    manifest: Option<PermissionsManifest>,
    registration: Option<(usize, i64)>,
    adapter_decisions: Vec<(u64, Option<Principal>, String, String, u32)>,
    effects: Vec<(i32, i32, i32, i32, String, usize, u32)>,
}

thread_local! {
    static EVIDENCE: RefCell<Evidence> = RefCell::new(Evidence::default());
}

pub(super) fn fixture_manifest() -> PermissionsManifest {
    EVIDENCE.with_borrow_mut(|e| e.manifest.take().unwrap_or_default())
}

pub(super) fn observe_registration(identity: usize, token: i64) {
    EVIDENCE.with_borrow_mut(|e| {
        if e.active {
            e.registration = Some((identity, token));
        }
    });
}

pub(crate) fn observe_adapter_decision(
    manifest: &PermissionsManifest,
    principal: Option<Principal>,
    capability: &str,
    decision: &Decision,
) {
    EVIDENCE.with_borrow_mut(|evidence| {
        if evidence.active {
            let code = match decision {
                Decision::Allow => "allow",
                Decision::Deny(reason) => reason.code(),
            };
            // SAFETY: querying the current thread needs no handles or initialization.
            let tid = unsafe { GetCurrentThreadId() };
            evidence.adapter_decisions.push((
                manifest_fingerprint(manifest),
                principal,
                capability.to_owned(),
                code.to_owned(),
                tid,
            ));
        }
    });
}

pub(super) fn observe_production_effect(
    kind: COREWEBVIEW2_PERMISSION_KIND,
    before: COREWEBVIEW2_PERMISSION_STATE,
    requested: COREWEBVIEW2_PERMISSION_STATE,
    after: COREWEBVIEW2_PERMISSION_STATE,
    uri: String,
    sender_identity: usize,
) {
    EVIDENCE.with_borrow_mut(|evidence| {
        if evidence.active {
            // SAFETY: the current-thread query has no preconditions.
            let tid = unsafe { GetCurrentThreadId() };
            evidence.effects.push((
                kind.0,
                before.0,
                requested.0,
                after.0,
                uri,
                sender_identity,
                tid,
            ));
        }
    });
}

pub(super) fn observe_browser_exit(actual: u32, expected: u32, normal: bool) {
    println!("KELD_MEDIA_BROWSER_EXIT pid={actual} expected={expected} normal={normal}");
}

pub(super) fn permission_uri(
    args: &ICoreWebView2PermissionRequestedEventArgs,
) -> windows::core::Result<String> {
    let mut pointer = PWSTR::null();
    // SAFETY: the callback owns live args and this is a writable LPWSTR output.
    unsafe { args.Uri(&raw mut pointer) }?;
    if pointer.is_null() {
        return Err(windows::core::Error::from(E_POINTER));
    }
    // SAFETY: successful Uri returned one COM-allocated NUL-terminated string.
    let value = unsafe { pointer.to_string() };
    // SAFETY: release that allocation exactly once after decoding it.
    unsafe { CoTaskMemFree(Some(pointer.as_ptr().cast())) };
    Ok(value?)
}

fn failure(detail: impl std::fmt::Display) -> WvError {
    WvError::Webview(format!("media acceptance: {detail}"))
}

fn send_and_wake<T>(sender: &mpsc::Sender<T>, value: T) -> windows::core::Result<()> {
    sender
        .send(value)
        .map_err(|_| windows::core::Error::from(E_UNEXPECTED))?;
    // SAFETY: callbacks run on the owning UI STA after tao created its queue.
    // GetMessage can dispatch a sent COM message without returning. A posted
    // WM_NULL makes wait_with_pump return to its receiver check even in that case.
    // https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmessagew
    unsafe { PostThreadMessageW(GetCurrentThreadId(), WM_NULL, WPARAM(0), LPARAM(0)) }
}

fn owned_string(pointer: PWSTR) -> Result<String, WvError> {
    if pointer.is_null() {
        return Err(failure("COM returned a null string"));
    }
    // SAFETY: callers pass successful WebView2 LPWSTR getter output. Decode before
    // releasing its single COM allocation, including on UTF-16 decode failure.
    let value = unsafe { pointer.to_string() };
    // SAFETY: this is the allocation returned by the successful getter above.
    unsafe { CoTaskMemFree(Some(pointer.as_ptr().cast())) };
    value.map_err(failure)
}

fn same_directory(actual: &str, expected: &Path) -> Result<(), WvError> {
    let actual = std::fs::canonicalize(actual).map_err(failure)?;
    let expected = std::fs::canonicalize(expected).map_err(failure)?;
    if actual != expected {
        return Err(failure(format!(
            "UDF override: actual={} expected={}",
            actual.display(),
            expected.display()
        )));
    }
    Ok(())
}

/// Runs one real `WebView2` media-callback case on the libtest-owned Windows UI thread.
///
/// Enabled only in the libtest executable by the non-default `media-acceptance`
/// development feature. `KELD_MEDIA_KIND`, `KELD_MEDIA_MODE`, and
/// `KELD_MEDIA_MANIFEST` select the row. Controls install no Keld adapter and must be
/// rejected by the same provenance oracle. Fake capture devices
/// are confined to a newly created, read-back-verified fixture profile. No fake-UI
/// flag is used; force-allow replaces only this fixture's callback to qualify its
/// synthetic devices. The fixture exits 124 if its process deadline expires.
/// This proves callback provenance, not profile
/// restart/revocation or absence of every transient platform prompt.
///
/// # Errors
/// Returns an error for missing runtime, API/identity/receipt failures, unexpected
/// JavaScript behavior, a control accepted by the oracle, or incomplete cleanup.
fn run_media_acceptance() -> Result<(), WvError> {
    let watchdog_probe = std::env::var("KELD_MEDIA_WATCHDOG_PROBE").ok();
    match watchdog_probe.as_deref() {
        Some("work") => run_with_watchdog(WATCHDOG_PROBE_DEADLINE, block_watchdog_probe),
        None => run_with_watchdog(FIXTURE_DEADLINE, run_media_acceptance_inner),
        Some(_) => Err(failure("unknown watchdog probe mode")),
    }
}

fn run_with_watchdog<T>(
    deadline: Duration,
    work: impl FnOnce() -> Result<T, WvError>,
) -> Result<T, WvError> {
    let (done_tx, watchdog) = spawn_watchdog(deadline);
    let result = work();
    let _ = done_tx.send(());
    watchdog.join().map_err(|_| failure("watchdog panicked"))?;
    result
}

fn block_watchdog_probe() -> Result<(), WvError> {
    println!("KELD_MEDIA_PHASE watchdog-probe-work-block");
    let _ = std::io::stdout().flush();
    let (_block_tx, block_rx) = mpsc::channel::<()>();
    let _ = block_rx.recv();
    Err(failure("watchdog probe returned before process exit"))
}

fn run_media_acceptance_inner() -> Result<(), WvError> {
    let parent_deadline = std::env::var("KELD_MEDIA_PARENT_DEADLINE_MS")
        .map_err(|_| failure("KELD_MEDIA_PARENT_DEADLINE_MS is required"))?;
    if !parent_deadline_is_valid(&parent_deadline) {
        return Err(failure(format!(
            "parent deadline must be at least {} milliseconds",
            (FIXTURE_DEADLINE + PARENT_DEADLINE_MARGIN).as_millis()
        )));
    }
    let kind = std::env::var("KELD_MEDIA_KIND")
        .map_err(|_| failure("KELD_MEDIA_KIND must be camera or microphone"))?;
    let mode = std::env::var("KELD_MEDIA_MODE").map_err(|_| {
        failure("KELD_MEDIA_MODE must be guarded, removed-guard, adapter-bypass, or force-allow")
    })?;
    let (constraints, track_kind) = match kind.as_str() {
        "camera" => ("{video:true}", "video"),
        "microphone" => ("{audio:true}", "audio"),
        _ => return Err(failure("unknown media kind")),
    };
    if !matches!(
        mode.as_str(),
        "guarded" | "removed-guard" | "adapter-bypass" | "force-allow"
    ) {
        return Err(failure("unknown control mode"));
    }
    let manifest_case = std::env::var("KELD_MEDIA_MANIFEST")
        .map_err(|_| failure("KELD_MEDIA_MANIFEST must be empty or app-grants"))?;
    let manifest = match manifest_case.as_str() {
        "empty" => PermissionsManifest::default(),
        "app-grants" => {
            keld_guard::parse_manifest(r#"{"app":{"web":{"camera":["*"],"microphone":["*"]}}}"#)
                .map_err(failure)?
        }
        _ => return Err(failure("unknown manifest case")),
    };
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(failure)?
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("keld-media-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).map_err(failure)?;
    println!("KELD_MEDIA_PROFILE {}", directory.display());
    let result = run_case(
        &kind,
        &mode,
        constraints,
        track_kind,
        nonce,
        &directory,
        manifest,
    );
    // run_case must finish the browser-release barrier before returning success.
    // On error preserve the exact directory for diagnosis; never retry removal.
    if result.is_ok() {
        std::fs::remove_dir_all(&directory).map_err(failure)?;
    } else {
        eprintln!("KELD_MEDIA_RETAINED {}", directory.display());
    }
    result
}

fn blank_spec() -> WebviewSpec {
    WebviewSpec {
        title: "Keld media acceptance".to_owned(),
        initial: NavTarget::Html("<!doctype html><title>setup</title>".to_owned()),
        size: LogicalSize {
            width: 640.0,
            height: 480.0,
        },
    }
}

fn media_spec(url: String) -> WebviewSpec {
    WebviewSpec {
        title: "Keld media acceptance".to_owned(),
        initial: NavTarget::Url(url),
        size: LogicalSize {
            width: 640.0,
            height: 480.0,
        },
    }
}

fn destroy_primers(
    engine: &mut WebView2Engine,
    count: usize,
) -> Result<Option<crate::WebviewId>, WvError> {
    let blank = blank_spec();
    let mut last = None;
    for _ in 0..count {
        let primer = engine.create(&blank)?;
        engine.destroy(primer)?;
        if !matches!(
            engine.navigate(primer, NavTarget::Html(String::new())),
            Err(WvError::UnknownWebview { .. })
        ) {
            return Err(failure("destroyed view remained navigable"));
        }
        println!("KELD_MEDIA_PHASE primer-destroyed id={}", primer.0);
        last = Some(primer);
    }
    Ok(last)
}

fn spawn_watchdog(deadline: Duration) -> (mpsc::Sender<()>, thread::JoinHandle<()>) {
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let watchdog = thread::spawn(move || {
        if done_rx.recv_timeout(deadline) == Err(mpsc::RecvTimeoutError::Timeout) {
            // This opt-in fixture is a disposable process. A process deadline also
            // bounds nested creation/cleanup pumps; one consumed WM_QUIT cannot do so.
            eprintln!(
                "KELD_MEDIA_TIMEOUT: fixture exceeded {} seconds; inspect any KELD_MEDIA_PROFILE receipt",
                deadline.as_secs_f64()
            );
            let _ = std::io::stderr().flush();
            std::process::exit(124);
        }
    });
    (done_tx, watchdog)
}

fn parent_deadline_is_valid(value: &str) -> bool {
    value.parse::<u64>().is_ok_and(|milliseconds| {
        Duration::from_millis(milliseconds) >= FIXTURE_DEADLINE + PARENT_DEADLINE_MARGIN
    })
}

fn run_case(
    kind: &str,
    mode: &str,
    constraints: &str,
    track_kind: &str,
    nonce: u128,
    directory: &Path,
    manifest: PermissionsManifest,
) -> Result<(), WvError> {
    let runtime = runtime_version()?;
    let mut event_loop = EventLoopBuilder::<WindowsLoopEvent>::with_user_event();
    // The ignored libtest entrypoint runs on a harness worker. Windows supports
    // a dedicated UI/message thread; every COM object and callback remains on
    // this same fixture thread.
    event_loop.with_any_thread(true);
    let event_loop = event_loop.build();
    // SAFETY: all COM work is confined to this fixture-owned UI STA.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(failure)?;
    // SAFETY: thread ID query is unconditional.
    let tid = unsafe { GetCurrentThreadId() };
    let environment = fixture_environment(directory)?;
    let expected_browser_pid = Arc::new(AtomicU32::new(0));
    let browser_exit =
        super::observe_profile_browser_exit(&environment, Arc::clone(&expected_browser_pid), None)?;
    println!("KELD_MEDIA_PHASE environment-ready");
    let mut engine = WebView2Engine::from_environment(event_loop, environment.clone());
    let primer_count = if kind == "camera" { 1 } else { 2 };
    let last_primer = destroy_primers(&mut engine, primer_count)?;
    println!("KELD_MEDIA_PHASE primers-destroyed count={primer_count}");
    let listener = TcpListener::bind("127.0.0.1:0").map_err(failure)?;
    let address = listener.local_addr().map_err(failure)?;
    let url = format!("http://{address}/{nonce}/");
    let expected_origin = format!("http://{address}/");
    let html = format!(
        r"<!doctype html><script>
(async()=>{{await fetch('/{nonce}/release',{{cache:'no-store'}});
try{{const stream=await navigator.mediaDevices.getUserMedia({constraints});
const matching=stream.getTracks().filter(track=>track.kind==='{track_kind}'&&track.readyState==='live');
if(matching.length===0)throw new DOMException('no requested live track','KeldNoLiveTrack');
for(const track of stream.getTracks())track.stop();
chrome.webview.postMessage('{nonce}:resolved:{track_kind}:'+matching.length+':'+matching.every(track=>track.readyState==='ended'));
}}catch(error){{chrome.webview.postMessage('{nonce}:'+window.isSecureContext+':'+error.name)}}}})();
</script>"
    );
    let (release_tx, release_rx) = mpsc::channel();
    let server = serve_page(listener, html, nonce, release_rx);
    let expected_manifest = manifest_fingerprint(&manifest);
    EVIDENCE.with_borrow_mut(|e| {
        *e = Evidence {
            active: true,
            manifest: Some(manifest),
            ..Evidence::default()
        };
    });
    // Use the production constructor's first navigation. The page blocks on
    // its nonce-bound release request until the independent observers exist.
    let media_id = engine.create(&media_spec(url.clone()))?;
    if last_primer == Some(media_id) {
        return Err(failure("view id was reused"));
    }
    let principal = webview_media_principal(media_id);
    let view = engine.view(media_id)?;
    let mut browser_pid = 0;
    // SAFETY: live view and writable process id on the creating STA.
    unsafe { view.webview.BrowserProcessId(&raw mut browser_pid) }.map_err(failure)?;
    println!(
        "KELD_MEDIA_VIEW id={} browser_pid={browser_pid}",
        media_id.0
    );
    expected_browser_pid.store(browser_pid, Ordering::Release);
    let registration = EVIDENCE.with_borrow(|e| e.registration);
    let Some((identity, token)) = registration else {
        return Err(failure("production constructor did not register media"));
    };
    if identity != super::canonical_webview_identity(&view.webview).map_err(failure)? {
        return Err(failure("registration belongs to another view"));
    }
    if mode != "guarded" {
        // SAFETY: token was recorded from this live view's actual registration.
        unsafe { view.webview.remove_PermissionRequested(token) }.map_err(failure)?;
    }
    let message_rx = observe_request(&view.webview, mode, &url)?;
    println!("KELD_MEDIA_PHASE observers-ready");
    release_tx
        .send(())
        .map_err(|_| failure("HTTP release receiver closed"))?;
    let result = wait_with_pump(message_rx).map_err(failure)?;
    println!("KELD_MEDIA_PROMISE {result:?}");
    let server_result = server.join().map_err(|_| failure("HTTP server panicked"))?;
    engine.destroy(media_id)?;
    println!("KELD_MEDIA_CLOSED waiting_for={browser_pid}");
    drop(engine);
    wait_with_pump(browser_exit.receiver).map_err(failure)??;
    super::remove_browser_exit_observer(&browser_exit.environment, browser_exit.token)?;
    drop(browser_exit.environment);
    drop(environment);
    server_result.map_err(failure)?;
    validate(Validation {
        kind,
        mode,
        nonce,
        principal,
        tid,
        manifest: expected_manifest,
        origin: &expected_origin,
        registration_identity: identity,
        view_id: media_id.0,
        browser_pid,
        result: &result?,
        runtime: &runtime,
    })
}

#[derive(Clone, Copy)]
struct Validation<'a> {
    kind: &'a str,
    mode: &'a str,
    nonce: u128,
    principal: Principal,
    tid: u32,
    manifest: u64,
    origin: &'a str,
    registration_identity: usize,
    view_id: u32,
    browser_pid: u32,
    result: &'a str,
    runtime: &'a str,
}

fn validate(input: Validation<'_>) -> Result<(), WvError> {
    let Validation {
        kind,
        mode,
        nonce,
        principal,
        tid,
        manifest: expected_manifest,
        origin: expected_origin,
        registration_identity,
        view_id,
        browser_pid,
        result,
        runtime,
    } = input;
    let evidence = EVIDENCE.with_borrow_mut(std::mem::take);
    let capability = format!("web.{kind}");
    let expected_kind = if kind == "camera" {
        COREWEBVIEW2_PERMISSION_KIND_CAMERA.0
    } else {
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE.0
    };
    let manifest_ok =
        evidence.adapter_decisions.first().map(|row| row.0) == Some(expected_manifest);
    let adapter_ok = evidence.adapter_decisions
        == vec![(
            expected_manifest,
            Some(principal),
            capability,
            "KELD-GUARD006".to_owned(),
            tid,
        )];
    let effect_ok = evidence.effects
        == vec![(
            expected_kind,
            COREWEBVIEW2_PERMISSION_STATE_DEFAULT.0,
            COREWEBVIEW2_PERMISSION_STATE_DENY.0,
            COREWEBVIEW2_PERMISSION_STATE_DENY.0,
            expected_origin.to_owned(),
            registration_identity,
            tid,
        )];
    let origin_ok = evidence.effects.len() == 1 && evidence.effects[0].4 == expected_origin;
    let denial_js_ok = result == format!("{nonce}:true:NotAllowedError");
    let accepted = manifest_ok && adapter_ok && effect_ok && origin_ok && denial_js_ok;
    let control_ok = mode != "guarded"
        && evidence.adapter_decisions.is_empty()
        && control_matches(
            ControlExpectation {
                mode,
                nonce,
                result,
                kind: expected_kind,
                origin: expected_origin,
                registration_identity,
                tid,
            },
            &evidence.effects,
        );
    let js_ok = if mode == "force-allow" {
        allow_result_matches(expected_kind, nonce, result)
    } else {
        denial_js_ok
    };
    let case_ok = accepted || control_ok;
    let (observed_kind, before, requested, after, origin_uri, sender_identity) = evidence
        .effects
        .first()
        .map_or((-1, -1, -1, -1, "missing", 0), |effect| {
            (
                effect.0,
                effect.1,
                effect.2,
                effect.3,
                effect.4.as_str(),
                effect.5,
            )
        });
    let (adapter_principal, adapter_capability, adapter_decision, adapter_tid) =
        observed_adapter(&evidence);
    println!(
        "KELD_MEDIA_RESULT kind={kind} mode={mode} nonce={nonce} runtime={runtime} host_pid={} browser_pid={browser_pid} view_id={view_id} tid={tid} manifest_fnv1a64={expected_manifest:016x} adapter_principal={adapter_principal} adapter_capability={adapter_capability} adapter_decision={adapter_decision} adapter_tid={adapter_tid} permission_kind={observed_kind} before={before} requested={requested} after={after} origin_uri={origin_uri} registration_identity={registration_identity:x} sender_identity={sender_identity:x} outcome={result} manifest={manifest_ok} adapter={adapter_ok} effect={effect_ok} origin={origin_ok} js={js_ok} control={control_ok} accepted={accepted} case_ok={case_ok}",
        std::process::id()
    );
    if mode == "guarded" && !accepted {
        return Err(failure(format!(
            "guarded oracle failed; js={result}; adapter_decisions={:?}; effects={:?}",
            evidence.adapter_decisions, evidence.effects
        )));
    }
    if mode != "guarded" && !control_ok {
        return Err(failure(
            "negative control did not isolate a missing adapter invocation",
        ));
    }
    Ok(())
}

fn observed_adapter(evidence: &Evidence) -> (String, &str, &str, u32) {
    evidence.adapter_decisions.first().map_or(
        (String::from("none"), "none", "none", 0),
        |decision| {
            let principal = match decision.1 {
                Some(Principal::Webview { id, generation }) => {
                    format!("webview:{id}:{generation}")
                }
                Some(Principal::AppProcess) => String::from("app"),
                Some(Principal::Plugin { id }) => format!("plugin:{id}"),
                None => String::from("none"),
            };
            (
                principal,
                decision.2.as_str(),
                decision.3.as_str(),
                decision.4,
            )
        },
    )
}

#[derive(Clone, Copy)]
struct ControlExpectation<'a> {
    mode: &'a str,
    nonce: u128,
    result: &'a str,
    kind: i32,
    origin: &'a str,
    registration_identity: usize,
    tid: u32,
}

fn control_matches(
    expected: ControlExpectation<'_>,
    effects: &[(i32, i32, i32, i32, String, usize, u32)],
) -> bool {
    let state = match expected.mode {
        "force-allow" => COREWEBVIEW2_PERMISSION_STATE_ALLOW,
        _ => COREWEBVIEW2_PERMISSION_STATE_DENY,
    };
    let js_ok = if expected.mode == "force-allow" {
        allow_result_matches(expected.kind, expected.nonce, expected.result)
    } else {
        expected.result == format!("{}:true:NotAllowedError", expected.nonce)
    };
    effects
        == [(
            expected.kind,
            COREWEBVIEW2_PERMISSION_STATE_DEFAULT.0,
            state.0,
            state.0,
            expected.origin.to_owned(),
            expected.registration_identity,
            expected.tid,
        )]
        && js_ok
}

fn allow_result_matches(kind: i32, nonce: u128, result: &str) -> bool {
    let track_kind = if kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA.0 {
        "video"
    } else if kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE.0 {
        "audio"
    } else {
        return false;
    };
    let prefix = format!("{nonce}:resolved:{track_kind}:");
    let Some(tail) = result.strip_prefix(&prefix) else {
        return false;
    };
    let Some((count, ended)) = tail.split_once(':') else {
        return false;
    };
    count.parse::<usize>().is_ok_and(|count| count > 0) && ended == "true"
}

fn observe_request(
    view: &ICoreWebView2,
    mode: &str,
    expected_url: &str,
) -> Result<mpsc::Receiver<Result<String, WvError>>, WvError> {
    let mut control_token = 0;
    if mode != "guarded" {
        let registered_identity = super::canonical_webview_identity(view).map_err(failure)?;
        let control_state = match mode {
            "force-allow" => COREWEBVIEW2_PERMISSION_STATE_ALLOW,
            "adapter-bypass" => super::webview2_permission_state(false),
            // Fixture-only completion after observing DEFAULT. The product
            // guard is absent; this callback prevents an unattended prompt
            // from becoming synchronization or acceptance evidence.
            _ => COREWEBVIEW2_PERMISSION_STATE_DENY,
        };
        // Negative controls use one fixture callback after the production
        // registration is removed. It records DEFAULT, applies the control
        // state, and reads it back before returning, so no event-handler order
        // assumption contributes to the oracle.
        // SAFETY: live view and event arguments on the UI STA.
        unsafe {
            view.add_PermissionRequested(
                &PermissionRequestedEventHandler::create(Box::new(move |sender, args| {
                    let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                    let Ok(Some(sender_identity)) = sender
                        .as_ref()
                        .map(super::canonical_webview_identity)
                        .transpose()
                    else {
                        return args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY);
                    };
                    if sender_identity != registered_identity {
                        return args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY);
                    }
                    let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                    let mut before = COREWEBVIEW2_PERMISSION_STATE_DEFAULT;
                    let mut after = COREWEBVIEW2_PERMISSION_STATE_DEFAULT;
                    args.PermissionKind(&raw mut kind)?;
                    args.State(&raw mut before)?;
                    args.SetState(control_state)?;
                    args.State(&raw mut after)?;
                    let uri = permission_uri(&args)?;
                    println!(
                        "KELD_MEDIA_CONTROL kind={} before={} requested={} after={} uri={uri}",
                        kind.0, before.0, control_state.0, after.0
                    );
                    // SAFETY: current-thread query has no preconditions.
                    let tid = GetCurrentThreadId();
                    EVIDENCE.with_borrow_mut(|e| {
                        e.effects.push((
                            kind.0,
                            before.0,
                            control_state.0,
                            after.0,
                            uri,
                            sender_identity,
                            tid,
                        ));
                    });
                    Ok(())
                })),
                &raw mut control_token,
            )
        }
        .map_err(failure)?;
    }
    let expected_url = expected_url.to_owned();
    let (message_tx, message_rx) = mpsc::channel();
    let mut message_token = 0;
    // SAFETY: view remains live until message completion and controller shutdown.
    unsafe {
        view.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                println!("KELD_MEDIA_MESSAGE_RECEIVED");
                let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                let mut source = PWSTR::null();
                args.Source(&raw mut source)?;
                let source = owned_string(source);
                let mut value = PWSTR::null();
                args.TryGetWebMessageAsString(&raw mut value)?;
                let result = source.and_then(|source| {
                    if source == expected_url {
                        owned_string(value)
                    } else {
                        // The string still belongs to this callback on mismatch.
                        let _ = owned_string(value);
                        Err(failure("message came from the wrong source"))
                    }
                });
                send_and_wake(&message_tx, result)?;
                Ok(())
            })),
            &raw mut message_token,
        )
    }
    .map_err(failure)?;
    Ok(message_rx)
}

fn fixture_environment(directory: &Path) -> Result<super::ICoreWebView2Environment, WvError> {
    let options = fixture_environment_options();
    let environment = create_environment_with_options(directory, options)?;
    let actual = super::environment_user_data_folder(&environment)?;
    let actual = actual
        .to_str()
        .ok_or_else(|| failure("actual user-data folder is not Unicode"))?;
    same_directory(actual, directory)?;
    Ok(environment)
}

fn fixture_environment_options() -> CoreWebView2EnvironmentOptions {
    let options = CoreWebView2EnvironmentOptions::default();
    // SAFETY: options is not shared or published yet. This documented development
    // switch supplies devices without changing the permission-request decision.
    // https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/webview-features-flags
    unsafe {
        options.set_additional_browser_arguments("--use-fake-device-for-media-stream".to_owned());
    };
    options
}

fn serve_page(
    listener: TcpListener,
    html: String,
    nonce: u128,
    release: mpsc::Receiver<()>,
) -> thread::JoinHandle<std::io::Result<()>> {
    thread::spawn(move || -> std::io::Result<()> {
        let page = format!("/{nonce}/");
        let gate = format!("/{nonce}/release");
        let mut served_page = false;
        loop {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            stream.set_write_timeout(Some(Duration::from_secs(3)))?;
            let mut request = [0_u8; 4096];
            let count = stream.read(&mut request)?;
            let first = std::str::from_utf8(&request[..count])
                .ok()
                .and_then(|request| request.lines().next())
                .unwrap_or_default();
            let path = first
                .strip_prefix("GET ")
                .and_then(|rest| rest.split_once(' '))
                .map(|(path, _)| path)
                .unwrap_or_default();
            if path == page {
                served_page = true;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                    html.len()
                )?;
            } else if path == gate && served_page {
                release
                    .recv_timeout(Duration::from_secs(10))
                    .map_err(std::io::Error::other)?;
                write!(
                    stream,
                    "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )?;
                return Ok(());
            } else {
                write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )?;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_fixture_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("fixture clock")
            .as_nanos();
        super::super::known_local_app_data()
            .expect("resolve fixture LocalAppData")
            .join("Keld")
            .join("test-fixtures")
            .join(format!(
                "keld-profile-acceptance-{label}-{}-{nonce}",
                std::process::id()
            ))
    }

    fn profile_fixture_engine(
        root: &Path,
        selection: crate::profile::WebProfileSelection,
    ) -> Result<WebView2Engine, WvError> {
        profile_fixture_engine_with_options(
            root,
            selection,
            CoreWebView2EnvironmentOptions::default(),
        )
    }

    fn profile_fixture_engine_with_options(
        root: &Path,
        selection: crate::profile::WebProfileSelection,
        options: CoreWebView2EnvironmentOptions,
    ) -> Result<WebView2Engine, WvError> {
        runtime_version()?;
        let profile = super::super::prepare_profile_before_event_loop(root, selection)?;
        let mut builder = EventLoopBuilder::<WindowsLoopEvent>::with_user_event();
        builder.with_any_thread(true).with_dpi_aware(false);
        let event_loop = builder.build();
        let environment =
            super::super::create_environment_for_profile_with_options(&profile, options)?;
        WebView2Engine::from_selected_environment(event_loop, environment, profile)
    }

    fn run_profile_fixture(mut engine: WebView2Engine) -> Result<(), WvError> {
        let (events_tx, _events_rx) = mpsc::channel();
        engine.create_app(&blank_spec(), events_tx)?;
        let (commands_tx, commands_rx) = mpsc::channel();
        commands_tx
            .send(crate::AppWindowCommand::Quit)
            .map_err(|_| failure("queue fixture Quit"))?;
        engine.run_app_until_quit(commands_rx, mpsc::channel().0)
    }

    fn assert_saved_media_state(
        profile: &super::super::ICoreWebView2Profile4,
        origin: &str,
        expected: COREWEBVIEW2_PERMISSION_STATE,
    ) -> Result<(), WvError> {
        let settings = super::super::profile_permission_settings(profile)?;
        for kind in [
            COREWEBVIEW2_PERMISSION_KIND_CAMERA,
            COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        ] {
            if !settings.iter().any(|setting| {
                setting.kind == kind && setting.origin == origin && setting.state == expected
            }) {
                return Err(failure(format!(
                    "saved media setting kind={} did not read back state={}",
                    kind.0, expected.0
                )));
            }
        }
        Ok(())
    }

    fn assert_saved_permission_state(
        profile: &super::super::ICoreWebView2Profile4,
        origin: &str,
        kind: COREWEBVIEW2_PERMISSION_KIND,
        expected: COREWEBVIEW2_PERMISSION_STATE,
    ) -> Result<(), WvError> {
        let settings = super::super::profile_permission_settings(profile)?;
        if settings.iter().any(|setting| {
            setting.kind == kind && setting.origin == origin && setting.state == expected
        }) {
            Ok(())
        } else {
            Err(failure(format!(
                "saved media setting kind={} did not read back state={}",
                kind.0, expected.0
            )))
        }
    }

    fn assert_no_saved_permission(
        profile: &super::super::ICoreWebView2Profile4,
        origin: &str,
        kind: COREWEBVIEW2_PERMISSION_KIND,
    ) -> Result<(), WvError> {
        if super::super::profile_permission_settings(profile)?
            .iter()
            .any(|setting| setting.kind == kind && setting.origin == origin)
        {
            Err(failure("control unexpectedly inherited a saved permission"))
        } else {
            Ok(())
        }
    }

    fn saved_grant_root(run_id: &str) -> Result<std::path::PathBuf, WvError> {
        if run_id.len() != 32
            || !run_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(failure("saved-grant run id must be 32 lowercase hex bytes"));
        }
        Ok(super::super::known_local_app_data()?
            .join("Keld")
            .join("test-fixtures")
            .join(format!("saved-grant-{run_id}")))
    }

    fn saved_grant_result_matches(result: &str, nonce: u128, phase: &str, track: &str) -> bool {
        let prior = if matches!(phase, "deny" | "control") {
            "persisted"
        } else {
            "none"
        };
        if matches!(
            phase,
            "deny" | "replaced-profile" | "changed-origin" | "omitted-seed" | "dev-fresh"
        ) {
            result == format!("{nonce}:{phase}:{prior}:error:NotAllowedError")
        } else {
            result.starts_with(&format!("{nonce}:{phase}:{prior}:resolved:{track}:"))
                && result.ends_with(":true")
        }
    }

    struct SavedGrantCase {
        phase: String,
        kind_name: String,
        kind: COREWEBVIEW2_PERMISSION_KIND,
        constraints: &'static str,
        track: &'static str,
        root: std::path::PathBuf,
        nonce: u128,
        address: String,
    }

    fn load_saved_grant_case() -> Result<SavedGrantCase, WvError> {
        let phase = std::env::var("KELD_PROFILE_SAVED_PHASE")
            .map_err(|_| failure("KELD_PROFILE_SAVED_PHASE is required"))?;
        if !matches!(
            phase.as_str(),
            "seed"
                | "deny"
                | "control"
                | "replaced-profile"
                | "changed-origin"
                | "omitted-seed"
                | "dev-seed"
                | "dev-fresh"
        ) {
            return Err(failure("unknown saved-grant phase"));
        }
        let kind_name = std::env::var("KELD_PROFILE_SAVED_KIND")
            .map_err(|_| failure("KELD_PROFILE_SAVED_KIND is required"))?;
        let (kind, constraints, track) = match kind_name.as_str() {
            "camera" => (COREWEBVIEW2_PERMISSION_KIND_CAMERA, "{video:true}", "video"),
            "microphone" => (
                COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
                "{audio:true}",
                "audio",
            ),
            _ => return Err(failure("saved kind must be camera or microphone")),
        };
        let run_id = std::env::var("KELD_PROFILE_SAVED_RUN_ID")
            .map_err(|_| failure("KELD_PROFILE_SAVED_RUN_ID is required"))?;
        Ok(SavedGrantCase {
            root: saved_grant_root(&run_id)?,
            nonce: u128::from_str_radix(&run_id, 16).map_err(failure)?,
            address: std::env::var("KELD_PROFILE_SAVED_ADDRESS")
                .map_err(|_| failure("KELD_PROFILE_SAVED_ADDRESS is required"))?,
            phase,
            kind_name,
            kind,
            constraints,
            track,
        })
    }

    struct SavedGrantPage {
        url: String,
        origin: String,
        release: mpsc::Sender<()>,
        server: thread::JoinHandle<std::io::Result<()>>,
    }

    fn start_saved_grant_page(case: &SavedGrantCase) -> Result<SavedGrantPage, WvError> {
        let listener = TcpListener::bind(&case.address).map_err(failure)?;
        let address = listener.local_addr().map_err(failure)?;
        let url = format!("http://{address}/{}/", case.nonce);
        let html = format!(
            r"<!doctype html><script>
(async()=>{{await fetch('/{nonce}/release',{{cache:'no-store'}});
const prior=localStorage.getItem('keld-profile-nonce')||'none';
if('{phase}'==='seed'||'{phase}'==='dev-seed')localStorage.setItem('keld-profile-nonce','persisted');
try{{const stream=await navigator.mediaDevices.getUserMedia({constraints});
const matching=stream.getTracks().filter(track=>track.kind==='{track}'&&track.readyState==='live');
for(const item of stream.getTracks())item.stop();
chrome.webview.postMessage('{nonce}:{phase}:'+prior+':resolved:{track}:'+matching.length+':'+matching.every(track=>track.readyState==='ended'));
}}catch(error){{chrome.webview.postMessage('{nonce}:{phase}:'+prior+':error:'+error.name);}}}})();
</script>",
            nonce = case.nonce,
            phase = case.phase,
            constraints = case.constraints,
            track = case.track,
        );
        let (release, release_rx) = mpsc::channel();
        Ok(SavedGrantPage {
            url,
            origin: format!("http://{address}/"),
            release,
            server: serve_page(listener, html, case.nonce, release_rx),
        })
    }

    struct PreparedSavedGrant {
        profile: super::super::ICoreWebView2Profile4,
        permission: super::super::SavedPermission,
    }

    fn prepare_saved_grant_permission(
        engine: &WebView2Engine,
        view_id: crate::WebviewId,
        case: &SavedGrantCase,
        origin: &str,
    ) -> Result<PreparedSavedGrant, WvError> {
        let view = engine.view(view_id)?;
        let profile = super::super::webview_profile(&view.webview)?;
        let permission = super::super::SavedPermission {
            kind: case.kind,
            state: COREWEBVIEW2_PERMISSION_STATE_ALLOW,
            origin: origin.to_owned(),
        };
        let registration = EVIDENCE
            .with_borrow(|evidence| evidence.registration)
            .ok_or_else(|| failure("saved-grant view has no production registration"))?;
        let expected = match case.phase.as_str() {
            "seed" | "control" | "dev-seed" => Some(COREWEBVIEW2_PERMISSION_STATE_ALLOW),
            "deny" => Some(COREWEBVIEW2_PERMISSION_STATE_DENY),
            "replaced-profile" | "changed-origin" | "omitted-seed" | "dev-fresh" => None,
            _ => return Err(failure("unreachable saved phase")),
        };
        if matches!(case.phase.as_str(), "seed" | "dev-seed") {
            super::super::set_profile_permission_state(
                &profile,
                &permission,
                COREWEBVIEW2_PERMISSION_STATE_ALLOW,
            )?;
        }
        if let Some(expected) = expected {
            assert_saved_permission_state(&profile, origin, case.kind, expected)?;
        } else {
            assert_no_saved_permission(&profile, origin, case.kind)?;
        }
        if matches!(case.phase.as_str(), "seed" | "dev-seed") {
            // SAFETY: this token came from the production registration on
            // this exact live view; the seed needs the stored Allow oracle.
            unsafe { view.webview.remove_PermissionRequested(registration.1) }.map_err(failure)?;
        }
        Ok(PreparedSavedGrant {
            profile,
            permission,
        })
    }

    fn run_saved_grant_phase() -> Result<(), WvError> {
        use crate::profile::{ProfileIdentity, WebProfileSelection};

        let case = load_saved_grant_case()?;
        if matches!(
            case.phase.as_str(),
            "seed" | "omitted-seed" | "dev-seed" | "dev-fresh"
        ) {
            std::fs::create_dir_all(&case.root).map_err(failure)?;
        } else if !case.root.is_dir() {
            return Err(failure("saved-grant persistent fixture is missing"));
        }
        let page = start_saved_grant_page(&case)?;
        let selection = match case.phase.as_str() {
            "dev-seed" => WebProfileSelection::ephemeral_dev(
                crate::profile::EphemeralProfile::from_host_random([24; 32])?,
            ),
            "dev-fresh" => WebProfileSelection::ephemeral_dev(
                crate::profile::EphemeralProfile::from_host_random([25; 32])?,
            ),
            _ => {
                let publisher = if case.phase == "replaced-profile" {
                    [22; 32]
                } else {
                    [21; 32]
                };
                WebProfileSelection::Persistent(ProfileIdentity::from_host_verified_parts(
                    publisher,
                    &format!("dev.keld.synthetic-saved-{}", case.kind_name),
                )?)
            }
        };
        let mut engine = profile_fixture_engine_with_options(
            &case.root,
            selection,
            fixture_environment_options(),
        )?;
        EVIDENCE.with_borrow_mut(|evidence| {
            *evidence = Evidence {
                active: true,
                manifest: Some(PermissionsManifest::default()),
                ..Evidence::default()
            };
        });
        let view_id = engine.create(&blank_spec())?;
        let origin = page.origin.clone();
        let prepared = prepare_saved_grant_permission(&engine, view_id, &case, &origin)?;
        let view = engine.view(view_id)?;
        let result_rx = observe_request(&view.webview, "guarded", &page.url)?;
        page.release
            .send(())
            .map_err(|_| failure("release saved-grant page"))?;
        engine.navigate(view_id, NavTarget::Url(page.url))?;
        let result = wait_with_pump(result_rx).map_err(failure)??;
        page.server
            .join()
            .map_err(|_| failure("saved server panicked"))?
            .map_err(failure)?;
        if !saved_grant_result_matches(&result, case.nonce, &case.phase, case.track) {
            return Err(failure(format!(
                "saved-grant {} oracle rejected result {result}",
                case.phase
            )));
        }
        if case.phase == "deny" {
            super::super::set_profile_permission_state(
                &prepared.profile,
                &prepared.permission,
                COREWEBVIEW2_PERMISSION_STATE_ALLOW,
            )?;
            assert_saved_permission_state(
                &prepared.profile,
                &origin,
                case.kind,
                COREWEBVIEW2_PERMISSION_STATE_ALLOW,
            )?;
        }
        println!(
            "KELD_PROFILE_SAVED_RESULT kind={} phase={} origin={origin} result={result}",
            case.kind_name, case.phase
        );
        drop(prepared.profile);
        let (commands_tx, commands_rx) = mpsc::channel();
        commands_tx
            .send(crate::AppWindowCommand::Quit)
            .map_err(|_| failure("queue saved fixture Quit"))?;
        engine.run_app_until_quit(commands_rx, mpsc::channel().0)?;
        if std::env::var_os("KELD_PROFILE_SAVED_CLEANUP").is_some() {
            std::fs::remove_dir_all(case.root).map_err(failure)?;
        }
        Ok(())
    }

    #[test]
    #[ignore = "real Windows WebView2 dev-profile cleanup"]
    fn windows_dev_profile_cleanup_subprocess() -> Result<(), WvError> {
        run_with_watchdog(FIXTURE_DEADLINE, || {
            let root = profile_fixture_root("dev");
            std::fs::create_dir_all(&root).map_err(failure)?;
            let old = crate::profile::EphemeralProfile::from_host_random([12; 32])?;
            let old_selection = crate::profile::WebProfileSelection::ephemeral_dev(old);
            let old_plan = super::super::windows_profile_plan(&root, old_selection)?;
            drop(super::super::prepare_windows_profile_at(
                &root,
                old_selection,
            )?);
            let ephemeral = crate::profile::EphemeralProfile::from_host_random([13; 32])?;
            let selection = crate::profile::WebProfileSelection::ephemeral_dev(ephemeral);
            let plan = super::super::windows_profile_plan(&root, selection)?;
            let result = profile_fixture_engine(&root, selection).and_then(run_profile_fixture);
            if result.is_ok() {
                if plan.control_dir.exists() || old_plan.control_dir.exists() {
                    return Err(failure(
                        "graceful cleanup or production stale-leaf scavenging was incomplete",
                    ));
                }
                std::fs::remove_dir_all(&root).map_err(failure)?;
            } else {
                eprintln!("KELD_PROFILE_RETAINED {}", root.display());
            }
            result
        })
    }

    #[test]
    #[ignore = "real production-path busy ephemeral retention"]
    fn windows_busy_ephemeral_scavenge_subprocess() -> Result<(), WvError> {
        run_with_watchdog(FIXTURE_DEADLINE, || {
            let root = profile_fixture_root("dev-busy");
            std::fs::create_dir_all(&root).map_err(failure)?;
            let old = crate::profile::EphemeralProfile::from_host_random([22; 32])?;
            let old_selection = crate::profile::WebProfileSelection::ephemeral_dev(old);
            let old_plan = super::super::windows_profile_plan(&root, old_selection)?;
            let old_owner = super::super::prepare_windows_profile_at(&root, old_selection)?;
            let current = crate::profile::EphemeralProfile::from_host_random([23; 32])?;
            let current = crate::profile::WebProfileSelection::ephemeral_dev(current);
            profile_fixture_engine(&root, current).and_then(run_profile_fixture)?;
            if !old_plan.control_dir.exists() {
                return Err(failure("busy old ephemeral leaf was deleted"));
            }
            drop(old_owner);
            std::fs::remove_dir_all(root).map_err(failure)
        })
    }

    #[test]
    #[ignore = "real Windows WebView2 exclusive-UDF recovery"]
    /// Seeds one synthetic dead `running` record, then proves the real hidden
    /// exclusive-UDF probe and clean `idle` transition. It is not an actual
    /// host-death cut across every durable phase.
    fn windows_persistent_recovery_subprocess() -> Result<(), WvError> {
        run_with_watchdog(FIXTURE_DEADLINE, || {
            use crate::profile::{
                ProfileIdentity, ProfileLifecyclePhase, ProfileLifecycleRecord,
                ProfileProcessIdentity, WebProfileSelection,
            };

            let root = profile_fixture_root("persistent-recovery");
            std::fs::create_dir_all(&root).map_err(failure)?;
            let identity =
                ProfileIdentity::from_host_verified_parts([14; 32], "dev.keld.synthetic-recovery")?;
            let selection = WebProfileSelection::Persistent(identity);
            let plan = super::super::windows_profile_plan(&root, selection)?;
            std::fs::create_dir_all(&plan.user_data_dir).map_err(failure)?;
            std::fs::write(
                plan.control_dir.join(super::super::PROFILE_MARKER),
                &plan.marker,
            )
            .map_err(failure)?;
            std::fs::write(plan.control_dir.join(super::super::PROFILE_LEASE), [])
                .map_err(failure)?;
            let dead = ProfileLifecycleRecord::windows_idle()
                .begin_startup(ProfileProcessIdentity::from_host_observation(
                    u32::MAX,
                    u64::MAX,
                )?)?
                .advance(ProfileLifecyclePhase::Running)?;
            let lifecycle = plan.control_dir.join(super::super::PROFILE_LIFECYCLE);
            std::fs::write(&lifecycle, dead.to_record_bytes()?).map_err(failure)?;

            let result = profile_fixture_engine(&root, selection).and_then(run_profile_fixture);
            if result.is_ok() {
                let durable = ProfileLifecycleRecord::from_windows_record_bytes(
                    &std::fs::read(&lifecycle).map_err(failure)?,
                )?;
                if durable.phase() != ProfileLifecyclePhase::Idle {
                    return Err(failure("clean recovery did not commit durable idle"));
                }
                std::fs::remove_dir_all(&root).map_err(failure)?;
            } else {
                eprintln!("KELD_PROFILE_RETAINED {}", root.display());
            }
            result
        })
    }

    #[test]
    #[ignore = "real WebView2 Profile4 saved-media reconciliation with synthetic identity"]
    fn windows_saved_media_reconciliation_subprocess() -> Result<(), WvError> {
        run_with_watchdog(FIXTURE_DEADLINE, || {
            use crate::profile::{ProfileIdentity, WebProfileSelection};

            let root = profile_fixture_root("saved-media");
            std::fs::create_dir_all(&root).map_err(failure)?;
            let identity = ProfileIdentity::from_host_verified_parts(
                [15; 32],
                "dev.keld.synthetic-saved-media",
            )?;
            let mut engine =
                profile_fixture_engine(&root, WebProfileSelection::Persistent(identity))?;
            let first = engine.create(&blank_spec())?;
            let profile = super::super::webview_profile(&engine.view(first)?.webview)?;
            let origin = "http://127.0.0.1:41000/";
            for kind in [
                COREWEBVIEW2_PERMISSION_KIND_CAMERA,
                COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
            ] {
                super::super::set_profile_permission_state(
                    &profile,
                    &super::super::SavedPermission {
                        kind,
                        state: COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                        origin: origin.to_owned(),
                    },
                    COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                )?;
            }
            assert_saved_media_state(&profile, origin, COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
            println!("KELD_PROFILE_SAVED_MEDIA seeded=allow camera=true microphone=true");

            engine.destroy(first)?;
            engine.saved_permissions_reconciled = false;
            let second = engine.create(&blank_spec())?;
            let reconciled = super::super::webview_profile(&engine.view(second)?.webview)?;
            assert_saved_media_state(&reconciled, origin, COREWEBVIEW2_PERMISSION_STATE_DENY)?;
            println!("KELD_PROFILE_SAVED_MEDIA reconciled=deny camera=true microphone=true");

            for kind in [
                COREWEBVIEW2_PERMISSION_KIND_CAMERA,
                COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
            ] {
                super::super::set_profile_permission_state(
                    &reconciled,
                    &super::super::SavedPermission {
                        kind,
                        state: COREWEBVIEW2_PERMISSION_STATE_DENY,
                        origin: origin.to_owned(),
                    },
                    COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                )?;
            }
            assert_saved_media_state(&reconciled, origin, COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
            println!(
                "KELD_PROFILE_SAVED_MEDIA mutation=restored-allow camera=true microphone=true"
            );

            let (commands_tx, commands_rx) = mpsc::channel();
            commands_tx
                .send(crate::AppWindowCommand::Quit)
                .map_err(|_| failure("queue fixture Quit"))?;
            let result = engine.run_app_until_quit(commands_rx, mpsc::channel().0);
            if result.is_ok() {
                std::fs::remove_dir_all(&root).map_err(failure)?;
            } else {
                eprintln!("KELD_PROFILE_RETAINED {}", root.display());
            }
            result
        })
    }

    #[test]
    #[ignore = "real same-profile media saved-grant phase subprocess"]
    fn windows_saved_grant_phase_subprocess() -> Result<(), WvError> {
        run_with_watchdog(FIXTURE_DEADLINE, run_saved_grant_phase)
    }

    #[test]
    #[ignore = "real Windows WebView2 media acceptance subprocess"]
    fn windows_media_acceptance_subprocess() -> Result<(), WvError> {
        run_media_acceptance()
    }

    #[test]
    fn stale_or_abnormal_browser_exit_cannot_release_profile() {
        assert!(!matching_browser_exit(20, 10, true));
        assert!(!matching_browser_exit(20, 20, false));
        assert!(!matching_browser_exit(0, 0, true));
        assert!(matching_browser_exit(20, 20, true));
    }
    #[test]
    fn controls_require_exact_platform_state_kind_and_thread() {
        let kind = COREWEBVIEW2_PERMISSION_KIND_CAMERA.0;
        let state = COREWEBVIEW2_PERMISSION_STATE_DEFAULT.0;
        let origin = "http://127.0.0.1:41000";
        assert!(control_matches(
            ControlExpectation {
                mode: "removed-guard",
                nonce: 7,
                result: "7:true:NotAllowedError",
                kind,
                origin,
                registration_identity: 11,
                tid: 3,
            },
            &[(
                kind,
                state,
                COREWEBVIEW2_PERMISSION_STATE_DENY.0,
                COREWEBVIEW2_PERMISSION_STATE_DENY.0,
                origin.to_owned(),
                11,
                3
            )]
        ));
        for tuple in [
            (
                kind,
                state,
                COREWEBVIEW2_PERMISSION_STATE_DEFAULT.0,
                COREWEBVIEW2_PERMISSION_STATE_DENY.0,
                origin.to_owned(),
                11,
                3,
            ),
            (
                COREWEBVIEW2_PERMISSION_KIND_MICROPHONE.0,
                state,
                COREWEBVIEW2_PERMISSION_STATE_DENY.0,
                COREWEBVIEW2_PERMISSION_STATE_DENY.0,
                origin.to_owned(),
                11,
                3,
            ),
            (
                kind,
                state,
                COREWEBVIEW2_PERMISSION_STATE_DENY.0,
                COREWEBVIEW2_PERMISSION_STATE_DENY.0,
                origin.to_owned(),
                11,
                9,
            ),
        ] {
            assert!(!control_matches(
                ControlExpectation {
                    mode: "removed-guard",
                    nonce: 7,
                    result: "7:true:NotAllowedError",
                    kind,
                    origin,
                    registration_identity: 11,
                    tid: 3,
                },
                &[tuple]
            ));
        }
    }

    #[test]
    fn allow_requires_a_requested_live_track_that_was_stopped() {
        let camera = COREWEBVIEW2_PERMISSION_KIND_CAMERA.0;
        assert!(allow_result_matches(camera, 7, "7:resolved:video:1:true"));
        for result in [
            "7:resolved:video:0:true",
            "7:resolved:video:1:false",
            "7:resolved:audio:1:true",
            "7:resolved",
        ] {
            assert!(!allow_result_matches(camera, 7, result), "{result}");
        }
    }

    #[test]
    fn parent_deadline_stays_above_the_complete_child_bound() {
        assert!(parent_deadline_is_valid("90000"));
        for value in ["89999", "60000", "invalid"] {
            assert!(!parent_deadline_is_valid(value), "{value}");
        }
    }
}
