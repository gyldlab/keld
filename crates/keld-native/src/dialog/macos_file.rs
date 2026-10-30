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
    NSMenu, NSMenuItem, NSModalResponse, NSModalResponseCancel, NSModalResponseOK, NSOpenPanel,
    NSPopUpButton, NSSavePanel, NSWindow, NSWorkspace,
};
use objc2_foundation::{NSArray, NSSize, NSString, NSURL};
use objc2_uniform_type_identifiers::UTType;

use super::file_filter::MacFileFilterModel;

mod delegate;
use self::delegate::{DelegateMode, ExactFilePanelDelegate};
use super::{
    DialogReplyV1, MAX_DIALOG_PATHS, OpenDialogOptions, OpenProperty, SaveDialogOptions,
    validate_dialog_reply, validate_open_options, validate_save_options,
};

fn invalid(detail: &'static str) -> IpcError {
    IpcError::Protocol { detail }
}

/// Use `AppKit`'s package classification, not filesystem `is_dir` alone. The
/// default Open/Save panel treats file packages as selectable files; an
/// application may separately opt into traversing packages as directories.
fn is_native_directory_selection(
    path: &Path,
    treats_packages_as_directories: bool,
    workspace: &NSWorkspace,
) -> bool {
    if !path.is_dir() {
        return false;
    }
    treats_packages_as_directories
        || !workspace.isFilePackageAtPath(&NSString::from_str(&path.to_string_lossy()))
}

fn selected_open_paths(
    urls: &NSArray<NSURL>,
    filter: &MacFileFilterModel,
    allow_files: bool,
    allow_directories: bool,
    treats_packages_as_directories: bool,
) -> Result<Vec<String>, IpcError> {
    // Count before retaining NSArray members or materializing POSIX strings.
    // AppKit owns its original selection; Keld only copies bounded replies.
    if urls.len() > MAX_DIALOG_PATHS {
        return Err(invalid("native Open selection exceeds path limit"));
    }
    let workspace = NSWorkspace::sharedWorkspace();
    urls.to_vec()
        .into_iter()
        .map(|url| {
            if !url.isFileURL() {
                return Err(invalid("native Open returned a non-file URL"));
            }
            let path = url
                .path()
                .map(|text| text.to_string())
                .ok_or_else(|| invalid("native Open URL has no POSIX file path"))?;
            let native_is_directory = is_native_directory_selection(
                Path::new(&path),
                treats_packages_as_directories,
                &workspace,
            );
            if !filter.accepts_selected_path(
                Path::new(&path),
                native_is_directory,
                allow_files,
                allow_directories,
            ) {
                return Err(invalid(
                    "native Open selected a file outside the active filter",
                ));
            }
            Ok(path)
        })
        .collect()
}

