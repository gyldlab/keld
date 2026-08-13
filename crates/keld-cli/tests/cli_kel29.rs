//! CLI wiring for `keld create` / `keld dev` / `keld doctor` (KEL-29).

#![allow(clippy::expect_used)] // extra test crate: expect is the assertion oracle

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use keld_cli::create::create_project;
use keld_cli::dev::{load_dev_window_html, run_dev_echo};

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
    let html = fs::read_to_string(root.join("index.html")).expect("html");
    assert!(html.contains("<title>hello</title>"), "{html}");
    let loaded = load_dev_window_html(&root).expect("dev renderer");
    assert_eq!(loaded, html, "keld dev must load the created index.html");
    assert!(!loaded.contains("Hello from WKWebView"));
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
    assert!(stdout.contains("[ok] renderer"), "{stdout}");
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
fn keld_doctor_fails_when_renderer_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("keld.config.ts"), "export default {}\n").expect("config");
    fs::create_dir_all(dir.path().join("src")).expect("src");
    fs::write(dir.path().join("src/main.ts"), "export {}\n").expect("main");
    let out = keld()
        .arg("doctor")
        .current_dir(dir.path())
        .output()
        .expect("doctor");
    assert!(!out.status.success(), "missing index.html must fail doctor");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[ok] project"), "{stdout}");
    assert!(stdout.contains("[FAIL] renderer"), "{stdout}");
    assert!(stdout.contains("KELD-CLI-035"), "{stdout}");
    assert!(stdout.contains("index.html"), "{stdout}");
    assert!(
        !stdout.contains("[FAIL] project"),
        "config+main is complete; only renderer is missing: {stdout}"
    );
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

#[test]
fn run_dev_echo_bun_nonzero_without_connect_does_not_hang() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = create_project(dir.path(), "app").expect("create");
    fs::write(root.join("src/main.ts"), "process.exit(7);\n").expect("overwrite main");

    let (done_tx, done_rx) = mpsc::channel();
    let keld_bin = keld_bin().to_owned();
    thread::spawn(move || {
        let result = run_dev_echo(&root, Path::new(&keld_bin));
        let _ = done_tx.send(result);
    });
    let result = done_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("bun non-zero without a socket connect must not block on accept()");
    let err = result.expect_err("bun exit 7 must fail the session");
    let msg = err.to_string();
    assert!(msg.contains("KELD-CLI-031"), "{msg}");
    assert!(msg.contains('7'), "{msg}");
}

#[test]
fn keld_doctor_unknown_flags_exit_2_without_running_checks() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "hello").expect("create");
    let project = dir.path().join("hello");

    for flag in ["--permissions", "--web-compat", "--attack", "--bogus"] {
        let out = keld()
            .args(["doctor", flag])
            .current_dir(&project)
            .output()
            .expect("doctor");
        assert_eq!(
            out.status.code(),
            Some(2),
            "{flag}: misuse is exit 2, got {:?}: stderr={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stderr.contains("KELD-CLI-044"), "{flag}: {stderr}");
        assert!(stderr.contains(flag), "{flag}: {stderr}");
        assert!(
            stdout.is_empty(),
            "{flag}: unknown flags must not print doctor output: {stdout}"
        );
        assert!(
            !stderr.contains("[ok] bun"),
            "{flag}: checks must not run: {stderr}"
        );
    }
}

#[test]
fn keld_doctor_json_unknown_flag_exits_2_without_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "hello").expect("create");
    let out = keld()
        .args(["doctor", "--json", "--permissions"])
        .current_dir(dir.path().join("hello"))
        .output()
        .expect("doctor");
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(stderr.contains("--permissions"), "{stderr}");
    assert!(
        serde_json::from_str::<serde_json::Value>(stdout.trim()).is_err(),
        "must not emit findings JSON after unknown flag: {stdout}"
    );
}

