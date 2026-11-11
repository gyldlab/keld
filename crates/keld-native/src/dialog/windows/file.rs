//! Common Item Dialog mapping; every returned path comes from an `IShellItem`.
#![deny(unsafe_op_in_unsafe_fn)]

use std::path::Path;
use std::rc::Rc;

use keld_ipc::IpcError;
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR,
    FOS_NOTESTFILECREATE, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog,
    FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
    SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{HRESULT, PCWSTR};

use super::super::{
    DialogReplyV1, FileDialogFilter, MAX_DIALOG_PATHS, OpenDialogOptions, OpenProperty,
    SaveDialogOptions, validate_open_options, validate_save_options,
};
use super::file_timer::FileTimer;
use super::{Cancellation, invalid, native, wide};

#[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
fn configure(
    dialog: &IFileDialog,
    filters: &[FileDialogFilter],
    hint: Option<&str>,
) -> Result<(), IpcError> {
    let fallback = [FileDialogFilter {
        name: "All Files (*.*)".into(),
        extensions: vec!["*".into()],
    }];
    let filters = if filters.is_empty() {
        &fallback[..]
    } else {
        filters
    };
    let names = filters
        .iter()
        .map(|filter| wide(&filter.name))
        .collect::<Result<Vec<_>, _>>()?;
    let specs = filters
        .iter()
        .map(|filter| {
            let mut patterns = Vec::with_capacity(filter.extensions.len());
            for extension in &filter.extensions {
                if extension
                    .chars()
                    .any(|ch| matches!(ch, ';' | '?' | '<' | '>' | ':' | '"' | '|'))
                {
                    return Err(invalid("unsupported Windows file filter extension"));
                }
                patterns.push(if extension == "*" {
                    "*.*".into()
                } else {
                    format!("*.{extension}")
                });
            }
            wide(&patterns.join(";"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let native_filters = names
        .iter()
        .zip(&specs)
        .map(|(name, spec)| COMDLG_FILTERSPEC {
            pszName: PCWSTR(name.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        })
        .collect::<Vec<_>>();
    // SAFETY: all strings and descriptors remain live through these synchronous
    // COM setters; IFileDialog copies them and stays on its creating STA.
    unsafe {
        native(
            dialog.SetFileTypes(&native_filters),
            "Windows dialog filter setup failed",
        )?;
        // Pinned Electron selects the first non-wildcard group and lets the
        // native dialog update extensions when the user changes filter groups.
        if let Some((index, filter)) = filters
            .iter()
            .enumerate()
            .find(|(_, filter)| filter.extensions[0] != "*")
        {
            let index = u32::try_from(index + 1)
                .map_err(|_| invalid("Windows filter index exceeds native range"))?;
            native(
                dialog.SetFileTypeIndex(index),
                "Windows dialog filter index failed",
            )?;
            let extension = wide(&filter.extensions[0])?;
            native(
                dialog.SetDefaultExtension(&extension),
                "Windows default extension failed",
            )?;
        }
    }
    if let Some(hint) = hint.filter(|value| !value.is_empty()) {
        default_hint(dialog, hint)?;
    }
    Ok(())
}

#[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
fn default_hint(dialog: &IFileDialog, hint: &str) -> Result<(), IpcError> {
    let path = Path::new(hint);
    // Match the pinned Windows branch: metadata distinguishes a directory; for
    // a missing path a trailing separator is a directory hint. This read grants
    // no access and cannot create/truncate the selected application file.
    let directory = path.metadata().map_or_else(
        |_| hint.ends_with(['\\', '/']),
        |metadata| metadata.is_dir(),
    );
    if !directory {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("invalid Windows default filename"))?;
        let name = wide(name)?;
        // SAFETY: the validated UTF-16 name remains live for the copying setter.
        native(
            unsafe { dialog.SetFileName(&name) },
            "Windows default filename failed",
        )?;
    }
    if path.is_absolute() {
        let folder = if directory {
            path
        } else {
            path.parent()
                .ok_or_else(|| invalid("Windows default path has no parent"))?
        };
        let folder = folder
            .to_str()
            .ok_or_else(|| invalid("invalid Windows default folder"))?;
        let folder = wide(folder)?;
        // SAFETY: NUL-terminated owned UTF-16; no bind context or raw ownership
        // adoption. The returned typed COM item owns its reference on this STA.
        let item: IShellItem = native(
            unsafe { SHCreateItemFromParsingName(&folder, None) },
            "Windows default folder lookup failed",
        )?;
        // SAFETY: both typed objects belong to this STA; setter retains its item.
        native(
            unsafe { dialog.SetFolder(&item) },
            "Windows default folder failed",
        )?;
    } else if path.components().count() != 1 {
        return Err(invalid(
            "Windows default path must be absolute or a filename",
        ));
    }
    Ok(())
}

#[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
fn modal(parent: HWND, dialog: &IFileDialog, cancel: &Rc<Cancellation>) -> Result<bool, IpcError> {
    if cancel.check() {
        return Ok(false);
    }
    let timer = FileTimer::start(dialog, Rc::clone(cancel))?;
    // Recheck after native setup and immediately before visible presentation.
    if cancel.check() {
        timer.finish()?;
        return Ok(false);
    }
    // SAFETY: checked same-STA parent is retained by Core for this call. No Core
    // lock/RefCell is held here. The STA timer dismisses this retained object.
    let shown = unsafe { dialog.Show(Some(parent)) };
    timer.finish()?;
    if cancel.check() {
        return Ok(false);
    }
    match shown {
        Ok(()) => Ok(true),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => Ok(false),
        Err(error) => native(Err(error), "Windows file dialog presentation failed"),
    }
}

#[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
fn path(item: &IShellItem) -> Result<String, IpcError> {
    // SAFETY: this typed item belongs to the current STA. SIGDN_FILESYSPATH
    // returns a CoTaskMem-owned, NUL-terminated string or a checked HRESULT.
    let allocated = native(
        unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) },
        "Windows file dialog result path failed",
    )?;
    if allocated.is_null() {
        return Err(invalid("Windows file dialog returned a null path"));
    }
    // SAFETY: the successful GetDisplayName contract owns this full UTF-16
    // allocation until CoTaskMemFree. Copy strictly, then free even on bad UTF-16.
    let copied = unsafe { allocated.to_string() };
    // SAFETY: exactly the allocation returned above, freed once after copying.
    unsafe { CoTaskMemFree(Some(allocated.0.cast())) };
    copied.map_err(|_| invalid("Windows dialog result contains invalid UTF-16"))
}

#[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
pub(super) fn open(
    parent: HWND,
    options: &OpenDialogOptions,
    cancel: &Rc<Cancellation>,
) -> Result<Option<DialogReplyV1>, IpcError> {
    validate_open_options(options)?;
    // SAFETY: show checked the existing Host STA; no new apartment/thread exists.
    let dialog: IFileOpenDialog = native(
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) },
        "Windows Open dialog allocation failed",
    )?;
    // SAFETY: typed current-STA object; preserve native defaults.
    let mut flags = native(
        unsafe { dialog.GetOptions() },
        "Windows Open options failed",
    )? | FOS_FORCEFILESYSTEM
        | FOS_NOCHANGEDIR
        | FOS_FILEMUSTEXIST;
    if options.properties.contains(&OpenProperty::OpenDirectory) {
        flags |= FOS_PICKFOLDERS;
    }
    if options.properties.contains(&OpenProperty::MultiSelections) {
        flags |= FOS_ALLOWMULTISELECT;
    }
    // SAFETY: bounded admitted flags; no pointer parameters or ownership transfer.
    native(
        unsafe { dialog.SetOptions(flags) },
        "Windows Open option setup failed",
    )?;
    configure(&dialog, &options.filters, options.default_path.as_deref())?;
    let selected = modal(parent, &dialog, cancel)?;
    if cancel.check() {
        return cancel.finish(None);
    }
    let mut file_paths = Vec::new();
    if selected {
        // SAFETY: Show returned success; this STA still owns the dialog.
        let items = native(
            unsafe { dialog.GetResults() },
            "Windows Open results failed",
        )?;
        // SAFETY: live typed current-STA array; bound before allocating/iteration.
        let count = native(
            unsafe { items.GetCount() },
            "Windows Open result count failed",
        )?;
        if count == 0 || count as usize > MAX_DIALOG_PATHS {
            return Err(invalid("Windows Open result count exceeds shared bounds"));
        }
        for index in 0..count {
            // SAFETY: index is within this retained array's reported bound.
            let item = native(
                unsafe { items.GetItemAt(index) },
                "Windows Open item failed",
            )?;
            file_paths.push(path(&item)?);
        }
    }
    cancel.finish(Some(DialogReplyV1::Open {
        canceled: !selected,
        file_paths,
    }))
}

