//! True `NSOpenPanel` / `NSSavePanel` main-thread sheet ownership.
//! No new generation, filesystem handle, bookmark, native thread or socket.
//! The Host must authenticate the role and resolve the parent before calling.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use block2::RcBlock;
use keld_ipc::IpcError;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSModalResponse, NSModalResponseCancel, NSModalResponseOK, NSOpenPanel, NSPopUpButton,
    NSSavePanel, NSWindow,
};
use objc2_foundation::{NSArray, NSSize, NSString, NSURL};
use objc2_uniform_type_identifiers::UTType;

use super::file_filter::MacFileFilterModel;
use super::{
    DialogReplyV1, MAX_DIALOG_PATHS, OpenDialogOptions, OpenProperty, SaveDialogOptions,
    validate_dialog_reply, validate_open_options, validate_save_options,
};

fn invalid(detail: &'static str) -> IpcError {
    IpcError::Protocol { detail }
}

fn selected_open_paths(urls: &NSArray<NSURL>) -> Result<Vec<String>, IpcError> {
    // Count before retaining NSArray members or materializing POSIX strings.
    // AppKit owns its original selection; Keld only copies bounded replies.
    if urls.len() > MAX_DIALOG_PATHS {
        return Err(invalid("native Open selection exceeds path limit"));
    }
    urls.to_vec()
        .into_iter()
        .map(|url| {
            if !url.isFileURL() {
                return Err(invalid("native Open returned a non-file URL"));
            }
            url.path()
                .map(|text| text.to_string())
                .ok_or_else(|| invalid("native Open URL has no POSIX file path"))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelState {
    Prepared,
    Presented,
    CancelRequested,
    Drained,
}

enum FilePanel {
    Open(Retained<NSOpenPanel>),
    Save(Retained<NSSavePanel>),
}

impl FilePanel {
    fn save_panel(&self) -> &NSSavePanel {
        match self {
            Self::Open(panel) => panel,
            Self::Save(panel) => panel,
        }
    }
}

/// A real AppKit-owned file panel and its one retained callback. Its Objective-C
/// objects are deliberately !Send, never copied into a background worker.
pub struct MacFilePanel {
    panel: FilePanel,
    completion: Rc<Cell<Option<NSModalResponse>>>,
    callback: RcBlock<dyn Fn(NSModalResponse)>,
    filter: MacFileFilterModel,
    popup: Option<Retained<NSPopUpButton>>,
    state: PanelState,
}

impl std::fmt::Debug for MacFilePanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MacFilePanel")
            .field("state", &self.state)
            .field("group_index", &self.filter.selected_index())
            .field("has_popup", &self.popup.is_some())
            .finish_non_exhaustive()
    }
}

fn default_hint(panel: &NSSavePanel, hint: Option<&String>, is_save: bool) {
    let Some(hint) = hint else { return };
    let path = Path::new(hint);
    // Match the pinned Electron distinction between a directory hint and a
    // filename hint. This metadata lookup configures UI only; it opens no
    // selected file and grants no filesystem authority.
    let is_directory = path.is_absolute() && path.is_dir();
    let directory = if is_directory {
        Some(path)
    } else if path.is_absolute() {
        path.parent()
    } else {
        None
    };
    if let Some(directory) = directory {
        let url = NSURL::fileURLWithPath_isDirectory(
            &NSString::from_str(&directory.to_string_lossy()),
            true,
        );
        panel.setDirectoryURL(Some(&url));
    }
    // A directory is never a proposed Save filename. Open filename-field
    // parity is separate from choosing the correct initial directory.
    if is_save
        && !is_directory
        && let Some(file) = path.file_name().and_then(|name| name.to_str())
    {
        panel.setNameFieldStringValue(&NSString::from_str(file));
    }
}

impl MacFilePanel {
    fn finish_setup(
        panel: FilePanel,
        hint: Option<&String>,
        filters: &[super::FileDialogFilter],
    ) -> Result<Self, IpcError> {
        let filter = MacFileFilterModel::new(filters)?;
        let completion = Rc::new(Cell::new(None));
        let state = Rc::clone(&completion);
        let callback: RcBlock<dyn Fn(NSModalResponse)> = RcBlock::new(move |response| {
            if state.get().is_none() {
                state.set(Some(response));
            }
        });
        let mut owner = Self {
            panel,
            completion,
            callback,
            filter,
            popup: None,
            state: PanelState::Prepared,
        };
        if filters.len() > 1 {
            let mtm = MainThreadMarker::new()
                .ok_or_else(|| invalid("file panel not on AppKit main thread"))?;
            let popup = NSPopUpButton::new(mtm);
            popup.setFrameSize(NSSize::new(280.0, 28.0));
            for label in owner.filter.labels() {
                popup.addItemWithTitle(&NSString::from_str(label));
            }
            popup.selectItemAtIndex(0);
            owner.panel.save_panel().setAccessoryView(Some(&popup));
            owner.popup = Some(popup);
        }
        owner.apply_filter()?;
        // Configure native content-type admission before selecting the first
        // directory. The modern panel may rebuild its file browser when the
        // type set changes; a path hint applied earlier can be lost.
        default_hint(
            owner.panel.save_panel(),
            hint,
            matches!(owner.panel, FilePanel::Save(_)),
        );
        Ok(owner)
    }

    fn apply_filter(&self) -> Result<(), IpcError> {
        // An empty NSArray is the documented modern 'unrestricted' type set.
        // Never union groups or invoke the deprecated setAllowedFileTypes.
        let types: Vec<Retained<UTType>> = self
            .filter
            .active_suffixes()
            .unwrap_or(&[])
            .iter()
            .map(|suffix| {
                UTType::typeWithFilenameExtension(&NSString::from_str(suffix))
                    .ok_or_else(|| invalid("native file type could not be represented"))
            })
            .collect::<Result<_, _>>()?;
        self.panel
            .save_panel()
            .setAllowedContentTypes(&NSArray::from_retained_slice(&types));
        Ok(())
    }

    /// Construct a bounded real open panel without showing any UI yet.
    ///
    /// # Errors
    ///
    /// Rejects unsupported option flags and native type conversion.
    pub fn prepare_open(
        main: MainThreadMarker,
        options: &OpenDialogOptions,
    ) -> Result<Self, IpcError> {
        validate_open_options(options)?;
        let panel = NSOpenPanel::openPanel(main);
        let properties = &options.properties;
        panel.setCanChooseFiles(
            !properties.contains(&OpenProperty::OpenDirectory)
                || properties.contains(&OpenProperty::OpenFile),
        );
        panel.setCanChooseDirectories(properties.contains(&OpenProperty::OpenDirectory));
        panel.setAllowsMultipleSelection(properties.contains(&OpenProperty::MultiSelections));
        Self::finish_setup(
            FilePanel::Open(panel),
            options.default_path.as_ref(),
            &options.filters,
        )
    }

    /// Construct a bounded real Save panel without writing a file or scope.
    ///
    /// # Errors
    ///
    /// Rejects unsupported bookmark/filters before a visible OS panel exists.
    pub fn prepare_save(
        main: MainThreadMarker,
        options: &SaveDialogOptions,
    ) -> Result<Self, IpcError> {
        validate_save_options(options)?;
        let panel = NSSavePanel::savePanel(main);
        panel.setCanCreateDirectories(false);
        Self::finish_setup(
            FilePanel::Save(panel),
            options.default_path.as_ref(),
            &options.filters,
        )
    }

    /// Native accessory selection is read on each Tao service turn. Switching
    /// groups replaces the current allowed type set, never unions filters.
    ///
    /// # Errors
    ///
    /// Refuses invalid native menu indexes without altering the selected set.
    pub fn refresh_filter_choice(&mut self) -> Result<bool, IpcError> {
        let Some(popup) = &self.popup else {
            return Ok(false);
        };
        let value = popup.indexOfSelectedItem();
        let index =
            usize::try_from(value).map_err(|_| invalid("no valid native filter selected"))?;
        if self.filter.selected_index() == Some(index) {
            return Ok(false);
        }
        self.filter.select(index)?;
        self.apply_filter()?;
        self.panel.save_panel().validateVisibleColumns();
        Ok(true)
    }

    /// Begin one actual parented `AppKit` file chooser sheet, never runModal
    /// inside Tao's event callback. The parent is Host/Wry-resolved.
    ///
    /// # Errors
    ///
    /// Rejects duplicate or already retired presentations.
    pub fn begin_sheet(&mut self, parent: &NSWindow) -> Result<(), IpcError> {
        if self.state != PanelState::Prepared {
            return Err(invalid("file dialog cannot present more than once"));
        }
        self.panel
            .save_panel()
            .beginSheetModalForWindow_completionHandler(parent, &self.callback);
        self.state = PanelState::Presented;
        Ok(())
    }

    /// Ask the actual parent to end its retained sheet; never invent a clicked
    /// file path or assume physical cleanup from the Cancel call alone.
    ///
    /// # Errors
    ///
    /// Refuses an inconsistent native sheet-parent state.
    pub fn request_cancel(&mut self) -> Result<bool, IpcError> {
        match self.state {
            PanelState::Prepared => {
                self.state = PanelState::Drained;
                Ok(true)
            }
            PanelState::Drained | PanelState::CancelRequested => Ok(false),
            PanelState::Presented => {
                if self.completion.get().is_some() {
                    return Ok(false);
                }
                let sheet: &NSWindow = self.panel.save_panel();
                let parent = sheet
                    .sheetParent()
                    .ok_or_else(|| invalid("native file sheet lost its parent"))?;
                parent.endSheet_returnCode(sheet, NSModalResponseCancel);
                self.state = PanelState::CancelRequested;
                Ok(true)
            }
        }
    }

    /// Return a genuine result only after the callback and native window
    /// invisibility, validating all returned paths before releasing custody.
    ///
    /// # Errors
    ///
    /// Rejects OS failure/abort, unsafe paths, or absent Save location.
    pub fn poll_outcome(&mut self) -> Result<Option<DialogReplyV1>, IpcError> {
        if !matches!(
            self.state,
            PanelState::Presented | PanelState::CancelRequested
        ) {
            return Ok(None);
        }
        let Some(response) = self.completion.get() else {
            if self.state == PanelState::Presented {
                self.refresh_filter_choice()?;
            }
            return Ok(None);
        };
        let panel = self.panel.save_panel();
        if panel.isVisible() {
            return Ok(None);
        }
        // The native callback and invisibility establish physical completion
        // even when a subsequent result-path validation reports an error.
        self.state = PanelState::Drained;
        let reply = if response == NSModalResponseCancel {
            match self.panel {
                FilePanel::Open(_) => DialogReplyV1::Open {
                    canceled: true,
                    file_paths: vec![],
                },
                FilePanel::Save(_) => DialogReplyV1::Save {
                    canceled: true,
                    file_path: String::new(),
                },
            }
        } else if response == NSModalResponseOK {
            match &self.panel {
                FilePanel::Open(open) => {
                    let file_paths = selected_open_paths(&open.URLs())?;
                    DialogReplyV1::Open {
                        // An OS OK with zero selections is not a user Cancel.
                        // Validation below rejects it as an invalid result.
                        canceled: false,
                        file_paths,
                    }
                }
                FilePanel::Save(save) => DialogReplyV1::Save {
                    canceled: false,
                    file_path: save
                        .URL()
                        .filter(|url| url.isFileURL())
                        .and_then(|url| url.path())
                        .map(|text| text.to_string())
                        .ok_or_else(|| invalid("native Save returned no POSIX location"))?,
                },
            }
        } else {
            return Err(invalid(
                "native file dialog did not report a selection or Cancel",
            ));
        };
        validate_dialog_reply(&reply)?;
        Ok(Some(reply))
    }

    /// True only after actual callback and physical native sheet drain, or
    /// cancellation before presentation. The UI owner must retain until then.
    #[must_use]
    pub fn quiescent(&self) -> bool {
        self.state == PanelState::Drained
    }
}

impl Drop for MacFilePanel {
    fn drop(&mut self) {
        // Unexpected owner teardown only: normal custody waits for the real
        // callback and panel invisibility. This does not assert completion.
        if matches!(
            self.state,
            PanelState::Presented | PanelState::CancelRequested
        ) {
            let panel: &NSWindow = self.panel.save_panel();
            if let Some(parent) = panel.sheetParent() {
                parent.endSheet_returnCode(panel, NSModalResponseCancel);
            } else if panel.isVisible() {
                panel.orderOut(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_open_selection_preserves_exact_paths() {
        let empty = NSArray::<NSURL>::new();
        assert_eq!(
            selected_open_paths(&empty).expect("empty count is bounded"),
            Vec::<String>::new()
        );

        let selected = NSURL::fileURLWithPath_isDirectory(
            &NSString::from_str("/tmp/gh478-selection.drawio"),
            false,
        );
        let entries = vec![selected; MAX_DIALOG_PATHS];
        let urls = NSArray::from_retained_slice(&entries);
        assert_eq!(
            selected_open_paths(&urls).expect("exactly 32 file URLs are bounded"),
            vec![String::from("/tmp/gh478-selection.drawio"); MAX_DIALOG_PATHS],
        );
    }

    #[test]
    fn excessive_open_selection_is_rejected_before_url_conversion() {
        // An invalid URL first ensures that per-entry conversion cannot take
        // precedence over the independently specified native selection count.
        let non_file = NSURL::URLWithString(&NSString::from_str("https://example.invalid/file"))
            .expect("synthetic non-file URL");
        let entries = vec![non_file; MAX_DIALOG_PATHS + 1];
        let urls = NSArray::from_retained_slice(&entries);
        let error = selected_open_paths(&urls).expect_err("32+1 native paths must be refused");
        assert!(
            matches!(
                error,
                IpcError::Protocol {
                    detail: "native Open selection exceeds path limit"
                }
            ),
            "native over-limit selection was processed before refusal: {error:?}"
        );
    }
}
