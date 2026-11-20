//! Below-gate commands use the real Windows Runtime and immutable provider tuple.
use super::{SelectedLaunch, native, prepare_selected_inner};
use crate::app_session::graph_contract::tests::{
    prepared_pinned_launch_source, prepared_witness_launch_source,
};
use crate::app_session::original_webapp::PreparedOriginalSource;
use crate::app_session::windows_primary_config;
use keld_ipc::bootstrap::BootstrapStream;
use keld_ipc::channel_table::{COMPAT_CONTROL, WINDOW};
use keld_ipc::lifecycle::{LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleRequest, LifecycleResponse};
use keld_ipc::link::{read_frame, write_frame};
use keld_ipc::window::{DesktopFact, DesktopFacts, Rect, WindowEvent};
use keld_ipc::{AppLinkDeadlines, CorrelationId, FrameKind};
use keld_runtime::primary::{
    BoundPrimaryGeneration, PrimaryRecoveryGate, PrimaryRoleEvent, PrimaryRoleSupervisor, RoleOwner,
};
use keld_runtime::{RuntimeError, SupervisorOutcome};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fs, io::Write};

const BOUND: Duration = Duration::from_secs(15);
const CHANGED_PROVIDER: &str = "node_modules/electron/dist/app-session-facts.d.ts";
const FIRST_DISPLAY_ID: u32 = 41;
const SUCCESSOR_DISPLAY_ID: u32 = 73;
const OUTPUT: &str = "KELD_WINDOWS_COMMAND_PROBE ";
const ENV_SAMPLE: &str = r#"
import { writeSync } from "node:fs";
const inherited = (key) => Reflect.get(process.env, key) ?? null;
const environment = {
  devLease: inherited("KELD_DEV_LEASE"),
  startGate: inherited("KELD_WINDOWS_LAUNCH_GATE"),
  bunOptions: inherited("BUN_OPTIONS"),
  facade: inherited("KELD_NATIVE_WINDOW_FACADE"),
  unrelated: inherited("KELD_TEST_LAUNCH_COMMAND_KEEP"),
};
"#;
const ORIGINAL_PROBE: &str = r#"
import { windowsDesktopSnapshotSelected } from "../node_modules/@keld/kipc/transport.js";
import { app, nativeTheme, screen } from "electron";
const initial = {
  name: app.getName(),
  root: app.getAppPath(),
  version: app.getVersion(),
  userData: app.getPath("userData"),
  packaged: app.isPackaged,
};
await app.whenReady();
writeSync(1, "KELD_WINDOWS_COMMAND_PROBE " + JSON.stringify({
  environment, initial, locale: app.getLocale(),
  mainRoleVersion: process.versions.electron,
  desktopSelected: windowsDesktopSnapshotSelected,
  desktopPrimaryId: screen.getPrimaryDisplay().id,
  desktopDark: nativeTheme.shouldUseDarkColors,
}) + "\n");
process.exit(17);
"#;
const ORDINARY_PROBE: &str = r#"
const { windowsDesktopSnapshotSelected } = await import(pathToFileURL(provider).href);
writeSync(1, "KELD_WINDOWS_COMMAND_PROBE " + JSON.stringify({
  environment, desktopSelected: windowsDesktopSnapshotSelected,
}) + "\n");
process.exit(0);
"#;

// Observe the application result independently of the preloaded stack capture.
const MODULE_WITNESS_PROBE: &str = r#"
import { app, screen } from "electron";
await app.whenReady();
writeSync(1, "KELD_B2_WINDOW " + JSON.stringify({
  pid: process.pid, display: screen.getPrimaryDisplay().id,
}) + "\n");
"#;

