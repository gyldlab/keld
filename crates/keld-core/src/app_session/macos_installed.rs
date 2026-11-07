//! Opaque installed macOS `LocalDevelopment` admission from this executable.
//!
//! Only the current process may establish a local bundle origin. The Apple
//! dynamic and full static seals prove integrity at validation time; the
//! retained no-follow handles and repeated handoff checks detect substitution.
//! An ad-hoc outer signature is not a publisher identity or a durable store.

use std::ffi::OsStr;
use std::fs::{File, Metadata};
use std::io;
use std::os::fd::AsFd;
use std::os::unix::fs::{FileExt as _, MetadataExt as _};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use keld_pack::{
    MACOS_APP_RELATIVE_PATH, MACOS_BUN_RELATIVE_PATH, MACOS_HOST_EXECUTABLE,
    MAX_MACOS_METADATA_BYTES, MacosBundleMetadata, validate_macos_executable_metadata,
};
use keld_wv::profile::validate_canonical_app_id;
use nix::fcntl::{OFlag, open, openat};
use nix::sys::stat::Mode;
use sha2::{Digest as _, Sha256};

use crate::macos_profile_identity::{
    MacLocalSigningInfo, verified_current_local_bundle_signing_info,
};

use super::boot_contract::{ParsedBoot, parse_installed_boot_bytes};
use super::graph_contract::{AdmittedStageGraph, read_bound_graph_leaf_from_retained_root};
use super::launch_facts::{
    LaunchFacts, MACOS_LOCAL_LAUNCH_PRELOAD_PATH, macos_local_launch_bootstrap_sources,
};
use super::{
    AppBootSelection, BOOT_FILE, HostAppError, InitialWindow, MAX_BOOT_BYTES, PERMISSIONS_FILE,
    ValidatedInitialWindow,
};

const FIX_LOCAL_BUNDLE: &str = "Reinstall an intact locally ad-hoc signed Keld .app with the fixed Host and Bun layout; do not edit signed contents.";
const MAX_LOCAL_POLICY_BYTES: usize = 64 * 1024;
const MAX_LOCAL_RENDERER_BYTES: usize = 16 * 1024 * 1024;
const MAX_LOCAL_BUN_BYTES: u64 = 1024 * 1024 * 1024;

fn admission_error(where_: &'static str, detail: impl std::fmt::Display) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-036",
        where_,
        detail.to_string(),
        FIX_LOCAL_BUNDLE,
    )
}

/// A lexical discriminator only. It cannot mint local origin or grant authority.
///
/// A malformed bundle candidate must be refused by installed admission and may
/// never quietly fall through to the owner-private dev-stage branch.
pub(super) fn current_exe_is_local_app_candidate() -> Result<bool, HostAppError> {
    let executable =
        std::env::current_exe().map_err(|error| admission_error("current executable", error))?;
    Ok(path_is_local_app_candidate(&executable))
}

fn path_is_local_app_candidate(executable: &Path) -> bool {
    // Treat any executable inside an enclosing .app as a candidate. Even a
    // malformed local installation must not be routed to dev-stage cleanup.
    // This broad test grants no capability; the exact layout is checked below.
    executable.is_absolute()
        && executable
            .ancestors()
            .skip(1)
            .any(|ancestor| ancestor.extension() == Some(OsStr::new("app")))
}

fn path_has_exact_local_host_layout(executable: &Path) -> bool {
    let Some(macos) = executable.parent() else {
        return false;
    };
    let Some(contents) = macos.parent() else {
        return false;
    };
    let Some(bundle) = contents.parent() else {
        return false;
    };
    executable.is_absolute()
        && executable.file_name() == Some(OsStr::new(MACOS_HOST_EXECUTABLE))
        && macos.file_name() == Some(OsStr::new("MacOS"))
        && contents.file_name() == Some(OsStr::new("Contents"))
        && bundle.extension() == Some(OsStr::new("app"))
}

fn verify_running_bundle_location(signing: &MacLocalSigningInfo) -> Result<(), HostAppError> {
    if !path_has_exact_local_host_layout(&signing.main_executable) {
        return Err(admission_error(
            "local application layout",
            "the signed main executable is not Contents/MacOS/keld-host in a .app",
        ));
    }
    let expected_main = signing
        .bundle_path
        .join("Contents/MacOS")
        .join(MACOS_HOST_EXECUTABLE);
    if expected_main != signing.main_executable {
        return Err(admission_error(
            "local executable identity",
            "the running main executable is outside the verified bundle",
        ));
    }
    let actual_main =
        std::env::current_exe().map_err(|error| admission_error("current executable", error))?;
    if actual_main != signing.main_executable {
        return Err(admission_error(
            "local executable identity",
            "the current executable differs from the Security.framework main executable",
        ));
    }
    Ok(())
}

