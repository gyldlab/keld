//! keld-host — the shipping host binary (pre-alpha).
//!
//! Destination (spec 01/06): app developers never compile this; `@keld/cli`
//! resolves a signed platform build that boots from the compiled form of
//! `keld.config.ts` and owns every OS resource for the app's lifetime.
//! `--hello` remains an unprivileged diagnostic. A no-flag launch consumes the
//! private validated stage beside this executable and owns the application
//! session until ordered shutdown.

use std::env;
#[cfg(target_os = "macos")]
use std::fs::{self, File};
#[cfg(target_os = "macos")]
use std::io::{self, Write};
#[cfg(target_os = "macos")]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::process;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = env::args().collect();
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

    #[cfg(target_os = "macos")]
    let dev_stage_cleanup = dev_stage_cleanup_root();
    let result = keld_core::app_session::ValidatedBootSelection::from_current_exe_unprivileged()
        .and_then(keld_core::app_session::run_guarded);
    let mut failed = false;
    if let Err(error) = result {
        eprintln!("{error}");
        failed = true;
    }
    #[cfg(target_os = "macos")]
    if let Some(root) = dev_stage_cleanup
        && let Err(source) = fs::remove_dir_all(&root)
    {
        eprintln!(
            "KELD-CORE-037: dev-stage cleanup failed for `{}` — {source}. Remove that owner-private nonce directory before relaunching.",
            root.display()
        );
        failed = true;
    }
    if failed {
        process::exit(1);
    }
}

#[cfg(target_os = "macos")]
fn dev_stage_cleanup_root() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let lease = env::var_os("KELD_DEV_LEASE");
    dev_stage_cleanup_root_for(&executable, lease.as_deref())
}

#[cfg(target_os = "macos")]
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
    let report = keld_runtime::macos_guardian::run_guarded_primary(
        std::io::stdin(),
        move |app_link| {
            let reopened = reopen_validated_entry(&root, &entry, expected_dev, expected_ino)?;
            // Advisory same-user dev-boundary check: retain the exact reopened
            // identity until immediately before Supervisor invokes spawn, then
            // close it. Bun resolves the name afterward, so this does not claim
            // release-grade resistance to a same-user replacement in that
            // residual window; the signed-container successor must close it.
            drop(reopened);
            let mut command = Command::new("bun");
            command
                .arg("run")
                .arg(root.join(&entry))
                .current_dir(&root)
                .env("KELD_APP_LINK", app_link)
                .stdin(Stdio::null());
            Ok(command)
        },
        std::io::stdout(),
    )
    .map_err(|error| {
        format!(
            "KELD-CORE-037: supervised Bun guardian failed — {error}. Fix the Bun app failure and relaunch the no-flag host."
        )
    })?;
    std::io::stderr()
        .write_all(report.stdout.as_bytes())
        .and_then(|()| std::io::stderr().write_all(report.stderr.as_bytes()))
        .map_err(|source| format!("KELD-CORE-037: guardian stderr failed — {source}. Retry."))?;
    Ok(())
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

    use super::{dev_stage_cleanup_root_for, reopen_validated_entry};

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
