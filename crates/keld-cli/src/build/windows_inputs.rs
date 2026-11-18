//! Native, read-only Windows application-authoring admission.
//!
//! This explicit check creates no package, stage, process, profile or grant.

use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::{Component, Path, PathBuf, Prefix};

use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

use super::{BuildError, config};
use crate::boot;

const AUTHORING_LIMIT: u64 = 64 * 1024;

fn has_windows_trailing_alias(name: &OsStr) -> bool {
    matches!(name.as_encoded_bytes().last(), Some(b'.' | b' '))
}

/// Return the stable drive path of the exact directory handle retained last
/// by `pin_input_root`. A mapped drive may be rebound while that handle remains
/// open; callers must use this path rather than reusing the original alias.
pub(super) fn stable_local_path(pins: &[File]) -> Result<PathBuf, BuildError> {
    let root = pins
        .last()
        .ok_or_else(|| BuildError::input("Windows local volume", "missing retained root handle"))?;
    keld_update::fixed_local_dos_path(root)
        .map_err(|error| BuildError::input("Windows local volume", error))
}

/// Turn a caller's leaf into a path beneath the already pinned parent.
/// Never use the original drive-letter alias for the later file open.
pub(super) fn stable_file_at_parent(path: &Path, parents: &[File]) -> Result<PathBuf, BuildError> {
    let Some(Component::Normal(leaf)) = path.components().next_back() else {
        return Err(BuildError::input(
            "Windows local file",
            "regular file leaf required",
        ));
    };
    if has_windows_trailing_alias(leaf) {
        return Err(BuildError::input(
            "Windows local file",
            "ambiguous trailing dot/space leaf",
        ));
    }
    Ok(stable_local_path(parents)?.join(leaf))
}

/// Resolve a directory that may be created later from its nearest existing,
/// retained fixed-local ancestor. The returned path no longer depends on a
/// mapped drive alias and the pins keep the selected namespace alive.
pub(super) fn stable_local_target_path(path: &Path) -> Result<(PathBuf, Vec<File>), BuildError> {
    let mut components = path.components();
    if !path.is_absolute()
        || !matches!(components.next(), Some(Component::Prefix(prefix))
        if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        || components
            .any(|component| !matches!(component, Component::RootDir | Component::Normal(_)))
    {
        return Err(BuildError::input(
            "Windows local target",
            "use an absolute local-disk target without parent traversal",
        ));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::Normal(name) if has_windows_trailing_alias(name)))
    {
        return Err(BuildError::input(
            "Windows local target",
            "ambiguous trailing dot/space target component",
        ));
    }
    let mut suffix = Vec::<OsString>::new();
    let mut cursor = path;
    loop {
        match std::fs::symlink_metadata(cursor) {
            Ok(_) => {
                let pins = pin_input_root(cursor)?;
                let mut stable = stable_local_path(&pins)?;
                for component in suffix.iter().rev() {
                    stable.push(component);
                }
                return Ok((stable, pins));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = cursor.file_name().ok_or_else(|| {
                    BuildError::input("Windows local target", "no existing ancestor")
                })?;
                suffix.push(name.to_os_string());
                cursor = cursor.parent().ok_or_else(|| {
                    BuildError::input("Windows local target", "no existing ancestor")
                })?;
            }
            Err(error) => return Err(BuildError::input("Windows local target", error)),
        }
    }
}

pub(super) struct CapturedInputs {
    _ancestors: Vec<File>,
    root: PathBuf,
    config: File,
    permissions: File,
}

