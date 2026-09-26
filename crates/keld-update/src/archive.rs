use std::io::{self, Read, Seek, SeekFrom};

pub use keld_pack::ArchiveEntryKind;
use keld_pack::{
    ARCHIVE_BLOCK_BYTES as TAR_BLOCK_BYTES, ArchiveMember, NO_MIGRATION_POLICY, UPDATE_POLICY_PATH,
};

use crate::error::{ArtifactDomain, UpdateError, hex_digest};
use crate::{ArtifactIdentity, VerifiedFull};

const STREAM_BUFFER_BYTES: usize = 16 * 1024;

/// One validated archive member, with its data offset relative to archive start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    name: String,
    kind: ArchiveEntryKind,
    size: u64,
    data_offset: u64,
}

impl ArchiveEntry {
    /// UTF-8 relative package path using forward slashes.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether this member is a file or directory.
    #[must_use]
    pub const fn kind(&self) -> ArchiveEntryKind {
        self.kind
    }

    /// File byte length; always zero for a directory.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Data start relative to the start of the canonical archive.
    #[must_use]
    pub const fn data_offset(&self) -> u64 {
        self.data_offset
    }
}

/// Receipt that the complete canonical Windows v0 archive passed preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedArchive {
    identity: ArtifactIdentity,
    content_size: u64,
    content_blake3: [u8; 32],
    entries: Vec<ArchiveEntry>,
}

impl ValidatedArchive {
    /// Signed release identity whose content was validated.
    #[must_use]
    pub const fn identity(&self) -> &ArtifactIdentity {
        &self.identity
    }

    /// Exact canonical archive byte count.
    #[must_use]
    pub const fn content_size(&self) -> u64 {
        self.content_size
    }

    /// BLAKE3 of the exact archive bytes read during preflight.
    #[must_use]
    pub const fn content_blake3(&self) -> &[u8; 32] {
        &self.content_blake3
    }

    /// Entries in the canonical byte-sorted order required by v0.
    #[must_use]
    pub fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }
}

impl VerifiedFull {
    /// Validates the exact Windows v0 archive represented by this full-artifact receipt.
    ///
    /// This performs a complete read-only preflight. It does not extract files or
    /// establish filesystem containment, staging protection, or install ACLs.
    /// The archive reader is consumed from its current position; its seek-reported
    /// remaining length must exactly match the signed decompressed byte count.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal for invalid archive bytes, a changed/short/long
    /// content stream, or an underlying read failure.
    #[cfg(windows)]
    pub fn validate_windows_archive<R: Read + Seek>(
        &self,
        archive: &mut R,
    ) -> Result<ValidatedArchive, UpdateError> {
        parse_canonical_ustar(self, archive, keld_guard::validate_windows_package_paths)
    }
}

pub(crate) fn parse_canonical_ustar<R, V>(
    receipt: &VerifiedFull,
    archive: &mut R,
    validate_windows_paths: V,
) -> Result<ValidatedArchive, UpdateError>
where
    R: Read + Seek,
    V: FnOnce(&[&str]) -> Result<(), String>,
{
    let start = archive
        .stream_position()
        .map_err(|error| processing("canonical archive position", error))?;
    let end = archive
        .seek(SeekFrom::End(0))
        .map_err(|error| processing("canonical archive length", error))?;
    let remaining = end
        .checked_sub(start)
        .ok_or_else(|| processing("canonical archive length", "end precedes input position"))?;
    if remaining != receipt.content_size() {
        return Err(size_mismatch(receipt.content_size(), remaining.to_string()));
    }
    archive
        .seek(SeekFrom::Start(start))
        .map_err(|error| processing("canonical archive rewind", error))?;
    let mut input = HashingReader::new(archive, receipt.content_size())?;
    let mut entries = Vec::<ArchiveEntry>::new();
    let mut policy_seen = false;
    let mut policy_valid = false;
    loop {
        let mut header = [0_u8; TAR_BLOCK_BYTES];
        read_exact_canonical(&mut input, &mut header, receipt.content_size())?;
        if header.iter().all(|byte| *byte == 0) {
            let mut second = [0_u8; TAR_BLOCK_BYTES];
            read_exact_canonical(&mut input, &mut second, receipt.content_size())?;
            if second.iter().any(|byte| *byte != 0) {
                return Err(invalid_archive("archive must end with two zero blocks"));
            }
            let mut trailing = [0_u8; 1];
            match input.read(&mut trailing) {
                Ok(0) => {}
                Ok(_) if input.bytes_read > receipt.content_size() => {
                    return Err(size_mismatch(
                        receipt.content_size(),
                        format!("at least {}", input.bytes_read),
                    ));
                }
                Ok(_) => return Err(invalid_archive("archive has trailing bytes")),
                Err(error) => return Err(processing("canonical archive read", error)),
            }
            break;
        }

        verify_header_checksum(&header)?;
        verify_fixed_fields(&header)?;
        let (name, kind, size) = parse_header(&header)?;
        if entries.try_reserve(1).is_err() {
            return Err(invalid_archive(
                "archive entry table exceeds available memory",
            ));
        }
        let data_offset = input.bytes_read;
        if name == UPDATE_POLICY_PATH {
            policy_seen = true;
            policy_valid = read_policy(&mut input, kind, size, receipt.content_size())?;
        } else {
            skip_data(&mut input, size, receipt.content_size())?;
        }
        let padding =
            (TAR_BLOCK_BYTES as u64 - (size % TAR_BLOCK_BYTES as u64)) % TAR_BLOCK_BYTES as u64;
        skip_zero_padding(&mut input, padding, receipt.content_size())?;
        entries.push(ArchiveEntry {
            name,
            kind,
            size,
            data_offset,
        });
    }

    if input.bytes_read != receipt.content_size() {
        return Err(size_mismatch(
            receipt.content_size(),
            input.bytes_read.to_string(),
        ));
    }
    validate_archive_members(&entries, validate_windows_paths)?;

    let actual_digest = *input.hasher.finalize().as_bytes();
    if actual_digest != *receipt.content_blake3() {
        return Err(UpdateError::ArtifactDigestMismatch {
            domain: ArtifactDomain::Content,
            expected: hex_digest(receipt.content_blake3()),
            actual: hex_digest(&actual_digest),
        });
    }
    // Decide policy only after authenticating the entire byte stream. A changed
    // input must remain a digest failure rather than masquerading as signed policy.
    if !policy_seen {
        return Err(invalid_archive("required no-migration policy is missing"));
    }
    if !policy_valid {
        return Err(invalid_archive(
            "no-migration policy is not the exact required regular file",
        ));
    }
    Ok(ValidatedArchive {
        identity: receipt.identity().clone(),
        content_size: receipt.content_size(),
        content_blake3: actual_digest,
        entries,
    })
}