fn require_local_executable(file: &File, kind: &'static str) -> Result<(), HostAppError> {
    let metadata = file
        .metadata()
        .map_err(|error| admission_error(kind, error))?;
    validate_macos_executable_metadata(&metadata).map_err(|error| admission_error(kind, error))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    mode: u32,
    size: u64,
    modified_sec: i64,
    modified_ns: i64,
    changed_sec: i64,
    changed_ns: i64,
}

impl FileIdentity {
    fn of(file: &File, kind: &'static str) -> Result<Self, HostAppError> {
        let meta = file
            .metadata()
            .map_err(|error| admission_error(kind, error))?;
        Ok(Self::from_metadata(&meta))
    }

    fn from_metadata(meta: &Metadata) -> Self {
        Self {
            device: meta.dev(),
            inode: meta.ino(),
            mode: meta.mode(),
            size: meta.len(),
            modified_sec: meta.mtime(),
            modified_ns: meta.mtime_nsec(),
            changed_sec: meta.ctime(),
            changed_ns: meta.ctime_nsec(),
        }
    }
}

fn open_launch_bootstrap(app: &File) -> Result<Vec<(File, FileIdentity)>, HostAppError> {
    // Every generated module is qualified against this Host's compiled
    // owner bytes. The sealed application cannot select another bootstrap.
    let mut launch_bootstrap = Vec::new();
    for (path, expected) in macos_local_launch_bootstrap_sources() {
        let file = open_file(app, Path::new(path), "installed launch bootstrap")?;
        let identity = FileIdentity::of(&file, "installed launch bootstrap")?;
        let actual = read_bounded_at(&file, expected.len(), "installed launch bootstrap")?;
        if actual != expected || FileIdentity::of(&file, "installed launch bootstrap")? != identity
        {
            return Err(admission_error(
                "installed launch bootstrap",
                format!("generated owner bytes differ or changed: {path}"),
            ));
        }
        launch_bootstrap.push((file, identity));
    }
    Ok(launch_bootstrap)
}

fn read_sealed_local_metadata(
    contents: &File,
    resources: &File,
    bun: &File,
    signing_identifier: &str,
) -> Result<(File, MacosBundleMetadata), HostAppError> {
    let info = open_file(contents, Path::new("Info.plist"), "Info.plist")?;
    let plist_bytes = read_bounded_at(&info, MAX_MACOS_METADATA_BYTES, "Info.plist")?;
    let metadata = MacosBundleMetadata::from_plist(&plist_bytes)
        .map_err(|error| admission_error("sealed Info.plist", error))?;
    validate_canonical_app_id(&metadata.app_id)
        .map_err(|error| admission_error("local application id", error))?;
    if signing_identifier != metadata.app_id.as_str() {
        return Err(admission_error(
            "local application id",
            "sealed CFBundleIdentifier differs from the outer signing identifier",
        ));
    }
    if metadata.icon {
        let _icon = open_file(resources, Path::new("app.icns"), "bundle icon")?;
    }
    if hash_bun(bun)? != metadata.bun_sha256 {
        return Err(admission_error(
            "bundled Bun digest",
            "the opened Bun bytes differ from the sealed bunSha256",
        ));
    }
    Ok((info, metadata))
}

fn read_installed_local_boot_files(
    app: &File,
    metadata: &MacosBundleMetadata,
) -> Result<(File, Vec<u8>, ParsedBoot, File, File), HostAppError> {
    let boot = open_file(app, Path::new(BOOT_FILE), "boot descriptor")?;
    let boot_bytes = read_bounded_at(&boot, MAX_BOOT_BYTES, "boot descriptor")?;
    let parsed = parse_installed_boot_bytes(&boot_bytes)?;
    if parsed.name != metadata.name {
        return Err(admission_error(
            "boot application identity",
            "the compiled boot name differs from the sealed CFBundleName",
        ));
    }
    if let Some(selected) = &parsed.installed_graph {
        // The installed reader is independently authoritative. A caller can
        // re-sign a bundle without using Pack's artifact-name validator.
        super::launch_facts::require_safe_installed_name(&metadata.name)?;
        if selected
            .single_instance
            .as_ref()
            .is_some_and(|declaration| declaration.id() != metadata.app_id)
        {
            return Err(admission_error(
                "single-instance identity",
                "declaration differs from the sealed application identifier",
            ));
        }
    }
    let policy = open_file(app, Path::new(PERMISSIONS_FILE), "permissions file")?;
    if hash_bounded_file(&policy, MAX_LOCAL_POLICY_BYTES, "permissions file")?
        != parsed.permissions_digest
    {
        return Err(admission_error(
            "permission identity",
            "the opened permissions file differs from the compiled boot digest",
        ));
    }
    let entry = open_file(app, &parsed.entry, "application entry")?;
    Ok((boot, boot_bytes, parsed, policy, entry))
}

