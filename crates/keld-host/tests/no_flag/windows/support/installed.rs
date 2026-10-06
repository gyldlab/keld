//! Fresh per-run installed packages for the KEL-254 T3 Part B rows.
//!
//! Each [`InstalledApp`] is a real `PerUserDirect` installation that keld-update's
//! production per-user initializer seeds, through its `kel254_per_user_install_fixture`
//! selector, around an operator-signed host that embeds its expected app identity. Its
//! tree holds the boot files the boot compiler produced from this run's project, so a
//! renderer may name this run's port-0 listeners. This is fixture construction: the
//! product oracles stay in the scenarios.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Names the keld-update unit-test executable that holds the install fixture.
pub(crate) const INSTALLER_ENV: &str = "KELD_KEL254_INSTALLER_FIXTURE";
/// An embedded host (fixture app id) signed by the KEL-135 acceptance publisher P1.
pub(crate) const SIGNED_HOST_A_P1: &str = "KELD_KEL254_SIGNED_HOST_A_P1";
const INSTALL_SELECTOR: &str =
    "windows_baseline::tests::installed_host_operator::kel254_per_user_install_fixture";
const RECEIPT_PREFIX: &str = "KELD_KEL254_INSTALLED_HOST ";
const HOST: &str = "keld-host.exe";

/// The operator fixture path that `name` must carry.
pub(crate) fn fixture_env(name: &str) -> OsString {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} must name a KEL-254 installed-host operator fixture"))
}

/// One fresh per-user installation. Dropping it removes the installation, so it must
/// outlive every host launched from it.
pub(crate) struct InstalledApp {
    host: PathBuf,
    app_id: String,
    root: Option<tempfile::TempDir>,
}

impl InstalledApp {
    /// Installs `signed_host` with the boot files compiled from `project`; the record
    /// names the host's embedded app id and its own verified publisher.
    pub(crate) fn install(project: &Path, signed_host: &OsStr, installer: &OsStr) -> Self {
        Self::install_recording(project, signed_host, installer, None)
    }

    /// [`Self::install`] whose record names the verified publisher of
    /// `recorded_publisher`, when given, instead of the installed host's.
    pub(crate) fn install_recording(
        project: &Path,
        signed_host: &OsStr,
        installer: &OsStr,
        recorded_publisher: Option<&OsStr>,
    ) -> Self {
        let local = PathBuf::from(
            std::env::var_os("LOCALAPPDATA").expect("the current user's LocalAppData"),
        );
        let root = tempfile::Builder::new()
            .prefix("keld-kel254-")
            .tempdir_in(&local)
            .expect("fresh per-run installation root beneath LocalAppData");
        // The boot compiler is the one producer of the strict descriptor and its
        // targets; the developer host it stages beside them is not installed.
        let stage =
            keld_cli::boot::stage_dev_boot(project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
                .expect("compile this run's boot files");
        let source = root.path().join("boot-source");
        fs::create_dir(&source).expect("create the boot source directory");
        copy_boot_files(stage.root(), &source, true);
        drop(stage);
        let parent = root.path().join("install");
        fs::create_dir(&parent).expect("create a new, empty install parent");

        let mut command = Command::new(installer);
        command
            .args([
                INSTALL_SELECTOR,
                "--ignored",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("KELD_KEL254_SIGNED_HOST", signed_host)
            .env("KELD_KEL254_BOOT_SOURCE", &source)
            .env("KELD_KEL254_INSTALL_PARENT", &parent);
        match recorded_publisher {
            Some(publisher) => command.env("KELD_KEL254_RECORD_PUBLISHER_HOST", publisher),
            None => command.env_remove("KELD_KEL254_RECORD_PUBLISHER_HOST"),
        };
        let output = command
            .output()
            .expect("run the keld-update per-user install fixture");
        let stdout = String::from_utf8(output.stdout).expect("install fixture stdout UTF-8");
        assert!(
            output.status.success(),
            "the install fixture failed with {}\nstdout:\n{stdout}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        // The fixture's receipt, not exit zero, proves the selector ran and installed.
        let receipt = stdout
            .lines()
            .find_map(|line| line.strip_prefix(RECEIPT_PREFIX))
            .unwrap_or_else(|| panic!("the install fixture printed no receipt:\n{stdout}"));
        let receipt: serde_json::Value =
            serde_json::from_str(receipt).expect("install receipt JSON");
        let host = PathBuf::from(receipt["host"].as_str().expect("installed host path"));
        assert!(
            host.starts_with(&parent) && host.file_name() == Some(OsStr::new(HOST)),
            "the receipt names this run's installed keld-host.exe: {}",
            host.display()
        );
        let app_id = receipt["app_id"]
            .as_str()
            .expect("recorded app id")
            .to_owned();
        println!(
            "KELD_KEL254_INSTALLED host={} app_id={app_id} publisher_scope={}",
            host.display(),
            receipt["publisher_scope"]
        );
        Self {
            host,
            app_id,
            root: Some(root),
        }
    }

    /// The app id the installation's protected record names.
    pub(crate) fn recorded_app_id(&self) -> &str {
        &self.app_id
    }

    /// A lease-less launch of the installed host from its tree, as a user starts it.
    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new(&self.host);
        command
            .current_dir(self.host.parent().expect("the installed host has its tree"))
            .env_remove("KELD_DEV_LEASE");
        command
    }
}

impl Drop for InstalledApp {
    fn drop(&mut self) {
        // Harness cleanup only; report rather than hide an installation left behind.
        if let Some(root) = self.root.take() {
            let path = root.path().to_path_buf();
            if let Err(error) = root.close() {
                eprintln!(
                    "KELD_KEL254_INSTALL_CLEANUP_FAILED path={} error={error}",
                    path.display()
                );
            }
        }
    }
}

/// Copies the compiled boot files, leaving out the staged developer host.
fn copy_boot_files(source: &Path, destination: &Path, top_level: bool) {
    for entry in fs::read_dir(source).expect("read the compiled boot files") {
        let entry = entry.expect("compiled boot entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("compiled entry kind").is_dir() {
            fs::create_dir(&target).expect("create a boot source directory");
            copy_boot_files(&entry.path(), &target, false);
        } else if !(top_level && entry.file_name() == HOST) {
            fs::copy(entry.path(), &target).expect("copy a compiled boot file");
        }
    }
}
