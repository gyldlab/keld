//! Shared public-repository path policy for standalone documentation tools.

use std::path::{Component, Path};

const FORBIDDEN_PREFIXES: &[&str] = &[
    ".git",
    "docs/research",
    "competitors",
    ".claude",
    "private",
    ".private",
];

/// Returns whether a repository-relative path escapes the public tracked contract.
pub(crate) fn escapes_public_repo(candidate: &Path) -> bool {
    let normalized = candidate.to_string_lossy().replace('\\', "/");
    let normalized = normalized.trim_start_matches("./").to_ascii_lowercase();
    let windows_drive = normalized.as_bytes().get(1) == Some(&b':')
        && normalized
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic);
    let windows_reserved = normalized.split('/').any(|component| {
        let stem = component.split('.').next().unwrap_or_default();
        let numbered_device = stem
            .strip_prefix("com")
            .or_else(|| stem.strip_prefix("lpt"))
            .is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            });
        component.ends_with(['.', ' '])
            || component.contains(':')
            || component
                .chars()
                .any(|character| character < ' ' || "<>\"|?*".contains(character))
            || matches!(stem, "con" | "prn" | "aux" | "nul")
            || numbered_device
    });
    candidate.as_os_str().is_empty()
        || candidate.is_absolute()
        || windows_drive
        || normalized.starts_with("//")
        || windows_reserved
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::ParentDir | Component::RootDir
            )
        })
        || normalized == "roadmap.md"
        || FORBIDDEN_PREFIXES
            .iter()
            .any(|prefix| normalized == *prefix || normalized.starts_with(&format!("{prefix}/")))
}
