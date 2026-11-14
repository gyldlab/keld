//! Scratch Windows caller. Local-source compiler closure only, not release trust.
//! All inputs remain pinned until the Job runner returns and output is decoded.
use super::{BuildError, local_source, windows_inputs};
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::FileExt as _;
use std::path::Path;

const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_APP: u64 = 256 * 1024 * 1024;
const MAX_RESPONSE: usize = 256 * 1024 * 1024 + 16 * 1024 * 1024 + 48;
fn refusal(error: impl std::fmt::Display) -> BuildError {
    BuildError::input("captured compiler", error)
}

fn reserved_transport(name: &str) -> bool {
    name.rsplit('/').next().is_some_and(|leaf| {
        leaf.eq_ignore_ascii_case("kipc-transport.ts")
            || leaf.eq_ignore_ascii_case("kipc-transport.js")
    })
}
fn read_pinned(source: &local_source::PinnedFile) -> Result<Vec<u8>, BuildError> {
    let size = source.file.metadata().map_err(refusal)?.len();
    if size > MAX_FILE {
        return Err(refusal("canonical transport bound"));
    }
    let mut bytes = vec![0; usize::try_from(size).map_err(refusal)?];
    let mut offset = 0;
    while offset < bytes.len() {
        let count = source
            .file
            .seek_read(&mut bytes[offset..], offset as u64)
            .map_err(refusal)?;
        if count == 0 {
            return Err(refusal("canonical transport truncated"));
        }
        offset += count;
    }
    if source.file.seek_read(&mut [0], size).map_err(refusal)? != 0 {
        return Err(refusal("canonical transport grew"));
    }
    Ok(bytes)
}
fn require_scaffold_bytes(
    actual: &[u8],
    canonical: &[u8],
    app_name: &str,
) -> Result<(), BuildError> {
    let canonical = std::str::from_utf8(canonical).map_err(refusal)?;
    // Reuse the existing create owner, including its release substitution and stamp.
    let expected = crate::template::render_transport(canonical, app_name);
    if actual != expected.as_bytes() {
        return Err(refusal(
            "scaffold transport differs from retained compiler-source canonical transport",
        ));
    }
    Ok(())
}
fn verify_scaffold_transport(
    actual: &local_source::PinnedFile,
    canonical: &local_source::PinnedFile,
    app_name: &str,
) -> Result<(), BuildError> {
    require_scaffold_bytes(&read_pinned(actual)?, &read_pinned(canonical)?, app_name)
}

