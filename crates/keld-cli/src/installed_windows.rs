//! Installed Windows CLI launch, separate from the developer-stage entry.
//!
//! The signed CLI may select only its adjacent signed Host. The Host must be
//! the exact independently pinned image in the current protected selected
//! `PerUserDirect` package. This command neither creates an installation nor
//! authorizes boot: the no-flag Host reopens and verifies its own installation.
use keld_cli::boot;
use keld_guard::WindowsAuthenticodeImage;
use keld_update::{ExpectedAppIdentity, WindowsLocatedImage, select_active_package_for_executable};
use sha2::{Digest as _, Sha256};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::Path;
use std::process::Command;

fn refuse(phase: &'static str, detail: impl std::fmt::Display) -> String {
    format!("KELD-CLI-048: installed Windows CLI {phase} refused: {detail}")
}

fn valid_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(refuse(
            "arguments",
            "installed launch accepts no flags, project locators or developer leases",
        ))
    }
}

fn pin_host_sha256(image: &std::fs::File, expected: [u8; 32]) -> Result<(), String> {
    let mut file = image
        .try_clone()
        .map_err(|error| refuse("Host image handle", error))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| refuse("Host image seek", error))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let len = file
            .read(&mut buffer)
            .map_err(|error| refuse("Host image digest", error))?;
        if len == 0 {
            break;
        }
        digest.update(&buffer[..len]);
    }
    let actual: [u8; 32] = digest.finalize().into();
    if actual != expected {
        return Err(refuse(
            "Host image digest",
            "the adjacent signed Host differs from the independently qualified final image",
        ));
    }
    Ok(())
}

fn exact_cli_leaf(path: &Path) -> Result<(), String> {
    if path.file_name().and_then(|part| part.to_str()) != Some("keld.exe") {
        return Err(refuse(
            "entry",
            "only the signed keld.exe in the exact installed tree may select a Host",
        ));
    }
    Ok(())
}

/// Launches only the selected signed installed Host; the Host independently
/// authenticates protected provenance and the bundled runtime before startup.
///
/// A developer bundle does not have the required KEL-53 installation and
/// cannot enter via this route.
pub(super) fn run(args: &[String]) -> Result<(), String> {
    valid_args(args)?;
    let cli_path = std::env::current_exe().map_err(|error| refuse("self locator", error))?;
    exact_cli_leaf(&cli_path)?;
    let root = cli_path
        .parent()
        .ok_or_else(|| refuse("installation", "CLI parent missing"))?;
    // Keep volume-root to leaf ancestry non-reparse and non-delete-sharing
    // throughout the child run. A signer check on the leaf alone is insufficient.
    let _guards = boot::pin_windows_launch_ancestors(root)
        .map_err(|error| refuse("installation ancestors", error))?;
    let cli = WindowsAuthenticodeImage::open(&cli_path)
        .and_then(WindowsAuthenticodeImage::verify)
        .map_err(|error| refuse("CLI signature", error))?;
    let host_path = root.join("keld-host.exe");
    let host = WindowsAuthenticodeImage::open(&host_path)
        .and_then(WindowsAuthenticodeImage::verify)
        .map_err(|error| refuse("Host signature", error))?;
    if cli.identity().app_id() != host.identity().app_id()
        || cli.identity().publisher_scope() != host.identity().publisher_scope()
    {
        return Err(refuse(
            "image pair",
            "the signed CLI and Host differ in app id or publisher scope",
        ));
    }
    let expected_digest = boot::qualified_windows_host_image_sha256().ok_or_else(|| {
        refuse(
            "distribution pin",
            "no independently qualified signed Windows Host image was compiled",
        )
    })?;
    pin_host_sha256(host.file(), expected_digest)?;
    let expected = ExpectedAppIdentity::from_signed_image(host.file())
        .map_err(|error| refuse("Host embedded expectation", error))?;
    // This strict existing KEL-53 selector proves protected provenance, exact
    // selected version, signed Host image identity and the publisher/app pair.
    // It does not treat the CLI signature as installation authority.
    let _selected_package = select_active_package_for_executable(
        WindowsLocatedImage::Host,
        &host_path,
        host.file(),
        &expected,
        host.identity().publisher_scope(),
        host.identity().app_id(),
    )
    .map_err(|error| refuse("protected installed package", error))?;
    // No "dev" flag, no stage graph or lease, and no PATH-injected Bun. The
    // installed Host must itself pin its bundled Bun; CLI cannot create that
    // authority by selecting an executable or forwarding a developer context.
    let status = Command::new(&host_path)
        .current_dir(root)
        .env_remove("KELD_DEV_LEASE")
        .status()
        .map_err(|error| refuse("Host start", error))?;
    if !status.success() {
        return Err(refuse("Host exit", status));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{exact_cli_leaf, pin_host_sha256, valid_args};
    use sha2::{Digest as _, Sha256};
    use std::io::Write as _;
    use std::path::Path;

    #[test]
    fn rejects_all_cli_arguments_before_installation_access() {
        valid_args(&[]).unwrap();
        for values in [
            vec!["dev".to_owned()],
            vec!["--allow-unsigned".to_owned()],
            vec!["--root".to_owned(), "C:/foreign".to_owned()],
            vec!["--keld-install-initial-v3".to_owned()],
        ] {
            assert!(valid_args(&values).is_err());
        }
    }

    #[test]
    fn independently_pinned_host_digest_rejects_changed_image() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("host-for-digest.bin");
        std::fs::File::create(&path)
            .unwrap()
            .write_all(b"original signed image fixture bytes")
            .unwrap();
        let image = std::fs::File::open(&path).unwrap();
        let correct: [u8; 32] = Sha256::digest(b"original signed image fixture bytes").into();
        pin_host_sha256(&image, correct).expect("byte-identical exact image");
        let mut wrong = correct;
        wrong[0] ^= 1;
        assert!(pin_host_sha256(&image, wrong).is_err());
        // The earlier success read this same file handle to EOF: the check
        // explicitly rewinds and cannot accidentally hash an empty suffix.
        pin_host_sha256(&image, correct).expect("position does not change authority");
    }

    #[test]
    fn only_named_installed_cli_selects_the_host() {
        assert!(exact_cli_leaf(Path::new(r"C:\package\tree\keld.exe")).is_ok());
        for name in ["other.exe", "keld-host.exe", "KELD.EXE", "keld.cmd"] {
            assert!(exact_cli_leaf(Path::new(r"C:\package\tree").join(name).as_path()).is_err());
        }
    }
}
