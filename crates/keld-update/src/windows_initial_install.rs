//! One-shot Windows developer initial install from a signed Host and signed catalog.
//! This is not normal Host boot, an update, or an OS-containment admission.
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

use crate::windows_extraction::{ensure_regular, open_ancestors, open_source};
use crate::windows_fs::{create_directory_relative_with_profile, qualified_volume_root};
use crate::{
    ArtifactIdentity, BaselineVerifier, DirectInstallMode, DirectInstallationIdentity,
    ExpectedAppIdentity, InstallOwner, PrincipalModel, SigningKeyId, UpdateError,
    WindowsBaselineTrust, WindowsLocatedImage, initialize_windows_per_user_baseline,
    select_active_package_for_executable, verify_literal_release_signature,
};

const MAX_REQUEST: u64 = 64 * 1024;
const MAX_CATALOG: u64 = 64 * 1024;
const MAX_MANIFEST: u64 = 1024 * 1024;
const MAX_SIGNATURE: u64 = 89;
const MAX_RAW_SIGNATURE: u64 = 64;

fn refuse(detail: impl std::fmt::Display) -> UpdateError {
    UpdateError::Baseline {
        step: "signed Windows initial-install adapter",
        detail: detail.to_string(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
struct Request {
    schema: String,
    install_root: PathBuf,
    update_root: PathBuf,
    version: String,
    scaffold_mode: ScaffoldMode,
    catalog: PathBuf,
    catalog_signature: PathBuf,
    manifest: PathBuf,
    manifest_signature: PathBuf,
    compressed_archive: PathBuf,
    release_public_key: [u8; 32],
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ScaffoldMode {
    Existing,
    CreateNew,
}

/// Exact signed installer configuration. Request paths are locators, never authority.
/// This signature is independently bound to the release key embedded in a verified PE.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Catalog {
    schema: String,
    app_id: String,
    channel: String,
    target: String,
    version: String,
    install_root: PathBuf,
    update_root: PathBuf,
    publisher_scope: String,
    volume_guid: String,
    profile_digest: String,
    principal_model: String,
    content_blake3: String,
}

fn pinned(path: &Path, max: u64) -> Result<(File, Vec<u8>), UpdateError> {
    if !path.is_absolute() {
        return Err(refuse("an absolute file locator is required"));
    }
    let mut file = open_source(path).map_err(refuse)?.into_std();
    let meta = cap_std::fs::Metadata::from_file(&file).map_err(refuse)?;
    ensure_regular(&meta).map_err(refuse)?;
    if meta.len() == 0 || meta.len() > max {
        return Err(refuse("input byte length exceeds its closed bound"));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(refuse)?;
    if bytes.len() as u64 != meta.len() {
        return Err(refuse("input changed across retained read"));
    }
    file.seek(SeekFrom::Start(0)).map_err(refuse)?;
    Ok((file, bytes))
}

fn hex32(hex: &str) -> Result<[u8; 32], UpdateError> {
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(refuse("expected one exact lowercase 32-byte hex value"));
    }
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).map_err(refuse)?;
    }
    Ok(digest)
}

fn catalog_trust(
    request: &Request,
    catalog: &Catalog,
    expected: &ExpectedAppIdentity,
    signature_publisher: &[u8; 32],
    public_key: &[u8; 32],
) -> Result<WindowsBaselineTrust, UpdateError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut publisher_hex = String::with_capacity(64);
    for byte in signature_publisher {
        publisher_hex.push(char::from(HEX[usize::from(byte >> 4)]));
        publisher_hex.push(char::from(HEX[usize::from(byte & 15)]));
    }
    if catalog.schema != "keld.windows-initial-install-catalog/v1"
        || request.version != catalog.version
        || request.install_root != catalog.install_root
        || request.update_root != catalog.update_root
        || request.release_public_key != *public_key
        || catalog.app_id != expected.app_id
        || catalog.channel != expected.channel.as_str()
        || catalog.target != expected.target
        || catalog.target != "windows-x64"
        || catalog.principal_model != "strict-distinct-os-principals"
        || catalog.publisher_scope != publisher_hex
    {
        return Err(refuse(
            "signed catalog and current signed Host/request identity disagree",
        ));
    }
    let content = hex32(&catalog.content_blake3)?;
    let profile = hex32(&catalog.profile_digest)?;
    if profile == [0; 32] {
        return Err(refuse(
            "unverified zero security-profile digest is not an install claim",
        ));
    }
    let channel = expected.channel;
    let artifact = ArtifactIdentity {
        app_id: catalog.app_id.clone(),
        channel,
        target: catalog.target.clone(),
        version: catalog.version.clone(),
        content_blake3: content,
    };
    let identity = DirectInstallationIdentity {
        install_mode: DirectInstallMode::PerUserDirect,
        app_id: catalog.app_id.clone(),
        channel,
        target: catalog.target.clone(),
        install_root: catalog.install_root.clone(),
        update_root: catalog.update_root.clone(),
        signing_key_id: SigningKeyId::from_public_key(public_key),
        baseline: artifact,
        profile_digest: keld_guard::ProfileDigest(profile),
        principal_model: PrincipalModel::StrictDistinctOsPrincipals,
    };
    Ok(WindowsBaselineTrust {
        installation: identity,
        owner: InstallOwner::Direct,
        publisher_scope: *signature_publisher,
        volume_guid: catalog.volume_guid.clone(),
    })
}