/// The live signed installation, retained across the Host's session.
///
/// None of these fields are caller-supplied. File handles outlive the boot,
/// profile, policy and Guardian decisions that depend upon their identity.
#[derive(Debug)]
pub(super) struct ValidatedMacLocalBundle {
    bundle_path: PathBuf,
    app_path: PathBuf,
    resources_path: PathBuf,
    bun_path: PathBuf,
    signing_identifier: String,
    metadata: MacosBundleMetadata,
    _bundle: File,
    _contents: File,
    _macos: File,
    _helpers: File,
    _resources: File,
    app: File,
    host: File,
    bun: File,
    info: File,
    boot: File,
    policy: File,
    entry: File,
    launch_bootstrap: Vec<(File, FileIdentity)>,
    policy_digest: [u8; 32],
    identities: [FileIdentity; 11],
    boot_bytes: Vec<u8>,
    graph: Option<Arc<AdmittedStageGraph>>,
    single_instance: Option<crate::hello::SingleInstanceDeclaration>,
    application_name_for_user_agent: Option<keld_wv::ApplicationNameForUserAgent>,
}

impl ValidatedMacLocalBundle {
    /// Re-derives local provenance from Security.framework's current process.
    ///
    /// The outer code must be ad-hoc and dynamically valid; all sealed code,
    /// resources and present architectures undergo strict static verification.
    /// Every fixed root is opened without following symlinks, and Bun bytes
    /// must equal the digest in the sealed Info.plist.
    pub(super) fn from_current_process() -> Result<Self, HostAppError> {
        if !cfg!(target_arch = "aarch64") {
            return Err(admission_error(
                "local bundle target",
                "the first local macOS artifact admits only an arm64 Host process",
            ));
        }
        let signing = verified_current_local_bundle_signing_info()
            .map_err(|error| admission_error("current local code signature", error))?;
        verify_running_bundle_location(&signing)?;
        let bundle = open_absolute_directory(&signing.bundle_path)?;
        require_directory_mode(&bundle, "bundle root")?;
        let contents = open_directory(&bundle, OsStr::new("Contents"), "Contents")?;
        let macos = open_directory(&contents, OsStr::new("MacOS"), "MacOS")?;
        let helpers = open_directory(&contents, OsStr::new("Helpers"), "Helpers")?;
        let resources = open_directory(&contents, OsStr::new("Resources"), "Resources")?;
        let app = open_directory(&resources, OsStr::new("app"), "app resources")?;
        let host = open_file(&macos, Path::new(MACOS_HOST_EXECUTABLE), "Host executable")?;
        let bun = open_file(&helpers, Path::new("bun"), "bundled Bun")?;
        for (kind, file) in [("Host executable", &host), ("bundled Bun", &bun)] {
            require_local_executable(file, kind)?;
        }
        let (info, metadata) =
            read_sealed_local_metadata(&contents, &resources, &bun, &signing.signing_identifier)?;
        let (boot, boot_bytes, parsed, policy, entry) =
            read_installed_local_boot_files(&app, &metadata)?;
        let launch_bootstrap = open_launch_bootstrap(&app)?;
        let app_path = signing.bundle_path.join(MACOS_APP_RELATIVE_PATH);
        let graph = parsed
            .installed_graph
            .as_ref()
            .map(|selected| {
                read_bound_graph_leaf_from_retained_root(
                    &app_path,
                    &app,
                    selected.graph_sha256,
                    parsed
                        .entry
                        .to_str()
                        .ok_or_else(|| admission_error("graph entry", "entry is not UTF-8"))?,
                    &selected.source,
                )
            })
            .transpose()?
            .map(Arc::new);
        let (single_instance, application_name_for_user_agent) =
            parsed.installed_graph.map_or((None, None), |selected| {
                (
                    selected.single_instance,
                    selected.application_name_for_user_agent,
                )
            });
        let identities = [
            FileIdentity::of(&bundle, "bundle root")?,
            FileIdentity::of(&contents, "Contents")?,
            FileIdentity::of(&macos, "MacOS")?,
            FileIdentity::of(&helpers, "Helpers")?,
            FileIdentity::of(&resources, "Resources")?,
            FileIdentity::of(&app, "app resources")?,
            FileIdentity::of(&host, "Host executable")?,
            FileIdentity::of(&bun, "bundled Bun")?,
            FileIdentity::of(&info, "Info.plist")?,
            FileIdentity::of(&boot, "boot descriptor")?,
            FileIdentity::of(&policy, "permissions file")?,
        ];
        Ok(Self {
            bundle_path: signing.bundle_path.clone(),
            app_path,
            resources_path: signing.bundle_path.join("Contents/Resources"),
            bun_path: signing.bundle_path.join(MACOS_BUN_RELATIVE_PATH),
            signing_identifier: signing.signing_identifier,
            metadata,
            _bundle: bundle,
            _contents: contents,
            _macos: macos,
            _helpers: helpers,
            _resources: resources,
            app,
            host,
            bun,
            info,
            boot,
            policy,
            entry,
            launch_bootstrap,
            policy_digest: parsed.permissions_digest,
            identities,
            boot_bytes,
            graph,
            single_instance,
            application_name_for_user_agent,
        })
    }

