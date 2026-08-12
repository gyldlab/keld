//! CLI wiring for `keld create` / `keld dev` / `keld doctor` (KEL-29).

#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::Command;

use keld_cli::create::create_project;
use keld_cli::dev::run_dev_echo;

fn keld_bin() -> &'static str {
    env!("CARGO_BIN_EXE_keld")
}

fn keld() -> Command {
    Command::new(keld_bin())
}

fn empty_path_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("empty PATH dir")
}

fn output_text(out: &std::process::Output) -> (String, String) {
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn keld_create_writes_expected_contents() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = keld()
        .args(["create", "hello"])
        .current_dir(dir.path())
        .output()
        .expect("spawn create");
    let (stdout, stderr) = output_text(&out);
    assert!(
        out.status.success(),
        "create failed: stdout={stdout} stderr={stderr}"
    );
    assert!(stdout.contains("Created keld project"), "{stdout}");
    assert!(stdout.contains("cd hello && keld dev"), "{stdout}");

    let root = dir.path().join("hello");
    let config = fs::read_to_string(root.join("keld.config.ts")).expect("config");
    assert!(config.contains("name: \"hello\""), "{config}");
    assert!(!config.contains("{{name}}"), "{config}");
    let main = fs::read_to_string(root.join("src/main.ts")).expect("main");
    assert!(
        main.contains("hello: main process ready (IPC echo ok)"),
        "{main}"
    );
    assert!(main.contains("KELD-CLI-010"), "{main}");
}

#[test]
fn keld_create_rejects_empty_and_uppercase() {
    let dir = tempfile::tempdir().expect("tempdir");

    let missing = keld()
        .arg("create")
        .current_dir(dir.path())
        .output()
        .expect("spawn");
    assert!(!missing.status.success());
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(stderr.contains("KELD-CLI-020"), "{stderr}");
    assert!(stderr.contains("empty"), "{stderr}");

    let empty = keld()
        .args(["create", ""])
        .current_dir(dir.path())
        .output()
        .expect("spawn");
    assert!(!empty.status.success());
    let stderr = String::from_utf8_lossy(&empty.stderr);
    assert!(stderr.contains("KELD-CLI-020"), "{stderr}");
    assert!(!dir.path().join("keld.config.ts").exists());

    let upper = keld()
        .args(["create", "Hello"])
        .current_dir(dir.path())
        .output()
        .expect("spawn");
    assert!(!upper.status.success());
    let stderr = String::from_utf8_lossy(&upper.stderr);
    assert!(stderr.contains("KELD-CLI-020"), "{stderr}");
    assert!(stderr.contains("Hello"), "{stderr}");
    assert!(!dir.path().join("Hello").exists());
    assert!(!dir.path().join("hello").exists());
}

#[test]
fn keld_create_rejects_invalid_path_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = keld()
        .args(["create", "foo/bar"])
        .current_dir(dir.path())
        .output()
        .expect("spawn");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("KELD-CLI-020"), "{stderr}");
    assert!(stderr.contains("foo/bar"), "{stderr}");
    assert!(!dir.path().join("foo").exists());
}

#[test]
fn keld_doctor_ok_in_created_project() {
    let dir = tempfile::tempdir().expect("tempdir");
    let created = keld()
        .args(["create", "hello"])
        .current_dir(dir.path())
        .output()
        .expect("create");
    assert!(created.status.success(), "create must succeed first");

    let out = keld()
        .arg("doctor")
        .current_dir(dir.path().join("hello"))
        .output()
        .expect("doctor");
    let (stdout, stderr) = output_text(&out);
    assert!(
        out.status.success(),
        "doctor should pass in a scaffolded project: stdout={stdout} stderr={stderr}"
    );
    assert!(stdout.contains("[ok] bun"), "{stdout}");
    assert!(stdout.contains("[ok] project"), "{stdout}");
    assert!(
        stdout.contains("hello"),
        "project detail should name the app dir: {stdout}"
    );
    assert!(!stdout.contains("[FAIL]"), "{stdout}");
}

#[test]
fn keld_doctor_fails_when_main_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
    let out = keld()
        .arg("doctor")
        .current_dir(dir.path())
        .output()
        .expect("doctor");
    assert!(!out.status.success(), "half-scaffolded project must fail");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[FAIL] project"), "{stdout}");
    assert!(stdout.contains("src/main.ts"), "{stdout}");
}

#[test]
fn keld_doctor_fails_when_bun_missing_from_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "hello").expect("create");
    let empty = empty_path_dir();
    let mut cmd = keld();
    cmd.arg("doctor")
        .current_dir(dir.path().join("hello"))
        .env("PATH", empty.path())
        .env("Path", empty.path());
    let out = cmd.output().expect("doctor");
    assert!(
        !out.status.success(),
        "doctor must fail when bun is not on PATH"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(combined.contains("[FAIL] bun"), "{combined}");
    assert!(combined.contains("bun.sh"), "{combined}");
    assert!(
        !combined.contains("[ok] bun"),
        "must not report bun ok without a binary: {combined}"
    );
}

#[test]
fn keld_dev_without_config_is_cli_032() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = keld()
        .arg("dev")
        .current_dir(dir.path())
        .output()
        .expect("dev");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("KELD-CLI-032"), "{stderr}");
    assert!(stderr.contains("keld.config.ts"), "{stderr}");
    assert!(stderr.contains("[FAIL] project"), "{stderr}");
}

#[test]
fn run_dev_echo_uses_project_name_and_reaps_socket() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("t{}", std::process::id());
    let root = create_project(dir.path(), &name).expect("create");
    let result = run_dev_echo(&root, Path::new(keld_bin())).expect("dev echo");
    assert!(
        result
            .stdout
            .contains(&format!("{name}: main process ready (IPC echo ok)")),
        "stdout={}",
        result.stdout
    );
    assert!(
        result
            .stdout
            .contains("ipc-echo ok: message=\"keld\" count=1"),
        "stdout={}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("{{name}}"),
        "template must be substituted: {}",
        result.stdout
    );
    #[cfg(unix)]
    {
        assert!(
            !Path::new(&result.link).exists(),
            "echo socket must be removed on teardown: {}",
            result.link
        );
    }
}