// Bun's Worker-specific preload is installed only by this ignored test. The
// Error proxy records a stack from the actual Worker rejecting a deliberately
// invalid Host frame; it does not modify the package or infer a path from argv.
const WORKER_WITNESS_PRELOAD: &str = r#"
import { parentPort } from "node:worker_threads";
import { statSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
const OriginalError = globalThis.Error;
globalThis.Error = new Proxy(OriginalError, {
  construct(target, args, newTarget) {
    const error = Reflect.construct(target, args, newTarget);
    if (String(args[0]).includes("KELD-IPC-005") &&
        error.stack?.includes("validateReceivedHeader")) {
      const match = error.stack.match(/([a-z]:[\\/][^\n()]*transport\.js):\d+:\d+/i);
      if (match) {
        const path = match[1];
        const stat = statSync(path, { bigint: true });
        parentPort.postMessage({ t: "keld-b2-worker-witness", pid: process.pid, path,
          dev: String(stat.dev), ino: String(stat.ino),
          sha256: createHash("sha256").update(readFileSync(path)).digest("hex"),
          stack: error.stack });
      }
    }
    return error;
  }
});
"#;

fn assert_probe_environment() {
    for (key, value) in [
        ("KELD_DEV_LEASE", "invalid-lease-probe"),
        ("KELD_WINDOWS_LAUNCH_GATE", "invalid-gate-probe"),
        ("KELD_NATIVE_WINDOW_FACADE", "wrong-facade-probe"),
        ("BUN_OPTIONS", "--preload=missing-command-probe.mjs"),
        ("KELD_TEST_LAUNCH_COMMAND_KEEP", "unrelated-value-probe"),
    ] {
        assert_eq!(
            std::env::var(key).as_deref(),
            Ok(value),
            "inherited {key} control"
        );
    }
}

fn expected_environment(facade: &str) -> Value {
    json!({
        "devLease": null,
        "startGate": null,
        "bunOptions": null,
        "facade": facade,
        "unrelated": "unrelated-value-probe",
    })
}

fn pinned_providers() -> PathBuf {
    std::env::var_os("KELD_TEST_PINNED_WINDOWS_PROVIDERS")
        .map(PathBuf::from)
        .expect("the three qualified provider directories under node_modules")
}

fn replay_desktop_snapshot(stream: &mut impl Write, display_id: u32) -> Result<(), Box<dyn Error>> {
    let event = WindowEvent::DesktopSnapshot {
        epoch: 1,
        snapshot: Some(DesktopFacts {
            primary_id: display_id,
            displays: vec![DesktopFact {
                id: display_id,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    width: 1280,
                    height: 800,
                },
                work_area: Rect {
                    x: 0,
                    y: 0,
                    width: 1280,
                    height: 760,
                },
                scale_factor: 1.25,
            }],
            should_use_dark_colors: false,
        }),
    };
    write_frame(
        stream,
        FrameKind::Event,
        0,
        WINDOW.id(),
        CorrelationId(0),
        &keld_ipc::codec::encode(&event)?,
    )?;
    Ok(())
}

fn terminal(supervisor: &PrimaryRoleSupervisor) -> (SupervisorOutcome, Vec<PrimaryRoleEvent>) {
    let deadline = Instant::now() + BOUND;
    let mut events = Vec::new();
    loop {
        if let Some(outcome) = supervisor.try_wait_for_outcome() {
            while let Some(event) = supervisor.try_recv_event() {
                events.push(event);
            }
            return (outcome, events);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            supervisor.shutdown();
            panic!(
                "actual Runtime terminal outcome missing: {:?}",
                supervisor.output()
            );
        }
        if let Some(event) = supervisor.recv_event(remaining) {
            events.push(event);
        }
    }
}

fn read_probes(supervisor: &PrimaryRoleSupervisor) -> Result<Vec<Value>, Box<dyn Error>> {
    let output = supervisor.output();
    Ok(output
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix(OUTPUT))
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?)
}

fn start_selected_probe(
    source: &PreparedOriginalSource,
    root: &Path,
    entry: &Path,
) -> Result<(PrimaryRoleSupervisor, PrimaryRecoveryGate), Box<dyn Error>> {
    start_selected_probe_with_preload(source, root, entry, None)
}

