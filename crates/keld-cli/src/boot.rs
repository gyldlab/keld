//! Non-release owner-private boot stage compiler (KEL-96/T1a/T4).

#[cfg(any(target_os = "macos", windows))]
use std::fs::{self, File, OpenOptions};
#[cfg(any(target_os = "macos", windows))]
use std::io::{self, Read, Seek, SeekFrom, Write};
#[cfg(any(target_os = "macos", windows))]
use std::path::Component;
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "macos", windows))]
use sha2::{Digest, Sha256};
#[cfg(windows)]
use windows_permissions::constants::{
    AccessRights, AceFlags, AceType, SeObjectType, SecurityInformation,
};
#[cfg(windows)]
use windows_permissions::utilities::current_process_sid;
#[cfg(windows)]
use windows_permissions::wrappers::{ConvertSidToStringSid, GetNamedSecurityInfo};
#[cfg(windows)]
use windows_permissions::{LocalBox, SecurityDescriptor};
#[cfg(windows)]
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

#[cfg(any(target_os = "macos", windows))]
const PERMISSIONS_BYTES: &[u8] = b"{}\n";
#[cfg(any(target_os = "macos", windows))]
const PERMISSIONS_FILE: &str = "keld.permissions.jsonc";
#[cfg(any(target_os = "macos", windows))]
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

/// One completed owner-private non-release app stage.
#[derive(Debug)]
pub struct DevBootStage {
    root: PathBuf,
    host: PathBuf,
    #[cfg(windows)]
    _launch_guards: Vec<File>,
}

impl DevBootStage {
    /// Canonical per-launch app root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Staged no-flag host executable.
    #[must_use]
    pub fn host(&self) -> &Path {
        &self.host
    }
}

/// Failure while compiling the non-release boot stage.
#[derive(Debug)]
pub struct BootCompileError {
    phase: &'static str,
    detail: String,
}

impl BootCompileError {
    fn new(phase: &'static str, detail: impl Into<String>) -> Self {
        Self {
            phase,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for BootCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-CLI-047: boot staging failed during {} — {}. \
             Fix the project files and regenerate a fresh owner-private dev stage.",
            self.phase, self.detail
        )
    }
}

impl std::error::Error for BootCompileError {}

#[cfg(any(target_os = "macos", windows))]
struct StageGuard {
    root: PathBuf,
    keep: bool,
}

#[cfg(any(target_os = "macos", windows))]
impl Drop for StageGuard {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

/// Compiles one fresh, non-release owner-private platform boot stage.
///
/// This is a tooling surface: it creates the bytes consumed by the exact
/// `keld_core::app_session` host API, but cannot select boot input inside the
/// host process or mint application authority.
///
/// # Errors
///
/// Returns [`BootCompileError`] for unsupported platforms, unsafe/missing
/// project inputs, random-source failure, stage I/O, or host copy mismatch.
pub fn stage_dev_boot(
    project_root: &Path,
    developer_host: &Path,
) -> Result<DevBootStage, BootCompileError> {
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (project_root, developer_host);
        Err(BootCompileError::new(
            "platform availability",
            "the boot compiler supports macOS and Windows; complete KEL-96/T4 for this platform",
        ))
    }
    #[cfg(any(target_os = "macos", windows))]
    {
        stage_dev_boot_platform(project_root, developer_host)
    }
}

