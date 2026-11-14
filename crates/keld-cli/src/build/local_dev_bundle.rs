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
/// Windows KEL-96 writes the literal fixed policy bytes into its dev stage.
/// No explicit grant, offline CSP, comment or alternate byte encoding may be
/// silently reinterpreted as that policy. Enforce exact byte replay before the
/// named launcher or bundled CLI is signed.
fn require_replayable_permissions(members: &[Member]) -> Result<(), BuildError> {
    let permissions = required_json(members, "keld.permissions.jsonc")?;
    let text = std::str::from_utf8(permissions).map_err(refuse)?;
    keld_guard::parse_manifest(text).map_err(refuse)?;
    if permissions != boot::PERMISSIONS_BYTES {
        return Err(refuse(
            "Windows KEL-96 dev stage cannot replay source permission policy bytes; only the exact empty undeclared policy is supported",
        ));
    }
    Ok(())
}

fn quoted(value: &str) -> Result<String, BuildError> {
    serde_json::to_string(value).map_err(refuse)
}
/// The developer folder reconstructs a minimal keld.config.ts from the
/// *compiled*, retained boot descriptor. It must never erase auth or change
/// the app identity: exact parsed auth round-trip is enforced below.
fn require_replayable_boot_descriptor(
    descriptor: &serde_json::Value,
    app_name: &str,
) -> Result<(), BuildError> {
    if descriptor.get("name").and_then(serde_json::Value::as_str) != Some(app_name) {
        return Err(refuse(
            "compiled boot app name differs from the retained authoring identity",
        ));
    }
    if descriptor
        .get("auth")
        .is_some_and(|value| !value.is_object())
    {
        return Err(refuse("compiled auth must be a literal object"));
    }
    Ok(())
}
fn bundle_recipe(pair: &PreparedLocalPair, members: &[Member]) -> Result<Vec<u8>, BuildError> {
    let descriptor: serde_json::Value =
        serde_json::from_slice(required_json(members, "keld.boot.json")?).map_err(refuse)?;
    require_replayable_boot_descriptor(&descriptor, &pair.authoring.app.name)?;
    require_replayable_permissions(members)?;
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
    render_config(
        &app.id,
        &app.version,
        &app.name,
        entry,
        renderer,
        descriptor.get("auth"),
    )
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
    compiled_auth: Option<&serde_json::Value>,
) -> Result<Vec<u8>, BuildError> {
    let mut config = format!(
        "export default {{\n  id: {},\n  version: {},\n  name: {},\n  entry: {},\n  renderer: {},\n",
        quoted(id)?,
        quoted(version)?,
        quoted(name)?,
        quoted(entry)?,
        quoted(renderer)?
    );
    if let Some(auth) = compiled_auth {
        if !auth.is_object() {
            return Err(refuse("compiled auth is not an object"));
        }
        config.push_str("  auth: ");
        config.push_str(&serde_json::to_string(auth).map_err(refuse)?);
        config.push_str(",\n");
    }
    // This marker controls only sidecar staging. It never grants installed
    // trust, permissions or an alternate Host boot path.
    config.push_str("  ");
    config.push_str(boot::COMPILED_KEL96_MARKER_LINE);
    config.push('\n');
    config.push_str("} as const;\n");
    if keld_core::entry_from_config_ts(&config).as_deref() != Some(entry)
        || keld_core::renderer_from_config_ts(&config).as_deref() != Some(renderer)
    {
        return Err(refuse(
            "generated developer config is not understood by Core's actual selectors",
        ));
    }
    // Core and Native own auth parsing/policy. Reparse through the exact
    // production Core reader; comparing JSON prevents changed/dropped fields
    // or silent provider credential downgrades at the developer bundle boundary.
    let replayed = keld_core::auth_configuration_from_config_ts(&config).map_err(refuse)?;
    let replayed = serde_json::to_value(replayed).map_err(refuse)?;
    let expected = compiled_auth
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if replayed != expected {
        return Err(refuse(
            "generated developer config does not preserve compiled auth configuration",
        ));
    }
    Ok(config.into_bytes())
}
// Developer launcher name validation belongs to the input admission owner in
// windows_inputs and runs before Host compilation or signing.
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
    let exe_name = windows_inputs::developer_launcher_name(&pair.authoring.app.name)?;
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
    let launcher = root.join(exe_name);
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
            None,
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
        assert_eq!(
            windows_inputs::developer_launcher_name("draw.io").unwrap(),
            "draw.io.exe"
        );
        assert_eq!(
            windows_inputs::developer_launcher_name("windows-runnable-hello").unwrap(),
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
            assert!(
                windows_inputs::developer_launcher_name(name).is_err(),
                "accepted {name:?}"
            );
        }
    }
    #[test]
    fn compiled_auth_replays_exactly_through_the_production_core_reader() {
        let no_auth = serde_json::json!({"name": "Example"});
        assert!(require_replayable_boot_descriptor(&no_auth, "Example").is_ok());
        let auth = serde_json::json!({
            "entra-common": {
                "clientId": "public-client",
                "fixedRedirect": "http://localhost:53161/callback"
            },
            "future-provider": {"asRedirect": "registered-return"}
        });
        let descriptor = serde_json::json!({"name": "Example", "auth": auth});
        assert!(require_replayable_boot_descriptor(&descriptor, "Example").is_ok());
        let bytes = render_config(
            "com.example.keld",
            "1.0.0",
            "Example",
            "code/main.js",
            "renderer/index.html",
            descriptor.get("auth"),
        )
        .expect("compiled auth must round-trip");
        let config = String::from_utf8(bytes).unwrap();
        assert_eq!(
            serde_json::to_value(keld_core::auth_configuration_from_config_ts(&config).unwrap())
                .unwrap(),
            descriptor["auth"]
        );
    }
    #[test]
    fn malformed_or_unreplayable_compiled_auth_fails_closed() {
        for auth in [
            serde_json::json!(null),
            serde_json::json!([]),
            serde_json::json!({"entra-common": {"clientID": "unknown-field"}}),
            serde_json::json!({"entra-common": {"clientId": 123}}),
            serde_json::json!({"entra-common": {"clientId": "x", "secret": "forbidden"}}),
        ] {
            let descriptor = serde_json::json!({"name": "Example", "auth": auth});
            assert!(
                require_replayable_boot_descriptor(&descriptor, "Example").is_err()
                    || render_config(
                        "com.example.keld",
                        "1.0.0",
                        "Example",
                        "code/main.js",
                        "renderer/index.html",
                        descriptor.get("auth")
                    )
                    .is_err(),
                "invalid compiled auth was admitted: {auth:?}"
            );
        }
    }
    #[test]
    fn compiled_boot_app_identity_must_match_authoring() {
        let mismatch = serde_json::json!({"name": "Another Application"});
        assert!(require_replayable_boot_descriptor(&mismatch, "Example").is_err());
        let malformed = serde_json::json!([]);
        assert!(require_replayable_boot_descriptor(&malformed, "Example").is_err());
    }

    #[test]
    fn launcher_invokes_only_dev_lease_not_bare_signed_exe() {
        let text = std::str::from_utf8(LAUNCH).unwrap();
        assert!(text.contains("keld.exe\" dev"));
        assert!(!text.contains("keld-host.exe\""));
        assert!(text.contains("cd /d \"%~dp0project\""));
        assert!(!text.contains("--keld-install-initial"));
    }
    #[test]
    fn admitted_permissions_must_match_the_actual_windows_dev_stage_bytes() {
        fn member(bytes: &[u8]) -> Vec<Member> {
            vec![Member {
                name: "keld.permissions.jsonc".into(),
                content: Some(Content::Generated(bytes.to_vec())),
            }]
        }
        assert!(require_replayable_permissions(&member(boot::PERMISSIONS_BYTES)).is_ok());
        for source in [
            br"{}" as &[u8],
            br#"{ "app": { "fs": { "read": ["/home"] } } }"#,
            br#"{ "windows": { "primary": { "web": { "csp": "default-src 'self'" } } } }"#,
            br#"{ "profile": null }"#,
            b"// benign comment\n{}\n",
        ] {
            assert!(
                require_replayable_permissions(&member(source)).is_err(),
                "incorrectly accepted non-identical staged permissions"
            );
        }
    }
    #[test]
    fn generated_marker_is_present_and_not_an_installed_admission_claim() {
        let output = render_config(
            "com.example.keld.runnablehello",
            "1.0.0",
            "windows-runnable-hello",
            "code/main.js",
            "renderer/index.html",
            None,
        )
        .unwrap();
        let config = std::str::from_utf8(&output).unwrap();
        assert!(
            config
                .lines()
                .any(|line| line.trim() == boot::COMPILED_KEL96_MARKER_LINE)
        );
        assert!(!config.contains("windows-initial-install"));
        assert!(!config.contains("Strict"));
    }
    #[test]
    fn bundle_parent_handles_refuse_rename_until_child_launch_is_retired() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("signed-bundle");
        std::fs::create_dir(&bundle).unwrap();
        let guards = boot::pin_windows_launch_ancestors(&bundle).unwrap();
        let moved = temp.path().join("replaced-bundle");
        assert!(std::fs::rename(&bundle, &moved).is_err());
        drop(guards);
        std::fs::rename(&bundle, &moved).unwrap();
    }
}
