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

        let limited = compressed.take(self.compressed_size);
        let mut decoder = zstd::stream::read::Decoder::new(limited)
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