pub(super) struct CapturedModules {
    entry: String,
    _scaffold_transport: Option<local_source::PinnedFile>,
    files: Vec<(String, local_source::PinnedFile, u64)>,
    renderer: local_source::PinnedFile,
    renderer_size: u64,
    _directories: Vec<File>,
}
impl CapturedModules {
    #[expect(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "Captured module names require exact canonical lowercase suffixes."
    )]
    pub(super) fn open(
        root: &Path,
        names: &[String],
        entry: &str,
        app_name: &str,
        sources: &local_source::ObservedCompilerSources<'_>,
    ) -> Result<Self, BuildError> {
        if names.is_empty() || names.len() > 4096 || !names.iter().any(|n| n == entry) {
            return Err(refusal(
                "expected declared entry and at most4096 explicit modules",
            ));
        }
        let mut sorted = names.to_vec();
        sorted.sort();
        if sorted.windows(2).any(|p| p[0] == p[1]) {
            return Err(refusal("duplicate module"));
        }
        let mut paths = std::collections::BTreeSet::new();
        for name in &sorted {
            if !(name.ends_with(".js") || name.ends_with(".ts"))
                || reserved_transport(name)
                || name.len() > 100
            {
                return Err(refusal("unsupported module member"));
            }
            paths.insert(name.as_str());
            let mut rest = name.as_str();
            while let Some((parent, _)) = rest.rsplit_once('/') {
                paths.insert(parent);
                rest = parent;
            }
        }
        // Sole native namespace owner, before filesystem access from names.
        keld_guard::validate_windows_package_paths(&paths.into_iter().collect::<Vec<_>>())
            .map_err(refusal)?;
        let mut directories = windows_inputs::pin_input_root(root)?;
        let scaffold_transport = if entry.ends_with(".ts") {
            let parent = root
                .join(entry)
                .parent()
                .ok_or_else(|| refusal("entry parent missing"))?
                .to_owned();
            directories.extend(windows_inputs::pin_input_root(&parent)?);
            let actual = local_source::pin_file(&parent.join("kipc-transport.ts"), MAX_FILE)?;
            let canonical = sources.canonical_transport()?;
            verify_scaffold_transport(&actual, canonical, app_name)?;
            Some(actual)
        } else {
            None
        };
        let mut files = Vec::new();
        let mut total = 0_u64;
        for name in sorted {
            let path = root.join(&name);
            let parent = path
                .parent()
                .ok_or_else(|| refusal("module parent missing"))?;
            directories.extend(windows_inputs::pin_input_root(parent)?);
            let file = local_source::pin_file(&path, MAX_FILE)?;
            let size = file.file.metadata().map_err(refusal)?.len();
            total = total
                .checked_add(size)
                .ok_or_else(|| refusal("source length overflow"))?;
            if total > MAX_APP {
                return Err(refusal("aggregate module bound"));
            }
            files.push((name, file, size));
        }
        let renderer = local_source::pin_file(&root.join("index.html"), MAX_FILE)?;
        let renderer_size = renderer.file.metadata().map_err(refusal)?.len();
        if total
            .checked_add(renderer_size)
            .is_none_or(|size| size > MAX_APP)
        {
            return Err(refusal("aggregate module and renderer bound"));
        }
        Ok(Self {
            entry: entry.to_owned(),
            _scaffold_transport: scaffold_transport,
            files,
            renderer,
            renderer_size,
            _directories: directories,
        })
    }

    /// Output must be an exclusive freshly-created private regular file.
    pub(super) fn write_request(&self, output: &mut File) -> Result<(), BuildError> {
        if output.metadata().map_err(refusal)?.len() != 0 {
            return Err(refusal("request sink is not empty"));
        }
        output.seek(SeekFrom::Start(0)).map_err(refusal)?;
        output.write_all(b"KELDCIN3").map_err(refusal)?;
        output
            .write_all(
                &u32::try_from(self.entry.len())
                    .map_err(refusal)?
                    .to_le_bytes(),
            )
            .map_err(refusal)?;
        output.write_all(self.entry.as_bytes()).map_err(refusal)?;
        output
            .write_all(
                &u32::try_from(self.files.len())
                    .map_err(refusal)?
                    .to_le_bytes(),
            )
            .map_err(refusal)?;
        for (name, source, size) in &self.files {
            output
                .write_all(&u32::try_from(name.len()).map_err(refusal)?.to_le_bytes())
                .map_err(refusal)?;
            output.write_all(name.as_bytes()).map_err(refusal)?;
            output
                .write_all(&u32::try_from(*size).map_err(refusal)?.to_le_bytes())
                .map_err(refusal)?;
            let mut offset = 0;
            let mut buffer = vec![0_u8; 64 * 1024];
            while offset < *size {
                let count =
                    usize::try_from((*size - offset).min(buffer.len() as u64)).map_err(refusal)?;
                let read = source
                    .file
                    .seek_read(&mut buffer[..count], offset)
                    .map_err(refusal)?;
                if read == 0 {
                    return Err(refusal("captured source truncated"));
                }
                output.write_all(&buffer[..read]).map_err(refusal)?;
                offset += read as u64;
            }
            if source
                .file
                .seek_read(&mut buffer[..1], offset)
                .map_err(refusal)?
                != 0
            {
                return Err(refusal("captured source grew"));
            }
        }
        output
            .write_all(
                &u32::try_from(self.renderer_size)
                    .map_err(refusal)?
                    .to_le_bytes(),
            )
            .map_err(refusal)?;
        let mut offset = 0;
        let mut buffer = vec![0_u8; 64 * 1024];
        while offset < self.renderer_size {
            let count = usize::try_from((self.renderer_size - offset).min(buffer.len() as u64))
                .map_err(refusal)?;
            let read = self
                .renderer
                .file
                .seek_read(&mut buffer[..count], offset)
                .map_err(refusal)?;
            if read == 0 {
                return Err(refusal("captured renderer truncated"));
            }
            output.write_all(&buffer[..read]).map_err(refusal)?;
            offset += read as u64;
        }
        if self
            .renderer
            .file
            .seek_read(&mut buffer[..1], offset)
            .map_err(refusal)?
            != 0
        {
            return Err(refusal("captured renderer grew"));
        }
        output.sync_all().map_err(refusal)?;
        output.seek(SeekFrom::Start(0)).map_err(refusal)?;
        Ok(())
    }
}