#[cfg(any(target_os = "macos", windows))]
#[allow(clippy::too_many_lines)] // one atomic staging transaction keeps cleanup and integrity checks contiguous
fn stage_dev_boot_platform(
    project_root: &Path,
    developer_host: &Path,
) -> Result<DevBootStage, BootCompileError> {
    #[cfg(target_os = "macos")]
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let project_root = project_root
        .canonicalize()
        .map_err(|source| BootCompileError::new("project root", source.to_string()))?;
    let config = fs::read_to_string(project_root.join("keld.config.ts"))
        .map_err(|source| BootCompileError::new("project config", source.to_string()))?;
    let name = keld_core::title_from_config_ts(&config)
        .unwrap_or_else(|| keld_core::DEFAULT_HELLO_TITLE.to_owned());
    let entry = keld_core::entry_from_config_ts(&config)
        .unwrap_or_else(|| keld_core::DEFAULT_ENTRY.to_owned());
    let renderer = keld_core::renderer_from_config_ts(&config)
        .unwrap_or_else(|| keld_core::DEFAULT_RENDERER.to_owned());
    validate_relative("entry", &entry)?;
    validate_relative("renderer", &renderer)?;

    let entry_source = contained_source(&project_root, &entry, "entry")?;
    let renderer_source = contained_source(&project_root, &renderer, "renderer")?;
    let developer_host = developer_host
        .canonicalize()
        .map_err(|source| BootCompileError::new("developer host", source.to_string()))?;
    let mut host_source = File::open(&developer_host)
        .map_err(|source| BootCompileError::new("developer host", source.to_string()))?;
    let host_source_metadata = host_source
        .metadata()
        .map_err(|source| BootCompileError::new("developer host", source.to_string()))?;
    if !host_source_metadata.is_file() {
        return Err(BootCompileError::new(
            "developer host",
            "source must be a regular file",
        ));
    }
    #[cfg(target_os = "macos")]
    if host_source_metadata.permissions().mode() & 0o100 == 0 {
        return Err(BootCompileError::new(
            "developer host",
            "source must be owner-executable",
        ));
    }

    let dev_root = project_root.join(".keld/dev");
    fs::create_dir_all(&dev_root)
        .map_err(|source| BootCompileError::new("dev root", source.to_string()))?;
    let root = create_launch_root(&dev_root)?;
    let mut guard = StageGuard {
        root: root.clone(),
        keep: false,
    };

    #[cfg(target_os = "macos")]
    let staged_host = root.join("keld-host");
    #[cfg(windows)]
    let staged_host = root.join("keld-host.exe");
    let source_digest = copy_host(&mut host_source, &staged_host)?;
    let staged_digest = digest_file(&staged_host, "staged host")?;
    if source_digest != staged_digest {
        return Err(BootCompileError::new(
            "host copy integrity",
            "staged host digest differs from the already-open source bytes",
        ));
    }
    #[cfg(target_os = "macos")]
    let staged_metadata = fs::metadata(&staged_host)
        .map_err(|source| BootCompileError::new("staged host", source.to_string()))?;
    #[cfg(target_os = "macos")]
    if (host_source_metadata.dev(), host_source_metadata.ino())
        == (staged_metadata.dev(), staged_metadata.ino())
    {
        return Err(BootCompileError::new(
            "host copy integrity",
            "staged host reuses the source inode",
        ));
    }
    #[cfg(target_os = "macos")]
    {
        let read_execute_mode = (host_source_metadata.permissions().mode() & 0o555) | 0o100;
        fs::set_permissions(&staged_host, fs::Permissions::from_mode(read_execute_mode)).map_err(
            |source| BootCompileError::new("staged host permissions", source.to_string()),
        )?;
    }

    stage_project_file(&root, &entry, &entry_source, "entry")?;
    stage_project_file(&root, &renderer, &renderer_source, "renderer")?;
    write_new_file(
        &root.join(PERMISSIONS_FILE),
        PERMISSIONS_BYTES,
        0o400,
        "permissions",
    )?;
    let digest = Sha256::digest(PERMISSIONS_BYTES);
    let descriptor = serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "name": name,
        "entry": entry,
        "renderer": renderer,
        "permissions": {
            "file": PERMISSIONS_FILE,
            "content_sha256": format!("sha256:{digest:x}"),
        }
    }))
    .map_err(|source| BootCompileError::new("boot descriptor", source.to_string()))?;
    write_new_file(
        &root.join("keld.boot.json"),
        &descriptor,
        0o400,
        "boot descriptor",
    )?;

    #[cfg(target_os = "macos")]
    {
        let mode = fs::metadata(&root)
            .map_err(|source| BootCompileError::new("stage mode", source.to_string()))?
            .permissions()
            .mode()
            & 0o7777;
        if mode != 0o700 {
            return Err(BootCompileError::new(
                "stage mode",
                format!("expected 0o700, found 0o{mode:o}"),
            ));
        }
    }
    #[cfg(windows)]
    let launch_guards = retain_windows_launch_guards(&project_root, &dev_root, &root)?;
    #[cfg(windows)]
    {
        verify_windows_stage_acl(&root)?;
        let locked_digest = digest_file(&staged_host, "locked staged host")?;
        if source_digest != locked_digest {
            return Err(BootCompileError::new(
                "host copy integrity",
                "locked staged host digest differs from the already-open source bytes",
            ));
        }
    }
    guard.keep = true;
    Ok(DevBootStage {
        root,
        host: staged_host,
        #[cfg(windows)]
        _launch_guards: launch_guards,
    })
}