#[test]
fn keld_hello_unknown_flag_exits_2_without_opening_window() {
    let (done_tx, done_rx) = mpsc::channel();
    let bin = keld_bin().to_owned();
    thread::spawn(move || {
        let out = Command::new(bin).args(["hello", "--devtools"]).output();
        let _ = done_tx.send(out);
    });
    let out = done_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("keld hello --devtools must reject the flag without opening a window")
        .expect("spawn hello");
    assert_eq!(
        out.status.code(),
        Some(2),
        "misuse is exit 2, got {:?}: stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(stderr.contains("--devtools"), "{stderr}");
    assert!(stderr.contains("takes no flags"), "{stderr}");
}

#[test]
fn keld_create_template_flag_exits_2_without_scaffolding() {
    let dir = tempfile::tempdir().expect("tempdir");

    let as_name = keld()
        .args(["create", "--template"])
        .current_dir(dir.path())
        .output()
        .expect("create --template");
    assert_eq!(as_name.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&as_name.stderr);
    let stdout = String::from_utf8_lossy(&as_name.stdout);
    assert!(stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(stderr.contains("--template"), "{stderr}");
    assert!(stderr.contains("not implemented"), "{stderr}");
    assert!(
        !stderr.contains("KELD-CLI-020"),
        "flag must not be parsed as a project name: {stderr}"
    );
    assert!(stdout.is_empty(), "{stdout}");
    assert!(
        !dir.path().join("--template").exists(),
        "must not scaffold a directory named --template"
    );

    let extra = keld()
        .args(["create", "hello", "--template", "react"])
        .current_dir(dir.path())
        .output()
        .expect("create hello --template");
    assert_eq!(extra.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&extra.stderr);
    assert!(stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(stderr.contains("--template"), "{stderr}");
    assert!(
        !dir.path().join("hello").exists(),
        "`keld create hello --template` must not scaffold vanilla hello"
    );

    let positional = keld()
        .args(["create", "hello", "world"])
        .current_dir(dir.path())
        .output()
        .expect("create extra positional");
    assert_eq!(positional.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&positional.stderr);
    assert!(stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(stderr.contains("`world`"), "{stderr}");
    assert!(!dir.path().join("hello").exists());
}

#[test]
fn keld_dev_unknown_flag_exits_2_without_opening_window() {
    let dir = tempfile::tempdir().expect("tempdir");
    create_project(dir.path(), "hello").expect("create");
    let project = dir.path().join("hello");

    let (done_tx, done_rx) = mpsc::channel();
    let bin = keld_bin().to_owned();
    thread::spawn(move || {
        let out = Command::new(bin)
            .args(["dev", "--watch"])
            .current_dir(&project)
            .output();
        let _ = done_tx.send(out);
    });
    let out = done_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("keld dev --watch must reject the flag without opening a window")
        .expect("spawn dev");
    assert_eq!(
        out.status.code(),
        Some(2),
        "misuse is exit 2, got {:?}: stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stderr.contains("KELD-CLI-044"), "{stderr}");
    assert!(stderr.contains("--watch"), "{stderr}");
    assert!(stderr.contains("takes no flags"), "{stderr}");
    assert!(
        stdout.is_empty(),
        "unknown flags must not start echo or print doctor output: {stdout}"
    );
    assert!(
        !stderr.contains("[ok] bun"),
        "doctor checks must not run: {stderr}"
    );
}

#[test]
fn keld_reserved_verbs_exit_2_with_045_and_do_not_scaffold() {
    let dir = tempfile::tempdir().expect("tempdir");

    for (verb, tracking) in [
        ("migrate", "KEL-17"),
        ("build", "KEL-19"),
        ("gen", "KEL-13"),
        ("ext", "KEL-19"),
    ] {
        let out = keld()
            .args([verb, "hello"])
            .current_dir(dir.path())
            .output()
            .expect(verb);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{verb}: misuse is exit 2, got {:?}: stderr={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stderr.contains("KELD-CLI-045"), "{verb}: {stderr}");
        assert!(stderr.contains(&format!("keld {verb}")), "{verb}: {stderr}");
        assert!(stderr.contains("not implemented"), "{verb}: {stderr}");
        assert!(stderr.contains(tracking), "{verb}: {stderr}");
        assert!(
            !stderr.contains("unknown command"),
            "{verb}: must not look like a missing dispatch arm: {stderr}"
        );
        assert!(
            stdout.is_empty(),
            "{verb}: reserved verbs must not print to stdout: {stdout}"
        );
        assert!(
            !dir.path().join("hello").exists(),
            "{verb} hello must not scaffold a project"
        );
    }
}

#[test]
fn keld_unknown_verb_exits_2_with_046() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = keld()
        .arg("bogus")
        .current_dir(dir.path())
        .output()
        .expect("bogus");
    assert_eq!(
        out.status.code(),
        Some(2),
        "misuse is exit 2, got {:?}: stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stderr.contains("KELD-CLI-046"), "{stderr}");
    assert!(stderr.contains("`bogus`"), "{stderr}");
    assert!(stderr.contains("create"), "{stderr}");
    assert!(!stderr.contains("KELD-CLI-045"), "{stderr}");
    assert!(
        stdout.is_empty(),
        "unknown verb must not print to stdout: {stdout}"
    );
}

#[test]
fn keld_no_args_still_exits_0_and_lists_reserved() {
    let out = keld().output().expect("no args");
    assert!(out.status.success(), "no-args usage is exit 0");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("reserved (not live)"), "{stderr}");
    assert!(stderr.contains("migrate"), "{stderr}");
}
