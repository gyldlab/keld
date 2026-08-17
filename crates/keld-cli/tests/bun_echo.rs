//! Dev session integration: Bun main + kipc echo (no window).

#![allow(clippy::expect_used)] // extra test crate: expect is the assertion oracle

use std::path::Path;
use std::process::Command;
use std::sync::mpsc;

use keld_cli::create::create_project;
use keld_cli::echo_link::EchoServer;

#[test]
fn bun_main_runs_ipc_echo_with_unique_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main_ts = dir.path().join("main.ts");
    std::fs::write(
        &main_ts,
        r#"
const link = process.env.KELD_APP_LINK;
if (!link) {
  console.error("KELD-CLI-010: KELD_APP_LINK is unset");
  process.exit(1);
}
const keld = process.env.KELD_BIN ?? "keld";
const message = process.env.KELD_ECHO_MESSAGE;
const count = process.env.KELD_ECHO_COUNT;
if (!message || !count) {
  console.error("KELD-CLI-011: KELD_ECHO_MESSAGE/COUNT unset");
  process.exit(1);
}
const proc = Bun.spawn(
  [keld, "ipc-client", "echo", "--link", link, "--message", message, "--count", count],
  { stdout: "inherit", stderr: "inherit", env: process.env },
);
const code = await proc.exited;
if (code !== 0) process.exit(code);
console.log("kel30: main process ready (IPC echo ok)");
"#,
    )
    .expect("write main.ts");

    let (ready_tx, ready_rx) = mpsc::channel();
    let server = EchoServer::start(&ready_tx).expect("bind echo server");
    ready_rx.recv().expect("server ready");
    let link = server.link();

    let unique = format!("kel30-{}", std::process::id());
    let keld_bin = env!("CARGO_BIN_EXE_keld");
    let output = Command::new("bun")
        .arg("run")
        .arg(&main_ts)
        .env("KELD_APP_LINK", &link)
        .env("KELD_BIN", keld_bin)
        .env("KELD_ECHO_MESSAGE", &unique)
        .env("KELD_ECHO_COUNT", "9")
        .output()
        .expect("spawn bun");

    server.join().expect("server join");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "bun failed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains(&format!("ipc-echo ok: message={unique:?} count=9")),
        "missing typed echo fields in stdout={stdout}"
    );
    assert!(
        stdout.contains("main process ready"),
        "missing ready line in stdout={stdout}"
    );
    assert!(
        !stdout.contains("message=\"keld\" count=1"),
        "must not print the hardcoded demo payload: {stdout}"
    );
}

#[test]
fn ipc_echo_binary_prints_demo_fields() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .arg("ipc-echo")
        .output()
        .expect("spawn ipc-echo");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "ipc-echo failed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("ipc-echo ok: message=\"keld\" count=1"),
        "stdout={stdout}"
    );
}

#[test]
fn ipc_client_missing_link_is_cli_040() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .args(["ipc-client", "echo"])
        .output()
        .expect("spawn");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KELD-CLI-040"), "{stderr}");
    assert!(stderr.contains("--link"), "{stderr}");
}

#[test]
fn ipc_client_link_flag_without_value_is_cli_040() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .args(["ipc-client", "echo", "--link"])
        .output()
        .expect("spawn");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KELD-CLI-040"), "{stderr}");
    assert!(stderr.contains("requires a value"), "{stderr}");
}

#[test]
fn ipc_client_link_without_token_is_ipc_007() {
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .args([
            "ipc-client",
            "echo",
            "--link",
            "/no/such/keld-echo-kel30.sock",
        ])
        .output()
        .expect("spawn");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KELD-IPC-007"), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("ipc-echo ok"),
        "must not print a fabricated reply"
    );
}

