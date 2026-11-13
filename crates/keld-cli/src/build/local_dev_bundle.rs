//! Developer-local runnable folder for the normal KEL-96 dev lease.
//! Never claims KEL-53 installed provenance, Strict, release or Store trust.
use super::{
    BuildError,
    local_images::PreparedLocalPair,
    windows_archive::{Content, Member},
    windows_inputs,
};
use crate::boot;
use sha2::{Digest as _, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::os::windows::fs::{FileExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

const MAX_COPY: u64 = 256 * 1024 * 1024;
fn refuse(e: impl std::fmt::Display) -> BuildError {
    BuildError::input("Windows developer lease bundle", e)
}
fn readback(path: &Path, expected: [u8; 32], size: u64) -> Result<(), BuildError> {
    let mut input = File::open(path).map_err(refuse)?;
    if input.metadata().map_err(refuse)?.len() != size {
        return Err(refuse("bundled file length changed before readback"));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 16384];
    loop {
        let count = input.read(&mut buffer).map_err(refuse)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if hash.finalize().as_slice() != expected {
        return Err(refuse(
            "bundle member readback digest differs from retained source",
        ));
    }
    Ok(())
}
fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), BuildError> {
    if bytes.len() as u64 > MAX_COPY {
        return Err(refuse("generated member exceeds bounded local copy"));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(refuse)?;
    output.write_all(bytes).map_err(refuse)?;
    output.sync_all().map_err(refuse)?;
    drop(output);
    let expected: [u8; 32] = Sha256::digest(bytes).into();
    readback(path, expected, bytes.len() as u64)
}
fn copy_file(path: &Path, source: &File, size: u64) -> Result<(), BuildError> {
    if size > MAX_COPY || source.metadata().map_err(refuse)?.len() != size {
        return Err(refuse("retained member file length exceeded or changed"));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(refuse)?;
    let mut offset = 0_u64;
    let mut input_hash = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    while offset < size {
        let remaining =
            usize::try_from((size - offset).min(buffer.len() as u64)).map_err(refuse)?;
        let count = source
            .seek_read(&mut buffer[..remaining], offset)
            .map_err(refuse)?;
        if count == 0 {
            return Err(refuse("retained member truncated during bundle copy"));
        }
        input_hash.update(&buffer[..count]);
        output.write_all(&buffer[..count]).map_err(refuse)?;
        offset += u64::try_from(count).map_err(refuse)?;
    }
    if source.seek_read(&mut buffer[..1], size).map_err(refuse)? != 0 {
        return Err(refuse("retained member grew during bundle copy"));
    }
    output.sync_all().map_err(refuse)?;
    drop(output);
    readback(path, input_hash.finalize().into(), size)
}
fn copy_member(target: &Path, content: &Content) -> Result<(), BuildError> {
    match content {
        Content::Generated(bytes) => write_bytes(target, bytes),
        Content::Retained { file, size } => copy_file(target, file, *size),
    }
}
fn required_json<'a>(members: &'a [Member], name: &str) -> Result<&'a [u8], BuildError> {
    let member = members
        .iter()
        .find(|member| member.name == name)
        .ok_or_else(|| refuse("canonical boot descriptor member absent"))?;
    match &member.content {
        Some(Content::Generated(bytes)) => Ok(bytes),
        _ => Err(refuse(
            "canonical boot descriptor is not generated immutable bytes",
        )),
    }
}
fn quoted(value: &str) -> Result<String, BuildError> {
    serde_json::to_string(value).map_err(refuse)
}
fn bundle_recipe(pair: &PreparedLocalPair, members: &[Member]) -> Result<Vec<u8>, BuildError> {
    let descriptor: serde_json::Value =
        serde_json::from_slice(required_json(members, "keld.boot.json")?).map_err(refuse)?;
    let entry = descriptor
        .get("entry")
        .and_then(|value| value.as_str())
        .ok_or_else(|| refuse("compiled descriptor entry absent"))?;
    let renderer = descriptor
        .get("renderer")
        .and_then(|value| value.as_str())
        .ok_or_else(|| refuse("compiled renderer path absent"))?;
    if entry != "code/main.js" || renderer != "renderer/index.html" {
        return Err(refuse(
            "first developer lease bundle requires compiled code/main.js and renderer/index.html",
        ));
    }
    let app = &pair.authoring.app;
    render_config(&app.id, &app.version, &app.name, entry, renderer)
}

/// Generate syntax that the existing Core configuration selectors ACTUALLY read.
/// The old one-line object falls back to src/main.ts and index.html in KEL-CLI-032,
/// so a positive parser round-trip is an admission gate, not an assumption.
fn render_config(
    id: &str,
    version: &str,
    name: &str,
    entry: &str,
    renderer: &str,
) -> Result<Vec<u8>, BuildError> {
    let config = format!(
        "export default {{\n  id: {},\n  version: {},\n  name: {},\n  entry: {},\n  renderer: {},\n}} as const;\n",
        quoted(id)?,
        quoted(version)?,
        quoted(name)?,
        quoted(entry)?,
        quoted(renderer)?
    );
    if keld_core::entry_from_config_ts(&config).as_deref() != Some(entry)
        || keld_core::renderer_from_config_ts(&config).as_deref() != Some(renderer)
    {
        return Err(refuse(
            "generated developer config is not understood by Core's actual selectors",
        ));
    }
    Ok(config.into_bytes())
}
/// Choose one Windows leaf for a locally signed application entrypoint; a
/// developer-controlled title must never become a path, device name or CLI.
fn launcher_name(name: &str) -> Result<String, BuildError> {
    if name.is_empty()
        || name.len() > 80
        || name.ends_with([' ', '.'])
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'-' | b'_' | b'.'))
    {
        return Err(refuse("app name is not a safe Windows executable leaf"));
    }
    let first = name.split('.').next().unwrap_or_default();
    let upper = first.trim_end().to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "KELD" | "BUN" | "KELD-HOST"
    ) || (upper.len() == 4
        && (upper.starts_with("COM") || upper.starts_with("LPT"))
        && matches!(upper.as_bytes()[3], b'1'..=b'9'))
    {
        return Err(refuse("reserved Windows device or KELD executable name"));
    }
    Ok(format!("{name}.exe"))
}
const LAUNCH: &[u8] = b"@echo off\r\nsetlocal EnableExtensions DisableDelayedExpansion\r\nset \"PATH=%~dp0;%PATH%\"\r\ncd /d \"%~dp0project\"\r\n\"%~dp0keld.exe\" dev\r\nexit /b %errorlevel%\r\n";

