//! LOCAL TEST complete archive assembly from same-execution retained owners.
//! No manifest signing, installer publication, installation or Strict claim.
use super::{
    BuildError,
    compiler_toolchain::{BoundCompilerEntry, CapturedCompilerToolchain},
    local_images::PreparedLocalPair,
    windows_archive::{self, Content, Member},
    windows_compiler::{self, CapturedModules},
    windows_inputs,
};
use crate::boot;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::PathBuf;
use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

pub(super) struct LocalArchiveCandidate<'images> {
    _images: Option<&'images PreparedLocalPair>,
    path: PathBuf,
    receipt: keld_pack::ProducedFull,
    // Keep exact output and namespace protected. Publication is a separate owner.
    file: File,
    _directories: Vec<File>,
}
impl LocalArchiveCandidate<'_> {
    pub(super) fn signed_pair(&self) -> Result<&PreparedLocalPair, BuildError> {
        self._images.ok_or_else(|| refuse("synthetic candidate cannot invoke installation"))
    }
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub(super) const fn receipt(&self) -> &keld_pack::ProducedFull {
        &self.receipt
    }
    #[cfg(test)]
    pub(super) fn synthetic_candidate(
        root: &std::path::Path,
        bytes: &[u8],
        receipt: keld_pack::ProducedFull,
    ) -> Result<Self, BuildError> {
        use std::io::Write as _;
        boot::create_windows_stage_root(root).map_err(refuse)?;
        let directories = windows_inputs::pin_input_root(root)?;
        let path = root.join("test-only-archive.candidate");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ)
            .open(&path)
            .map_err(refuse)?;
        file.write_all(bytes).map_err(refuse)?;
        file.sync_all().map_err(refuse)?;
        Ok(Self {
            _images: None,
            path,
            receipt,
            file,
            _directories: directories,
        })
    }
    /// Exclusive positioned access to the exact emitted handle. This borrows the
    /// owner, preserves namespace custody and exposes no writer/path reopen.
    pub(super) fn retained_reader(&mut self) -> io::Result<impl Read + Seek + '_> {
        self.file.seek(SeekFrom::Start(0))?;
        Ok(&mut self.file)
    }
}
fn refuse(e: impl std::fmt::Display) -> BuildError {
    BuildError::input("local application archive", e)
}

/// The compiler entry comes only from pair.source; a generated pinned preload
/// binds its TypeScript imports to the captured official package. No path/receipt
/// for external Host/helper images is accepted here.
pub(super) fn prepare<'images>(
    pair: &'images PreparedLocalPair,
    tools: &CapturedCompilerToolchain,
    module_names: &[String],
) -> Result<LocalArchiveCandidate<'images>, BuildError> {
    // Compile shared boot before adding side effects. These members bind the
    // same literal permissions used to admit the signed pair's app identity.
    let mut members = pair.authoring.archive_members()?;
    // Derive code/renderer from the same pinned namespace as config/policy;
    // never accept an independently captured application root.
    let sources = pair.source.compiler_sources()?;
    let modules = CapturedModules::open(&pair.app_root, module_names, &pair.authoring.app.entry, &pair.authoring.app.name, &sources)?;
    let root = pair.session.join("local-app-archive");
    boot::create_windows_stage_root(&root).map_err(refuse)?;
    let directories = windows_inputs::pin_input_root(&root)?;
    let compiler = BoundCompilerEntry::bind(sources, tools, &root)?;
    let mut request = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(root.join("compiler-request.bin"))
        .map_err(refuse)?;
    let mut environment = Vec::<(OsString, OsString)>::new();
    for name in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            environment.push((name.into(), value));
        }
    }
    for name in [
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
    ] {
        environment.push((name.into(), root.as_os_str().to_owned()));
    }
    let compilation =
        windows_compiler::compile(&modules, &compiler, &root, &environment, &mut request)?;
    let mut app = compilation.into_archive(modules)?;
    members.append(&mut app.members);
    // Guard-verified image witnesses remain retained for the entire emission.
    members.push(Member {
        name: "keld-host.exe".into(),
        content: Some(Content::Generated(pair.host.bytes.clone())),
    });
    members.push(Member {
        name: "keld-updater-helper.exe".into(),
        content: Some(Content::Generated(pair.helper.bytes.clone())),
    });
    members.push(Member {
        name: "bun.exe".into(),
        content: Some(Content::Retained {
            file: tools.bun.file.try_clone().map_err(refuse)?,
            size: tools.bun.file.metadata().map_err(refuse)?.len(),
        }),
    });
    let path = root.join("app.keld.tar.zst.candidate");
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)
        .map_err(refuse)?;
    let receipt = windows_archive::emit(&mut members, &mut output).map_err(refuse)?;
    output.sync_all().map_err(refuse)?;
    drop(compiler);
    let output = reopen_emitted_archive(output, &path, receipt.compressed_size())?;
    // An error leaves a diagnostic candidate, never a receipt or final installable
    // name. This success owns complete local archive bytes only, not publication.
    Ok(LocalArchiveCandidate {
        _images: Some(pair),
        path,
        receipt,
        file: output,
        _directories: directories,
    })
}