fn start_selected_probe_with_preload(
    source: &PreparedOriginalSource,
    root: &Path,
    entry: &Path,
    preload: Option<&Path>,
) -> Result<(PrimaryRoleSupervisor, PrimaryRecoveryGate), Box<dyn Error>> {
    let prepared = prepare_selected_inner(
        SelectedLaunch {
            root,
            entry,
            name: "Command Fixture",
            version: Some("31.7.0-probe"),
            declaration: None,
            source,
        },
        || {
            Ok(native::Observation {
                app_data: Some(r"D:\Core Command Fixture\Roaming".to_owned()),
                documents: None,
                locale: Some("fr-CA".to_owned()),
            })
        },
        |_| panic!("an undeclared boot cannot elect a lease"),
        preload,
    )?;
    assert!(prepared.winner_lease.is_none());
    Ok(PrimaryRoleSupervisor::start_with_bound_generations_gated(
        windows_primary_config(root, entry, true, None, Some(prepared.config))?
            .admission_timeout(BOUND),
    )?)
}

fn receive_selected_probe(
    supervisor: &PrimaryRoleSupervisor,
    attempt: u32,
) -> Result<BoundPrimaryGeneration, Box<dyn Error>> {
    let bound = supervisor.recv_bound_generation(BOUND).ok_or_else(|| {
        format!(
            "original generation {attempt} did not authenticate: {:?}",
            supervisor.output()
        )
    })?;
    assert_eq!(bound.attempt(), attempt);
    assert_eq!(bound.principal().owner(), RoleOwner::Primary);
    assert_eq!(
        bound
            .runtime_versions()
            .and_then(|versions| versions.get("electron"))
            .map(String::as_str),
        Some("0.0.1")
    );
    Ok(bound)
}

fn accept_selected_subscriptions(
    stream: &mut BootstrapStream,
    display_id: u32,
) -> Result<(), Box<dyn Error>> {
    stream.set_app_link_read_deadline(Some(BOUND))?;
    for (channel, correlation) in [(WINDOW, 2), (COMPAT_CONTROL, 3)] {
        accept_selected_subscription(stream, channel.wire_id(), correlation, display_id)?;
    }
    Ok(())
}

fn accept_selected_subscription(
    stream: &mut BootstrapStream,
    channel: u16,
    correlation: u32,
    display_id: u32,
) -> Result<(), Box<dyn Error>> {
    let (header, message) = read_frame(stream)?;
    assert_eq!(header.kind, FrameKind::Call);
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(header.corr, CorrelationId(correlation));
    assert_eq!(
        keld_ipc::codec::decode::<LifecycleRequest>(&message)?,
        LifecycleRequest::Subscribe { channel }
    );
    if channel == WINDOW.wire_id() {
        replay_desktop_snapshot(stream, display_id)?;
    }
    write_frame(
        stream,
        FrameKind::Reply,
        0,
        LIFECYCLE_CHANNEL,
        header.corr,
        &keld_ipc::codec::encode(&LifecycleResponse::Subscribed)?,
    )?;
    Ok(())
}

fn send_ready(stream: &mut BootstrapStream) -> Result<(), Box<dyn Error>> {
    write_frame(
        stream,
        FrameKind::Event,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(0),
        &keld_ipc::codec::encode(&LifecycleEvent::Ready)?,
    )?;
    Ok(())
}

fn expected_selected_probe(root: &Path, display_id: u32) -> Value {
    json!({
        "environment": expected_environment("1"),
        "initial": {
            "name": "Command Fixture",
            "root": root.to_str().expect("verbatim UTF-8 stage"),
            "version": "31.7.0-probe",
            "userData": r"D:\Core Command Fixture\Roaming\Command Fixture",
            "packaged": false,
        },
        "locale": "fr-CA",
        "mainRoleVersion": "0.0.1",
        "desktopSelected": true,
        "desktopPrimaryId": display_id,
        "desktopDark": false,
    })
}