#[cfg(windows)]
fn retain_windows_launch_guards(
    project_root: &Path,
    dev_root: &Path,
    stage_root: &Path,
) -> Result<Vec<File>, BootCompileError> {
    let keld_root = dev_root.parent().ok_or_else(|| {
        BootCompileError::new("launch namespace", "the dev root has no .keld parent")
    })?;
    [project_root, keld_root, dev_root, stage_root]
        .into_iter()
        .map(open_windows_launch_guard)
        .collect()
}

#[cfg(windows)]
fn open_windows_launch_guard(path: &Path) -> Result<File, BootCompileError> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};

    let guard = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|source| BootCompileError::new("launch namespace", source.to_string()))?;
    let metadata = guard
        .metadata()
        .map_err(|source| BootCompileError::new("launch namespace", source.to_string()))?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(BootCompileError::new(
            "launch namespace",
            format!("{} is not a real directory", path.display()),
        ));
    }
    Ok(guard)
}

#[cfg(any(target_os = "macos", windows))]
fn create_launch_root(dev_root: &Path) -> Result<PathBuf, BootCompileError> {
    #[cfg(target_os = "macos")]
    use std::os::unix::fs::DirBuilderExt;

    for _ in 0..8 {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce)
            .map_err(|source| BootCompileError::new("launch nonce", source.to_string()))?;
        let mut name = String::with_capacity(32);
        for byte in nonce {
            name.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
            name.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
        }
        let root = dev_root.join(name);
        #[cfg(target_os = "macos")]
        let created = fs::DirBuilder::new().mode(0o700).create(&root);
        #[cfg(windows)]
        let created = create_windows_stage_root(&root);
        match created {
            Ok(()) => return Ok(root),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(BootCompileError::new(
                    "launch directory",
                    source.to_string(),
                ));
            }
        }
    }
    Err(BootCompileError::new(
        "launch nonce",
        "eight random launch-directory collisions",
    ))
}

