//! keld-host — the shipping host binary.
//!
//! The host owns framework-controlled OS resources and dispatches into validated
//! library owners. Normative boundaries are in architecture 01/06; repository maturity
//! and evidence live in `docs/engineering/product-status.tsv`.

use std::env;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::fs;
#[cfg(target_os = "macos")]
use std::fs::File;
#[cfg(target_os = "macos")]
use std::io;
use std::io::Write as _;
#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::path::PathBuf;
use std::process;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

#[cfg(windows)]
const WINDOWS_DEV_STAGE_CLEANUP_ARG: &str = "--keld-windows-dev-stage-cleanup-v1";

fn main() {
    let native_args: Vec<std::ffi::OsString> = env::args_os().collect();
    // Private Windows roles consume the untouched native argv first.
    #[cfg(windows)]
    if handle_windows_private_role(&native_args) {
        return;
    }
    // Native argv may be non-Unicode on every OS. Refuse it before boot
    // selection or any WebView/OS authority instead of panicking in args().
    let args = keld_core::unicode_args(native_args).unwrap_or_else(|error| {
        eprintln!("{error}");
        process::exit(2);
    });
    #[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
    if run_macos_profile_fixture_role(&args) {
        return;
    }
    #[cfg(target_os = "macos")]
    if args.get(1).map(String::as_str)
        == Some(keld_runtime::macos_guardian::SUPERVISED_GUARDIAN_ARG)
    {
        // The argv value is only a role discriminator. `run_guarded_primary` must
        // authenticate GuardianBootstrap's one-use registration and validate
        // the inherited host-liveness pipe before it evaluates this process's
        // root/entry command factory or can spawn Bun.
        if let Err(error) = run_supervised_guardian(&args) {
            eprintln!("{error}");
            process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--hello") {
        if let Err(error) = keld_core::prepare_webview_process() {
            eprintln!("{error}");
            process::exit(1);
        }
        if let Some(flag) = keld_core::host_hello_unknown_arg(&args) {
            eprintln!(
                "KELD-CLI-044: unknown hello flag `{flag}`. \
                 Use `--hello` and optional `--title <name>`."
            );
            process::exit(2);
        }
        let cwd = env::current_dir().ok();
        let title = keld_core::resolve_hello_title(&args, cwd.as_deref());
        if let Err(err) = keld_core::run_hello_window_titled(&title) {
            eprintln!("{err}");
            process::exit(1);
        }
        return;
    }
    if args.len() != 1 {
        eprintln!(
            "KELD-CLI-044: unknown host argument. Launch the staged app with no arguments, or use `--hello` for the diagnostic window."
        );
        process::exit(2);
    }

    #[cfg(windows)]
    if keld_runtime::windows_job::launcher_start_gate_requested()
        && let Err(error) = keld_runtime::windows_job::accept_host_start_v1()
    {
        eprintln!("{error}");
        process::exit(1);
    }

    // Keep the Job until exit (KEL-53 §4); S6c owns the committed release_for_exit call.
    #[cfg(windows)]
    let _host_death_job = match keld_runtime::windows_job::install_host_death_job() {
        Ok(host_death_job) => host_death_job,
        Err(error) => {
            eprintln!("{error}");
            process::exit(1);
        }
    };

    if let Err(error) = keld_core::prepare_webview_process() {
        eprintln!("{error}");
        process::exit(1);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let dev_stage_cleanup = dev_stage_cleanup_root();
    let result = run_validated_no_flag();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let retains_main_custody = result
        .as_ref()
        .err()
        .is_some_and(keld_core::app_session::HostRunFailure::retains_main_custody);
    let mut failed = false;
    if let Err(error) = &result {
        // Reporting failure must not unwind/drop an owned main-custody payload.
        let mut output = std::io::stderr().lock();
        let _ = writeln!(output, "{error}");
        let _ = output.flush();
        failed = true;
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if !retains_main_custody
        && let Some(root) = dev_stage_cleanup
        && let Err(source) = fs::remove_dir_all(&root)
        && source.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!(
            "KELD-CORE-037: dev-stage cleanup failed for `{}` — {source}. Remove that owner-private nonce directory before relaunching.",
            root.display()
        );
        failed = true;
    }
    if failed {
        // The result and any !Send unproved-custody payload are still owned.
        // This existing fatal exit is failure, never a physical-drain receipt.
        process::exit(1);
    }
}

fn run_validated_no_flag() -> Result<(), keld_core::app_session::HostRunFailure> {
    let boot = keld_core::app_session::ValidatedBootSelection::from_current_exe_unprivileged()
        .map_err(keld_core::app_session::HostRunFailure::from)?;
    #[cfg(target_os = "macos")]
    {
        keld_core::app_session::run_guarded(boot, pre_init_terminal)
    }
    #[cfg(not(target_os = "macos"))]
    {
        keld_core::app_session::run_guarded(boot)
    }
}

/// A stalled native Init cannot be unwound: its blocked main stack retains
/// AppKit/presentation/profile custody and the live dev stage through exit.
#[cfg(target_os = "macos")]
fn pre_init_terminal(error: &keld_core::app_session::HostAppError) -> ! {
    let mut stderr = std::io::stderr().lock();
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    if env::var_os("KELD_T1B_STALL_BEFORE_INIT").is_some() {
        let _ = writeln!(stderr, "KELD_T1B_TERMINAL_ENTERED");
    }
    let _ = writeln!(stderr, "{error}");
    let _ = stderr.flush();
    process::exit(1)
}

#[cfg(windows)]
fn handle_windows_private_role(args: &[std::ffi::OsString]) -> bool {
    if handle_profile_test_root_probe(args) {
        return true;
    }
    if args.get(1).map(std::ffi::OsString::as_os_str)
        == Some(std::ffi::OsStr::new(WINDOWS_DEV_STAGE_CLEANUP_ARG))
    {
        if let Err(error) = run_windows_dev_stage_cleanup(args) {
            let mut output = std::io::stdout().lock();
            let _ = writeln!(output, "{error}");
            let _ = output.flush();
            process::exit(1);
        }
        return true;
    }
    false
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
fn run_macos_profile_fixture_role(args: &[String]) -> bool {
    match args.get(1).map(String::as_str) {
        Some("--keld-profile-identity-fixture-v1") => {
            if args.len() != 2 {
                eprintln!("KELD-WV-009: malformed private profile identity fixture invocation.");
                process::exit(2);
            }
            match keld_core::app_session::macos_profile_identity_fixture_report() {
                Ok(report) => println!("{report}"),
                Err(error) => {
                    eprintln!("{error}");
                    process::exit(1);
                }
            }
            true
        }
        Some("--keld-profile-presence-fixture-v1") => {
            if args.len() != 2 {
                eprintln!("KELD-WV-009: malformed private profile presence fixture invocation.");
                process::exit(2);
            }
            match keld_core::app_session::macos_profile_store_presence_fixture_report() {
                Ok(report) => println!("{report}"),
                Err(error) => {
                    eprintln!("{error}");
                    process::exit(1);
                }
            }
            true
        }
        Some("--keld-profile-purge-fixture-v1") => {
            if args.len() != 2 {
                eprintln!("KELD-WV-009: malformed private profile purge fixture invocation.");
                process::exit(2);
            }
            match keld_core::app_session::macos_profile_purge_fixture() {
                Ok(report) => println!("{report}"),
                Err(error) => {
                    eprintln!("{error}");
                    process::exit(1);
                }
            }
            true
        }
        Some("--keld-profile-webview-fixture-v1") => {
            if args.len() != 2 {
                eprintln!("KELD-WV-009: malformed private profile WebView fixture invocation.");
                process::exit(2);
            }
            if let Err(error) = keld_core::app_session::run_macos_profile_webview_fixture() {
                eprintln!("{error}");
                process::exit(1);
            }
            true
        }
        _ => false,
    }
}

#[cfg(windows)]
fn handle_profile_test_root_probe(args: &[std::ffi::OsString]) -> bool {
    if args.get(1).map(std::ffi::OsString::as_os_str)
        != Some(std::ffi::OsStr::new("--keld-profile-test-root-probe-v1"))
    {
        return false;
    }
    #[cfg(feature = "kel135-profile-test-root")]
    {
        if args.len() != 2 {
            eprintln!("KELD-CORE-037: profile test-root probe arguments are malformed.");
            process::exit(2);
        }
        let Some(root) = env::var_os("KELD_PROFILE_TEST_ROOT") else {
            eprintln!("KELD-CORE-037: profile test-root probe has no selected root.");
            process::exit(2);
        };
        println!("KELD_PROFILE_TEST_ROOT_FEATURE {}", root.to_string_lossy());
        return true;
    }
    #[cfg(not(feature = "kel135-profile-test-root"))]
    {
        eprintln!("KELD-CORE-037: profile test-root probe is unavailable in this host.");
        process::exit(2);
    }
}

#[cfg(windows)]
fn run_windows_dev_stage_cleanup(args: &[std::ffi::OsString]) -> Result<(), String> {
    if args.len() != 4 {
        return Err(String::from(
            "KELD-CORE-037: private Windows dev-stage cleanup invocation is malformed. Launch it only through `keld dev`.",
        ));
    }
    let root = PathBuf::from(&args[2]);
    let host_pid = args[3].to_string_lossy().parse::<u32>().map_err(|source| {
        format!(
            "KELD-CORE-037: private Windows dev-stage cleanup PID is invalid: {source}. Relaunch through `keld dev`."
        )
    })?;
    let attempt_job =
        keld_runtime::windows_job::WindowsProcessJob::adopt_cleanup_observer_from_stderr(host_pid)
            .map_err(|error| format!("KELD-CORE-037: cleanup Job admission failed: {error}."))?;
    let cleanup = keld_core::app_session::WindowsDevStageCleanup::prepare(&root, host_pid)
        .map_err(|error| error.to_string())?;
    println!("KELD_WINDOWS_DEV_STAGE_CLEANUP_READY");
    std::io::stdout().flush().map_err(|source| {
        format!(
            "KELD-CORE-037: private Windows dev-stage cleanup readiness failed: {source}. Relaunch through `keld dev`."
        )
    })?;
    let signal = keld_runtime::windows_job::WindowsProcessJob::await_cleanup_release_v1();
    let family_cleanup = if matches!(
        &signal,
        Ok(
            keld_runtime::windows_job::WindowsCleanupReleaseSignal::Released
                | keld_runtime::windows_job::WindowsCleanupReleaseSignal::LauncherLost,
        )
    ) {
        attempt_job.gracefully_wait_and_reap_attached(
            std::time::Duration::from_secs(10),
            std::time::Duration::from_secs(30),
        )
    } else {
        attempt_job.terminate_and_wait_attached(std::time::Duration::from_secs(30))
    };
    family_cleanup.map_err(|error| {
        format!("KELD-CORE-037: attempt process-family cleanup failed: {error}.")
    })?;
    cleanup
        .wait_and_delete_after_family_exit(&attempt_job)
        .map_err(|error| error.to_string())?;
    match signal {
        Ok(
            keld_runtime::windows_job::WindowsCleanupReleaseSignal::Released
            | keld_runtime::windows_job::WindowsCleanupReleaseSignal::LauncherLost,
        ) => Ok(()),
        Ok(keld_runtime::windows_job::WindowsCleanupReleaseSignal::Malformed) => Err(String::from(
            "KELD-CORE-037: malformed cleanup release was refused after safely reaping the exact attempt.",
        )),
        Err(error) => Err(format!(
            "KELD-CORE-037: cleanup control pipe failed; exact attempt was safely reaped: {error}."
        )),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn dev_stage_cleanup_root() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let lease = env::var_os("KELD_DEV_LEASE");
    dev_stage_cleanup_root_for(&executable, lease.as_deref())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn dev_stage_cleanup_root_for(
    executable: &std::path::Path,
    lease: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if lease != Some(std::ffi::OsStr::new("stdin-v1")) {
        return None;
    }
    let executable = executable.canonicalize().ok()?;
    let root = executable.parent()?;
    let nonce = root.file_name()?.to_str()?;
    if nonce.len() != 32
        || !nonce
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        || root.parent()?.file_name()? != "dev"
        || root.parent()?.parent()?.file_name()? != ".keld"
    {
        return None;
    }
    let metadata = fs::symlink_metadata(root).ok()?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o7777 != 0o700 {
        return None;
    }
    Some(root.to_path_buf())
}

#[cfg(target_os = "macos")]
fn run_supervised_guardian(args: &[String]) -> Result<(), String> {
    if args.len() != 6 {
        return Err(String::from(
            "KELD-CORE-037: private guardian invocation is malformed. Launch it only through the validated no-flag host.",
        ));
    }
    let root = PathBuf::from(&args[2]);
    let entry = PathBuf::from(&args[3]);
    let expected_dev = args[4]
        .parse::<u64>()
        .map_err(|source| format!("KELD-CORE-037: invalid private entry device — {source}. Relaunch the validated no-flag host."))?;
    let expected_ino = args[5]
        .parse::<u64>()
        .map_err(|source| format!("KELD-CORE-037: invalid private entry inode — {source}. Relaunch the validated no-flag host."))?;
    // The guardian is the last process executing from the staged directory:
    // the host parent exits when its CLI lease closes. Re-derive the exact
    // nonce from this executable rather than trusting the role argument.
    let cleanup_root = guardian_stage_cleanup_root(&root);
    let cleanup_validation = cleanup_root.clone();
    let report = keld_runtime::macos_guardian::run_guarded_primary(
        std::io::stdin(),
        move |app_link| {
            guardian_bun_command(
                &cleanup_validation,
                &root,
                &entry,
                expected_dev,
                expected_ino,
                app_link,
            )
        },
        std::io::stdout(),
    );
    // Schema-two facade startup can exit directly from the coordinator while
    // AppKit is still blocked before Init. Its parent Host alone can prove
    // native release and delete the stage on ordinary return. This flag is
    // overwritten by that Host on the authenticated Guardian spawn.
    let cleanup = if env::var_os("KELD_HOST_OWNS_FACADE_STAGE_CLEANUP")
        == Some(std::ffi::OsString::from("1"))
    {
        cleanup_root.map(|_| ())
    } else {
        cleanup_root.and_then(|cleanup_root| match fs::remove_dir_all(&cleanup_root) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(format!(
                "KELD-CORE-037: dev-stage cleanup failed for `{}` — {source}. Remove that owner-private nonce directory before relaunching.",
                cleanup_root.display()
            )),
        })
    };
    let report = report.map_err(|error| {
        format!(
            "KELD-CORE-037: supervised Bun guardian failed — {error}. Fix the Bun app failure and relaunch the no-flag host."
        )
    });
    let (report, cleanup_failure) = reconcile_guardian_result(report, cleanup)?;
    let stdout_notice = keld_runtime::CapturedOutput::elision_notice(report.stdout_dropped_bytes)
        .unwrap_or_default();
    let stderr_notice = keld_runtime::CapturedOutput::elision_notice(report.stderr_dropped_bytes)
        .unwrap_or_default();
    std::io::stderr()
        .write_all(report.stdout.as_bytes())
        .and_then(|()| std::io::stderr().write_all(stdout_notice.as_bytes()))
        .and_then(|()| std::io::stderr().write_all(report.stderr.as_bytes()))
        .and_then(|()| std::io::stderr().write_all(stderr_notice.as_bytes()))
        .map_err(|source| format!("KELD-CORE-037: guardian stderr failed — {source}. Retry."))?;
    cleanup_failure.map_or(Ok(()), Err)
}

#[cfg(target_os = "macos")]
fn guardian_bun_command(
    cleanup_validation: &Result<PathBuf, String>,
    root: &Path,
    entry: &Path,
    expected_dev: u64,
    expected_ino: u64,
    app_link: &str,
) -> Result<Command, keld_runtime::RuntimeError> {
    cleanup_validation
        .as_ref()
        .map_err(|error| guardian_entry_error(error.clone()))?;
    let reopened = reopen_validated_entry(root, entry, expected_dev, expected_ino)?;
    // Advisory same-user dev-boundary check: retain the exact reopened
    // identity until immediately before Supervisor invokes spawn, then
    // close it. Bun resolves the name afterward, so this does not claim
    // release-grade resistance to a same-user replacement in that
    // residual window; the signed-container successor must close it.
    drop(reopened);
    // No `pre_exec` hook here: the Guardian's SpawnAdmission gate must be
    // the only one, so a child refused before its exec permit is its whole
    // process group (keld-runtime `GuardedPrimaryPreparer::prepare`).
    let mut command = Command::new("bun");
    let sidecar = root.join("src").join("kipc-transport.ts");
    match std::fs::metadata(&sidecar) {
        Ok(metadata) if metadata.is_file() => {
            // A local bunfig can replace Bun's CLI preload. The dev-stage
            // producer copies only named boot artifacts; recheck the qualified
            // cwd on every generation before the original entry is executed.
            for config in ["bunfig.toml", ".bunfig.toml"] {
                match std::fs::symlink_metadata(root.join(config)) {
                    Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                    _ => {
                        return Err(guardian_entry_error(format!(
                            "KELD-CORE-037: staged runtime config {config} can override the canonical preload. Restage through `keld dev`."
                        )));
                    }
                }
            }
            command.arg(format!("--preload={}", sidecar.display()));
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(guardian_entry_error(String::from(
                "KELD-CORE-037: staged kipc transport is not a regular file. Restage through `keld dev`.",
            )));
        }
        Err(source) => {
            return Err(guardian_entry_error(format!(
                "KELD-CORE-037: staged kipc transport cannot be inspected: {source}. Restage through `keld dev`."
            )));
        }
    }
    command
        .arg("run")
        .arg(root.join(entry))
        .current_dir(root)
        .env("KELD_APP_LINK", app_link)
        .stdin(Stdio::null());
    Ok(command)
}

#[cfg(target_os = "macos")]
fn reconcile_guardian_result<T>(
    report: Result<T, String>,
    cleanup: Result<(), String>,
) -> Result<(T, Option<String>), String> {
    match (report, cleanup) {
        (Ok(report), Ok(())) => Ok((report, None)),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(report), Err(cleanup)) => Ok((report, Some(cleanup))),
        (Err(primary), Err(cleanup)) => {
            Err(format!("{primary} Additional cleanup failure: {cleanup}"))
        }
    }
}

#[cfg(target_os = "macos")]
fn guardian_stage_cleanup_root(argument: &Path) -> Result<PathBuf, String> {
    let executable = env::current_exe().map_err(|source| {
        format!(
            "KELD-CORE-037: guardian stage identity failed — {source}. Relaunch through `keld dev`."
        )
    })?;
    let expected = dev_stage_cleanup_root_for(&executable, Some(std::ffi::OsStr::new("stdin-v1")))
        .ok_or_else(|| String::from("KELD-CORE-037: guardian is not executing from a validated private dev stage. Relaunch through `keld dev`."))?;
    let argument = argument.canonicalize().map_err(|source| {
        format!(
            "KELD-CORE-037: guardian stage identity failed — {source}. Relaunch through `keld dev`."
        )
    })?;
    if argument != expected {
        return Err(String::from(
            "KELD-CORE-037: guardian stage argument does not match its validated executable stage. Relaunch through `keld dev`.",
        ));
    }
    Ok(expected)
}

#[cfg(target_os = "macos")]
fn guardian_entry_error(detail: impl Into<String>) -> keld_runtime::RuntimeError {
    keld_runtime::RuntimeError::Lifecycle {
        phase: "KEL-96 validated entry handoff",
        source: io::Error::new(io::ErrorKind::InvalidData, detail.into()),
    }
}

#[cfg(target_os = "macos")]
fn reopen_validated_entry(
    root: &std::path::Path,
    entry: &std::path::Path,
    expected_dev: u64,
    expected_ino: u64,
) -> Result<File, keld_runtime::RuntimeError> {
    let reopened = File::open(root.join(entry))
        .map_err(|source| guardian_entry_error(format!("entry reopen failed: {source}")))?;
    let metadata = reopened
        .metadata()
        .map_err(|source| guardian_entry_error(format!("entry identity read failed: {source}")))?;
    if !metadata.is_file() || metadata.dev() != expected_dev || metadata.ino() != expected_ino {
        return Err(guardian_entry_error(
            "entry path no longer names the validated regular-file identity",
        ));
    }
    Ok(reopened)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    use super::{
        dev_stage_cleanup_root_for, guardian_bun_command, reconcile_guardian_result,
        reopen_validated_entry,
    };

    #[test]
    fn successful_guardian_report_survives_cleanup_failure() {
        let (report, cleanup_failure) =
            reconcile_guardian_result(Ok("captured output"), Err(String::from("cleanup failed")))
                .expect("successful report must remain available for diagnostics");

        assert_eq!(report, "captured output");
        assert_eq!(cleanup_failure.as_deref(), Some("cleanup failed"));
    }

    #[test]
    fn failed_stage_validation_refuses_the_bun_command_with_a_valid_entry() {
        let temp = tempfile::tempdir().expect("stage-gate fixture");
        let entry = temp.path().join("entry.ts");
        fs::write(&entry, "console.log('must not run');\n").expect("entry fixture");
        let identity = fs::metadata(&entry).expect("entry identity");
        let relative = std::path::Path::new("entry.ts");

        // The entry identity is deliberately valid, so a refusal here is
        // attributable to the stage result alone and not to the inode check.
        let error = guardian_bun_command(
            &Err(String::from(
                "KELD-CORE-037: guardian stage argument does not match its validated executable stage. Relaunch through `keld dev`.",
            )),
            temp.path(),
            relative,
            identity.dev(),
            identity.ino(),
            "/tmp/link#token",
        )
        .expect_err("failed stage validation must refuse the Bun command");
        assert!(
            error
                .to_string()
                .contains("guardian stage argument does not match its validated executable stage"),
            "{error}"
        );

        // Negative control: identical inputs with a validated stage still
        // produce the command, so the refusal is not an unrelated failure.
        let command = guardian_bun_command(
            &Ok(temp.path().to_path_buf()),
            temp.path(),
            relative,
            identity.dev(),
            identity.ino(),
            "/tmp/link#token",
        )
        .expect("validated stage must still produce the Bun command");
        assert_eq!(command.get_program(), "bun");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["run".as_ref(), entry.as_os_str()]
        );

        fs::create_dir(temp.path().join("src")).expect("transport directory");
        let sidecar = temp.path().join("src/kipc-transport.ts");
        fs::write(&sidecar, "export {};\n").expect("transport sidecar");
        let command = guardian_bun_command(
            &Ok(temp.path().to_path_buf()),
            temp.path(),
            relative,
            identity.dev(),
            identity.ino(),
            "/tmp/link#token",
        )
        .expect("canonical sidecar launch");
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(args[0], format!("--preload={}", sidecar.display()).as_str());
        assert_eq!(args[1], "run");
        assert_eq!(args[2], entry.as_os_str());
        assert_eq!(command.get_current_dir(), Some(temp.path()));
        // Bun config can replace a CLI preload. Qualify every generation,
        // including recovery after a previously absent config appears.
        for config in ["bunfig.toml", ".bunfig.toml"] {
            let config_path = temp.path().join(config);
            fs::write(&config_path, "preload = [\"./app-hook.ts\"]\n").expect("hostile config");
            let error = guardian_bun_command(
                &Ok(temp.path().to_path_buf()),
                temp.path(),
                relative,
                identity.dev(),
                identity.ino(),
                "/tmp/link#token",
            )
            .expect_err("ambient preload override must refuse launch");
            assert!(error.to_string().contains("runtime config"), "{error}");
            fs::remove_file(config_path).expect("restore qualified stage");
        }
    }

    #[test]
    fn dev_stage_cleanup_accepts_only_the_exact_private_nonce_layout() {
        let temp = tempfile::tempdir().expect("cleanup fixture");
        let root = temp
            .path()
            .join("project/.keld/dev/0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&root).expect("private nonce root");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("private mode");
        let executable = root.join("keld-host");
        fs::write(&executable, b"host").expect("host fixture");
        let canonical_root = root.canonicalize().expect("canonical nonce root");

        assert_eq!(
            dev_stage_cleanup_root_for(&executable, Some(OsStr::new("stdin-v1"))).as_deref(),
            Some(canonical_root.as_path())
        );
        assert!(dev_stage_cleanup_root_for(&executable, None).is_none());
        assert!(dev_stage_cleanup_root_for(&executable, Some(OsStr::new("wrong"))).is_none());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).expect("public mode");
        assert!(dev_stage_cleanup_root_for(&executable, Some(OsStr::new("stdin-v1"))).is_none());

        let unrelated = temp.path().join("bin/keld-host");
        fs::create_dir_all(unrelated.parent().expect("unrelated parent"))
            .expect("unrelated directory");
        fs::write(&unrelated, b"host").expect("unrelated host");
        assert!(dev_stage_cleanup_root_for(&unrelated, Some(OsStr::new("stdin-v1"))).is_none());
    }

    #[test]
    fn guardian_rejects_entry_path_replacement_after_parent_validation() {
        let temp = tempfile::tempdir().expect("entry identity fixture");
        let entry = temp.path().join("entry.ts");
        fs::write(&entry, "console.log('original');\n").expect("original entry");
        let original = fs::File::open(&entry).expect("open original entry");
        let identity = original.metadata().expect("original identity");
        fs::remove_file(&entry).expect("unlink original entry");
        fs::write(&entry, "console.log('replacement');\n").expect("replacement entry");

        let error = reopen_validated_entry(
            temp.path(),
            std::path::Path::new("entry.ts"),
            identity.dev(),
            identity.ino(),
        )
        .expect_err("replacement inode must fail authenticated handoff");
        assert!(
            error
                .to_string()
                .contains("validated regular-file identity")
        );
    }
}
