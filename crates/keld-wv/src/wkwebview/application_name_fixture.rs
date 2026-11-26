//! Debug-only native UA observation: inert first-party pages, no source or preload.
use std::sync::mpsc;
use std::time::Instant;

use serde_json::{Value, json};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::run_return::EventLoopExtRunReturn;

mod observer;
use observer::UaObservation;

use super::managed_windows::{self, NativeBuild};
use super::{AppWindowCommand, AppWindowEvent, WkUserEvent, WkWebViewEngine};
use crate::profile::{EphemeralProfile, WebProfileSelection};
use crate::{ApplicationNameForUserAgent, NavTarget, WebEngine, WebviewSpec, WvError};

fn failure(detail: impl Into<String>) -> WvError {
    WvError::EventLoop(format!("GH476 native UA fixture: {}", detail.into()))
}

/// Observes actual `navigator.userAgent` on ordinary and managed native views.
/// Run in a fresh debug child on the `AppKit` main thread. This bypasses no
/// application admission: it is a backend probe with no app source, preload,
/// principal, IPC endpoint or permission claim. Core/CLI admission is tested
/// separately. The result is observation data, not native identity evidence.
///
/// # Errors
/// Refuses invalid fixture cases, native build/readback failures or deadline.
pub fn run_application_name_fixture_child(case: &str) -> Result<Value, WvError> {
    let requested = match case {
        "default" => None,
        "first-party-a" => Some("FirstPartyA/1 FrameworkMetadata"),
        "first-party-b" => Some("FirstPartyB/2 FrameworkMetadata"),
        _ => return Err(failure("expected default, first-party-a or first-party-b")),
    };
    let metadata = requested
        .map(|value| ApplicationNameForUserAgent::new(value.to_owned()))
        .transpose()?;
    let mut nonce = [0_u8; 32];
    getrandom::fill(&mut nonce).map_err(|error| failure(error.to_string()))?;
    let profile = WebProfileSelection::ephemeral_dev(EphemeralProfile::from_host_random(nonce)?);
    let mut engine = match metadata {
        None => WkWebViewEngine::new(profile)?,
        Some(metadata) => {
            WkWebViewEngine::new_with_application_name_for_user_agent(profile, Some(metadata))?
        }
    };
    let proxy = engine
        .event_loop
        .as_ref()
        .ok_or_else(|| failure("missing event loop"))?
        .create_proxy();
    let mut probe = UaObservation::new(proxy.clone())?;
    engine.create(&WebviewSpec {
        title: String::from("GH476 inert ordinary view"),
        initial: NavTarget::Html(probe.page("ordinary")),
        ..WebviewSpec::default()
    })?;
    let mut event_loop = engine
        .event_loop
        .take()
        .ok_or_else(|| failure("missing event loop"))?;

    let mut started = false;
    let mut problem = None;
    let code = event_loop.run_return(|event, target, control| {
        *control = ControlFlow::WaitUntil(probe.deadline);
        let turn = (|| {
            if !started {
                started = true;
                build_managed(&mut engine, target, &proxy, &probe.page("managed"))?;
                verify_paths(&engine)?;
            }
            if matches!(
                event,
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                }
            ) {
                return Err(failure("window closed before both UA observations"));
            }
            probe.observe()?;
            if probe.len() == 2 {
                *control = ControlFlow::Exit;
            } else if Instant::now() >= probe.deadline {
                return Err(failure("native UA readback deadline expired"));
            }
            Ok(())
        })();
        if let Err(error) = turn {
            problem = Some(error);
            *control = ControlFlow::Exit;
        }
    });
    // Every view is source-less and has no native reader/bridge custody. Drop
    // the actual views before the same engine's profile cleanup, as production.
    engine.views.clear();
    engine.profile.clean_shutdown()?;
    if let Some(error) = problem {
        return Err(error);
    }
    if code != 0 {
        return Err(failure(format!("native loop returned {code}")));
    }
    Ok(
        json!({"schema":"gh476-native-ua-observation.v1", "case":case, "requested":requested, "backend":"WKWebView", "source":false, "preload":false, "ipc_endpoint":false, "rows":probe.finish()? }),
    )
}

fn build_managed(
    engine: &mut WkWebViewEngine,
    target: &EventLoopWindowTarget<WkUserEvent>,
    proxy: &EventLoopProxy<WkUserEvent>,
    page: &str,
) -> Result<(), WvError> {
    let (events, incoming) = mpsc::channel();
    let native = NativeBuild {
        target,
        profile: &engine.profile,
        startup: &engine.startup,
        media_seed_allow: false,
        renderer_endpoint: None,
        resource_source: None,
        resource_wake: proxy.clone(),
        resource_events: None,
    };
    managed_windows::command(
        AppWindowCommand::CreateWindow {
            build: 476,
            width: 640,
            height: 480,
            x: None,
            y: None,
            original_source: None,
            preload_image: None,
        },
        &native,
        &mut engine.next_id,
        &mut engine.views,
        &mut engine.renderer_bridges,
        &events,
    )?;
    let event = incoming
        .try_recv()
        .map_err(|error| failure(error.to_string()))?;
    let AppWindowEvent::WindowBuilt {
        build: 476, view, ..
    } = event
    else {
        return Err(failure(
            "managed build did not return its actual WindowBuilt event",
        ));
    };
    let selected = engine
        .views
        .get(&view.0)
        .ok_or_else(|| failure("actual managed view absent"))?;
    selected
        .webview
        .load_html(page)
        .map_err(|error| failure(error.to_string()))?;
    Ok(())
}

fn verify_paths(engine: &WkWebViewEngine) -> Result<(), WvError> {
    if engine.views.len() != 2
        || !engine.renderer_bridges.is_empty()
        || engine.views.values().any(|view| view.resource.is_some())
    {
        return Err(failure(
            "fixture requires two real source-less unbridged views",
        ));
    }
    if engine
        .views
        .values()
        .filter(|view| view.registry_managed)
        .count()
        != 1
    {
        return Err(failure(
            "ordinary/managed construction paths were not both exercised",
        ));
    }
    Ok(())
}