    pub(super) fn app_root(&self) -> &Path {
        &self.app_path
    }

    pub(super) fn resources_root(&self) -> &Path {
        &self.resources_path
    }

    pub(super) fn bun_path(&self) -> &Path {
        &self.bun_path
    }

    pub(super) fn app_name(&self) -> &str {
        &self.metadata.name
    }

    pub(super) fn app_version(&self) -> &str {
        &self.metadata.version
    }

    pub(super) fn graph(&self) -> Option<&Arc<AdmittedStageGraph>> {
        self.graph.as_ref()
    }

    pub(super) fn single_instance(&self) -> Option<&crate::hello::SingleInstanceDeclaration> {
        self.single_instance.as_ref()
    }

    pub(super) fn application_name_for_user_agent(
        &self,
    ) -> Option<&keld_wv::ApplicationNameForUserAgent> {
        self.application_name_for_user_agent.as_ref()
    }

    pub(super) fn integrity_pins(&self) -> MacLocalIntegrityPins {
        let mut boot_digest = [0_u8; 32];
        boot_digest.copy_from_slice(&Sha256::digest(&self.boot_bytes));
        MacLocalIntegrityPins {
            bun: self.metadata.bun_sha256,
            boot: boot_digest,
            permissions: self.policy_digest,
        }
    }

    /// Refreshes Apple's full seal and each retained filesystem identity.
    ///
    /// The Host retains this origin for the whole session. A Guardian derives
    /// its own origin independently and calls this before every new generation.
    pub(super) fn revalidate_generation(&self) -> Result<(), HostAppError> {
        if let Some(graph) = &self.graph {
            graph.recheck()?;
        }
        let next = Self::from_current_process()?;
        if self.graph.as_deref().map(AdmittedStageGraph::graph_sha256)
            != next.graph.as_deref().map(AdmittedStageGraph::graph_sha256)
        {
            return Err(admission_error(
                "local graph identity",
                "bound graph changed between generations",
            ));
        }
        if self.launch_bootstrap.len() != next.launch_bootstrap.len() {
            return Err(admission_error(
                "local generation identity",
                "installed bootstrap module set changed after admission",
            ));
        }
        for ((retained, identity), (reopened, next_identity)) in
            self.launch_bootstrap.iter().zip(&next.launch_bootstrap)
        {
            if identity != next_identity
                || FileIdentity::of(retained, "retained launch bootstrap")?
                    != FileIdentity::of(reopened, "reopened launch bootstrap")?
            {
                return Err(admission_error(
                    "local generation identity",
                    "installed bootstrap module identity changed after admission",
                ));
            }
        }
        if self.bundle_path != next.bundle_path
            || self.app_path != next.app_path
            || self.resources_path != next.resources_path
            || self.bun_path != next.bun_path
            || self.signing_identifier != next.signing_identifier
            || self.metadata != next.metadata
            || self.policy_digest != next.policy_digest
            || self.identities != next.identities
            || self.boot_bytes != next.boot_bytes
            || FileIdentity::of(&self.host, "retained Host")?
                != FileIdentity::of(&next.host, "reopened Host")?
            || FileIdentity::of(&self.bun, "retained Bun")?
                != FileIdentity::of(&next.bun, "reopened Bun")?
            || FileIdentity::of(&self.info, "retained metadata")?
                != FileIdentity::of(&next.info, "reopened metadata")?
            || FileIdentity::of(&self.boot, "retained boot")?
                != FileIdentity::of(&next.boot, "reopened boot")?
            || FileIdentity::of(&self.policy, "retained permissions")?
                != FileIdentity::of(&next.policy, "reopened permissions")?
            || FileIdentity::of(&self.entry, "retained entry")?
                != FileIdentity::of(&next.entry, "reopened entry")?
        {
            return Err(admission_error(
                "local generation identity",
                "installed bundle, boot, policy, entry or Bun identity changed after admission",
            ));
        }
        Ok(())
    }
}

/// Parent-to-Guardian bundle inputs for one authenticated private role.
///
/// These bytes come from Core's validated local bundle, not a caller path or
/// source-tree file. The Guardian independently derives all three before Bun.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacLocalIntegrityPins {
    bun: [u8; 32],
    boot: [u8; 32],
    permissions: [u8; 32],
}

impl MacLocalIntegrityPins {
    /// Decodes exactly three canonical lowercase 64-hex SHA-256 values from
    /// a private, authenticated Host-to-Guardian role invocation.
    ///
    /// This is inert expected data until the Guardian compares every value
    /// with its independently opened, sealed runtime, boot and policy facts.
    ///
    /// # Errors
    ///
    /// Refuses malformed, uppercase, non-ASCII, empty or overlong hex values.
    pub fn from_encoded(
        bun_sha256: &str,
        boot_sha256: &str,
        permissions_sha256: &str,
    ) -> Result<Self, HostAppError> {
        Ok(Self {
            bun: decode_lower_hex_32(bun_sha256)?,
            boot: decode_lower_hex_32(boot_sha256)?,
            permissions: decode_lower_hex_32(permissions_sha256)?,
        })
    }

