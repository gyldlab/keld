//! Below-gate commands use the real Windows Runtime and immutable provider tuple.
use super::{SelectedLaunch, native, prepare_selected};
use crate::app_session::graph_contract::tests::prepared_pinned_launch_source;
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
    let prepared = prepare_selected(
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
        let (header, message) = read_frame(stream)?;
        assert_eq!(header.kind, FrameKind::Call);
        assert_eq!(header.channel, LIFECYCLE_CHANNEL);
        assert_eq!(header.corr, CorrelationId(correlation));
        assert_eq!(
            keld_ipc::codec::decode::<LifecycleRequest>(&message)?,
            LifecycleRequest::Subscribe {
                channel: channel.wire_id()
            }
        );
        if channel.id() == WINDOW.id() {
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
    }
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
fn selected_command_delivers_pinned_facts_and_rechecks_provider_before_successor()
-> Result<(), Box<dyn Error>> {
    assert_probe_environment();
    let (_guard, source, root, entry) = prepared_pinned_launch_source(
        &pinned_providers(),
        &format!("{ENV_SAMPLE}{ORIGINAL_PROBE}"),
    );
    let (supervisor, recovery) = start_selected_probe(&source, &root, entry)?;
    let mut stream = receive_selected_probe(&supervisor, 1)?.into_stream();
    accept_selected_subscriptions(&mut stream, FIRST_DISPLAY_ID)?;
    // Change one byte in a nonexecuted, otherwise admitted provider member.
    // G1 already captured the real provider and cannot finish before Ready.
    let changed = root.join(CHANGED_PROVIDER);
    let mut bytes = fs::read(&changed)?;
    bytes[0] ^= 1;
    fs::write(changed, bytes)?;
    send_ready(&mut stream)?;
    assert!(recovery.arm());
    let (outcome, events) = terminal(&supervisor);
    assert!(
        matches!(outcome, SupervisorOutcome::Failed(RuntimeError::Lifecycle {
            phase: "retained role source recheck", ref source,
        }) if source.to_string().contains(CHANGED_PROVIDER)),
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
    let first = receive_selected_probe(&supervisor, 1)?;
    let first_generation = first.generation();
    let mut stream = first.into_stream();
    accept_selected_subscriptions(&mut stream, FIRST_DISPLAY_ID)?;
    // Ready prompts the probe's deliberate exit. A separate snapshot on G2
    // proves its app observed its own Worker traffic after Ready.
    send_ready(&mut stream)?;
    assert!(recovery.arm());

    let successor = receive_selected_probe(&supervisor, 2)?;
    assert_ne!(successor.generation(), first_generation);
    let mut successor_stream = successor.into_stream();
    accept_selected_subscriptions(&mut successor_stream, SUCCESSOR_DISPLAY_ID)?;
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
    source.launch_graph().recheck()?;
    assert_eq!(
        read_probes(&supervisor)?,
        [
            expected_selected_probe(&root, FIRST_DISPLAY_ID),
            expected_selected_probe(&root, SUCCESSOR_DISPLAY_ID),
        ],
        "{:?}",
        supervisor.output(),
    );
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
