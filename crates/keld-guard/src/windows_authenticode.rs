//! The single KEL-135 Windows Authenticode verifier (KEL-270 owner decision D4).
//!
//! [`WindowsAuthenticodeImage::open`] pins an executable image against writes, renames
//! and deletes and refuses a leaf reparse point; [`WindowsAuthenticodeImage::verify`]
//! runs `WinVerifyTrust` on that handle with chain revocation (root excluded) and no
//! UI, admits exactly one primary signature and no secondary signature, derives the
//! publisher scope from the verified leaf certificate's SPKI, and reads the app id only
//! from that signer's authenticated `SPC_SP_OPUS_INFO` program name with the exact
//! `keld.app-id/v1:` prefix. Every `WinTrust` and Crypt32 call stays in this module, and
//! callers receive only owned results; the caller owns the user-facing diagnostic code.

#![deny(unsafe_op_in_unsafe_fn)]

use std::fs::{self, File};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::AsRawHandle as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CryptDecodeObjectEx, CryptEncodeObjectEx, PKCS_7_ASN_ENCODING,
    X509_ASN_ENCODING, X509_PUBLIC_KEY_INFO,
};
use windows_sys::Win32::Security::WinTrust::{
    CRYPT_PROVIDER_SGNR, SPC_SP_OPUS_INFO, SPC_SP_OPUS_INFO_OBJID, SPC_SP_OPUS_INFO_STRUCT,
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WINTRUST_SIGNATURE_SETTINGS, WSS_GET_SECONDARY_SIG_COUNT, WTD_CACHE_ONLY_URL_RETRIEVAL,
    WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOCATION_CHECK_NONE,
    WTD_REVOKE_NONE, WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
    WTD_UI_NONE, WTD_UICONTEXT_EXECUTE, WTHelperGetProvCertFromChain,
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
};

mod refusals;
// Pure D-45 issuer/profile/role predicates only. The owning native producer
// remains closed until T-82 grants the exact certificate/PE observations.
mod d45_role_policy;
pub use refusals::WindowsAuthenticodeError;
use refusals::{parse_windows_signed_program_name, windows_identity_error};

const WINDOWS_SIGNED_APP_ID_PREFIX: &str = "keld.app-id/v1:";
const WINDOWS_SIGNED_PROGRAM_NAME_MAX_UNITS: usize = WINDOWS_SIGNED_APP_ID_PREFIX.len() + 255;
const WINDOWS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b"keld.publisher.windows/v1\0";
const WINDOWS_SIGNER_SPKI_MAX_BYTES: u32 = 64 * 1024;
const WINDOWS_OPUS_INFO_MAX_BYTES: u32 = 4 * 1024;

/// Hash one retained file in fixed chunks. Host size is never an allocation size.
const WINDOWS_HOST_DIGEST_CHUNK_BYTES: usize = 64 * 1024;

/// Two distinct trust policies over the same retained, read-only image handle.
/// Cache-only observation never substitutes for full revocation verification.
#[derive(Clone, Copy)]
enum WindowsTrustPolicy {
    Full,
    OfflinePreflight,
}

/// An executable image held open for KEL-135 Authenticode verification.
///
/// The handle shares only reads, so the file cannot be written, renamed or deleted
/// while this value lives. The absolute path only locates the image for `WinTrust`.
#[derive(Debug)]
pub struct WindowsAuthenticodeImage {
    path: PathBuf,
    image: File,
}

impl WindowsAuthenticodeImage {
    /// Makes `executable` absolute and opens it as a pinned trust image.
    ///
    /// # Errors
    /// Refuses a path that cannot be made absolute or opened, a leaf reparse point, and
    /// anything other than a regular file.
    pub fn open(executable: &Path) -> Result<Self, WindowsAuthenticodeError> {
        let path = std::path::absolute(executable).map_err(|source| {
            windows_identity_error(
                format!("the current executable path cannot be made absolute: {source}"),
                "Restore the signed package executable and relaunch it.",
            )
        })?;
        let image = open_windows_trust_image(&path)?;
        Ok(Self { path, image })
    }

    /// Verifies the pinned image once and binds its authenticated publisher and app id
    /// to the same open handle.
    ///
    /// Any `WinTrust` state it creates is closed, or its close failure reported, before
    /// it returns. A refused image's handle is closed with it.
    ///
    /// # Errors
    /// Refuses a `WinVerifyTrust` failure, any signature set other than one primary
    /// signature, a signer, certificate, SPKI or program-name fact that is missing,
    /// malformed or over its bound, a program name without the exact `keld.app-id/v1:`
    /// prefix and a 1-255 byte app id, and a failure to close the `WinTrust` state.
    pub fn verify(self) -> Result<VerifiedWindowsImage, WindowsAuthenticodeError> {
        let identity = verified_windows_identity_from_image(&self.path, &self.image)?;
        Ok(VerifiedWindowsImage {
            image: self.image,
            identity,
        })
    }

    /// Observes a signed image using only cached local trust data, before any
    /// discovery/network decision. Retains the original file handle and returns
    /// a distinct, non-authority type; it does not check revocation status.
    ///
    /// # Errors
    /// Refuses an absent or untrusted local chain, ambiguous signatures, missing
    /// or invalid signed program-name facts, and any `WinTrust` CLOSE failure.
    pub fn observe_preflight(self) -> Result<WindowsPreflightImage, WindowsAuthenticodeError> {
        let observed = windows_identity_from_image_with_policy(
            &self.path,
            &self.image,
            WindowsTrustPolicy::OfflinePreflight,
        )?;
        Ok(WindowsPreflightImage {
            source: self,
            observed_publisher_scope: observed.publisher_scope,
            observed_app_id: observed.app_id,
        })
    }
}

