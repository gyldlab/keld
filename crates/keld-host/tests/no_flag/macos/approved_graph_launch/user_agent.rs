//! Shipping no-flag Host cold configuration -> actual native UA.
//! No original source, app preload, graph/provider admission or UA shim.

use super::{FirstProofLaunch, host};
use crate::renderer_bridge::bundle_public_api_entry;
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::native_window::{
    NativeWindowObserver, compile_native_window_census, native_windows,
};
use crate::support::process::{await_process_gone_before, process_ids, session_dirs_for};
use keld_cli::boot::DevBootStage;
use std::fs;
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

const META_A: &str = "KeldCarrierA/0.1-gh476";
const META_B: &str = "KeldCarrierB/0.2-gh476";

fn stage_case(root: &Path, name: &str, label: &str, suffix: Option<&str>) -> DevBootStage {
    let project = root.join(label);
    fs::create_dir_all(project.join("src")).expect("owned first-party renderer project");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let api = serde_json::to_string(&repo.join("packages/@keld/api/src/index.ts"))
        .expect("public API path");
    // Spawn the real descendant before HELLO/Ready, so physical initial window
    // presentation cannot precede this fixture's actual Host/Guardian/Bun/cat.
    let role = format!(
        r"import {{ app }} from {api};
import {{ spawn }} from 'node:child_process';
import {{ once }} from 'node:events';
const descendant = spawn('/bin/cat', [], {{stdio:['pipe','ignore','ignore']}});
await once(descendant, 'spawn');
await app.whenReady();
"
    );
    bundle_public_api_entry(&project, repo, "gh476-cold-ua", &role);
    let title = serde_json::to_string(name).expect("literal title");
    let label_json = serde_json::to_string(label).expect("literal case");
    let expected = serde_json::to_string(&suffix).expect("independent page expectation");
    let all = serde_json::to_string(&[META_A, META_B]).expect("distinct generic metadata");
    let html = format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>{name}</title>
<style>body{{background:#111;color:#eee}}button{{font-size:20px;margin:24px}}</style></head>
<body><button id="result">GH476 Host UA pending</button><script>
const actual = navigator.userAgent;
const requested = {expected};
const known = {all};
const engine = actual.startsWith('Mozilla/') && actual.includes('AppleWebKit/');
const exact = requested === null ? !known.some(value => actual.includes(value)) : actual.split(' ').includes(requested);
const isolated = !known.some(value => value !== requested && actual.includes(value));
document.getElementById('result').textContent = 'GH476 Host UA ' + {label_json} + ((engine && exact && isolated) ? ' matched' : ' mismatch');
</script></body></html>
"#
    );
    fs::write(project.join("index.html"), html).expect("first-party native UA observer page");
    let metadata = suffix.map_or_else(String::new, |value| {
        format!(
            "applicationNameForUserAgent: {},",
            serde_json::to_string(value).expect("literal metadata")
        )
    });
    fs::write(project.join("keld.config.ts"), format!(
        "export default {{\n  name: {title},\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n  {metadata}\n}} as const;\n"
    )).expect("ordinary hello-template config, no special app selection");
    // The authored role spawns a real descendant. Select the existing Legacy
    // profile explicitly; UA metadata grants no OS or resource authority.
    fs::write(
        project.join("keld.permissions.jsonc"),
        b"{\"profile\":\"legacy\"}\n",
    )
    .expect("explicit Legacy fixture profile, no added grants");
    keld_cli::boot::stage_dev_boot(&project, host()).expect("ordinary shipping stage producer")
}

fn assert_staged_metadata(stage_root: &Path, name: &str, suffix: Option<&str>) {
    let descriptor: serde_json::Value = serde_json::from_slice(
        &fs::read(stage_root.join("keld.boot.json")).expect("actual staged descriptor"),
    )
    .expect("descriptor JSON");
    assert_eq!(descriptor["name"], name, "configured native window title");
    assert_eq!(
        descriptor["schema"], 3,
        "explicit Legacy renderer boot, no graph/preload gate"
    );
    assert_eq!(
        fs::read(stage_root.join("keld.permissions.jsonc")).expect("actual staged policy"),
        b"{\"profile\":\"legacy\"}\n",
        "explicit fixture profile survives staging exactly"
    );
    if let Some(value) = suffix {
        assert_eq!(descriptor["applicationNameForUserAgent"], value);
    } else {
        assert!(
            descriptor.get("applicationNameForUserAgent").is_none(),
            "default field stays absent"
        );
    }
}

fn observe_case(
    stage: DevBootStage,
    name: &str,
    label: &str,
    suffix: Option<&str>,
    ax: &Path,
    mut native: NativeWindowObserver,
) {
    let deadline = Instant::now() + EVENT_DEADLINE;
    let mut launch = FirstProofLaunch::start_prepared_captured(stage, deadline);
    let pid = launch.host_pid();
    let stage_root = launch.stage_root().to_owned();
    let _ = launch.cleanup.observe_family_before(deadline);
    assert_staged_metadata(&stage_root, name, suffix);
    assert_eq!(
        native
            .expect_initial(pid, "GH476 shipping Host native renderer")
            .len(),
        1
    );
    let family = launch.cleanup.observe_family_before(deadline);
    assert_eq!(
        family.len(),
        4,
        "actual Host/Guardian/Bun/cat exit witnesses"
    );
    let guardians = process_ids("ppid", pid);
    assert_eq!(guardians.len(), 1);
    let guardian = guardians[0];
    let groups = launch.cleanup.observed_group_ids();
    let expected = format!("GH476 Host UA {label} matched");
    let observer = Command::new(ax)
        .arg(pid.to_string())
        .arg(&expected)
        .arg("--observe")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("existing exact-PID AX observer");
    let output = wait_child_output(observer, deadline.saturating_duration_since(Instant::now()));
    assert!(
        output.status.success(),
        "actual Host cold UA missing {expected:?}: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("AX observation"),
        format!("REAL_HOST_AX_BUTTON_VISIBLE pid={pid} title={expected}\n")
    );
    assert!(stage_root.is_dir(), "live native window retains stage");
    drop(launch.lease.take());
    let output = launch
        .cleanup
        .wait(deadline.saturating_duration_since(Instant::now()));
    assert!(
        output.status.success(),
        "actual Host lease shutdown failed: status={:?},stdout_bytes={},stderr_bytes={}",
        output.status,
        output.stdout.len(),
        output.stderr.len()
    );
    for (target, exit) in launch.cleanup.take_observed_exits() {
        let status = exit.wait_before(deadline);
        if target == pid || target == guardian {
            assert!(status.success(), "Host/Guardian orderly exit: {status:?}");
        } else {
            assert!(
                status.success() || matches!(status.signal(), Some(15 | 9)),
                "owned Bun/descendant retirement: {status:?}"
            );
        }
    }
    for target in family {
        await_process_gone_before(target, deadline);
        assert!(
            session_dirs_for(target).is_empty(),
            "owned app-link session remains"
        );
    }
    for group in groups {
        assert!(
            process_ids("pgid", group).is_empty(),
            "owned group survived"
        );
    }
    assert!(
        native_windows(pid, name).is_empty(),
        "native window survived"
    );
    assert!(
        !stage_root.exists(),
        "actual Host must delete stage before disarm"
    );
    launch.complete();
}

#[test]
fn gh476_shipping_host_config_reaches_native_user_agent_default_a_b() {
    let root = tempfile::tempdir().expect("owned Host UA acceptance root");
    let ax = root.path().join("observe-actual-ua");
    let compiler = Command::new("/usr/bin/swiftc")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/macos_dialog_ax_press.swift"),
        )
        .arg("-o")
        .arg(&ax)
        .spawn()
        .expect("compile existing AX observer");
    let output = wait_child_output(compiler, EVENT_DEADLINE);
    assert!(output.status.success(), "AX observer compiler: {output:?}");
    let census: PathBuf = compile_native_window_census(root.path());
    for (name, label, suffix) in [
        ("Cold Framework Default", "default", None),
        ("Workshop Amber", "alpha", Some(META_A)),
        ("Comet Laboratory", "beta", Some(META_B)),
    ] {
        let native = NativeWindowObserver::arm_for_title(&census, name);
        let stage = stage_case(root.path(), name, label, suffix);
        observe_case(stage, name, label, suffix, &ax, native);
    }
}