fn read_policy<R: Read>(
    input: &mut HashingReader<'_, R>,
    kind: ArchiveEntryKind,
    size: u64,
    expected: u64,
) -> Result<bool, UpdateError> {
    if kind != ArchiveEntryKind::File || size != NO_MIGRATION_POLICY.len() as u64 {
        skip_data(input, size, expected)?;
        return Ok(false);
    }
    let mut bytes = [0_u8; NO_MIGRATION_POLICY.len()];
    read_exact_canonical(input, &mut bytes, expected)?;
    Ok(bytes == NO_MIGRATION_POLICY)
}

fn validate_archive_members<V>(
    entries: &[ArchiveEntry],
    validate_windows_paths: V,
) -> Result<(), UpdateError>
where
    V: FnOnce(&[&str]) -> Result<(), String>,
{
    let mut members = Vec::new();
    members
        .try_reserve_exact(entries.len())
        .map_err(|_| invalid_archive("archive metadata allocation failed"))?;
    members.extend(entries.iter().map(|entry| ArchiveMember {
        name: &entry.name,
        kind: entry.kind,
        size: entry.size,
    }));
    keld_pack::validate_v0_members(&members).map_err(|error| match error {
        keld_pack::PackError::InvalidMetadata { detail } => invalid_archive(detail),
        _ => invalid_archive("canonical archive metadata is invalid"),
    })?;
    let mut path_refs = Vec::new();
    path_refs
        .try_reserve_exact(entries.len())
        .map_err(|_| invalid_archive("archive path table allocation failed"))?;
    path_refs.extend(entries.iter().map(|entry| entry.name.as_str()));
    validate_windows_paths(&path_refs)
        .map_err(|_| invalid_archive("Windows package namespace is invalid"))
}

struct HashingReader<'a, R> {
    input: io::Take<&'a mut R>,
    hasher: blake3::Hasher,
    bytes_read: u64,
}

impl<'a, R: Read> HashingReader<'a, R> {
    fn new(input: &'a mut R, expected: u64) -> Result<Self, UpdateError> {
        let limit = expected
            .checked_add(1)
            .ok_or_else(|| processing("canonical archive bound", "byte limit overflow"))?;
        Ok(Self {
            input: input.take(limit),
            hasher: blake3::Hasher::new(),
            bytes_read: 0,
        })
    }
}

impl<R: Read> Read for HashingReader<'_, R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let read = self.input.read(output)?;
        self.bytes_read = self
            .bytes_read
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::other("canonical archive byte count overflow"))?;
        self.hasher.update(&output[..read]);
        Ok(read)
    }
}

fn read_exact_canonical<R: Read>(
    input: &mut HashingReader<'_, R>,
    output: &mut [u8],
    expected: u64,
) -> Result<(), UpdateError> {
    match input.read_exact(output) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            if input.bytes_read == expected {
                Err(invalid_archive("archive ended before both terminal blocks"))
            } else {
                let observed = if input.bytes_read > expected {
                    format!("at least {}", input.bytes_read)
                } else {
                    input.bytes_read.to_string()
                };
                Err(size_mismatch(expected, observed))
            }
        }
        Err(error) => Err(processing("canonical archive read", error)),
    }
}

