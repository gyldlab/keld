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
    // Retain the captured source handles through the entire compiler transaction.
    // This is custody; no ambient source lookup or second provenance claim.
    _source: local_source::ObservedCompilerSources<'a>,
    pub(super) tools: &'a CapturedCompilerToolchain,
    pub(super) preload: local_source::PinnedFile,
    pub(super) entry: local_source::PinnedFile,
    _projected_compiler: local_source::PinnedFile,
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
        let closed_app = source.closed_app()?;
        let mut imports = Vec::new();
        for (name, relative) in [
            ("typescript/unstable/ast", "dist/ast/index.js"),
            ("typescript/unstable/async", "dist/api/async/api.js"),
            ("typescript/unstable/fs", "dist/api/fs.js"),
        ] {
            imports.push((name, local_file_url(&tools.package_root.join(relative))?));
        }
        let projected = project_imports(
            &read_compiler_source(closed_app)?,
            include_bytes!("../../tools/closed-app.ts"),
            &imports,
        )?;
        let projected_compiler = write_generated_input(stage, "closed-app-bound.ts", &projected)?;
        let tool_root = source
            .entry()
            .path
            .parent()
            .ok_or_else(|| refuse("compiler entry parent absent"))?;
        let projected = project_imports(
            &read_compiler_source(source.entry())?,
            include_bytes!("../../tools/closed-app-runner.ts"),
            &[
                ("./closed-app.ts", local_file_url(&projected_compiler.path)?),
                (
                    "./protocol.ts",
                    local_file_url(&tool_root.join("protocol.ts"))?,
                ),
                (
                    "./static-renderer.ts",
                    local_file_url(&tool_root.join("static-renderer.ts"))?,
                ),
                (
                    "../../../packages/@keld/kipc/scripts/package-artifacts.ts",
                    local_file_url(&source.kipc_root().join("scripts/package-artifacts.ts"))?,
                ),
                (
                    "../../../packages/@keld/kipc/src/transport.ts",
                    local_file_url(&source.kipc_root().join("src/transport.ts"))?,
                ),
            ],
        )?;
        let entry = write_generated_input(stage, "compiler-bound-runner.ts", &projected)?;
        let preload = write_generated_input(
            stage,
            "compiler-preload.ts",
            b"// Fixed explicit compiler imports; no runtime resolver hook.\n",
        )?;
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
            _source: source,
            tools,
            preload,
            entry,
            _projected_compiler: projected_compiler,
            configuration,
            tsconfig,
            _global_configuration: global_configuration,
            _directories: directories,
        })
    }
}

// The caller retains the exact original compiler source and official package.
// This is a fixed source projection, not an application import parser/resolver.
fn project_imports(
    bytes: &[u8],
    expected: &[u8],
    bindings: &[(&str, String)],
) -> Result<Vec<u8>, BuildError> {
    if bytes != expected {
        return Err(refuse(
            "captured compiler source differs from this CLI's reviewed tool bytes",
        ));
    }
    let mut text = std::str::from_utf8(bytes).map_err(refuse)?.to_owned();
    for (literal, target) in bindings {
        let old = serde_json::to_string(literal).map_err(refuse)?;
        if text.matches(&old).count() != 1 {
            return Err(refuse("fixed compiler import literal is not unique"));
        }
        text = text.replace(&old, &serde_json::to_string(target).map_err(refuse)?);
    }
    Ok(text.into_bytes())
}

pub(super) fn local_file_url(path: &Path) -> Result<String, BuildError> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    if !path.is_absolute()
        || !matches!(path.components().next(),
        Some(std::path::Component::Prefix(prefix))
            if matches!(prefix.kind(), std::path::Prefix::Disk(_)))
    {
        return Err(refuse(
            "compiler URL requires an ordinary absolute local disk path",
        ));
    }
    if path.components().skip(1).any(|component| {
        !matches!(
            component,
            std::path::Component::RootDir | std::path::Component::Normal(_)
        )
    }) {
        return Err(refuse("compiler URL path traversal refused"));
    }
    let normalized = path
        .to_str()
        .ok_or_else(|| refuse("compiler path is not UTF8"))?
        .replace('\\', "/");
    let mut result = String::from("file:///");
    for byte in normalized.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            result.push(char::from(byte));
        } else {
            result.push('%');
            result.push(char::from(HEX[usize::from(byte >> 4)]));
            result.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    Ok(result)
}

fn read_compiler_source(source: &local_source::PinnedFile) -> Result<Vec<u8>, BuildError> {
    use std::os::windows::fs::FileExt as _;
    let size = source.file.metadata().map_err(refuse)?.len();
    if size > 64 * 1024 {
        return Err(refuse("compiler source projection bound"));
    }
    let mut bytes = vec![0; usize::try_from(size).map_err(refuse)?];
    let mut offset = 0;
    while offset < bytes.len() {
        let count = source
            .file
            .seek_read(&mut bytes[offset..], u64::try_from(offset).map_err(refuse)?)
            .map_err(refuse)?;
        if count == 0 {
            return Err(refuse("retained compiler source truncated"));
        }
        offset += count;
    }
    let observed: [u8; 32] = Sha256::digest(&bytes).into();
    if observed != source.sha256 {
        return Err(refuse("retained compiler source hash changed"));
    }
    Ok(bytes)
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

#[cfg(test)]
mod explicit_projection_tests {
    use super::*;

    #[test]
    fn explicit_url_encodes_path_data_and_refuses_unqualified_prefixes() {
        assert_eq!(
            local_file_url(Path::new(r"C:\Keld Tools\a#b?c%é.js")).expect("ordinary disk URL"),
            "file:///C:/Keld%20Tools/a%23b%3Fc%25%C3%A9.js"
        );
        for path in [
            r"\\?\C:\typescript",
            r"\\server\share\typescript",
            r"C:\a\..\b",
            "relative",
        ] {
            assert!(local_file_url(Path::new(path)).is_err());
        }
    }

    #[test]
    fn projection_requires_exact_expected_source_and_unique_known_literal() {
        let source = b"import * as ts from \"typescript/unstable/ast\";\n";
        let bindings = [(
            "typescript/unstable/ast",
            "file:///C:/owned/ast.js".to_owned(),
        )];
        assert_eq!(
            project_imports(source, source, &bindings).expect("fixed projection"),
            b"import * as ts from \"file:///C:/owned/ast.js\";\n"
        );
        assert!(project_imports(b"changed", source, &bindings).is_err());
        assert!(
            project_imports(
                source,
                source,
                &[("absent", "file:///C:/owned/ast.js".into())]
            )
            .is_err()
        );
        let duplicated = [source.as_slice(), source.as_slice()].concat();
        assert!(project_imports(&duplicated, &duplicated, &bindings).is_err());
    }
}
