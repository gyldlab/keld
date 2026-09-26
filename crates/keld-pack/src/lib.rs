//! keld-pack — canonical Windows v0 update packages and installer format contracts.
//!
//! [`Format`] names installer outputs. Architecture, signing, and assembly rules are in
//! `docs/architecture/06-runtime-and-tooling.md` §3; repository maturity and evidence
//! live in `docs/engineering/product-status.tsv`.
//! [`produce_windows_v0`] streams a package on Windows; other hosts refuse before I/O.
//! This library API does not create installers, sign releases, or activate updates.

use std::fmt;
use std::io::{self, Read, Write};

#[cfg(windows)]
mod producer;
#[cfg(test)]
mod tests;

/// Exact relative path of the content-authenticated Slice-A update policy.
pub const UPDATE_POLICY_PATH: &str = ".keld/update-policy.v1";
/// Exact UTF-8 policy payload; this slice provides no data-migration hooks.
pub const NO_MIGRATION_POLICY: &[u8] = b"{\"schema\":1,\"dataMigration\":\"none\"}\n";
/// Block size of the canonical v0 ustar representation.
pub const ARCHIVE_BLOCK_BYTES: usize = 512;
/// Positive artifact counts must fit the signed feed's JSON safe-integer domain.
pub const MAX_ARTIFACT_BYTES: u64 = 9_007_199_254_740_991;

/// Entry kinds representable by the canonical v0 archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveEntryKind {
    /// Regular file with exact content bytes.
    File,
    /// Explicit directory with no data bytes.
    Directory,
}

/// Borrowed canonical member metadata shared by producer and consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveMember<'a> {
    /// UTF-8 relative name, without a trailing slash.
    pub name: &'a str,
    /// File or directory.
    pub kind: ArchiveEntryKind,
    /// Exact file length; zero for directories.
    pub size: u64,
}

/// Validates the platform-independent canonical v0 metadata contract.
///
/// The list must be strictly byte-sorted and contain every parent directory.
/// Native Windows namespace admission is a separate guard-owned predicate.
///
/// # Errors
/// Returns [`PackError::InvalidMetadata`] for an unrepresentable or conflicting tree.
pub fn validate_v0_members(members: &[ArchiveMember<'_>]) -> Result<(), PackError> {
    let mut previous: Option<&str> = None;
    for member in members {
        let name = member.name;
        if name.len() > 100 || name.starts_with('/') || name.ends_with('/') {
            return Err(invalid(
                "entry name is absolute, trailing-slash, or too long",
            ));
        }
        if name
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | "..") || part.contains('\\'))
        {
            return Err(invalid("entry name has an invalid path component"));
        }
        if previous.is_some_and(|prior| prior.as_bytes() >= name.as_bytes()) {
            return Err(invalid("entry names are not strictly byte-sorted"));
        }
        previous = Some(name);
        if member.size > 0o77_777_777_777 {
            return Err(invalid("entry size does not fit canonical ustar"));
        }
        if member.kind == ArchiveEntryKind::Directory && member.size != 0 {
            return Err(invalid("directory size must be zero"));
        }
    }
    for member in members {
        let mut child = member.name;
        while let Some((parent, _)) = child.rsplit_once('/') {
            let found =
                members.binary_search_by(|entry| entry.name.as_bytes().cmp(parent.as_bytes()));
            match found.ok().map(|index| members[index].kind) {
                Some(ArchiveEntryKind::Directory) => child = parent,
                Some(ArchiveEntryKind::File) => {
                    return Err(invalid("a file is an ancestor of another entry"));
                }
                None => return Err(invalid("entry is missing an explicit parent directory")),
            }
        }
    }
    Ok(())
}

/// Borrowed package source. Readers begin at their current positions.
pub enum PackageEntry<'a> {
    /// Explicit application directory.
    Directory {
        /// UTF-8 relative package name.
        name: &'a str,
    },
    /// Regular file streamed without buffering the whole payload.
    File {
        /// UTF-8 relative package name.
        name: &'a str,
        /// Exact remaining source length; an extra EOF probe rejects surplus bytes.
        size: u64,
        /// Caller-owned reader, consumed only after metadata admission.
        input: &'a mut dyn Read,
    },
}

impl fmt::Debug for PackageEntry<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Directory { name } => f.debug_struct("Directory").field("name", name).finish(),
            Self::File { name, size, .. } => f
                .debug_struct("File")
                .field("name", name)
                .field("size", size)
                .finish_non_exhaustive(),
        }
    }
}

