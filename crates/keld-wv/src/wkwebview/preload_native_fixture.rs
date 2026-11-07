//! Debug-only real-WK original-runtime identity and completion-custody probe.
//! Its Rust test port is not a Bun role, `HostGuard` or product round-trip proof.

use super::managed_windows::{self, NativeBuild};
use super::{
    AdmittedWebAppSource, AppPreloadBudget, AppWindowCommand, AppWindowEvent,
    MacNativeDocumentLease, OriginalDocumentDestroyAdmission, OriginalDocumentDestroyDecision,
    RendererBridgeOperation, RendererBridgeOutcome, WkUserEvent, WkWebViewEngine,
};
use crate::engine::{NativeNavigationContext, OriginalEntryQuery};
use crate::{WebviewId, WvError};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use tao::event::Event;
use tao::event_loop::ControlFlow;
use tao::platform::run_return::EventLoopExtRunReturn;
use wry::WebViewExtMacOS;

fn failure(detail: impl Into<String>) -> WvError {
    WvError::EventLoop(format!("preload native fixture: {}", detail.into()))
}
fn load(
    view: WebviewId,
    operation: u64,
    query: &OriginalEntryQuery,
) -> Result<AppWindowCommand, WvError> {
    Ok(AppWindowCommand::LoadAdmittedOriginal {
        context: NativeNavigationContext::new(
            NonZeroU64::new(operation).ok_or_else(|| failure("zero fixture operation"))?,
            view,
        ),
        query: query.clone(),
    })
}
fn register_script(topic: &str) -> String {
    // Fixed first-party probe, not an original application caller or arbitrary module.
    format!(
        r"(()=>{{
      const doc=document; if(!globalThis.__preloadFixtureLog){{
        const values=[];globalThis.__preloadFixtureLog=values;
        for(const name of ['__keld_page_call_data_v1','__keld_app_callback_data_v1','__keld_page_callback_result_v1','__keld_app_ready_data_v1'])
          doc.addEventListener(name,event=>{{if(values.length<32)values.push(typeof event.detail==='string'?event.detail:String(event.detail));}});
      }}
      window.electron.registerMsgListener('{topic}',value=>{{
        const key=value==='old'?'data-old-effects':'data-fresh-effects';
        doc.documentElement.setAttribute(key,String(Number(doc.documentElement.getAttribute(key)||0)+1));
        doc.documentElement.setAttribute('data-last-callback-value',typeof value==='string'?value:'unexpected-type');
      }});
    }})()"
    )
}

fn waiting_for(case: &str, stage: u8) -> &'static str {
    let family = if case == "preload-stale-close-cut" {
        "preload-replay"
    } else {
        case
    };
    match (family, stage) {
        ("preload-replay", 0) => "A finished and native credential/registry/custody ready",
        ("preload-replay", 1) => "actual A page call reached held native ingress",
        ("preload-replay", 2) => "second actual same-URL commit finished and B native ready",
        ("preload-replay", 3) => "fresh B original Function finished exactly once and native idle",
        ("preload-replay", 4) => {
            "actual replay WK completion and exactly one old native callback rejection and native idle"
        }
        ("preload-replay", 5) => "old-topic event actual Consumed and WK EventQuiesced",
        ("preload-replay", 6) => "fresh-topic event actual Consumed and WK EventQuiesced",
        ("preload-replay", 7) => "actual current B page snapshot callback",
        ("preload-held-bootstrap", 0) => "A native Bound held and actual load finished",
        ("preload-held-bootstrap", 1) => {
            "B same-URL commit finished and both native Bound data held"
        }
        ("preload-held-bootstrap", 2) => "actual old Bound WK completion",
        ("preload-held-bootstrap", 3) => "actual fresh Bound WK completion and B registry ready",
        ("preload-held-bootstrap", 4) => "fresh B original Function finished once",
        ("preload-held-bootstrap", 5) => "fresh B event actual Consumed and WK EventQuiesced",
        ("preload-origin", 0) => "actual load finished and native registry ready",
        ("preload-origin", 1) => "actual private-world origin message",
        ("preload-postbind-burst", 1) => {
            "four native call ingresses while the first real WK call completion is held"
        }
        ("preload-postbind-burst", 2) => {
            "four original Functions finished and the actual rendererReq reached the test receiver"
        }
        ("preload-postbind-burst", 3) => "original mainResp handler and actual event quiescence",
        ("preload-postbind-burst", 4) => {
            "three ordered local listener callbacks and event quiescence"
        }
        ("preload-postbind-burst", 5) => "actual current page callback census",
        (_, 0) => "actual load finished and native registry ready or captured hook failure",
        (_, 1) => "original local registration finished once and native idle",
        (_, 2) => "actual consumer receipt and held actual WK completion",
        (_, 3) => "released real WK completion and physical native release",
        _ => "bounded fixture state transition",
    }
}

/// Run one dedicated child on the real `AppKit` thread using the existing owner.
/// The caller supplies the fixed independently verified inert S1 source policy.
/// This never qualifies actual Host/Bun grants, ring release or draw.io UX.
pub(super) fn run(source: &Arc<AdmittedWebAppSource>, case: &str) -> Result<Value, WvError> {
    if !matches!(
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
        return Err(failure("unknown bounded case"));
    }
    let budget = AppPreloadBudget::default();
    let image = Some(PreloadFixture::image(&budget)?);
    let mut engine = WkWebViewEngine::new_dev_ephemeral()?;
    let mut event_loop = engine
        .event_loop
        .take()
        .ok_or_else(|| failure("event loop unavailable"))?;
    let proxy = event_loop.create_proxy();
    let (requests, native_requests) = mpsc::sync_channel(1);
    let endpoint = (requests, 7_u16);
    let mut fixture = PreloadFixture::new(source, case, proxy, native_requests, budget, image)?;
    let code = event_loop.run_return(|event, target, control| {
        if matches!(&event, Event::LoopDestroyed) {
            return;
        }
        *control = ControlFlow::WaitUntil(fixture.deadline);
        let native = NativeBuild {
            target,
            profile: &engine.profile,
            startup: &engine.startup,
            media_seed_allow: false,
            renderer_endpoint: Some(&endpoint),
            resource_source: None,
            resource_wake: fixture.proxy.clone(),
            resource_events: Some(fixture.events.clone()),
        };
        let turn = fixture.turn(event, &native);
        fixture.finish_turn(turn, control);
    });
    fixture.finish(engine, event_loop, code)
}

#[derive(Default)]
struct NavigationObservation {
    lease: Option<MacNativeDocumentLease>,
    old_lease: Option<MacNativeDocumentLease>,
    first_url: Option<String>,
    commits: u32,
    started: u32,
    last_fact: Value,
    finished: bool,
}
#[derive(Default)]
struct DeliveryObservation {
    id: u32,
    consumed: bool,
    quiesced: bool,
    preflight_facts_seen: bool,
    disposition: Option<String>,
}
#[derive(Default)]
struct BurstObservation {
    base_ingress: u64,
    preflight_seen: bool,
    send_seen: bool,
    preflight_count: usize,
    send_count: usize,
    aborted_count: usize,
}
#[derive(Default)]
struct PageObservation {
    inflight: bool,
    value: Option<Value>,
}
#[derive(Default)]
struct ConditionalObservation {
    stale_refusal_observed: bool,
    cancelled_refusal_observed: bool,
    conditional_release_observed: bool,
}
struct ScenarioProgress {
    stage: u8,
    last_diagnostic_stage: Option<u8>,
    base_calls: u32,
    expected_live_successor: NativeSuccessorFacts,
    held_original_close: Option<AppWindowCommand>,
    stale_admission: Option<Arc<OriginalDocumentDestroyAdmission>>,
    conditional_admission: Option<Arc<OriginalDocumentDestroyAdmission>>,
    close: ConditionalObservation,
}
#[derive(Default)]
struct RetirementObservation {
    held_release_observed: bool,
    released: bool,
}
#[derive(Default)]
struct FixtureEnding {
    terminal: bool,
    problem: Option<WvError>,
    report: Option<Value>,
}
struct FixtureViews {
    bridges: BTreeMap<u32, super::MacRendererBridge>,
    views: BTreeMap<u32, super::View>,
    next_id: u32,
}
impl FixtureViews {
    fn command(
        &mut self,
        native: &NativeBuild<'_>,
        events: &mpsc::Sender<AppWindowEvent>,
        command: AppWindowCommand,
    ) -> Result<(), WvError> {
        managed_windows::command(
            command,
            native,
            &mut self.next_id,
            &mut self.views,
            &mut self.bridges,
            events,
        )
    }
    fn view(&self, id: WebviewId) -> Result<&super::View, WvError> {
        self.views.get(&id.0).ok_or_else(|| failure("view absent"))
    }
    fn bridge(&self, id: WebviewId) -> Result<&super::MacRendererBridge, WvError> {
        self.bridges
            .get(&id.0)
            .ok_or_else(|| failure("bridge absent"))
    }
}

