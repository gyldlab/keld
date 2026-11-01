//! Actual no-flag Host -> Guardian -> canonical preload -> facade data delivery.
//! This is a launch-facts fixture, not Draw.io or installed-package acceptance.

use std::fs;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::renderer_bridge::bundle_public_api_entry;
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::product::ProductFixture;
use crate::support::recovery_cycle::RecoveryCycle;

fn independent_native_facts() -> serde_json::Value {
    // Independent public URL-returning Foundation API, not Core's path-query
    // helper or the emitted launch record. This observer creates no directory.
    const SCRIPT: &str = r#"
import Foundation
let manager = FileManager.default
let appData = manager.urls(for: .applicationSupportDirectory, in: .userDomainMask)
let documents = manager.urls(for: .documentDirectory, in: .userDomainMask)
guard appData.count == 1 && documents.count == 1 else { exit(2) }
// These unbundled test processes use the same native preferences context.
// This observes preferred language, not Chromium/WebKit locale negotiation.
guard let locale = Locale.preferredLanguages.first else { exit(3) }
let result = ["appData": appData[0].path, "documents": documents[0].path, "locale": locale]
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: result))
"#;
    let child = Command::new("/usr/bin/xcrun")
        .args(["swift", "-e", SCRIPT])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("independent native facts observer");
    let output = wait_child_output(child, EVENT_DEADLINE);
    assert!(
        output.status.success(),
        "native facts observer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("native facts JSON")
}

#[test]
fn shipping_host_delivers_development_facts_before_ready_and_quits_cleanly() {
    let temp = tempfile::tempdir().expect("launch-facts project");
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("src")).expect("source directory");
    let native = independent_native_facts();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let facade = serde_json::to_string(&repo.join("packages/@keld/electron/src/app.ts"))
        .expect("facade import");
    let api = serde_json::to_string(&repo.join("packages/@keld/api/src/app.ts"))
        .expect("lifecycle import");
    let entry = format!(
        r#"import {{ app as facade }} from {facade};
import {{ app }} from {api};
process.env.HOME = "/not-the-native-home";
process.env.LANG = "zz_TEST.UTF-8";
process.env.LC_ALL = "zz_TEST.UTF-8";
const read = () => ({{ name: facade.getName(), property: facade.name,
    root: facade.getAppPath(), packaged: facade.isPackaged,
    version: facade.getVersion(), appData: facade.getPath("appData"),
    userData: facade.getPath("userData"), documents: facade.getPath("documents"),
    locale: facade.getLocale() }});
console.error("GH452_BEFORE_READY " + JSON.stringify({{...read(), ready: app.isReady()}}));
await app.whenReady();
console.error("GH452_AFTER_READY " + JSON.stringify({{...read(), ready: app.isReady()}}));
await app.quit();
console.error("GH452_QUIT_ACK");
"#
    );
    bundle_public_api_entry(&project, repo, "gh452-launch-facts", &entry);
    fs::write(
        project.join("keld.config.ts"),
        "export default {\n  name: \"GH452 native values\",\n  version: \"29.1.7-native+initial\",\n  entry: \"src/main.ts\",\n  initialWindow: \"none\",\n} as const;\n",
    ).expect("facade development config");
    let stage =
        keld_cli::boot::stage_dev_boot(&project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
            .expect("shipping stage compiler");
    let descriptor: serde_json::Value = serde_json::from_slice(
        &fs::read(stage.root().join("keld.boot.json")).expect("compiled descriptor"),
    )
    .expect("descriptor JSON");
    assert_eq!(
        descriptor["name"], "GH452 native values",
        "fixture must use the admitted template grammar before testing delivery"
    );
    assert_eq!(descriptor["version"], "29.1.7-native+initial");
    let child = Command::new(stage.host())
        .process_group(0)
        .current_dir(temp.path()) // A different cwd must not become the app path.
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping no-flag Host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let lease = cleanup
        .cli
        .as_mut()
        .expect("live Host")
        .stdin
        .take()
        .expect("retained dev lease");
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    drop(lease);
    let stderr = String::from_utf8(output.stderr).expect("Host output UTF-8");
    assert!(output.status.success(), "Host failed: {stderr}");
    for (prefix, ready) in [("GH452_BEFORE_READY ", false), ("GH452_AFTER_READY ", true)] {
        let records: Vec<_> = stderr
            .lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .collect();
        assert_eq!(records.len(), 1, "one exact {prefix} observation: {stderr}");
        let observed: serde_json::Value = serde_json::from_str(records[0]).expect("observed data");
        assert_eq!(
            observed,
            serde_json::json!({
                "name": "GH452 native values", "property": "GH452 native values",
                "root": stage.root().to_str().expect("test root UTF-8"), "packaged": false, "ready": ready,
                "version": "29.1.7-native+initial", "appData": native["appData"],
                "documents": native["documents"],
                "locale": native["locale"],
                "userData": Path::new(native["appData"].as_str().expect("native appData")).join("GH452 native values"),
            })
        );
    }
    assert_eq!(
        stderr
            .lines()
            .filter(|line| *line == "GH452_QUIT_ACK")
            .count(),
        1,
        "{stderr}"
    );
    assert!(
        !stage.root().exists(),
        "normal Host teardown must remove the private stage"
    );
}