/// A D-45 cache-only signed-image observation retaining the exact source file.
///
/// The observed values are for local read-only Direct/Managed discovery only.
/// Neither the type nor its fields can mint `VerifiedWindowsImage`, bypass
/// revocation, or grant installation authority. A later approved Direct-decision
/// adapter must independently perform full verification on this retained file.
#[derive(Debug)]
pub struct WindowsPreflightImage {
    source: WindowsAuthenticodeImage,
    observed_publisher_scope: [u8; 32],
    observed_app_id: Box<str>,
}

/// Native D-43 Direct-discovery eligibility required before D-45 full verification.
///
/// KEL-53/T-82 has not qualified an issuer for authenticated publisher,
/// profile OID, issuer policy, repository census, or old-reader evidence.
/// This type has no production constructor and does not accept a caller
/// Boolean, pathname, or digest as an authority grant.
#[allow(dead_code)] // T-82 has not commissioned a native Direct-discovery issuer.
struct WindowsD45DirectDiscoveryEligibility {
    original_host_sha256: [u8; 32],
    // Exact signed-catalogue host.bytes, never from the filesystem or request.
    expected_host_bytes: u64,
}

impl WindowsPreflightImage {
    /// Locally observed publisher scope. No current revocation status is implied.
    #[must_use]
    pub const fn observed_publisher_scope(&self) -> &[u8; 32] {
        &self.observed_publisher_scope
    }

    /// Locally observed legacy `keld.app-id/v1:` signed program-name app ID.
    /// The v2 D-WIRE policy/role check is a separate pending Guard boundary.
    #[must_use]
    pub fn observed_app_id(&self) -> &str {
        &self.observed_app_id
    }

    /// The retained file supplying read-only discovery bytes. No path reopen.
    /// Only the future Direct-decision owner may promote this observation.
    #[must_use]
    pub const fn file(&self) -> &File {
        &self.source.image
    }

    /// Revalidate the exact same retained image using full chain revocation.
    ///
    /// Kept private and requires a non-forgeable, current-lease Direct-discovery
    /// witness. No production witness constructor exists before T-82 grants
    /// the native issuer/profile/role checks and the KEL-53 repository census.
    /// Neither a preflight value nor a caller Boolean can authorize conversion.
    #[allow(dead_code)] // T-82 pending: no production Direct-discovery witness exists yet.
    fn verify_full_bound(
        self,
        direct: &WindowsD45DirectDiscoveryEligibility,
    ) -> Result<VerifiedWindowsImage, WindowsAuthenticodeError> {
        // The Direct witness must bind an exact authenticated byte count and
        // digest before any full-file read; preflight performs no such scan.
        let exact_digest =
            sha256_retained_windows_image(&self.source.image, direct.expected_host_bytes)?;
        if exact_digest != direct.original_host_sha256 {
            return Err(windows_identity_error(
                "the Direct discovery source differs from the retained offline Host",
                "Repeat authenticated owner discovery against the original signed Host.",
            ));
        }
        let full = self.source.verify()?;
        if full.identity.publisher_scope != self.observed_publisher_scope
            || full.identity.app_id != self.observed_app_id
        {
            return Err(windows_identity_error(
                "the authenticated signer or app ID differs from the retained offline preflight",
                "Restore the original signed Host and repeat discovery and full verification.",
            ));
        }
        Ok(full)
    }
}

/// Computes a complete byte digest through one retained file object. No path is
/// opened or trusted here, and this digest alone cannot authorize a signed Host.
fn sha256_retained_windows_image(
    image: &File,
    expected_bytes: u64,
) -> Result<[u8; 32], WindowsAuthenticodeError> {
    use std::os::windows::fs::FileExt as _;

    let length = image
        .metadata()
        .map_err(|source| {
            windows_identity_error(
                format!("the retained signed Host length cannot be read: {source}"),
                "Restore the signed Host file and restart read-only verification.",
            )
        })?
        .len();
    if expected_bytes == 0 || length != expected_bytes {
        return Err(windows_identity_error(
            "the retained signed Host byte count differs from the authenticated expected length",
            "Restore the exact signed Host and repeat authenticated source selection.",
        ));
    }
    let mut hasher = Sha256::new();
    // Keep the 64-KiB bounded source-read buffer on the heap. A signed Host
    // cannot force a large stack allocation in this verifier.
    let mut bytes = vec![0_u8; WINDOWS_HOST_DIGEST_CHUNK_BYTES];
    let mut offset = 0_u64;
    while offset < expected_bytes {
        let remaining = expected_bytes - offset;
        let take = usize::try_from(remaining)
            .unwrap_or(WINDOWS_HOST_DIGEST_CHUNK_BYTES)
            .min(WINDOWS_HOST_DIGEST_CHUNK_BYTES);
        let count = image
            .seek_read(&mut bytes[..take], offset)
            .map_err(|source| {
                windows_identity_error(
                    format!("the retained signed Host read failed: {source}"),
                    "Restore the signed Host file and restart read-only verification.",
                )
            })?;
        if count == 0 {
            return Err(windows_identity_error(
                "the retained signed Host ended before its handle-derived length",
                "Restore the signed Host file and restart read-only verification.",
            ));
        }
        hasher.update(&bytes[..count]);
        offset += count as u64;
    }
    let digest = hasher.finalize();
    let mut exact = [0_u8; 32];
    exact.copy_from_slice(&digest);
    Ok(exact)
}

/// A pinned executable image and the identity that verifying that handle proved.
///
/// Only [`WindowsAuthenticodeImage::verify`] constructs it, so the identity always
/// belongs to [`file`](Self::file).
#[derive(Debug)]
pub struct VerifiedWindowsImage {
    image: File,
    identity: WindowsAuthenticodeIdentity,
}

impl VerifiedWindowsImage {
    /// The publisher scope and app id that verification of [`file`](Self::file) proved.
    #[must_use]
    pub const fn identity(&self) -> &WindowsAuthenticodeIdentity {
        &self.identity
    }