pub(super) struct CompiledApp {
    renderer_sha256: [u8; 32],
    pub(super) main: Vec<u8>,
    pub(super) transport: Vec<u8>,
}

/// Own source witnesses alongside the actual emitted application members.
/// Images/Bun and authoring members are added by their separate admission owners.
pub(super) struct CapturedArchiveApp {
    pub(super) members: Vec<super::windows_archive::Member>,
    _source: CapturedModules,
}
impl CompiledApp {
    pub(super) fn into_archive(
        self,
        source: CapturedModules,
    ) -> Result<CapturedArchiveApp, BuildError> {
        use super::windows_archive::{Content, Member};
        if self.renderer_sha256 != source.renderer.sha256 {
            return Err(refusal("renderer admission is bound to different bytes"));
        }
        let renderer = source.renderer.file.try_clone().map_err(refusal)?;
        let members = vec![
            Member {
                name: "code".into(),
                content: None,
            },
            Member {
                name: "code/main.js".into(),
                content: Some(Content::Generated(self.main)),
            },
            Member {
                name: "code/kipc-transport.js".into(),
                content: Some(Content::Generated(self.transport)),
            },
            Member {
                name: "renderer".into(),
                content: None,
            },
            Member {
                name: "renderer/index.html".into(),
                content: Some(Content::Retained {
                    file: renderer,
                    size: source.renderer_size,
                }),
            },
        ];
        Ok(CapturedArchiveApp {
            members,
            _source: source,
        })
    }
}

fn decode_response(bytes: &[u8]) -> Result<CompiledApp, BuildError> {
    fn component(input: &mut std::io::Cursor<&[u8]>, limit: u64) -> Result<Vec<u8>, BuildError> {
        let mut length = [0; 4];
        input.read_exact(&mut length).map_err(refusal)?;
        let size = u64::from(u32::from_le_bytes(length));
        if size == 0 || size > limit || size > input.get_ref().len() as u64 - input.position() {
            return Err(refusal("compiler component length"));
        }
        let mut output = vec![0; usize::try_from(size).map_err(refusal)?];
        input.read_exact(&mut output).map_err(refusal)?;
        Ok(output)
    }
    if bytes.len() > MAX_RESPONSE {
        return Err(refusal("compiler response exceeds bound"));
    }
    let mut input = std::io::Cursor::new(bytes);
    let mut magic = [0; 8];
    input.read_exact(&mut magic).map_err(refusal)?;
    if &magic != b"KELDCOT2" {
        return Err(refusal("compiler response version"));
    }
    let mut renderer_sha256 = [0; 32];
    input.read_exact(&mut renderer_sha256).map_err(refusal)?;

    let main = component(&mut input, MAX_APP)?;
    let transport = component(&mut input, MAX_FILE)?;
    if input.position() != bytes.len() as u64 {
        return Err(refusal("trailing compiler response"));
    }
    Ok(CompiledApp {
        renderer_sha256,
        main,
        transport,
    })
}