struct PreloadFixture<'a> {
    platform: FixtureViews,
    source: &'a Arc<AdmittedWebAppSource>,
    case: &'a str,
    budget: AppPreloadBudget,
    image: Option<Arc<super::AppPreloadImage>>,
    proxy: tao::event_loop::EventLoopProxy<WkUserEvent>,
    events: mpsc::Sender<AppWindowEvent>,
    incoming: mpsc::Receiver<AppWindowEvent>,
    native_requests: mpsc::Receiver<super::RendererBridgeRequest>,
    receiver_role: super::RendererRoleWitness,
    initialized: bool,
    selected: Option<WebviewId>,
    query: OriginalEntryQuery,
    deadline: Instant,
    navigation: NavigationObservation,
    delivery: DeliveryObservation,
    burst: BurstObservation,
    page: PageObservation,
    progress: ScenarioProgress,
    retirement: RetirementObservation,
    ending: FixtureEnding,
}
impl<'a> PreloadFixture<'a> {
    fn new(
        source: &'a Arc<AdmittedWebAppSource>,
        case: &'a str,
        proxy: tao::event_loop::EventLoopProxy<WkUserEvent>,
        native_requests: mpsc::Receiver<super::RendererBridgeRequest>,
        budget: AppPreloadBudget,
        image: Option<Arc<super::AppPreloadImage>>,
    ) -> Result<Self, WvError> {
        let (events, incoming) = mpsc::channel();
        let expected_live_successor = NativeSuccessorFacts {
            document: NativeDocumentFacts {
                lease_current: true,
                view_matches: true,
                commit_matches: true,
            },
            source: NativeSourceFacts {
                lease_matches: true,
                owner_matches: true,
            },
            runtime: NativeRuntimeFacts {
                registered: true,
                reader_shutdown: false,
                reader_cancelled: false,
            },
            image_bytes: Some(1657),
        };
        let deadline = Instant::now() + Duration::from_secs(25);
        let query = OriginalEntryQuery::new("inert-preload-native=1").map_err(failure)?;
        Ok(Self {
            platform: FixtureViews {
                bridges: BTreeMap::new(),
                views: BTreeMap::new(),
                next_id: 1,
            },
            source,
            case,
            budget,
            image,
            proxy,
            events,
            incoming,
            native_requests,
            receiver_role: super::RendererRoleWitness::new(Arc::new(
                std::sync::atomic::AtomicBool::new(false),
            )),
            initialized: false,
            selected: None,
            query,
            deadline,
            navigation: NavigationObservation::default(),
            delivery: DeliveryObservation::default(),
            burst: BurstObservation::default(),
            page: PageObservation::default(),
            progress: ScenarioProgress {
                stage: 0,
                last_diagnostic_stage: None,
                base_calls: 0,
                expected_live_successor,
                held_original_close: None,
                stale_admission: None,
                conditional_admission: None,
                close: ConditionalObservation::default(),
            },
            retirement: RetirementObservation::default(),
            ending: FixtureEnding::default(),
        })
    }
    fn image(budget: &AppPreloadBudget) -> Result<Arc<super::AppPreloadImage>, WvError> {
        use sha2::Digest as _;
        let bytes = serde_json::from_str::<String>(include_str!(
            "../../tests/fixtures/generic-original-preload.json"
        ))
        .map_err(|error| failure(error.to_string()))?
        .into_bytes();
        // Independent pin of the generic inert preload, not draw.io's preload.
        if bytes.len() != 1657
            || format!("{:x}", sha2::Sha256::digest(&bytes))
                != "83018b62154e41fe2459a65a4bc2c4f69bbc9fe03de00012b5517590d6538477"
        {
            return Err(failure("generic native fixture preload changed"));
        }
        let selected_sha256 = sha2::Sha256::digest(&bytes).into();
        let image = Arc::new(
            budget.reserve_window()?.from_qualified_graph(
                bytes,
                selected_sha256,
                vec![],
                [
                    (String::from("keld"), env!("CARGO_PKG_VERSION").to_owned()),
                    // Controlled metadata for this no-Bun native fixture only.
                    (String::from("bun"), String::from("1.4.4")),
                ]
                .into(),
            )?,
        );
        Ok(image)
    }
    fn turn(
        &mut self,
        event: Event<'_, WkUserEvent>,
        native: &NativeBuild<'_>,
    ) -> Result<(), WvError> {
        if !self.initialized {
            self.initialized = true;
            self.platform.command(
                native,
                &self.events,
                AppWindowCommand::CreateWindow {
                    build: 1,
                    width: 640,
                    height: 480,
                    x: None,
                    y: None,
                    original_source: Some(Arc::clone(self.source)),
                    preload_image: self.image.take(),
                },
            )?;
        }
        self.consume_platform_event(event)?;
        self.consume_app_observations(native)?;
        let Some(id) = self.selected else {
            return Ok(());
        };
        self.consume_renderer_requests(id)?;
        if self.ending.terminal {
            return Ok(());
        }
        match self.case {
            "preload-attempt-hook-denied" | "preload-attempt-hook-reentrant" => {}
            "preload-control-error" => self.advance_control(id)?,
            "preload-preflight-facts" => self.advance_preflight(id)?,
            "preload-postbind-burst"
            | "preload-postbind-effect-burst"
            | "preload-postbind-refusal-order"
            | "preload-postbind-retire-order" => self.advance_burst(id)?,
            "preload-origin" => self.advance_origin(id)?,
            "preload-held-bootstrap" => self.advance_bootstrap(native, id)?,
            "preload-replay" | "preload-stale-close-cut" => self.advance_replay(native, id)?,
            _ => self.advance_held(native, id)?,
        }
        Ok(())
    }
    fn consume_platform_event(&mut self, event: Event<'_, WkUserEvent>) -> Result<(), WvError> {
        match event {
            Event::UserEvent(WkUserEvent::Resource(completion)) => {
                if let Some(resource) = self
                    .platform
                    .views
                    .get(&completion.view.0)
                    .and_then(|view| view.resource.as_ref())
                {
                    resource.handler.deliver(completion);
                }
            }
            Event::UserEvent(WkUserEvent::ReaderTerminated(completion)) => {
                if let Some(resource) = self
                    .platform
                    .views
                    .get(&completion.view.0)
                    .and_then(|view| view.resource.as_ref())
                {
                    resource.handler.observe_reader_termination(&completion);
                }
            }
            Event::UserEvent(WkUserEvent::RendererCustody) => {
                for (id, bridge) in &self.platform.bridges {
                    if let Some(view) = self.platform.views.get(id) {
                        bridge.service_original_calls(&view.webview.webview())?;
                    } else {
                        bridge.flush_controls();
                    }
                }
            }
            Event::UserEvent(WkUserEvent::PreloadFixtureSnapshot {
                view,
                document,
                raw,
            }) => {
                self.page.inflight = false;
                if self.selected == Some(view) && document.is_current() {
                    self.page.value = Some(super::s1_resource_fixture::snapshot(&raw)?);
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn consume_app_observations(&mut self, native: &NativeBuild<'_>) -> Result<(), WvError> {
        while let Ok(observation) = self.incoming.try_recv() {
            match observation {
                AppWindowEvent::WindowBuilt { view, .. } => {
                    self.selected = Some(view);
                    if self.case == "preload-held-bootstrap" {
                        self.platform
                            .bridges
                            .get(&view.0)
                            .ok_or_else(|| failure("bridge absent"))?
                            .fixture_hold_bindings();
                    }
                    if matches!(
                        self.case,
                        "preload-attempt-hook-denied" | "preload-attempt-hook-reentrant"
                    ) {
                        self.platform
                            .bridges
                            .get(&view.0)
                            .ok_or_else(|| failure("bridge absent"))?
                            .fixture_attempt_hook_fault(
                                if self.case == "preload-attempt-hook-denied" {
                                    1
                                } else {
                                    2
                                },
                            );
                    }
                    self.platform
                        .command(native, &self.events, load(view, 1, &self.query)?)?;
                }
                AppWindowEvent::WindowBuildFailed { .. } => {
                    return Err(failure("real native build failed"));
                }
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
        match fact {
            crate::engine::MacOriginalNavigationFact::Committed {
                stamp,
                native_url,
                document,
            } => {
                if self.selected != Some(context.view())
                    || stamp != document.stamp()
                    || !document.is_current()
                    || !document.belongs_to_source(self.source)
                {
                    return Err(failure("commit lease/source mismatch"));
                }
                if let Some(first) = self.navigation.first_url.as_ref() {
                    if first != native_url.as_str() {
                        return Err(failure(
                            "reload did not use exactly the same actual native URL",
                        ));
                    }
                } else {
                    self.navigation.first_url = Some(native_url.as_str().to_owned());
                }
                self.navigation.last_fact = json!({"kind":"committed","operation":context.operation().get(),"view":context.view().0,"generation":stamp.observed_generation()});
                self.navigation.commits += 1;
                self.navigation.lease = Some(document);
            }
            crate::engine::MacOriginalNavigationFact::Finished { stamp } => {
                self.navigation.last_fact = json!({"kind":"finished","operation":context.operation().get(),"view":context.view().0,"generation":stamp.observed_generation()});
                self.navigation.finished = true;
            }
            crate::engine::MacOriginalNavigationFact::Started { attempt } => {
                self.navigation.last_fact = json!({"kind":"started","operation":context.operation().get(),"view":context.view().0,"attempt_view":attempt.view().0});
                self.navigation.started += 1;
            }
            crate::engine::MacOriginalNavigationFact::Failed { attempt, revoked }
                if matches!(
                    self.case,
                    "preload-attempt-hook-denied" | "preload-attempt-hook-reentrant"
                ) =>
            {
                self.navigation.last_fact = json!({"kind":"failed","operation":context.operation().get(),"view":context.view().0,"attempt_view":attempt.view().0,"revoked":revoked.is_some()});
                if self.navigation.started != 0 || self.navigation.commits != 0 {
                    return Err(failure(
                        "failed pre-load hook still started/committed native load",
                    ));
                }
                let native_calls = self
                    .platform
                    .views
                    .get(&context.view().0)
                    .and_then(|view| view.resource.as_ref())
                    .ok_or_else(|| failure("native navigation owner absent"))?
                    .navigation
                    .fixture_native_load_calls();
                if native_calls != 0 {
                    return Err(failure(
                        "failed/reentrant hook still called actual native loadRequest",
                    ));
                }
                self.ending.report = Some(
                    json!({"real_native_attempt_hook_failed":true,"native_started":0,"native_commits":0,"same_native_owner_load_request_calls":native_calls}),
                );
                self.ending.terminal = true;
            }
            crate::engine::MacOriginalNavigationFact::Failed { attempt, revoked } => {
                self.navigation.last_fact = json!({"kind":"failed","operation":context.operation().get(),"view":context.view().0,"attempt_view":attempt.view().0,"revoked":revoked.is_some()});
                return Err(failure(format!(
                    "actual native operation{} failed for view{} (revoked={}); inspect same navigation owner failure-code observation",
                    context.operation(),
                    context.view().0,
                    revoked.is_some()
                )));
            }
            crate::engine::MacOriginalNavigationFact::Refused { error } => {
                self.navigation.last_fact = json!({"kind":"refused","operation":context.operation().get(),"view":context.view().0,"detail":error.to_string()});
                return Err(failure(format!(
                    "actual native operation{} refused for view{}: {error}",
                    context.operation(),
                    context.view().0
                )));
            }
            crate::engine::MacOriginalNavigationFact::Revoked { previous } => {
                self.navigation.last_fact = json!({"kind":"revoked","operation":context.operation().get(),"view":context.view().0,"previous_generation":previous.map(super::MacNativeDocumentStamp::observed_generation)});
            }
        }
        Ok(())
    }
    fn consume_renderer_requests(&mut self, id: WebviewId) -> Result<(), WvError> {
        while let Ok(request) = self.native_requests.try_recv() {
            if request.webview() != id {
                return Err(failure("foreign native request view"));
            }
            match request.operation() {
                RendererBridgeOperation::Preflight { channel }
                    if self.case == "preload-preflight-facts" =>
                {
                    self.refuse_fixture_preflight(id, &request, channel)?;
                }
                RendererBridgeOperation::Preflight { channel }
                    if matches!(
                        self.case,
                        "preload-postbind-burst"
                            | "preload-postbind-effect-burst"
                            | "preload-postbind-refusal-order"
                            | "preload-postbind-retire-order"
                    ) =>
                {
                    self.admit_burst_preflight(id, &request, channel)?;
                }
                RendererBridgeOperation::Send { channel }
                    if matches!(
                        self.case,
                        "preload-postbind-burst"
                            | "preload-postbind-effect-burst"
                            | "preload-postbind-refusal-order"
                            | "preload-postbind-retire-order"
                    ) =>
                {
                    self.consume_burst_send(id, &request, channel)?;
                }
                RendererBridgeOperation::PreflightAborted
                    if self.case == "preload-postbind-retire-order" =>
                {
                    let lease = request
                        .document_lease()
                        .ok_or_else(|| failure("cancelled preflight omitted original lease"))?;
                    if self.burst.aborted_count != 0
                        || request.request() != 4
                        || !lease.is_current()
                        || !lease.belongs_to_source(self.source)
                        || lease.stamp().view() != id
                        || u64::from(lease.stamp().observed_generation()) != request.navigation()
                    {
                        return Err(failure(
                            "cancelled preflight is repeated or targets another document",
                        ));
                    }
                    self.burst.aborted_count += 1;
                }
                RendererBridgeOperation::PreflightAborted if self.ending.terminal => {}
                RendererBridgeOperation::Registered => {
                    self.register_fixture_receiver(id, &request)?;
                }
                RendererBridgeOperation::Consumed {
                    disposition: actual,
                } => {
                    if request.request() != self.delivery.id {
                        return Err(failure("foreign delivery consumer"));
                    }
                    self.delivery.consumed = true;
                    self.delivery.disposition = Some(format!("{actual:?}"));
                }
                RendererBridgeOperation::EventQuiesced { completed } => {
                    if request.request() != self.delivery.id || !*completed {
                        return Err(failure("actual WK event evaluation failed"));
                    }
                    self.delivery.quiesced = true;
                }
                _ => {
                    return Err(failure(
                        "unexpected outbound effect in local identity fixture",
                    ));
                }
            }
            // Real removal from this one bounded test receiver supplies capacity.
            for (id, bridge) in &self.platform.bridges {
                if let Some(view) = self.platform.views.get(id) {
                    bridge.service_original_calls(&view.webview.webview())?;
                } else {
                    bridge.flush_controls();
                }
            }
        }
        Ok(())
    }
    fn register_fixture_receiver(
        &self,
        id: WebviewId,
        request: &super::RendererBridgeRequest,
    ) -> Result<(), WvError> {
        let document = request
            .document_lease()
            .ok_or_else(|| failure("actual native registration omitted lease"))?;
        if document.is_current() {
            if !document.belongs_to_source(self.source) || document.stamp().view() != id {
                return Err(failure("native registration has wrong source/view"));
            }
            // This is availability of the actual local Rust test
            // receiver, not an authenticated Bun subscription.
            self.platform
                .bridges
                .get(&id.0)
                .ok_or_else(|| failure("registered bridge absent"))?
                .deliver(
                    &self
                        .platform
                        .views
                        .get(&id.0)
                        .ok_or_else(|| failure("registered view absent"))?
                        .webview
                        .webview(),
                    RendererBridgeOutcome::RouteState {
                        webview: id,
                        navigation: request.navigation(),
                        ready: true,
                        role: self.receiver_role.clone(),
                    },
                )?;
        }
        Ok(())
    }
    fn refuse_fixture_preflight(
        &mut self,
        id: WebviewId,
        request: &super::RendererBridgeRequest,
        channel: &str,
    ) -> Result<(), WvError> {
        if self.delivery.preflight_facts_seen || channel != "rendererReq" {
            return Err(failure("unexpected repeated preflight or channel"));
        }
        let document = request
            .document_lease()
            .ok_or_else(|| failure("preflight omitted actual native lease"))?;
        let url = request
            .url()
            .ok_or_else(|| failure("preflight omitted actual native callback URL"))?;
        let origin = request.origin().ok_or_else(|| {
            failure("preflight omitted source-qualified actual native callback origin")
        })?;
        if origin.is_empty()
            || !document.is_current()
            || !document.belongs_to_source(self.source)
            || document.stamp().view() != id
            || u64::from(document.stamp().observed_generation()) != request.navigation()
        {
            return Err(failure(
                "preflight callback facts do not match actual current source/document",
            ));
        }
        self.source
            .qualify_document_url(url)
            .map_err(|_| failure("preflight callback URL belongs to another source"))?;
        self.delivery.preflight_facts_seen = true;
        // Explicit refusal proves producer facts without claiming
        // actual Host Guard/Bun admission or original effects.
        self.platform
            .bridges
            .get(&id.0)
            .ok_or_else(|| failure("preflight bridge absent"))?
            .deliver(
                &self
                    .platform
                    .views
                    .get(&id.0)
                    .ok_or_else(|| failure("preflight view absent"))?
                    .webview
                    .webview(),
                RendererBridgeOutcome::Preflight {
                    webview: id,
                    navigation: request.navigation(),
                    request: request.request(),
                    accepted: false,
                },
            )?;
        Ok(())
    }
    fn start_event(
        &self,
        id: WebviewId,
        request: u32,
        channel: &str,
        payload: Vec<u8>,
    ) -> Result<(), WvError> {
        let navigation = u64::from(
            self.navigation
                .lease
                .as_ref()
                .ok_or_else(|| failure("lease absent"))?
                .stamp()
                .observed_generation(),
        );
        self.platform.bridge(id)?.deliver(
            &self.platform.view(id)?.webview.webview(),
            RendererBridgeOutcome::Event {
                webview: id,
                navigation,
                request,
                channel: channel.to_owned(),
                payload,
            },
        )
    }
    fn advance_control(&mut self, id: WebviewId) -> Result<(), WvError> {
        let bridge = self
            .platform
            .bridges
            .get(&id.0)
            .ok_or_else(|| failure("bridge absent"))?;
        if self.progress.stage == 0 && self.navigation.finished && bridge.fixture_ready() {
            let diagnostics = bridge.fixture_diagnostics();
            if diagnostics["evaluation_failures_control_call_event_replay_binding"][0].as_u64()
                != Some(0)
            {
                return Err(failure(
                    "initial fixed bind controls reported native result failures",
                ));
            }
            bridge.fixture_throw_control(
                &self
                    .platform
                    .views
                    .get(&id.0)
                    .ok_or_else(|| failure("view absent"))?
                    .webview
                    .webview(),
            )?;
            self.progress.stage = 1;
        } else if self.progress.stage == 1 && bridge.fixture_ready() {
            let diagnostics = bridge.fixture_diagnostics();
            let actual = diagnostics["actual_control_errors"]
                .as_array()
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row["kind"] == "other-fixed-control"
                            && row["code"].as_i64() == Some(4)
                            && row["count"].as_u64() == Some(1)
                    })
                });
            if !actual
                || diagnostics["evaluation_failures_control_call_event_replay_binding"][0].as_u64()
                    != Some(1)
            {
                return Err(failure(
                    "actual thrown JS did not remain a native code4 failed completion",
                ));
            }
            self.progress.base_calls = bridge.fixture_finished_calls();
            self.platform
                .view(id)?
                .webview
                .evaluate_script(&register_script("fixture-current"))
                .map_err(|error| failure(error.to_string()))?;
            self.progress.stage = 2;
        } else if self.progress.stage == 2
            && bridge.fixture_ready()
            && bridge.fixture_finished_calls() == self.progress.base_calls + 1
        {
            self.delivery.id = 1;
            self.start_event(
                id,
                self.delivery.id,
                "fixture-current",
                vec![6, 1, 0, 5, 5, 0, 102, 114, 101, 115, 104],
            )?;
            self.progress.stage = 3;
        } else if self.progress.stage == 3 && self.delivery.consumed && self.delivery.quiesced {
            if self.delivery.disposition.as_deref() != Some("Consumed") {
                return Err(failure(
                    "healthy original event failed after deliberate control exception",
                ));
            }
            release_event(
                &self.platform.bridges,
                &self.platform.views,
                id,
                self.navigation
                    .lease
                    .as_ref()
                    .ok_or_else(|| failure("lease absent"))?,
                self.delivery.id,
            )?;
            self.ending.report = Some(
                json!({"initial_bind_control_result_failures":0,"actual_thrown_js_code":4,"actual_failed_control_receipt":true,"healthy_original_followup_consumed":true}),
            );
            self.ending.terminal = true;
        }
        Ok(())
    }
    fn advance_preflight(&mut self, id: WebviewId) -> Result<(), WvError> {
        let bridge = self
            .platform
            .bridges
            .get(&id.0)
            .ok_or_else(|| failure("bridge absent"))?;
        if self.progress.stage == 0 && self.navigation.finished && bridge.fixture_ready() {
            self.progress.base_calls = bridge.fixture_finished_calls();
            self.platform
                .view(id)?
                .webview
                .evaluate_script(
                    "window.electron.request({action:'fixture-preflight-facts'},()=>{},()=>{});",
                )
                .map_err(|error| failure(error.to_string()))?;
            self.progress.stage = 1;
        } else if self.progress.stage == 1
            && self.delivery.preflight_facts_seen
            && bridge.fixture_ready()
        {
            if bridge.fixture_finished_calls() != self.progress.base_calls {
                return Err(failure(
                    "denied preflight entered original effectful Function",
                ));
            }
            self.platform
                .view(id)?
                .webview
                .evaluate_script(&register_script("fixture-current"))
                .map_err(|error| failure(error.to_string()))?;
            self.progress.stage = 2;
        } else if self.progress.stage == 2
            && bridge.fixture_ready()
            && bridge.fixture_finished_calls() == self.progress.base_calls + 1
        {
            self.delivery.id = 1;
            self.start_event(
                id,
                self.delivery.id,
                "fixture-current",
                vec![6, 1, 0, 5, 5, 0, 102, 114, 101, 115, 104],
            )?;
            self.progress.stage = 3;
        } else if self.progress.stage == 3 && self.delivery.consumed && self.delivery.quiesced {
            if self.delivery.disposition.as_deref() != Some("Consumed") {
                return Err(failure(
                    "healthy local listener failed after explicit preflight refusal",
                ));
            }
            release_event(
                &self.platform.bridges,
                &self.platform.views,
                id,
                self.navigation
                    .lease
                    .as_ref()
                    .ok_or_else(|| failure("lease absent"))?,
                self.delivery.id,
            )?;
            self.ending.report = Some(
                json!({"actual_callback_url_present":true,"source_qualified_actual_callback_origin_present":true,"native_source_document_binding":true,"original_effectful_entries_before_refusal":0,"healthy_local_followup_consumed":true,"authenticated_host_bun_preflight_proved":false}),
            );
            self.ending.terminal = true;
        }
        Ok(())
    }
    fn check_burst_request(
        &self,
        id: WebviewId,
        request: &super::RendererBridgeRequest,
    ) -> Result<(), WvError> {
        let document = request
            .document_lease()
            .ok_or_else(|| failure("burst request omitted native document lease"))?;
        let url = request
            .url()
            .ok_or_else(|| failure("burst request omitted actual native callback URL"))?;
        if !document.is_current()
            || !document.belongs_to_source(self.source)
            || document.stamp().view() != id
            || u64::from(document.stamp().observed_generation()) != request.navigation()
            || request.origin().is_none_or(str::is_empty)
        {
            return Err(failure(
                "burst request has stale or foreign native source facts",
            ));
        }
        self.source
            .qualify_document_url(url)
            .map_err(|_| failure("burst request URL belongs to another admitted source"))?;
        Ok(())
    }
    fn admit_burst_preflight(
        &mut self,
        id: WebviewId,
        request: &super::RendererBridgeRequest,
        channel: &str,
    ) -> Result<(), WvError> {
        let topics = ["rendererReq", "fixture-save-result", "fixture-followup"];
        let count = if self.case == "preload-postbind-effect-burst" {
            3
        } else {
            1
        };
        if self.burst.preflight_count >= count
            || topics[self.burst.preflight_count] != channel
            || request
                .role_witness()
                .is_none_or(|role| !role.is_live() || !role.same_role(&self.receiver_role))
        {
            return Err(failure("burst preflight is repeated or has another topic"));
        }
        self.check_burst_request(id, request)?;
        self.burst.preflight_seen = true;
        self.burst.preflight_count += 1;
        if self.case == "preload-postbind-retire-order" {
            let old = self.receiver_role.clone();
            let successor = super::RendererRoleWitness::new(Arc::new(
                std::sync::atomic::AtomicBool::new(false),
            ));
            for (ready, role) in [
                (false, old.clone()),
                (true, successor.clone()),
                (false, old),
            ] {
                self.platform.bridge(id)?.deliver(
                    &self.platform.view(id)?.webview.webview(),
                    RendererBridgeOutcome::RouteState {
                        webview: id,
                        navigation: request.navigation(),
                        ready,
                        role,
                    },
                )?;
            }
            self.receiver_role = successor;
            return Ok(());
        }
        // This admits only the existing local Rust test receiver. It is neither
        // a HostGuard decision nor proof of an authenticated Bun subscription.
        self.platform.bridge(id)?.deliver(
            &self.platform.view(id)?.webview.webview(),
            RendererBridgeOutcome::Preflight {
                webview: id,
                navigation: request.navigation(),
                request: request.request(),
                accepted: self.case != "preload-postbind-refusal-order",
            },
        )
    }
    fn consume_burst_send(
        &mut self,
        id: WebviewId,
        request: &super::RendererBridgeRequest,
        channel: &str,
    ) -> Result<(), WvError> {
        // GH544 bytes for [{action:"isFullscreen",reqId:1}], emitted by the
        // canonical endpoint codec. The pinned original preload starts reqId at1.
        const EXPECTED: &[u8] = &[
            6, 1, 0, 7, 2, 0, 6, 0, 97, 99, 116, 105, 111, 110, 5, 12, 0, 105, 115, 70, 117, 108,
            108, 115, 99, 114, 101, 101, 110, 5, 0, 114, 101, 113, 73, 100, 4, 0, 0, 0, 0, 0, 0,
            240, 63,
        ];
        let topics = ["rendererReq", "fixture-save-result", "fixture-followup"];
        let count = if self.case == "preload-postbind-effect-burst" {
            3
        } else {
            1
        };
        let expected = match self.burst.send_count {
            0 => EXPECTED,
            1 => &[6, 1, 0, 3], // canonical endpoint arguments [true]
            _ => &[6, 1, 0, 2], // canonical endpoint arguments [false]
        };
        if self.burst.preflight_count != self.burst.send_count + 1
            || self.burst.send_count >= count
            || channel != topics[self.burst.send_count]
            || request.payload() != expected
        {
            return Err(failure(
                "burst did not produce the exact original rendererReq",
            ));
        }
        self.check_burst_request(id, request)?;
        self.burst.send_count += 1;
        self.burst.send_seen = self.burst.send_count == count;
        // Consumption is real at this test port; production transport is not
        // involved. The matching test response is sent only after this observation.
        self.platform.bridge(id)?.deliver(
            &self.platform.view(id)?.webview.webview(),
            RendererBridgeOutcome::SendResult {
                webview: id,
                navigation: request.navigation(),
                request: request.request(),
                disposition: crate::wv_link::WvTerminalDisposition::Consumed,
            },
        )
    }
    fn start_burst(&mut self, id: WebviewId, effect_burst: bool) -> Result<(), WvError> {
        let bridge = self.platform.bridge(id)?;
        self.progress.base_calls = bridge.fixture_finished_calls();
        self.burst.base_ingress = bridge.fixture_diagnostics()["call_ingress"]
            .as_u64()
            .ok_or_else(|| failure("native call ingress counter absent"))?;
        bridge.fixture_hold_call_completion();
        let extra = if effect_burst {
            "window.electron.sendMessage('fixture-save-result',true);window.electron.sendMessage('fixture-followup',false);"
        } else {
            ""
        };
        let script = format!(
            "(()=>{{globalThis.__preloadBurst={{order:[],responses:0,errors:0,response:null,refusals:[]}};document.addEventListener('__keld_page_refusal_data_v1',e=>globalThis.__preloadBurst.refusals.push(e.detail));for(const index of [1,2,3])window.electron.registerMsgListener('fixture-burst-listeners',value=>{{if(value==='fresh')globalThis.__preloadBurst.order.push(index);else globalThis.__preloadBurst.errors++;}});window.electron.request({{action:'isFullscreen'}},value=>{{globalThis.__preloadBurst.responses++;globalThis.__preloadBurst.response=value;}},()=>{{globalThis.__preloadBurst.errors++;}});{extra}}})()"
        );
        self.platform
            .view(id)?
            .webview
            .evaluate_script(&script)
            .map_err(|error| failure(error.to_string()))?;
        self.progress.stage = 1;
        Ok(())
    }
    fn advance_burst(&mut self, id: WebviewId) -> Result<(), WvError> {
        let effect_burst = self.case == "preload-postbind-effect-burst";
        let refusal = matches!(
            self.case,
            "preload-postbind-refusal-order" | "preload-postbind-retire-order"
        );
        let expected_calls = if effect_burst { 6 } else { 4 };
        let bridge = self.platform.bridge(id)?;
        if self.progress.stage == 0 && self.navigation.finished && bridge.fixture_ready() {
            self.start_burst(id, effect_burst)?;
        } else if self.progress.stage == 1 && bridge.fixture_completion_held() {
            let native = bridge.fixture_diagnostics();
            let ingress = native["call_ingress"].as_u64().unwrap_or(0);
            if ingress < self.burst.base_ingress + expected_calls
                || bridge.fixture_finished_calls() < self.progress.base_calls + 1
            {
                return Ok(());
            }
            let held_first = ingress == self.burst.base_ingress + expected_calls
                && bridge.fixture_finished_calls() == self.progress.base_calls + 1
                && native["active_call"]["function_finished"] == true
                && native["active_call"]["evaluation_finished"] == false;
            // Always return the actual receipt before a RED result, so the
            // negative control itself does not prevent physical view retirement.
            bridge.fixture_release_actual_completion()?;
            if !held_first {
                return Err(failure(
                    "burst displaced the first call before real WK release",
                ));
            }
            if !self.burst.preflight_seen {
                return Err(failure(
                    "four native ingresses but no request preflight while first WK completion held",
                ));
            }
            self.progress.stage = 2;
        } else if self.progress.stage == 2
            && refusal
            && self.burst.preflight_seen
            && bridge.fixture_ready()
            && bridge.fixture_finished_calls() == self.progress.base_calls + 3
        {
            let native = bridge.fixture_diagnostics();
            if self.burst.send_count != 0
                || native["refused_before_entry"] != 1
                || native["last_refusal_call"] != 4
                || native["last_refusal_after_calls"] != self.progress.base_calls + 3
                || native["local_retained_records"] != 0
            {
                return Err(failure(
                    "refusal overtook older local calls, entered an effect, or retained native charge",
                ));
            }
            self.delivery.id = 2;
            self.start_event(
                id,
                2,
                "fixture-burst-listeners",
                vec![6, 1, 0, 5, 5, 0, 102, 114, 101, 115, 104],
            )?;
            self.progress.stage = 4;
        } else if self.progress.stage == 2
            && self.burst.send_seen
            && bridge.fixture_ready()
            && u64::from(bridge.fixture_finished_calls())
                == u64::from(self.progress.base_calls) + expected_calls
        {
            self.delivery.id = 1;
            // Matching local-receiver response for the actually observed reqId1;
            // GH544 [{success:true,data:false,reqId:1}], never a product fake reply.
            self.start_event(
                id,
                self.delivery.id,
                "mainResp",
                vec![
                    6, 1, 0, 7, 3, 0, 4, 0, 100, 97, 116, 97, 2, 5, 0, 114, 101, 113, 73, 100, 4,
                    0, 0, 0, 0, 0, 0, 240, 63, 7, 0, 115, 117, 99, 99, 101, 115, 115, 3,
                ],
            )?;
            self.progress.stage = 3;
        } else if self.progress.stage == 3 && self.delivery.consumed && self.delivery.quiesced {
            self.release_burst_event(id)?;
            self.delivery.id = 2;
            self.start_event(
                id,
                self.delivery.id,
                "fixture-burst-listeners",
                vec![6, 1, 0, 5, 5, 0, 102, 114, 101, 115, 104],
            )?;
            self.progress.stage = 4;
        } else if self.progress.stage == 4 && self.delivery.consumed && self.delivery.quiesced {
            self.release_burst_event(id)?;
            self.progress.stage = 5;
        }
        if self.progress.stage == 5 {
            self.observe_burst_page(id)?;
        }
        Ok(())
    }
    fn release_burst_event(&mut self, id: WebviewId) -> Result<(), WvError> {
        if self.delivery.disposition.as_deref() != Some("Consumed") {
            return Err(failure("actual burst callback was not consumed"));
        }
        release_event(
            &self.platform.bridges,
            &self.platform.views,
            id,
            self.navigation
                .lease
                .as_ref()
                .ok_or_else(|| failure("lease absent"))?,
            self.delivery.id,
        )?;
        self.delivery.consumed = false;
        self.delivery.quiesced = false;
        self.delivery.disposition = None;
        Ok(())
    }
    fn observe_burst_page(&mut self, id: WebviewId) -> Result<(), WvError> {
        if !self.page.inflight && self.page.value.is_none() {
            self.page.inflight = true;
            let wake = self.proxy.clone();
            let document = self
                .navigation
                .lease
                .clone()
                .ok_or_else(|| failure("lease absent"))?;
            self.platform
                .view(id)?
                .webview
                .evaluate_script_with_callback(
                    "JSON.stringify(globalThis.__preloadBurst)",
                    move |raw| {
                        let _ = wake.send_event(WkUserEvent::PreloadFixtureSnapshot {
                            view: id,
                            document: document.clone(),
                            raw,
                        });
                    },
                )
                .map_err(|error| failure(error.to_string()))?;
        }
        if let Some(page) = self.page.value.take() {
            let refused = matches!(
                self.case,
                "preload-postbind-refusal-order" | "preload-postbind-retire-order"
            );
            if page["order"] != json!([1, 2, 3])
                || page["responses"].as_u64() != Some(u64::from(!refused))
                || page["errors"].as_u64() != Some(0)
                || if refused {
                    page["response"] != serde_json::Value::Null || page["refusals"] != json!([4])
                } else {
                    page["response"] != false
                }
            {
                return Err(failure(
                    "burst original callback or listener-order census failed",
                ));
            }
            let counts = self.platform.bridge(id)?.fixture_counts();
            if counts.2 != 0 {
                return Err(failure(
                    "burst completed while native evaluations remain retained",
                ));
            }
            self.ending.report = Some(json!({
                "native_call_ingresses":if self.case == "preload-postbind-effect-burst" {6} else {4},
                "original_function_completions":if refused {3} else if self.case == "preload-postbind-effect-burst" {6} else {4},
                "effectful_sends":self.burst.send_count,
                "held_actual_first_call_completion":true,"request_preflight_before_release":true,
                "actual_original_renderer_req_observed":!refused,"original_main_resp_callback_observed":!refused,
                "one_ordered_private_refusal_after_older_locals":refused,
                "late_old_role_retirement_cannot_retire_successor":self.case == "preload-postbind-retire-order",
                "local_listener_order":[1,2,3],"actual_wk_completions":counts.0,
                "native_consumed":counts.1,"pending_evaluations":counts.2,"page":page,
                "authenticated_host_bun_preflight_proved":false,
                "second_effectful_call_qualified":self.case == "preload-postbind-effect-burst"
            }));
            self.ending.terminal = true;
        }
        Ok(())
    }
    fn advance_origin(&mut self, id: WebviewId) -> Result<(), WvError> {
        if self.progress.stage == 0
            && self.navigation.finished
            && self
                .platform
                .bridges
                .get(&id.0)
                .is_some_and(super::macos_bridge::MacRendererBridge::fixture_ready)
        {
            self.platform.bridge(id)?.fixture_origin_probe(
                &self
                    .platform
                    .views
                    .get(&id.0)
                    .ok_or_else(|| failure("view absent"))?
                    .webview
                    .webview(),
            )?;
            self.progress.stage = 1;
        }
        if self.progress.stage == 1
            && let Some(origin) = self
                .platform
                .bridges
                .get(&id.0)
                .and_then(super::macos_bridge::MacRendererBridge::fixture_origin_result)
        {
            self.ending.report =
                Some(json!({"origin_probe":origin,"shipping_origin_qualification":false}));
            self.ending.terminal = true;
        }
        Ok(())
    }
    fn advance_bootstrap(
        &mut self,
        native: &NativeBuild<'_>,
        id: WebviewId,
    ) -> Result<(), WvError> {
        let bridge = self
            .platform
            .bridges
            .get(&id.0)
            .ok_or_else(|| failure("bridge absent"))?;
        if self.progress.stage == 0
            && self.navigation.finished
            && bridge.fixture_held_bindings() == 1
            && bridge.fixture_evaluations_idle()
        {
            if bridge.fixture_registered() {
                return Err(failure(
                    "A bound while its real native bootstrap reply was held",
                ));
            }
            self.navigation.old_lease.clone_from(&self.navigation.lease);
            self.navigation.finished = false;
            self.platform
                .command(native, &self.events, load(id, 2, &self.query)?)?;
            self.progress.stage = 1;
        } else if self.progress.stage == 1
            && self.navigation.commits == 2
            && self.navigation.finished
            && bridge.fixture_held_bindings() == 2
            && bridge.fixture_evaluations_idle()
        {
            if bridge.fixture_registered()
                || self
                    .navigation
                    .old_lease
                    .as_ref()
                    .is_none_or(MacNativeDocumentLease::is_current)
            {
                return Err(failure("B registered early or A lease survived"));
            }
            bridge.fixture_replay_binding(
                &self
                    .platform
                    .views
                    .get(&id.0)
                    .ok_or_else(|| failure("view absent"))?
                    .webview
                    .webview(),
                0,
            )?;
            self.progress.stage = 2;
        } else if self.progress.stage == 2 && bridge.fixture_binding_completed(0).is_some() {
            if bridge.fixture_binding_completed(0) != Some(true) {
                return Err(failure("actual old Bound WK evaluation failed"));
            }
            if bridge.fixture_registered() {
                return Err(failure("delayed Bound(A) changed successor B registration"));
            }
            bridge.fixture_replay_binding(
                &self
                    .platform
                    .views
                    .get(&id.0)
                    .ok_or_else(|| failure("view absent"))?
                    .webview
                    .webview(),
                1,
            )?;
            self.progress.stage = 3;
        } else if self.progress.stage == 3
            && bridge.fixture_ready()
            && bridge.fixture_binding_completed(1).is_some()
        {
            if bridge.fixture_binding_completed(1) != Some(true) {
                return Err(failure("actual fresh Bound WK evaluation failed"));
            }
            self.progress.base_calls = bridge.fixture_finished_calls();
            self.platform
                .view(id)?
                .webview
                .evaluate_script(&register_script("fixture-current"))
                .map_err(|error| failure(error.to_string()))?;
            self.progress.stage = 4;
        } else {
            self.complete_bootstrap_control(id)?;
        }
        Ok(())
    }
    fn complete_bootstrap_control(&mut self, id: WebviewId) -> Result<(), WvError> {
        let bridge = self.platform.bridge(id)?;
        if self.progress.stage == 4
            && bridge.fixture_ready()
            && bridge.fixture_finished_calls() == self.progress.base_calls + 1
        {
            self.delivery.id = 1;
            self.start_event(
                id,
                self.delivery.id,
                "fixture-current",
                vec![6, 1, 0, 5, 5, 0, 102, 114, 101, 115, 104],
            )?;
            self.progress.stage = 5;
        } else if self.progress.stage == 5 && self.delivery.consumed && self.delivery.quiesced {
            if self.delivery.disposition.as_deref() != Some("Consumed") {
                return Err(failure(
                    "fresh B admission failed after delayed A bootstrap",
                ));
            }
            release_event(
                &self.platform.bridges,
                &self.platform.views,
                id,
                self.navigation
                    .lease
                    .as_ref()
                    .ok_or_else(|| failure("lease absent"))?,
                self.delivery.id,
            )?;
            self.ending.report = Some(
                json!({"same_actual_url":true,"native_commits":self.navigation.commits,"genuine_bound_a_before_bound_b":true,"successor_credential_registration_mutations_from_a":0,"fresh_b_original_function_and_event":true,"delivery_target":"native-main-frame-explicit-app-world"}),
            );
            self.ending.terminal = true;
        }
        Ok(())
    }
    fn advance_replay(&mut self, native: &NativeBuild<'_>, id: WebviewId) -> Result<(), WvError> {
        match self.progress.stage {
            0 | 1 => self.begin_replay(native, id)?,
            2 => self.prepare_replay_successor(native, id)?,
            3 | 4 => self.replay_old_message(id)?,
            5 | 6 => self.complete_replay_events(id)?,
            _ => {}
        }
        self.observe_replay_page(id)
    }
    fn begin_replay(&mut self, native: &NativeBuild<'_>, id: WebviewId) -> Result<(), WvError> {
        let ready = self
            .platform
            .bridges
            .get(&id.0)
            .is_some_and(super::macos_bridge::MacRendererBridge::fixture_ready);
        if self.progress.stage == 0 && self.navigation.finished && ready {
            self.platform.bridge(id)?.fixture_hold_next_call();
            self.platform
                .view(id)?
                .webview
                .evaluate_script(&register_script("fixture-old"))
                .map_err(|error| failure(error.to_string()))?;
            self.progress.stage = 1;
        } else if self.progress.stage == 1
            && self
                .platform
                .bridges
                .get(&id.0)
                .is_some_and(super::macos_bridge::MacRendererBridge::fixture_has_held_call)
        {
            self.navigation.old_lease.clone_from(&self.navigation.lease);
            self.navigation.finished = false;
            if self.case == "preload-stale-close-cut" {
                let expected = self
                    .navigation
                    .old_lease
                    .clone()
                    .ok_or_else(|| failure("actual A lease absent"))?;
                if !expected.is_current() || !expected.belongs_to_source(self.source) {
                    return Err(failure(
                        "A close command was not captured from actual current source/document",
                    ));
                }
                let admission = Arc::new(OriginalDocumentDestroyAdmission::new());
                self.progress.held_original_close =
                    Some(AppWindowCommand::DestroyOriginalDocument {
                        view: id,
                        expected,
                        admission: Arc::clone(&admission),
                    });
                self.progress.stale_admission = Some(admission);
            }
            self.platform
                .command(native, &self.events, load(id, 2, &self.query)?)?;
            self.progress.stage = 2;
            if self
                .navigation
                .old_lease
                .as_ref()
                .is_none_or(MacNativeDocumentLease::is_current)
            {
                return Err(failure(
                    "host load did not revoke old native lease before commit",
                ));
            }
        }
        Ok(())
    }
    fn prepare_replay_successor(
        &mut self,
        native: &NativeBuild<'_>,
        id: WebviewId,
    ) -> Result<(), WvError> {
        let ready = self
            .platform
            .bridges
            .get(&id.0)
            .is_some_and(super::macos_bridge::MacRendererBridge::fixture_ready);
        if self.progress.stage == 2
            && self.navigation.commits == 2
            && self.navigation.finished
            && ready
        {
            if self
                .navigation
                .old_lease
                .as_ref()
                .zip(self.navigation.lease.as_ref())
                .is_none_or(|(old, new)| old.same_binding(new) || old.is_current())
            {
                return Err(failure("same-URL replacement reused the old native lease"));
            }
            if self.case == "preload-stale-close-cut" {
                let successor = self
                    .navigation
                    .lease
                    .clone()
                    .ok_or_else(|| failure("actual B lease absent"))?;
                self.refuse_stale_close(native, id, &successor)?;
                self.cancel_current_close(native, id, &successor)?;
            }
            self.progress.base_calls = self
                .platform
                .bridges
                .get(&id.0)
                .ok_or_else(|| failure("bridge absent"))?
                .fixture_finished_calls();
            self.platform
                .view(id)?
                .webview
                .evaluate_script(&register_script("fixture-current"))
                .map_err(|error| failure(error.to_string()))?;
            self.progress.stage = 3;
        }
        Ok(())
    }
    fn replay_old_message(&mut self, id: WebviewId) -> Result<(), WvError> {
        let ready = self
            .platform
            .bridges
            .get(&id.0)
            .is_some_and(super::macos_bridge::MacRendererBridge::fixture_ready);
        if self.progress.stage == 3
            && ready
            && self.platform.bridges.get(&id.0).is_some_and(|bridge| {
                bridge.fixture_finished_calls() == self.progress.base_calls + 1
            })
        {
            self.progress.base_calls += 1;
            self.platform.bridge(id)?.fixture_replay_held(
                &self
                    .platform
                    .views
                    .get(&id.0)
                    .ok_or_else(|| failure("view absent"))?
                    .webview
                    .webview(),
            )?;
            self.progress.stage = 4;
        } else if self.progress.stage == 4
            && ready
            && self.platform.bridges.get(&id.0).is_some_and(|bridge| {
                bridge.fixture_replay_completed().is_some() && bridge.fixture_old_replays() == 1
            })
        {
            if self
                .platform
                .bridges
                .get(&id.0)
                .is_none_or(|bridge| bridge.fixture_replay_completed() != Some(true))
            {
                return Err(failure(
                    "actual old-message/outbound WK replay evaluation failed",
                ));
            }
            if self
                .platform
                .bridges
                .get(&id.0)
                .is_none_or(|bridge| bridge.fixture_finished_calls() != self.progress.base_calls)
            {
                return Err(failure("old message entered a successor original Function"));
            }
            self.delivery.id = 1;
            self.delivery.consumed = false;
            self.delivery.quiesced = false;
            self.start_event(
                id,
                self.delivery.id,
                "fixture-old",
                vec![6, 1, 0, 5, 3, 0, 111, 108, 100],
            )?;
            self.progress.stage = 5;
        }
        Ok(())
    }
    fn complete_replay_events(&mut self, id: WebviewId) -> Result<(), WvError> {
        if self.progress.stage == 5 && self.delivery.consumed && self.delivery.quiesced {
            if self.delivery.disposition.as_deref() != Some("NoListener") {
                return Err(failure("old message installed a successor listener"));
            }
            release_event(
                &self.platform.bridges,
                &self.platform.views,
                id,
                self.navigation
                    .lease
                    .as_ref()
                    .ok_or_else(|| failure("lease absent"))?,
                self.delivery.id,
            )?;
            self.delivery.id = 2;
            self.delivery.consumed = false;
            self.delivery.quiesced = false;
            self.start_event(
                id,
                self.delivery.id,
                "fixture-current",
                vec![6, 1, 0, 5, 5, 0, 102, 114, 101, 115, 104],
            )?;
            self.progress.stage = 6;
        } else if self.progress.stage == 6 && self.delivery.consumed && self.delivery.quiesced {
            if self.delivery.disposition.as_deref() != Some("Consumed") {
                return Err(failure(
                    "fresh current original listener did not consume control event",
                ));
            }
            release_event(
                &self.platform.bridges,
                &self.platform.views,
                id,
                self.navigation
                    .lease
                    .as_ref()
                    .ok_or_else(|| failure("lease absent"))?,
                self.delivery.id,
            )?;
            self.progress.stage = 7;
        }
        Ok(())
    }
    fn observe_replay_page(&mut self, id: WebviewId) -> Result<(), WvError> {
        if self.progress.stage == 7 && !self.page.inflight && self.page.value.is_none() {
            self.page.inflight = true;
            let wake = self.proxy.clone();
            let document = self
                .navigation
                .lease
                .clone()
                .ok_or_else(|| failure("lease absent"))?;
            self.platform.views.get(&id.0).ok_or_else(||failure("view absent"))?.webview.evaluate_script_with_callback("JSON.stringify({old:Number(document.documentElement.getAttribute('data-old-effects')||0),fresh:Number(document.documentElement.getAttribute('data-fresh-effects')||0),last:document.documentElement.getAttribute('data-last-callback-value'),dom:JSON.stringify(globalThis.__preloadFixtureLog||[])})",move|raw|{let _=wake.send_event(WkUserEvent::PreloadFixtureSnapshot{view:id,document:document.clone(),raw});}).map_err(|error|failure(error.to_string()))?;
        }
        if self.progress.stage == 7
            && let Some(page) = self.page.value.take()
        {
            if page["old"].as_u64() != Some(0)
                || page["fresh"].as_u64() != Some(1)
                || page["last"].as_str() != Some("fresh")
            {
                return Err(failure("real successor page callback census failed"));
            }
            let counts = self
                .platform
                .bridges
                .get(&id.0)
                .ok_or_else(|| failure("bridge absent"))?
                .fixture_counts();
            self.ending.report = Some(
                json!({"same_actual_url":true,"native_commits":self.navigation.commits,"stale_original_close_refused":self.progress.close.stale_refusal_observed,"cancelled_current_close_no_effect":self.progress.close.cancelled_refusal_observed,"original_host_getter_dialog_qualified":false,"actual_role_retirement_qualified":false,"old_callback_effects":0,"fresh_callback_effects":1,"old_native_ingress_and_outbound_delivery_refused":true,"actual_old_native_callback_observed":1,"actual_replay_wk_completed":true,"delivery_target":"native-main-frame-explicit-app-world","actual_wk_completions":counts.0,"native_consumed":counts.1,"pending_evaluations":counts.2,"page":page}),
            );
            self.ending.terminal = true;
        }
        Ok(())
    }
    fn refuse_stale_close(
        &mut self,
        native: &NativeBuild<'_>,
        id: WebviewId,
        successor: &MacNativeDocumentLease,
    ) -> Result<(), WvError> {
        let command = self
            .progress
            .held_original_close
            .take()
            .ok_or_else(|| failure("held immutable A close command absent"))?;
        let admission = self
            .progress
            .stale_admission
            .as_ref()
            .ok_or_else(|| failure("captured A admission absent"))?;
        self.platform.command(native, &self.events, command)?;
        if admission.decision() != OriginalDocumentDestroyDecision::Refused {
            return Err(failure(
                "stale A conditional native teardown was admitted after B commit",
            ));
        }
        match self.incoming.try_recv() {
            Ok(AppWindowEvent::OriginalDocumentDestroyDecision {
                view,
                admission: observed,
            }) if view == id && Arc::ptr_eq(&observed, admission) => {}
            _ => {
                return Err(failure(
                    "actual stale-refusal receipt did not preserve its captured admission",
                ));
            }
        }
        let observed = observe_conditional_successor(
            &self.platform.views,
            &self.platform.bridges,
            id,
            successor,
            self.source,
            &self.budget,
        );
        if self.retirement.released
            || observed.as_ref() != Some(&self.progress.expected_live_successor)
        {
            return Err(failure(format!(
                "stale A refusal changed actual B facts: {observed:?}"
            )));
        }
        self.progress.close.stale_refusal_observed = true;
        Ok(())
    }
    fn cancel_current_close(
        &mut self,
        native: &NativeBuild<'_>,
        id: WebviewId,
        successor: &MacNativeDocumentLease,
    ) -> Result<(), WvError> {
        // Exercise Pending cancellation against CURRENT B, so
        // stale-A rejection cannot accidentally prove cancellation.
        let cancelled = Arc::new(OriginalDocumentDestroyAdmission::new());
        if !cancelled.cancel_before_admission() {
            return Err(failure("actual Pending cancellation did not win"));
        }
        self.platform.command(
            native,
            &self.events,
            AppWindowCommand::DestroyOriginalDocument {
                view: id,
                expected: successor.clone(),
                admission: Arc::clone(&cancelled),
            },
        )?;
        match self.incoming.try_recv() {
            Ok(AppWindowEvent::OriginalDocumentDestroyDecision {
                view,
                admission: observed,
            }) if view == id
                && Arc::ptr_eq(&observed, &cancelled)
                && observed.decision() == OriginalDocumentDestroyDecision::Cancelled => {}
            _ => {
                return Err(failure(
                    "native current-B cancellation receipt absent or retargeted",
                ));
            }
        }
        let observed = observe_conditional_successor(
            &self.platform.views,
            &self.platform.bridges,
            id,
            successor,
            self.source,
            &self.budget,
        );
        if self.retirement.released
            || observed.as_ref() != Some(&self.progress.expected_live_successor)
        {
            return Err(failure(format!(
                "cancelled current close changed actual B facts: {observed:?}"
            )));
        }
        self.progress.close.cancelled_refusal_observed = true;
        Ok(())
    }
    fn advance_held(&mut self, native: &NativeBuild<'_>, id: WebviewId) -> Result<(), WvError> {
        let ready = self
            .platform
            .bridges
            .get(&id.0)
            .is_some_and(super::macos_bridge::MacRendererBridge::fixture_ready);
        if self.progress.stage == 0 && self.navigation.finished && ready {
            self.progress.base_calls = self
                .platform
                .bridges
                .get(&id.0)
                .ok_or_else(|| failure("bridge absent"))?
                .fixture_finished_calls();
            self.platform
                .view(id)?
                .webview
                .evaluate_script(&register_script("fixture-current"))
                .map_err(|error| failure(error.to_string()))?;
            self.progress.stage = 1;
        } else if self.progress.stage == 1
            && ready
            && self.platform.bridges.get(&id.0).is_some_and(|bridge| {
                bridge.fixture_finished_calls() == self.progress.base_calls + 1
            })
        {
            let bridge = self
                .platform
                .bridges
                .get(&id.0)
                .ok_or_else(|| failure("bridge absent"))?;
            bridge.fixture_hold_event_completion();
            self.delivery.id = 1;
            self.start_event(
                id,
                self.delivery.id,
                "fixture-current",
                vec![6, 1, 0, 5, 5, 0, 102, 114, 101, 115, 104],
            )?;
            self.progress.stage = 2;
        } else if self.progress.stage == 2
            && self.delivery.consumed
            && self
                .platform
                .bridges
                .get(&id.0)
                .is_some_and(super::macos_bridge::MacRendererBridge::fixture_completion_held)
        {
            let native_drained = if self.case == "preload-held-conditional-destroy" {
                self.admit_current_conditional(native, id)?
            } else if self.case == "preload-held-quit" {
                // Exercise the same terminal revocation helper as the real
                // application loop while an actual WK evaluation receipt is
                // held. It must leave the bridge in the owner map until the
                // genuine WebKit completion is observed and release() drains.
                super::destroy_renderer_bridges(&mut self.platform.bridges);
                if !self.platform.bridges.contains_key(&id.0) {
                    return Err(failure(
                        "terminal revocation dropped actual WK evaluation custody before release",
                    ));
                }
                managed_windows::release_all(
                    &mut self.platform.views,
                    &mut self.platform.bridges,
                    &self.events,
                )?
            } else {
                managed_windows::release(
                    &mut self.platform.views,
                    &mut self.platform.bridges,
                    &self.events,
                    id,
                )?
            };
            if native_drained
                || !self.platform.views.contains_key(&id.0)
                || self.retirement.released
                || self.budget.retained_bytes() != Some(1657)
                || self.delivery.quiesced
            {
                return Err(failure(
                    "Destroy/Quit released real view/image before native completion custody",
                ));
            }
            self.retirement.held_release_observed = true;
            self.platform
                .bridges
                .get(&id.0)
                .ok_or_else(|| failure("custody lost bridge"))?
                .fixture_release_actual_completion()?;
            self.progress.stage = 3;
        } else if self.progress.stage == 3
            && self.delivery.quiesced
            && managed_windows::release_all(
                &mut self.platform.views,
                &mut self.platform.bridges,
                &self.events,
            )?
        {
            self.observe_held_release(id)?;
        }
        Ok(())
    }
    fn observe_held_release(&mut self, id: WebviewId) -> Result<(), WvError> {
        if self.case == "preload-held-conditional-destroy" {
            match self.incoming.try_recv() {
                Ok(AppWindowEvent::WindowReleased { view }) if view == id => {
                    self.progress.close.conditional_release_observed = true;
                }
                _ => {
                    return Err(failure(
                        "conditional physical drain omitted its actual WindowReleased fact",
                    ));
                }
            }
            if self
                .progress
                .conditional_admission
                .as_ref()
                .is_none_or(|admission| {
                    admission.decision() != OriginalDocumentDestroyDecision::Accepted
                })
            {
                return Err(failure(
                    "conditional admission changed before physical release",
                ));
            }
        }
        self.retirement.released = true;
        self.ending.report = Some(
            json!({"real_wk_receipt_held":true,"early_release_refused":self.retirement.held_release_observed,"image_bytes_after_actual_release":self.budget.retained_bytes(),"physical_view_released":true,"conditional_window_released_observed":self.progress.close.conditional_release_observed,"original_host_getter_dialog_qualified":false,"actual_role_retirement_qualified":false}),
        );
        self.ending.terminal = true;
        Ok(())
    }
    fn admit_current_conditional(
        &mut self,
        native: &NativeBuild<'_>,
        id: WebviewId,
    ) -> Result<bool, WvError> {
        let expected = self
            .navigation
            .lease
            .as_ref()
            .ok_or_else(|| failure("actual current lease absent"))?
            .clone();
        let observed = observe_conditional_successor(
            &self.platform.views,
            &self.platform.bridges,
            id,
            &expected,
            self.source,
            &self.budget,
        );
        if self.retirement.released
            || observed.as_ref() != Some(&self.progress.expected_live_successor)
        {
            return Err(failure(format!(
                "current conditional close lacked its actual live facts: {observed:?}"
            )));
        }
        let admission = Arc::new(OriginalDocumentDestroyAdmission::new());
        self.platform.command(
            native,
            &self.events,
            AppWindowCommand::DestroyOriginalDocument {
                view: id,
                expected: expected.clone(),
                admission: Arc::clone(&admission),
            },
        )?;
        if admission.decision() != OriginalDocumentDestroyDecision::Accepted
            || expected.is_current()
        {
            return Err(failure(
                "current conditional acceptance did not irreversibly retire its native document",
            ));
        }
        if admission.cancel_before_admission()
            || admission.decision() != OriginalDocumentDestroyDecision::Accepted
        {
            return Err(failure(
                "late cancellation changed actual native Accepted winner",
            ));
        }
        let mut accepted = false;
        let mut revoked = false;
        while let Ok(observation) = self.incoming.try_recv() {
            match observation {
                AppWindowEvent::MacOriginalNavigation {
                    context,
                    fact:
                        crate::engine::MacOriginalNavigationFact::Revoked {
                            previous: Some(previous),
                        },
                } if context.view() == id && previous == expected.stamp() => {
                    revoked = true;
                }
                AppWindowEvent::OriginalDocumentDestroyDecision {
                    view,
                    admission: observed,
                } if view == id
                    && Arc::ptr_eq(&observed, &admission)
                    && observed.decision() == OriginalDocumentDestroyDecision::Accepted =>
                {
                    accepted = true;
                }
                AppWindowEvent::WindowReleased { .. } => {
                    return Err(failure(
                        "Accepted emitted physical release before held actual WK completion",
                    ));
                }
                _ => {
                    return Err(failure(
                        "unexpected actual native observation during conditional acceptance",
                    ));
                }
            }
        }
        if !accepted || !revoked {
            return Err(failure(
                "actual native acceptance/revocation observations absent",
            ));
        }
        self.progress.conditional_admission = Some(admission);
        Ok(!self.platform.views.contains_key(&id.0))
    }
    fn finish_turn(&mut self, turn: Result<(), WvError>, control: &mut ControlFlow) {
        let stage = self.progress.stage;
        let expired = Instant::now() >= self.deadline;
        if self.progress.last_diagnostic_stage != Some(self.progress.stage)
            || turn.is_err()
            || expired
        {
            let native = self
                .selected
                .and_then(|id| self.platform.bridges.get(&id.0))
                .map(super::macos_bridge::MacRendererBridge::fixture_diagnostics);
            let resource=self.selected.and_then(|id|self.platform.views.get(&id.0)).and_then(|view|view.resource.as_ref()).map(|resource|{
                let (shutdown,cancelled,bytes,tasks)=resource.handler.lifecycle_status();
                let resource_live=resource.handler.current_document().borrow().active_resource_lease(resource.handler.selected_source().source_id()).is_some();
                json!({"shutdown_requested":shutdown,"reader_cancelled":cancelled,"source_retained_bytes":bytes,"native_tasks":tasks,"source_resource_lease_live":resource_live,"native_committed":resource.navigation.current_committed().is_some(),"native_finished_current":resource.navigation.finished_current_attempt().is_some(),"native_load_request_calls":resource.navigation.fixture_native_load_calls(),"actual_native_failure_callbacks":resource.navigation.fixture_native_failures()})
            });

            let diagnostic = json!({"case":self.case,"stage":self.progress.stage,"waiting_for":waiting_for(self.case,self.progress.stage),"native_started":self.navigation.started,"native_commits":self.navigation.commits,"preflight_facts_seen":self.delivery.preflight_facts_seen,"last_navigation_fact":self.navigation.last_fact,"finished":self.navigation.finished,"consumed":self.delivery.consumed,"quiesced":self.delivery.quiesced,"delivery":self.delivery.id,"base_function_calls":self.progress.base_calls,"snapshot_inflight":self.page.inflight,"snapshot_received":self.page.value.is_some(),"view_count":self.platform.views.len(),"image_retained_bytes":self.budget.retained_bytes(),"old_lease_current":self.navigation.old_lease.as_ref().map(MacNativeDocumentLease::is_current),"current_lease_current":self.navigation.lease.as_ref().map(MacNativeDocumentLease::is_current),"native":native,"resource":resource});
            eprintln!("KELD preload fixture observation: {diagnostic}");
            self.progress.last_diagnostic_stage = Some(self.progress.stage);
        }
        if let Err(error) = turn {
            self.ending.problem.get_or_insert(error);
            self.ending.terminal = true;
        }
        if expired {
            self.ending.problem.get_or_insert_with(|| {
                failure(format!(
                    "real native probe or completion deadline expired at stage{stage}: {}",
                    waiting_for(self.case, self.progress.stage)
                ))
            });
            self.ending.terminal = true;
        }
        if self.ending.terminal {
            match managed_windows::release_all(
                &mut self.platform.views,
                &mut self.platform.bridges,
                &self.events,
            ) {
                Ok(true) => {
                    self.retirement.released = true;
                    *control = ControlFlow::Exit;
                }
                Ok(false) => {
                    *control = ControlFlow::WaitUntil(self.deadline);
                }
                Err(error) => {
                    self.ending.problem.get_or_insert(error);
                    *control = ControlFlow::Exit;
                }
            }
        }
    }
    fn finish(
        mut self,
        mut engine: WkWebViewEngine,
        event_loop: tao::event_loop::EventLoop<WkUserEvent>,
        code: i32,
    ) -> Result<Value, WvError> {
        if !self.platform.views.is_empty() {
            engine.views = std::mem::take(&mut self.platform.views);
            engine.renderer_bridges = std::mem::take(&mut self.platform.bridges);
            std::mem::forget(engine);
            std::mem::forget(event_loop);
            return Err(self.ending.problem.unwrap_or_else(|| {
                failure("native custody remains unproved; retained to child exit")
            }));
        }
        if let Some(error) = self.ending.problem {
            return Err(error);
        }
        if !self.retirement.released || self.budget.retained_bytes() != Some(0) {
            return Err(failure(
                "native loop exited without proved view/image release",
            ));
        }
        let mut report = self
            .ending
            .report
            .ok_or_else(|| failure("no real native probe result"))?;
        report["scope"] = json!("real-wk-local-original-runtime-only");
        report["case"] = json!(self.case);
        report["canonical_bun_release_qualified"] = json!(false);
        report["route_availability_source"] =
            json!("actual-native-registration-and-local-rust-test-receiver");
        report["product_acceptance"] = json!(false);
        report["run_return_code"] = json!(code);
        report["released"] = json!(self.retirement.released);
        Ok(report)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct NativeDocumentFacts {
    lease_current: bool,
    view_matches: bool,
    commit_matches: bool,
}
#[derive(Debug, PartialEq, Eq)]
struct NativeSourceFacts {
    lease_matches: bool,
    owner_matches: bool,
}
#[derive(Debug, PartialEq, Eq)]
struct NativeRuntimeFacts {
    registered: bool,
    reader_shutdown: bool,
    reader_cancelled: bool,
}
#[derive(Debug, PartialEq, Eq)]
struct NativeSuccessorFacts {
    document: NativeDocumentFacts,
    source: NativeSourceFacts,
    runtime: NativeRuntimeFacts,
    image_bytes: Option<usize>,
}
fn observe_conditional_successor(
    views: &BTreeMap<u32, super::View>,
    bridges: &BTreeMap<u32, super::MacRendererBridge>,
    id: WebviewId,
    expected: &MacNativeDocumentLease,
    source: &Arc<AdmittedWebAppSource>,
    budget: &AppPreloadBudget,
) -> Option<NativeSuccessorFacts> {
    let view = views.get(&id.0)?;
    let resource = view.resource.as_ref()?;
    let (shutdown, cancelled, _, _) = resource.handler.lifecycle_status();
    Some(NativeSuccessorFacts {
        document: NativeDocumentFacts {
            lease_current: expected.is_current(),
            view_matches: expected.stamp().view() == id,
            commit_matches: resource.navigation.current_committed() == Some(expected.stamp().0),
        },
        source: NativeSourceFacts {
            lease_matches: expected.belongs_to_source(source),
            owner_matches: Arc::ptr_eq(resource.handler.selected_source(), source),
        },
        runtime: NativeRuntimeFacts {
            registered: bridges
                .get(&id.0)
                .is_some_and(super::macos_bridge::MacRendererBridge::fixture_registered),
            reader_shutdown: shutdown,
            reader_cancelled: cancelled,
        },
        image_bytes: budget.retained_bytes(),
    })
}

fn release_event(
    bridges: &BTreeMap<u32, super::MacRendererBridge>,
    views: &BTreeMap<u32, super::View>,
    id: WebviewId,
    lease: &MacNativeDocumentLease,
    delivery: u32,
) -> Result<(), WvError> {
    bridges
        .get(&id.0)
        .ok_or_else(|| failure("bridge absent"))?
        .deliver(
            &views
                .get(&id.0)
                .ok_or_else(|| failure("view absent"))?
                .webview
                .webview(),
            RendererBridgeOutcome::EventReleased {
                webview: id,
                navigation: u64::from(lease.stamp().observed_generation()),
                request: delivery,
            },
        )
}
