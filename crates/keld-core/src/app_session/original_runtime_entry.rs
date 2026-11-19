//! One canonical generated original-entry source for CLI and Host admission.
//! This pure renderer grants no graph, profile, provider, or native authority.

use super::HostAppError;

/// Fixed app-root ESM bootstrap with Core-compiled, exact-checked bytes.
/// Its position beside the original entry keeps the installed API and user
/// entry in one Bun module graph even where CLI preload caches are isolated.
pub const ORIGINAL_RUNTIME_ENTRY_PATH: &str = "keld-installed-runtime-entry.mjs";
const BOOT_ENTRY_MARKER: &str = "__KELD_SELECTED_ENTRY__";
const BOOT_ENTRY_TEMPLATE: &str =
    include_str!("../../../../packages/@keld/api/src/installed-runtime-entry.mjs");

/// Renders the qualified Windows development entry from the shared template.
///
/// The fixed physical facade is already part of the independently pinned graph.
/// Its canonical installer runs in the same module graph before Runtime captures
/// the versions that the original renderer receives. Remove the internal wrapper
/// slot once it has finished checking the physical entry, leaving Bun's operator
/// launch shape for the original application's `defaultApp` parser.
///
/// # Errors
/// Refuses invalid original paths or a compiled template without its capture call.
pub fn windows_original_entry_bootstrap_source(
    original_relative: &str,
) -> Result<Vec<u8>, HostAppError> {
    let mut source = render_original_entry(original_relative)?;
    let capture = source.find("await bootHostSelectedEntry(").ok_or_else(|| {
        HostAppError::new(
            "KELD-CORE-036",
            "Windows original entry template",
            "compiled KELD owner template has no Runtime capture boundary",
            "Rebuild an intact KELD Host and CLI from reviewed sources.",
        )
    })?;
    source.insert_str(
        capture,
        concat!(
            "await import(\"./node_modules/electron/dist/index.js\");\n",
            "// The verified wrapper is not a user argument to the original entry.\n",
            "process.argv.splice(1, 1);\n",
        ),
    );
    Ok(source.into_bytes())
}

fn render_original_entry(original_relative: &str) -> Result<String, HostAppError> {
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
    Ok(BOOT_ENTRY_TEMPLATE.replace(BOOT_ENTRY_MARKER, original_relative))
}

#[cfg(test)]
mod tests;