#[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
pub(super) fn save(
    parent: HWND,
    options: &SaveDialogOptions,
    cancel: &Rc<Cancellation>,
) -> Result<Option<DialogReplyV1>, IpcError> {
    validate_save_options(options)?;
    // SAFETY: show checked the existing Host STA; the typed object never leaves.
    let dialog: IFileSaveDialog = native(
        unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER) },
        "Windows Save dialog allocation failed",
    )?;
    // SAFETY: live typed current-STA object; preserve native default options.
    let flags = native(
        unsafe { dialog.GetOptions() },
        "Windows Save options failed",
    )? | FOS_FORCEFILESYSTEM
        | FOS_NOCHANGEDIR
        | FOS_PATHMUSTEXIST
        | FOS_OVERWRITEPROMPT
        | FOS_NOTESTFILECREATE;
    // SAFETY: admitted flags only. NOTESTFILECREATE prevents the native write test.
    native(
        unsafe { dialog.SetOptions(flags) },
        "Windows Save option setup failed",
    )?;
    configure(&dialog, &options.filters, options.default_path.as_deref())?;
    let selected = modal(parent, &dialog, cancel)?;
    if cancel.check() {
        return cancel.finish(None);
    }
    let file_path = if selected {
        // SAFETY: result is requested only after successful Show on its STA.
        path(&native(
            unsafe { dialog.GetResult() },
            "Windows Save result failed",
        )?)?
    } else {
        String::new()
    };
    cancel.finish(Some(DialogReplyV1::Save {
        canceled: !selected,
        file_path,
    }))
}