    /// Canonical encoded fields for the fixed `local-bundle-v1` private role.
    #[must_use]
    pub fn to_encoded(&self) -> [String; 3] {
        [
            encode_lower_hex_32(&self.bun),
            encode_lower_hex_32(&self.boot),
            encode_lower_hex_32(&self.permissions),
        ]
    }

    /// Reconstructs parent-selected bytes from the authenticated private role.
    ///
    /// Merely holding these three values provides no authority. The Guardian
    /// must call `verify_parent_integrity` against its own independently
    /// validated origin before using the selected executable.
    #[must_use]
    pub const fn from_parent_bytes(
        bun_sha256: [u8; 32],
        boot_sha256: [u8; 32],
        permissions_sha256: [u8; 32],
    ) -> Self {
        Self {
            bun: bun_sha256,
            boot: boot_sha256,
            permissions: permissions_sha256,
        }
    }

    /// SHA-256 of the opened runtime executable selected by the Host.
    #[must_use]
    pub const fn bun_sha256(&self) -> &[u8; 32] {
        &self.bun
    }

    /// SHA-256 of the exact sealed boot descriptor selected by the Host.
    #[must_use]
    pub const fn boot_sha256(&self) -> &[u8; 32] {
        &self.boot
    }

    /// SHA-256 of the verified permission manifest selected by the Host.
    #[must_use]
    pub const fn permissions_sha256(&self) -> &[u8; 32] {
        &self.permissions
    }
}

/// One Core-admitted boot and its retained installed origin.
///
/// The app-session owner consumes these fields into its opaque macOS boot
/// origin. The origin remains alive through Guard preflight and shutdown.
pub(super) struct ValidatedMacLocalBoot {
    pub(super) schema: u8,
    pub(super) app: AppBootSelection,
    pub(super) permissions_file: File,
    pub(super) permissions_digest: [u8; 32],
}

/// Validates the sealed local origin, then reuses the existing boot and policy
/// byte parsers. No caller supplies a root or an executable.
pub(super) fn validate_current_exe_local_bundle() -> Result<ValidatedMacLocalBoot, HostAppError> {
    let origin = ValidatedMacLocalBundle::from_current_process()?;
    let parsed = parse_installed_boot_bytes(&origin.boot_bytes)?;
    let initial_window = match parsed.initial {
        InitialWindow::Renderer(renderer) => {
            let file = open_file(&origin.app, &renderer, "renderer")?;
            let html = read_bounded_at(&file, MAX_LOCAL_RENDERER_BYTES, "renderer")?;
            std::str::from_utf8(&html).map_err(|error| {
                admission_error("renderer", format!("HTML is not UTF-8: {error}"))
            })?;
            ValidatedInitialWindow::Renderer(html)
        }
        InitialWindow::None => ValidatedInitialWindow::None,
    };
    let permissions_file = origin
        .policy
        .try_clone()
        .map_err(|error| admission_error("permissions handle", error))?;
    let entry_file = origin
        .entry
        .try_clone()
        .map_err(|error| admission_error("entry handle", error))?;
    Ok(ValidatedMacLocalBoot {
        schema: parsed.schema,
        app: AppBootSelection {
            root: origin.app_path.clone(),
            name: parsed.name,
            entry_path: parsed.entry,
            entry_file,
            initial_window,
            auth_configuration: parsed.auth_configuration,
            mac_origin: super::MacBootOrigin::LocalDevelopment(Box::new(origin)),
        },
        permissions_file,
        permissions_digest: parsed.permissions_digest,
    })
}

/// A Guardian-only proof for a local installed bundle.
///
/// This value can only be minted from the Guardian's own running executable;
/// arguments from the Host's private role channel are checked against it.
#[derive(Debug)]
pub struct MacLocalGuardianFacts {
    origin: ValidatedMacLocalBundle,
    entry_relative: PathBuf,
    entry_identity: FileIdentity,
    native_facade: bool,
    original_graph: bool,
    launch_facts: LaunchFacts,
}

fn decode_lower_hex_32(value: &str) -> Result<[u8; 32], HostAppError> {
    let bytes = value.as_bytes();
    if bytes.len() != 64
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(admission_error(
            "private local-bundle-v1 digest",
            "expected exactly 64 lowercase hexadecimal bytes",
        ));
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
        let high = if pair[0].is_ascii_digit() {
            pair[0] - b'0'
        } else {
            pair[0] - b'a' + 10
        };
        let low = if pair[1].is_ascii_digit() {
            pair[1] - b'0'
        } else {
            pair[1] - b'a' + 10
        };
        digest[index] = (high << 4) | low;
    }
    Ok(digest)
}

