//! Native, read-only Windows application-authoring admission.
//!
//! This explicit check creates no package, stage, process, profile or grant.

use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::{Component, Path, Prefix};

use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

use super::{BuildError, config};
use crate::boot;

const AUTHORING_LIMIT: u64 = 64 * 1024;

/// Windows has no qualified explicit-profile primary launch yet. Preserve the
/// original strict authoring/diagnostic lane; a developer-local demo uses only
/// the already-supported undeclared/unverified Windows profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProfileAdmission {
    ExplicitStrict,
    UndeclaredDeveloper,
}

pub(super) struct CapturedInputs {
    _ancestors: Vec<File>,
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
        ancestors.push(
            boot::open_windows_input_directory_guard(path)
                .map_err(|error| BuildError::input("Windows authoring namespace", error))?,
        );
    }
    Ok(ancestors)
}

impl CapturedInputs {
    pub(super) fn open(root: &Path) -> Result<Self, BuildError> {
        let ancestors = pin_input_root(root)?;
        boot::ensure_current_principal_owns(root)
            .map_err(|error| BuildError::input("Windows project owner", error))?;
        let config = open_leaf(root, "keld.config.ts")?;
        let permissions = open_leaf(root, "keld.permissions.jsonc")?;
        Ok(Self {
            _ancestors: ancestors,
            config,
            permissions,
        })
    }

    pub(super) fn check(self, root: &Path) -> Result<config::AppConfig, BuildError> {
        Ok(self.capture(root)?.app)
    }

    /// Retain literal authoring bytes and their Windows namespace witnesses.
    pub(super) fn capture(self, root: &Path) -> Result<CapturedAuthoring, BuildError> {
        self.capture_with_profile(root, ProfileAdmission::ExplicitStrict)
    }

    pub(super) fn capture_developer(self, root: &Path) -> Result<CapturedAuthoring, BuildError> {
        self.capture_with_profile(root, ProfileAdmission::UndeclaredDeveloper)
    }

    fn capture_with_profile(
        mut self,
        root: &Path,
        profile: ProfileAdmission,
    ) -> Result<CapturedAuthoring, BuildError> {
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
        match profile {
            ProfileAdmission::ExplicitStrict
                if manifest.declared_profile() != Some(keld_guard::DeclaredProfile::Strict) =>
            {
                return Err(BuildError::input(
                    "Windows declared profile",
                    "explicit /profile:strict is required; a declaration is not runtime qualification",
                ));
            }
            ProfileAdmission::UndeclaredDeveloper if manifest.declared_profile().is_some() => {
                return Err(BuildError::input(
                    "Windows developer-local profile",
                    "Windows primary launch does not qualify explicit Strict or Legacy; developer-local requires an undeclared profile and makes no containment claim",
                ));
            }
            _ => {}
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
            _inputs: self,
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
    _inputs: CapturedInputs,
}

impl CapturedAuthoring {
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
    let metadata = file
        .metadata()
        .map_err(|error| BuildError::input(name, error))?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(BuildError::input(
            name,
            "input must be a non-reparse regular file",
        ));
    }
    boot::ensure_current_principal_owns(&path).map_err(|error| BuildError::input(name, error))?;
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
    fn developer_local_build_matches_supported_undeclared_windows_primary_profile() {
        let fixture = Fixture::new();
        assert!(
            CapturedInputs::open(&fixture.0)
                .unwrap()
                .capture_developer(&fixture.0)
                .is_err(),
            "explicit Strict must not enter developer-local"
        );
        fs::write(fixture.0.join("keld.permissions.jsonc"), b"{}\n").unwrap();
        let captured = CapturedInputs::open(&fixture.0)
            .unwrap()
            .capture_developer(&fixture.0)
            .expect("undeclared Windows profile is supported for local demonstration");
        let members = captured.archive_members().unwrap();
        let Some(super::super::windows_archive::Content::Generated(boot)) = &members[0].content
        else {
            panic!("compiler produced boot descriptor");
        };
        let boot: serde_json::Value = serde_json::from_slice(boot).unwrap();
        assert_eq!(
            boot["schema"],
            keld_core::app_session::boot_schema(false, None)
        );
        drop(captured);
        for declared in [r#"{"profile":"strict"}"#, r#"{"profile":"legacy"}"#] {
            fs::write(fixture.0.join("keld.permissions.jsonc"), declared).unwrap();
            let error = CapturedInputs::open(&fixture.0)
                .unwrap()
                .capture_developer(&fixture.0);
            assert!(error.is_err(), "explicit profile must refuse: {declared}");
        }
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