#[test]
fn ipc_client_missing_socket_is_ipc_001() {
    let hex = "11".repeat(32);
    #[cfg(unix)]
    let link = format!("/no/such/keld-echo-kel30.sock#{hex}");
    #[cfg(windows)]
    let link = format!("1#{hex}");
    let output = Command::new(env!("CARGO_BIN_EXE_keld"))
        .args([
            "ipc-client",
            "echo",
            "--link",
            &link,
            "--message",
            "should-not-echo",
            "--count",
            "3",
        ])
        .output()
        .expect("spawn");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KELD-IPC-001"), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("should-not-echo"),
        "must not print a fabricated reply"
    );
}

#[test]
fn created_template_main_runs_ipc_echo() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    let project = dir.path().join("app");

    let (ready_tx, ready_rx) = mpsc::channel();
    let server = EchoServer::start(&ready_tx).expect("bind echo server");
    ready_rx.recv().expect("server ready");
    let link = server.link();

    let keld_bin = env!("CARGO_BIN_EXE_keld");
    let output = Command::new("bun")
        .arg("run")
        .arg("src/main.ts")
        .current_dir(&project)
        .env("KELD_APP_LINK", &link)
        .env("KELD_BIN", keld_bin)
        .output()
        .expect("spawn bun");

    server.join().expect("server join");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "template bun failed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("ipc-echo ok: message=\"keld\" count=1"),
        "template must speak kipc echo: stdout={stdout}"
    );
    assert!(
        stdout.contains("app: main process ready (IPC echo ok)"),
        "substituted project name missing: stdout={stdout}"
    );
    assert!(
        !stdout.contains("{{name}}"),
        "unsubstituted template leaked: stdout={stdout}"
    );
}

#[test]
fn kipc_ts_golden_vectors_pass_under_bun_test() {
    let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/hello");
    let output = Command::new("bun")
        .args(["test", "src/kipc.test.ts"])
        .current_dir(&hello)
        .output()
        .expect("bun must be on PATH (same contract as bun_echo)");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "kipc.ts golden vectors failed: stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn created_template_session_two_echoes_on_one_connection() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "app").expect("create");
    let project = dir.path().join("app");
    std::fs::write(
        project.join("src/two.ts"),
        r#"
import { AppLinkSession } from "./kipc";

const link = process.env.KELD_APP_LINK;
if (!link) {
  console.error("KELD-CLI-010: KELD_APP_LINK is unset");
  process.exit(1);
}
const firstMsg = process.env.KELD_ECHO_A;
const secondMsg = process.env.KELD_ECHO_B;
if (!firstMsg || !secondMsg) {
  console.error("KELD-CLI-011: KELD_ECHO_A/B unset");
  process.exit(1);
}

const session = await AppLinkSession.connect(link);
try {
  const first = await session.echo({ message: firstMsg, count: 2 });
  const second = await session.echo({ message: secondMsg, count: 9 });
  console.log(`first=${JSON.stringify(first.message)}:${first.count}`);
  console.log(`second=${JSON.stringify(second.message)}:${second.count}`);
} finally {
  session.close();
}
"#,
    )
    .expect("write two.ts");

    let (ready_tx, ready_rx) = mpsc::channel();
    let server = EchoServer::start(&ready_tx).expect("bind echo server");
    ready_rx.recv().expect("server ready");
    let link = server.link();

    let unique_a = format!("kel30-a-{}", std::process::id());
    let unique_b = format!("kel30-b-{}", std::process::id());
    let output = Command::new("bun")
        .arg("run")
        .arg("src/two.ts")
        .current_dir(&project)
        .env("KELD_APP_LINK", &link)
        .env("KELD_ECHO_A", &unique_a)
        .env("KELD_ECHO_B", &unique_b)
        .output()
        .expect("spawn bun");

    server.join().expect("server join");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "two-call session failed: stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains(&format!("first={unique_a:?}:2")),
        "missing first unique echo: stdout={stdout}"
    );
    assert!(
        stdout.contains(&format!("second={unique_b:?}:9")),
        "missing second unique echo: stdout={stdout}"
    );
    assert_ne!(unique_a, unique_b);
    assert!(
        !stdout.contains("message=\"keld\" count=1"),
        "must not print the hardcoded demo payload: {stdout}"
    );
}
