//! Below-gate commands use the real Windows Runtime and immutable provider tuple.
use super::{SelectedLaunch, native, prepare_selected};
use crate::app_session::graph_contract::tests::prepared_pinned_launch_source;
use crate::app_session::windows_primary_config;
use keld_ipc::channel_table::WINDOW;
use keld_ipc::lifecycle::{LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleRequest, LifecycleResponse};
use keld_ipc::link::{read_frame, write_frame};
use keld_ipc::{AppLinkDeadlines, CorrelationId, FrameKind};
use keld_runtime::primary::{PrimaryRoleEvent, PrimaryRoleSupervisor};
use keld_runtime::{RuntimeError, SupervisorOutcome};
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BOUND: Duration = Duration::from_secs(15);
const CHANGED_PROVIDER: &str = "node_modules/electron/dist/app-session-facts.d.ts";
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
import { app } from "electron";
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
}) + "\n");
process.exit(17);
"#;
const ORDINARY_PROBE: &str = r#"
writeSync(1, "KELD_WINDOWS_COMMAND_PROBE " + JSON.stringify(environment) + "\n");
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

fn read_probe(supervisor: &PrimaryRoleSupervisor) -> Result<Value, Box<dyn Error>> {
    let output = supervisor.output();
    let mut records = output
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix(OUTPUT));
    let result = records
        .next()
        .ok_or_else(|| format!("Bun probe absent: {output:?}"))?;
    assert!(
        records.next().is_none(),
        "only the verified generation emitted a probe"
    );
    Ok(serde_json::from_str(result)?)
}

#[test]
#[ignore = "requires the qualified Windows x64 provider fixture and inherited command probe environment"]
fn selected_command_delivers_pinned_facts_and_rechecks_provider_before_successor()
-> Result<(), Box<dyn Error>> {
    assert_probe_environment();
    let providers = std::env::var_os("KELD_TEST_PINNED_WINDOWS_PROVIDERS")
        .map(PathBuf::from)
        .expect("the three qualified provider directories under node_modules");
    let (_guard, source, root, entry) =
        prepared_pinned_launch_source(&providers, &format!("{ENV_SAMPLE}{ORIGINAL_PROBE}"));
    let prepared = prepare_selected(
        SelectedLaunch {
            root: &root,
            entry,
            name: "Command Fixture",
            version: Some("31.7.0-probe"),
            declaration: None,
            source: &source,
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
    let (supervisor, recovery) = PrimaryRoleSupervisor::start_with_bound_generations_gated(
        windows_primary_config(&root, entry, true, None, Some(prepared.config))?
            .admission_timeout(BOUND),
    )?;
    let bound = supervisor.recv_bound_generation(BOUND).ok_or_else(|| {
        format!(
            "original G1 did not authenticate: {:?}",
            supervisor.output()
        )
    })?;
    assert_eq!(
        bound
            .runtime_versions()
            .and_then(|versions| versions.get("electron"))
            .map(String::as_str),
        Some("0.0.1")
    );
    let mut stream = bound.into_stream();
    stream.set_app_link_read_deadline(Some(BOUND))?;
    let (header, message) = read_frame(&mut stream)?;
    assert_eq!(header.kind, FrameKind::Call);
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(
        keld_ipc::codec::decode::<LifecycleRequest>(&message)?,
        LifecycleRequest::Subscribe {
            channel: WINDOW.wire_id()
        }
    );
    write_frame(
        &mut stream,
        FrameKind::Reply,
        0,
        LIFECYCLE_CHANNEL,
        header.corr,
        &keld_ipc::codec::encode(&LifecycleResponse::Subscribed)?,
    )?;
    // Change one byte in a nonexecuted, otherwise admitted provider member.
    // G1 already captured the real provider and cannot finish before Ready.
    let changed = root.join(CHANGED_PROVIDER);
    let mut bytes = fs::read(&changed)?;
    bytes[0] ^= 1;
    fs::write(changed, bytes)?;
    write_frame(
        &mut stream,
        FrameKind::Event,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(0),
        &keld_ipc::codec::encode(&LifecycleEvent::Ready)?,
    )?;
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
        read_probe(&supervisor)?,
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
        })
    );
    Ok(())
}

#[test]
#[ignore = "requires Windows Bun and inherited command probe environment"]
fn ordinary_command_still_scrubs_private_environment_and_preserves_ordinary_mode()
-> Result<(), Box<dyn Error>> {
    assert_probe_environment();
    let guard = tempfile::tempdir()?;
    let root = guard.path().canonicalize()?;
    let entry = std::path::Path::new("ordinary.mjs");
    fs::write(root.join(entry), format!("{ENV_SAMPLE}{ORDINARY_PROBE}"))?;
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
    assert_eq!(read_probe(&supervisor)?, expected_environment("0"));
    Ok(())
}