fn encode_lower_hex_32(value: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in value {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

impl MacLocalGuardianFacts {
    /// Validates the Guardian's own current executable and sealed app.
    ///
    /// # Errors
    ///
    /// Refuses missing/altered local signing, layout, boot, permissions or Bun.
    pub fn from_current_exe() -> Result<Self, HostAppError> {
        let origin = ValidatedMacLocalBundle::from_current_process()?;
        if origin.graph().is_some() {
            return Err(admission_error(
                "installed original handoff",
                "an original graph requires the authenticated local-bundle-v2 facts handoff",
            ));
        }
        let parsed = parse_installed_boot_bytes(&origin.boot_bytes)?;
        let native_facade = matches!(parsed.initial, InitialWindow::None);
        let entry_relative = parsed.entry;
        let entry_identity = FileIdentity::of(&origin.entry, "bundled app entry")?;
        let launch_facts = LaunchFacts::from_validated_local(&origin)?;
        Ok(Self {
            origin,
            entry_relative,
            entry_identity,
            native_facade,
            original_graph: false,
            launch_facts,
        })
    }

    /// Independently admits a sealed original graph, then consumes its Host's
    /// bounded native observations after registration and host-retention ACK.
    ///
    /// # Errors
    /// Refuses a missing graph, altered sealed input, ambiguous policy, forged
    /// native shape or declaration/verdict mismatch before Bun is created.
    pub fn from_current_exe_with_host_facts(text: &str) -> Result<Self, HostAppError> {
        let origin = ValidatedMacLocalBundle::from_current_process()?;
        let parsed = parse_installed_boot_bytes(&origin.boot_bytes)?;
        let graph = origin.graph().ok_or_else(|| {
            admission_error(
                "installed original handoff",
                "local-bundle-v2 requires the sealed installed graph",
            )
        })?;
        let policy = origin
            .policy
            .try_clone()
            .map_err(|error| admission_error("Guardian permissions handle", error))?;
        let verified = keld_guard::verified_manifest::load_verified_manifest(
            policy,
            origin.app_root().join(PERMISSIONS_FILE),
            origin.policy_digest,
        )
        .map_err(HostAppError::manifest)?;
        super::profile_preflight::validate_installed_profile(parsed.schema, &verified)?;
        let selected_policy = verified
            .resolve_unique_original_window_csp()
            .map_err(HostAppError::manifest)?;
        let launch_facts = LaunchFacts::from_authenticated_installed(
            text,
            &origin,
            graph.graph_sha256(),
            &selected_policy,
            origin.single_instance().is_some(),
        )?;
        let entry_identity = FileIdentity::of(&origin.entry, "bundled app entry")?;
        Ok(Self {
            origin,
            entry_relative: parsed.entry,
            entry_identity,
            native_facade: true,
            original_graph: true,
            launch_facts,
        })
    }

    /// Exact current-process-derived resource root, never a role argument.
    #[must_use]
    pub fn app_root(&self) -> &Path {
        self.origin.app_root()
    }

    /// Exact current-process-derived Contents/Resources root.
    #[must_use]
    pub fn resources_root(&self) -> &Path {
        self.origin.resources_root()
    }

    /// Verified application entry relative to `app_root()`.
    #[must_use]
    pub fn entry_relative(&self) -> &Path {
        &self.entry_relative
    }

    /// Verified fixed bundled Bun executable, never PATH-selected.
    #[must_use]
    pub fn bun_path(&self) -> &Path {
        self.origin.bun_path()
    }

    /// One immutable parse-time record from this Guardian's admitted bundle.
    ///
    /// # Errors
    ///
    /// Refuses an encoding/size failure; never falls back to environment data.
    pub fn launch_facts_bun_argument(&self) -> Result<String, HostAppError> {
        self.launch_facts.bun_define_argument()
    }

    /// Fixed, exact-byte-checked preload below the verified application root.
    #[must_use]
    pub fn launch_facts_preload_path(&self) -> PathBuf {
        self.app_root().join(MACOS_LOCAL_LAUNCH_PRELOAD_PATH)
    }

    /// Selects the existing API runtime-Init wrapper from the admitted graph.
    ///
    /// The graph owner checks the fixed provider member through its retained
    /// root and expected digest. An original graph never falls back to an app
    /// sidecar or skips its required first post-HELLO runtime-facts exchange.
    ///
    /// # Errors
    /// Refuses inconsistent original origin or a missing/changed wrapper.
    pub fn runtime_preload_path(&self) -> Result<Option<PathBuf>, HostAppError> {
        match (self.original_graph, self.origin.graph()) {
            (true, Some(graph)) => graph.installed_runtime_preload().map(Some),
            (false, None) => Ok(None),
            _ => Err(admission_error(
                "installed runtime preload",
                "original launch mode and retained installed graph disagree",
            )),
        }
    }

    /// Whether the sealed boot descriptor selects Host-owned native windows.
    ///
    /// The Guardian uses this value instead of a caller argument to choose the
    /// canonical preload; a disagreement with the Host role argument refuses.
    #[must_use]
    pub const fn native_facade(&self) -> bool {
        self.native_facade
    }

    /// Whether current-code admission selected the installed original graph.
    #[must_use]
    pub const fn original_graph(&self) -> bool {
        self.original_graph
    }

    /// Independently verified installed runtime, boot and permission digests.
    #[must_use]
    pub fn integrity_pins(&self) -> MacLocalIntegrityPins {
        self.origin.integrity_pins()
    }

    /// Requires the parent-selected bundle inputs to match this Guardian's
    /// independently validated local bundle.
    ///
    /// # Errors
    ///
    /// Refuses a replaced/re-signed bundle or mismatched parent-selected input.
    pub fn verify_parent_integrity(
        &self,
        parent: MacLocalIntegrityPins,
    ) -> Result<(), HostAppError> {
        if self.integrity_pins() != parent {
            return Err(admission_error(
                "Guardian parent bundle identity",
                "Host and Guardian selected different runtime, boot or permission bytes",
            ));
        }
        Ok(())
    }

    /// Refuses any Host role argument that selects a different root/entry inode.
    ///
    /// # Errors
    ///
    /// Mismatched parent/Guardian path, inode or device cannot select execution.
    pub fn verify_parent_entry(
        &self,
        supplied_root: &Path,
        supplied_entry: &Path,
        expected_device: u64,
        expected_inode: u64,
    ) -> Result<(), HostAppError> {
        if supplied_root != self.app_root()
            || supplied_entry != self.entry_relative()
            || expected_device != self.entry_identity.device
            || expected_inode != self.entry_identity.inode
        {
            return Err(admission_error(
                "Guardian parent entry",
                "the parent-requested root or entry differs from the Guardian's sealed bundle",
            ));
        }
        Ok(())
    }

    /// Refuses an altered local seal, runtime, policy or entry before new Bun.
    ///
    /// # Errors
    ///
    /// A changed on-disk identity causes a generation admission failure.
    pub fn revalidate_generation(&self) -> Result<(), HostAppError> {
        self.origin.revalidate_generation()
    }
}

fn open_absolute_directory(path: &Path) -> Result<File, HostAppError> {
    if !path.is_absolute() {
        return Err(admission_error(
            "local bundle root",
            "the signed bundle path is not absolute",
        ));
    }
    let fd = open(
        Path::new("/"),
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| admission_error("local bundle ancestry", error))?;
    let mut current = File::from(fd);
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                // Every ancestor is opened relative to the previously retained
                // directory, so a parent symlink never silently redirects us.
                let fd = openat(
                    current.as_fd(),
                    name,
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|error| admission_error("local bundle ancestry", error))?;
                current = File::from(fd);
            }
            _ => {
                return Err(admission_error(
                    "local bundle ancestry",
                    "the bundle path is not a lexical absolute path",
                ));
            }
        }
    }
    Ok(current)
}

