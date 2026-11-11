//! Scratch custody owner for an explicitly selected LOCAL compiler toolchain.
//! Fixed TypeScript file expectations derive from the official integrity-checked
//!7.0.2 tarball. This owns retained bytes, not release/platform qualification.
use super::{BuildError, local_source, windows_inputs};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::Path;

pub(super) struct CapturedCompilerToolchain {
    pub(super) bun: local_source::PinnedFile,
    pub(super) parser: local_source::PinnedFile,
    package_root: std::path::PathBuf,
    _files: Vec<local_source::PinnedFile>,
    _directories: Vec<File>,
}
fn refuse(error: impl std::fmt::Display) -> BuildError {
    BuildError::input("captured compiler toolchain", error)
}
fn matches_hex(actual: &[u8; 32], expected: &str) -> bool {
    expected.len() == 64
        && expected
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .zip(actual)
            .all(|(pair, byte)| {
                let text = std::str::from_utf8(pair).ok();
                text.and_then(|text| u8::from_str_radix(text, 16).ok()) == Some(*byte)
            })
}
impl CapturedCompilerToolchain {
    /// Expected Bun/parser digests must come from the already authorized tool
    /// source, never by hashing an arbitrary file and calling it qualified.
    pub(super) fn capture(
        package_root: &Path,
        bun: &Path,
        expected_bun: &[u8; 32],
        parser: &Path,
        expected_parser: &[u8; 32],
    ) -> Result<Self, BuildError> {
        let manifest: BTreeMap<String, String> =
            serde_json::from_str(include_str!("typescript-files.json")).map_err(refuse)?;
        if manifest.len() != 416 {
            return Err(refuse("fixed compiler manifest changed"));
        }
        let mut paths = BTreeSet::new();
        for name in manifest.keys() {
            paths.insert(name.as_str());
            let mut rest = name.as_str();
            while let Some((parent, _)) = rest.rsplit_once('/') {
                paths.insert(parent);
                rest = parent;
            }
        }
        keld_guard::validate_windows_package_paths(&paths.into_iter().collect::<Vec<_>>())
            .map_err(refuse)?;
        let mut directories = windows_inputs::pin_input_root(package_root)?;
        let mut parents = BTreeSet::new();
        let mut files = Vec::new();
        for (name, expected) in manifest {
            let path = package_root.join(name);
            let parent = path
                .parent()
                .ok_or_else(|| refuse("compiler file parent missing"))?;
            if parents.insert(parent.to_owned()) {
                directories.extend(windows_inputs::pin_input_root(parent)?);
            }
            let file = local_source::pin_file(&path, 16 * 1024 * 1024)?;
            if !matches_hex(&file.sha256, &expected) {
                return Err(refuse("official compiler file mismatch"));
            }
            files.push(file);
        }
        for tool in [bun, parser] {
            directories.extend(windows_inputs::pin_input_root(
                tool.parent().ok_or_else(|| refuse("tool parent missing"))?,
            )?);
        }
        let bun = local_source::pin_file(bun, 256 * 1024 * 1024)?;
        let parser = local_source::pin_file(parser, 256 * 1024 * 1024)?;
        if &bun.sha256 != expected_bun || &parser.sha256 != expected_parser {
            return Err(refuse("explicit compiler executable digest mismatch"));
        }
        Ok(Self {
            bun,
            parser,
            package_root: package_root.to_owned(),
            _files: files,
            _directories: directories,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_digest_refuses_malformed_and_changed_expectations() {
        assert!(matches_hex(&[0; 32], &"00".repeat(32)));
        assert!(!matches_hex(&[1; 32], &"00".repeat(32)));
        for bad in ["0".repeat(63), "00".repeat(33), "zz".repeat(32)] {
            assert!(!matches_hex(&[0; 32], &bad));
        }
    }
}

/// Retains source and dependency custody, binding only the three parser imports
/// to the exact captured official package instead of ambient `node_modules`.
pub(super) struct BoundCompilerEntry<'a> {
    pub(super) source: local_source::ObservedCompilerSources<'a>,
    pub(super) tools: &'a CapturedCompilerToolchain,
    pub(super) preload: local_source::PinnedFile,
    pub(super) configuration: local_source::PinnedFile,
    pub(super) tsconfig: local_source::PinnedFile,
    _global_configuration: local_source::PinnedFile,
    _directories: Vec<File>,
}
impl<'a> BoundCompilerEntry<'a> {
    pub(super) fn bind(
        source: local_source::ObservedCompilerSources<'a>,
        tools: &'a CapturedCompilerToolchain,
        stage: &Path,
    ) -> Result<Self, BuildError> {
        let directories = windows_inputs::pin_input_root(stage)?;
        let mut imports = BTreeMap::new();
        for (name, relative) in [
            ("typescript/unstable/ast", "dist/ast/index.js"),
            ("typescript/unstable/async", "dist/api/async/api.js"),
            ("typescript/unstable/fs", "dist/api/fs.js"),
        ] {
            let path = tools.package_root.join(relative);
            imports.insert(
                name,
                path.to_str()
                    .ok_or_else(|| refuse("compiler path is not UTF8"))?
                    .to_owned(),
            );
        }
        let map = serde_json::to_string(&imports).map_err(refuse)?;
        // Fixed code plus JSON-escaped paths; never application code or shell.
        let text = format!(
            "const paths = {map};\nBun.plugin({{name:'keld-owned-typescript',setup(build){{build.onResolve({{filter:/^typescript(?:\\/|$)/}},args=>{{const path=paths[args.path];if(!path)throw new Error('Unadmitted TypeScript import');return {{path,namespace:'file'}};}});}}}});\n"
        );
        let preload = write_generated_input(stage, "compiler-preload.ts", text.as_bytes())?;
        // Bun1.4.4 loads HOME/.bunfig.toml even with --config. Both are exact
        // create-new retained files, not an absence check vulnerable to creation.
        let configuration = write_generated_input(
            stage,
            "bunfig.toml",
            b"# KELD empty local compiler configuration\n",
        )?;
        let global_configuration = write_generated_input(
            stage,
            ".bunfig.toml",
            b"# KELD empty global compiler configuration\n",
        )?;
        let tsconfig =
            write_generated_input(stage, "tsconfig.json", b"{\"compilerOptions\":{}}\n")?;
        Ok(Self {
            source,
            tools,
            preload,
            configuration,
            tsconfig,
            _global_configuration: global_configuration,
            _directories: directories,
        })
    }
}

fn write_generated_input(
    stage: &Path,
    name: &str,
    bytes: &[u8],
) -> Result<local_source::PinnedFile, BuildError> {
    use std::io::Write as _;
    use std::os::windows::fs::OpenOptionsExt as _;
    let expected: [u8; 32] = Sha256::digest(bytes).into();
    let path = stage.join(name);
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
            .open(&path)
            .map_err(refuse)?;
        file.write_all(bytes).map_err(refuse)?;
        file.sync_all().map_err(refuse)?;
    }
    pin_generated_input(&path, expected)
}

// Reopening must authenticate the generated bytes, not merely measure whatever
// occupies the name after the writer is closed. The returned pin denies mutation.
fn pin_generated_input(
    path: &Path,
    expected: [u8; 32],
) -> Result<local_source::PinnedFile, BuildError> {
    let pinned = local_source::pin_file(path, 64 * 1024)?;
    if pinned.sha256 != expected {
        return Err(refuse(
            "generated compiler startup input changed before capture",
        ));
    }
    Ok(pinned)
}

#[cfg(test)]
mod generated_preload_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn generated_preload_rejects_substitution_and_retains_exact_bytes() {
        let root = std::env::temp_dir().join(format!(
            "keld-preload-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).expect("fresh fixture directory");
        let path = root.join("compiler-preload.ts");
        let generated = b"const exactGeneratedFixture = 1;";
        let expected: [u8; 32] = Sha256::digest(generated).into();
        // Model substitution in the close-writer/reopen interval. Measuring the
        // substitute without checking the expected digest must fail this oracle.
        std::fs::write(&path, b"const substitutedFixture = 2;").expect("fixture substitute");
        assert!(pin_generated_input(&path, expected).is_err());
        std::fs::write(&path, generated).expect("fixture original");
        let pinned = pin_generated_input(&path, expected).expect("exact generated bytes");
        assert_eq!(pinned.sha256, expected);
        let denied = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect_err("retained preload must deny writer");
        assert_eq!(denied.raw_os_error(), Some(32)); // ERROR_SHARING_VIOLATION
        drop(pinned);
        drop(
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .expect("writer succeeds after retained pin releases"),
        );
        std::fs::remove_file(&path).expect("remove owned fixture file");
        std::fs::remove_dir(&root).expect("remove empty owned fixture directory");
    }
}