#[cfg(any(target_os = "macos", windows))]
fn validate_relative(kind: &'static str, value: &str) -> Result<(), BootCompileError> {
    if value.is_empty()
        || value.contains('\\')
        || value.contains(':')
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || Path::new(value)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(BootCompileError::new(
            kind,
            "path is not portable and project-relative",
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn contained_source(
    project_root: &Path,
    relative: &str,
    kind: &'static str,
) -> Result<PathBuf, BootCompileError> {
    let path = project_root
        .join(relative)
        .canonicalize()
        .map_err(|source| BootCompileError::new(kind, source.to_string()))?;
    if !path.starts_with(project_root) {
        return Err(BootCompileError::new(
            kind,
            "target escapes the project root",
        ));
    }
    let metadata =
        fs::metadata(&path).map_err(|source| BootCompileError::new(kind, source.to_string()))?;
    if !metadata.is_file() {
        return Err(BootCompileError::new(kind, "target is not a regular file"));
    }
    Ok(path)
}

#[cfg(any(target_os = "macos", windows))]
fn copy_host(source: &mut File, destination: &Path) -> Result<[u8; 32], BootCompileError> {
    #[cfg(target_os = "macos")]
    use std::os::unix::fs::OpenOptionsExt;

    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| BootCompileError::new("developer host", error.to_string()))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(target_os = "macos")]
    options.mode(0o700);
    let mut destination = options
        .open(destination)
        .map_err(|error| BootCompileError::new("staged host", error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| BootCompileError::new("developer host", error.to_string()))?;
        if read == 0 {
            break;
        }
        destination
            .write_all(&buffer[..read])
            .map_err(|error| BootCompileError::new("staged host", error.to_string()))?;
        hasher.update(&buffer[..read]);
    }
    destination
        .sync_all()
        .map_err(|error| BootCompileError::new("staged host", error.to_string()))?;
    Ok(hasher.finalize().into())
}

#[cfg(any(target_os = "macos", windows))]
fn digest_file(path: &Path, kind: &'static str) -> Result<[u8; 32], BootCompileError> {
    let mut file =
        File::open(path).map_err(|error| BootCompileError::new(kind, error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| BootCompileError::new(kind, error.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(any(target_os = "macos", windows))]
fn stage_project_file(
    root: &Path,
    relative: &str,
    source: &Path,
    kind: &'static str,
) -> Result<(), BootCompileError> {
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| BootCompileError::new(kind, error.to_string()))?;
    }
    let bytes = fs::read(source).map_err(|error| BootCompileError::new(kind, error.to_string()))?;
    write_new_file(&destination, &bytes, 0o400, kind)
}

#[cfg(any(target_os = "macos", windows))]
fn write_new_file(
    path: &Path,
    bytes: &[u8],
    mode: u32,
    kind: &'static str,
) -> Result<(), BootCompileError> {
    #[cfg(target_os = "macos")]
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(target_os = "macos")]
    options.mode(mode);
    #[cfg(windows)]
    let _ = mode;
    let mut file = options
        .open(path)
        .map_err(|error| BootCompileError::new(kind, error.to_string()))?;
    file.write_all(bytes)
        .map_err(|error| BootCompileError::new(kind, error.to_string()))?;
    file.sync_all()
        .map_err(|error| BootCompileError::new(kind, error.to_string()))
}

#[cfg(windows)]
#[allow(unsafe_code)] // reviewed Windows-only atomic creation boundary; see SAFETY proof at CreateDirectoryW
fn create_windows_stage_root(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;

    let current = current_process_sid()?;
    let sid = ConvertSidToStringSid(&current)?;
    let descriptor: LocalBox<SecurityDescriptor> = format!(
        "O:{}D:P(A;OICI;FA;;;{})",
        sid.to_string_lossy(),
        sid.to_string_lossy()
    )
    .parse()?;
    let mut path_wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    path_wide.push(0);
    let length =
        u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).map_err(io::Error::other)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: length,
        lpSecurityDescriptor: descriptor.as_ptr().cast(),
        bInheritHandle: 0,
    };
    // SAFETY: `path_wide` is a live NUL-terminated UTF-16 buffer;
    // `attributes` is correctly sized and points to the live self-relative
    // descriptor owned by `descriptor`. Neither allocation moves or drops
    // until CreateDirectoryW returns, and handle inheritance is disabled.
    if unsafe { CreateDirectoryW(path_wide.as_ptr(), &raw const attributes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if let Err(error) = verify_windows_stage_acl_for(path, &current) {
        let _ = fs::remove_dir(path);
        return Err(io::Error::other(error));
    }
    Ok(())
}

#[cfg(windows)]
fn verify_windows_stage_acl(path: &Path) -> Result<(), BootCompileError> {
    let current = current_process_sid()
        .map_err(|source| BootCompileError::new("stage DACL TokenUser", source.to_string()))?;
    verify_windows_stage_acl_for(path, &current)
}

#[cfg(windows)]
fn verify_windows_stage_acl_for(
    path: &Path,
    current: &windows_permissions::Sid,
) -> Result<(), BootCompileError> {
    let descriptor = GetNamedSecurityInfo(
        path.as_os_str(),
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
    .map_err(|source| BootCompileError::new("stage DACL readback", source.to_string()))?;
    if descriptor.owner() != Some(current) {
        return Err(BootCompileError::new(
            "stage DACL readback",
            "owner does not equal the current process TokenUser SID",
        ));
    }
    let sddl = descriptor
        .as_sddl()
        .map_err(|source| BootCompileError::new("stage DACL readback", source.to_string()))?;
    if !sddl.to_string_lossy().contains("D:P") {
        return Err(BootCompileError::new(
            "stage DACL readback",
            "DACL inheritance is not protected",
        ));
    }
    let dacl = descriptor.dacl().ok_or_else(|| {
        BootCompileError::new(
            "stage DACL readback",
            "security descriptor contains no DACL",
        )
    })?;
    if dacl.len() != 1 {
        return Err(BootCompileError::new(
            "stage DACL readback",
            format!("expected one access rule, found {}", dacl.len()),
        ));
    }
    let ace = dacl.get_ace(0).ok_or_else(|| {
        BootCompileError::new("stage DACL readback", "the one access rule is unreadable")
    })?;
    let required_flags = AceFlags::ContainerInherit | AceFlags::ObjectInherit;
    if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE
        || ace.mask() != AccessRights::FileAllAccess
        || ace.sid() != Some(current)
        || ace.flags() != required_flags
    {
        return Err(BootCompileError::new(
            "stage DACL readback",
            "expected one non-inherited current-user full-control rule for files and directories",
        ));
    }
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
#[allow(clippy::expect_used, clippy::panic)] // unit-test fixture failures are assertion oracles
mod tests {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use sha2::{Digest, Sha256};

    use super::*;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let temp = tempfile::tempdir().expect("temp root");
        let project = temp.path().join("project");
        fs::create_dir_all(project.join("src")).expect("project src");
        fs::write(
            project.join("keld.config.ts"),
            "export default {\n  name: \"Stage Fixture\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n} as const;\n",
        )
        .expect("config");
        fs::write(project.join("src/main.ts"), "console.log('fixture');\n").expect("entry");
        fs::write(project.join("index.html"), "<p id=exact>fixture</p>\n").expect("renderer");
        let host = temp.path().join("developer-keld-host");
        fs::write(&host, b"developer-host-bytes").expect("host");
        fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).expect("host mode");
        (temp, project, host)
    }

    #[test]
    fn stage_is_random_owner_private_new_inode_and_byte_consistent() {
        let (_temp, project, source_host) = fixture();
        let staged = stage_dev_boot(&project, &source_host).expect("stage boot fixture");
        let nonce = staged.root().file_name().expect("nonce").to_string_lossy();
        assert_eq!(nonce.len(), 32);
        assert!(
            nonce
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
        assert_eq!(
            fs::metadata(staged.root())
                .expect("root metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::read(staged.root().join("keld.permissions.jsonc")).expect("permissions"),
            b"{}\n"
        );
        assert_eq!(
            fs::read(staged.root().join("src/main.ts")).expect("entry"),
            b"console.log('fixture');\n"
        );
        assert_eq!(
            fs::read(staged.root().join("index.html")).expect("renderer"),
            b"<p id=exact>fixture</p>\n"
        );

        let source_metadata = fs::metadata(&source_host).expect("source metadata");
        let staged_metadata = fs::metadata(staged.host()).expect("staged metadata");
        assert_ne!(
            (source_metadata.dev(), source_metadata.ino()),
            (staged_metadata.dev(), staged_metadata.ino()),
            "hard links are forbidden"
        );
        assert_eq!(
            fs::read(&source_host).expect("source"),
            fs::read(staged.host()).expect("staged")
        );
        assert_ne!(staged_metadata.permissions().mode() & 0o100, 0);
        assert_eq!(staged_metadata.permissions().mode() & 0o222, 0);

        let descriptor: serde_json::Value = serde_json::from_slice(
            &fs::read(staged.root().join("keld.boot.json")).expect("boot descriptor"),
        )
        .expect("parse descriptor");
        assert_eq!(descriptor["schema"], 1);
        assert_eq!(descriptor["name"], "Stage Fixture");
        assert_eq!(descriptor["entry"], "src/main.ts");
        assert_eq!(descriptor["renderer"], "index.html");
        assert_eq!(descriptor["permissions"]["file"], "keld.permissions.jsonc");
        let digest = Sha256::digest(b"{}\n");
        assert_eq!(
            descriptor["permissions"]["content_sha256"],
            format!("sha256:{digest:x}")
        );
    }

    #[test]
    fn staging_twice_uses_distinct_launch_roots() {
        let (_temp, project, source_host) = fixture();
        let first = stage_dev_boot(&project, &source_host).expect("first");
        let second = stage_dev_boot(&project, &source_host).expect("second");
        assert_ne!(first.root(), second.root());
    }

    #[test]
    fn failed_stage_removes_partial_launch_directory() {
        let (_temp, project, source_host) = fixture();
        fs::write(project.join("keld.permissions.jsonc"), "entry collision\n")
            .expect("colliding entry source");
        fs::write(
            project.join("keld.config.ts"),
            "export default {\n  name: \"Stage Fixture\",\n  entry: \"keld.permissions.jsonc\",\n  renderer: \"index.html\",\n} as const;\n",
        )
        .expect("collision config");
        let error = stage_dev_boot(&project, &source_host)
            .expect_err("fixed permissions destination collision must fail");
        assert!(error.to_string().contains("permissions"), "{error}");
        let dev_root = project.join(".keld/dev");
        let children = fs::read_dir(&dev_root).map_or(0, std::iter::Iterator::count);
        assert_eq!(
            children, 0,
            "post-root failure leaked a partial launch stage"
        );
    }
}
