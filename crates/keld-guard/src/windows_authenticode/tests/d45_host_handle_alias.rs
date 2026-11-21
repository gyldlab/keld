//! D-45 native Windows pathname substitution tests for the existing Guard owner.
//!
//! An ordinary parent rename is denied while a child is held with `FILE_SHARE_READ`.
//! The fixture therefore uses a disposable directory junction as an ancestor
//! alias. Renaming that junction leaves the original file HANDLE alive, while
//! its old absolute pathname can name a different file. The ordinary test
//! uses a genuinely trusted embedded-signed third-party image. The ignored
//! positive requires an independently pinned, trusted Keld-signed Host.

use std::fs::{self, File};
use std::os::windows::fs::{FileExt as _, MetadataExt as _};
use std::os::windows::io::AsRawHandle as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
};

use super::{
    FILE_ATTRIBUTE_REPARSE_POINT, FixtureDirectory, WindowsAuthenticodeError,
    WindowsAuthenticodeImage, WindowsD45DirectDiscoveryEligibility, sha256_retained_windows_image,
};

/// Own only the junction leaves, never either target directory.
struct TestJunctions {
    visible: PathBuf,
    retired: PathBuf,
}

impl TestJunctions {
    fn new(root: &Path, target: &Path) -> Self {
        let links = Self {
            visible: root.join("visible"),
            retired: root.join("retired"),
        };
        create_directory_junction(&links.visible, target);
        links
    }

    fn substitute(&self, new_target: &Path) {
        fs::rename(&self.visible, &self.retired)
            .expect("move only the disposable ancestor junction while Host remains held");
        create_directory_junction(&self.visible, new_target);
    }

    fn host_path(&self) -> PathBuf {
        self.visible.join("keld-host.exe")
    }
}

impl Drop for TestJunctions {
    fn drop(&mut self) {
        for link in [&self.visible, &self.retired] {
            if let Err(error) = fs::remove_dir(link)
                && error.kind() != std::io::ErrorKind::NotFound
                && !std::thread::panicking()
            {
                panic!("remove isolated junction {}: {error}", link.display());
            }
        }
    }
}

fn create_directory_junction(link: &Path, target: &Path) {
    assert!(!link.exists(), "a test junction must be new");
    assert!(target.is_dir(), "a junction target must already exist");
    // Windows symlink_dir requires privilege on these test hosts. PowerShell
    // creates a local directory junction without elevation; no policy is changed.
    // Single-quoted PowerShell literals escape every apostrophe in the path.
    let quote = |path: &Path| -> String {
        let value = path
            .to_str()
            .expect("the Windows test fixture path is Unicode");
        format!("'{}'", value.replace(char::from(39), "''"))
    };
    let script = format!(
        "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path {} -Target {} | Out-Null",
        quote(link),
        quote(target)
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("start the native Windows junction constructor");
    assert!(
        output.status.success(),
        "junction creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = fs::symlink_metadata(link).expect("the junction leaf is published");
    assert_ne!(
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT,
        0,
        "the new ancestor must be a reparse link, not a normal directory"
    );
}

#[allow(unsafe_code)] // Test-only file-ID observer; no production verifier or installer FFI.
fn windows_file_identity(file: &File) -> (u32, u64) {
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: file owns a live readable Windows HANDLE, and info is writable
    // storage large enough for the Win32 fixed-size output structure.
    let success =
        unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), info.as_mut_ptr()) };
    assert_ne!(
        success,
        0,
        "query the retained Windows file identity: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: the successful API call initialized the entire output structure.
    let info = unsafe { info.assume_init() };
    (
        info.dwVolumeSerialNumber,
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    )
}

fn path_identity(path: &Path) -> (u32, u64) {
    let file = File::open(path).expect("open test-only path for identity");
    windows_file_identity(&file)
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut value, "{byte:02x}").expect("format a fixed byte digest");
    }
    value
}

fn checked_foreign_authenticode_carrier() -> PathBuf {
    let carrier = std::env::var_os("KELD_TEST_FOREIGN_AUTHENTICODE_CARRIER").map_or_else(
        || {
            PathBuf::from(std::env::var_os("ProgramFiles").expect("native Windows ProgramFiles"))
                .join("Git/cmd/git.exe")
        },
        PathBuf::from,
    );
    assert!(
        carrier.is_file(),
        "a real embedded-signed Git fixture must exist"
    );
    carrier
}

