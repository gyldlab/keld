//! GH-569: inspect an early dependency in the real staged, supervised Bun role.
//! This records process facts only; it is not an installed app or workflow claim.

use std::fs;
use std::io::{BufReader, Write as _};
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::support::EVENT_DEADLINE;
use crate::support::control::{accept_before, parse_pid, read_control_line, wait_child_output};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::process::{await_process_gone, process_group};
use crate::support::product::ProductFixture;

#[test]
fn shipping_role_early_dependency_reports_process_facts() {
    let fixture = ProductFixture::new("gh569-role");
    let stage = stage_fixture(&fixture);
    let listener =
        UnixListener::bind(fixture.root.path().join("gh569.sock")).expect("bind owned observer");
    listener
        .set_nonblocking(true)
        .expect("nonblocking observer");
    let child = Command::new(stage.host())
        .process_group(0)
        .current_dir(stage.root())
        .env("KELD_T1B_CONTROL", fixture.root.path().join("gh569.sock"))
        .env("KELD_T4_SKIP_DESCENDANT", "1")
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T2_EXIT_ON_LINK_EOF", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch current host");
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let host = cleanup.cli.as_mut().expect("live host");
    let host_pid = host.id();
    let _lease = host.stdin.take().expect("retain dev lease");
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bound control reads");
    let mut reader = BufReader::new(control.try_clone().expect("clone observer"));
    let mut writer = control;
    let hello = read_control_line(&mut reader);
    let parts: Vec<_> = hello.split_whitespace().collect();
    assert_eq!(parts.first().copied(), Some("HELLO"), "{hello}");
    let bun_pid = parse_pid(parts.get(1).copied(), &hello);
    cleanup.record_authenticated_groups(process_group(host_pid), process_group(bun_pid));
    assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
    let line = read_control_line(&mut reader);
    let payload = line
        .strip_prefix("GH569 ")
        .expect("early dependency marker");
    let observed: serde_json::Value = serde_json::from_str(payload).expect("early facts JSON");
    eprintln!("GH569 early role observation: {observed}");
    assert_eq!(
        observed["facadeAfter"], "browser",
        "real Keld facade imported after early dependency"
    );
    validate_fact_tags(&observed);
    let argv = observed["argv"].as_array().expect("actual role argv array");
    assert!(
        argv.iter().all(serde_json::Value::is_string),
        "role argv entries must be strings"
    );
    let current_argv = observed["liveArgv"].as_array().expect("live role argv");
    assert_eq!(
        current_argv.len(),
        argv.len() + 1,
        "late mutation changes only live argv"
    );
    assert_eq!(
        &current_argv[..argv.len()],
        argv.as_slice(),
        "early argv is an independent snapshot"
    );
    assert_eq!(
        current_argv.last().and_then(serde_json::Value::as_str),
        Some("gh569-late-mutation")
    );
    assert!(
        !argv.iter().any(|value| value == "gh569-late-mutation"),
        "late value leaked into early snapshot"
    );
    assert_eq!(read_control_line(&mut reader), "READY");
    assert_eq!(read_control_line(&mut reader), "ECHO1");
    assert_eq!(read_control_line(&mut reader), "ECHO2");
    writer
        .write_all(b"QUIT\n")
        .expect("request authenticated quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    drop(writer);
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    assert!(output.status.success(), "host failed: {output:?}");
    await_process_gone(bun_pid);
    await_process_gone(host_pid);
}

fn validate_fact_tags(observed: &serde_json::Value) {
    for name in [
        "type",
        "defaultApp",
        "mas",
        "windowsStore",
        "sandboxed",
        "contextIsolated",
        "resourcesPath",
        "electron",
    ] {
        let tagged = observed[name].as_object().expect("tagged role fact");
        let kind = tagged
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .expect("fact kind");
        if kind == "undefined" {
            assert_eq!(tagged.len(), 1, "undefined fact carries no value: {name}");
        } else {
            assert!(
                matches!(kind, "boolean" | "string" | "number" | "object"),
                "unrecognized fact kind: {name}"
            );
            let value = tagged.get("value").expect("defined fact carries a value");
            let valid = match kind {
                "boolean" => value.is_boolean(),
                "string" => value.is_string(),
                "number" => value.is_number(),
                "object" => value.is_object() || value.is_null(),
                _ => false,
            };
            assert!(
                valid && tagged.len() == 2,
                "fact kind/value mismatch: {name}"
            );
        }
    }
}

fn stage_fixture(fixture: &ProductFixture) -> keld_cli::boot::DevBootStage {
    let source_dir = fixture.project.join("src");
    let dependency = fixture.project.join("node_modules/gh569-early");
    fs::create_dir_all(&dependency).expect("create early dependency package");
    fs::write(
        dependency.join("package.json"),
        r#"{"name":"gh569-early","main":"index.ts"}"#,
    )
    .expect("write dependency package manifest");
    fs::write(
        dependency.join("index.ts"),
        include_str!("../../fixtures/module_process_early.ts"),
    )
    .expect("write early dependency");
    let ready = "await sendControl(\"READY\");";
    assert_eq!(
        fixture.entry_source.matches(ready).count(),
        1,
        "unique lifecycle ready marker"
    );
    let instrumented = fixture.entry_source.replacen(
        ready,
        "await sendControl(`GH569 ${JSON.stringify({...gh569Early, facadeAfter: process.type, liveArgv: process.argv})}`);\nawait sendControl(\"READY\");",
        1,
    );
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let facade_path = serde_json::to_string(
        &repo
            .join("packages/@keld/electron/src/index.ts")
            .to_string_lossy(),
    )
    .expect("encode real facade path");
    // Runtime-variable dynamic import keeps the real facade and canonical transport
    // external to this fixture bundle; it does not exercise a transport Worker.
    let wrapper = format!(
        "import {{ gh569Early }} from 'gh569-early';\nconst gh569FacadePath = {facade_path};\nconst gh569Facade = await import(gh569FacadePath);\nif (!gh569Facade.default?.app) throw new Error('missing real Keld facade app');\nprocess.argv.push('gh569-late-mutation');\n{instrumented}"
    );
    let source = source_dir.join("module-process-entry.ts");
    let bundle = source_dir.join("main.ts");
    fs::write(&source, wrapper).expect("write early-import entry");
    let build = Command::new("bun")
        .arg("build")
        .arg(&source)
        .args(["--target=bun", "--format=esm", "--outfile"])
        .arg(&bundle)
        .output()
        .expect("bundle role fixture");
    assert!(build.status.success(), "fixture bundling failed: {build:?}");
    let bundled = fs::read_to_string(&bundle).expect("read fixture bundle");
    assert!(
        bundled.contains("import(gh569FacadePath)"),
        "facade import must remain external to fixture bundle"
    );
    fs::write(fixture.project.join("keld.config.ts"),
        "export default { name: 'GH569 role', entry: 'src/main.ts', initialWindow: 'none' } as const;\n")
        .expect("rendererless fixture config");
    keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
        .expect("stage current shipping host and entry")
}