fn open_directory(parent: &File, name: &OsStr, kind: &'static str) -> Result<File, HostAppError> {
    let fd = openat(
        parent.as_fd(),
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| admission_error(kind, error))?;
    let directory = File::from(fd);
    require_directory_mode(&directory, kind)?;
    Ok(directory)
}

fn require_directory_mode(directory: &File, kind: &'static str) -> Result<(), HostAppError> {
    let metadata = directory
        .metadata()
        .map_err(|error| admission_error(kind, error))?;
    if !metadata.is_dir() || metadata.mode() & 0o7777 != 0o755 {
        return Err(admission_error(
            kind,
            "the signed bundle's fixed directory must be a real 0755 directory",
        ));
    }
    Ok(())
}

fn open_file(root: &File, path: &Path, kind: &'static str) -> Result<File, HostAppError> {
    super::open_relative_file(root, path, kind)
}

fn read_bounded_at(
    file: &File,
    maximum: usize,
    kind: &'static str,
) -> Result<Vec<u8>, HostAppError> {
    let before = FileIdentity::of(file, kind)?;
    let count = usize::try_from(before.size)
        .ok()
        .filter(|count| *count <= maximum)
        .ok_or_else(|| admission_error(kind, format!("file exceeds the {maximum}-byte bound")))?;
    let mut result = vec![0_u8; count];
    let mut filled = 0_usize;
    while filled < count {
        match file.read_at(&mut result[filled..], filled as u64) {
            Ok(0) => {
                return Err(admission_error(
                    kind,
                    "file shrank while its bytes were read",
                ));
            }
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(admission_error(kind, error)),
        }
    }
    if FileIdentity::of(file, kind)? != before {
        return Err(admission_error(kind, "file changed during validation"));
    }
    Ok(result)
}

fn hash_bounded_file(
    file: &File,
    maximum: usize,
    kind: &'static str,
) -> Result<[u8; 32], HostAppError> {
    let bytes = read_bounded_at(file, maximum, kind)?;
    let digest = Sha256::digest(&bytes);
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    Ok(output)
}

