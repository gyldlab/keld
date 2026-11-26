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
    #[cfg(any(target_os = "macos", windows))]
    {
        keld_core::app_session::run_guarded(boot, pre_init_terminal)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        keld_core::app_session::run_guarded(boot)
    }
}

/// A stalled native Init cannot be unwound: its blocked main stack retains
/// native window/presentation/profile custody and the live dev stage through exit.
#[cfg(any(target_os = "macos", windows))]
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
    #[cfg(target_os = "macos")]
    if keld_core::app_session::macos_local_app_candidate().ok()? {
        // Even a malformed installed candidate has no development-stage delete
        // capability. Full admission happens separately and must fail closed.
        return None;
    }
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
fn forward_untrusted_guardian_output(
    sink: &mut impl io::Write,
    source: &'static str,
    bytes: &str,
) -> io::Result<()> {
    // Darwin's PIPE_BUF is 512 bytes (sys/syslimits.h). Include the longest
    // prefix and optional split newline inside that atomic write budget.
    const CHUNK_BYTES: usize = 384;
    // Guardian-captured Bun bytes share the Host's inherited stderr at `keld
    // dev`. Without framing, a Bun program can print an identical GH480 JSONL
    // row with no Menu CALL and fool a Host-only acceptance oracle. Keep
    // forwarding on the same approved stderr path. Ordinary plain-text app
    // logs (including native acceptance markers) must stay byte-identical.
    let prefix: &[u8] = match source {
        "stdout" => b"[bun stdout] ",
        "stderr" => b"[bun stderr] ",
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown Bun output source",
            ));
        }
    };
    // Cap each framed write below the POSIX PIPE_BUF minimum (512 bytes) so the
    // prefix and content travel as one write on the inherited pipe. Split
    // oversized untrusted lines rather than ever emitting an unframed tail.
    let mut framed = Vec::with_capacity(CHUNK_BYTES + 32);
    for line in bytes.split_inclusive(|ch: char| {
        matches!(
            ch,
            '\n' | '\r'
                | '\u{000b}'
                | '\u{000c}'
                | '\u{001c}'
                | '\u{001d}'
                | '\u{001e}'
                | '\u{0085}'
                | '\u{2028}'
                | '\u{2029}'
        )
    }) {
        let mut remaining = line;
        let mut continued = false;
        while !remaining.is_empty() {
            let mut end = remaining.len().min(CHUNK_BYTES);
            while !remaining.is_char_boundary(end) {
                end -= 1;
            }
            let (part, rest) = remaining.split_at(end);
            framed.clear();
            // Only a JSON-looking line can impersonate an unprefixed Host
            // JSONL record. A continuation of an oversized line is always
            // framed so a malicious boundary cannot create a new raw row.
            if continued || part.trim_start().starts_with('{') {
                framed.extend_from_slice(prefix);
            }
            framed.extend_from_slice(part.as_bytes());
            if !rest.is_empty() {
                framed.push(b'\n');
            }
            sink.write_all(&framed)?;
            remaining = rest;
            continued = true;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_supervised_guardian(args: &[String]) -> Result<(), String> {
    let (native_facade, local_launch, development_facts) = parse_guardian_launch_arguments(args)?;
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
    let cleanup_root = match keld_core::app_session::macos_local_app_candidate() {
        Ok(true) => Ok(None),
        Ok(false) => guardian_stage_cleanup_root(&root).map(Some),
        Err(error) => Err(error.to_string()),
    };
    let cleanup_validation = cleanup_root.clone();
    let mut local_facts: Option<keld_core::app_session::MacLocalGuardianFacts> = None;
    let report = keld_runtime::macos_guardian::run_guarded_primary(
        std::io::stdin(),
        move || {
            Ok(move |app_link: &str| {
                let owned_stage = cleanup_validation
                    .as_ref()
                    .map_err(|error| guardian_entry_error(error.clone()))?;
                if let Some(stage) = owned_stage {
                    if local_launch.is_some() {
                        return Err(guardian_entry_error(
                            "a development-stage Guardian cannot consume installed bundle pins",
                        ));
                    }
                    guardian_bun_command(
                        &Ok(stage.clone()),
                        &root,
                        &entry,
                        expected_dev,
                        expected_ino,
                        (native_facade, development_facts.as_deref()),
                        app_link,
                    )
                } else {
                    if development_facts.is_some() {
                        return Err(guardian_entry_error(
                            "installed Guardian cannot consume development facts",
                        ));
                    }
                    let handoff = local_launch.as_ref().ok_or_else(|| {
                        guardian_entry_error("installed Guardian requires exact parent bundle pins")
                    })?;
                    // Runtime authenticates registration and liveness before it
                    // invokes this per-generation factory. Expected pins are data,
                    // never authority to select another installed bundle.
                    local_guardian_bun_command(
                        &mut local_facts,
                        &root,
                        &entry,
                        (expected_dev, expected_ino),
                        handoff,
                        native_facade,
                        app_link,
                    )
                }
            })
        },
        std::io::stdout(),
    );
    // Schema-two facade startup can exit directly from the coordinator while
    // AppKit is still blocked before Init. Its parent Host alone can prove
    // native release and delete the stage on ordinary return. This flag is
    // overwritten by that Host on the authenticated Guardian spawn.
    let cleanup = cleanup_root.and_then(|owned_stage| {
        let Some(cleanup_root) = owned_stage else {
            return Ok(());
        };
        if env::var_os("KELD_HOST_OWNS_FACADE_STAGE_CLEANUP")
            == Some(std::ffi::OsString::from("1"))
        {
            return Ok(());
        }
        match fs::remove_dir_all(&cleanup_root) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(format!(
                "KELD-CORE-037: dev-stage cleanup failed for `{}` — {source}. Remove that owner-private nonce directory before relaunching.",
                cleanup_root.display()
            )),
        }
    });
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
    let mut stderr = std::io::stderr().lock();
    forward_untrusted_guardian_output(&mut stderr, "stdout", &report.stdout)
        .and_then(|()| stderr.write_all(stdout_notice.as_bytes()))
        .and_then(|()| forward_untrusted_guardian_output(&mut stderr, "stderr", &report.stderr))
        .and_then(|()| stderr.write_all(stderr_notice.as_bytes()))
        .map_err(|source| format!("KELD-CORE-037: guardian stderr failed — {source}. Retry."))?;
    cleanup_failure.map_or(Ok(()), Err)
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct LocalGuardianLaunch {
    pins: keld_core::app_session::MacLocalIntegrityPins,
    // Opaque until Runtime has authenticated the one-use registration and
    // retained the Host liveness owner. Core then independently validates it.
    host_facts: Option<String>,
}

#[cfg(target_os = "macos")]
fn local_guardian_bun_command(
    retained: &mut Option<keld_core::app_session::MacLocalGuardianFacts>,
    root: &Path,
    entry: &Path,
    expected_entry: (u64, u64),
    handoff: &LocalGuardianLaunch,
    native_facade: bool,
    app_link: &str,
) -> Result<Command, keld_runtime::RuntimeError> {
    if retained.is_none() {
        let admitted = match handoff.host_facts.as_deref() {
            Some(text) => {
                keld_core::app_session::MacLocalGuardianFacts::from_current_exe_with_host_facts(
                    text,
                )
            }
            None => keld_core::app_session::MacLocalGuardianFacts::from_current_exe(),
        };
        *retained = Some(admitted.map_err(|error| guardian_entry_error(error.to_string()))?);
    }
    let facts = retained.as_ref().ok_or_else(|| {
        guardian_entry_error("installed Guardian lost its validated bundle owner")
    })?;
    facts
        .verify_parent_entry(root, entry, expected_entry.0, expected_entry.1)
        .and_then(|()| facts.verify_parent_integrity(handoff.pins))
        .and_then(|()| facts.revalidate_generation())
        .map_err(|error| guardian_entry_error(error.to_string()))?;
    if facts.native_facade() != native_facade
        || facts.original_graph() != handoff.host_facts.is_some()
    {
        return Err(guardian_entry_error(
            "parent window mode differs from the sealed installed boot",
        ));
    }
    let mut command = Command::new(facts.bun_path());
    keld_runtime::primary::configure_local_macos_bun_startup(&mut command);
    command
        .arg(
            facts
                .launch_facts_bun_argument()
                .map_err(|error| guardian_entry_error(error.to_string()))?,
        )
        .arg(format!(
            "--preload={}",
            facts.launch_facts_preload_path().display()
        ));
    let runtime_preload = facts
        .runtime_preload_path()
        .map_err(|error| guardian_entry_error(error.to_string()))?;
    let runtime_entry = facts
        .runtime_entry_bootstrap_path()
        .map_err(|error| guardian_entry_error(error.to_string()))?;
    configure_installed_runtime_bootstrap(
        &mut command,
        facts.app_root(),
        facts.native_facade(),
        runtime_preload.as_deref(),
        runtime_entry.as_deref(),
    )?;
    append_installed_main(&mut command, facts.app_root(), facts.entry_relative());
    command
        .current_dir(facts.app_root())
        .env("KELD_APP_LINK", app_link)
        .stdin(Stdio::null());
    Ok(command)
}

/// The sealed preload finishes authenticated initialization before the
/// original runs as Bun main. The selected original occupies argv[1] only;
/// no app-private selector becomes an additional user argument.
/// Non-graph installed callers retain their direct-entry behavior.
#[cfg(target_os = "macos")]
fn append_installed_main(command: &mut Command, root: &Path, entry_relative: &Path) {
    command.arg("run").arg(root.join(entry_relative));
}

/// Selects only admitted installed source roles. Immutable launch-facts preload
/// is already first. An original graph must have both its independently
/// verified provider wrapper AND the fixed Core-compiled app-root bootstrap.
/// Bun executes the signed bootstrap as a pre-entry preload and the selected
/// original as its actual main, after authenticated Init ACK/subscriptions.
/// This Mac route requires a verified shared module instance across preload
/// and main. The current Windows Bun cache split remains unsupported.
#[cfg(target_os = "macos")]
fn configure_installed_runtime_bootstrap(
    command: &mut Command,
    root: &Path,
    native_facade: bool,
    admitted_runtime_preload: Option<&Path>,
    admitted_bootstrap: Option<&Path>,
) -> Result<(), keld_runtime::RuntimeError> {
    match (admitted_runtime_preload, admitted_bootstrap) {
        (Some(_), Some(bootstrap)) => {
            if !native_facade
                || bootstrap != root.join(keld_core::app_session::MACOS_LOCAL_RUNTIME_ENTRY_PATH)
            {
                return Err(guardian_entry_error(
                    "an original runtime bootstrap requires the exact compiled owner member",
                ));
            }
            command
                .arg("--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\"")
                .arg("--define=__KELD_RUNTIME_PROCESS_FACTS__:true")
                .arg(format!("--preload={}", bootstrap.display()));
            Ok(())
        }
        (None, None) => {
            // Non-graph installed callers retain their optional sidecar,
            // unchanged direct-entry identity and existing process semantics.
            command
                .arg("--define=__KELD_RUNTIME_PROCESS_FACTS__:false")
                .args(keld_runtime::primary::staged_bun_launch_args(
                    root,
                    native_facade,
                )?);
            Ok(())
        }
        _ => Err(guardian_entry_error(
            "original graph wrapper and fixed Host bootstrap must agree",
        )),
    }
}

#[cfg(target_os = "macos")]
fn parse_guardian_boot_mode(args: &[String]) -> Result<bool, String> {
    if args.len() == 7 {
        match args[6].as_str() {
            "0" => return Ok(false),
            "1" => return Ok(true),
            _ => {}
        }
    }
    Err(String::from(
        "KELD-CORE-037: private guardian invocation is malformed. Launch it only through the validated no-flag host.",
    ))
}

#[cfg(target_os = "macos")]
fn parse_guardian_launch_arguments(
    args: &[String],
) -> Result<(bool, Option<LocalGuardianLaunch>, Option<String>), String> {
    if args.len() == 7 {
        return parse_guardian_boot_mode(args).map(|mode| (mode, None, None));
    }
    // Only the canonical JSON-object envelope is a development payload.
    // Installed suffix markers must not be reinterpreted when their pins are truncated.
    // Keep the bounded body opaque until authenticated startup.
    if args.len() == 8
        && args[7].starts_with('{')
        && args[7].ends_with('}')
        && args[7].len() <= 16_384
    {
        let mode = parse_guardian_boot_mode(&args[..7])?;
        return Ok((mode, None, Some(args[7].clone())));
    }
    if args.len() == 11 && args[7] == "local-bundle-v1" {
        let mode = parse_guardian_boot_mode(&args[..7])?;
        let pins = keld_core::app_session::MacLocalIntegrityPins::from_encoded(
            &args[8], &args[9], &args[10],
        )
        .map_err(|error| error.to_string())?;
        return Ok((
            mode,
            Some(LocalGuardianLaunch {
                pins,
                host_facts: None,
            }),
            None,
        ));
    }
    if args.len() == 12
        && args[6] == "2"
        && args[7] == "local-bundle-v2"
        && !args[11].is_empty()
        && args[11].len() <= 16_384
    {
        let pins = keld_core::app_session::MacLocalIntegrityPins::from_encoded(
            &args[8], &args[9], &args[10],
        )
        .map_err(|error| error.to_string())?;
        return Ok((
            true,
            Some(LocalGuardianLaunch {
                pins,
                host_facts: Some(args[11].clone()),
            }),
            None,
        ));
    }
    Err(String::from(
        "KELD-CORE-037: private Guardian requires the exact development, local-bundle-v1 or local-bundle-v2 argument shape. Launch only through the validated Host.",
    ))
}

#[cfg(target_os = "macos")]
fn guardian_bun_command(
    cleanup_validation: &Result<PathBuf, String>,
    root: &Path,
    entry: &Path,
    expected_dev: u64,
    expected_ino: u64,
    startup: (bool, Option<&str>),
    app_link: &str,
) -> Result<Command, keld_runtime::RuntimeError> {
    let (native_facade, launch_facts) = startup;
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
    // This factory runs only after Guardian registration/liveness admission.
    // Reuse the canonical closed codec and bind to the validated development stage.
    let launch_facts = launch_facts
        .map(|text| {
            keld_core::app_session::DevelopmentLaunchFacts::from_private_argument(text, root)
                .map_err(|error| guardian_entry_error(error.to_string()))
        })
        .transpose()?;
    // No `pre_exec` hook here: the Guardian's SpawnAdmission gate must be
    // the only one, so a child refused before its exec permit is its whole
    // process group (keld-runtime `GuardedPrimaryPreparer::prepare`).
    // Recheck the staged cwd on every generation: the one admission owner refuses
    // runtime config, which can override both CLI define and preload, and any link.
    let mut command = Command::new("bun");
    command
        .args(keld_runtime::primary::staged_bun_launch_args(
            root,
            native_facade,
        )?)
        // Absence is also fixed by the Host; an application global cannot
        // fabricate the private constant on the historical development path.
        .arg(match launch_facts.as_ref() {
            Some(facts) => facts.bun_define_argument().map_err(|error| guardian_entry_error(error.to_string()))?,
            None => String::from("--define=__KELD_HOST_LAUNCH_FACTS__:undefined"),
        })
        .arg("run")
        .arg(root.join(entry))
        .current_dir(root)
        .env("KELD_APP_LINK", app_link)
        .env_remove(keld_runtime::primary::BUN_OPTIONS_ENV)
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
        dev_stage_cleanup_root_for, forward_untrusted_guardian_output, guardian_bun_command,
        parse_guardian_boot_mode, parse_guardian_launch_arguments, reconcile_guardian_result,
        reopen_validated_entry,
    };

    struct AtomicPipeProbe(Vec<u8>);

    impl std::io::Write for AtomicPipeProbe {
        fn write(&mut self, chunk: &[u8]) -> std::io::Result<usize> {
            assert!(
                chunk.len() <= 512,
                "one inherited-pipe write exceeds PIPE_BUF"
            );
            self.0.extend_from_slice(chunk);
            Ok(chunk.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn untrusted_guardian_output_cannot_forge_a_raw_host_menu_jsonl_line() {
        let forged = r#"{"code":"KELD-COMPAT-015","cause_code":"KELD-GUARD001","phase":"install","result":"denied","correlation":3}"#;
        // A logger may use CR-only, LF or Unicode line breaks; a parser
        // must not mistake a later untrusted segment for Host-origin JSONL.
        for break_char in ['\n', '\r', '\u{000b}', '\u{0085}', '\u{2028}', '\u{2029}'] {
            let source = format!("Bun app ready{break_char}{forged}{break_char}");
            for role in ["stdout", "stderr"] {
                let mut captured = Vec::new();
                forward_untrusted_guardian_output(&mut captured, role, &source)
                    .expect("forward bounded application diagnostics");
                let message = String::from_utf8(captured).expect("UTF-8 application diagnostics");
                assert!(message.contains("Bun app ready"), "{break_char:?} {role}");
                assert!(message.contains(forged), "{break_char:?} {role}");
                assert!(
                    !message
                        .split(['\n', '\r', '\u{000b}', '\u{0085}', '\u{2028}', '\u{2029}'])
                        .any(|line| line.starts_with(forged)),
                    "Bun-only JSONL must not appear as a raw Host diagnostic: {break_char:?} {role}"
                );
            }
        }
        // A large UTF-8 application line must not acquire an unframed
        // continuation when it crosses the 3 KiB atomic-write boundary.
        let mut captured = AtomicPipeProbe(Vec::new());
        let oversized = format!("{}{forged}\n", "🧵".repeat(2_250));
        forward_untrusted_guardian_output(&mut captured, "stderr", &oversized)
            .expect("framed long UTF-8 line");
        let text = String::from_utf8(captured.0).expect("long UTF-8 output");
        assert!(text.contains(forged));
        assert!(text.lines().count() > 3);
        assert!(
            text.lines()
                .skip(1)
                .all(|line| line.starts_with("[bun stderr] "))
        );
        for source in ["stdout", "stderr"] {
            let plain = "KELD_KEL135_SIGNED_IDENTITY current=true\nBun app ready\n";
            let mut forwarded = Vec::new();
            forward_untrusted_guardian_output(&mut forwarded, source, plain)
                .expect("ordinary acceptance logs");
            assert_eq!(
                forwarded,
                plain.as_bytes(),
                "ordinary {source} bytes changed"
            );
        }
    }

    #[test]
    fn real_host_menu_diagnostic_remains_unique_after_forged_bun_lines() {
        let host = r#"{"code":"KELD-COMPAT-015","cause_code":"KELD-GUARD001","phase":"install","result":"denied","correlation":3}"#;
        let mut merged = Vec::new();
        forward_untrusted_guardian_output(&mut merged, "stderr", &format!("{host}\n"))
            .expect("forward fake-only Bun output");
        assert_eq!(
            String::from_utf8(merged.clone())
                .expect("UTF-8")
                .lines()
                .filter(|line| line.starts_with(host))
                .count(),
            0,
            "fake-only app output cannot qualify an actual Host menu CALL"
        );
        let mut real = format!("{host}\n").into_bytes();
        forward_untrusted_guardian_output(&mut real, "stderr", &format!("{host}\n"))
            .expect("forward duplicate Bun output");
        assert_eq!(
            String::from_utf8(real)
                .expect("UTF-8")
                .lines()
                .filter(|line| line.starts_with(host))
                .count(),
            1,
            "one genuine Host record stays unique despite a Bun-origin fake"
        );
    }

    fn development_facts_fixture() -> std::io::Result<(tempfile::TempDir, fs::Metadata, Vec<String>)>
    {
        let temp = tempfile::tempdir()?;
        let entry = temp.path().join("entry.ts");
        fs::write(
            &entry,
            "console.log(JSON.stringify(__KELD_HOST_LAUNCH_FACTS__ ?? null));",
        )?;
        let identity = fs::metadata(&entry)?;
        let root = temp.path().to_str().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "development fixture root is not UTF-8",
            )
        })?;
        let encoded = serde_json::json!({"schema":1,"kind":"development","name":"Dev","root":root})
            .to_string();
        let args = vec![
            "host".to_owned(),
            "--keld-supervised-guardian".to_owned(),
            root.to_owned(),
            "entry.ts".to_owned(),
            identity.dev().to_string(),
            identity.ino().to_string(),
            "1".to_owned(),
            encoded.clone(),
        ];
        Ok((temp, identity, args))
    }

    #[test]
    fn guardian_development_facts_use_closed_stage_bound_argv() {
        let (temp, identity, args) =
            development_facts_fixture().expect("development facts fixture");
        let encoded = args[7].clone();
        let (facade, local, facts) =
            parse_guardian_launch_arguments(&args).expect("closed dev facts");
        assert!(facade && local.is_none());
        let mut command = guardian_bun_command(
            &Ok(temp.path().to_path_buf()),
            temp.path(),
            std::path::Path::new("entry.ts"),
            identity.dev(),
            identity.ino(),
            (facade, facts.as_deref()),
            "/tmp/link#token",
        )
        .expect("command");
        let command_args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let emitted = command_args
            .iter()
            .find_map(|arg| arg.strip_prefix("--define=__KELD_HOST_LAUNCH_FACTS__:"))
            .expect("facts define");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(emitted).expect("emitted JSON"),
            serde_json::from_str::<serde_json::Value>(&encoded).expect("input JSON")
        );
        let definition = command_args
            .iter()
            .position(|arg| arg.starts_with("--define=__KELD_HOST_LAUNCH_FACTS__:"))
            .expect("define");
        let entry_index = command_args
            .iter()
            .position(|arg| arg == "run")
            .expect("entry");
        assert!(definition < entry_index);
        let output = command
            .output()
            .expect("actual Bun launch with emitted facts");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("Bun fact record"),
            serde_json::from_str::<serde_json::Value>(&encoded).expect("input record")
        );
        let absent = guardian_bun_command(
            &Ok(temp.path().to_path_buf()),
            temp.path(),
            std::path::Path::new("entry.ts"),
            identity.dev(),
            identity.ino(),
            (facade, None),
            "/tmp/link#token",
        )
        .expect("historical command")
        .output()
        .expect("actual Bun absent launch");
        assert!(
            absent.status.success(),
            "{}",
            String::from_utf8_lossy(&absent.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&absent.stdout), "null\n");
    }

    #[test]
    fn guardian_development_facts_preserve_historical_fields() {
        let (temp, identity, mut args) =
            development_facts_fixture().expect("development facts fixture");
        let root = temp.path().to_str().expect("root");
        let facade = true;
        for schema in [2, 3] {
            let mut input = serde_json::json!({"schema":schema,"kind":"development","name":"Dev","root":root,"version":"1.2.3","paths":{"appData":"/Users/native/Library/Application Support","userData":"/Users/native/Library/Application Support/Dev","documents":"/Users/native/Documents"}});
            if schema == 3 {
                input["locale"] = serde_json::json!("en-GB");
            }
            args[7] = input.to_string();
            let (_, _, facts) = parse_guardian_launch_arguments(&args).expect("historical shape");
            let output = guardian_bun_command(
                &Ok(temp.path().to_path_buf()),
                temp.path(),
                std::path::Path::new("entry.ts"),
                identity.dev(),
                identity.ino(),
                (facade, facts.as_deref()),
                "/tmp/link#token",
            )
            .expect("historical fields command")
            .output()
            .expect("Bun historical fields");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&output.stdout)
                    .expect("emitted historical fields"),
                input
            );
        }
    }

    #[test]
    fn guardian_development_facts_reject_foreign_root_and_installed_markers() {
        let (temp, identity, mut args) =
            development_facts_fixture().expect("development facts fixture");
        let facade = true;
        args[7] =
            serde_json::json!({"schema":1,"kind":"development","name":"Dev","root":"/foreign"})
                .to_string();
        let (_, _, facts) = parse_guardian_launch_arguments(&args).expect("opaque input");
        assert!(
            guardian_bun_command(
                &Ok(temp.path().to_path_buf()),
                temp.path(),
                std::path::Path::new("entry.ts"),
                identity.dev(),
                identity.ino(),
                (facade, facts.as_deref()),
                "/tmp/link#token"
            )
            .is_err()
        );
        for malformed in ["", "local-bundle-v1", "local-bundle-v2"] {
            args[7] = malformed.to_owned();
            assert!(parse_guardian_launch_arguments(&args).is_err());
        }
    }

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
            (true, None),
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
            (true, None),
            "/tmp/link#token",
        )
        .expect("validated stage must still produce the Bun command");
        assert_eq!(command.get_program(), "bun");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\"".as_ref(),
                "--define=__KELD_HOST_LAUNCH_FACTS__:undefined".as_ref(),
                "run".as_ref(),
                entry.as_os_str()
            ]
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
            (true, None),
            "/tmp/link#token",
        )
        .expect("canonical sidecar launch");
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(
            args[0],
            "--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\""
        );
        assert_eq!(args[1], format!("--preload={}", sidecar.display()).as_str());
        assert_eq!(args[2], "--define=__KELD_HOST_LAUNCH_FACTS__:undefined");
        assert_eq!(args[3], "run");
        assert_eq!(args[4], entry.as_os_str());
        assert_eq!(args.len(), 5);
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
                (true, None),
                "/tmp/link#token",
            )
            .expect_err("ambient preload override must refuse launch");
            assert!(error.to_string().contains("runtime config"), "{error}");
            fs::remove_file(config_path).expect("restore qualified stage");
        }
    }

    #[test]
    fn guardian_freezes_mode_with_and_without_a_transport_sidecar() {
        let temp = tempfile::tempdir().expect("boot mode fixture");
        let entry = temp.path().join("entry.ts");
        fs::write(&entry, "console.log('ENTRY_REACHED');\n").expect("entry fixture");
        let identity = fs::metadata(&entry).expect("entry identity");
        for with_sidecar in [false, true] {
            let sidecar = temp.path().join("src/kipc-transport.ts");
            if with_sidecar {
                fs::create_dir_all(sidecar.parent().expect("sidecar directory"))
                    .expect("create src");
                fs::write(&sidecar, "export {};\n").expect("sidecar fixture");
            }
            for native_facade in [false, true] {
                let command = guardian_bun_command(
                    &Ok(temp.path().to_path_buf()),
                    temp.path(),
                    std::path::Path::new("entry.ts"),
                    identity.dev(),
                    identity.ino(),
                    (native_facade, None),
                    "/tmp/link#token",
                )
                .expect("raw no-sidecar launch remains supported");
                let mut expected = vec![std::ffi::OsString::from(if native_facade {
                    "--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\""
                } else {
                    "--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"0\""
                })];
                if with_sidecar {
                    expected.push(format!("--preload={}", sidecar.display()).into());
                }
                expected.push("--define=__KELD_HOST_LAUNCH_FACTS__:undefined".into());
                expected.extend(["run".into(), entry.as_os_str().to_owned()]);
                assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
            }
        }
    }

    #[test]
    fn guardian_accepts_only_an_exact_private_boot_mode_argument() {
        let mut args: Vec<String> = ["host", "guardian", "root", "entry", "1", "2"]
            .map(String::from)
            .into();
        assert!(
            parse_guardian_boot_mode(&args)
                .expect_err("missing mode")
                .starts_with("KELD-CORE-037:")
        );
        for mode in ["", "true", "false", "01", "2", "-1", " 1", "1 "] {
            args.push(mode.into());
            assert!(
                parse_guardian_boot_mode(&args)
                    .expect_err("strict private bit")
                    .starts_with("KELD-CORE-037:")
            );
            args.pop();
        }
        for (mode, expected) in [("0", false), ("1", true)] {
            args.push(mode.into());
            assert_eq!(parse_guardian_boot_mode(&args), Ok(expected));
            args.push("extra".into());
            assert!(
                parse_guardian_boot_mode(&args).is_err(),
                "extra arguments cannot be ignored"
            );
            args.truncate(6);
        }
    }

    #[test]
    fn development_object_envelope_never_consumes_truncated_installed_markers() {
        let mut args: Vec<String> = ["host", "guardian", "/stage", "entry", "1", "2", "1", "{}"]
            .map(String::from)
            .into();
        let (_, local, body) =
            parse_guardian_launch_arguments(&args).expect("opaque object envelope");
        assert!(local.is_none());
        assert_eq!(body.as_deref(), Some("{}"));
        // Object validity remains deferred: this proves framing does not parse/mint facts.
        assert!(
            keld_core::app_session::DevelopmentLaunchFacts::from_private_argument(
                "{}",
                std::path::Path::new("/stage")
            )
            .is_err()
        );
        args[7] = "{not-json}".to_owned();
        let (_, _, body) = parse_guardian_launch_arguments(&args).expect("opaque invalid JSON");
        assert_eq!(body.as_deref(), Some("{not-json}"));
        assert!(
            keld_core::app_session::DevelopmentLaunchFacts::from_private_argument(
                body.as_deref().expect("body"),
                std::path::Path::new("/stage")
            )
            .is_err()
        );
        args[7] = format!("{{{}}}", " ".repeat(16_382));
        assert_eq!(args[7].len(), 16_384);
        assert!(parse_guardian_launch_arguments(&args).is_ok());
        args[7] = format!("{{{}}}", " ".repeat(16_383));
        assert_eq!(args[7].len(), 16_385);
        assert!(parse_guardian_launch_arguments(&args).is_err());
        for marker in [
            "local-bundle-v1",
            "local-bundle-v2",
            "local-bundle-v3",
            "{",
            "}",
        ] {
            args[7] = marker.to_owned();
            assert!(
                parse_guardian_launch_arguments(&args).is_err(),
                "truncated/unknown carrier {marker}"
            );
        }
    }

    #[test]
    fn local_guardian_suffix_is_closed_and_does_not_change_development_arguments() {
        let base: Vec<String> = ["host", "guardian", "root", "entry", "1", "2", "1"]
            .map(String::from)
            .into();
        let (mode, pins, _) = parse_guardian_launch_arguments(&base).expect("existing dev shape");
        assert!(mode);
        assert!(pins.is_none());

        let mut local = base.clone();
        local.extend([
            "local-bundle-v1".to_owned(),
            "ab".repeat(32),
            "cd".repeat(32),
            "ef".repeat(32),
        ]);
        let (mode, pins, _) = parse_guardian_launch_arguments(&local).expect("closed local inputs");
        assert!(mode);
        let local_launch = pins.expect("local expected pins");
        assert!(local_launch.host_facts.is_none());
        let pins = local_launch.pins;
        assert_eq!(pins.bun_sha256(), &[0xab; 32]);
        assert_eq!(pins.boot_sha256(), &[0xcd; 32]);
        assert_eq!(pins.permissions_sha256(), &[0xef; 32]);
        for length in [8, 9, 10] {
            assert!(parse_guardian_launch_arguments(&local[..length]).is_err());
        }
        for (index, value) in [
            (6, "true".to_owned()),
            (7, "local-bundle-v2".to_owned()),
            (8, "AB".repeat(32)),
            (9, "cd".repeat(31)),
            (10, "not-a-digest".to_owned()),
        ] {
            let mut mutated = local.clone();
            mutated[index] = value;
            assert!(parse_guardian_launch_arguments(&mutated).is_err());
        }
        local.push("ignored-extra".to_owned());
        assert!(parse_guardian_launch_arguments(&local).is_err());
    }

    #[test]
    fn original_local_guardian_accepts_only_a_bounded_versioned_handoff() {
        let mut args: Vec<String> = [
            "host",
            "guardian",
            "root",
            "entry",
            "1",
            "2",
            "2",
            "local-bundle-v2",
        ]
        .map(String::from)
        .into();
        args.extend(["ab".repeat(32), "cd".repeat(32), "ef".repeat(32)]);
        // The outer dispatcher may bound this value, but must not decode native
        // data before Runtime authenticates and retains the Host registration.
        args.push("not-yet-authenticated-json".to_owned());
        let (facade, local, _) = parse_guardian_launch_arguments(&args)
            .expect("closed role-v2 carries opaque bounded data until authentication");
        assert!(facade);
        assert!(local.is_some());
        for (index, value) in [
            (6, "0".to_owned()),
            (6, "1".to_owned()),
            (6, "02".to_owned()),
            (7, "local-bundle-v1".to_owned()),
            (8, "AB".repeat(32)),
            (11, String::new()),
            (11, "x".repeat(16_385)),
        ] {
            let mut changed = args.clone();
            changed[index] = value;
            assert!(parse_guardian_launch_arguments(&changed).is_err());
        }
        assert!(parse_guardian_launch_arguments(&args[..11]).is_err());
        args.push("extra".to_owned());
        assert!(parse_guardian_launch_arguments(&args).is_err());
        args.pop();
        args[11] = "x".repeat(16_384);
        assert!(parse_guardian_launch_arguments(&args).is_ok());
    }

    #[test]
    fn installed_original_selects_verified_root_preload_before_true_main() {
        let root = std::path::Path::new("/Applications/Example.app/Contents/Resources/app");
        let wrapper = root.join("node_modules/@keld/api/runtime-preload.js");
        let bootstrap = root.join("keld-installed-runtime-entry.mjs");
        let wrong_same_root = root.join("unverified-same-root.mjs");
        let mut command = std::process::Command::new("/verified/bun");
        command.arg("--preload=/verified/immutable-facts.ts");
        super::configure_installed_runtime_bootstrap(
            &mut command,
            root,
            true,
            Some(&wrapper),
            Some(&bootstrap),
        )
        .expect("both independently admitted graph members");
        super::append_installed_main(&mut command, root, std::path::Path::new("main.js"));
        let observed = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            vec![
                "--preload=/verified/immutable-facts.ts".to_owned(),
                "--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\"".to_owned(),
                "--define=__KELD_RUNTIME_PROCESS_FACTS__:true".to_owned(),
                format!("--preload={}", bootstrap.display()),
                "run".to_owned(),
                root.join("main.js").display().to_string(),
            ]
        );
        let mut ordinary = std::process::Command::new("/verified/bun");
        super::append_installed_main(&mut ordinary, root, std::path::Path::new("main.js"));
        assert_eq!(
            ordinary
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["run".to_owned(), root.join("main.js").display().to_string()]
        );
        for (facade, wrapper, bootstrap) in [
            (false, Some(wrapper.as_path()), Some(bootstrap.as_path())),
            (true, None, Some(bootstrap.as_path())),
            (true, Some(wrapper.as_path()), None),
            (
                true,
                Some(wrapper.as_path()),
                Some(std::path::Path::new("/unverified/bootstrap.mjs")),
            ),
            (
                true,
                Some(wrapper.as_path()),
                Some(wrong_same_root.as_path()),
            ),
        ] {
            let mut refused = std::process::Command::new("/verified/bun");
            assert!(
                super::configure_installed_runtime_bootstrap(
                    &mut refused,
                    root,
                    facade,
                    wrapper,
                    bootstrap,
                )
                .is_err(),
                "mismatched source or facade must fail closed"
            );
            assert_eq!(
                refused.get_args().count(),
                0,
                "refusal must precede command mutation"
            );
        }
    }

    #[test]
    fn guardian_rejects_runtime_config_without_a_transport_sidecar() {
        let temp = tempfile::tempdir().expect("config admission fixture");
        let entry = temp.path().join("entry.ts");
        fs::write(&entry, "console.log('ENTRY_REACHED');\n").expect("entry fixture");
        let identity = fs::metadata(&entry).expect("entry identity");
        for native_facade in [false, true] {
            for name in ["bunfig.toml", ".bunfig.toml"] {
                let config = temp.path().join(name);
                fs::write(
                    &config,
                    "[define]\n\"process.env.KELD_NATIVE_WINDOW_FACADE\" = '\"0\"'\n",
                )
                .expect("hostile staged config");
                let error = guardian_bun_command(
                    &Ok(temp.path().to_path_buf()),
                    temp.path(),
                    std::path::Path::new("entry.ts"),
                    identity.dev(),
                    identity.ino(),
                    (native_facade, None),
                    "/tmp/link#token",
                )
                .expect_err("runtime config must refuse before entry execution");
                assert!(
                    error
                        .to_string()
                        .contains(&format!("runtime config {name}")),
                    "{error}"
                );
                fs::remove_file(config).expect("restore stage");
            }
        }
    }

    #[test]
    fn guardian_removes_inherited_bun_options_before_the_canonical_preload() {
        let temp = tempfile::tempdir().expect("inherited preload fixture");
        let entry = temp.path().join("entry.ts");
        fs::write(&entry, "console.log('ENTRY_REACHED');\n").expect("entry fixture");
        let identity = fs::metadata(&entry).expect("entry identity");
        let command = guardian_bun_command(
            &Ok(temp.path().to_path_buf()),
            temp.path(),
            std::path::Path::new("entry.ts"),
            identity.dev(),
            identity.ino(),
            (true, None),
            "/tmp/link#token",
        )
        .expect("qualified command");
        assert!(
            command
                .get_envs()
                .any(|(name, value)| name == "BUN_OPTIONS" && value.is_none()),
            "ambient Bun arguments must be removed at the actual child command"
        );
    }

    #[test]
    fn guardian_inherited_preload_cannot_run_before_qualified_capture() {
        let temp = tempfile::tempdir().expect("actual Bun inherited preload fixture");
        fs::create_dir(temp.path().join("src")).expect("sidecar directory");
        fs::write(
            temp.path().join("src/kipc-transport.ts"),
            "console.log('QUALIFIED_CAPTURE');\n",
        )
        .expect("independent preload order marker");
        fs::write(
            temp.path().join("injected.ts"),
            "console.log('INHERITED_PRELOAD_EXECUTED');\n",
        )
        .expect("independent hostile preload marker");
        fs::write(
            temp.path().join("entry.ts"),
            "console.log('ORIGINAL_ENTRY_EXECUTED');\n",
        )
        .expect("independent original entry marker");
        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "tests::guardian_inherited_preload_child",
                "--nocapture",
            ])
            .env("KELD_GUARDIAN_PRELOAD_FIXTURE", temp.path())
            .env(
                "BUN_OPTIONS",
                format!("--preload={}", temp.path().join("injected.ts").display()),
            )
            .output()
            .expect("isolated inherited-environment child");
        assert!(
            output.status.success(),
            "Bun command child failed: {output:?}"
        );
        let stdout = String::from_utf8(output.stdout).expect("marker output");
        assert!(
            stdout.contains("QUALIFIED_CAPTURE\nORIGINAL_ENTRY_EXECUTED\n"),
            "{stdout}"
        );
        assert!(!stdout.contains("INHERITED_PRELOAD_EXECUTED"), "{stdout}");
    }

    #[test]
    fn guardian_inherited_preload_child() {
        let Some(root) = std::env::var_os("KELD_GUARDIAN_PRELOAD_FIXTURE") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let identity = fs::metadata(root.join("entry.ts")).expect("entry identity");
        let output = guardian_bun_command(
            &Ok(root.clone()),
            &root,
            std::path::Path::new("entry.ts"),
            identity.dev(),
            identity.ino(),
            (true, None),
            "/tmp/link#token",
        )
        .expect("actual shipping Bun command")
        .output()
        .expect("execute actual Bun command");
        assert!(output.status.success(), "Bun failed: {output:?}");
        print!("{}", String::from_utf8(output.stdout).expect("Bun stdout"));
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
