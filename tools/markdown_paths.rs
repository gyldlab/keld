//! Shared extension owner for Markdown validation and discovery.

use std::path::Path;

/// GitHub-rendered Markdown extensions, matched without ASCII case distinctions.
pub(crate) const MARKDOWN_EXTENSIONS: [&str; 13] = [
    "litcoffee",
    "livemd",
    "markdown",
    "md",
    "mdown",
    "mdwn",
    "mdx",
    "mkd",
    "mkdn",
    "mkdown",
    "ronn",
    "scd",
    "workbook",
];

pub(crate) fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}
