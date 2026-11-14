//! Explicit LOCAL TEST image preparation. Never release or installed-app authority.
//!
//! Consumes same-execution observed source outputs. No external receipt or arbitrary
//! Host/helper path can enter this action. Authenticode and Ed25519 update identities
//! are deliberately separate. Existing certificates only; never creates keys/trust.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::os::windows::fs::FileExt as _;
use std::path::{Path, PathBuf};

use keld_guard::{VerifiedWindowsImage, WindowsAuthenticodeImage};
use keld_pack::{ExpectedAppIdentityPayload, check_host_identity_post_sign, embed_host_identity};
use sha2::{Digest, Sha256};

use super::{BuildError, local_source, windows_inputs};
use crate::boot;

const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

fn refuse(phase: &'static str, error: impl std::fmt::Display) -> BuildError {
    BuildError::input(phase, error)
}

pub(super) fn hex<const N: usize>(value: &str) -> Result<[u8; N], BuildError> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(refuse(
            "local image identity",
            "exact hexadecimal identity required",
        ));
    }
    let mut result = [0; N];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|error| refuse("local image identity", error))?;
    }
    Ok(result)
}

fn read_exact_image(file: &File, limit: u64) -> Result<Vec<u8>, BuildError> {
    let length = file
        .metadata()
        .map_err(|error| refuse("local image read", error))?
        .len();
    if length > limit {
        return Err(refuse("local image read", "image exceeds byte ceiling"));
    }
    let length = usize::try_from(length).map_err(|error| refuse("local image read", error))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|error| refuse("local image read", error))?;
    bytes.resize(length, 0);
    let mut done = 0;
    while done < length {
        let offset = u64::try_from(done).map_err(|error| refuse("local image read", error))?;
        let count = file
            .seek_read(&mut bytes[done..], offset)
            .map_err(|error| refuse("local image read", error))?;
        if count == 0 {
            return Err(refuse("local image read", "retained image truncated"));
        }
        done += count;
    }
    Ok(bytes)
}

/// Caller-selected LOCAL TEST signer identity, not a production qualification.
struct LocalSigner {
    tool: local_source::PinnedFile,
    certificate_sha1: String,
    // Guard domain-separated publisher scope, NOT plain SHA256(SPKI).
    publisher_scope: [u8; 32],
    _directory_pins: Vec<File>,
}

impl LocalSigner {
    fn capture(
        tool: &Path,
        expected_tool: &str,
        certificate: &str,
        publisher: &str,
    ) -> Result<Self, BuildError> {
        let parent = tool
            .parent()
            .ok_or_else(|| refuse("local signer", "tool parent missing"))?;
        let directory_pins = windows_inputs::pin_input_root(parent)?;
        let tool = local_source::pin_file(tool, MAX_IMAGE_BYTES)?;
        if tool.sha256 != hex::<32>(expected_tool)? {
            return Err(refuse(
                "local signer",
                "selected SignTool bytes differ from expected SHA256",
            ));
        }
        hex::<20>(certificate)?;
        Ok(Self {
            tool,
            certificate_sha1: certificate.to_ascii_uppercase(),
            publisher_scope: hex::<32>(publisher)?,
            _directory_pins: directory_pins,
        })
    }
}

fn sign_args(certificate: &str, app_id: &str, candidate: &Path) -> Vec<OsString> {
    // Explicit CurrentUser\My only. No /a, /as, /sm, PFX, passwords or fallback.
    [
        "sign",
        "/s",
        "My",
        "/sha1",
        certificate,
        "/fd",
        "SHA256",
        "/d",
    ]
    .map(OsString::from)
    .into_iter()
    .chain([
        OsString::from(format!("keld.app-id/v1:{app_id}")),
        candidate.as_os_str().to_owned(),
    ])
    .collect()
}

