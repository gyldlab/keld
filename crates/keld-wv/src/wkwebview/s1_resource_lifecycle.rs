//! Debug-only actual WK task/source/Destroy/Quit lifecycle regression.
//! A held-read observation is never a thread-completion or release receipt.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{BufRead as _, Write as _};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tao::event::Event;

use super::macos_resource_source::AdmittedWebAppSource;
use super::{
    AppReadyTrigger, AppWindowCommand, AppWindowEvent, View, WkUserEvent, WkWebViewEngine,
};
use crate::engine::{MacOriginalNavigationFact, NativeNavigationContext, OriginalEntryQuery};
use crate::{WebviewId, WvError};

fn failure(message: impl Into<String>) -> WvError {
    WvError::EventLoop(format!(
        "P0-1 actual resource lifecycle: {}",
        message.into()
    ))
}

fn emit(value: &Value) -> Result<(), WvError> {
    writeln!(std::io::stdout().lock(), "{value}").map_err(|error| failure(error.to_string()))
}

pub(super) use super::s1_resource_lifecycle_observer::{
    CallbackCensus, HeldRead, LifecycleEvent, ReadGate,
};

#[derive(Default)]
struct BaselineObservation {
    inflight: bool,
    ready: bool,
}
#[derive(Default)]
struct LoadObservation {
    finished: bool,
    fetch_started: bool,
    baseline: BaselineObservation,
}
#[derive(Default)]
struct GateObservation {
    released: bool,
    cancellation_reported: bool,
}
#[derive(Default)]
struct StopObservation {
    reported: bool,
    result_turn: bool,
    drained: bool,
}

pub(super) struct LifecycleProbe {
    case: String,
    commands: mpsc::Sender<AppWindowCommand>,
    incoming: mpsc::Receiver<AppWindowEvent>,
    selected: Arc<AtomicU32>,
    source: Arc<AdmittedWebAppSource>,
    census: Rc<CallbackCensus>,
    release: Option<mpsc::SyncSender<()>>,
    view: Option<WebviewId>,
    load: LoadObservation,
    gate: GateObservation,
    stop: StopObservation,
    held: Option<HeldRead>,
    held_task: Option<usize>,
    releases: usize,
    join_wakes: usize,
    proxy: tao::event_loop::EventLoopProxy<WkUserEvent>,
    fanout_inflight: bool,
    fanout: Option<Value>,
}

