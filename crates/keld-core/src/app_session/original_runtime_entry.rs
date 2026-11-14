//! One canonical generated original-entry source for CLI and Host admission.
//! This pure renderer grants no graph, profile, provider, or native authority.

use super::HostAppError;

/// Fixed app-root ESM bootstrap with Core-compiled, exact-checked bytes.
/// Its position beside the original entry keeps the installed API and user
/// entry in one Bun module graph even where CLI preload caches are isolated.
pub const ORIGINAL_RUNTIME_ENTRY_PATH: &str = "keld-installed-runtime-entry.mjs";
const BOOT_ENTRY_MARKER: &str = "__KELD_SELECTED_ENTRY__";
pub(super) const BOOT_ENTRY_TEMPLATE: &str =
    include_str!("../../../../packages/@keld/api/src/installed-runtime-entry.mjs");

/// Render the one graph/boot-selected original as a **literal** dynamic import
/// in a KELD-authored, pre-sign app-root source file. The pinned Bun 1.4.4
/// Windows loader creates a *different* package module cache for variable
/// imports, even when they resolve to the same exact original file.
///
/// This renderer is shared by CLI producer and Core installed admission; a
/// caller cannot supply new executable source bytes. Boot/graph validation
/// independently binds the original path before either use.
///
/// # Errors
/// Refuses noncanonical, escaping, or URL-sensitive paths that the pinned
/// runtime cannot safely resolve as literal ESM module specifiers.
pub fn original_entry_bootstrap_source(original_relative: &str) -> Result<Vec<u8>, HostAppError> {
    super::graph_selection::validate_graph_relative_path(original_relative)?;
    if !original_relative
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_'))
    {
        return Err(HostAppError::new(
            "KELD-CORE-036",
            "original entry import",
            "the pinned Bun runtime requires URL-neutral original main source names",
            "Choose a URL-neutral original main path before source capture; do not edit signed files.",
        ));
    }
    if BOOT_ENTRY_TEMPLATE.matches(BOOT_ENTRY_MARKER).count() != 2 {
        return Err(HostAppError::new(
            "KELD-CORE-036",
            "original entry template",
            "compiled KELD owner template has unexpected replacement slots",
            "Rebuild an intact KELD Host and Pack from reviewed sources.",
        ));
    }
    Ok(BOOT_ENTRY_TEMPLATE
        .replace(BOOT_ENTRY_MARKER, original_relative)
        .into_bytes())
}

#[cfg(test)]
mod tests;