fn selected_save_path(
    url: Option<Retained<NSURL>>,
    filter: &MacFileFilterModel,
    treats_packages_as_directories: bool,
) -> Result<String, IpcError> {
    let path = url
        .filter(|url| url.isFileURL())
        .and_then(|url| url.path())
        .map(|text| text.to_string())
        .ok_or_else(|| invalid("native Save returned no POSIX location"))?;
    let native_is_directory = is_native_directory_selection(
        Path::new(&path),
        treats_packages_as_directories,
        &NSWorkspace::sharedWorkspace(),
    );
    if !filter.accepts_selected_path(Path::new(&path), native_is_directory, true, false) {
        return Err(invalid(
            "native Save selected a file outside the active filter",
        ));
    }
    Ok(path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelState {
    Prepared,
    Presented,
    CancelRequested,
    Drained,
}

enum FilePanel {
    Open {
        panel: Retained<NSOpenPanel>,
        requested_files: bool,
        requested_directories: bool,
    },
    Save(Retained<NSSavePanel>),
}

impl FilePanel {
    fn save_panel(&self) -> &NSSavePanel {
        match self {
            Self::Open { panel, .. } => panel,
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
    native_groups: Vec<Vec<Retained<UTType>>>,
    failed_filter: bool,
    popup: Option<Retained<NSPopUpButton>>,
    // AppKit's delegate property is weak: this owner retains the exact native
    // admission callback until the sheet has physically drained.
    delegate: Option<Retained<ExactFilePanelDelegate>>,
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

fn default_hint(panel: &NSSavePanel, hint: Option<&String>) {
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
    // Electron applies a proposed filename to both Open and Save. An
    // existing directory hint only selects that folder, never its basename.
    if !is_directory && let Some(file) = path.file_name().and_then(|name| name.to_str()) {
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
        // Preflight EVERY group before presentation. A later accessory
        // selection must not discover an unrepresentable UTType while a
        // physical sheet owns the effect and its original correlation.
        let mut candidate = filter.clone();
        let mut native_groups = Vec::with_capacity(filters.len().max(1));
        for index in 0..filters.len().max(1) {
            if !filters.is_empty() {
                candidate.select(index)?;
            }
            let group = candidate
                .active_suffixes()
                .unwrap_or(&[])
                .iter()
                .map(|suffix| {
                    UTType::typeWithFilenameExtension(&NSString::from_str(suffix))
                        .ok_or_else(|| invalid("native file type could not be represented"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            native_groups.push(group);
        }
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
            native_groups,
            failed_filter: false,
            popup: None,
            delegate: None,
            state: PanelState::Prepared,
        };
        if filters.len() > 1 {
            let mtm = MainThreadMarker::new()
                .ok_or_else(|| invalid("file panel not on AppKit main thread"))?;
            let popup = NSPopUpButton::new(mtm);
            popup.setFrameSize(NSSize::new(280.0, 28.0));
            // NSPopUpButton::addItemWithTitle removes an earlier item with the
            // same title. Electron permits identically named filter groups,
            // whose *positions* have distinct meaning; allocate real menu
            // items instead, preserving every original index and label.
            let menu = NSMenu::new(mtm);
            for label in owner.filter.labels() {
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(label));
                menu.addItem(&item);
            }
            if usize::try_from(menu.numberOfItems()).ok() != Some(filters.len()) {
                return Err(invalid("native filter menu lost a declared group"));
            }
            popup.setMenu(Some(&menu));
            popup.selectItemAtIndex(0);
            owner.panel.save_panel().setAccessoryView(Some(&popup));
            owner.popup = Some(popup);
        }
        let mode = match &owner.panel {
            FilePanel::Open {
                requested_files,
                requested_directories,
                ..
            } => DelegateMode::Open {
                files: *requested_files,
                directories: *requested_directories,
            },
            FilePanel::Save(_) => DelegateMode::Save,
        };
        let main = MainThreadMarker::new()
            .ok_or_else(|| invalid("file panel not on AppKit main thread"))?;
        let native_delegate = ExactFilePanelDelegate::new(
            main,
            owner.filter.clone(),
            owner.popup.clone(),
            mode,
            owner.panel.save_panel().treatsFilePackagesAsDirectories(),
        )
        .ok_or_else(|| invalid("native file panel delegate initialization failed"))?;
        native_delegate.attach_to(owner.panel.save_panel());
        owner.delegate = Some(native_delegate);
        owner.apply_filter()?;
        // Configure native content-type admission before selecting the first
        // directory. The modern panel may rebuild its file browser when the
        // type set changes; a path hint applied earlier can be lost.
        default_hint(owner.panel.save_panel(), hint);
        if let FilePanel::Open {
            panel,
            requested_files,
            requested_directories,
        } = &owner.panel
        {
            // The directory hint may rebuild Open's browser after its type
            // set was installed. Preserve both admitted selection modes at
            // the final native setup boundary, including mixed file+folder.
            panel.setCanChooseFiles(*requested_files);
            panel.setCanChooseDirectories(*requested_directories);
        }
        Ok(owner)
    }

    fn apply_filter(&self) -> Result<(), IpcError> {
        // An empty NSArray is the documented modern 'unrestricted' type set.
        // Never union groups or invoke the deprecated setAllowedFileTypes.
        let types = self
            .native_groups
            .get(self.filter.selected_index().unwrap_or(0))
            .ok_or_else(|| invalid("native filter index missing its validated type group"))?;
        self.panel
            .save_panel()
            .setAllowedContentTypes(&NSArray::from_retained_slice(types));
        if let FilePanel::Open {
            panel,
            requested_files,
            requested_directories,
        } = &self.panel
        {
            // The platform can rewrite these UI flags while updating content
            // types. Restore the admitted request mode after each group switch.
            panel.setCanChooseFiles(*requested_files);
            panel.setCanChooseDirectories(*requested_directories);
        }
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
        let requested_directories = properties.contains(&OpenProperty::OpenDirectory);
        let requested_files =
            !requested_directories || properties.contains(&OpenProperty::OpenFile);
        panel.setCanChooseFiles(requested_files);
        panel.setCanChooseDirectories(requested_directories);
        panel.setAllowsMultipleSelection(properties.contains(&OpenProperty::MultiSelections));
        Self::finish_setup(
            FilePanel::Open {
                panel,
                requested_files,
                requested_directories,
            },
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
    fn update_filter_index_from_popup(&mut self) -> Result<bool, IpcError> {
        let Some(popup) = &self.popup else {
            return Ok(false);
        };
        let value = popup.indexOfSelectedItem();
        let index =
            usize::try_from(value).map_err(|_| invalid("no valid native filter selected"))?;
        self.filter.select(index)
    }

    /// Keep `AppKit`'s current content types synchronized with the user's
    /// native accessory choice. This is also re-read at final completion,
    /// because the last click can outrun the next Tao polling turn.
    ///
    /// # Errors
    ///
    /// Refuses an invalid native accessory index or missing type group.
    pub fn refresh_filter_choice(&mut self) -> Result<bool, IpcError> {
        if !self.update_filter_index_from_popup()? {
            return Ok(false);
        }
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
            if self.state == PanelState::Presented && self.refresh_filter_choice().is_err() {
                // Fail the request only after the real sheet is dismissed.
                // Do not misreport this native failure as user Cancel.
                self.failed_filter = true;
                self.request_cancel()?;
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
        if self.failed_filter {
            return Err(invalid("native filter selection failed"));
        }
        let reply = if response == NSModalResponseCancel {
            match self.panel {
                FilePanel::Open { .. } => DialogReplyV1::Open {
                    canceled: true,
                    file_paths: vec![],
                },
                FilePanel::Save(_) => DialogReplyV1::Save {
                    canceled: true,
                    file_path: String::new(),
                },
            }
        } else if response == NSModalResponseOK {
            // The last popup change can happen between our service turns.
            // Capture the real current selection after physical drain before
            // validating the returned URL, without altering native UI again.
            self.update_filter_index_from_popup()?;
            match &self.panel {
                FilePanel::Open {
                    panel: open,
                    requested_files,
                    requested_directories,
                } => {
                    let file_paths = selected_open_paths(
                        &open.URLs(),
                        &self.filter,
                        *requested_files,
                        *requested_directories,
                        open.treatsFilePackagesAsDirectories(),
                    )?;
                    DialogReplyV1::Open {
                        // An OS OK with zero selections is not a user Cancel.
                        // Validation below rejects it as an invalid result.
                        canceled: false,
                        file_paths,
                    }
                }
                FilePanel::Save(save) => DialogReplyV1::Save {
                    canceled: false,
                    file_path: selected_save_path(
                        save.URL(),
                        &self.filter,
                        save.treatsFilePackagesAsDirectories(),
                    )?,
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
        // Keep the exact delegate alive during physical dismissal. AppKit can
        // still consult its weak callback while endSheet/orderOut runs.
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
        if self.delegate.is_some() {
            // The weak AppKit reference must be cleared before deallocation,
            // including failed setup and unexpected sheet teardown.
            ExactFilePanelDelegate::detach_from(self.panel.save_panel());
            self.delegate.take();
        }
    }
}

#[cfg(test)]
#[path = "macos_file/tests.rs"]
mod tests;