fn hash_bun(file: &File) -> Result<[u8; 32], HostAppError> {
    let before = FileIdentity::of(file, "bundled Bun")?;
    if before.size == 0 || before.size > MAX_LOCAL_BUN_BYTES {
        return Err(admission_error(
            "bundled Bun",
            "bundled Bun size is outside the supported local executable bound",
        ));
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut offset = 0_u64;
    while offset < before.size {
        let remaining = usize::try_from((before.size - offset).min(buffer.len() as u64))
            .map_err(|error| admission_error("bundled Bun", error))?;
        match file.read_at(&mut buffer[..remaining], offset) {
            Ok(0) => return Err(admission_error("bundled Bun", "Bun shrank during hashing")),
            Ok(read) => {
                hasher.update(&buffer[..read]);
                offset += read as u64;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(admission_error("bundled Bun", error)),
        }
    }
    if FileIdentity::of(file, "bundled Bun")? != before {
        return Err(admission_error("bundled Bun", "Bun changed during hashing"));
    }
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(&hasher.finalize());
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Seek as _, SeekFrom};
    use std::os::unix::fs::symlink;

    use super::{
        MacLocalIntegrityPins, hash_bounded_file, open_absolute_directory,
        path_is_local_app_candidate, read_bounded_at,
    };

    #[test]
    fn guardian_private_bundle_digest_fields_require_exact_canonical_hex() {
        let bun = "0123456789abcdef".repeat(4);
        let boot = "ff".repeat(32);
        let policy = "00".repeat(32);
        let pins = MacLocalIntegrityPins::from_encoded(&bun, &boot, &policy)
            .expect("exact private bundle digest fields");
        assert_eq!(&pins.bun_sha256()[..4], &[0x01, 0x23, 0x45, 0x67]);
        assert_eq!(&pins.bun_sha256()[28..], &[0x89, 0xab, 0xcd, 0xef]);
        assert_eq!(
            pins.to_encoded(),
            [bun.clone(), boot.clone(), policy.clone()]
        );

        for invalid in [
            String::new(),
            String::from("0"),
            String::from("00"),
            "1".repeat(63),
            "1".repeat(65),
            "G".repeat(64),
            "é".repeat(64),
            format!(" {} ", "a".repeat(62)),
        ] {
            assert!(
                MacLocalIntegrityPins::from_encoded(&invalid, &boot, &policy).is_err(),
                "malformed parent-selected digest must be rejected: {invalid:?}",
            );
        }
        assert!(MacLocalIntegrityPins::from_encoded(&bun, "FF", &policy).is_err());
        assert!(MacLocalIntegrityPins::from_encoded(&bun, &boot, "nan").is_err());
    }

    #[test]
    fn installed_candidate_requires_exact_anchored_app_shape() {
        for candidate in [
            "/Applications/App.app/Contents/MacOS/keld-host",
            "/tmp/An Example.app/Contents/MacOS/keld-host",
        ] {
            assert!(path_is_local_app_candidate(std::path::Path::new(candidate)));
        }
        for rejected in [
            "/tmp/keld-host",
            "/tmp/App/Contents/MacOS/keld-host",
            "App.app/Contents/MacOS/keld-host",
        ] {
            assert!(!path_is_local_app_candidate(std::path::Path::new(rejected)));
        }
        for malformed in [
            "/tmp/App.app/Contents/MacOS/other",
            "/tmp/App.app/Contents/Helpers/keld-host",
            "/tmp/App.app/Contents/MacOS/renamed",
            "/tmp/App.app/Contents/something-else",
        ] {
            assert!(path_is_local_app_candidate(std::path::Path::new(malformed)));
            assert!(!super::path_has_exact_local_host_layout(
                std::path::Path::new(malformed)
            ));
        }
    }

    #[test]
    fn metadata_reader_preserves_shared_offset_and_checks_bound() {
        let temp = tempfile::tempdir().expect("owned fixture");
        let path = temp.path().join("policy");
        fs::write(&path, b"abc").expect("write");
        let mut file = fs::File::open(&path).expect("open");
        file.seek(SeekFrom::End(0)).expect("end");
        assert_eq!(read_bounded_at(&file, 3, "fixture").expect("read"), b"abc");
        assert_eq!(file.stream_position().expect("position"), 3);
        assert!(read_bounded_at(&file, 2, "fixture").is_err());
        assert_eq!(
            hash_bounded_file(&file, 3, "fixture").expect("sha256"),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ],
        );
    }

    #[test]
    fn local_bundle_root_refuses_ancestor_symlink() {
        let temp = tempfile::tempdir().expect("owned fixture");
        let canonical = temp.path().canonicalize().expect("canonical fixture");
        let real = canonical.join("real");
        fs::create_dir(&real).expect("real directory");
        symlink(&real, canonical.join("alias")).expect("symlink");
        assert!(open_absolute_directory(&real).is_ok());
        assert!(open_absolute_directory(&canonical.join("alias")).is_err());
    }
}