#[test]
#[ignore = "requires the qualified Windows x64 provider fixture and inherited command probe environment"]
fn selected_command_holds_provider_and_rechecks_app_before_successor() -> Result<(), Box<dyn Error>>
{
    assert_probe_environment();
    let (_guard, source, root, entry) = prepared_pinned_launch_source(
        &pinned_providers(),
        &format!("{ENV_SAMPLE}{ORIGINAL_PROBE}"),
    );
    let (supervisor, recovery) = start_selected_probe(&source, &root, entry)?;
    let mut stream = receive_selected_probe(&supervisor, 1)?.into_stream();
    accept_selected_subscriptions(&mut stream, FIRST_DISPLAY_ID)?;
    // G1 is live: the retained graph must deny changing a provider while a
    // successor may need it. The app entry remains a separate recheck input.
    let changed = root.join(CHANGED_PROVIDER);
    let mut bytes = fs::read(&changed)?;
    bytes[0] ^= 1;
    assert!(
        fs::write(changed, bytes).is_err(),
        "provider must remain held"
    );
    let changed_app = root.join(entry);
    let mut app_bytes = fs::read(&changed_app)?;
    app_bytes.push(b' ');
    fs::write(changed_app, app_bytes)?;
    send_ready(&mut stream)?;
    assert!(recovery.arm());
    let (outcome, events) = terminal(&supervisor);
    assert!(
        matches!(outcome, SupervisorOutcome::Failed(RuntimeError::Lifecycle {
            phase: "retained role source recheck", ref source,
        }) if source.to_string().contains(entry.to_str().expect("fixture entry is UTF-8"))),
        "{outcome:?} / {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, PrimaryRoleEvent::Revoked { attempt: 1, .. }))
    );
    assert!(!events.iter().any(|event| matches!(
        event,
        PrimaryRoleEvent::Provisioned { attempt: 2.., .. }
            | PrimaryRoleEvent::Spawned { attempt: 2.., .. }
            | PrimaryRoleEvent::LinkBound { attempt: 2.., .. }
    )));
    assert!(supervisor.try_recv_bound_generation().is_none());
    assert_eq!(
        read_probes(&supervisor)?,
        [expected_selected_probe(&root, FIRST_DISPLAY_ID)],
        "{:?}",
        supervisor.output(),
    );
    Ok(())
}

#[test]
#[ignore = "requires the qualified Windows x64 provider fixture and inherited command probe environment"]
fn selected_command_delivers_pinned_facts_to_two_authenticated_generations()
-> Result<(), Box<dyn Error>> {
    assert_probe_environment();
    let (_guard, source, root, entry) = prepared_pinned_launch_source(
        &pinned_providers(),
        &format!("{ENV_SAMPLE}{ORIGINAL_PROBE}"),
    );
    let (supervisor, recovery) = start_selected_probe(&source, &root, entry)?;
    drop(source);
    let first = receive_selected_probe(&supervisor, 1)?;
    let first_generation = first.generation();
    let mut stream = first.into_stream();
    accept_selected_subscriptions(&mut stream, FIRST_DISPLAY_ID)?;
    let held_providers = [
        root.join("node_modules/@keld/api/app.js"),
        root.join("node_modules/@keld/kipc/transport.js"),
    ];
    for provider in &held_providers {
        assert!(fs::write(provider, b"first generation overwrite").is_err());
    }
    // Ready prompts the probe's deliberate exit. A separate snapshot on G2
    // proves its app observed its own Worker traffic after Ready.
    send_ready(&mut stream)?;
    assert!(recovery.arm());

    let successor = receive_selected_probe(&supervisor, 2)?;
    assert_ne!(successor.generation(), first_generation);
    let mut successor_stream = successor.into_stream();
    accept_selected_subscriptions(&mut successor_stream, SUCCESSOR_DISPLAY_ID)?;
    for provider in &held_providers {
        assert!(fs::write(provider, b"successor overwrite").is_err());
    }
    assert!(recovery.deny());
    send_ready(&mut successor_stream)?;

    let (outcome, events) = terminal(&supervisor);
    assert!(matches!(outcome, SupervisorOutcome::Stopped), "{outcome:?}");
    let retired_attempts: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            PrimaryRoleEvent::Revoked { attempt, .. } => Some(*attempt),
            _ => None,
        })
        .collect();
    assert_eq!(retired_attempts, [1, 2], "{events:?}");
    assert!(supervisor.try_recv_bound_generation().is_none());
    assert_eq!(
        read_probes(&supervisor)?,
        [
            expected_selected_probe(&root, FIRST_DISPLAY_ID),
            expected_selected_probe(&root, SUCCESSOR_DISPLAY_ID),
        ],
        "{:?}",
        supervisor.output(),
    );
    drop(supervisor);
    for provider in &held_providers {
        fs::write(provider, b"released after both generations")?;
    }
    Ok(())
}