fn assert_authenticated_non_keld_refusal(error: &WindowsAuthenticodeError) {
    assert!(
        matches!(
            error.detail(),
            "the verified Authenticode signature has no Keld app-id description"
                | "the verified signature has no authenticated Keld app-id attribute"
        ),
        "expected WinTrust to verify the original signed carrier before rejecting its app ID: {error}"
    );
    assert!(
        error.close_failure().is_none(),
        "the provider state must close after identity refusal: {error}"
    );
}

/// The reopening mutant is real, not an invented signature result: the original
/// signed file reaches authenticated app-ID extraction, whereas the alias path
/// resolves to unsigned bytes after substitution and fails `WinVerifyTrust`.
#[test]
fn d45_wintrust_retains_original_signed_hfile_after_parent_alias_substitution() {
    let directory = FixtureDirectory::new("d45-hfile-replacement-negative");
    let original_dir = directory.path().join("original");
    let foreign_dir = directory.path().join("foreign");
    fs::create_dir(&original_dir).expect("create original fixture directory");
    fs::create_dir(&foreign_dir).expect("create foreign fixture directory");
    let original = original_dir.join("keld-host.exe");
    let foreign = foreign_dir.join("keld-host.exe");
    let signed_carrier = checked_foreign_authenticode_carrier();
    fs::copy(&signed_carrier, &original).expect("copy genuinely signed carrier into the fixture");
    fs::write(&foreign, b"unsigned foreign pathname substitution")
        .expect("write the unsigned substitution fixture");

    let baseline = WindowsAuthenticodeImage::open(&original)
        .expect("open the original embedded-signed image")
        .verify()
        .expect_err("the trusted non-Keld signer does not mint a Keld app identity");
    assert_authenticated_non_keld_refusal(&baseline);

    let junctions = TestJunctions::new(directory.path(), &original_dir);
    let visible = junctions.host_path();
    let pinned =
        WindowsAuthenticodeImage::open(&visible).expect("Guard retains the original signed file");
    let retained_identity = windows_file_identity(&pinned.image);
    let original_len = fs::metadata(&original)
        .expect("the original signed source has a known fixture length")
        .len();
    let original_digest = sha256_retained_windows_image(&pinned.image, original_len)
        .expect("hash original signed Host from its retained handle");
    assert_eq!(retained_identity, path_identity(&original));

    junctions.substitute(&foreign_dir);
    assert_eq!(
        path_identity(&visible),
        path_identity(&foreign),
        "the same pathname must now resolve to the foreign file"
    );
    assert_ne!(
        retained_identity,
        path_identity(&visible),
        "the retained original HANDLE must not name the substituted path object"
    );
    assert_eq!(
        sha256_retained_windows_image(&pinned.image, original_len)
            .expect("substituting the path cannot change the original handle's signed bytes"),
        original_digest
    );
    assert_ne!(
        sha256_retained_windows_image(
            &File::open(&visible).expect("open the foreign alias for the negative control"),
            fs::metadata(&visible)
                .expect("foreign image metadata")
                .len(),
        )
        .expect("the replacement has readable bytes"),
        original_digest,
        "a path reopen must observe the foreign unsigned bytes"
    );
    let size = usize::try_from(fs::metadata(&original).expect("original image").len())
        .expect("signed Git fixture fits the test address space");
    let mut observed = vec![0_u8; size];
    let count = pinned
        .image
        .seek_read(&mut observed, 0)
        .expect("read original signed bytes through retained hFile");
    assert_eq!(count, observed.len());
    assert_eq!(
        observed,
        fs::read(&original).expect("read original from relocated target")
    );
    assert_eq!(
        fs::read(&visible).expect("read current alias target"),
        b"unsigned foreign pathname substitution"
    );

    let pinned_refusal = pinned
        .verify()
        .expect_err("a trusted third-party signer still cannot mint the Keld app identity");
    assert_authenticated_non_keld_refusal(&pinned_refusal);

    let reopened_refusal = WindowsAuthenticodeImage::open(&visible)
        .expect("foreign pathname remains a regular file")
        .verify()
        .expect_err("the unsigned pathname must fail native WinTrust");
    assert!(
        reopened_refusal
            .detail()
            .contains("WinVerifyTrust rejected"),
        "the path-reopen negative control must reach signature verification: {reopened_refusal}"
    );
}