impl LifecycleProbe {
    fn view<'a>(&self, views: &'a BTreeMap<u32, View>) -> Result<&'a View, WvError> {
        self.view
            .and_then(|id| views.get(&id.0))
            .ok_or_else(|| failure("actual selected View missing"))
    }

    pub(super) fn before_event(
        &mut self,
        event: &Event<'_, WkUserEvent>,
        views: &BTreeMap<u32, View>,
    ) -> Result<(), WvError> {
        match event {
            Event::UserEvent(WkUserEvent::ResourceLifecycle(LifecycleEvent::Held(held))) => {
                let view = self.view(views)?;
                let handler = &view
                    .resource
                    .as_ref()
                    .ok_or_else(|| failure("actual handler absent"))?
                    .handler;
                self.held_task = handler.lifecycle_held_task();
                if self.held_task.is_none() {
                    return Err(failure("held read has no actual admitted WK task"));
                }
                let (_, _, credit, tasks) = handler.lifecycle_status();
                if credit == 0 || tasks == 0 || self.census.total() == 0 {
                    return Err(failure(
                        "held custody or live callback positive control absent",
                    ));
                }
                self.held = Some(HeldRead {
                    read_fd: held.read_fd,
                    root_fd: held.root_fd,
                    inode: held.inode,
                });
                emit(
                    &json!({"phase":"held", "case":self.case,"pid":std::process::id(),
                    "read_fd":held.read_fd,"root_fd":held.root_fd,"inode":held.inode,
                    "view_present":true,"retained_bytes":credit,"native_tasks":tasks,
                    "live_callback_positive":self.census.total(),"window_released":self.releases}),
                )?;
            }
            Event::UserEvent(WkUserEvent::S1FixtureSnapshot { view, stamp, raw })
                if self.case == "resource-fanout" && self.load.baseline.inflight =>
            {
                self.accept_baseline(*view, *stamp, raw, views)?;
            }
            Event::UserEvent(WkUserEvent::ResourceLifecycle(LifecycleEvent::ObserveFanout)) => {
                self.observe_fanout(views)?;
            }
            Event::UserEvent(WkUserEvent::ResourceLifecycle(LifecycleEvent::FanoutSnapshot(
                raw,
            ))) => {
                self.fanout_inflight = false;
                let value = super::s1_resource_fixture::snapshot(raw)?;
                let (_, _, credit, tasks) = self
                    .view(views)?
                    .resource
                    .as_ref()
                    .ok_or_else(|| failure("fanout handler absent"))?
                    .handler
                    .lifecycle_status();
                emit(
                    &json!({"phase":"fanout-observation","case":self.case,"result":value,
                    "native_tasks":tasks,"retained_bytes":credit,"gate_released":self.gate.released}),
                )?;
                self.fanout = Some(value);
            }
            Event::UserEvent(WkUserEvent::ResourceLifecycle(LifecycleEvent::Abort)) => {
                self.view(views)?
                    .webview
                    .evaluate_script("globalThis.__keldLifecycleAbort.abort();")
                    .map_err(|error| failure(error.to_string()))?;
            }
            Event::UserEvent(WkUserEvent::ResourceLifecycle(LifecycleEvent::Release)) => {
                self.release
                    .take()
                    .ok_or_else(|| failure("gate released twice"))?
                    .try_send(())
                    .map_err(|error| failure(error.to_string()))?;
                self.gate.released = true;
            }
            Event::UserEvent(WkUserEvent::ResourceLifecycle(LifecycleEvent::TaskStopped(key))) => {
                if self.case == "native-task-stop"
                    && self.held_task == Some(*key)
                    && !self.stop.reported
                {
                    self.stop.reported = true;
                    emit(
                        &json!({"phase":"task-stopped","case":self.case,"native_task":key,
                        "callbacks_after_stop":self.census.after_stop(),"gate_still_held":!self.gate.released}),
                    )?;
                }
            }
            Event::UserEvent(WkUserEvent::Resource(completion)) => {
                if self.held_task == Some(completion.ticket.native_identity()) {
                    self.stop.result_turn = true;
                }
            }
            Event::UserEvent(WkUserEvent::ReaderTerminated(completion))
                if self.view == Some(completion.view) =>
            {
                self.join_wakes += 1;
            }
            _ => {}
        }
        Ok(())
    }

    fn observe_baseline(&mut self, views: &BTreeMap<u32, View>) -> Result<(), WvError> {
        if self.load.baseline.inflight || self.load.baseline.ready {
            return Ok(());
        }
        let view = self.view(views)?;
        let resource = view
            .resource
            .as_ref()
            .ok_or_else(|| failure("baseline resource owner absent"))?;
        let stamp = resource
            .navigation
            .current_committed()
            .ok_or_else(|| failure("baseline native commit absent"))?;
        let id = self.view.ok_or_else(|| failure("baseline view absent"))?;
        let proxy = self.proxy.clone();
        view.webview.evaluate_script_with_callback(
            "JSON.stringify({complete:document.documentElement.getAttribute('data-probe-complete')==='yes',result:(document.getElementById('result')?.textContent||'').slice(0,8192)})",
            move |raw| {
                if let Err(error) = proxy.send_event(WkUserEvent::S1FixtureSnapshot { view: id, stamp, raw }) {
                    eprintln!("baseline observation queue closed: {error}");
                }
            },
        ).map_err(|error| failure(error.to_string()))?;
        self.load.baseline.inflight = true;
        Ok(())
    }

    fn accept_baseline(
        &mut self,
        view: WebviewId,
        stamp: super::macos_document::NativeCommittedStamp,
        raw: &str,
        views: &BTreeMap<u32, View>,
    ) -> Result<(), WvError> {
        let resource = self
            .view(views)?
            .resource
            .as_ref()
            .ok_or_else(|| failure("baseline resource owner absent"))?;
        if self.view != Some(view) || resource.navigation.current_committed() != Some(stamp) {
            return Err(failure("baseline snapshot lost its native document"));
        }
        self.load.baseline.inflight = false;
        let snapshot = super::s1_resource_fixture::snapshot(raw)?;
        if snapshot["complete"] != true {
            return Ok(());
        }
        let result: Value = serde_json::from_str(
            snapshot["result"]
                .as_str()
                .ok_or_else(|| failure("completed baseline has no result"))?,
        )
        .map_err(|error| failure(error.to_string()))?;
        if result.get("failure").is_some()
            || result["case"] != "baseline"
            || result["observerAlive"] != "yes"
            || result["wasm"]["compiled"] != true
            || result["text"]["status"] != 200
            || result["wasmResource"]["status"] != 200
        {
            return Err(failure(
                "baseline probe did not complete successfully before fanout",
            ));
        }
        let (_, _, credit, tasks) = self
            .view(views)?
            .resource
            .as_ref()
            .ok_or_else(|| failure("baseline resource owner absent"))?
            .handler
            .lifecycle_status();
        if tasks != 0 {
            return Ok(());
        }
        self.load.baseline.ready = true;
        emit(&json!({"phase":"fanout-baseline-complete","case":self.case,
            "native_tasks":tasks,"retained_bytes":credit,"result":result}))
    }

    fn observe_fanout(&mut self, views: &BTreeMap<u32, View>) -> Result<(), WvError> {
        if self.case != "resource-fanout" || self.fanout_inflight {
            return Err(failure("fanout observation absent or already in flight"));
        }
        let proxy = self.proxy.clone();
        self.view(views)?
            .webview
            .evaluate_script_with_callback(
                "JSON.stringify(globalThis.__keldResourceFanout||null)",
                move |raw| {
                    if let Err(error) = proxy.send_event(WkUserEvent::ResourceLifecycle(
                        LifecycleEvent::FanoutSnapshot(raw),
                    )) {
                        eprintln!("fanout observation queue closed: {error}");
                    }
                },
            )
            .map_err(|error| failure(error.to_string()))?;
        self.fanout_inflight = true;
        Ok(())
    }

    fn observe_application_event(
        &mut self,
        event: &AppWindowEvent,
        views: &BTreeMap<u32, View>,
    ) -> Result<(), WvError> {
        match event {
            AppWindowEvent::WindowBuilt { view, .. } => {
                self.view = Some(*view);
                self.selected.store(view.0, Ordering::Release);
                self.view(views)?
                    .resource
                    .as_ref()
                    .ok_or_else(|| failure("actual handler missing"))?
                    .handler
                    .attach_lifecycle_census(Rc::clone(&self.census));
                self.commands
                    .send(AppWindowCommand::LoadAdmittedOriginal {
                        context: NativeNavigationContext::new(
                            std::num::NonZeroU64::new(1)
                                .ok_or_else(|| failure("fixture operation absent"))?,
                            *view,
                        ),
                        query: OriginalEntryQuery::new("case=baseline").map_err(failure)?,
                    })
                    .map_err(|error| failure(error.to_string()))?;
            }
            AppWindowEvent::MacOriginalNavigation {
                fact: MacOriginalNavigationFact::Finished { .. },
                ..
            } => self.load.finished = true,
            AppWindowEvent::MacOriginalNavigation {
                fact: MacOriginalNavigationFact::Failed { .. },
                ..
            }
            | AppWindowEvent::WindowBuildFailed { .. } => {
                return Err(failure("actual native construction/navigation failed"));
            }
            AppWindowEvent::WindowReleased { view } if self.view == Some(*view) => {
                self.releases += 1;
                if !self.gate.released || views.contains_key(&view.0) || self.join_wakes != 1 {
                    return Err(failure(
                        "early native View/WindowReleased before actual joined reader",
                    ));
                }
                emit(
                    &json!({"phase":"released","case":self.case,"window_released":self.releases,
                        "view_present":false,"joined_wakes":self.join_wakes,
                        "retained_bytes":self.source.retained_credit_account().load(Ordering::Acquire),
                        "callbacks_after_stop":self.census.after_stop()}),
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn final_report(&self) -> Result<Value, WvError> {
        if self.releases != 1
            || self.join_wakes != 1
            || self.census.after_stop() != 0
            || (self.case == "native-task-stop" && !self.stop.drained)
            || (!matches!(self.case.as_str(), "native-task-stop" | "resource-fanout")
                && !self.gate.cancellation_reported)
        {
            return Err(failure("actual lifecycle observables incomplete"));
        }
        let held = self
            .held
            .as_ref()
            .ok_or_else(|| failure("no actual opened read FD observed"))?;
        let report = json!({"schema":"keld.s1.resource-lifecycle.v1","case":self.case,
        "pid":std::process::id(),"read_fd":held.read_fd,"root_fd":held.root_fd,"inode":held.inode,
        "window_released":self.releases,"joined_wakes":self.join_wakes,
        "callbacks_after_stop":self.census.after_stop(),"live_callback_positive":self.census.total(),
        "retained_bytes":self.source.retained_credit_account().load(Ordering::Acquire),
        "original_app_executed":false,"shipping_policy":"NOT PROVED",
        "fanout":self.fanout});
        Ok(report)
    }

    fn start_resource_workload(&mut self, views: &BTreeMap<u32, View>) -> Result<(), WvError> {
        self.load.fetch_started = true;
        let script = if self.case == "resource-fanout" {
            r"globalThis.__keldResourceFanout={requested:5,settled:0,fulfilled:0,rejected:0,allSame:false};
                const state=globalThis.__keldResourceFanout;
                Promise.allSettled(Array.from({length:5},(_,i)=>fetch('leak.svg?'+(i===0?'lifecycle=held':'fanout='+i),{cache:'no-store'}).then(r=>{if(!r.ok)throw Error('status');return r.text();}).then(body=>{state.fulfilled++;state.settled++;return body;},error=>{state.rejected++;state.settled++;throw error;}))).then(results=>{
                  const bodies=results.filter(r=>r.status==='fulfilled').map(r=>r.value);
                  state.allSame=bodies.length===5&&bodies[0].length>0&&bodies.every(body=>body===bodies[0]);
                });"
        } else {
            "globalThis.__keldLifecycleAbort=new AbortController();fetch('leak.svg?lifecycle=held',{signal:globalThis.__keldLifecycleAbort.signal}).then(r=>r.text()).catch(()=>{});"
        };
        if self.case == "resource-fanout" {
            emit(&json!({"phase":"fanout-started","requested":5}))?;
        }
        self.view(views)?
            .webview
            .evaluate_script(script)
            .map_err(|error| failure(error.to_string()))?;
        Ok(())
    }

    pub(super) fn after_turn(&mut self, views: &BTreeMap<u32, View>) -> Result<(), WvError> {
        while let Ok(event) = self.incoming.try_recv() {
            let observed = self.observe_application_event(&event, views);
            drop(event); // Preserve the former by-value helper's receipt drop cut.
            observed?;
        }
        if self.load.finished && !self.load.fetch_started {
            if self.case == "resource-fanout" && !self.load.baseline.ready {
                self.observe_baseline(views)?;
            } else {
                self.start_resource_workload(views)?;
            }
        }
        if self.held.is_some() && !self.gate.released && !self.gate.cancellation_reported {
            let view = self.view(views)?;
            let (requested, cancelled, credit, tasks) = view
                .resource
                .as_ref()
                .ok_or_else(|| failure("handler lost while held"))?
                .handler
                .lifecycle_status();
            if requested && !cancelled {
                return Err(failure(
                    "actual Destroy/Quit did not set source cancellation",
                ));
            }
            if cancelled {
                if credit == 0 || tasks == 0 || self.releases != 0 {
                    return Err(failure(
                        "early native/source custody release during blocked Destroy/Quit",
                    ));
                }
                self.gate.cancellation_reported = true;
                emit(
                    &json!({"phase":"cancel-pending","case":self.case,"view_present":true,
                    "cancelled":true,"retained_bytes":credit,"native_tasks":tasks,"window_released":self.releases}),
                )?;
            }
        }
        if self.case == "native-task-stop" && self.stop.result_turn && !self.stop.drained {
            if !self.stop.reported || self.census.after_stop() != 0 {
                return Err(failure(
                    "native selector attempted after actual stopURLSchemeTask",
                ));
            }
            let credit = self
                .source
                .retained_credit_account()
                .load(Ordering::Acquire);
            let handler = &self
                .view(views)?
                .resource
                .as_ref()
                .ok_or_else(|| failure("actual handler missing"))?
                .handler;
            if self
                .held_task
                .is_some_and(|key| handler.lifecycle_task_present(key))
            {
                return Err(failure(
                    "stopped task remains retained after actual deliver returns",
                ));
            }
            self.stop.drained = true;
            emit(
                &json!({"phase":"stopped-drained","case":self.case,"callbacks_after_stop":0,
                "retained_bytes":credit,"stopped_native_task_retained":false,"live_callback_positive":self.census.total()}),
            )?;
        }
        Ok(())
    }
}

fn spawn_controller(
    case: &str,
    commands: &mpsc::Sender<AppWindowCommand>,
    selected: Arc<AtomicU32>,
    proxy: tao::event_loop::EventLoopProxy<WkUserEvent>,
) -> Result<std::thread::JoinHandle<Result<(), String>>, WvError> {
    let control_case = case.to_owned();
    let control_commands = commands.clone();
    std::thread::Builder::new()
        .name(String::from("keld-lifecycle-test-control"))
        .spawn(move || -> Result<(), String> {
            for line in std::io::stdin().lock().lines() {
                let line = line.map_err(|error| error.to_string())?;
                match line.as_str() {
                    "cancel" if control_case == "native-task-stop" => proxy
                        .send_event(WkUserEvent::ResourceLifecycle(LifecycleEvent::Abort))
                        .map_err(|error| error.to_string())?,
                    "cancel" => {
                        let command = if control_case == "blocked-quit" {
                            AppWindowCommand::Quit
                        } else {
                            AppWindowCommand::DestroyWindow {
                                view: WebviewId(selected.load(Ordering::Acquire)),
                            }
                        };
                        control_commands
                            .send(command)
                            .map_err(|error| error.to_string())?;
                    }
                    "observe" if control_case == "resource-fanout" => proxy
                        .send_event(WkUserEvent::ResourceLifecycle(
                            LifecycleEvent::ObserveFanout,
                        ))
                        .map_err(|error| error.to_string())?,
                    "release" => proxy
                        .send_event(WkUserEvent::ResourceLifecycle(LifecycleEvent::Release))
                        .map_err(|error| error.to_string())?,
                    "finish" => control_commands
                        .send(AppWindowCommand::Quit)
                        .map_err(|error| error.to_string())?,
                    "exit" => return Ok(()),
                    _ => return Err(String::from("unknown bounded lifecycle test control")),
                }
            }
            Err(String::from(
                "lifecycle controller lost parent before observation completed",
            ))
        })
        .map_err(|error| failure(error.to_string()))
}

pub(super) fn run(source: Arc<AdmittedWebAppSource>, case: &str) -> Result<Value, WvError> {
    let mut engine = WkWebViewEngine::new_dev_ephemeral()?;
    engine.admit_initial_legacy_webapp(Arc::clone(&source))?;
    let proxy = engine
        .event_loop
        .as_ref()
        .ok_or_else(|| failure("actual Tao loop absent"))?
        .create_proxy();
    let (commands, incoming_commands) = mpsc::channel();
    let (events, incoming) = mpsc::channel();
    let selected = Arc::new(AtomicU32::new(0));
    let (release, held) = mpsc::sync_channel(1);
    source
        .arm_lifecycle_gate(ReadGate::new(String::from("leak.svg"), proxy.clone(), held))
        .map_err(failure)?;
    let census = Rc::new(CallbackCensus::new(proxy.clone()));
    let probe = Rc::new(RefCell::new(LifecycleProbe {
        case: case.to_owned(),
        commands: commands.clone(),
        incoming,
        selected: Arc::clone(&selected),
        source: Arc::clone(&source),
        census: Rc::clone(&census),
        release: Some(release),
        view: None,
        load: LoadObservation::default(),
        gate: GateObservation::default(),
        stop: StopObservation::default(),
        held: None,
        held_task: None,
        releases: 0,
        join_wakes: 0,
        proxy: proxy.clone(),
        fanout_inflight: false,
        fanout: None,
    }));
    engine.resource_lifecycle_probe = Some(Rc::clone(&probe));
    commands
        .send(AppWindowCommand::CreateWindow {
            build: 1,
            width: 640,
            height: 480,
            x: None,
            y: None,
            original_source: Some(Arc::clone(&source)),
            preload_image: None,
        })
        .map_err(|error| failure(error.to_string()))?;
    let controller = spawn_controller(case, &commands, selected, proxy)?;
    let scenario_deadline = Instant::now() + Duration::from_secs(25);
    let mut retains = false;
    let outcome = engine.run_app_until_quit_with_main_owner(
        incoming_commands,
        events,
        None,
        AppReadyTrigger::HostInitialized,
        &mut |_, _, _, _, terminal| {
            if Instant::now() >= scenario_deadline {
                return Err(failure("absolute scenario failure deadline"));
            }
            Ok((true, (!terminal).then_some(scenario_deadline)))
        },
        (&mut retains, || {}),
    );
    if let Err(error) = outcome {
        emit(
            &json!({"phase":"failed","case":case,"error":error.to_string(),"retains_main_custody":retains}),
        )?;
        // Dedicated child failure only: retain unproved actual owners to exit.
        std::mem::forget(engine);
        std::mem::forget(probe);
        std::mem::forget(source);
        let _ = controller.join();
        return Err(error);
    }
    let report = probe.borrow().final_report()?;
    engine.resource_lifecycle_probe.take();
    drop(engine);
    drop(probe);
    drop(source);
    emit(
        &json!({"phase":"owners-dropped","case":case,"pid":std::process::id(),
        "read_fd":report["read_fd"],"root_fd":report["root_fd"],"inode":report["inode"]}),
    )?;
    // UI loop and resource owner are already proved retired. Parent observes
    // descriptors before it permits this dedicated child/controller to exit.
    controller
        .join()
        .map_err(|_| failure("test controller panicked"))?
        .map_err(failure)?;
    if case == "resource-fanout" {
        validate_fanout_report(&report)?;
    }
    Ok(report)
}

fn validate_fanout_report(report: &Value) -> Result<(), WvError> {
    let result = &report["fanout"];
    let passed = result["requested"] == 5
        && result["settled"] == 5
        && result["fulfilled"] == 5
        && result["rejected"] == 0
        && result["allSame"] == true;
    emit(&json!({"phase":"fanout-result","passed":passed,"result":result}))?;
    if !passed {
        return Err(failure(
            "five parallel verified resources did not all complete; see fanout-result",
        ));
    }
    Ok(())
}