fn parse_header(
    header: &[u8; TAR_BLOCK_BYTES],
) -> Result<(String, ArchiveEntryKind, u64), UpdateError> {
    let name_field = &header[..100];
    let name_len = name_field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name_field.len());
    if name_len == 0 || name_field[name_len..].iter().any(|byte| *byte != 0) {
        return Err(invalid_archive(
            "name field is empty or has nonzero tail bytes",
        ));
    }
    let name_text = std::str::from_utf8(&name_field[..name_len])
        .map_err(|_| invalid_archive("entry name is not UTF-8"))?;
    let mut name = String::new();
    name.try_reserve_exact(name_text.len())
        .map_err(|_| invalid_archive("archive entry name allocation failed"))?;
    name.push_str(name_text);

    let kind = match header[156] {
        b'0' => ArchiveEntryKind::File,
        b'5' => ArchiveEntryKind::Directory,
        _ => {
            return Err(invalid_archive(
                "entry type is not a regular file or directory",
            ));
        }
    };
    let size = parse_octal_field(&header[124..136])
        .ok_or_else(|| invalid_archive("size field is not canonical octal"))?;
    let mtime = parse_octal_field(&header[136..148])
        .ok_or_else(|| invalid_archive("mtime field is not canonical octal"))?;
    if mtime != 0 {
        return Err(invalid_archive("mtime must be zero"));
    }
    if kind == ArchiveEntryKind::Directory && size != 0 {
        return Err(invalid_archive("directory size must be zero"));
    }
    Ok((name, kind, size))
}

fn verify_fixed_fields(header: &[u8; TAR_BLOCK_BYTES]) -> Result<(), UpdateError> {
    let expected_mode = match header[156] {
        b'0' => b"0000644\0".as_slice(),
        b'5' => b"0000755\0".as_slice(),
        _ => {
            return Err(invalid_archive(
                "entry type is not a regular file or directory",
            ));
        }
    };
    if &header[100..108] != expected_mode
        || &header[108..116] != b"0000000\0"
        || &header[116..124] != b"0000000\0"
        || &header[257..263] != b"ustar\0"
        || &header[263..265] != b"00"
        || !all_zero(&header[157..257])
        || !all_zero(&header[265..297])
        || !all_zero(&header[297..329])
        || &header[329..337] != b"0000000\0"
        || &header[337..345] != b"0000000\0"
        || !all_zero(&header[345..500])
        || !all_zero(&header[500..512])
    {
        return Err(invalid_archive("header fields are not canonical v0 values"));
    }
    Ok(())
}

fn verify_header_checksum(header: &[u8; TAR_BLOCK_BYTES]) -> Result<(), UpdateError> {
    let sum = header.iter().enumerate().fold(0_u64, |sum, (index, byte)| {
        sum + if (148..156).contains(&index) {
            u64::from(b' ')
        } else {
            u64::from(*byte)
        }
    });
    let mut canonical = [0_u8; 8];
    let mut remaining = sum;
    for digit in canonical[..6].iter_mut().rev() {
        *digit = b'0' + (remaining % 8) as u8;
        remaining /= 8;
    }
    canonical[6] = 0;
    canonical[7] = b' ';
    if remaining != 0 || header[148..156] != canonical {
        return Err(invalid_archive("header checksum is not canonical"));
    }
    Ok(())
}

fn parse_octal_field(field: &[u8]) -> Option<u64> {
    if field.len() != 12 || field[11] != 0 {
        return None;
    }
    field[..11].iter().try_fold(0_u64, |value, byte| {
        if !(b'0'..=b'7').contains(byte) {
            return None;
        }
        value.checked_mul(8)?.checked_add(u64::from(*byte - b'0'))
    })
}

fn skip_data<R: Read>(
    input: &mut HashingReader<'_, R>,
    size: u64,
    expected: u64,
) -> Result<(), UpdateError> {
    let mut remaining = size;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    while remaining != 0 {
        let length = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| invalid_archive("entry size exceeds addressable buffer length"))?;
        read_exact_canonical(input, &mut buffer[..length], expected)?;
        remaining -= length as u64;
    }
    Ok(())
}

fn skip_zero_padding<R: Read>(
    input: &mut HashingReader<'_, R>,
    length: u64,
    expected: u64,
) -> Result<(), UpdateError> {
    let mut remaining = length;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    while remaining != 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| invalid_archive("padding exceeds addressable buffer length"))?;
        read_exact_canonical(input, &mut buffer[..count], expected)?;
        if !all_zero(&buffer[..count]) {
            return Err(invalid_archive("entry data padding is nonzero"));
        }
        remaining -= count as u64;
    }
    Ok(())
}

fn all_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

fn size_mismatch(expected: u64, observed: String) -> UpdateError {
    UpdateError::ArtifactSizeMismatch {
        domain: ArtifactDomain::Content,
        expected,
        observed,
    }
}

fn invalid_archive(detail: &'static str) -> UpdateError {
    UpdateError::ArchiveInvalid { detail }
}

fn processing(stage: &'static str, error: impl std::fmt::Display) -> UpdateError {
    UpdateError::ArtifactProcessing {
        stage,
        detail: error.to_string(),
    }
}