pub(super) fn pin_input_root(root: &Path) -> Result<Vec<File>, BuildError> {
    // Do not canonicalize: following a junction and then checking the target
    // would erase the evidence that the selected namespace was redirected.
    let mut components = root.components();
    if !root.is_absolute()
        || !matches!(components.next(), Some(Component::Prefix(prefix))
        if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        || components
            .any(|component| !matches!(component, Component::RootDir | Component::Normal(_)))
    {
        return Err(BuildError::input(
            "Windows authoring namespace",
            "use an absolute local-disk project directory without parent traversal",
        ));
    }
    let mut ancestors = Vec::new();
    for path in root.ancestors().collect::<Vec<_>>().into_iter().rev() {
        let guard = boot::open_windows_input_directory_guard(path)
            .map_err(|error| BuildError::input("Windows authoring namespace", error))?;
        keld_update::fixed_local_dos_path(&guard)
            .map_err(|error| BuildError::input("Windows local volume", error))?;
        ancestors.push(guard);
    }
    Ok(ancestors)
}

impl CapturedInputs {
    pub(super) fn open(root: &Path) -> Result<Self, BuildError> {
        let ancestors = pin_input_root(root)?;
        let root = stable_local_path(&ancestors)?;
        boot::ensure_current_principal_owns(&root)
            .map_err(|error| BuildError::input("Windows project owner", error))?;
        let config = open_leaf(&root, "keld.config.ts")?;
        let permissions = open_leaf(&root, "keld.permissions.jsonc")?;
        Ok(Self {
            _ancestors: ancestors,
            root,
            config,
            permissions,
        })
    }

    pub(super) fn check(self, root: &Path) -> Result<config::AppConfig, BuildError> {
        Ok(self.capture(root)?.app)
    }

    /// Retain literal authoring bytes and their Windows namespace witnesses.
    pub(super) fn capture(mut self, _root: &Path) -> Result<CapturedAuthoring, BuildError> {
        let root = self.root.clone();
        let mut source = Vec::new();
        (&mut self.config)
            .take(AUTHORING_LIMIT + 1)
            .read_to_end(&mut source)
            .map_err(|error| BuildError::input("keld.config.ts", error))?;
        if source.len() as u64 > AUTHORING_LIMIT {
            return Err(BuildError::input("keld.config.ts", "input exceeds 64 KiB"));
        }
        let source = std::str::from_utf8(&source)
            .map_err(|error| BuildError::input("keld.config.ts", error))?;
        let app = config::parse_app(source)?;
        require_windows_authoring_paths(&app.entry, app.renderer.as_deref())?;
        if app.renderer.is_none() {
            return Err(BuildError::input(
                "Windows application cell",
                "only renderer-driven authoring is checked; initialWindow:none is unsupported",
            ));
        }
        let policy = keld_guard::read_manifest_bytes(
            &mut self.permissions,
            &root.join("keld.permissions.jsonc"),
        )
        .map_err(|error| BuildError::input("keld.permissions.jsonc", error))?;
        let policy_text = std::str::from_utf8(&policy)
            .map_err(|error| BuildError::input("keld.permissions.jsonc", error))?;
        let manifest = keld_guard::parse_manifest(policy_text)
            .map_err(|error| BuildError::input("keld.permissions.jsonc", error))?;
        if manifest.declared_profile() != Some(keld_guard::DeclaredProfile::Strict) {
            return Err(BuildError::input(
                "Windows declared profile",
                "explicit /profile:strict is required; a declaration is not runtime qualification",
            ));
        }
        // The compiler is the sole owner of relative boot paths/schema and the
        // digest of these exact policy bytes. Drop no source pins until it ends.
        let _descriptor = boot::compile_boot_descriptor_bytes(
            source,
            &app.name,
            &app.entry,
            app.renderer.as_deref(),
            &policy,
        )
        .map_err(|error| BuildError::input("Windows boot input compiler", error))?;
        Ok(CapturedAuthoring {
            config_source: source.to_owned(),
            permissions: policy,
            app,
            inputs: self,
        })
    }
}

/// No module-graph, Bun or runtime qualification is implied by this value.
/// The admission handles remain alive while boot bytes and package members are
/// assembled; consumer code must not reopen either source path.
pub(super) struct CapturedAuthoring {
    pub(super) app: config::AppConfig,
    config_source: String,
    permissions: Vec<u8>,
    inputs: CapturedInputs,
}

impl CapturedAuthoring {
    /// The authoring input owner retains the handles behind this fixed-local path.
    pub(super) fn stable_root(&self) -> &Path {
        &self.inputs.root
    }

    /// Stage only the two framework-owned authoring outputs. The caller must
    /// separately supply and admit the complete module/Bun/image inventory.
    pub(super) fn archive_members(
        &self,
    ) -> Result<Vec<super::windows_archive::Member>, BuildError> {
        use super::windows_archive::{Content, Member};
        // The same captured declared entry is compiled by the closed graph owner
        // into fixed code/main.js. This descriptor does not rename source imports.
        if self.app.renderer.as_deref() != Some("index.html") {
            return Err(BuildError::input(
                "Windows archive renderer",
                "this proposed archive cell requires root index.html",
            ));
        }
        let descriptor = boot::compile_boot_descriptor_bytes(
            &self.config_source,
            &self.app.name,
            "code/main.js",
            Some("renderer/index.html"),
            &self.permissions,
        )
        .map_err(|error| BuildError::input("Windows archive boot", error))?;
        Ok(vec![
            Member {
                name: "keld.boot.json".into(),
                content: Some(Content::Generated(descriptor)),
            },
            Member {
                name: "keld.permissions.jsonc".into(),
                content: Some(Content::Generated(self.permissions.clone())),
            },
        ])
    }
}

fn open_leaf(root: &Path, name: &'static str) -> Result<File, BuildError> {
    let path = root.join(name);
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)
        .map_err(|error| BuildError::input(name, error))?;
    let stable_path = keld_update::fixed_local_dos_path(&file)
        .map_err(|error| BuildError::input("Windows local volume", error))?;
    let metadata = file
        .metadata()
        .map_err(|error| BuildError::input(name, error))?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(BuildError::input(
            name,
            "input must be a non-reparse regular file",
        ));
    }
    boot::ensure_current_principal_owns(&stable_path)
        .map_err(|error| BuildError::input(name, error))?;
    Ok(file)
}