// Bun's optional --config value is accepted only in the same argv token.
// Push the native path without display/string conversion or shell quoting.
fn bun_config_argument(path: &Path) -> OsString {
    let mut argument = OsString::from("--config=");
    argument.push(path.as_os_str());
    argument
}

/// Caller owns an admitted source compiler closure and qualified Bun pin. This
/// API never upgrades a local tool digest to release authority. The script and
/// its entire trusted import closure need retained protected ancestry, not just
/// this entry leaf. Cwd is an existing owner-private, config-free session.
pub(super) fn compile(
    modules: &CapturedModules,
    compiler: &super::compiler_toolchain::BoundCompilerEntry<'_>,
    private_session: &Path,
    environment: &[(OsString, OsString)],
    request: &mut File,
) -> Result<CompiledApp, BuildError> {
    modules.write_request(request)?;
    let args = [
        OsString::from("--no-install"),
        OsString::from("--no-env-file"),
        bun_config_argument(&compiler.configuration.path),
        OsString::from("--tsconfig-override"),
        compiler.tsconfig.path.as_os_str().to_owned(),
        OsString::from("--preload"),
        compiler.preload.path.as_os_str().to_owned(),
        compiler.source.entry().path.as_os_str().to_owned(),
        compiler.source.kipc_root().as_os_str().to_owned(),
        compiler.tools.parser.path.as_os_str().to_owned(),
    ];
    let bytes = local_source::run_captured(
        &compiler.tools.bun,
        &args,
        environment,
        private_session,
        request,
        MAX_RESPONSE,
    )?;
    decode_response(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bun_config_value_is_one_exact_native_argument() {
        let path = Path::new(r"D:\owned stage\配置\bunfig.toml");
        assert_eq!(
            bun_config_argument(path),
            OsString::from(r"--config=D:\owned stage\配置\bunfig.toml")
        );
    }
    #[test]
    fn response_requires_exact_bounded_frames() {
        let mut valid = b"KELDCOT2".to_vec();
        valid.extend([0_u8; 32]);
        valid.extend(1_u32.to_le_bytes());
        valid.push(b'a');
        valid.extend(1_u32.to_le_bytes());
        valid.push(b'b');
        let output = decode_response(&valid).unwrap();
        assert_eq!(output.main, b"a");
        assert_eq!(output.transport, b"b");
        for end in 0..valid.len() {
            assert!(decode_response(&valid[..end]).is_err());
        }
        valid.push(0);
        assert!(decode_response(&valid).is_err());
        valid[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_response(&valid).is_err());
    }
}

#[cfg(test)]
mod scaffold_source_tests {
    use super::*;
    #[test]
    fn actual_canonical_transport_is_required_not_an_arbitrary_stamped_file() {
        let source = "declare const KELD_KIPC_RELEASE: boolean | undefined;\nexport const app = '{{name}}';\n";
        let rendered = crate::template::render_transport(source, "hello");
        require_scaffold_bytes(rendered.as_bytes(), source.as_bytes(), "hello").unwrap();
        assert!(require_scaffold_bytes(rendered.as_bytes(), source.as_bytes(), "other").is_err());
        assert!(
            require_scaffold_bytes(
                b"export const arbitrary = true;",
                source.as_bytes(),
                "hello"
            )
            .is_err()
        );
        let changed = crate::template::restamp_transport("export const changed = true;\n");
        assert!(require_scaffold_bytes(changed.as_bytes(), source.as_bytes(), "hello").is_err());
        assert!(require_scaffold_bytes(rendered.as_bytes(), &[0xff], "hello").is_err());
    }
    #[test]
    fn reserved_source_names_are_case_insensitive_in_every_directory() {
        for name in [
            "kipc-transport.ts",
            "src/KIPC-TRANSPORT.TS",
            "any/Kipc-Transport.Js",
        ] {
            assert!(reserved_transport(name));
        }
        assert!(!reserved_transport("src/echo.generated.ts"));
    }
}