    /// The pinned handle that `WinVerifyTrust` verified, opened for reading while sharing
    /// only reads, so the image cannot be written, renamed or deleted while this value
    /// lives.
    ///
    /// Callers MUST read the verified image through this handle and never reopen it by
    /// path. The cursor is shared, so no caller may rely on its position.
    #[must_use]
    pub const fn file(&self) -> &File {
        &self.image
    }
}

/// The publisher scope and app id that one successful verification proved.
///
/// Only [`WindowsAuthenticodeImage::verify`] constructs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsAuthenticodeIdentity {
    publisher_scope: [u8; 32],
    app_id: Box<str>,
}

impl WindowsAuthenticodeIdentity {
    /// SHA-256 of `keld.publisher.windows/v1\0` followed by the DER SPKI of the
    /// verified leaf signing certificate.
    #[must_use]
    pub const fn publisher_scope(&self) -> &[u8; 32] {
        &self.publisher_scope
    }

    /// The 1-255 byte app id after the exact `keld.app-id/v1:` prefix of the signer's
    /// authenticated program name. Its canonical app-id grammar is not checked here;
    /// the consumer that derives a profile or compares a configured id applies it.
    #[must_use]
    pub fn app_id(&self) -> &str {
        &self.app_id
    }
}

/// Opens the KEL-135 trust image for reading while sharing only reads, so the file
/// cannot be written, renamed or deleted while it is verified; a leaf reparse point
/// is refused rather than followed. The path only locates the image.
fn open_windows_trust_image(executable: &Path) -> Result<File, WindowsAuthenticodeError> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let image = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(executable)
        .map_err(|source| {
            windows_identity_error(
                format!("the current executable cannot be opened for trust verification: {source}"),
                "Restore the signed package executable and relaunch it.",
            )
        })?;
    let metadata = image.metadata().map_err(|source| {
        windows_identity_error(
            format!("the current executable metadata is unavailable: {source}"),
            "Restore the signed package executable and relaunch it.",
        )
    })?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(windows_identity_error(
            "the current executable is a reparse point or not a regular file",
            "Install the signed package executable as a regular file and launch it directly.",
        ));
    }
    Ok(image)
}

fn verified_windows_identity_from_image(
    executable: &Path,
    image: &File,
) -> Result<WindowsAuthenticodeIdentity, WindowsAuthenticodeError> {
    windows_identity_from_image_with_policy(executable, image, WindowsTrustPolicy::Full)
}

#[allow(unsafe_code, clippy::too_many_lines)] // one shared VERIFY/extract/CLOSE state owner for full and offline policies
fn windows_identity_from_image_with_policy(
    executable: &Path,
    image: &File,
    policy: WindowsTrustPolicy,
) -> Result<WindowsAuthenticodeIdentity, WindowsAuthenticodeError> {
    let offline = matches!(policy, WindowsTrustPolicy::OfflinePreflight);
    let (revocation_checks, provider_flags) = match policy {
        WindowsTrustPolicy::Full => (
            WTD_REVOKE_WHOLECHAIN,
            WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
        ),
        WindowsTrustPolicy::OfflinePreflight => (
            WTD_REVOKE_NONE,
            WTD_REVOCATION_CHECK_NONE | WTD_CACHE_ONLY_URL_RETRIEVAL,
        ),
    };
    let mut executable_wide: Vec<u16> = executable.as_os_str().encode_wide().collect();
    if executable_wide.contains(&0) {
        return Err(windows_identity_error(
            "the current executable path contains an embedded NUL",
            "Install the signed package at a normal Windows filesystem path.",
        ));
    }
    executable_wide.push(0);

    let file_info_size =
        u32::try_from(std::mem::size_of::<WINTRUST_FILE_INFO>()).map_err(|_| {
            windows_identity_error(
                "the WinTrust file structure size does not fit the platform ABI",
                "Use a supported 64-bit Windows Keld build.",
            )
        })?;
    let trust_data_size = u32::try_from(std::mem::size_of::<WINTRUST_DATA>()).map_err(|_| {
        windows_identity_error(
            "the WinTrust data structure size does not fit the platform ABI",
            "Use a supported 64-bit Windows Keld build.",
        )
    })?;
    let signature_settings_size = u32::try_from(std::mem::size_of::<WINTRUST_SIGNATURE_SETTINGS>())
        .map_err(|_| {
            windows_identity_error(
                "the WinTrust signature-settings size does not fit the platform ABI",
                "Use a supported 64-bit Windows Keld build.",
            )
        })?;
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: file_info_size,
        pcwszFilePath: executable_wide.as_ptr(),
        hFile: image.as_raw_handle().cast(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut signature_settings = WINTRUST_SIGNATURE_SETTINGS {
        cbStruct: signature_settings_size,
        dwIndex: 0,
        dwFlags: WSS_GET_SECONDARY_SIG_COUNT,
        cSecondarySigs: 0,
        dwVerifiedSigIndex: 0,
        pCryptoPolicy: std::ptr::null_mut(),
    };
    let mut trust_data = WINTRUST_DATA {
        cbStruct: trust_data_size,
        pPolicyCallbackData: std::ptr::null_mut(),
        pSIPClientData: std::ptr::null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: revocation_checks,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &raw mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: std::ptr::null_mut(),
        pwszURLReference: std::ptr::null_mut(),
        dwProvFlags: provider_flags,
        dwUIContext: WTD_UICONTEXT_EXECUTE,
        pSignatureSettings: &raw mut signature_settings,
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `file_info`, its NUL-terminated path, the borrowed open trust-image
    // handle, and `signature_settings` remain live and unmoved with `trust_data`
    // until the matching CLOSE call. Other optional pointers are null and the selected
    // union arm is FILE.
    let trust_status = unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &raw mut action,
            (&raw mut trust_data).cast(),
        )
    };
    if trust_status != 0 {
        let failure_detail = if offline {
            format!(
                "WinVerifyTrust rejected the current executable with status 0x{:08x} during the cache-only preflight",
                u32::from_ne_bytes(trust_status.to_ne_bytes())
            )
        } else {
            format!(
                "WinVerifyTrust rejected the current executable with status 0x{:08x}",
                u32::from_ne_bytes(trust_status.to_ne_bytes())
            )
        };
        let fix = if offline {
            "Install a package signed by a locally trusted Authenticode publisher; do not bypass signature verification."
        } else {
            "Install a package signed by a trusted Authenticode publisher with a valid chain and revocation status."
        };
        let primary = windows_identity_error(failure_detail, fix);
        if !trust_data.hWVTStateData.is_null() {
            trust_data.dwStateAction = WTD_STATEACTION_CLOSE;
            // SAFETY: this closes only state created by the VERIFY call above
            // while the same action/data storage is still live.
            let close_status = unsafe {
                WinVerifyTrust(
                    std::ptr::null_mut(),
                    &raw mut action,
                    (&raw mut trust_data).cast(),
                )
            };
            if close_status != 0 {
                let cleanup = windows_trust_close_error(close_status);
                return Err(primary.with_close_failure(cleanup));
            }
        }
        return Err(primary);
    }

    let identity_result = validate_windows_signature_cardinality(
        signature_settings.cSecondarySigs,
        signature_settings.dwVerifiedSigIndex,
    )
    .and_then(|()| windows_identity_from_verified_trust_state(&trust_data));
    trust_data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: this is the mandatory CLOSE for the successful VERIFY call;
    // action/data storage and every referenced input remain live here.
    let close_status = unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &raw mut action,
            (&raw mut trust_data).cast(),
        )
    };
    let close_result = if close_status == 0 {
        Ok(())
    } else {
        Err(windows_trust_close_error(close_status))
    };
    match (identity_result, close_result) {
        (Ok(identity), Ok(())) => Ok(identity),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(primary), Ok(())) => Err(primary),
        (Err(primary), Err(cleanup)) => Err(primary.with_close_failure(cleanup)),
    }
}