#[test]
#[ignore = "requires an independently pinned trusted Keld-signed Host, KELD_D45_SIGNED_HOST, KELD_D45_SIGNED_HOST_SHA256, KELD_D45_SIGNED_APP_ID and KELD_D45_SIGNED_PUBLISHER_SCOPE"]
fn d45_trusted_signed_host_verifies_original_hfile_after_path_substitution() {
    use sha2::Digest as _;

    let fixture = PathBuf::from(
        std::env::var_os("KELD_D45_SIGNED_HOST")
            .expect("a previously signed, trusted Host is required"),
    );
    let expected_sha = std::env::var("KELD_D45_SIGNED_HOST_SHA256")
        .expect("independent release Host SHA256 is required")
        .to_ascii_lowercase();
    let expected_app =
        std::env::var("KELD_D45_SIGNED_APP_ID").expect("independent release app ID is required");
    let expected_publisher = std::env::var("KELD_D45_SIGNED_PUBLISHER_SCOPE")
        .expect("independent publisher scope is required")
        .to_ascii_lowercase();
    assert_eq!(expected_sha.len(), 64, "expect a SHA256 digest");
    assert_eq!(expected_publisher.len(), 64, "expect a 32-byte scope");
    let original_bytes = fs::read(&fixture).expect("read independently released signed Host");
    let original_sha = sha2::Sha256::digest(&original_bytes);
    let actual_sha = hex_bytes(&original_sha);
    assert_eq!(
        actual_sha, expected_sha,
        "signed Host differs from the independent build record"
    );

    let directory = FixtureDirectory::new("d45-signed-hfile-positive");
    let original_dir = directory.path().join("original");
    let foreign_dir = directory.path().join("foreign");
    fs::create_dir(&original_dir).expect("create original fixture directory");
    fs::create_dir(&foreign_dir).expect("create foreign fixture directory");
    let original = original_dir.join("keld-host.exe");
    let foreign = foreign_dir.join("keld-host.exe");
    fs::write(&original, &original_bytes).expect("create exact signed Host test copy");
    fs::write(&foreign, b"unsigned foreign Host").expect("create foreign unsigned replacement");
    let junctions = TestJunctions::new(directory.path(), &original_dir);
    let visible = junctions.host_path();
    let pinned = WindowsAuthenticodeImage::open(&visible).expect("retain the signed Host handle");
    let original_id = windows_file_identity(&pinned.image);
    assert_eq!(original_id, path_identity(&original));
    junctions.substitute(&foreign_dir);
    assert_ne!(original_id, path_identity(&visible));

    // This ignored operator case is restricted to a genuinely commissioned
    // Keld-signed carrier. It exercises source continuity only; it supplies no
    // D-43 Direct decision or v2 issuer/profile commissioning evidence.
    let preflight = pinned
        .observe_preflight()
        .expect("local offline WinTrust must accept the signed original Host");
    assert_eq!(windows_file_identity(preflight.file()), original_id);
    let mut independently_pinned_digest = [0_u8; 32];
    independently_pinned_digest.copy_from_slice(&original_sha);
    let verified = preflight
        .verify_full_bound(WindowsD45DirectDiscoveryEligibility {
            original_host_sha256: independently_pinned_digest,
            expected_host_bytes: u64::try_from(original_bytes.len())
                .expect("the signed fixture length fits u64"),
        })
        .expect("full WinTrust must independently verify the same retained signed hFile");
    assert_eq!(verified.identity().app_id(), expected_app);
    let actual_publisher = hex_bytes(verified.identity().publisher_scope());
    assert_eq!(actual_publisher, expected_publisher);
    assert_eq!(windows_file_identity(verified.file()), original_id);
    let mut observed = vec![0_u8; original_bytes.len()];
    let count = verified
        .file()
        .seek_read(&mut observed, 0)
        .expect("the verified file still supplies the original signed content");
    assert_eq!(count, original_bytes.len());
    assert_eq!(observed, original_bytes);
    assert_eq!(fs::read(&visible).unwrap(), b"unsigned foreign Host");
    drop(verified);

    let reopened = WindowsAuthenticodeImage::open(&visible)
        .expect("open the now-substituted path")
        .verify()
        .expect_err("the unsigned path must never inherit the original verified identity");
    assert!(
        reopened.detail().contains("WinVerifyTrust rejected"),
        "path-reopen mutant must be refused by the native trust provider: {reopened}"
    );
}