pub(super) fn run() -> Result<(), BuildError> {
    let root = std::env::current_dir()
        .map_err(|error| BuildError::input("Windows project directory", error))?;
    let app = CapturedInputs::open(&root)?.check(&root)?;
    println!(
        "Checked Windows app authoring: {:?} ({:?}) version {:?}; entry {:?}; renderer {:?}.",
        app.name,
        app.id,
        app.version,
        app.entry,
        app.renderer.as_deref().unwrap_or_default(),
    );
    println!(
        "Input check only: no application was built, signed, installed or launched. \
         keld.build.ts, executable/module bytes, release authenticity and runtime qualification were not checked."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    const APP: &str = "export default {name:'Input check',id:'com.example.check',version:'1.0.0',entry:'main.js',renderer:'index.html'};";

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "keld-win-input-check-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&root).expect("fresh fixture");
            fs::write(root.join("keld.config.ts"), APP).expect("config");
            fs::write(
                root.join("keld.permissions.jsonc"),
                br#"{"profile":"strict"}"#,
            )
            .expect("policy");
            Self(root)
        }
        fn check(&self) -> Result<config::AppConfig, BuildError> {
            CapturedInputs::open(&self.0)?.check(&self.0)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove exact test-owned fixture");
        }
    }

    #[test]
    fn valid_inputs_use_shared_compiler_without_creating_output() {
        let fixture = Fixture::new();
        let app = fixture.check().expect("valid authoring");
        assert_eq!(app.id, "com.example.check");
        assert_eq!(fs::read_dir(&fixture.0).expect("census").count(), 2);
        assert_eq!(
            fs::read_to_string(fixture.0.join("keld.config.ts")).expect("read"),
            APP
        );
    }

    #[test]
    fn retained_authoring_binds_exact_policy_to_archive_boot() {
        use super::super::windows_archive::Content;
        let fixture = Fixture::new();
        let literal = b"{ \"profile\": \"strict\" }\n";
        fs::write(fixture.0.join("keld.permissions.jsonc"), literal).unwrap();
        let captured = CapturedInputs::open(&fixture.0)
            .unwrap()
            .capture(&fixture.0)
            .unwrap();
        // Retaining the authoring value must retain write/delete share denial.
        assert!(
            OpenOptions::new()
                .write(true)
                .open(fixture.0.join("keld.permissions.jsonc"))
                .is_err()
        );
        let members = captured.archive_members().unwrap();
        let Some(Content::Generated(policy)) = &members[1].content else {
            panic!("literal policy");
        };
        assert_eq!(policy, literal);
        let Some(Content::Generated(boot)) = &members[0].content else {
            panic!("compiled boot");
        };
        let boot: serde_json::Value = serde_json::from_slice(boot).unwrap();
        assert_eq!(boot["entry"], "code/main.js");
        assert_eq!(boot["renderer"], "renderer/index.html");
        assert_eq!(
            boot["permissions"]["content_sha256"],
            format!("sha256:{:x}", Sha256::digest(literal))
        );
        assert_eq!(
            boot["schema"],
            keld_core::app_session::boot_schema(false, Some(keld_guard::DeclaredProfile::Strict))
        );
        drop(captured);
        assert!(
            OpenOptions::new()
                .write(true)
                .open(fixture.0.join("keld.permissions.jsonc"))
                .is_ok()
        );
    }

    #[test]
    fn archive_descriptor_keeps_fixed_output_for_declared_nested_source() {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("keld.config.ts"),
            APP.replace("main.js", "src/main.js"),
        )
        .unwrap();
        let captured = CapturedInputs::open(&fixture.0)
            .unwrap()
            .capture(&fixture.0)
            .unwrap();
        assert_eq!(captured.app.entry, "src/main.js");
        assert!(captured.archive_members().is_ok()); // Actual graph capture/compile is separately required.
    }

    #[test]
    fn archive_cell_refuses_unwitnessed_renderer_relocation() {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("keld.config.ts"),
            APP.replace("index.html", "ui/home.html"),
        )
        .unwrap();
        let captured = CapturedInputs::open(&fixture.0)
            .unwrap()
            .capture(&fixture.0)
            .unwrap();
        assert!(captured.archive_members().is_err());
    }

    #[test]
    fn absent_or_legacy_policy_never_synthesizes_strict() {
        let fixture = Fixture::new();
        for policy in [r"{}", r#"{"profile":"legacy"}"#] {
            fs::write(fixture.0.join("keld.permissions.jsonc"), policy).expect("policy");
            assert!(
                fixture
                    .check()
                    .expect_err("strict required")
                    .to_string()
                    .contains("explicit /profile:strict")
            );
        }
        fs::remove_file(fixture.0.join("keld.permissions.jsonc")).expect("remove policy");
        assert!(fixture.check().is_err());
    }

    #[test]
    fn dynamic_unknown_facade_and_escaped_path_refuse() {
        let fixture = Fixture::new();
        for source in [
            "export default makeConfig();".to_owned(),
            APP.replace("version:", "unknown:true,version:"),
            APP.replace(",renderer:'index.html'", ",initialWindow:'none'"),
            APP.replace("entry:'main.js'", "entry:'../outside.js'"),
        ] {
            fs::write(fixture.0.join("keld.config.ts"), source).expect("config");
            assert!(fixture.check().is_err());
        }
    }

    #[test]
    fn authoring_limit_is_exact_and_invalid_utf8_refuses() {
        let fixture = Fixture::new();
        let mut boundary = APP.as_bytes().to_vec();
        boundary.resize(
            usize::try_from(AUTHORING_LIMIT).expect("authoring bound fits usize"),
            b' ',
        );
        fs::write(fixture.0.join("keld.config.ts"), &boundary).expect("boundary");
        fixture.check().expect("exact bound");
        boundary.push(b' ');
        fs::write(fixture.0.join("keld.config.ts"), boundary).expect("oversized");
        assert!(
            fixture
                .check()
                .expect_err("bound")
                .to_string()
                .contains("64 KiB")
        );
        fs::write(fixture.0.join("keld.config.ts"), [0xff]).expect("invalid UTF8");
        assert!(fixture.check().is_err());
    }

    #[test]
    fn retained_sources_deny_write_and_replacement_until_capture_finishes() {
        let fixture = Fixture::new();
        let writable_directory = || {
            OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS)
                .open(&fixture.0)
        };
        drop(writable_directory().expect("positive control before capture"));
        let capture = CapturedInputs::open(&fixture.0).expect("capture");
        for name in ["keld.config.ts", "keld.permissions.jsonc"] {
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(fixture.0.join(name))
                    .is_err()
            );
            assert!(
                fs::rename(
                    fixture.0.join(name),
                    fixture.0.join(format!("{name}.moved"))
                )
                .is_err()
            );
        }
        assert!(fs::rename(&fixture.0, fixture.0.with_extension("moved")).is_err());
        // A retained ancestor must deny the writable directory handle required
        // for reparse metadata mutation, not merely deny rename/delete.
        let error = writable_directory().expect_err("capture denies writable directory handle");
        assert_eq!(
            error.raw_os_error(),
            Some(
                i32::try_from(windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION)
                    .expect("Windows error fits i32")
            ),
        );
        capture.check(&fixture.0).expect("retained exact inputs");
        drop(writable_directory().expect("positive control after release"));
        assert!(
            OpenOptions::new()
                .write(true)
                .open(fixture.0.join("keld.config.ts"))
                .is_ok()
        );
    }

    #[test]
    fn directory_in_place_of_config_refuses() {
        let fixture = Fixture::new();
        fs::remove_file(fixture.0.join("keld.config.ts")).expect("remove config");
        fs::create_dir(fixture.0.join("keld.config.ts")).expect("directory");
        assert!(fixture.check().is_err());
    }
}