fn create_scaffold(trust: &WindowsBaselineTrust) -> Result<(), UpdateError> {
    keld_guard::require_windows_non_system_token().map_err(refuse)?;
    let identity = &trust.installation;
    if identity.update_root.parent() != Some(identity.install_root.as_path()) {
        return Err(refuse(
            "signed update root must be one direct child of install root",
        ));
    }
    let parent_path = identity
        .install_root
        .parent()
        .ok_or_else(|| refuse("install parent absent"))?;
    let install_leaf = identity
        .install_root
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| refuse("invalid install directory name"))?;
    let update_leaf = identity
        .update_root
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| refuse("invalid update directory name"))?;
    let parents = open_ancestors(parent_path).map_err(refuse)?;
    let parent = parents
        .last()
        .ok_or_else(|| refuse("parent anchor absent"))?
        .try_clone()
        .map_err(refuse)?
        .into_std_file();
    let actual = qualified_volume_root(&parent).map_err(refuse)?;
    if !actual.eq_ignore_ascii_case(&trust.volume_guid) {
        return Err(refuse("trusted volume and actual parent volume disagree"));
    }
    let profile = keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate;
    let install =
        create_directory_relative_with_profile(&parent, install_leaf, profile).map_err(refuse)?;
    let update =
        create_directory_relative_with_profile(&install, update_leaf, profile).map_err(refuse)?;
    let _versions =
        create_directory_relative_with_profile(&update, "versions", profile).map_err(refuse)?;
    Ok(())
}