pub(super) fn reopen_emitted_archive(mut output:File,path:&std::path::Path,expected_size:u64)->Result<File,BuildError> {
    // Retain exact emitted bytes across closing the creation writer. The Host
    // opens with deny-write sharing, so retaining a write-capable handle would
    // make the legitimate installer read fail. Namespace custody stays live.
    use sha2::{Digest as _,Sha256};
    output.seek(SeekFrom::Start(0)).map_err(refuse)?;
    let mut emitted=Sha256::new();let mut buffer=[0_u8;16*1024];
    loop {let count=output.read(&mut buffer).map_err(refuse)?;if count==0 {break;}emitted.update(&buffer[..count]);}
    let expected:[u8;32]=emitted.finalize().into();
    // Overlap a read-only bridge with the creation handle. Sharing WRITE here
    // accommodates the still-open creation writer; neither handle shares DELETE.
    // Thus the same named file cannot be replaced during the handoff. The final
    // read-only pin denies WRITE again before the bridge is released.
    let bridge=OpenOptions::new().read(true).share_mode(FILE_SHARE_READ|FILE_SHARE_WRITE)
        .open(path).map_err(refuse)?;
    drop(output);
    let reopened=super::local_source::pin_file(path,expected_size)?;
    if reopened.sha256!=expected || reopened.file.metadata().map_err(refuse)?.len()!=expected_size {
        return Err(refuse("archive changed across read-only reopening"));
    }
    drop(bridge);
    Ok(reopened.file)

}
#[cfg(test)]
mod handoff_tests {
    use super::*;
    use std::io::Write as _;
    fn output(path:&std::path::Path)->File {
        let mut file=OpenOptions::new().read(true).write(true).create_new(true).share_mode(FILE_SHARE_READ).open(path).unwrap();
        file.write_all(b"actual emitted bytes").unwrap();file.sync_all().unwrap();file
    }
    #[test] fn readonly_handoff_preserves_bytes_and_blocks_mutation_until_release() {
        let root=tempfile::tempdir().unwrap();let path=root.path().join("candidate");
        let mut pin=reopen_emitted_archive(output(&path),&path,b"actual emitted bytes".len() as u64).unwrap();
        let mut bytes=Vec::new();pin.read_to_end(&mut bytes).unwrap();assert_eq!(bytes,b"actual emitted bytes");
        assert!(OpenOptions::new().write(true).share_mode(FILE_SHARE_READ|FILE_SHARE_WRITE).open(&path).is_err());
        assert!(std::fs::rename(&path,root.path().join("replaced")).is_err());
        assert!(std::fs::remove_file(&path).is_err());
        drop(pin);assert!(OpenOptions::new().write(true).open(&path).is_ok());
    }
    #[test] fn declared_length_mismatch_never_returns_candidate_pin() {
        let root=tempfile::tempdir().unwrap();let path=root.path().join("candidate");
        assert!(reopen_emitted_archive(output(&path),&path,21).is_err());
    }
}
