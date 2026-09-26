use std::io::{self, Read, Write};

use crate::{
    ARCHIVE_BLOCK_BYTES, ArchiveEntryKind, ArchiveMember, MAX_ARTIFACT_BYTES, NO_MIGRATION_POLICY,
    PackError, PackageEntry, ProducedFull, UPDATE_POLICY_PATH, invalid, validate_v0_members,
};

const BLOCK_SIZE: u64 = ARCHIVE_BLOCK_BYTES as u64;
const POLICY_PARENT: &str = ".keld";

pub(super) fn produce<W: Write>(
    entries: &mut [PackageEntry<'_>],
    output: &mut W,
) -> Result<ProducedFull, PackError> {
    let (plan, content_size) = plan(entries)?;
    // Encoder creation can write to its sink. Every metadata/namespace predicate
    // has therefore already passed before the encoder takes the caller's writer.
    let compressed = HashingWriter::new(output);
    let encoder = zstd::stream::write::Encoder::new(compressed, 0)
        .map_err(|source| processing("zstd initialization", source))?;
    let mut content = HashingWriter::new(encoder);
    let zeroes = [0_u8; ARCHIVE_BLOCK_BYTES];
    for (member, source_index) in plan {
        content
            .write_all(&header(member))
            .map_err(|source| processing("archive header write", source))?;
        if member.kind == ArchiveEntryKind::File {
            if let Some(index) = source_index {
                let Some(PackageEntry::File { input, .. }) = entries.get_mut(index) else {
                    return Err(processing(
                        "source lookup",
                        io::Error::other("source plan changed"),
                    ));
                };
                copy_exact(*input, member, &mut content)?;
            } else {
                content
                    .write_all(NO_MIGRATION_POLICY)
                    .map_err(|source| processing("policy write", source))?;
            }
            let padding = padding(member.size);
            let count = usize::try_from(padding).map_err(|_| {
                processing(
                    "padding size",
                    io::Error::other("padding exceeds block size"),
                )
            })?;
            content
                .write_all(&zeroes[..count])
                .map_err(|source| processing("archive padding write", source))?;
        }
    }
    content
        .write_all(&zeroes)
        .and_then(|()| content.write_all(&zeroes))
        .map_err(|source| processing("archive terminator write", source))?;
    if content.count != content_size {
        return Err(processing(
            "archive size",
            io::Error::other("canonical byte count mismatch"),
        ));
    }
    let content_blake3 = *content.hasher.finalize().as_bytes();
    let compressed = content
        .inner
        .finish()
        .map_err(|source| processing("zstd finalization", source))?;
    Ok(ProducedFull {
        compressed_size: compressed.count,
        compressed_blake3: *compressed.hasher.finalize().as_bytes(),
        content_size,
        content_blake3,
    })
}

type PlannedMember<'a> = (ArchiveMember<'a>, Option<usize>);

fn plan<'a>(entries: &[PackageEntry<'a>]) -> Result<(Vec<PlannedMember<'a>>, u64), PackError> {
    let capacity = entries
        .len()
        .checked_add(2)
        .ok_or_else(|| invalid("too many package entries"))?;
    let mut plan = Vec::new();
    plan.try_reserve_exact(capacity)
        .map_err(|_| invalid("package metadata allocation failed"))?;
    let mut has_policy_parent = false;
    for (index, entry) in entries.iter().enumerate() {
        let member = match entry {
            PackageEntry::Directory { name } => ArchiveMember {
                name,
                kind: ArchiveEntryKind::Directory,
                size: 0,
            },
            PackageEntry::File { name, size, .. } => ArchiveMember {
                name,
                kind: ArchiveEntryKind::File,
                size: *size,
            },
        };
        if member.name == UPDATE_POLICY_PATH {
            return Err(invalid("update policy is reserved to the producer"));
        }
        if member.name == POLICY_PARENT {
            if member.kind != ArchiveEntryKind::Directory {
                return Err(invalid("update policy parent must be a directory"));
            }
            has_policy_parent = true;
        }
        plan.push((member, Some(index)));
    }
    if !has_policy_parent {
        plan.push((
            ArchiveMember {
                name: POLICY_PARENT,
                kind: ArchiveEntryKind::Directory,
                size: 0,
            },
            None,
        ));
    }
    plan.push((
        ArchiveMember {
            name: UPDATE_POLICY_PATH,
            kind: ArchiveEntryKind::File,
            size: NO_MIGRATION_POLICY.len() as u64,
        },
        None,
    ));
    plan.sort_unstable_by(|left, right| left.0.name.as_bytes().cmp(right.0.name.as_bytes()));
    let mut members = Vec::new();
    members
        .try_reserve_exact(plan.len())
        .map_err(|_| invalid("package metadata allocation failed"))?;
    members.extend(plan.iter().map(|(member, _)| *member));
    validate_v0_members(&members)?;
    let mut paths = Vec::new();
    paths
        .try_reserve_exact(members.len())
        .map_err(|_| invalid("package path allocation failed"))?;
    paths.extend(members.iter().map(|member| member.name));
    keld_guard::validate_windows_package_paths(&paths)
        .map_err(|_| invalid("Windows package namespace is invalid"))?;
    let size = members.iter().try_fold(BLOCK_SIZE * 2, |total, member| {
        total
            .checked_add(BLOCK_SIZE)
            .and_then(|n| n.checked_add(member.size))
            .and_then(|n| n.checked_add(padding(member.size)))
            .filter(|n| *n <= MAX_ARTIFACT_BYTES)
            .ok_or_else(|| invalid("canonical archive exceeds the signed size domain"))
    })?;
    Ok((plan, size))
}