/// Installs exactly one release authenticated by the verified current Host's release key.
/// The caller must run in a separate private role, before Host Job/WebView/Bun creation.
/// All trust facts come from the signed image and separate literal signed catalog.
/// Failures preserve partial diagnostic state; never retry or repair an existing root.
///
/// # Errors
/// Refuses any missing or mismatched signature, profile, baseline, version, OS root,
/// source change, malformed request or unsupported installation state.
pub fn install_windows_initial_from_signed_host(
    request_path: &Path,
) -> Result<PathBuf, UpdateError> {
    let executable = std::env::current_exe().map_err(refuse)?;
    let signed = keld_guard::WindowsAuthenticodeImage::open(&executable)
        .map_err(refuse)?
        .verify()
        .map_err(refuse)?;
    let expected = ExpectedAppIdentity::from_signed_image(signed.file())?;
    let payload = keld_pack::read_host_identity(signed.file()).map_err(refuse)?;
    let public_key = *payload.update_public_key();
    if expected.signing_key_id != SigningKeyId::from_public_key(&public_key)
        || signed.identity().app_id() != expected.app_id
    {
        return Err(refuse(
            "verified PE publisher, app id and embedded release key disagree",
        ));
    }
    let (_request_pin, request_bytes) = pinned(request_path, MAX_REQUEST)?;
    let request: Request = serde_json::from_slice(&request_bytes).map_err(refuse)?;
    if request.schema != "keld.windows-initial-install-request/v2" {
        return Err(refuse("unsupported initial-install request schema"));
    }
    let (_catalog_pin, catalog_bytes) = pinned(&request.catalog, MAX_CATALOG)?;
    let (_catalog_sig_pin, catalog_signature) =
        pinned(&request.catalog_signature, MAX_RAW_SIGNATURE)?;
    if catalog_signature.len() != 64 {
        return Err(refuse(
            "catalog requires exactly one 64-byte raw Ed25519 signature",
        ));
    }
    // Reuse Update's sole Ed25519 verification rule, converting the raw carrier to
    // the canonical base64 detached carrier expected by that rule.
    let mut signature = [0u8; 89];
    let encoded = STANDARD
        .encode_slice(&catalog_signature, &mut signature[..88])
        .map_err(refuse)?;
    signature[encoded] = b'\n';
    verify_literal_release_signature(public_key, &catalog_bytes, &signature[..=encoded])?;
    let catalog: Catalog = serde_json::from_slice(&catalog_bytes).map_err(refuse)?;
    if serde_json::to_vec(&catalog).map_err(refuse)? != catalog_bytes {
        return Err(refuse(
            "signed catalog bytes must use the exact canonical schema encoding",
        ));
    }
    let trust = catalog_trust(
        &request,
        &catalog,
        &expected,
        signed.identity().publisher_scope(),
        &public_key,
    )?;
    let (_manifest_pin, manifest) = pinned(&request.manifest, MAX_MANIFEST)?;
    let (_manifest_sig_pin, manifest_sig) = pinned(&request.manifest_signature, MAX_SIGNATURE)?;
    let mut compressed = open_source(&request.compressed_archive).map_err(refuse)?;
    let selected = BaselineVerifier::new(trust.installation.clone(), public_key)?
        .verify_manifest(&manifest, &manifest_sig)?;
    let output = request
        .compressed_archive
        .parent()
        .ok_or_else(|| refuse("candidate parent absent"))?
        .join("authenticated-initial-baseline.tar.candidate");
    let mut content = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(&output)
        .map_err(refuse)?;
    let verified = selected.verify_full(&mut compressed, &mut content)?;
    content.sync_all().map_err(refuse)?;
    drop(content);
    let mut pinned_content = open_source(&output).map_err(refuse)?;
    verified.validate_windows_archive(&mut pinned_content)?;
    if request.scaffold_mode == ScaffoldMode::CreateNew {
        create_scaffold(&trust)?;
    }
    let receipt = initialize_windows_per_user_baseline(&verified, &output, &trust)?;
    drop(receipt);
    verify_newly_installed_host(&trust)
}

fn verify_newly_installed_host(trust: &WindowsBaselineTrust) -> Result<PathBuf, UpdateError> {
    let installed = trust
        .installation
        .update_root
        .join("versions")
        .join(&trust.installation.baseline.version)
        .join("tree")
        .join("keld-host.exe");
    let installed_image = keld_guard::WindowsAuthenticodeImage::open(&installed)
        .map_err(refuse)?
        .verify()
        .map_err(refuse)?;
    let installed_expected = ExpectedAppIdentity::from_signed_image(installed_image.file())?;
    let selected = select_active_package_for_executable(
        WindowsLocatedImage::Host,
        &installed,
        installed_image.file(),
        &installed_expected,
        installed_image.identity().publisher_scope(),
        installed_image.identity().app_id(),
    )?;
    if selected.tree_root()
        != installed
            .parent()
            .ok_or_else(|| refuse("installed tree absent"))?
    {
        return Err(refuse(
            "selected active tree is not the newly installed baseline",
        ));
    }
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_catalog_parser_refuses_unknown_fields_or_unverified_profile() {
        let input = serde_json::json!({
            "schema":"keld.windows-initial-install-request/v2", "install_root":"D:/app",
            "update_root":"D:/app/updates", "version":"1.0.0", "scaffold_mode":"create-new",
            "catalog":"D:/c", "catalog_signature":"D:/s", "manifest":"D:/m",
            "manifest_signature":"D:/ms", "compressed_archive":"D:/archive",
            "release_public_key":vec![9; 32]});
        assert!(serde_json::from_value::<Request>(input.clone()).is_ok());
        let mut invalid = input;
        invalid["release_key_override"] = serde_json::json!([3, 4]);
        assert!(serde_json::from_value::<Request>(invalid).is_err());
        assert!(hex32(&"00".repeat(32)).is_ok());
        assert!(hex32(&"GG".repeat(32)).is_err());
        assert!(hex32(&"ff".repeat(31)).is_err());
    }
}