fn validate_windows_signature_cardinality(
    secondary_signatures: u32,
    verified_signature_index: u32,
) -> Result<(), WindowsAuthenticodeError> {
    if secondary_signatures == 0 && verified_signature_index == 0 {
        return Ok(());
    }
    Err(windows_identity_error(
        format!(
            "the verified package has one primary and {secondary_signatures} secondary Authenticode signatures, with verified index {verified_signature_index}; exactly one signature is required"
        ),
        "Sign the package once without appending a secondary Authenticode signature.",
    ))
}

fn windows_trust_close_error(status: i32) -> WindowsAuthenticodeError {
    windows_identity_error(
        format!(
            "WinVerifyTrust could not close verified state (status 0x{:08x})",
            u32::from_ne_bytes(status.to_ne_bytes())
        ),
        "Restart Windows and retry the signed package; if this persists, repair the package trust installation.",
    )
}

#[allow(unsafe_code)] // read-only walk of provider memory retained by the live WinTrust state
fn windows_identity_from_verified_trust_state(
    trust_data: &WINTRUST_DATA,
) -> Result<WindowsAuthenticodeIdentity, WindowsAuthenticodeError> {
    // SAFETY: the caller invokes this only after a successful VERIFY and
    // before CLOSE, so the state owns the returned provider graph.
    let provider = unsafe { WTHelperProvDataFromStateData(trust_data.hWVTStateData) };
    if provider.is_null() {
        return Err(windows_identity_error(
            "WinTrust returned no verified provider state",
            "Re-sign the package with one supported primary Authenticode signature.",
        ));
    }
    // SAFETY: `provider` is non-null and owned by the live WinTrust state.
    let provider_ref = unsafe { &*provider };
    if provider_ref.csSigners != 1 {
        return Err(windows_identity_error(
            format!(
                "the verified package has {} primary signers; exactly one is required",
                provider_ref.csSigners
            ),
            "Sign the package with exactly one primary Authenticode signer.",
        ));
    }
    // SAFETY: signer index zero is in range because `csSigners == 1`; the
    // provider graph remains live until the caller performs CLOSE.
    let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, 0, 0) };
    if signer.is_null() {
        return Err(windows_identity_error(
            "WinTrust returned no primary signer",
            "Re-sign the package with one supported primary Authenticode signature.",
        ));
    }
    let publisher_scope = windows_publisher_scope_from_signer(signer)?;
    let signed_program_name = windows_signed_program_name_from_signer(signer)?;
    let app_id = parse_windows_signed_program_name(&signed_program_name)?;
    Ok(WindowsAuthenticodeIdentity {
        publisher_scope,
        app_id: app_id.into(),
    })
}