#[test]
fn shipping_host_replays_frozen_launch_facts_into_a_recovered_bun_generation() {
    let mut fixture = ProductFixture::new("gh452-recovery");
    fixture.version = Some("29.1.7-native+recovery");
    let native = independent_native_facts();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let import_path =
        |relative: &str| serde_json::to_string(&repo.join(relative)).expect("import JSON");
    let facade = import_path("packages/@keld/electron/src/app.ts");
    let api_link = import_path("packages/@keld/api/src/link.ts");
    let kipc = import_path("packages/@keld/kipc/src/transport.ts");
    let observation_path = fixture.root.path().join("launch-observations.jsonl");
    let observation_json = serde_json::to_string(&observation_path).expect("observer path JSON");
    let entry = format!(
        r#"
import {{ app as facade }} from {facade};
import {{ appendFileSync }} from "node:fs";
import {{ DrainSignal, FrameKind, FrameReader, LIFECYCLE_CHANNEL, WriteQueue, errorFromErrFrame, isWin32PipeEndpoint, parseAppLink, withIoDeadline }} from {api_link};
import {{ parseWin32DiagnosticPort }} from {kipc};
// Test-only observation before the unchanged authenticated lifecycle harness.
process.env.LANG = "zz_TEST.UTF-8";
process.env.LC_ALL = "zz_TEST.UTF-8";
appendFileSync({observation_json}, JSON.stringify({{
  name: facade.getName(), root: facade.getAppPath(), packaged: facade.isPackaged,
  version: facade.getVersion(), appData: facade.getPath("appData"),
  userData: facade.getPath("userData"), documents: facade.getPath("documents"),
  locale: facade.getLocale(),
  ready: facade.isReady(), pid: process.pid,
}}) + "\n");
{}"#,
        include_str!("../../fixtures/t1b_harness.ts")
    );
    bundle_public_api_entry(&fixture.project, repo, "gh452-recovery", &entry);
    fixture.entry_source = fs::read_to_string(fixture.project.join("src/main.ts"))
        .expect("exact facade plus existing recovery harness bundle");
    let mut cycle = RecoveryCycle::launch(&fixture, "launch-facts-recovery");
    let staged_roots: Vec<_> = fs::read_dir(fixture.stage_parent())
        .expect("physical staged roots")
        .map(|entry| entry.expect("stage entry").path())
        .collect();
    assert_eq!(
        staged_roots.len(),
        1,
        "fixture must have one actual Host stage"
    );
    // Resolve the independently discovered fixture path while the stage exists.
    // Host validation publishes its canonical root, not the /var symlink spelling.
    // Keep the observed payload untouched so a foreign or stale root still fails.
    let expected_root = staged_roots[0]
        .canonicalize()
        .expect("canonical physical stage before recovery");
    fs::write(
        fixture.project.join("keld.config.ts"),
        "export default { name: 'FORGED', version: 'FORGED' };\n",
    )
    .expect("original project metadata changes cannot rewrite the retained launch snapshot");
    let first = cycle.crash_and_recover();
    let second = cycle.current_evidence();
    cycle.quit_and_expect_success();
    let records: Vec<serde_json::Value> = fs::read_to_string(observation_path)
        .expect("actual role observations")
        .lines()
        .map(|line| serde_json::from_str(line).expect("role observation JSON"))
        .collect();
    assert_eq!(
        records.len(),
        2,
        "exactly initial and recovered observations"
    );
    for (record, pid) in records.iter().zip([first.bun_pid, second.bun_pid]) {
        assert_eq!(
            *record,
            serde_json::json!({
                "name": crate::support::TITLE, "root": expected_root,
                "packaged": false, "ready": false, "pid": pid,
                "version": "29.1.7-native+recovery", "appData": native["appData"],
                "documents": native["documents"],
                "locale": native["locale"],
                "userData": Path::new(native["appData"].as_str().expect("native appData")).join(crate::support::TITLE),
            })
        );
    }
    assert!(
        !staged_roots[0].exists(),
        "accepted Quit removes the retained stage"
    );
}