fn with_admitted_lengths<T>(
    host_bytes: usize,
    helper_bytes: usize,
    action: impl FnOnce() -> Result<T, BuildError>,
) -> Result<T, BuildError> {
    for length in [host_bytes, helper_bytes] {
        if length as u64 > MAX_IMAGE_BYTES {
            return Err(refuse(
                "local embedded pair",
                "embedded image exceeds byte ceiling before signing",
            ));
        }
    }
    action()
}

pub(super) struct PreparedLocalImage {
    pub(super) executable: local_source::PinnedFile,
    // Retained through pair completion; later consumers cannot reopen by path.
    pub(super) verified: VerifiedWindowsImage,
    pub(super) bytes: Vec<u8>,
}

fn sign_and_admit_windows_image(
    baseline: &[u8],
    expected: &ExpectedAppIdentityPayload,
    candidate: &Path,
    signer: &LocalSigner,
    environment: &[(OsString, OsString)],
    session: &Path,
) -> Result<PreparedLocalImage, BuildError> {
    if baseline.len() as u64 > MAX_IMAGE_BYTES {
        return Err(refuse(
            "local image baseline",
            "embedded image exceeds ceiling",
        ));
    }
    {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(candidate)
            .map_err(|error| refuse("local image candidate", error))?;
        output
            .write_all(baseline)
            .map_err(|error| refuse("local image candidate", error))?;
        output
            .sync_all()
            .map_err(|error| refuse("local image candidate", error))?;
    }
    let input = File::open("NUL").map_err(|error| refuse("local signer stdin", error))?;
    local_source::run_captured(
        &signer.tool,
        &sign_args(&signer.certificate_sha1, expected.app_id(), candidate),
        environment,
        session,
        &input,
        64 * 1024,
    )?;
    // Admit byte size before crypto or signer verification. Retain this exact
    // non-reparse leaf pin through Guard's open so namespace replacement cannot
    // substitute a larger object between the precheck and trust verification.
    let candidate_pin = local_source::pin_file(candidate, MAX_IMAGE_BYTES)?;
    let verified = WindowsAuthenticodeImage::open(candidate)
        .and_then(WindowsAuthenticodeImage::verify)
        .map_err(|error| refuse("local signed image", error))?;
    // Retain the exact signed candidate pin through later Host execution.
    if verified.identity().app_id() != expected.app_id()
        || verified.identity().publisher_scope() != &signer.publisher_scope
    {
        return Err(refuse(
            "local signed identity",
            "app id or expected publisher scope differs",
        ));
    }
    let bytes = read_exact_image(verified.file(), MAX_IMAGE_BYTES)?;
    check_host_identity_post_sign(baseline, &bytes, expected)
        .map_err(|error| refuse("local signed byte integrity", error))?;
    keld_update::ExpectedAppIdentity::from_signed_image(verified.file())
        .map_err(|error| refuse("local signed update identity", error))?;
    // The Guard pin stays held while SignTool opens the same immutable candidate.
    // No /tw: this explicitly selected local-only cell is untimestamped.
    let mut args: Vec<OsString> = ["verify", "/pa", "/v"].map(OsString::from).into();
    args.push(candidate.as_os_str().to_owned());
    local_source::run_captured(&signer.tool, &args, environment, session, &input, 64 * 1024)?;
    Ok(PreparedLocalImage {
        verified,
        bytes,
        executable: candidate_pin,
    })
}

/// The optional cache mode changes dependency observation, never signing authority.
pub(super) fn image_source_args(args: &[String]) -> Result<Vec<String>, BuildError> {
    if args.len() != 15 && !(args.len() == 16 && args[15] == "--use-developer-cache") {
        return Err(BuildError::invocation(
            "expected fifteen local-image inputs, optionally followed by --use-developer-cache",
        ));
    }
    let mut source = args[..7].to_vec();
    if args.len() == 16 {
        source.push(args[15].clone());
    }
    Ok(source)
}

