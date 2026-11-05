//! GH453 real no-flag Host/Bun pair. NOT RUN; no original-app claim.
use std::fs;
use std::io::{BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};
use std::time::Instant;

use crate::renderer_bridge::bundle_public_api_entry;
use crate::support::EVENT_DEADLINE;
use crate::support::control::{accept_before, read_control_line, wait_child_output};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::product::ProductFixture;

struct LiveInstance {
    cleanup: ShippingLaunchCleanup,
    lease: ChildStdin,
    control: BufReader<UnixStream>,
    stage: keld_cli::boot::DevBootStage,
    observed: serde_json::Value,
    _fixture: ProductFixture,
}

fn launch(repo: &Path, id: &str, name: &str) -> LiveInstance {
    let mut fixture = ProductFixture::new(name);
    let project = fixture.project.clone();
    fs::create_dir_all(project.join("src")).expect("fixture source");
    let endpoint = project.join("control.sock");
    let listener = UnixListener::bind(&endpoint).expect("fixture control listener");
    listener.set_nonblocking(true).expect("bounded accept");
    let facade = serde_json::to_string(&repo.join("packages/@keld/electron/src/app.ts"))
        .expect("facade import");
    let api =
        serde_json::to_string(&repo.join("packages/@keld/api/src/app.ts")).expect("API import");
    let control = serde_json::to_string(&endpoint).expect("endpoint JSON");
    let entry = format!(
        r#"import {{ app as facade }} from {facade};
import {{ app }} from {api};
import {{ createConnection }} from "node:net";
import {{ once }} from "node:events";
const before = facade.requestSingleInstanceLock();
const beforeReady = app.isReady();
await app.whenReady();
const after = facade.requestSingleInstanceLock();
const control = createConnection({control});
await once(control, "connect");
control.write(JSON.stringify({{ before, after, beforeReady, afterReady: app.isReady(), pid: process.pid }}) + "\n");
await once(control, "data");
await app.quit();
control.end();
console.error("GH453_QUIT_ACK");
"#
    );
    bundle_public_api_entry(&project, repo, "gh453-single-instance", &entry);
    fixture.entry_source = fs::read_to_string(project.join("src/main.ts")).expect("bundled role");
    fixture.single_instance = Some(id.to_owned());
    // This test consumes explicit Legacy; it does not silently select a profile.
    fs::write(
        project.join("keld.permissions.jsonc"),
        "{\"profile\":\"legacy\"}\n",
    )
    .expect("explicit Legacy fixture policy");
    let stage = fixture.stage();
    let descriptor: serde_json::Value = serde_json::from_slice(
        &fs::read(stage.root().join("keld.boot.json")).expect("descriptor bytes"),
    )
    .expect("descriptor JSON");
    assert_eq!(
        descriptor["singleInstance"]["id"], id,
        "compiler lost declaration"
    );
    assert_eq!(
        descriptor["schema"], 3,
        "explicit-profile renderer selection changed"
    );
    let child = Command::new(stage.host())
        .process_group(0)
        .current_dir(project.parent().expect("different cwd"))
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping no-flag Host");
    let host_pid = child.id();
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let lease = cleanup
        .cli
        .as_mut()
        .expect("Host custody")
        .stdin
        .take()
        .expect("dev lease");
    let stream = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    stream
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded read");
    let mut control = BufReader::new(stream);
    let observed: serde_json::Value =
        serde_json::from_str(&read_control_line(&mut control)).expect("Bun observations");
    cleanup.record_authenticated_groups(host_pid, 0);
    assert_eq!(observed["beforeReady"], false);
    assert_eq!(observed["afterReady"], true);
    assert!(
        observed["pid"]
            .as_u64()
            .is_some_and(|pid| pid != u64::from(host_pid))
    );
    LiveInstance {
        cleanup,
        lease,
        control,
        stage,
        observed,
        _fixture: fixture,
    }
}

fn quit(mut instance: LiveInstance) {
    instance
        .control
        .get_mut()
        .write_all(b"quit\n")
        .expect("release role after observation");
    let output = wait_child_output(instance.cleanup.release(), EVENT_DEADLINE);
    drop(instance.lease);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("GH453_QUIT_ACK"));
    assert!(
        !instance.stage.root().exists(),
        "stage cleanup after normal Host quit"
    );
}

#[test]
fn shipping_hosts_use_one_real_noarg_verdict_across_distinct_stages() {
    let temp = tempfile::tempdir().expect("pair fixture");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace");
    let unique: String = temp
        .path()
        .file_name()
        .expect("unique temp name")
        .to_string_lossy()
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| char::from(byte.to_ascii_lowercase()))
        .collect();
    let id = format!("org.keld.gh453.{unique}");
    let first = launch(repo, &id, "first display name");
    assert_eq!(first.observed["before"], true);
    assert_eq!(first.observed["after"], true);
    let second = launch(repo, &id, "different display name");
    assert_ne!(first.stage.root(), second.stage.root());
    assert_eq!(
        second.observed["before"], false,
        "constant true or stage-key bypass"
    );
    assert_eq!(
        second.observed["after"], false,
        "losing Bun still reached Ready"
    );
    quit(second);
    quit(first);
    let third = launch(repo, &id, "relaunch");
    assert_eq!(third.observed["before"], true, "normal Host release failed");
    assert_eq!(third.observed["after"], true);
    quit(third);
    // Persistent empty internal lease is preserved by design, never unlinked
    // to counterfeit normal release. This fixture does not prove Host-death
    // noninheritance, dirty original app Quit, or original draw.io startup.
}