fn read_witnesses(supervisor: &PrimaryRoleSupervisor, prefix: &str) -> Vec<Value> {
    parse_complete_witnesses(&supervisor.output().stdout, prefix)
}

fn parse_complete_witnesses(output: &str, prefix: &str) -> Vec<Value> {
    output
        .split_inclusive('\n')
        .filter(|line| line.ends_with('\n'))
        .map(str::trim_end)
        .filter_map(|line| line.strip_prefix(prefix))
        .map(|line| serde_json::from_str(line).expect("complete witness JSON"))
        .collect()
}

#[test]
fn witness_records_ignore_partial_stdout_until_terminated() {
    let partial = "other\nKELD_B2_WORKER {\"pid\":1}\nKELD_B2_WORKER {\"pid\":";
    assert_eq!(
        parse_complete_witnesses(partial, "KELD_B2_WORKER "),
        [json!({"pid": 1})]
    );
    let complete = format!("{partial}2}}\n");
    assert_eq!(
        parse_complete_witnesses(&complete, "KELD_B2_WORKER "),
        [json!({"pid": 1}), json!({"pid": 2})]
    );
}

fn wait_for_witnesses(
    supervisor: &PrimaryRoleSupervisor,
    prefix: &str,
    count: usize,
) -> Vec<Value> {
    let deadline = Instant::now() + BOUND;
    loop {
        let witnesses = read_witnesses(supervisor, prefix);
        if witnesses.len() >= count {
            return witnesses;
        }
        assert!(
            Instant::now() < deadline,
            "missing {prefix}: {:?}",
            supervisor.output()
        );
        std::thread::yield_now();
    }
}

