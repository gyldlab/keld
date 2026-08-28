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
use std::fs::File;
#[cfg(target_os = "macos")]
use std::io::{self, Write};
#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt;
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
        // The argv value is only a role discriminator. `run_supervised` must
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

    let result = keld_core::app_session::ValidatedBootSelection::from_current_exe_unprivileged()
        .and_then(keld_core::app_session::run_unprivileged);
    if let Err(error) = result {
        eprintln!("{error}");
        process::exit(1);
    }
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
    let app_link = env::var("KELD_APP_LINK").map_err(|source| {
        format!(
            "KELD-CORE-037: private guardian app link is unavailable — {source}. Launch it only through the validated no-flag host."
        )
    })?;
    if app_link.is_empty() {
        return Err(String::from(
            "KELD-CORE-037: private guardian app link is empty. Launch it only through the validated no-flag host.",
        ));
    }
    let report = keld_runtime::macos_guardian::run_supervised(
        std::io::stdin(),
        move || {
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
                .env("KELD_APP_LINK", &app_link)
                .stdin(Stdio::null());
            Ok(command)
        },
        std::io::stdout(),
        || Ok(()),
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
    use std::fs;
    use std::os::unix::fs::MetadataExt as _;

    use super::reopen_validated_entry;

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