/// Typed data-only inputs shared by diagnostic and ordinary product routes.
/// Existing capture/signing owners retain all validation and operation authority.
pub(super) struct ImageRequest {
    pub(super) source: local_source::Request,
    pub(super) app_root: PathBuf,
    pub(super) channel: String,
    pub(super) release_public_key: PathBuf,
    pub(super) sign_tool: PathBuf,
    pub(super) sign_tool_sha256: String,
    pub(super) certificate_thumbprint: String,
    pub(super) publisher_scope: String,
    pub(super) profile_admission: windows_inputs::ProfileAdmission,
}
pub(super) struct PreparedLocalPair {
    pub(super) release_public_key: [u8; 32],
    pub(super) authoring: windows_inputs::CapturedAuthoring,
    pub(super) host: PreparedLocalImage,
    pub(super) helper: PreparedLocalImage,
    pub(super) source: local_source::LocalSourceBuildObservation,
    pub(super) session: PathBuf,
    // Same retained authoring namespace; archive callers cannot supply another root.
    pub(super) app_root: PathBuf,
    // Retain this exact already-qualified signer/tool handle for the developer
    // app entrypoint, not a second independently selected certificate.
    signer: LocalSigner,
    signer_environment: Vec<(OsString, OsString)>,
    _image_directories: Vec<File>,
}

#[allow(clippy::too_many_lines)] // one transaction retains source, signer and both verified-image witnesses
pub(super) fn prepare(args: &[String]) -> Result<PreparedLocalPair, BuildError> {
    prepare_with_role(args, false)
}
#[allow(clippy::too_many_lines)] // same transaction; explicit build role never enables a qualification authority
fn prepare_with_role(
    args: &[String],
    initial_install: bool,
) -> Result<PreparedLocalPair, BuildError> {
    prepare_request(request_from_args(args)?, initial_install)
}
pub(super) fn request_from_args(args: &[String]) -> Result<ImageRequest, BuildError> {
    let source_args = image_source_args(args)?;
    if args[14] != "local-test-untimestamped" {
        return Err(refuse(
            "local timestamp mode",
            "only explicit local-test-untimestamped is implemented; no production timestamp claim",
        ));
    }
    Ok(ImageRequest {
        source: local_source::request_from_args(&source_args)?,
        app_root: args[7].clone().into(),
        channel: args[8].clone(),
        release_public_key: args[9].clone().into(),
        sign_tool: args[10].clone().into(),
        sign_tool_sha256: args[11].clone(),
        certificate_thumbprint: args[12].clone(),
        publisher_scope: args[13].clone(),
        profile_admission: windows_inputs::ProfileAdmission::ExplicitStrict,
    })
}
#[allow(clippy::too_many_lines)] // Same existing source/sign/image-custody transaction, not a second pipeline.
pub(super) fn prepare_request(
    request: ImageRequest,
    initial_install: bool,
) -> Result<PreparedLocalPair, BuildError> {
    let app_root = request.app_root.as_path();
    let inputs = windows_inputs::CapturedInputs::open(app_root)?;
    let authoring = match request.profile_admission {
        windows_inputs::ProfileAdmission::ExplicitStrict => inputs.capture(app_root)?,
        windows_inputs::ProfileAdmission::UndeclaredDeveloper => {
            inputs.capture_developer(app_root)?
        }
    };
    let app = &authoring.app;
    let key_path = request.release_public_key.as_path();
    let key_parent = key_path
        .parent()
        .ok_or_else(|| refuse("update public key", "parent missing"))?;
    let _key_directories = windows_inputs::pin_input_root(key_parent)?;
    let key = local_source::pin_file(key_path, 32)?;
    let key_bytes: [u8; 32] = read_exact_image(&key.file, 32)?.try_into().map_err(|_| {
        refuse(
            "update public key",
            "exactly32 raw public-key bytes required",
        )
    })?;
    let expected =
        ExpectedAppIdentityPayload::new(&app.id, &request.channel, "windows-x64", key_bytes)
            .map_err(|error| refuse("local expected identity", error))?;
    keld_update::ExpectedAppIdentity::decode(&expected.encode())
        .map_err(|error| refuse("local update identity", error))?;
    let signer = LocalSigner::capture(
        &request.sign_tool,
        &request.sign_tool_sha256,
        &request.certificate_thumbprint,
        &request.publisher_scope,
    )?;
    let session = request.source.session.clone();
    let source = local_source::prepare(request.source, initial_install)?;
    let images = session.join("local-test-images");
    boot::create_windows_stage_root(&images)
        .map_err(|error| refuse("local images directory", error))?;
    let image_directories = windows_inputs::pin_input_root(&images)?;
    let mut environment = Vec::new();
    for name in ["SystemRoot", "WINDIR", "PATH"] {
        if let Some(value) = std::env::var_os(name) {
            environment.push((name.into(), value));
        }
    }
    environment.push(("TEMP".into(), session.as_os_str().to_owned()));
    environment.push(("TMP".into(), session.as_os_str().to_owned()));
    let host = read_exact_image(&source.host.file, MAX_IMAGE_BYTES)?;
    let helper = read_exact_image(&source.helper.file, MAX_IMAGE_BYTES)?;
    let host = embed_host_identity(&host, &expected)
        .map_err(|error| refuse("local Host identity", error))?;
    let helper = embed_host_identity(&helper, &expected)
        .map_err(|error| refuse("local helper identity", error))?;
    // Both unsigned inputs must admit before either certificate operation.
    let (host, helper) = with_admitted_lengths(host.len(), helper.len(), || {
        let host = sign_and_admit_windows_image(
            &host,
            &expected,
            &images.join("keld-host.exe"),
            &signer,
            &environment,
            &session,
        )?;
        let helper = sign_and_admit_windows_image(
            &helper,
            &expected,
            &images.join("keld-updater-helper.exe"),
            &signer,
            &environment,
            &session,
        )?;
        Ok((host, helper))
    })?;
    Ok(PreparedLocalPair {
        release_public_key: key_bytes,
        authoring,
        host,
        helper,
        source,
        session,
        app_root: app_root.to_owned(),
        signer,
        signer_environment: environment,
        _image_directories: image_directories,
    })
}

