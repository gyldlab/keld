//! W3 T2. A real guarded file navigation must serve only the admitted graph.
#![forbid(unsafe_code)]

use super::saved_permission_deadline::with_engine;
use super::{
    FIXTURE_DEADLINE, blank_spec, failure, observe_request, run_with_watchdog, wait_with_pump,
};
use crate::renderer_bridge::RendererRoleWitness;
use crate::resource_source::windows::tests::{ENTRY, Fixture};
use crate::webview2::original_callbacks::NavigationCallbacks;
use crate::webview2::original_load::{LoadDispatch, PreparedNativeLoad};
use crate::webview2::original_navigation::OriginalNavigation;
use crate::webview2::original_resource::{ResourceSession, ResourceView};
use crate::webview2::{
    PROFILE_LAUNCH_DEADLINE, WebView2Engine, check_launch_deadline, milliseconds_until,
    pump_message_or_handle,
};
use crate::{
    AppWindowEvent, NativeNavigationContext, OriginalEntryQuery, OriginalNavigationFact, WebEngine,
    WvError,
};
use std::cell::{Cell, RefCell};
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::{Arc, atomic::AtomicBool, mpsc};
use std::time::Instant;

const QUERY: &str = "launch=fixture%2Fcase&second=+";
const PAGE: &[u8] =
    br#"<!doctype html><html data-entry="entry-89b475"><title>W3 resource proof</title>
<script src="selected.js" defer></script></html>"#;
const SCRIPT: &[u8] = br#"
const probe = document.createElement('script');
const report = result => chrome.webview.postMessage([
  document.documentElement.dataset.entry, 'asset-ac7143', location.search, result,
  String(globalThis.unlistedNonce),
].join('|'));
probe.addEventListener('load', () => report('load'), { once: true });
probe.addEventListener('error', () => report('error'), { once: true });
probe.src = 'unlisted.js';
document.head.append(probe);
"#;

#[test]
#[ignore = "real Windows desktop and x64 WebView2 runtime qualified for Q22(a,b)"]
fn windows_original_resources_serve_only_the_selected_graph() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || with_engine(serve_selected_graph))
}

fn serve_selected_graph(engine: &mut WebView2Engine) -> Result<(), WvError> {
    let fixture = Fixture::with_files(
        PAGE,
        &[
            ("views/selected.js", SCRIPT, true),
            (
                "views/unlisted.js",
                b"globalThis.unlistedNonce = 'outside-31a996';",
                false,
            ),
        ],
    );
    let (initial_tx, initial_rx) = mpsc::channel();
    let id = engine.create_app(&blank_spec(), initial_tx)?;
    // This harness starts a preliminary NavigateToString. Its real completion
    // must precede installing the selected file-navigation callbacks.
    let initial = wait_with_pump(initial_rx).map_err(failure)?;
    if !matches!(initial, AppWindowEvent::NavigationReady) {
        return Err(failure(format!(
            "preliminary navigation was not ready: {initial:?}"
        )));
    }
    // The fixture closes the selected controller itself. Ordinary profile
    // retirement keeps a separate live controller in the same environment.
    let release_witness = engine.create(&blank_spec())?;
    let source = fixture.source_arc();
    let session = ResourceSession::new(
        Arc::clone(&source),
        engine.creation_event_loop()?.create_proxy(),
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        None,
    )?;
    let native = engine.view(id)?;
    let navigation = Rc::new(RefCell::new(
        OriginalNavigation::new(id, source).map_err(failure)?,
    ));
    let resource = ResourceView::bind(
        &native.webview,
        &engine.environment,
        Rc::clone(&navigation),
        Rc::clone(&session),
        id,
        Rc::clone(&native.close_custody),
    )?;
    let callbacks = NavigationCallbacks::prepare(
        native.webview.clone(),
        Rc::clone(&navigation),
        Rc::clone(&session),
        id,
    );
    let observed = (|| {
        resource.install_on_ui()?;
        let outcome = Rc::new(Cell::new(None));
        let completion = Rc::clone(&outcome);
        callbacks
            .install_on_ui(Rc::new(move |_, fact| match fact {
                OriginalNavigationFact::Finished { .. } => completion.set(Some(true)),
                OriginalNavigationFact::Failed { .. } => completion.set(Some(false)),
                _ => {}
            }))
            .map_err(failure)?;
        let (message_tx, message_rx) = mpsc::channel();
        let expected_url = format!("{}?{QUERY}", fixture.url(ENTRY));
        observe_request(&native.webview, "guarded", &expected_url, message_tx)?;

        let query = OriginalEntryQuery::from_optional_query(Some(QUERY)).map_err(failure)?;
        let (qualified, call) = PreparedNativeLoad::prepare(&resource, &query)
            .map_err(failure)?
            .into_parts();
        let context = NativeNavigationContext::new(NonZeroU64::MIN, id);
        let role = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
        navigation
            .borrow_mut()
            .begin_host_load(qualified, context, role, Instant::now())
            .map_err(failure)?;
        if !matches!(call.invoke(), LoadDispatch::Returned(Ok(()))) {
            return Err(failure("selected native load did not return success"));
        }
        let mut message = None;
        drive(&session, || {
            if outcome.get() == Some(false) {
                return Err(failure("selected native navigation failed"));
            }
            if message.is_none() {
                match message_rx.try_recv() {
                    Ok(result) => message = Some(result?),
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(error) => return Err(failure(error)),
                }
            }
            Ok(outcome.get() == Some(true) && message.is_some())
        })?;
        message.ok_or_else(|| failure("selected page sent no observation"))
    })();

    // Always retire the same resource and callback owners before evaluating the
    // page oracle, including when installation, navigation, or readback failed.
    let retired = (|| {
        let session_retired = session.retire_on_ui();
        let callbacks_retired = callbacks.retire_on_ui().map_err(failure);
        resource.retire_on_ui()?;
        if !native.close_custody.close_on_ui() {
            return Err(failure("selected controller did not close"));
        }
        resource.controller_closed_on_ui();
        callbacks.controller_closed_on_ui();
        drive(&session, || Ok(session.is_drained()))?;
        if !resource.is_drained() || !callbacks.is_drained() {
            return Err(failure(
                "selected callback or resource custody did not drain",
            ));
        }
        if !engine.view(release_witness)?.close_custody.is_open() {
            return Err(failure("ordinary browser release lost its witness"));
        }
        session_retired.and(callbacks_retired)
    })();
    if let Err(error) = &retired {
        eprintln!("KELD_W3_RESOURCE_RETIREMENT_ERROR {error}");
    }
    let message = observed?;
    retired?;
    let expected = format!("entry-89b475|asset-ac7143|?{QUERY}|error|undefined");
    if message != expected {
        return Err(failure(format!(
            "unexpected browser resource report {message:?}"
        )));
    }
    println!("KELD_W3_RESOURCE_SELECTED_AND_DRAINED {message}");
    Ok(())
}

fn drive(
    session: &ResourceSession,
    mut complete: impl FnMut() -> Result<bool, WvError>,
) -> Result<(), WvError> {
    let deadline = Instant::now() + PROFILE_LAUNCH_DEADLINE;
    loop {
        check_launch_deadline(deadline)?;
        session.drain_ui()?;
        if complete()? {
            return Ok(());
        }
        pump_message_or_handle(None, milliseconds_until(deadline)?)?;
    }
}
