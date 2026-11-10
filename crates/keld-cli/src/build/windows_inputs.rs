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

struct CapturedInputs {
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
    fn open(root: &Path) -> Result<Self, BuildError> {
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

    fn check(mut self, root: &Path) -> Result<config::AppConfig, BuildError> {
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
        Ok(app)
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
