//! Native, local ad-hoc macOS assembly. Input qualification and policy compilation
//! belong to callers; this module checks the captured bytes and owns publication.

mod assemble;
mod capture;
#[cfg(all(test, target_arch = "aarch64"))]
mod tests;
mod tool_output;

use std::fmt;
use std::fs::{File, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::MacosBundleMetadata;

pub use capture::{FileReceipt as MacosCopiedMember, ReceiptKind as MacosCopiedMemberKind};

/// The only two leaves a copied-tree graph compiler may return to Pack.
///
/// Pack alone writes their fixed destinations before signing. These bytes and
/// the copied inventory do not confer installed admission or publisher authority.
#[derive(Debug)]
pub struct MacosGraphCompiled {
    /// Closed `keld.stage-graph.json` bytes, at most 8 MiB.
    pub graph: Vec<u8>,
    /// Closed `keld.boot.json` bytes binding that graph, at most 64 KiB.
    pub boot: Vec<u8>,
}

/// A prebuilt executable selected by an independent input qualification record.
///
/// The expected digest and version must come from that record, not a digest of an
/// otherwise unqualified input computed by the caller during this build. Assembly
/// checks content, architecture, minimum OS, executability and the input signature;
/// a successful build does not replace semantic qualification of the shipping Host.
#[derive(Debug, Clone)]
pub struct MacosExecutableInput {
    /// Absolute path, or a path relative to the canonical project directory.
    pub path: PathBuf,
    /// SHA-256 of the independently selected input, before outer application signing.
    pub sha256: [u8; 32],
    /// Exact version recorded by that input's qualification owner.
    pub version: String,
}

/// A generated application leaf from the existing boot/graph/transport compiler.
#[derive(Debug, Clone)]
pub struct MacosGeneratedFile {
    /// Canonical project-relative destination below `Contents/Resources/app`.
    pub path: String,
    /// Exact compiled bytes. Pack does not interpret the boot or permission language.
    pub bytes: Vec<u8>,
}

/// One native macOS-arm64, local ad-hoc packaging request.
#[derive(Debug)]
pub struct MacosPackageInput {
    /// Canonical invocation directory; there is no ancestor project search.
    pub project_root: PathBuf,
    /// Shared closed platform metadata. Its Bun digest must equal the selected pin.
    pub metadata: MacosBundleMetadata,
    /// Independently qualified shipping Host input.
    pub host: MacosExecutableInput,
    /// Independently qualified, already signed Bun input.
    pub bun: MacosExecutableInput,
    /// Project-relative entry, included once even when covered by a resource root.
    pub entry: String,
    /// Project-relative renderer, absent only for a supported facade boot.
    pub renderer: Option<String>,
    /// Explicit project-relative roots, without duplicates or ancestor overlaps.
    pub resources: Vec<String>,
    /// Optional project-relative `.icns` source.
    pub icon: Option<String>,
    /// Exact output from the existing CLI boot compiler.
    pub boot: Vec<u8>,
    /// Exact captured authoring permission bytes; the caller uses Guard to admit them.
    pub permissions: Vec<u8>,
    /// Canonical transport and qualified producer leaves, when present.
    pub generated: Vec<MacosGeneratedFile>,
    /// New final directory, absolute or relative to `project_root`.
    pub out_dir: PathBuf,
}

/// Paths published atomically after application, disk-image and copied-app checks.
#[derive(Debug, Clone)]
pub struct ProducedMacosLocal {
    /// Final directory. This invocation created it without replacing an existing path.
    pub directory: PathBuf,
    /// Verified application bundle.
    pub app: PathBuf,
    /// Verified read-only compressed disk image.
    pub dmg: PathBuf,
    /// External receipt; it is evidence, never an installed authority input.
    pub receipt: PathBuf,
}

/// Typed failure of the native local packaging transaction.
#[derive(Debug)]
pub struct MacosPackError {
    phase: &'static str,
    detail: String,
}

impl MacosPackError {
    pub(super) fn new(phase: &'static str, detail: impl fmt::Display) -> Self {
        Self {
            phase,
            detail: detail.to_string(),
        }
    }

    /// Stable diagnostic identity.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "KELD-PACK-014"
    }
}

impl fmt::Display for MacosPackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: macOS local packaging failed during {}: {}",
            self.code(),
            self.phase,
            self.detail
        )
    }
}

impl std::error::Error for MacosPackError {}

/// Checks the executable-file facts shared by packaging and installed admission.
///
/// Callers supply metadata from their own retained no-follow file handle. This
/// predicate neither opens a path nor treats a signature as proof of executability.
///
/// # Errors
/// Returns [`MacosPackError`] unless the file is regular with exact mode `0755`.
pub fn validate_macos_executable_metadata(metadata: &Metadata) -> Result<(), MacosPackError> {
    if !metadata.is_file() || metadata.mode() & 0o7777 != 0o755 {
        return Err(MacosPackError::new(
            "final executable mode",
            "Host and Bun must be regular files with exact POSIX mode 0755",
        ));
    }
    Ok(())
}

/// Assemble, sign, verify, construct and copy-check a local `.app` and DMG.
///
/// Only native arm64 macOS is supported. Every candidate lives in one newly owned
/// staging directory on the destination filesystem. An exclusive atomic directory
/// rename publishes the complete output set; errors never overwrite an old output.
///
/// # Errors
/// Returns [`MacosPackError`] for invalid, changed, escaping or overlapping inputs,
/// invalid native code/signatures, failed Apple tools, or an existing output path.
pub fn produce_macos_local(
    input: &MacosPackageInput,
) -> Result<ProducedMacosLocal, MacosPackError> {
    if !cfg!(target_arch = "aarch64") {
        return Err(MacosPackError::new(
            "target",
            "macos-arm64 requires a native arm64 build host",
        ));
    }
    assemble::produce(input)
}

/// Compiles graph and boot once from the exact copied application tree.
///
/// This reuses the ordinary capture/sign/image transaction. Graph mode requires
/// empty `input.boot`; it never writes a placeholder boot. The callback receives
/// Pack's copied path, its matching retained no-follow directory descriptor and
/// immutable sorted inventory. It must not mutate that tree. Pack rejects changed
/// identities, membership, contents or modes and writes only the two returned
/// fixed leaves before signing and exclusive publication.
///
/// # Errors
/// Refuses pre-supplied authority leaves, invalid copied inputs, compiler failure,
/// oversized output, changed members, or native packaging failure. A failed
/// transaction publishes no output. Runtime admission remains Core-owned.
pub fn produce_macos_local_with_graph<E, F>(
    input: &MacosPackageInput,
    finalize: F,
) -> Result<ProducedMacosLocal, MacosPackError>
where
    E: fmt::Display,
    F: FnOnce(&Path, &File, &[MacosCopiedMember]) -> Result<MacosGraphCompiled, E>,
{
    if !cfg!(target_arch = "aarch64") {
        return Err(MacosPackError::new(
            "target",
            "macos-arm64 requires a native arm64 build host",
        ));
    }
    assemble::produce_with_graph(input, finalize)
}
