//! Debug-only first-party effect probe through the actual S1 native owners.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use nix::fcntl::{OFlag, open};
use nix::sys::stat::Mode;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tao::event::{Event, WindowEvent};
use tao::event_loop::ControlFlow;
use tao::platform::run_return::EventLoopExtRunReturn;

use super::managed_windows::{self, NativeBuild};
use super::{
    AdmittedWebAppSource, AppWindowCommand, AppWindowEvent, VerifiedResourceResponsePolicy,
    VerifiedWebAsset, WkUserEvent, WkWebViewEngine,
};
use crate::WvError;
use crate::engine::{NativeNavigationContext, OriginalEntryQuery};

fn failure(detail: impl Into<String>) -> WvError {
    WvError::EventLoop(format!("S1 first-party native fixture: {}", detail.into()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            let _ = write!(&mut output, "{byte:02x}");
            output
        },
    )
}

pub(super) fn snapshot(raw: &str) -> Result<Value, WvError> {
    if raw.len() > 32 * 1024 {
        return Err(failure("observer result exceeds its private bound"));
    }
    let outer: Value = serde_json::from_str(raw).map_err(|error| failure(error.to_string()))?;
    match outer {
        Value::String(inner) => {
            serde_json::from_str(&inner).map_err(|error| failure(error.to_string()))
        }
        value => Ok(value),
    }
}

/// Construct only a trusted fixed first-party fixture after its independent
/// capsule and explicit Legacy manifest admission. Never an original-app certificate.
///
/// # Errors
/// Refuses source open, entropy, graph/policy shape or captured asset failure.
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub fn admit_s1_fixture_source(
    webapp: &Path,
    assets: Vec<VerifiedWebAsset>,
    source_hash: [u8; 32],
    policy: VerifiedResourceResponsePolicy,
) -> Result<Arc<AdmittedWebAppSource>, WvError> {
    let root = open(
        webapp,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| failure(error.to_string()))?;
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| failure(error.to_string()))?;
    AdmittedWebAppSource::from_admitted_graph(
        root,
        source_hash,
        assets,
        Path::new("index.html"),
        policy,
        String::from("keld-resource"),
        hex(&nonce),
    )
    .map_err(failure)
}

/// Run only in a dedicated debug child with trusted first-party fixture inputs.
///
/// This uses the actual S1 source/handler/navigation/release implementations.
/// The source hash identifies an independently frozen inert fixture manifest;
/// it is explicitly not a GH499 original-application admission certificate.
/// No app link, Bun process, original script or native page privilege handler exists.
///
/// # Errors
/// Returns a native fixture failure for unsupported cases, source/setup faults,
/// stale snapshots or missing effects. Pending native retirement is returned as
/// an explicit unproved-cleanup status with custody retained until child exit;
/// this effect probe never polls or reports that pending release succeeded.
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub fn run_s1_resource_fixture_child(
    webapp: &Path,
    assets: Vec<VerifiedWebAsset>,
    source_hash: [u8; 32],
    policy: VerifiedResourceResponsePolicy,
    case: &str,
) -> Result<Value, WvError> {
    if !matches!(
        case,
        "baseline"
            | "without-wasm"
            | "preflight-preserves"
            | "blocked-destroy"
            | "blocked-quit"
            | "native-task-stop"
            | "resource-fanout"
            | "preload-origin"
            | "preload-preflight-facts"
            | "preload-postbind-burst"
            | "preload-postbind-effect-burst"
            | "preload-postbind-refusal-order"
            | "preload-postbind-retire-order"
            | "preload-control-error"
            | "preload-replay"
            | "preload-stale-close-cut"
            | "preload-held-conditional-destroy"
            | "preload-held-destroy"
            | "preload-held-quit"
            | "preload-held-bootstrap"
            | "preload-attempt-hook-denied"
            | "preload-attempt-hook-reentrant"
    ) {
        return Err(failure(
            "this bounded smoke accepts baseline, without-wasm or preflight-preserves",
        ));
    }
    let source = admit_s1_fixture_source(webapp, assets, source_hash, policy)?;
    if case == "without-wasm" {
        source.fixture_omit_wasm_token().map_err(failure)?;
    }
    if matches!(
        case,
        "preload-origin"
            | "preload-preflight-facts"
            | "preload-postbind-burst"
            | "preload-postbind-effect-burst"
            | "preload-postbind-refusal-order"
            | "preload-postbind-retire-order"
            | "preload-control-error"
            | "preload-replay"
            | "preload-stale-close-cut"
            | "preload-held-conditional-destroy"
            | "preload-held-destroy"
            | "preload-held-quit"
            | "preload-held-bootstrap"
            | "preload-attempt-hook-denied"
            | "preload-attempt-hook-reentrant"
    ) {
        return super::preload_native_fixture::run(&source, case);
    }
    if matches!(
        case,
        "blocked-destroy" | "blocked-quit" | "native-task-stop" | "resource-fanout"
    ) {
        return super::s1_resource_lifecycle::run(source, case);
    }
    run_source_fixture(&source, case)
}