fn require_windows_authoring_paths(entry: &str, renderer: Option<&str>) -> Result<(), BuildError> {
    let mut paths = vec![entry];
    if let Some(renderer) = renderer {
        paths.push(renderer);
    }
    keld_guard::validate_windows_package_paths(&paths)
        .map_err(|error| BuildError::input("Windows authoring paths", error))
}
#[cfg(test)]
mod windows_local_volume_tests {
    use super::*;
    use sha2::Digest as _;
    use std::process::{Command, ExitStatus};
    use std::sync::Mutex;

    // Never allow two native fixture lifecycles to share the test-only Z: alias.
    static Z_MAPPING_TEST_LOCK: Mutex<()> = Mutex::new(());
    const LOCAL_CAPTURE_MARKER: &[u8] = b"R208_CAPTURED_LOCAL_ORIGIN_ONLY";

    #[test]
    fn retained_local_directory_returns_a_stable_fixed_dos_path() {
        let owned = tempfile::tempdir().expect("private Windows fixture");
        let pins = pin_input_root(owned.path()).expect("fixed local directory");
        let stable = stable_local_path(&pins).expect("handle-derived stable path");
        assert!(stable.is_absolute());
        assert!(stable.is_dir());
        assert_eq!(
            stable.canonicalize().expect("stable path"),
            owned.path().canonicalize().expect("fixture path")
        );
    }