#[test]
#[ignore = "requires Windows Bun and the separately recorded joined SDK fixture"]
fn witness_joined_decoder_and_worker_in_two_authenticated_generations() -> Result<(), Box<dyn Error>>
{
    let preload_dir = tempfile::tempdir()?;
    let worker_preload = preload_dir.path().join("worker-witness.mjs");
    let parent_preload = preload_dir.path().join("parent-witness.mjs");
    fs::write(&worker_preload, WORKER_WITNESS_PRELOAD)?;
    let worker_preload_js =
        serde_json::to_string(worker_preload.to_str().expect("UTF-8 temp path"))?;
    fs::write(
        &parent_preload,
        format!(
            r#"
const wt = require("node:worker_threads");
const NativeWorker = wt.Worker;
const originalGetFloat64 = DataView.prototype.getFloat64;
let witnessedDecoder = false;
DataView.prototype.getFloat64 = function (...args) {{
  const value = Reflect.apply(originalGetFloat64, this, args);
  const stack = new Error().stack;
  if (!witnessedDecoder && stack?.includes("window-source.js") &&
      stack.includes("window-codec.js")) {{
    const match = stack.match(/([a-z]:[\\/][^\n()]*window-codec\.js):\d+:\d+/i);
    if (!match) throw new Error("executing snapshot decoder missing from stack");
    const fs = require("node:fs");
    const path = match[1];
    const stat = fs.statSync(path, {{ bigint: true }});
    witnessedDecoder = true;
    fs.writeSync(1, "KELD_B2_DECODER " + JSON.stringify({{
      pid: process.pid, path, value, stack,
      dev: String(stat.dev), ino: String(stat.ino),
      sha256: require("node:crypto").createHash("sha256").update(fs.readFileSync(path)).digest("hex")
    }}) + "\n");
  }}
  return value;
}};
wt.Worker = new Proxy(NativeWorker, {{
  construct(target, args, newTarget) {{
    args[1] = {{ ...args[1], preload: [{worker_preload_js}] }};
    const worker = Reflect.construct(target, args, newTarget);
    worker.on("message", message => {{
      if (message?.t === "keld-b2-worker-witness") {{
        require("node:fs").writeSync(1, "KELD_B2_WORKER " + JSON.stringify(message) + "\n");
        process.exit(17);
      }}
    }});
    return worker;
  }}
}});
"#
        ),
    )?;

    let (_guard, source, root, entry) = prepared_witness_launch_source(
        &pinned_providers(),
        &format!("{ENV_SAMPLE}{MODULE_WITNESS_PROBE}"),
    );
    let (supervisor, recovery) =
        start_selected_probe_with_preload(&source, &root, entry, Some(&parent_preload))?;
    drop(source);
    let decoder = root.join("node_modules/@keld/api/window-codec.js");
    let worker = root.join("node_modules/@keld/kipc/transport.js");
    let expected = [
        (
            &decoder,
            "20b0defb936c4d0b693b99fb8315726a61ac53c77e7d594da3a54a8e7153e431",
        ),
        (
            &worker,
            "c4e8cbf6add4fdd746674a19d969b8cdd1d5e7776fc167b2fafbaded85948c4b",
        ),
    ];
    for (path, digest) in expected {
        assert_eq!(format!("{:x}", Sha256::digest(fs::read(path)?)), digest);
    }
    let mut generations = Vec::new();
    for (attempt, display) in [(1_u32, FIRST_DISPLAY_ID), (2, SUCCESSOR_DISPLAY_ID)] {
        let bound = receive_selected_probe(&supervisor, attempt)?;
        generations.push(bound.generation());
        let mut stream = bound.into_stream();
        stream.set_app_link_read_deadline(Some(BOUND))?;
        // The joined SDK currently subscribes only Window; channel 7 is not
        // represented by this fixture and remains a separate acceptance gap.
        accept_selected_subscription(&mut stream, WINDOW.wire_id(), 2, display)?;
        for (path, _) in expected {
            assert!(fs::write(path, b"must remain retained").is_err());
        }
        if attempt == 1 {
            assert!(recovery.arm());
        } else {
            assert!(recovery.deny());
        }
        send_ready(&mut stream)?;
        let observed = wait_for_witnesses(&supervisor, "KELD_B2_WINDOW ", attempt as usize);
        assert_eq!(observed[(attempt - 1) as usize]["display"], display);
        // Exercise the actual Worker's receive validator after the valid
        // snapshot. This invalid channel must produce its own source stack.
        write_frame(
            &mut stream,
            FrameKind::Event,
            0,
            keld_ipc::channel_table::DIALOG.id(),
            CorrelationId(0),
            &[],
        )?;
        wait_for_witnesses(&supervisor, "KELD_B2_WORKER ", attempt as usize);
    }
    assert_ne!(generations[0], generations[1]);
    let (outcome, events) = terminal(&supervisor);
    assert!(matches!(outcome, SupervisorOutcome::Stopped), "{outcome:?}");
    let pids: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            PrimaryRoleEvent::Spawned {
                pid,
                attempt,
                generation,
            } => Some((*pid, *attempt, *generation)),
            _ => None,
        })
        .collect();
    assert_eq!(pids.len(), 2, "{events:?}");
    assert_ne!(pids[0].0, pids[1].0);
    let decoder_witness = read_witnesses(&supervisor, "KELD_B2_DECODER ");
    let window_witness = read_witnesses(&supervisor, "KELD_B2_WINDOW ");
    let worker_witness = read_witnesses(&supervisor, "KELD_B2_WORKER ");
    assert_eq!(decoder_witness.len(), 2);
    assert_eq!(window_witness.len(), 2);
    assert_eq!(worker_witness.len(), 2);
    for i in 0..2 {
        assert_eq!(pids[i].1 as usize, i + 1);
        assert_eq!(pids[i].2, generations[i]);
        assert_eq!(window_witness[i]["pid"], pids[i].0);
        assert_eq!(decoder_witness[i]["value"], 1.25);
        assert!(
            decoder_witness[i]["stack"]
                .as_str()
                .is_some_and(|stack| stack.contains("window-source.js"))
        );
        for (value, path, digest) in [
            (&decoder_witness[i], &decoder, expected[0].1),
            (&worker_witness[i], &worker, expected[1].1),
        ] {
            assert_eq!(value["pid"], pids[i].0);
            let actual = value["path"].as_str().expect("executing source path");
            assert_eq!(fs::canonicalize(actual)?, fs::canonicalize(path)?);
            assert_eq!(value["sha256"], digest);
            assert_ne!(value["ino"], "0");
            assert!(
                value["stack"]
                    .as_str()
                    .is_some_and(|stack| stack.contains(actual))
            );
        }
    }
    assert_eq!(decoder_witness[0]["dev"], decoder_witness[1]["dev"]);
    assert_eq!(decoder_witness[0]["ino"], decoder_witness[1]["ino"]);
    assert_eq!(worker_witness[0]["dev"], worker_witness[1]["dev"]);
    assert_eq!(worker_witness[0]["ino"], worker_witness[1]["ino"]);
    println!(
        "KELD_B2_MODULE_WITNESSES {}",
        json!({"decoder": decoder_witness, "worker": worker_witness, "pids": pids.iter().map(|p| p.0).collect::<Vec<_>>() })
    );
    drop(supervisor);
    for (path, _) in expected {
        fs::write(path, b"released after both generations")?;
    }
    Ok(())
}

