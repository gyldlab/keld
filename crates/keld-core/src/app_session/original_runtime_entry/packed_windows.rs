//! Exercise the checked Windows entry bytes with the SDK producer and real role admission.
use super::super::{ORIGINAL_RUNTIME_ENTRY_PATH, windows_original_entry_bootstrap_source};
use keld_ipc::channel_table::WINDOW;
use keld_ipc::lifecycle::{LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleRequest, LifecycleResponse};
use keld_ipc::link::{read_frame, write_frame};
use keld_ipc::{AppLinkDeadlines, CorrelationId, FrameKind};
use keld_runtime::primary::{PrimaryRoleConfig, PrimaryRoleEvent, PrimaryRoleSupervisor};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const ORIGINAL: &str = "selected.mjs";
const BOUND: Duration = Duration::from_secs(12);
const ENTRY: &str = r"
import { createRequire } from 'node:module';
import { writeFileSync } from 'node:fs';
const require = createRequire(import.meta.url);
const electron = require('electron');
await electron.app.whenReady();
writeFileSync(new URL('./entered.json', import.meta.url), JSON.stringify({
  version: process.versions.electron,
  type: process.type,
  defaultApp: process.defaultApp,
  args: process.argv.slice(1),
}));
process.exit(0);
";

fn run_checked(command: &mut Command) -> Result<(), Box<dyn Error>> {
    let output = command.output()?;
    assert!(
        output.status.success(),
        "command {command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn unpack_providers(root: &Path) -> Result<(), Box<dyn Error>> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/@keld");
    run_checked(
        Command::new("bun")
            .arg(packages.join("kipc/scripts/package-artifacts.ts"))
            .arg(root)
            .arg("--electron"),
    )?;
    for (source, target) in [
        ("api", "@keld/api"),
        ("kipc", "@keld/kipc"),
        ("electron", "electron"),
    ] {
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(packages.join(source).join("package.json"))?)?;
        let version = manifest["version"]
            .as_str()
            .ok_or("missing provider version")?;
        let directory = root.join("node_modules").join(target);
        fs::create_dir_all(&directory)?;
        run_checked(
            Command::new("tar")
                .arg("-xzf")
                .arg(root.join(format!("keld-{source}-{version}.tgz")))
                .arg("--strip-components=1")
                .arg("-C")
                .arg(directory),
        )?;
    }
    Ok(())
}

fn launch(root: &Path, expect_early_facade: bool) -> Result<(), Box<dyn Error>> {
    let script = root.join(ORIGINAL);
    let config = PrimaryRoleConfig::new("bun")
        .env_remove("BUN_OPTIONS")
        .args([
            "--no-env-file",
            "--no-install",
            "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
            "--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\"",
            "run",
        ])
        .arg(root.join(ORIGINAL_RUNTIME_ENTRY_PATH))
        .arg(&script)
        .require_runtime_process_facts()
        .admission_timeout(BOUND)
        .current_dir(root);
    let supervisor = PrimaryRoleSupervisor::start_with_bound_generations(config)?;
    let bound = supervisor
        .recv_bound_generation(BOUND)
        .ok_or_else(|| format!("no admitted generation: {:?}", supervisor.output()))?;
    let versions = bound
        .runtime_versions()
        .ok_or("admission did not retain runtime versions")?;
    assert!(versions.contains_key("bun"));
    assert_eq!(
        versions.get("electron").map(String::as_str),
        expect_early_facade.then_some("0.0.1")
    );
    let mut stream = bound.into_stream();
    stream.set_app_link_read_deadline(Some(BOUND))?;
    let (header, message) = read_frame(&mut stream)?;
    assert_eq!(header.kind, FrameKind::Call);
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(header.corr, CorrelationId(2));
    assert_eq!(
        keld_ipc::codec::decode::<LifecycleRequest>(&message)?,
        LifecycleRequest::Subscribe {
            channel: WINDOW.wire_id()
        }
    );
    let marker = root.join("entered.json");
    assert!(!marker.exists());
    write_frame(
        &mut stream,
        FrameKind::Reply,
        0,
        LIFECYCLE_CHANNEL,
        header.corr,
        &keld_ipc::codec::encode(&LifecycleResponse::Subscribed)?,
    )?;
    write_frame(
        &mut stream,
        FrameKind::Event,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(0),
        &keld_ipc::codec::encode(&LifecycleEvent::Ready)?,
    )?;
    let end = Instant::now() + BOUND;
    loop {
        let event = supervisor
            .recv_event(end.saturating_duration_since(Instant::now()))
            .ok_or_else(|| format!("role did not retire: {:?}", supervisor.output()))?;
        if matches!(event, PrimaryRoleEvent::Revoked { .. }) {
            break;
        }
    }
    assert!(
        matches!(
            supervisor.wait_for_outcome(),
            keld_runtime::SupervisorOutcome::Stopped
        ),
        "{:?}",
        supervisor.output()
    );
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(&marker)?)?;
    assert_eq!(receipt["args"], serde_json::json!([script]));
    assert_eq!(receipt["defaultApp"], true);
    assert_eq!(receipt["type"], "browser");
    assert_eq!(receipt["version"], "0.0.1");
    fs::remove_file(marker)?;
    Ok(())
}

#[test]
#[ignore = "builds the real SDK artifacts; requires the pinned Bun package dependencies in api, kipc and electron"]
fn packed_windows_entry_preserves_identity_argv_and_pre_entry_capture() -> Result<(), Box<dyn Error>>
{
    let stage = tempfile::tempdir()?;
    let root = stage.path();
    unpack_providers(root)?;
    fs::write(root.join(ORIGINAL), ENTRY)?;
    let wrapper = root.join(ORIGINAL_RUNTIME_ENTRY_PATH);
    let source = String::from_utf8(windows_original_entry_bootstrap_source(ORIGINAL)?)?;
    fs::write(&wrapper, &source)?;
    let other = root.join("different.mjs");
    fs::write(&other, ENTRY)?;
    let wrong = Command::new("bun")
        .args([
            "--no-env-file",
            "--no-install",
            "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
            "run",
        ])
        .arg(&wrapper)
        .arg(other)
        .current_dir(root)
        .output()?;
    assert!(!wrong.status.success());
    assert!(
        String::from_utf8_lossy(&wrong.stderr)
            .contains("Selected original does not match sealed bootstrap")
    );
    assert!(!root.join("entered.json").exists());
    launch(root, true)?;
    let early = "await import(\"./node_modules/electron/dist/index.js\");";
    assert_eq!(source.matches(early).count(), 1);
    fs::write(wrapper, source.replace(early, ""))?;
    launch(root, false)
}
