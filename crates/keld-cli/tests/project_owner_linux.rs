//! Real Linux project-owner boundary proof (KEL-167).

#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::too_many_lines)] // fixture/setup failures are test assertions; one test keeps account cleanup and product oracles contiguous

use std::fs;
use std::io::Read as _;
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const PROOF_ENV: &str = "KELD_REAL_LINUX_OWNER_PROOF";

struct ForeignAccount {
    name: String,
    fixture_root: PathBuf,
    invoking_uid: u32,
    invoking_gid: u32,
    active: bool,
}

impl ForeignAccount {
    fn cleanup(&mut self) -> Result<(), String> {
        if !self.active {
            return Ok(());
        }
        checked_command(
            Command::new("sudo").args([
                "chown",
                "-R",
                &format!("{}:{}", self.invoking_uid, self.invoking_gid),
                &self.fixture_root.display().to_string(),
            ]),
            "restore fixture ownership",
        )?;
        checked_command(
            Command::new("sudo").args(["userdel", &self.name]),
            "delete foreign fixture account",
        )?;
        self.active = false;
        Ok(())
    }
}

impl Drop for ForeignAccount {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn checked_command(command: &mut Command, label: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("{label}: could not start command: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{label}: status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn command_output(command: &mut Command, label: &str) -> Output {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("{label}: could not start command: {error}"));
    let pid =
        rustix::process::Pid::from_raw(i32::try_from(child.id()).expect("child pid fits i32"))
            .expect("child pid is nonzero");
    let mut stdout = child.stdout.take().expect("captured child stdout");
    let mut stderr = child.stderr.take().expect("captured child stderr");
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::yield_now(),
            Ok(None) => {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
                let _ = child.kill();
                let status = child.wait().expect("reap timed-out child");
                let stdout = stdout_reader
                    .join()
                    .expect("join stdout reader")
                    .expect("read timed-out child stdout");
                let stderr = stderr_reader
                    .join()
                    .expect("join stderr reader")
                    .expect("read timed-out child stderr");
                panic!(
                    "{label}: exceeded 10s and was reaped with {status}; stdout={} stderr={}",
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                );
            }
            Err(error) => panic!("{label}: could not poll child: {error}"),
        }
    };
    Output {
        status,
        stdout: stdout_reader
            .join()
            .expect("join stdout reader")
            .expect("read child stdout"),
        stderr: stderr_reader
            .join()
            .expect("join stderr reader")
            .expect("read child stderr"),
    }
}

fn assert_cli_refusal(binary: &Path, cwd: &Path, project: &Path, arguments: &[&str]) {
    let output = command_output(
        Command::new(binary).args(arguments).current_dir(cwd),
        "run shipping CLI against foreign-owned project",
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KELD-CLI-049"), "{stderr}");
    assert!(stderr.contains(&project.display().to_string()), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("keld project at"),
        "foreign project was reported as adopted: {output:?}"
    );
}

#[test]
#[ignore = "requires a real Linux x86_64 runner with passwordless sudo"]
fn real_linux_second_account_refuses_foreign_owned_project() {
    assert_eq!(std::env::var(PROOF_ENV).as_deref(), Ok("1"));
    assert_eq!(std::env::consts::ARCH, "x86_64");

    let fixture = tempfile::tempdir().expect("owner-proof fixture root");
    let project = fixture.path().join("foreign-project");
    let victim = project.join("victim");
    let source = project.join("src/main.ts");
    let renderer = project.join("index.html");
    let config = project.join("keld.config.ts");
    let marker = fixture.path().join("foreign-entry-executed");
    fs::create_dir_all(&victim).expect("victim cwd");
    fs::create_dir_all(project.join("src")).expect("project source directory");
    fs::write(
        &config,
        "export default { entry: \"src/main.ts\", renderer: \"index.html\" } as const;\n",
    )
    .expect("foreign config");
    fs::write(
        &source,
        format!("await Bun.write({marker:?}, \"executed\\n\");\n"),
    )
    .expect("foreign entry");
    fs::write(&renderer, "<!doctype html><title>foreign owner</title>\n")
        .expect("foreign renderer");

    let invoking_principal = (
        rustix::process::geteuid().as_raw(),
        rustix::process::getegid().as_raw(),
    );
    let account_name = format!("keld167{}", std::process::id());
    checked_command(
        Command::new("sudo").args([
            "useradd",
            "--no-create-home",
            "--shell",
            "/usr/sbin/nologin",
            &account_name,
        ]),
        "create foreign fixture account",
    )
    .expect("real second unprivileged account");
    let mut account = ForeignAccount {
        name: account_name.clone(),
        fixture_root: fixture.path().to_owned(),
        invoking_uid: invoking_principal.0,
        invoking_gid: invoking_principal.1,
        active: true,
    };
    let uid_output = command_output(
        Command::new("id").args(["-u", &account_name]),
        "read foreign fixture uid",
    );
    assert!(uid_output.status.success(), "{uid_output:?}");
    let foreign_uid = String::from_utf8(uid_output.stdout)
        .expect("foreign uid UTF-8")
        .trim()
        .parse::<u32>()
        .expect("foreign numeric uid");
    assert_ne!(foreign_uid, invoking_principal.0);

    for path in [&project, &config, &source, &renderer] {
        checked_command(
            Command::new("sudo").args([
                "chown",
                &foreign_uid.to_string(),
                &path.display().to_string(),
            ]),
            "assign foreign fixture ownership",
        )
        .expect("foreign path owner");
        assert_eq!(
            fs::metadata(path).expect("foreign path metadata").uid(),
            foreign_uid
        );
    }
    assert_eq!(
        fs::metadata(&victim).expect("victim metadata").uid(),
        invoking_principal.0
    );

    let binary = Path::new(env!("CARGO_BIN_EXE_keld"));
    assert_cli_refusal(binary, &victim, &project, &["doctor", "--json"]);
    assert_cli_refusal(binary, &victim, &project, &["dev"]);
    assert_cli_refusal(binary, &victim, &project, &["mcp", "serve"]);

    let stage_error = keld_cli::boot::stage_dev_boot(&project, Path::new("unreached-host"))
        .expect_err("direct staging must reject the real foreign uid before host lookup");
    assert!(stage_error.to_string().contains("KELD-CLI-049"));
    assert!(!project.join(".keld").exists());
    assert!(
        !marker.exists(),
        "foreign entry executed as the invoking user"
    );
    println!(
        "KELD_LINUX_PROJECT_OWNER invoking_uid={} foreign_uid={foreign_uid} \
         doctor=refused dev=refused mcp=refused stage=refused marker=absent",
        invoking_principal.0
    );

    account
        .cleanup()
        .expect("remove real foreign account fixture");
    let deleted = command_output(
        Command::new("id").args(["-u", &account_name]),
        "verify foreign fixture account deletion",
    );
    assert!(
        !deleted.status.success(),
        "foreign account survived cleanup"
    );
}