#[test]
#[ignore = "requires Windows Bun, the qualified transport, and inherited command probe environment"]
fn ordinary_command_still_scrubs_private_environment_and_preserves_ordinary_mode()
-> Result<(), Box<dyn Error>> {
    assert_probe_environment();
    let guard = tempfile::tempdir()?;
    let root = guard.path().canonicalize()?;
    let entry = std::path::Path::new("ordinary.mjs");
    let provider = pinned_providers().join("@keld/kipc/transport.js");
    let provider = serde_json::to_string(provider.to_str().expect("UTF-8 fixture path"))?;
    fs::write(
        root.join(entry),
        format!(
            "import {{ pathToFileURL }} from 'node:url';\nconst provider = {provider};\n{ENV_SAMPLE}{ORDINARY_PROBE}"
        ),
    )?;
    let supervisor = PrimaryRoleSupervisor::start(
        windows_primary_config(&root, entry, false, None, None)?.admission_timeout(BOUND),
    )?;
    let (outcome, events) = terminal(&supervisor);
    assert!(matches!(outcome, SupervisorOutcome::Stopped), "{outcome:?}");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, PrimaryRoleEvent::Spawned { attempt: 1, .. }))
    );
    assert_eq!(
        read_probes(&supervisor)?,
        [json!({ "environment": expected_environment("0"), "desktopSelected": false })],
        "{:?}",
        supervisor.output(),
    );
    Ok(())
}