/// Creates one fresh, self-contained LOCAL developer runner using already emitted,
/// authenticated signed Host and same-execution compiled application members.
/// Only normal KEL-96 dev authority is used when Run.cmd executes. No KEL-53
/// installed-provenance writer, user credential import or extra signer is invoked.
///
/// # Errors
/// Refuses any missing compiled bundle member, path/size/digest discrepancy or
/// output collision. A failure leaves the partial unique output for diagnosis.
pub(super) fn emit(pair: &PreparedLocalPair, members: &[Member]) -> Result<PathBuf, BuildError> {
    let config = bundle_recipe(pair, members)?;
    let root = pair.session.join("developer-runnable-bundle");
    boot::create_windows_stage_root(&root).map_err(refuse)?;
    let _root_pins = windows_inputs::pin_input_root(&root)?;
    let project = root.join("project");
    fs::create_dir(&project).map_err(refuse)?;
    for folder in ["code", "renderer"] {
        fs::create_dir(project.join(folder)).map_err(refuse)?;
    }
    // No paths supplied by the caller enter this selection; Pack already
    // validated the entire member namespace before archive publication.
    for member in members {
        let target = match member.name.as_str() {
            "keld-host.exe" | "bun.exe" => Some(root.join(&member.name)),
            "code/main.js"
            | "code/kipc-transport.js"
            | "renderer/index.html"
            | "keld.permissions.jsonc" => Some(project.join(&member.name)),
            _ => None,
        };
        if let Some(target) = target {
            let content = member
                .content
                .as_ref()
                .ok_or_else(|| refuse("required bundle member is not a file"))?;
            copy_member(&target, content)?;
        }
    }
    for required in [
        root.join("keld-host.exe"),
        root.join("bun.exe"),
        project.join("code/main.js"),
        project.join("code/kipc-transport.js"),
        project.join("renderer/index.html"),
        project.join("keld.permissions.jsonc"),
    ] {
        if !required.is_file() {
            return Err(refuse("required compiled runtime member missing"));
        }
    }
    write_bytes(&project.join("keld.config.ts"), &config)?;
    let cli = std::env::current_exe().map_err(refuse)?;
    let cli_file = super::local_source::pin_file(&cli, MAX_COPY)?;
    copy_file(
        &root.join("keld.exe"),
        &cli_file.file,
        cli_file.file.metadata().map_err(refuse)?.len(),
    )?;
    // The included CLI is invoked by the named GUI entrypoint in a child
    // process with a pinned PATH. Authenticate this child with the SAME
    // previously captured developer signer; do not ship a mutable unsigned
    // executable behind a trusted application launcher.
    let cli_sha = super::local_images::sign_named_developer_launcher(pair, &root.join("keld.exe"))?;
    write_bytes(&root.join("Run.cmd"), LAUNCH)?;
    // The application-named executable uses only the normal KEL-96 dev lease.
    // Neither EXE is an installed KEL-53 principal or Store release authority.
    let launcher = root.join(launcher_name(&pair.authoring.app.name)?);
    copy_file(
        &launcher,
        &cli_file.file,
        cli_file.file.metadata().map_err(refuse)?.len(),
    )?;
    let launcher_sha = super::local_images::sign_named_developer_launcher(pair, &launcher)?;
    let host = keld_guard::WindowsAuthenticodeImage::open(&root.join("keld-host.exe"))
        .map_err(refuse)?
        .verify()
        .map_err(refuse)?;
    if host.identity() != pair.host.verified.identity() {
        return Err(refuse(
            "bundle Host publisher/app id changed after signed candidate",
        ));
    }
    println!(
        "Prepared Windows LOCAL DEVELOPER runnable folder: {} (Run.cmd and {} use KEL-96 dev lease, not installed provenance, Strict or Store)",
        root.display(),
        launcher.file_name().unwrap_or_default().to_string_lossy()
    );
    println!(
        "Verified LOCAL DEVELOPER application entrypoint Authenticode: {} SHA256 {}",
        launcher.display(),
        super::local_source::observed_hex(&launcher_sha)
    );
    println!(
        "Verified LOCAL DEVELOPER co-bundled CLI Authenticode: SHA256 {}",
        super::local_source::observed_hex(&cli_sha)
    );
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_core_selectors_read_generated_compiled_app_and_renderer() {
        let bytes = render_config(
            "com.example.keld.runnablehello",
            "1.0.0",
            "windows-runnable-hello",
            "code/main.js",
            "renderer/index.html",
        )
        .expect("literal generated config must parse");
        let config = String::from_utf8(bytes).unwrap();
        assert_eq!(
            keld_core::entry_from_config_ts(&config).as_deref(),
            Some("code/main.js")
        );
        assert_eq!(
            keld_core::renderer_from_config_ts(&config).as_deref(),
            Some("renderer/index.html")
        );
    }
    #[test]
    fn app_exe_name_accepts_draw_io_and_refuses_win32_aliases() {
        assert_eq!(launcher_name("draw.io").unwrap(), "draw.io.exe");
        assert_eq!(
            launcher_name("windows-runnable-hello").unwrap(),
            "windows-runnable-hello.exe"
        );
        for name in [
            "",
            ".",
            "bad.",
            "bad ",
            "../evil",
            r"foo\evil",
            "foo:bar",
            "CON",
            "NUL.txt",
            "COM1",
            "LPT9",
            "keld",
            "keld-host",
            "bun",
            "executable<bad>",
            "literal*wildcard",
        ] {
            assert!(launcher_name(name).is_err(), "accepted {name:?}");
        }
    }

    #[test]
    fn launcher_invokes_only_dev_lease_not_bare_signed_exe() {
        let text = std::str::from_utf8(LAUNCH).unwrap();
        assert!(text.contains("keld.exe\" dev"));
        assert!(!text.contains("keld-host.exe\""));
        assert!(text.contains("cd /d \"%~dp0project\""));
        assert!(!text.contains("--keld-install-initial"));
    }
}
