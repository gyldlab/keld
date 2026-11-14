//! Fixed instrumented first-inline policy for one exact admitted fixture image.
//! No caller supplies a header and no shipping build compiles this module.
use crate::preload_image::AppPreloadImage;
use crate::resource_source::{SelectedRead, VerifiedBytes};
use sha2::{Digest as _, Sha256};

const ORIGINAL_PRELOAD_SHA256: [u8; 32] = [
    0x10, 0x91, 0xfd, 0x21, 0xef, 0x72, 0x7a, 0x52, 0xef, 0x89, 0xbd, 0xe8, 0x14, 0xaa, 0x66, 0x2e,
    0xed, 0x4a, 0x0c, 0x3c, 0xe3, 0x2b, 0xeb, 0x85, 0x8a, 0x06, 0x8a, 0x96, 0xb2, 0x85, 0xbf, 0x98,
];
const INLINE_DOCUMENT_SHA256: [u8; 32] = [
    0x32, 0x3e, 0xef, 0x85, 0xc2, 0xae, 0xbc, 0x08, 0x34, 0xe1, 0xad, 0xda, 0xd4, 0xe3, 0x8d, 0x43,
    0xe0, 0xf6, 0x87, 0x11, 0x62, 0xe0, 0x53, 0xd1, 0xd2, 0xd1, 0x4a, 0x52, 0xe3, 0xd2, 0x10, 0xf9,
];
const INLINE_DOCUMENT_BYTES: usize = 7647;
#[cfg(test)]
const INLINE_SCRIPT_SHA256: [u8; 32] = [
    0xe4, 0xdf, 0xe6, 0xa7, 0xba, 0x25, 0x7b, 0x0a, 0xf8, 0x86, 0x66, 0x0e, 0x50, 0xbb, 0x88, 0xbd,
    0xbb, 0x9d, 0x72, 0x86, 0xdd, 0xe2, 0x9a, 0xe9, 0xee, 0x96, 0x61, 0x84, 0x9e, 0x1a, 0xb7, 0x8f,
];
#[cfg(test)]
const SCRIPT_SOURCE_TOKEN: &str = " 'sha256-5N/mp7olewr4hmYOULuIvbudcobd4prp7pZhhJ4at48='";
const BASE_POLICY: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='; font-src 'self' data:; media-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'";
const INLINE_POLICY: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval' 'sha256-5N/mp7olewr4hmYOULuIvbudcobd4prp7pZhhJ4at48='; connect-src 'self'; img-src 'self' data:; style-src 'self' 'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='; font-src 'self' data:; media-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'";

pub(super) struct InlineFixture {
    original_image: bool,
}

impl InlineFixture {
    /// The complete `OriginalView` lends its actual immutable qualified image.
    pub(super) fn capture(image: &AppPreloadImage) -> Self {
        Self {
            original_image: <[u8; 32]>::from(Sha256::digest(image.source().as_bytes()))
                == ORIGINAL_PRELOAD_SHA256,
        }
    }

    /// Cover the possible header before request capture, from the same account.
    pub(super) fn maximum_header_len(&self, ordinary: usize) -> usize {
        if self.original_image {
            ordinary.max(INLINE_POLICY.len())
        } else {
            ordinary
        }
    }

    /// Called only for the same live View/Pending after real verified-file I/O.
    /// Returning the ordinary policy on any mismatch never grants a document.
    pub(super) fn select<'a>(
        &self,
        ordinary: &'a str,
        selected: &SelectedRead,
        body: &VerifiedBytes,
    ) -> &'a str {
        if self.original_image
            && ordinary == BASE_POLICY
            && selected.sha256 == INLINE_DOCUMENT_SHA256
            && selected.size == INLINE_DOCUMENT_BYTES as u64
            && body.bytes.len() == INLINE_DOCUMENT_BYTES
            && selected.mime == "text/html; charset=utf-8"
            && body.mime == selected.mime
            && <[u8; 32]>::from(Sha256::digest(&body.bytes)) == INLINE_DOCUMENT_SHA256
        {
            INLINE_POLICY
        } else {
            ordinary
        }
    }
}

#[cfg(test)]
mod tests;