const fn padding(size: u64) -> u64 {
    (BLOCK_SIZE - size % BLOCK_SIZE) % BLOCK_SIZE
}

fn header(member: ArchiveMember<'_>) -> [u8; ARCHIVE_BLOCK_BYTES] {
    let mut bytes = [0_u8; ARCHIVE_BLOCK_BYTES];
    bytes[..member.name.len()].copy_from_slice(member.name.as_bytes());
    bytes[100..108].copy_from_slice(match member.kind {
        ArchiveEntryKind::File => b"0000644\0",
        ArchiveEntryKind::Directory => b"0000755\0",
    });
    for field in [108..116, 116..124, 329..337, 337..345] {
        bytes[field].copy_from_slice(b"0000000\0");
    }
    octal(member.size, &mut bytes[124..136]);
    octal(0, &mut bytes[136..148]);
    bytes[148..156].fill(b' ');
    bytes[156] = match member.kind {
        ArchiveEntryKind::File => b'0',
        ArchiveEntryKind::Directory => b'5',
    };
    bytes[257..263].copy_from_slice(b"ustar\0");
    bytes[263..265].copy_from_slice(b"00");
    let sum = bytes.iter().map(|byte| u64::from(*byte)).sum();
    octal(sum, &mut bytes[148..155]);
    bytes[155] = b' ';
    bytes
}

fn octal(mut value: u64, output: &mut [u8]) {
    let digits = output.len() - 1;
    for byte in output[..digits].iter_mut().rev() {
        *byte = b'0' + (value.to_le_bytes()[0] & 7);
        value >>= 3;
    }
    output[digits] = 0;
}

fn copy_exact<W: Write>(
    input: &mut dyn Read,
    member: ArchiveMember<'_>,
    output: &mut W,
) -> Result<(), PackError> {
    let mut observed = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    while observed < member.size {
        let count =
            usize::try_from((member.size - observed).min(buffer.len() as u64)).map_err(|_| {
                processing(
                    "source bound",
                    io::Error::other("buffer length is unrepresentable"),
                )
            })?;
        let read = read_source(input, &mut buffer[..count])?;
        if read == 0 {
            return Err(source_size(member, observed));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|source| processing("archive data write", source))?;
        observed += read as u64;
    }
    let mut extra = [0_u8; 1];
    if read_source(input, &mut extra)? != 0 {
        return Err(source_size(member, observed + 1));
    }
    Ok(())
}

fn read_source(input: &mut dyn Read, buffer: &mut [u8]) -> Result<usize, PackError> {
    loop {
        match input.read(buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(source) => return Err(processing("source read", source)),
            Ok(read) if read <= buffer.len() => return Ok(read),
            Ok(_) => {
                return Err(processing(
                    "source read",
                    io::Error::other("reader exceeded the supplied buffer"),
                ));
            }
        }
    }
}

fn source_size(member: ArchiveMember<'_>, observed: u64) -> PackError {
    PackError::SourceSizeMismatch {
        name: member.name.to_owned(),
        expected: member.size,
        observed,
    }
}

fn processing(stage: &'static str, source: io::Error) -> PackError {
    PackError::Processing { stage, source }
}

struct HashingWriter<W> {
    inner: W,
    count: u64,
    hasher: blake3::Hasher,
}

impl<W> HashingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            count: 0,
            hasher: blake3::Hasher::new(),
        }
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .count
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= MAX_ARTIFACT_BYTES)
            .ok_or_else(|| io::Error::other("artifact exceeds the signed size domain"))?;
        let written = self.inner.write(bytes)?;
        if written > bytes.len() {
            return Err(io::Error::other("writer exceeded the supplied buffer"));
        }
        self.count = next - (bytes.len() - written) as u64;
        self.hasher.update(&bytes[..written]);
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