fn run_source_fixture(source: &Arc<AdmittedWebAppSource>, case: &str) -> Result<Value, WvError> {
    let mut engine = WkWebViewEngine::new_dev_ephemeral()?;
    engine.admit_initial_legacy_webapp(Arc::clone(source))?;
    let mut event_loop = engine
        .event_loop
        .take()
        .ok_or_else(|| failure("event loop unavailable"))?;
    let mut fixture = SourceFixture::new(source, case, event_loop.create_proxy())?;
    let code = event_loop.run_return(|event, target, control| {
        *control = ControlFlow::WaitUntil(fixture.retirement.deadline.unwrap_or(fixture.deadline));
        let reader_completion_turn =
            matches!(&event, Event::UserEvent(WkUserEvent::ReaderTerminated(_)));
        let native = NativeBuild {
            target,
            profile: &engine.profile,
            startup: &engine.startup,
            media_seed_allow: false,
            renderer_endpoint: None,
            resource_source: Some(source),
            resource_wake: fixture.proxy.clone(),
            resource_events: Some(fixture.events.clone()),
        };
        let turn = fixture.turn(event, &native);
        fixture.finish_turn(turn, reader_completion_turn, control);
    });
    fixture.finish(engine, code)
}

#[derive(Default)]
struct SourceNavigation {
    selected: Option<crate::WebviewId>,
    committed: bool,
    finished: bool,
    committed_url: Option<String>,
    forwarded_url: Option<super::MacNavigationUrl>,
    forwarded_stamp: Option<super::MacNativeDocumentStamp>,
}
#[derive(Default)]
struct SourceObservation {
    inflight: bool,
    page: Option<Value>,
    reads: Vec<Value>,
    reader_wakes: u32,
    preflight_checked: bool,
    preflight_refused: bool,
}
#[derive(Default)]
struct SourceRetirement {
    terminal: bool,
    released: bool,
    attempted: bool,
    deadline: Option<Instant>,
    problem: Option<WvError>,
}
struct SourceFixture<'a> {
    bridges: BTreeMap<u32, super::MacRendererBridge>,
    views: BTreeMap<u32, super::View>,
    source: &'a Arc<AdmittedWebAppSource>,
    case: &'a str,
    proxy: tao::event_loop::EventLoopProxy<WkUserEvent>,
    events: mpsc::Sender<AppWindowEvent>,
    incoming: mpsc::Receiver<AppWindowEvent>,
    next_id: u32,
    initialized: bool,
    query: OriginalEntryQuery,
    deadline: Instant,
    navigation: SourceNavigation,
    observation: SourceObservation,
    retirement: SourceRetirement,
}
impl<'a> SourceFixture<'a> {
    fn new(
        source: &'a Arc<AdmittedWebAppSource>,
        case: &'a str,
        proxy: tao::event_loop::EventLoopProxy<WkUserEvent>,
    ) -> Result<Self, WvError> {
        let (events, incoming) = mpsc::channel();
        let deadline = Instant::now() + Duration::from_secs(20);
        let query = OriginalEntryQuery::new(&format!("case={case}")).map_err(failure)?;
        Ok(Self {
            bridges: BTreeMap::new(),
            views: BTreeMap::new(),
            source,
            case,
            proxy,
            events,
            incoming,
            next_id: 1,
            initialized: false,
            query,
            deadline,
            navigation: SourceNavigation::default(),
            observation: SourceObservation::default(),
            retirement: SourceRetirement::default(),
        })
    }
    fn turn(
        &mut self,
        event: Event<'_, WkUserEvent>,
        native: &NativeBuild<'_>,
    ) -> Result<(), WvError> {
        if !self.initialized {
            self.initialized = true;
            managed_windows::command(
                AppWindowCommand::CreateWindow {
                    build: 1,
                    width: 640,
                    height: 480,
                    x: None,
                    y: None,
                    original_source: Some(Arc::clone(self.source)),
                    preload_image: None,
                },
                native,
                &mut self.next_id,
                &mut self.views,
                &mut self.bridges,
                &self.events,
            )?;
        }
        self.consume_platform_event(event)?;
        self.consume_application_events(native)?;
        self.advance_observation(native)
    }
    fn consume_platform_event(&mut self, event: Event<'_, WkUserEvent>) -> Result<(), WvError> {
        match event {
            Event::UserEvent(WkUserEvent::ReaderTerminated(completion)) => {
                if let Some(resource) = self
                    .views
                    .get(&completion.view.0)
                    .and_then(|view| view.resource.as_ref())
                    && resource.handler.observe_reader_termination(&completion)
                {
                    self.observation.reader_wakes += 1;
                }
            }
            Event::UserEvent(WkUserEvent::Resource(completion)) => {
                if let Ok(bytes) = &completion.result {
                    if self.observation.reads.len() >= 64 {
                        return Err(failure("read observation census exceeded"));
                    }
                    self.observation.reads.push(json!({
                        "bytes": bytes.bytes.len(), "mime": bytes.mime,
                        "sha256": hex(&Sha256::digest(&bytes.bytes)),
                    }));
                }
                if let Some(resource) = self
                    .views
                    .get(&completion.view.0)
                    .and_then(|view| view.resource.as_ref())
                {
                    resource.handler.deliver(completion);
                }
            }
            Event::UserEvent(WkUserEvent::S1FixtureSnapshot { view, stamp, raw }) => {
                self.observe_snapshot(view, stamp, &raw)?;
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } if !self.retirement.terminal => {
                return Err(failure("native window ended before probe completion"));
            }
            Event::LoopDestroyed if !self.retirement.terminal => {
                return Err(failure("native loop ended before probe completion"));
            }
            _ => {}
        }
        Ok(())
    }
    fn observe_snapshot(
        &mut self,
        view: crate::WebviewId,
        stamp: super::macos_document::NativeCommittedStamp,
        raw: &str,
    ) -> Result<(), WvError> {
        let case = self.case;
        self.observation.inflight = false;
        let current = self
            .views
            .get(&view.0)
            .and_then(|view| view.resource.as_ref())
            .and_then(|resource| resource.navigation.current_committed());
        if current != Some(stamp) {
            return Err(failure(
                "native document changed during read-only observation",
            ));
        }
        let value = snapshot(raw)?;
        if value.get("complete").and_then(Value::as_bool) == Some(true) {
            let text = value
                .get("result")
                .and_then(Value::as_str)
                .ok_or_else(|| failure("completed observer has no result"))?;
            let actual = self
                .views
                .get(&view.0)
                .ok_or_else(|| failure("actual native view absent"))?
                .webview
                .url()
                .map_err(|error| failure(error.to_string()))?;
            let expected = self
                .source
                .admitted_entry_url(&format!("case={case}"))
                .ok_or_else(|| failure("fixture query was not admitted"))?;
            if actual != expected {
                return Err(failure(
                    "actual native URL/query differs from admitted entry",
                ));
            }
            if self
                .navigation
                .forwarded_url
                .as_ref()
                .map(|url: &super::MacNavigationUrl| url.as_str())
                != Some(actual.as_str())
            {
                return Err(failure(
                    "actual native commit URL was lost before observation",
                ));
            }
            self.navigation.committed_url = Some(actual);
            self.observation.page = Some(
                serde_json::from_str::<Value>(text).map_err(|error| failure(error.to_string()))?,
            );
            self.retirement.terminal = true;
        }
        Ok(())
    }
    fn consume_application_events(&mut self, native: &NativeBuild<'_>) -> Result<(), WvError> {
        while let Ok(event) = self.incoming.try_recv() {
            match event {
                AppWindowEvent::WindowBuilt { view, .. } => {
                    self.navigation.selected = Some(view);
                    let operation = std::num::NonZeroU64::new(1)
                        .ok_or_else(|| failure("fixture operation absent"))?;
                    managed_windows::command(
                        AppWindowCommand::LoadAdmittedOriginal {
                            context: NativeNavigationContext::new(operation, view),
                            query: self.query.clone(),
                        },
                        native,
                        &mut self.next_id,
                        &mut self.views,
                        &mut self.bridges,
                        &self.events,
                    )?;
                }
                AppWindowEvent::WindowBuildFailed { .. } => {
                    return Err(failure("native build failed"));
                }
                // These values come only from the actual installed native
                // callback through its existing Sender, never test-created facts.
                AppWindowEvent::MacOriginalNavigation { context, fact } => {
                    self.observe_navigation(context, fact)?;
                }
                AppWindowEvent::WindowReleased { .. } => self.retirement.released = true,
                _ => {}
            }
        }
        Ok(())
    }
    fn observe_navigation(
        &mut self,
        context: NativeNavigationContext,
        fact: crate::engine::MacOriginalNavigationFact,
    ) -> Result<(), WvError> {
        if context.operation().get() == 2
            && self.case == "preflight-preserves"
            && self.observation.preflight_checked
        {
            if matches!(
                fact,
                crate::engine::MacOriginalNavigationFact::Refused {
                    error: crate::engine::NativeNavigationRefusal::NativeUrlRejected,
                }
            ) {
                self.observation.preflight_refused = true;
                return Ok(());
            }
            return Err(failure(
                "oversize preflight produced a native attempt or wrong outcome",
            ));
        }
        let view = context.view();
        if context.operation().get() != 1 {
            return Err(failure("foreign native operation context"));
        }
        match fact {
            crate::engine::MacOriginalNavigationFact::Committed {
                stamp,
                native_url,
                document,
            } => {
                if self.navigation.committed
                    || self.navigation.selected != Some(view)
                    || stamp.view() != view
                {
                    return Err(failure("duplicate/foreign native commit observation"));
                }
                let actual_view = self
                    .views
                    .get(&view.0)
                    .ok_or_else(|| failure("commit view is absent"))?;
                let resource = actual_view
                    .resource
                    .as_ref()
                    .ok_or_else(|| failure("commit has no admitted native resource owner"))?;
                // Same UI turn: merely storing the stamp would not
                // establish that the living native owner still accepts it.
                if !resource.navigation.still_current(stamp) || stamp.observed_generation() == 0 {
                    return Err(failure("commit stamp is no longer current"));
                }
                if !document.is_current()
                    || document.stamp() != stamp
                    || !document.belongs_to_source(self.source)
                {
                    return Err(failure(
                        "native commit did not carry its same actual source-bound lease",
                    ));
                }
                let actual = actual_view
                    .webview
                    .url()
                    .map_err(|error| failure(error.to_string()))?;
                if native_url.as_str() != actual
                    || self
                        .source
                        .qualify_document_url(native_url.as_str())
                        .is_err()
                {
                    return Err(failure(
                        "callback did not forward its qualified actual native URL",
                    ));
                }
                self.navigation.forwarded_url = Some(native_url);
                self.navigation.forwarded_stamp = Some(stamp);
                self.navigation.committed = true;
            }
            crate::engine::MacOriginalNavigationFact::Finished { stamp } => {
                let current = self
                    .views
                    .get(&view.0)
                    .and_then(|view| view.resource.as_ref())
                    .is_some_and(|resource| resource.navigation.still_current(stamp));
                if self.navigation.finished
                    || self.navigation.selected != Some(view)
                    || self.navigation.forwarded_stamp != Some(stamp)
                    || !current
                {
                    return Err(failure("finish does not belong to the current real commit"));
                }
                self.navigation.finished = true;
            }
            crate::engine::MacOriginalNavigationFact::Failed { .. } => {
                return Err(failure("actual native navigation failed"));
            }
            crate::engine::MacOriginalNavigationFact::Refused { .. } => {
                return Err(failure("actual native load refused"));
            }
            _ => {}
        }
        Ok(())
    }
    fn advance_observation(&mut self, native: &NativeBuild<'_>) -> Result<(), WvError> {
        if !self.retirement.terminal && !self.observation.inflight && self.navigation.finished {
            if self.case == "preflight-preserves" && !self.observation.preflight_checked {
                let id = self
                    .navigation
                    .selected
                    .ok_or_else(|| failure("no actual native view"))?;
                let view = self
                    .views
                    .get(&id.0)
                    .ok_or_else(|| failure("view absent before preflight"))?;
                let resource = view
                    .resource
                    .as_ref()
                    .ok_or_else(|| failure("source absent before preflight"))?;
                let before = resource.navigation.current_committed();
                let finished_before = resource.navigation.finished_current_attempt();
                let url_before = view
                    .webview
                    .url()
                    .map_err(|error| failure(error.to_string()))?;
                let operation = std::num::NonZeroU64::new(2)
                    .ok_or_else(|| failure("fixture operation absent"))?;
                let oversize = OriginalEntryQuery::new(&"x".repeat(2048)).map_err(failure)?;
                managed_windows::command(
                    AppWindowCommand::LoadAdmittedOriginal {
                        context: NativeNavigationContext::new(operation, id),
                        query: oversize,
                    },
                    native,
                    &mut self.next_id,
                    &mut self.views,
                    &mut self.bridges,
                    &self.events,
                )?;
                let after = self
                    .views
                    .get(&id.0)
                    .ok_or_else(|| failure("preflight destroyed native view"))?;
                let owner = after
                    .resource
                    .as_ref()
                    .ok_or_else(|| failure("preflight retired source"))?;
                if before.is_none()
                    || owner.navigation.current_committed() != before
                    || owner.navigation.finished_current_attempt() != finished_before
                    || after
                        .webview
                        .url()
                        .map_err(|error| failure(error.to_string()))?
                        != url_before
                {
                    return Err(failure(
                        "preflight refusal changed the existing actual native document",
                    ));
                }
                self.observation.preflight_checked = true;
            }
            let id = self
                .navigation
                .selected
                .ok_or_else(|| failure("no actual native view"))?;
            let view = self
                .views
                .get(&id.0)
                .ok_or_else(|| failure("native view disappeared"))?;
            let stamp = view
                .resource
                .as_ref()
                .and_then(|resource| resource.navigation.current_committed())
                .ok_or_else(|| failure("no qualified native document"))?;
            let callback_proxy = self.proxy.clone();
            self.observation.inflight = true;
            view.webview.evaluate_script_with_callback(
                    "JSON.stringify({complete:document.documentElement.getAttribute('data-probe-complete')==='yes',result:(document.getElementById('result')?.textContent||'').slice(0,8192)})",
                    move |raw| {
                        if let Err(error) = callback_proxy.send_event(WkUserEvent::S1FixtureSnapshot { view: id, stamp, raw }) {
                            eprintln!("S1 fixture observation queue closed: {error}");
                        }
                    },
                ).map_err(|error| failure(error.to_string()))?;
        }
        Ok(())
    }
    fn finish_turn(
        &mut self,
        turn: Result<(), WvError>,
        reader_completion_turn: bool,
        control: &mut ControlFlow,
    ) {
        if let Err(error) = turn {
            self.retirement.problem.get_or_insert(error);
            self.retirement.terminal = true;
        }
        if !self.retirement.terminal && Instant::now() >= self.deadline {
            self.retirement
                .problem
                .get_or_insert_with(|| failure("native effect deadline expired"));
            self.retirement.terminal = true;
        }
        let shutdown_expired = self
            .retirement
            .deadline
            .is_some_and(|limit| Instant::now() >= limit);
        if self.retirement.terminal
            && (!self.retirement.attempted || reader_completion_turn || shutdown_expired)
        {
            self.retirement.attempted = true;
            match managed_windows::release_all(&mut self.views, &mut self.bridges, &self.events) {
                Ok(true) => {
                    while let Ok(event) = self.incoming.try_recv() {
                        if matches!(event, AppWindowEvent::WindowReleased { .. }) {
                            self.retirement.released = true;
                        }
                    }
                    *control = ControlFlow::Exit;
                }
                Ok(false) => {
                    // Original custody deadline is a failure guard. Only the
                    // real accepted ReaderTerminated event permits release.
                    self.retirement.deadline = self
                        .retirement
                        .deadline
                        .or_else(|| managed_windows::resource_shutdown_deadline(&self.views));
                    if let Some(limit) = self.retirement.deadline {
                        *control = ControlFlow::WaitUntil(limit);
                    } else {
                        self.retirement.problem.get_or_insert_with(|| {
                            failure("pending reader has no original deadline")
                        });
                        *control = ControlFlow::Exit;
                    }
                }
                Err(error) => {
                    self.retirement.problem.get_or_insert(error);
                    *control = ControlFlow::Exit;
                }
            }
        }
    }
    fn finish(self, engine: WkWebViewEngine, code: i32) -> Result<Value, WvError> {
        let custody_retained_to_child_exit = !self.views.is_empty();
        if custody_retained_to_child_exit {
            // Dedicated child only: retain actual unproved owners until exit.
            // A missing reader-termination wake is reported, never bypassed.
            std::mem::forget(self.views);
            std::mem::forget(self.bridges);
            std::mem::forget(engine);
        }
        if let Some(error) = self.retirement.problem {
            let released = self.retirement.released;
            let reader_completion_wakes = self.observation.reader_wakes;
            return Err(failure(format!(
                "{error}; native_release_observed={released}; custody_retained_to_child_exit={custody_retained_to_child_exit}; accepted_post_join_reader_wakes={reader_completion_wakes}"
            )));
        }
        if code != 0 || !self.navigation.committed || !self.navigation.finished {
            return Err(failure(
                "actual commit/finish/release observables are incomplete",
            ));
        }
        if self.case == "preflight-preserves"
            && !(self.observation.preflight_checked && self.observation.preflight_refused)
        {
            return Err(failure(
                "actual nonfatal preflight outcome/state preservation not observed",
            ));
        }
        let page = self
            .observation
            .page
            .ok_or_else(|| failure("no physical page observation"))?;
        Ok(json!({
            "schema": "keld.s1.resource-smoke.v1", "case": self.case,
            "source_admission_scope": "trusted inert fixture manifest; GH499/application admission NOT RUN",
            "policy_scope": "opaque verified fixed-fixture catalog selection; wasm-denial is test-only restriction",
            "actual_s1_handler": true, "original_app_executed": false, "bun_started": false,
            "qualified_commit_observed": self.navigation.committed, "actual_committed_url": self.navigation.committed_url,
            "actual_url_forwarded_from_native_callback": self.navigation.forwarded_url.as_ref().map(super::MacNavigationUrl::as_str),
            "finish_matched_current_native_commit": self.navigation.finished && self.navigation.forwarded_stamp.is_some(),
            "native_finish_observed": self.navigation.finished,
            "actual_nonfatal_preflight_preserved_document": self.observation.preflight_checked && self.observation.preflight_refused,
            "native_release_observed": self.retirement.released, "loop_exit": code,
            "accepted_post_join_reader_wakes": self.observation.reader_wakes,
            "retirement_status": if custody_retained_to_child_exit {
                "CLEANUP_UNPROVED_READER_TERMINATION"
            } else { "ACTUAL_RELEASE_OBSERVED" },
            "custody_retained_to_child_exit": custody_retained_to_child_exit,
            "shipping_profile_enforcement": "NOT PROVED",
            "shipping_csp_selector": "UNKNOWN/NOT PROVED", "fixture_csp_selector_verified": true,
            "gh499_original_graph_admission": "NOT PROVED",
            "policy": self.source.response_policy(), "verified_policy_window_key": self.source.window_key(), "verified_policy_sha256": hex(&self.source.verified_manifest_sha256()), "page": page,
            "verified_worker_reads_not_native_callback_census": self.observation.reads,
        }))
    }
}