#[allow(unsafe_code)] // bounded read and DER encoding of the live WinTrust leaf certificate
fn windows_publisher_scope_from_signer(
    signer: *mut CRYPT_PROVIDER_SGNR,
) -> Result<[u8; 32], WindowsAuthenticodeError> {
    // SAFETY: the caller supplies the non-null primary signer from the live
    // provider graph and keeps that WinTrust state open for this call.
    let signer_ref = unsafe { &*signer };
    if signer_ref.dwError != 0 || signer_ref.csCertChain == 0 {
        return Err(windows_identity_error(
            "the verified primary signer has no error-free certificate chain",
            "Repair the Authenticode certificate chain and re-sign the package.",
        ));
    }
    // SAFETY: certificate index zero is in range because the signer chain is
    // nonempty; the provider graph remains live for this call.
    let certificate = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if certificate.is_null() {
        return Err(windows_identity_error(
            "WinTrust returned no leaf signing certificate",
            "Repair the Authenticode certificate chain and re-sign the package.",
        ));
    }
    // SAFETY: `certificate` is non-null and owned by the live provider graph.
    let certificate_ref = unsafe { &*certificate };
    if certificate_ref.dwError != 0 || certificate_ref.pCert.is_null() {
        return Err(windows_identity_error(
            "the Authenticode leaf signing certificate is invalid",
            "Repair the Authenticode certificate chain and re-sign the package.",
        ));
    }
    // SAFETY: the non-null certificate context belongs to the live provider.
    let certificate_context = unsafe { &*certificate_ref.pCert };
    if certificate_context.pCertInfo.is_null() {
        return Err(windows_identity_error(
            "the Authenticode leaf certificate has no certificate information",
            "Repair the Authenticode certificate and re-sign the package.",
        ));
    }
    // SAFETY: `pCertInfo` is non-null and remains owned by the provider.
    let certificate_info = unsafe { &*certificate_context.pCertInfo };
    let mut spki_size = 0_u32;
    // SAFETY: the public-key-info value belongs to the live certificate; this
    // call supplies no output buffer and asks Crypt32 for the size.
    let sized = unsafe {
        CryptEncodeObjectEx(
            X509_ASN_ENCODING,
            X509_PUBLIC_KEY_INFO,
            std::ptr::from_ref(&certificate_info.SubjectPublicKeyInfo).cast(),
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
            &raw mut spki_size,
        )
    };
    if sized == 0 || spki_size == 0 || spki_size > WINDOWS_SIGNER_SPKI_MAX_BYTES {
        return Err(windows_identity_error(
            "the Authenticode leaf public key cannot be encoded within the bounded SPKI limit",
            "Use a supported Authenticode signing certificate and re-sign the package.",
        ));
    }
    let mut spki_der = vec![0_u8; spki_size as usize];
    let mut encoded_size = spki_size;
    // SAFETY: `spki_der` is writable for `spki_size` bytes, and the input
    // public-key-info remains live in the provider graph.
    let encoded = unsafe {
        CryptEncodeObjectEx(
            X509_ASN_ENCODING,
            X509_PUBLIC_KEY_INFO,
            std::ptr::from_ref(&certificate_info.SubjectPublicKeyInfo).cast(),
            0,
            std::ptr::null(),
            spki_der.as_mut_ptr().cast(),
            &raw mut encoded_size,
        )
    };
    if encoded == 0 || encoded_size == 0 || encoded_size > spki_size {
        return Err(windows_identity_error(
            "the Authenticode leaf public key DER encoding failed",
            "Use a supported Authenticode signing certificate and re-sign the package.",
        ));
    }
    spki_der.truncate(encoded_size as usize);
    let mut publisher_hasher = Sha256::new();
    publisher_hasher.update(WINDOWS_PUBLISHER_SCOPE_DOMAIN);
    publisher_hasher.update(&spki_der);
    let publisher_digest = publisher_hasher.finalize();
    let mut publisher_scope = [0_u8; 32];
    publisher_scope.copy_from_slice(&publisher_digest);
    Ok(publisher_scope)
}

#[allow(unsafe_code)] // bounded decode of the authenticated signer-description attribute
fn windows_signed_program_name_from_signer(
    signer: *mut CRYPT_PROVIDER_SGNR,
) -> Result<String, WindowsAuthenticodeError> {
    // SAFETY: the caller supplies the non-null primary signer from the live
    // WinTrust provider graph and retains that state for this call.
    let signer_ref = unsafe { &*signer };
    if signer_ref.psSigner.is_null() {
        return Err(windows_identity_error(
            "the verified primary signer has no signed attribute record",
            "Sign the package with `/d \"keld.app-id/v1:<canonical-app.id>\"`.",
        ));
    }
    // SAFETY: `psSigner` is non-null and retained by the live provider.
    let signer_info = unsafe { &*signer_ref.psSigner };
    let attributes = &signer_info.AuthAttrs;
    if attributes.cAttr == 0 || attributes.cAttr > 64 || attributes.rgAttr.is_null() {
        return Err(windows_identity_error(
            "the verified primary signer has no bounded authenticated attribute set",
            "Sign the package with `/d \"keld.app-id/v1:<canonical-app.id>\"`.",
        ));
    }
    // SAFETY: WinTrust supplied `cAttr` entries at `rgAttr`; the explicit
    // upper bound prevents an attacker-controlled unbounded slice.
    let attribute_slice =
        unsafe { std::slice::from_raw_parts(attributes.rgAttr, attributes.cAttr as usize) };
    let mut opus_blob = None;
    for attribute in attribute_slice {
        if attribute.pszObjId.is_null() {
            continue;
        }
        // SAFETY: WinTrust/Crypt32 define attribute OIDs as NUL-terminated
        // strings owned by the provider graph. The state is still live.
        let oid = unsafe { std::ffi::CStr::from_ptr(attribute.pszObjId.cast()) };
        // SAFETY: this SDK constant is a static NUL-terminated OID string.
        let expected_oid = unsafe { std::ffi::CStr::from_ptr(SPC_SP_OPUS_INFO_OBJID.cast()) };
        if oid == expected_oid {
            if opus_blob.is_some() || attribute.cValue != 1 || attribute.rgValue.is_null() {
                return Err(windows_identity_error(
                    "the verified signature has duplicate or noncanonical app-id attributes",
                    "Sign exactly one Authenticode description with `/d \"keld.app-id/v1:<canonical-app.id>\"`.",
                ));
            }
            // SAFETY: `cValue == 1` and `rgValue` is non-null.
            opus_blob = Some(unsafe { *attribute.rgValue });
        }
    }
    let opus_blob = opus_blob.ok_or_else(|| {
        windows_identity_error(
            "the verified signature has no authenticated Keld app-id attribute",
            "Sign the package with `/d \"keld.app-id/v1:<canonical-app.id>\"`.",
        )
    })?;
    if opus_blob.cbData == 0 || opus_blob.pbData.is_null() {
        return Err(windows_identity_error(
            "the authenticated Keld app-id attribute is empty",
            "Sign the package with `/d \"keld.app-id/v1:<canonical-app.id>\"`.",
        ));
    }
    decode_windows_opus_program_name(opus_blob)
}

