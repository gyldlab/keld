//! Approved Windows bounds used by the retained resource source (GH1185 section 4.5).

pub(crate) struct IntakeLimits {
    pub(crate) max_url_utf16_units: usize,
    pub(crate) max_asset_bytes: usize,
}

pub(crate) const ORIGINAL_WEBAPP_LIMITS: IntakeLimits = IntakeLimits {
    max_url_utf16_units: 2_048,
    max_asset_bytes: 64 * 1024 * 1024,
};
