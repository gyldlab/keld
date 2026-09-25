use std::io::{Read, Seek, SeekFrom, Write};

use crate::error::{ArtifactDomain, UpdateError, hex_digest};
use crate::{ArtifactIdentity, SelectedFull};

const STREAM_BUFFER_BYTES: usize = 16 * 1024;

/// Receipt proving the selected full artifact passed both signed byte domains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFull {
    identity: ArtifactIdentity,
    content_size: u64,
    content_blake3: [u8; 32],
}

impl VerifiedFull {
    /// Exact selected release identity whose bytes passed verification.
    #[must_use]
    pub const fn identity(&self) -> &ArtifactIdentity {
        &self.identity
    }

    /// Exact decompressed canonical-content byte count.
    #[must_use]
    pub const fn content_size(&self) -> u64 {
        self.content_size
    }

    /// Computed BLAKE3 of the decompressed canonical-content bytes.
    #[must_use]
    pub const fn content_blake3(&self) -> &[u8; 32] {
        &self.content_blake3
    }
}

impl SelectedFull {
    /// Verifies downloaded full-package bytes and streams canonical content to `output`.
    ///
    /// The compressed input must be rewindable because transport size/BLAKE3 are checked
    /// completely before zstd decoding starts. Decompressed output is counted and hashed
    /// incrementally. On any error the caller MUST discard all bytes already written to
    /// `output`; only an [`VerifiedFull`] receipt makes that sink eligible for T3 archive
    /// validation.
    ///
    /// # Errors
    ///
    /// Returns a typed refusal on seek/read/write/decode failure, short or long compressed
    /// or decompressed data, or either digest mismatch.
    pub fn verify_full<R, W>(
        &self,
        compressed: &mut R,
        output: &mut W,
    ) -> Result<VerifiedFull, UpdateError>
    where
        R: Read + Seek,
        W: Write,
    {
        let start = compressed
            .stream_position()
            .map_err(|error| processing("input seek", error))?;
        hash_exact_compressed(compressed, self.compressed_size, &self.compressed_blake3)?;
        compressed
            .seek(SeekFrom::Start(start))
            .map_err(|error| processing("input rewind", error))?;

        // A generic `Read + Seek` source can change between the authenticated
        // transport pass above and this decode pass. Hash the exact bytes fed to
        // zstd as well, then compare them with the signed transport identity before
        // returning a receipt.
        let second_pass_limit = self
            .compressed_size
            .checked_add(1)
            .ok_or_else(|| processing("compressed byte limit", "u64 overflow"))?;
        let limited = compressed.take(second_pass_limit);
        let source_with_hash = CompressedHashReader::new(limited);
        let mut decoder = zstd::stream::read::Decoder::new(source_with_hash)
            .map_err(|error| processing("zstd decoder initialization", error))?;
        let mut hasher = blake3::Hasher::new();
        let mut produced = 0_u64;
        let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
        loop {
            let read = decoder
                .read(&mut buffer)
                .map_err(|error| processing("zstd decode", error))?;
            if read == 0 {
                break;
            }
            let next = produced
                .checked_add(read as u64)
                .ok_or_else(|| processing("content byte counter", "u64 overflow"))?;
            if next > self.identity_content_size() {
                return Err(size_mismatch(
                    ArtifactDomain::Content,
                    self.identity_content_size(),
                    format!("at least {next}"),
                ));
            }
            output
                .write_all(&buffer[..read])
                .map_err(|error| processing("verified-content sink write", error))?;
            hasher.update(&buffer[..read]);
            produced = next;
        }
        if produced != self.identity_content_size() {
            return Err(size_mismatch(
                ArtifactDomain::Content,
                self.identity_content_size(),
                produced.to_string(),
            ));
        }
        let actual = *hasher.finalize().as_bytes();
        if actual != self.identity.content_blake3 {
            return Err(digest_mismatch(
                ArtifactDomain::Content,
                &self.identity.content_blake3,
                &actual,
            ));
        }

        // `Decoder::read` can hand out the last content bytes before a frame
        // epilogue is complete. Finish the frame, then require that the decoder
        // consumed every buffered and underlying byte. Do not raw-drain after a
        // decoder EOF: a generic `Read` may return zero and later resume, and those
        // later bytes were never authenticated by zstd.
        decoder
            .finish_frame()
            .map_err(|error| processing("zstd frame completion", error))?;
        let compressed_reader = decoder.finish();
        let buffered = compressed_reader.buffer().len() as u64;
        let compressed_input = compressed_reader.get_ref();
        let consumed = compressed_input
            .bytes_read
            .checked_sub(buffered)
            .ok_or_else(|| processing("compressed byte counter", "buffer exceeds bytes read"))?;
        if buffered != 0 || compressed_input.bytes_read != self.compressed_size {
            return Err(size_mismatch(
                ArtifactDomain::Compressed,
                self.compressed_size,
                format!("{consumed} consumed; {buffered} buffered"),
            ));
        }
        let actual_compressed = *compressed_input.hasher.finalize().as_bytes();
        if actual_compressed != self.compressed_blake3 {
            return Err(digest_mismatch(
                ArtifactDomain::Compressed,
                &self.compressed_blake3,
                &actual_compressed,
            ));
        }

        Ok(VerifiedFull {
            identity: self.identity.clone(),
            content_size: produced,
            content_blake3: actual,
        })
    }