/// Signs an application-named launcher with the exact retained developer signer
/// already used for its Host/helper pair. The launcher invokes only KEL-96;
/// it is not a KEL-53 installed application or a signed update manifest.
pub(super) fn sign_named_developer_launcher(
    pair: &PreparedLocalPair,
    launcher: &Path,
) -> Result<[u8; 32], BuildError> {
    if !launcher.starts_with(pair.session.join("developer-runnable-bundle")) {
        return Err(refuse(
            "developer launcher",
            "launcher escaped the owned output session",
        ));
    }
    let input = File::open("NUL").map_err(|error| refuse("developer launcher stdin", error))?;
    local_source::run_captured(
        &pair.signer.tool,
        &sign_args(
            &pair.signer.certificate_sha1,
            &pair.authoring.app.id,
            launcher,
        ),
        &pair.signer_environment,
        &pair.session,
        &input,
        64 * 1024,
    )?;
    let launcher_pin = local_source::pin_file(launcher, MAX_IMAGE_BYTES)?;
    let verified = WindowsAuthenticodeImage::open(launcher)
        .and_then(WindowsAuthenticodeImage::verify)
        .map_err(|error| refuse("developer signed launcher", error))?;
    if verified.identity().app_id() != pair.authoring.app.id
        || verified.identity().publisher_scope() != pair.host.verified.identity().publisher_scope()
    {
        return Err(refuse(
            "developer signed launcher",
            "the signed app launcher and verified Host have different application/publisher identities",
        ));
    }
    let signed_bytes = read_exact_image(verified.file(), MAX_IMAGE_BYTES)?;
    if signed_bytes.len() as u64
        != launcher_pin
            .file
            .metadata()
            .map_err(|e| refuse("launcher metadata", e))?
            .len()
    {
        return Err(refuse(
            "developer signed launcher",
            "retained length differs after signing",
        ));
    }
    let mut args: Vec<OsString> = ["verify", "/pa", "/v"].map(OsString::from).into();
    args.push(launcher.as_os_str().to_owned());
    local_source::run_captured(
        &pair.signer.tool,
        &args,
        &pair.signer_environment,
        &pair.session,
        &input,
        64 * 1024,
    )?;
    Ok(Sha256::digest(&signed_bytes).into())
}