/// Sizes and digests of one successfully finished full-package output.
///
/// This proves emitted bytes, not durable filesystem storage or a source snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducedFull {
    compressed_size: u64,
    compressed_blake3: [u8; 32],
    content_size: u64,
    content_blake3: [u8; 32],
}

impl ProducedFull {
    /// Exact emitted zstd byte count.
    #[must_use]
    pub const fn compressed_size(&self) -> u64 {
        self.compressed_size
    }
    /// BLAKE3 of emitted compressed bytes.
    #[must_use]
    pub const fn compressed_blake3(&self) -> &[u8; 32] {
        &self.compressed_blake3
    }
    /// Exact canonical ustar byte count.
    #[must_use]
    pub const fn content_size(&self) -> u64 {
        self.content_size
    }
    /// BLAKE3 of the canonical ustar bytes before compression.
    #[must_use]
    pub const fn content_blake3(&self) -> &[u8; 32] {
        &self.content_blake3
    }
}

/// Produces a Windows x64 v0 update package, including the fixed no-migration policy.
///
/// All metadata and the augmented Windows namespace are checked before any source
/// read or sink write. Caller entries are not reordered. An exact `.keld` directory
/// may be supplied; the policy file itself is reserved to the producer.
///
/// # Errors
/// Unsupported hosts and invalid metadata perform no I/O. After streaming begins,
/// any error invalidates **all** output bytes: the caller must discard the sink.
/// A receipt is returned only after exact source lengths and zstd finalization pass.
pub fn produce_windows_v0<W: Write>(
    entries: &mut [PackageEntry<'_>],
    output: &mut W,
) -> Result<ProducedFull, PackError> {
    #[cfg(windows)]
    {
        producer::produce(entries, output)
    }
    #[cfg(not(windows))]
    {
        let _ = (entries, output);
        Err(PackError::UnsupportedHost)
    }
}

/// Typed package-production failures with actionable diagnostics.
#[derive(Debug)]
pub enum PackError {
    /// Native Windows namespace admission is unavailable on this producer host.
    UnsupportedHost,
    /// Input metadata cannot form a canonical package.
    InvalidMetadata {
        /// Stable reason without untrusted path bytes.
        detail: &'static str,
    },
    /// A reader ended early or supplied bytes beyond its declared length.
    SourceSizeMismatch {
        /// Admitted relative package name.
        name: String,
        /// Declared remaining source length.
        expected: u64,
        /// Observed count; at most one byte beyond the declaration is read.
        observed: u64,
    },
    /// Source, sink, or compression processing failed.
    Processing {
        /// Stage that failed.
        stage: &'static str,
        /// Original I/O error.
        source: io::Error,
    },
}

impl PackError {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedHost => "KELD-PACK-001",
            Self::InvalidMetadata { .. } => "KELD-PACK-002",
            Self::SourceSizeMismatch { .. } => "KELD-PACK-003",
            Self::Processing { .. } => "KELD-PACK-004",
        }
    }
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedHost => f.write_str("KELD-PACK-001: Windows v0 package production requires a Windows host. Build this package on Windows for native namespace admission; no output was written."),
            Self::InvalidMetadata { detail } => write!(f, "KELD-PACK-002: invalid Windows v0 package input ({detail}). Supply a complete file/directory tree with canonical Windows names and leave update-policy.v1 to the producer; no output was written."),
            Self::SourceSizeMismatch { name, expected, observed } => write!(f, "KELD-PACK-003: package source `{name}` declared {expected} bytes but supplied {observed}. Discard partial output and rebuild from sources with correct lengths."),
            Self::Processing { stage, source } => write!(f, "KELD-PACK-004: package {stage} failed ({source}). Discard partial output, repair the source or sink, and rebuild the package."),
        }
    }
}

impl std::error::Error for PackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Processing { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn invalid(detail: &'static str) -> PackError {
    PackError::InvalidMetadata { detail }
}

/// Installer formats keld-pack can author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// macOS application bundle.
    App,
    /// macOS disk image.
    Dmg,
    /// Windows NSIS installer.
    Nsis,
    /// Windows MSI installer.
    Msi,
    /// Debian package.
    Deb,
    /// RPM package.
    Rpm,
    /// Linux `AppImage`.
    AppImage,
}