    #[test]
    fn not_yet_created_target_uses_a_retained_existing_local_ancestor() {
        let owned = tempfile::tempdir().expect("private Windows fixture");
        let target = owned.path().join("new-install").join("updates");
        let (stable, pins) = stable_local_target_path(&target).expect("existing local ancestor");
        let stable_root = stable_local_path(&pins).expect("retained ancestor path");
        assert_eq!(stable, stable_root.join("new-install").join("updates"));
        assert!(!stable.exists());
        assert!(stable_local_target_path(&owned.path().join("..").join("escape")).is_err());
    }

    #[test]
    fn nonexistent_target_rejects_windows_trailing_dot_space_aliases() {
        let owned = tempfile::tempdir().expect("private Windows fixture");
        for suffix in [
            "new-install.",
            "new-install ",
            "new-install./updates",
            "new-install /updates",
            "new-install/updates.",
            "new-install/updates ",
        ] {
            let error = stable_local_target_path(&owned.path().join(suffix))
                .expect_err("ambiguous Windows target must fail closed");
            assert!(
                error.to_string().contains("trailing dot/space"),
                "target {suffix:?} failed for the wrong reason: {error}"
            );
        }
    }

    // Failure-first falsifier for a caller that pins a directory but opens
    // its file through the original mutable DOS alias after an A-to-B rebind.
    #[test]
    #[ignore = "requires an unused Z: SUBST letter"]
    fn pinned_parent_does_not_secure_a_later_raw_drive_alias_file_open() {
        let _lock = Z_MAPPING_TEST_LOCK.lock().expect("shared Z: lock");
        let original = tempfile::tempdir().expect("original root");
        let replacement = tempfile::tempdir().expect("replacement root");
        std::fs::write(original.path().join("release.key"), [0x11_u8; 32]).unwrap();
        std::fs::write(replacement.path().join("release.key"), [0x22_u8; 32]).unwrap();
        assert!(!Path::new(r"Z:\").exists(), "unused Z: required");
        assert!(!net_use(&["use", "Z:"]).unwrap().success());
        let mut mapping = TemporaryMappedDrive {
            kind: MappingKind::None,
        };
        assert!(
            subst(&["Z:", original.path().to_str().unwrap()])
                .unwrap()
                .success()
        );
        mapping.kind = MappingKind::Subst;
        let old_parent = pin_input_root(Path::new(r"Z:\")).expect("original fixed local root");
        let stable_original = stable_local_path(&old_parent).expect("original retained path");
        assert!(subst(&["Z:", "/D"]).unwrap().success());
        mapping.kind = MappingKind::None;
        assert!(
            subst(&["Z:", replacement.path().to_str().unwrap()])
                .unwrap()
                .success()
        );
        mapping.kind = MappingKind::Subst;
        assert_eq!(
            std::fs::read(stable_original.join("release.key")).unwrap(),
            [0x11_u8; 32]
        );
        let raw = super::super::local_source::pin_file(Path::new(r"Z:\release.key"), 32)
            .expect("fixed-local replacement file remains admissible");
        let expected: [u8; 32] = sha2::Sha256::digest([0x11_u8; 32]).into();
        assert_ne!(
            raw.sha256, expected,
            "negative control must witness B, not A"
        );
        let stable_file = stable_file_at_parent(Path::new(r"Z:\release.key"), &old_parent)
            .expect("original fixed-local file locator");
        let pinned = super::super::local_source::pin_file(&stable_file, 32)
            .expect("rebound drive must not redirect retained parent selection");
        assert_eq!(
            pinned.sha256, expected,
            "retained parent must select original A"
        );
        assert!(stable_file_at_parent(Path::new(r"Z:\release.key."), &old_parent).is_err());
        let initial_sha =
            super::super::local_initial_install::pin_sha_with_retained_parent_for_test(
                Path::new(r"Z:\release.key"),
                32,
                old_parent,
            )
            .expect("initial-install shared production pin must retain original A");
        assert_eq!(
            initial_sha, expected,
            "initial-install must not reselect rebound local B"
        );
    }

    /// The exact authoring-owner path forwarded to the direct image/archive
    /// compiler must not change identity after a SUBST alias becomes SMB.
    /// This tests the native custody boundary, not a signed image build.
    #[test]
    #[ignore = "requires a controlled SMB share and a free Z: drive letter"]
    fn captured_authoring_root_survives_subst_to_smb_rebind() {
        let _mapping_guard = Z_MAPPING_TEST_LOCK.lock().expect("Z: fixture lock");
        let remote_share =
            std::env::var("KELD_TEST_REBIND_SHARE").expect("controlled SMB rebind share");
        let owned = tempfile::tempdir().expect("private local authoring fixture");
        let config = "export default {name:'Input check',id:'com.example.check',version:'1.0.0',entry:'main.js',renderer:'index.html'};";
        std::fs::write(owned.path().join("keld.config.ts"), config).expect("config fixture");
        std::fs::write(
            owned.path().join("keld.permissions.jsonc"),
            br#"{"profile":"strict"}"#,
        )
        .expect("policy fixture");
        // The archive's actual module-capture sink must retain these local
        // bytes after the caller's Z: alias points to a remote share.
        std::fs::write(owned.path().join("main.js"), LOCAL_CAPTURE_MARKER)
            .expect("original entry fixture");
        std::fs::write(
            owned.path().join("index.html"),
            b"<main>Original local</main>",
        )
        .expect("original renderer fixture");
        assert!(!Path::new(r"Z:\").exists(), "Z: must be unused");
        assert!(
            !net_use(&["use", "Z:"]).expect("net use query").success(),
            "Z: must have no existing network connection"
        );
        let mut mapping = TemporaryMappedDrive {
            kind: MappingKind::None,
        };
        assert!(
            subst(&["Z:", owned.path().to_str().expect("local Unicode fixture")])
                .expect("subst executable")
                .success(),
            "create one-use local alias"
        );
        mapping.kind = MappingKind::Subst;
        let captured = CapturedInputs::open(Path::new(r"Z:\"))
            .expect("local project admission")
            .capture(Path::new(r"Z:\"))
            .expect("retained authoring input");
        let stable = captured.stable_root().to_owned();
        assert!(
            !stable.starts_with(r"Z:\"),
            "must not retain mutable DOS alias"
        );
        assert_eq!(
            stable.canonicalize().expect("stable captured project"),
            owned.path().canonicalize().expect("original project")
        );
        assert!(
            subst(&["Z:", "/D"]).expect("subst removal").success(),
            "remove alias with original project handles retained"
        );
        mapping.kind = MappingKind::None;
        assert!(
            net_use(&["use", "Z:", remote_share.as_str(), "/persistent:no"])
                .expect("net use rebind")
                .success(),
            "rebind same drive letter to SMB"
        );
        mapping.kind = MappingKind::Smb;
        assert!(
            pin_input_root(Path::new(r"Z:\")).is_err(),
            "remote alias must now fail closed"
        );
        assert_original_archive_module_capture_after_rebind(
            &captured,
            &stable,
            owned.path(),
            config,
        );
    }

    fn assert_original_archive_module_capture_after_rebind(
        captured: &CapturedAuthoring,
        stable: &Path,
        original: &Path,
        config: &str,
    ) {
        assert_eq!(captured.stable_root(), stable);
        assert_eq!(
            std::fs::read_to_string(stable.join("keld.config.ts")).expect("original bytes"),
            config,
            "archive/compile continuation must use the original captured project"
        );
        assert_eq!(
            captured
                .archive_members()
                .expect("archive's retained authoring members")
                .len(),
            2,
            "the same original authoring owner must emit both framework members"
        );
        let names = vec![captured.app.entry.clone()];
        let modules = super::super::windows_compiler::CapturedModules::open_js_fixture(
            stable,
            &names,
            &captured.app.entry,
            &captured.app.name,
        )
        .expect("archive's real native module capture from the retained local root");
        let sink_path = original.join("captured-compiler-request.bin");
        let mut sink = std::fs::File::create(&sink_path).expect("private request sink");
        modules
            .write_request(&mut sink)
            .expect("archive module capture must retain original bytes");
        drop(sink);
        let captured_bytes = std::fs::read(&sink_path).expect("read emitted compiler request");
        assert!(
            captured_bytes
                .windows(LOCAL_CAPTURE_MARKER.len())
                .any(|bytes| bytes == LOCAL_CAPTURE_MARKER),
            "actual compiler request must contain the original local entry"
        );
        assert!(
            super::super::windows_compiler::CapturedModules::open_js_fixture(
                Path::new(r"Z:\"),
                &names,
                &captured.app.entry,
                &captured.app.name,
            )
            .is_err(),
            "the rebound remote alias must be rejected at the same module-capture sink"
        );
    }

    fn system32_command(executable: &str, arguments: &[&str]) -> std::io::Result<ExitStatus> {
        let system_root = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        Command::new(system_root.join("System32").join(executable))
            .args(arguments)
            .status()
    }

    fn net_use(arguments: &[&str]) -> std::io::Result<ExitStatus> {
        system32_command("net.exe", arguments)
    }

    fn subst(arguments: &[&str]) -> std::io::Result<ExitStatus> {
        system32_command("subst.exe", arguments)
    }

    #[derive(Clone, Copy)]
    enum MappingKind {
        None,
        Smb,
        Subst,
    }

    struct TemporaryMappedDrive {
        kind: MappingKind,
    }

    impl Drop for TemporaryMappedDrive {
        fn drop(&mut self) {
            match self.kind {
                MappingKind::None => {}
                MappingKind::Smb => {
                    let _ = net_use(&["use", "Z:", "/delete", "/y"]);
                }
                MappingKind::Subst => {
                    let _ = subst(&["Z:", "/D"]);
                }
            }
        }
    }

    /// Run only on a Windows acceptance host with an existing readable SMB share:
    /// `KELD_TEST_SMB_SHARE`=\\\\host\\share,
    /// `KELD_TEST_REBIND_SHARE`=\\\\host\\other-share,
    /// `KELD_TEST_SMB_DIRECTORY`=Z:\\existing\\directory,
    /// `KELD_TEST_REBIND_DIRECTORY`=Z:\\existing\\directory.
    #[test]
    #[ignore = "requires a controlled SMB share and a free Z: drive letter"]
    fn mapped_smb_and_rebound_drive_handles_fail_closed() {
        let _mapping_guard = Z_MAPPING_TEST_LOCK.lock().expect("Z: fixture lock");
        let remote_share = std::env::var("KELD_TEST_SMB_SHARE").expect("mapped SMB share fixture");
        let rebound_share =
            std::env::var("KELD_TEST_REBIND_SHARE").expect("second SMB share fixture");
        let remote_directory =
            std::env::var("KELD_TEST_SMB_DIRECTORY").expect("mapped directory fixture");
        let rebound_directory =
            std::env::var("KELD_TEST_REBIND_DIRECTORY").expect("rebound directory fixture");
        assert!(remote_directory.starts_with(r"Z:\"));
        assert!(rebound_directory.starts_with(r"Z:\"));
        assert!(
            !Path::new(r"Z:\").exists(),
            "Z: must be unused before the test"
        );
        assert!(
            !net_use(&["use", "Z:"]).expect("net use query").success(),
            "Z: must not already have a network connection"
        );
        let mut mapping = TemporaryMappedDrive {
            kind: MappingKind::None,
        };
        assert!(
            net_use(&["use", "Z:", remote_share.as_str(), "/persistent:no"])
                .expect("net use executable")
                .success(),
            "initial temporary SMB mapping"
        );
        mapping.kind = MappingKind::Smb;
        assert!(
            pin_input_root(Path::new(&remote_directory)).is_err(),
            "the complete authoring-root admission must refuse mapped SMB"
        );
        let original = boot::open_windows_input_directory_guard(Path::new(&remote_directory))
            .expect("open mapped remote directory");
        assert!(
            keld_update::fixed_local_dos_path(&original).is_err(),
            "mapped SMB handle must be refused"
        );
        assert!(
            net_use(&["use", "Z:", "/delete", "/y"])
                .expect("net use delete")
                .success(),
            "Windows must release the temporary mapping while the handle remains open"
        );
        mapping.kind = MappingKind::None;
        assert!(
            net_use(&["use", "Z:", rebound_share.as_str(), "/persistent:no"])
                .expect("net use rebind")
                .success(),
            "rebind the same drive letter to a different SMB share"
        );
        mapping.kind = MappingKind::Smb;
        assert!(
            pin_input_root(Path::new(&rebound_directory)).is_err(),
            "authoring-root admission must refuse the rebound SMB namespace"
        );
        assert!(
            keld_update::fixed_local_dos_path(&original).is_err(),
            "the original retained handle must not become a local DOS path after rebind"
        );
        let rebound = boot::open_windows_input_directory_guard(Path::new(&rebound_directory))
            .expect("open newly rebound remote directory");
        assert!(
            keld_update::fixed_local_dos_path(&rebound).is_err(),
            "new handle on the rebound remote share must be refused"
        );
    }

    /// Verify that rebinding a local SUBST alias to SMB cannot redirect a
    /// retained local handle or the stable path returned from that handle.
    #[test]
    #[ignore = "requires a local D: fixture, a controlled SMB share and a free Z: drive letter"]
    fn local_alias_rebind_keeps_the_retained_object_on_its_fixed_volume() {
        let _mapping_guard = Z_MAPPING_TEST_LOCK.lock().expect("Z: fixture lock");
        let local_directory =
            std::env::var("KELD_TEST_LOCAL_DIRECTORY").expect("local directory fixture");
        let remote_share =
            std::env::var("KELD_TEST_REBIND_SHARE").expect("SMB rebind share fixture");
        let remote_directory =
            std::env::var("KELD_TEST_REBIND_DIRECTORY").expect("SMB rebind directory fixture");
        assert!(!Path::new(r"Z:\").exists(), "Z: must be unused");
        assert!(
            !net_use(&["use", "Z:"]).expect("net use query").success(),
            "Z: must not already have a network connection"
        );
        let mut mapping = TemporaryMappedDrive {
            kind: MappingKind::None,
        };
        assert!(
            subst(&["Z:", local_directory.as_str()])
                .expect("subst executable")
                .success(),
            "create temporary local DOS alias"
        );
        mapping.kind = MappingKind::Subst;
        let local_pins =
            pin_input_root(Path::new(r"Z:\")).expect("local alias must admit as fixed local");
        assert!(
            !stable_local_path(&local_pins)
                .expect("stable local alias")
                .starts_with(r"Z:\"),
            "the admitted stable path must not retain the mutable alias"
        );
        drop(local_pins);
        let original = boot::open_windows_input_directory_guard(Path::new(r"Z:\"))
            .expect("open local alias directory");
        let stable_before = keld_update::fixed_local_dos_path(&original)
            .expect("handle resolves to fixed local volume");
        assert!(stable_before.is_absolute());
        assert!(stable_before.is_dir());
        assert!(
            !stable_before.starts_with(r"Z:\"),
            "handle-derived path must not retain the mutable Z: drive alias"
        );
        assert_eq!(
            stable_before.canonicalize().expect("stable local identity"),
            Path::new(&local_directory)
                .canonicalize()
                .expect("original local directory identity"),
        );
        assert!(
            subst(&["Z:", "/D"])
                .expect("remove local DOS alias")
                .success(),
            "remove alias while the original handle remains open"
        );
        mapping.kind = MappingKind::None;
        assert!(
            net_use(&["use", "Z:", remote_share.as_str(), "/persistent:no"])
                .expect("net use rebind")
                .success(),
            "rebind the same letter to a remote SMB share"
        );
        mapping.kind = MappingKind::Smb;
        assert!(
            pin_input_root(Path::new(&remote_directory)).is_err(),
            "authoring-root admission must refuse an alias rebound to SMB"
        );
        let stable_after = keld_update::fixed_local_dos_path(&original)
            .expect("retained local handle must remain bound to its original object");
        assert_eq!(stable_after, stable_before);
        let rebound = boot::open_windows_input_directory_guard(Path::new(&remote_directory))
            .expect("open newly rebound remote directory");
        assert!(
            keld_update::fixed_local_dos_path(&rebound).is_err(),
            "new handle on the remote mapping must be refused"
        );
        let local_again = boot::open_windows_input_directory_guard(&stable_after)
            .expect("open stable handle-derived local path");
        assert_eq!(
            keld_update::fixed_local_dos_path(&local_again)
                .expect("stable local path remains fixed"),
            stable_before
        );
    }
}

#[cfg(test)]
mod windows_authoring_path_tests {
    use super::*;
    #[test]
    fn actual_windows_grammar_rejects_device_and_normalization_aliases() {
        require_windows_authoring_paths("main.js", Some("index.html")).unwrap();
        for bad in [
            "CON",
            "NUL.txt",
            "aux.js",
            "main.js.",
            "main.js ",
            "dir/../main.js",
            "dir/file:stream",
        ] {
            assert!(
                require_windows_authoring_paths(bad, Some("index.html")).is_err(),
                "entry {bad}"
            );
            assert!(
                require_windows_authoring_paths("main.js", Some(bad)).is_err(),
                "renderer {bad}"
            );
        }
        assert!(require_windows_authoring_paths("Main.js", Some("main.js")).is_err());
    }
}