pub(super) fn run(args: &[String]) -> Result<(), BuildError> {
    let pair = prepare(args)?;
    let app = &pair.authoring.app;
    let host = &pair.host;
    let helper = &pair.helper;
    let source = &pair.source;
    let hex = local_source::observed_hex;
    println!(
        "Prepared LOCAL TEST untimestamped signed Host/helper image pair for {:?}.",
        app.id
    );
    println!(
        "Observed source commit {}; source SHA256 {}.",
        source.source_commit,
        hex(&source.source_sha256)
    );
    println!(
        "Host SHA256 {}; helper SHA256 {}.",
        hex(&Sha256::digest(&host.bytes)),
        hex(&Sha256::digest(&helper.bytes))
    );
    println!(
        "Verified publisher scope {}.",
        hex(host.verified.identity().publisher_scope())
    );
    match source.cache_sha256 {
        Some(digest) => println!("Offline registry snapshot {}.", hex(&digest)),
        None => println!(
            "Mutable developer Cargo cache reused in place; dependency contents were not copied, measured or pinned."
        ),
    }
    println!(
        "Trust is only this machine's current Authenticode trust. No public CA/SmartScreen, official release provenance, RFC3161 timestamp, complete application package, installation or Strict runtime claim."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_cache_mode_requires_explicit_opt_in_and_preserves_signer_inputs() {
        let mut args: Vec<String> = (0..15).map(|index| format!("input-{index}")).collect();
        let original = args.clone();
        assert_eq!(image_source_args(&args).expect("snapshot"), original[..7]);
        args.push("--use-developer-cache".into());
        let forwarded = image_source_args(&args).expect("explicit developer cache");
        assert_eq!(&forwarded[..7], &original[..7]);
        assert_eq!(forwarded[7], "--use-developer-cache");
        assert_eq!(&args[7..15], &original[7..15], "signing inputs unchanged");
        args[15] = "--unknown".into();
        assert!(image_source_args(&args).is_err());
        args.push("--use-developer-cache".into());
        assert!(image_source_args(&args).is_err());
        assert!(image_source_args(&original[..14]).is_err());
    }

    #[test]
    fn signer_arguments_have_exact_store_identity_and_no_implicit_selection() {
        let args = sign_args(
            &"A".repeat(40),
            "dev.keld.test",
            Path::new(r"C:\owned\host.exe"),
        );
        let strings: Vec<_> = args
            .iter()
            .map(|arg| arg.to_str().expect("fixture UTF8"))
            .collect();
        assert_eq!(
            strings,
            [
                "sign",
                "/s",
                "My",
                "/sha1",
                &"A".repeat(40),
                "/fd",
                "SHA256",
                "/d",
                "keld.app-id/v1:dev.keld.test",
                r"C:\owned\host.exe"
            ]
        );
        for forbidden in ["/a", "/as", "/sm", "/f", "/p", "/tr", "/t"] {
            assert!(!strings.contains(&forbidden));
        }
    }

    #[test]
    fn selector_identity_requires_exact_hex_width_without_shell_syntax() {
        assert_eq!(hex::<2>("aB01").expect("hex"), [0xab, 1]);
        for bad in ["", "0", "000", "00000", "gg00", "aa;0"] {
            assert!(hex::<2>(bad).is_err());
        }
    }
    #[test]
    fn oversized_helper_refuses_before_any_signer_invocation() {
        let mut invoked = false;
        let oversized = usize::try_from(MAX_IMAGE_BYTES + 1).expect("Windows size bound");
        let result = with_admitted_lengths(1, oversized, || {
            invoked = true;
            Ok(())
        });
        assert!(result.is_err());
        assert!(!invoked);
    }
}
