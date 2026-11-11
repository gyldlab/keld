//! Shared bounded file verification and content types for retained native sources.

use super::{ResourceSourceError, SelectedRead, VerifiedBytes};
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::io::Read as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

// The worker's existing admitted result allocation is the read buffer. The
// one-byte stack probe detects growth without reserving any extra retained byte.
pub(crate) fn read_verified_file(
    mut file: File,
    request: &SelectedRead,
    checked_size: usize,
    max_asset_bytes: usize,
    cancelled: &AtomicBool,
) -> Result<VerifiedBytes, ResourceSourceError> {
    let mut result = vec![0_u8; checked_size];
    let mut digest = Sha256::new();
    let mut read_bytes = 0_usize;
    while read_bytes < checked_size {
        // Regular-file reads are not preemptible; the same worker owns this
        // descriptor/buffer until the call returns and cancellation is observed.
        if cancelled.load(Ordering::Acquire) {
            return Err(ResourceSourceError::Cancelled);
        }
        let end = read_bytes
            .checked_add((64 * 1024).min(checked_size - read_bytes))
            .ok_or(ResourceSourceError::SourceChanged)?;
        let count = file
            .read(&mut result[read_bytes..end])
            .map_err(|_| ResourceSourceError::Io)?;
        if count == 0 {
            break;
        }
        let next = read_bytes
            .checked_add(count)
            .ok_or(ResourceSourceError::SourceChanged)?;
        digest.update(&result[read_bytes..next]);
        read_bytes = next;
    }
    result.truncate(read_bytes);
    if cancelled.load(Ordering::Acquire) {
        return Err(ResourceSourceError::Cancelled);
    }
    if read_bytes == checked_size {
        let mut overrun = [0_u8; 1];
        if file
            .read(&mut overrun)
            .map_err(|_| ResourceSourceError::Io)?
            != 0
        {
            return Err(ResourceSourceError::SourceChanged);
        }
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(ResourceSourceError::Cancelled);
    }
    let actual_sha256: [u8; 32] = digest.finalize().into();
    if result.len() as u64 != request.size
        || result.len() > max_asset_bytes
        || actual_sha256 != request.sha256
    {
        return Err(ResourceSourceError::SourceChanged);
    }
    Ok(VerifiedBytes {
        bytes: result,
        mime: request.mime,
    })
}

pub(crate) fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "ttf" => "font/ttf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        "txt" | "md" | "properties" | "mf" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        "jar" => "application/java-archive",
        _ => "application/octet-stream",
    }
}