#[allow(unsafe_code)] // bounded Crypt32 decode; source blob remains owned by the live signer
fn decode_windows_opus_program_name(
    opus_blob: CRYPT_INTEGER_BLOB,
) -> Result<String, WindowsAuthenticodeError> {
    let mut decoded_size = 0_u32;
    // SAFETY: the encoded attribute blob belongs to the live signer; this
    // first call asks Crypt32 for the decoded structure size only.
    let decoded_size_ok = unsafe {
        CryptDecodeObjectEx(
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            SPC_SP_OPUS_INFO_STRUCT,
            opus_blob.pbData,
            opus_blob.cbData,
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
            &raw mut decoded_size,
        )
    };
    let minimum_opus_size =
        u32::try_from(std::mem::size_of::<SPC_SP_OPUS_INFO>()).map_err(|_| {
            windows_identity_error(
                "the Authenticode description structure size does not fit the platform ABI",
                "Use a supported 64-bit Windows Keld build.",
            )
        })?;
    if decoded_size_ok == 0
        || decoded_size < minimum_opus_size
        || decoded_size > WINDOWS_OPUS_INFO_MAX_BYTES
    {
        return Err(windows_identity_error(
            "the authenticated Keld app-id attribute is malformed or exceeds its bound",
            "Re-sign the package with one short canonical Keld app-id description.",
        ));
    }
    let word_size = std::mem::size_of::<usize>();
    let decoded_words = (decoded_size as usize).div_ceil(word_size);
    let mut decoded = vec![0_usize; decoded_words];
    let mut actual_decoded_size = decoded_size;
    // SAFETY: the usize buffer is suitably aligned for SPC_SP_OPUS_INFO and
    // writable for at least `decoded_size` bytes; the source blob is live.
    let decoded_ok = unsafe {
        CryptDecodeObjectEx(
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            SPC_SP_OPUS_INFO_STRUCT,
            opus_blob.pbData,
            opus_blob.cbData,
            0,
            std::ptr::null(),
            decoded.as_mut_ptr().cast(),
            &raw mut actual_decoded_size,
        )
    };
    if decoded_ok == 0
        || actual_decoded_size < minimum_opus_size
        || actual_decoded_size > decoded_size
    {
        return Err(windows_identity_error(
            "the authenticated Keld app-id attribute cannot be decoded",
            "Re-sign the package with one canonical Keld app-id description.",
        ));
    }
    // SAFETY: the aligned buffer contains at least one decoded
    // SPC_SP_OPUS_INFO according to the successful Crypt32 call.
    let opus_info = unsafe { &*decoded.as_ptr().cast::<SPC_SP_OPUS_INFO>() };
    if opus_info.pwszProgramName.is_null() {
        return Err(windows_identity_error(
            "the authenticated Authenticode description has no program name",
            "Sign the package with `/d \"keld.app-id/v1:<canonical-app.id>\"`.",
        ));
    }
    let mut program_name_utf16 = Vec::with_capacity(WINDOWS_SIGNED_PROGRAM_NAME_MAX_UNITS);
    for index in 0..=WINDOWS_SIGNED_PROGRAM_NAME_MAX_UNITS {
        // SAFETY: Crypt32 returned a NUL-terminated program-name pointer
        // inside the retained decoded buffer. Reads stop at the approved
        // maximum plus its required terminator.
        let unit = unsafe { *opus_info.pwszProgramName.add(index) };
        if unit == 0 {
            return String::from_utf16(&program_name_utf16).map_err(|_| {
                windows_identity_error(
                    "the authenticated Authenticode description is not valid UTF-16",
                    "Re-sign the package with an ASCII canonical Keld app-id description.",
                )
            });
        }
        program_name_utf16.push(unit);
    }
    Err(windows_identity_error(
        "the authenticated Authenticode description has no terminator within the app-id bound",
        "Re-sign the package with a 1-255 byte canonical Keld app-id description.",
    ))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    mod d45_host_handle_alias;

    /// One isolated trust-image fixture directory under the system temporary directory.
    struct FixtureDirectory(PathBuf);

    impl FixtureDirectory {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "keld-guard-authenticode-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create an isolated trust-image fixture directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for FixtureDirectory {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.0)
                && error.kind() != std::io::ErrorKind::NotFound
                && !std::thread::panicking()
            {
                panic!("remove trust-image fixture {}: {error}", self.0.display());
            }
        }
    }

    #[test]
    fn windows_signed_program_name_carrier_is_exact_and_bounded() {
        assert_eq!(
            parse_windows_signed_program_name("keld.app-id/v1:com.example.app")
                .expect("exact signed carrier"),
            "com.example.app"
        );
        for invalid in [
            "",
            "com.example.app",
            "keld.app-id/v1:",
            "KELD.app-id/v1:com.example.app",
        ] {
            assert!(
                parse_windows_signed_program_name(invalid).is_err(),
                "accepted signed carrier: {invalid:?}"
            );
        }
        let maximum = format!("keld.app-id/v1:{}", "a".repeat(255));
        assert_eq!(
            parse_windows_signed_program_name(&maximum)
                .expect("255-byte signed app id")
                .len(),
            255
        );
        let over = format!("keld.app-id/v1:{}", "a".repeat(256));
        assert!(parse_windows_signed_program_name(&over).is_err());
    }

    #[test]
    fn windows_trust_image_is_pinned_against_write_rename_and_delete() {
        let directory = FixtureDirectory::new("pinned");
        let image_path = directory.path().join("image.exe");
        let moved = directory.path().join("moved.exe");
        fs::write(&image_path, b"trust image fixture").expect("write trust-image fixture");

        let image = WindowsAuthenticodeImage::open(&image_path).expect("a regular image opens");
        assert!(
            fs::OpenOptions::new()
                .write(true)
                .open(&image_path)
                .is_err(),
            "a writer opened the image while it was held for verification"
        );
        assert!(
            fs::rename(&image_path, &moved).is_err(),
            "the image was renamed while it was held for verification"
        );
        assert!(
            fs::remove_file(&image_path).is_err(),
            "the image was deleted while it was held for verification"
        );
        drop(image);
        fs::rename(&image_path, &moved).expect("a released image can move");
    }

    #[test]
    fn d45_retained_host_digest_matches_independent_sha256_literal() {
        let directory = FixtureDirectory::new("d45-retained-digest");
        let path = directory.path().join("host-source.exe");
        fs::write(&path, b"abc").expect("write exact fixed hash vector");
        let image = WindowsAuthenticodeImage::open(&path)
            .expect("unsigned synthetic image may be held for read-only hashing");
        assert_eq!(
            sha256_retained_windows_image(&image.image, 3)
                .expect("read all source bytes through the pinned hFile"),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ],
            "the digest must cover the exact original bytes"
        );
        let error = sha256_retained_windows_image(&image.image, 4)
            .expect_err("a different expected size must refuse before a file scan");
        assert!(
            error
                .detail()
                .contains("differs from the authenticated expected length"),
            "{error}"
        );
    }

    #[test]
    fn d45_foreign_direct_source_refuses_before_full_wintrust() {
        // This deliberately fabricated *test-only* preflight state cannot
        // authenticate anything. It isolates the independent native-source
        // identity check that must precede full WinTrust after D-43 discovery.
        let directory = FixtureDirectory::new("d45-foreign-source-witness");
        let path = directory.path().join("unsigned-host.exe");
        fs::write(&path, b"bounded owner mismatch fixture").expect("write source fixture");
        let source =
            WindowsAuthenticodeImage::open(&path).expect("retain the test's original regular file");
        let preflight = WindowsPreflightImage {
            source,
            observed_publisher_scope: [0x22; 32],
            observed_app_id: "com.example.app".into(),
        };
        let error = preflight
            .verify_full_bound(&WindowsD45DirectDiscoveryEligibility {
                original_host_sha256: [0_u8; 32],
                expected_host_bytes: u64::try_from(b"bounded owner mismatch fixture".len())
                    .expect("fixture source size fits u64"),
            })
            .expect_err("a different Direct source digest cannot reach full WinTrust");
        assert!(
            error
                .detail()
                .contains("Direct discovery source differs from the retained offline Host"),
            "{error}"
        );
        assert!(error.close_failure().is_none(), "{error}");
    }

    #[test]
    fn d45_cache_only_preflight_refuses_unsigned_source() {
        let directory = FixtureDirectory::new("d45-offline-unsigned");
        let host_path = directory.path().join("unsigned-keld-host.exe");
        fs::write(&host_path, b"unsigned synthetic source").expect("write unsigned image");

        let error = WindowsAuthenticodeImage::open(&host_path)
            .expect("a regular unsigned fixture opens for a read-only observation")
            .observe_preflight()
            .expect_err("offline preflight must refuse an unsigned image");
        assert!(
            error.detail().contains("cache-only preflight"),
            "preflight must report its separate offline trust policy: {error}"
        );
        assert!(error.close_failure().is_none(), "{error}");
    }

    #[test]
    fn d45_host_path_copy_substitution_waits_for_image_handle_release() {
        use std::os::windows::fs::FileExt as _;

        let directory = FixtureDirectory::new("d45-host-copy-substitution");
        let host_path = directory.path().join("keld-host.exe");
        let foreign_path = directory.path().join("foreign-host.exe");
        let original = b"original host image: unsigned custody fixture";
        let foreign = b"foreign host image: unsigned custody fixture";
        fs::write(&host_path, original).expect("create the original Host fixture");
        fs::write(&foreign_path, foreign).expect("create the foreign Host fixture");

        let pinned = WindowsAuthenticodeImage::open(&host_path)
            .expect("open the original Host image with Guard's production source owner");
        let error = fs::copy(&foreign_path, &host_path)
            .expect_err("a foreign image cannot replace a pinned Host pathname");
        assert_eq!(
            error.raw_os_error(),
            Some(32),
            "Windows must refuse foreign byte replacement through the pinned source"
        );

        let mut observed = vec![0; original.len()];
        let bytes_read = pinned
            .image
            .seek_read(&mut observed, 0)
            .expect("read from the retained original image handle");
        assert_eq!(bytes_read, original.len());
        assert_eq!(&observed[..], &original[..]);
        assert_eq!(fs::read(&host_path).unwrap(), &original[..]);
        assert_eq!(fs::read(&foreign_path).unwrap(), &foreign[..]);

        drop(pinned);
        assert_eq!(
            fs::copy(&foreign_path, &host_path).expect("replace the Host path after release"),
            u64::try_from(foreign.len()).unwrap()
        );
        assert_eq!(fs::read(&host_path).unwrap(), &foreign[..]);
    }

    #[test]
    fn windows_trust_image_refuses_a_leaf_reparse_point() {
        let directory = FixtureDirectory::new("reparse");
        let target = directory.path().join("target.exe");
        let link = directory.path().join("link.exe");
        fs::write(&target, b"trust image fixture").expect("write trust-image target");
        std::os::windows::fs::symlink_file(&target, &link).expect("create a leaf symlink");

        let error =
            WindowsAuthenticodeImage::open(&link).expect_err("a leaf reparse point is refused");
        assert!(error.detail().contains("reparse point"), "{error}");
        assert!(error.close_failure().is_none(), "{error}");
    }

    #[test]
    fn windows_signature_cardinality_rejects_secondary_or_nonprimary_selection() {
        validate_windows_signature_cardinality(0, 0).expect("one primary signature");
        for (secondary, verified_index) in [(1, 0), (u32::MAX, 0), (0, 1), (1, 1)] {
            let error = validate_windows_signature_cardinality(secondary, verified_index)
                .expect_err("ambiguous Authenticode signature set must fail");
            assert!(
                error.detail().contains("exactly one signature is required"),
                "{error}"
            );
        }
    }

    /// A trusted foreign executable with an embedded Authenticode signature passes
    /// OS trust but cannot mint a Keld app identity. Windows system binaries commonly
    /// use catalog signatures instead, which this file-handle verifier rejects.
    #[test]
    fn trusted_windows_program_without_keld_app_id_is_refused_after_trust() {
        let executable = std::env::var_os("KELD_TEST_FOREIGN_AUTHENTICODE_CARRIER").map_or_else(
            || {
                PathBuf::from(
                    std::env::var_os("ProgramFiles").expect("native Windows provides ProgramFiles"),
                )
                .join("Git/cmd/git.exe")
            },
            PathBuf::from,
        );
        assert!(
            executable.is_file(),
            "provide a trusted third-party embedded-signed executable: {}",
            executable.display()
        );

        for attempt in 0..2 {
            let image = WindowsAuthenticodeImage::open(&executable)
                .expect("open the installed, embedded-signed third-party image");
            let error = image.verify().expect_err(
                "a trusted third-party-signed executable cannot become a Keld application",
            );
            assert!(
                matches!(
                    error.detail(),
                    "the verified Authenticode signature has no Keld app-id description"
                        | "the verified signature has no authenticated Keld app-id attribute"
                ),
                "attempt {attempt}: expected a post-trust Keld app-id refusal, got {error}"
            );
            assert!(
                error.close_failure().is_none(),
                "attempt {attempt}: post-trust refusal reported a CLOSE failure: {error}"
            );
        }
    }

    #[test]
    fn current_test_executable_is_rejected_by_win_verify_trust() {
        let executable = std::env::current_exe().expect("the running test executable path");
        let image = WindowsAuthenticodeImage::open(&executable)
            .expect("the running image opens as a pinned trust image");
        let error = image
            .verify()
            .expect_err("the unsigned test executable is not a signed Keld carrier");
        // The pinned running image reached WinVerifyTrust and was refused there.
        assert!(
            error.detail().contains("WinVerifyTrust rejected"),
            "{error}"
        );
        assert!(error.close_failure().is_none(), "{error}");
    }

    #[test]
    fn refusal_text_carries_detail_fix_and_any_close_failure() {
        let refusal = windows_identity_error("synthetic refusal", "Synthetic fix.");
        assert_eq!(refusal.to_string(), "synthetic refusal. Synthetic fix.");
        assert!(refusal.close_failure().is_none());

        let combined = refusal.with_close_failure(windows_trust_close_error(-2_146_762_496));
        let close_failure = combined.close_failure().expect("the close failure is kept");
        assert_eq!(
            close_failure.detail(),
            "WinVerifyTrust could not close verified state (status 0x800b0100)"
        );
        assert_eq!(combined.detail(), "synthetic refusal");
        assert_eq!(
            combined.to_string(),
            "synthetic refusal. Synthetic fix.; cleanup: WinVerifyTrust could not close verified state (status 0x800b0100). Restart Windows and retry the signed package; if this persists, repair the package trust installation."
        );
    }

    /// Runs only on the KEL-135 operator path, with a trusted Authenticode-signed carrier.
    #[test]
    #[ignore = "requires a trusted Authenticode-signed fixture executable"]
    fn kel135_verified_image_file_is_the_pinned_verified_handle() {
        use std::os::windows::fs::{FileExt as _, MetadataExt as _};

        let carrier = std::env::var_os("KELD_KEL135_CARRIER_UNDER_TEST")
            .expect("KELD_KEL135_CARRIER_UNDER_TEST must name a signed carrier");
        let carrier = Path::new(&carrier);
        let image = WindowsAuthenticodeImage::open(carrier).expect("the signed carrier opens");
        let opened = image.image.as_raw_handle();
        let verified = image.verify().expect("the signed carrier verifies");

        // The verified handle is the opened one, never a reopen by path.
        assert_eq!(verified.file().as_raw_handle(), opened);
        // It still shares only reads, so a writer is refused while it is held.
        let writer = fs::OpenOptions::new()
            .write(true)
            .open(carrier)
            .expect_err("a writer opened the verified image");
        assert_eq!(writer.raw_os_error(), Some(32), "{writer}");
        // Its file facts are the opened path's, and every byte read through it matches.
        let held = verified
            .file()
            .metadata()
            .expect("verified handle metadata");
        let named = fs::metadata(carrier).expect("carrier path metadata");
        assert_eq!(
            (
                held.file_size(),
                held.creation_time(),
                held.last_write_time()
            ),
            (
                named.file_size(),
                named.creation_time(),
                named.last_write_time()
            )
        );
        let expected = fs::read(carrier).expect("read the carrier by path for comparison");
        let mut through_handle = vec![0_u8; expected.len()];
        let mut filled = 0_usize;
        while filled < through_handle.len() {
            let offset = u64::try_from(filled).expect("the offset fits u64");
            let read = verified
                .file()
                .seek_read(&mut through_handle[filled..], offset)
                .expect("positioned read through the verified handle");
            assert_ne!(read, 0, "the verified image ended early");
            filled += read;
        }
        assert_eq!(through_handle, expected);
        println!(
            "KELD_KEL135_VERIFIED_HANDLE same_handle=true writer_refused=true bytes={} app_id={}",
            expected.len(),
            verified.identity().app_id()
        );
    }
}
