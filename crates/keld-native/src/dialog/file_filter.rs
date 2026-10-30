//! Pure native file-filter group ownership, prior to `NSOpenPanel` creation.
//! A group switch replaces the one active allowed-type set, never a union.

use std::collections::HashSet;
use std::path::Path;

use keld_ipc::IpcError;

use super::{FileDialogFilter, validate_filter_groups};

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilterGroup {
    label: String,
    /// `None` is the exact unrestricted `*` selection, not another extension.
    suffixes: Option<Vec<String>>,
}

/// One selected native accessory filter group. No `AppKit` objects or effect
/// authority are retained by this model. The main-thread UI owner will bind
/// the selected group to the real native panel and its observable popup state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacFileFilterModel {
    groups: Vec<FilterGroup>,
    selected: Option<usize>,
}

impl MacFileFilterModel {
    /// Prepare one immutable ordered list of first-proof native file types.
    ///
    /// # Errors
    ///
    /// Rejects malformed groups before any `NSOpenPanel`/`NSSavePanel` exists.
    pub fn new(filters: &[FileDialogFilter]) -> Result<Self, IpcError> {
        validate_filter_groups(filters)?;
        let mut groups = Vec::with_capacity(filters.len());
        for filter in filters {
            let suffixes = if filter.extensions == ["*"] {
                None
            } else {
                let mut seen = HashSet::new();
                let mut suffixes = Vec::with_capacity(filter.extensions.len());
                for extension in &filter.extensions {
                    // A compound extension like `draw.io` has an effective
                    // `io` type in the pinned macOS first proof. The complete
                    // source spelling remains untouched in the wire DTO.
                    let suffix = extension.rsplit('.').next().ok_or(IpcError::Protocol {
                        detail: "native file extension has no terminal suffix",
                    })?;
                    if suffix.is_empty() {
                        return Err(IpcError::Protocol {
                            detail: "native file extension has an empty terminal suffix",
                        });
                    }
                    let suffix = suffix.to_ascii_lowercase();
                    if seen.insert(suffix.clone()) {
                        suffixes.push(suffix);
                    }
                }
                Some(suffixes)
            };
            groups.push(FilterGroup {
                label: filter.name.clone(),
                suffixes,
            });
        }
        let selected = (!groups.is_empty()).then_some(0);
        Ok(Self { groups, selected })
    }

    /// Every presented accessory label, in the original filter group order.
    #[must_use]
    pub fn labels(&self) -> Vec<&str> {
        self.groups
            .iter()
            .map(|group| group.label.as_str())
            .collect()
    }

    /// The selected `NSPopUpButton` item, or no accessory when unfiltered.
    #[must_use]
    pub const fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// The selected group's native extension restrictions. `None` means no
    /// restriction, either because no group exists or `All Files` is selected.
    #[must_use]
    pub fn active_suffixes(&self) -> Option<&[String]> {
        self.selected
            .and_then(|index| self.groups.get(index))
            .and_then(|group| group.suffixes.as_deref())
    }

    /// Do not treat a native `UTType`'s conformance as an exact file-extension
    /// match. `AppKit` can enable `.svg` for an XML-only `allowedContentTypes`
    /// group; selected file paths must never inherit that wider admission.
    #[must_use]
    pub fn accepts_selected_path(
        &self,
        path: &Path,
        native_is_directory: bool,
        allow_files: bool,
        allow_directories: bool,
    ) -> bool {
        // The native panel owns directory-versus-file classification. macOS
        // packages can be directories on disk but selectable *files* in the
        // panel; this pure group model must not redo that OS decision.
        if native_is_directory {
            return allow_directories;
        }
        if !allow_files {
            return false;
        }
        let Some(suffixes) = self.active_suffixes() else {
            return true;
        };
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                suffixes
                    .iter()
                    .any(|suffix| ext.eq_ignore_ascii_case(suffix))
            })
    }

    /// Replace the active group only if the requested item actually exists.
    ///
    /// # Errors
    ///
    /// A forged/out-of-range index leaves the current group unchanged.
    pub fn select(&mut self, index: usize) -> Result<bool, IpcError> {
        if index >= self.groups.len() {
            return Err(IpcError::Protocol {
                detail: "native file filter selection is outside declared groups",
            });
        }
        if self.selected == Some(index) {
            return Ok(false);
        }
        self.selected = Some(index);
        Ok(true)
    }
}