    const fn identity_content_size(&self) -> u64 {
        // `contentSize` is signed metadata distinct from the artifact identity. It is
        // kept on `SelectedFull` below; this helper gives the streaming loop one owner.
        self.content_size
    }
}

struct CompressedHashReader<R> {
    inner: R,
    hasher: blake3::Hasher,
    bytes_read: u64,
}

impl<R> CompressedHashReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: blake3::Hasher::new(),
            bytes_read: 0,
        }
    }
}

impl<R: Read> Read for CompressedHashReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        if read != 0 {
            self.bytes_read = self
                .bytes_read
                .checked_add(read as u64)
                .ok_or_else(|| std::io::Error::other("compressed byte count overflow"))?;
            self.hasher.update(&buffer[..read]);
        }
        Ok(read)
    }
}

fn hash_exact_compressed<R: Read>(
    input: &mut R,
    expected_size: u64,
    expected_digest: &[u8; 32],
) -> Result<(), UpdateError> {
    let mut hasher = blake3::Hasher::new();
    let mut observed = 0_u64;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    while observed < expected_size {
        let remaining = expected_size - observed;
        let limit = usize::try_from(remaining.min(STREAM_BUFFER_BYTES as u64))
            .map_err(|error| processing("compressed byte counter", error))?;
        let read = input
            .read(&mut buffer[..limit])
            .map_err(|error| processing("compressed input read", error))?;
        if read == 0 {
            return Err(size_mismatch(
                ArtifactDomain::Compressed,
                expected_size,
                observed.to_string(),
            ));
        }
        hasher.update(&buffer[..read]);
        observed += read as u64;
    }

    let mut extra = [0_u8; 1];
    let extra_read = input
        .read(&mut extra)
        .map_err(|error| processing("compressed input boundary read", error))?;
    if extra_read != 0 {
        return Err(size_mismatch(
            ArtifactDomain::Compressed,
            expected_size,
            format!("at least {}", expected_size + 1),
        ));
    }

    let actual = *hasher.finalize().as_bytes();
    if &actual != expected_digest {
        return Err(digest_mismatch(
            ArtifactDomain::Compressed,
            expected_digest,
            &actual,
        ));
    }
    Ok(())
}

fn size_mismatch(domain: ArtifactDomain, expected: u64, observed: String) -> UpdateError {
    UpdateError::ArtifactSizeMismatch {
        domain,
        expected,
        observed,
    }
}

fn digest_mismatch(domain: ArtifactDomain, expected: &[u8; 32], actual: &[u8; 32]) -> UpdateError {
    UpdateError::ArtifactDigestMismatch {
        domain,
        expected: hex_digest(expected),
        actual: hex_digest(actual),
    }
}

fn processing(stage: &'static str, error: impl std::fmt::Display) -> UpdateError {
    UpdateError::ArtifactProcessing {
        stage,
        detail: error.to_string(),
    }
}

/// Raw-byte fuzzer hook for the canonical archive parser; never enabled in product builds.
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub fn fuzz_canonical_archive(bytes: &[u8]) {
    let Ok(content_size) = u64::try_from(bytes.len()) else {
        return;
    };
    let digest = *blake3::hash(bytes).as_bytes();
    let receipt = VerifiedFull {
        identity: crate::ArtifactIdentity {
            app_id: "fuzz.invalid".to_owned(),
            channel: crate::Channel::Stable,
            target: "windows-x64".to_owned(),
            version: "0.0.0".to_owned(),
            content_blake3: digest,
        },
        content_size,
        content_blake3: digest,
    };
    let mut cursor = std::io::Cursor::new(bytes);
    let _ = crate::archive::parse_canonical_ustar(&receipt, &mut cursor, |paths| {
        for path in paths {
            for component in path.split('/') {
                keld_guard::validate_windows_package_component(component)?;
            }
        }
        Ok(())
    });
}
