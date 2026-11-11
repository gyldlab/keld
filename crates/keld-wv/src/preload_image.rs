//! One Host-accounted immutable preload image shared by native backends.
//! Core owns graph admission; this owner only checks/retains the selected bytes.

use crate::error::WvError;
use sha2::{Digest as _, Sha256};
use std::sync::{Arc, Mutex};

/// Maximum source bytes admitted for one window before UTF-8 parsing.
pub const MAX_APP_PRELOAD_BYTES: usize = 512 * 1024;
const MAX_HOST_PRELOAD_BYTES: usize = 8 * 1024 * 1024;

/// One Host session's source-image retention account; share across all windows.
#[derive(Clone, Debug, Default)]
pub struct AppPreloadBudget(Arc<Mutex<usize>>);

impl AppPreloadBudget {
    /// Reserve the full per-window cap before opening/copying any source bytes.
    /// The successful image shrinks this reservation to its actual size.
    ///
    /// # Errors
    /// Refuses Host aggregate exhaustion; no bytes or descriptors are retained.
    pub fn reserve_window(&self) -> Result<AppPreloadReservation, WvError> {
        let mut used = self
            .0
            .lock()
            .map_err(|_| preload_error("source account is poisoned"))?;
        let next = used
            .checked_add(MAX_APP_PRELOAD_BYTES)
            .filter(|next| *next <= MAX_HOST_PRELOAD_BYTES)
            .ok_or_else(|| preload_error("8 MiB Host source-image limit exhausted"))?;
        *used = next;
        Ok(AppPreloadReservation {
            budget: self.clone(),
            charged: MAX_APP_PRELOAD_BYTES,
        })
    }

    /// Retained and currently reserved source bytes, for owner observations.
    #[must_use]
    pub fn retained_bytes(&self) -> Option<usize> {
        self.0.lock().ok().map(|used| *used)
    }
}

/// Capacity retained across one bounded descriptor read and exact image check.
#[derive(Debug)]
pub struct AppPreloadReservation {
    budget: AppPreloadBudget,
    charged: usize,
}

impl Drop for AppPreloadReservation {
    fn drop(&mut self) {
        // A poisoned account refuses future admission. Recover only for exact
        // resource release, never to authorize additional construction.
        let mut used = self
            .budget
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *used = used.saturating_sub(self.charged);
    }
}

impl AppPreloadReservation {
    /// Retain the exact complete image and digest selected by Core's installed,
    /// boot-bound, rechecked graph. No renderer URL or JavaScript argument may
    /// supply the expected digest or select an alternate preload.
    /// `versions` is supplied by the canonical Host session-facts owner.
    ///
    /// # Errors
    /// Refuses changed source, invalid UTF-8, missing/bounded facts or arguments.
    pub fn from_qualified_graph(
        mut self,
        bytes: Vec<u8>,
        selected_sha256: [u8; 32],
        additional_arguments: Vec<String>,
        versions: std::collections::BTreeMap<String, String>,
    ) -> Result<AppPreloadImage, WvError> {
        if bytes.is_empty()
            || bytes.len() > MAX_APP_PRELOAD_BYTES
            || <[u8; 32]>::from(Sha256::digest(&bytes)) != selected_sha256
        {
            return Err(preload_error(
                "preload bytes differ from the Core-qualified boot-selected graph digest",
            ));
        }
        if additional_arguments.len() > 16 {
            return Err(preload_error("more than 16 preload arguments"));
        }
        let argument_bytes = additional_arguments
            .iter()
            .try_fold(0_usize, |total, value| {
                if value.len() > 256 || value.contains('\0') {
                    return None;
                }
                total.checked_add(value.len())
            })
            .filter(|total| *total <= 2_048)
            .ok_or_else(|| preload_error("preload arguments exceed their byte bound"))?;
        let _ = argument_bytes;
        let fact_bytes = versions.iter().try_fold(0_usize, |total, (key, value)| {
            if key.is_empty()
                || key.len() > 128
                || value.is_empty()
                || value.len() > 256
                || key.contains('\0')
                || value.contains('\0')
            {
                return None;
            }
            total.checked_add(key.len())?.checked_add(value.len())
        });
        if versions.is_empty() || versions.len() > 32 || fact_bytes.is_none_or(|size| size > 8_192)
        {
            return Err(preload_error(
                "canonical Host process versions are missing or exceed their bound",
            ));
        }
        let source =
            String::from_utf8(bytes).map_err(|_| preload_error("original preload is not UTF-8"))?;
        {
            let mut used = self
                .budget
                .0
                .lock()
                .map_err(|_| preload_error("source account is poisoned"))?;
            *used -= self.charged - source.len();
            self.charged = source.len();
        }
        Ok(AppPreloadImage(Arc::new(AppPreloadImageInner {
            source: source.into_boxed_str(),
            sha256: selected_sha256,
            additional_arguments,
            versions,
            _reservation: self,
        })))
    }
}

#[derive(Debug)]
struct AppPreloadImageInner {
    source: Box<str>,
    sha256: [u8; 32],
    additional_arguments: Vec<String>,
    versions: std::collections::BTreeMap<String, String>,
    _reservation: AppPreloadReservation,
}

/// Immutable Host-authenticated original image/configuration for exactly one window.
/// Clones share bytes and the one retention charge; navigation never reopens a path.
#[derive(Clone, Debug)]
pub struct AppPreloadImage(Arc<AppPreloadImageInner>);

impl AppPreloadImage {
    /// Borrow the exact already admitted image; native adapters never reopen its path.
    pub(crate) fn source(&self) -> &str {
        &self.0.source
    }

    /// Number of immutable source bytes retained by this image.
    #[must_use]
    pub fn source_bytes(&self) -> usize {
        self.0.source.len()
    }
    /// Ordered original window arguments captured before native construction.
    #[must_use]
    pub fn additional_arguments(&self) -> &[String] {
        &self.0.additional_arguments
    }
    /// Immutable canonical Host session process facts used by the preload.
    #[must_use]
    pub fn versions(&self) -> &std::collections::BTreeMap<String, String> {
        &self.0.versions
    }
    /// Whether a surviving window may adopt this exact source/configuration.
    #[must_use]
    pub fn same_configuration(&self, other: &Self) -> bool {
        self.0.sha256 == other.0.sha256
            && self.0.additional_arguments == other.0.additional_arguments
            && self.0.versions == other.0.versions
    }
}

pub(crate) fn preload_error(detail: impl Into<String>) -> WvError {
    WvError::AppPreloadSource {
        detail: detail.into(),
    }
}
