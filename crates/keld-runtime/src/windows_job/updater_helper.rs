//! Win32 elevated updater helper launch, loader hardening and COM lifetime.
//! This private module retains the original parent public API through re-exports.

use std::ffi::OsStr;
use std::io;
use std::marker::PhantomData;
use std::os::windows::io::{
    AsHandle, AsRawHandle as _, BorrowedHandle, FromRawHandle as _, OwnedHandle,
};

use keld_ipc::WindowsNamedPipeBootstrapStream;
use windows_sys::Win32::Foundation::{ERROR_CANCELLED, HANDLE, S_FALSE, S_OK};
use windows_sys::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::System::LibraryLoader::{
    LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
};
use windows_sys::Win32::System::Threading::GetProcessId;
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};

use super::WindowsUpdaterHelperPath;
use crate::windows_lpac::nul_terminated_wide;

/// Failure to restrict this process's DLL search to System32
/// (`KELD-RUNTIME-018`).
#[derive(Debug)]
pub struct WindowsDllSearchError {
    pub(super) source: io::Error,
}

impl std::fmt::Display for WindowsDllSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-RUNTIME-018: restricting this process's DLL search to System32 failed: {}. \
             Exit before loading any library or doing protected work; never continue on the \
             standard search path, which includes the current directory and PATH.",
            self.source
        )
    }
}

impl std::error::Error for WindowsDllSearchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Restricts every later DLL search of this process to System32.
///
/// This is the updater helper's first statement (KEL-53 §4 "Helper launch and
/// self-anchor"): `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`.
/// Microsoft documents the resulting search path as process-wide, lasting for
/// the life of the process and impossible to revert to the standard search
/// path. Afterwards a `LoadLibraryW` by module name searches none of the
/// application directory, the current directory and `PATH`. It does not cover
/// the static imports that the loader resolved before `main`.
///
/// # Errors
///
/// Returns [`WindowsDllSearchError`] when Windows refuses the call; the caller
/// must then exit without loading anything.
pub fn restrict_dll_search_to_system32() -> Result<(), WindowsDllSearchError> {
    // SAFETY: SetDefaultDllDirectories takes one flags value and no pointer;
    // LOAD_LIBRARY_SEARCH_SYSTEM32 is a documented flag, and the call changes
    // only this process's loader search path.
    if unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) } == 0 {
        return Err(WindowsDllSearchError {
            source: io::Error::last_os_error(),
        });
    }
    Ok(())
}

/// The updater helper's fixed recovery-role selector (KEL-53 §4 "Helper launch
/// and self-anchor"; Coordination record, Linear KEL-270 comment
/// `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07).
///
/// Exact ASCII and case-sensitive. It is the helper's only accepted argument
/// besides an attempt rendezvous name, and the helper's argument parser imports
/// this constant rather than restating it. It is a cross-version argument
/// contract: a host may start an older tree's helper for recovery.
pub const WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR: &str = "--recovery-role";

/// The updater helper's single argument, from a closed set (KEL-53 §4
/// "Machine-UAC bootstrap" items 1 and 3).
///
/// It is either the activation role's bootstrap rendezvous name, exactly
/// `\\.\pipe\keld-attempt-<64 lowercase hex>`, or
/// [`WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR`]. Neither conveys authority.
/// Both are a single command-line token, because neither contains whitespace
/// or a quotation mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsUpdaterHelperArgument {
    role: UpdaterHelperRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpdaterHelperRole {
    Activation { rendezvous: String },
    Recovery,
}

impl WindowsUpdaterHelperArgument {
    /// The activation role's argument: the name of the bootstrap endpoint that
    /// the host created before the launch.
    ///
    /// # Errors
    ///
    /// Returns [`WindowsUpdaterHelperLaunchError::ArgumentShape`] unless
    /// `rendezvous` passes keld-ipc's
    /// [`WindowsNamedPipeBootstrapStream::is_attempt_endpoint`], the one owner
    /// of that shape.
    pub fn activation(rendezvous: &str) -> Result<Self, WindowsUpdaterHelperLaunchError> {
        if !WindowsNamedPipeBootstrapStream::is_attempt_endpoint(rendezvous) {
            return Err(WindowsUpdaterHelperLaunchError::ArgumentShape);
        }
        Ok(Self {
            role: UpdaterHelperRole::Activation {
                rendezvous: rendezvous.to_owned(),
            },
        })
    }

    /// The recovery role's argument,
    /// [`WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR`].
    #[must_use]
    pub const fn recovery() -> Self {
        Self {
            role: UpdaterHelperRole::Recovery,
        }
    }

    /// The exact argument text the helper receives.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.role {
            UpdaterHelperRole::Activation { rendezvous } => rendezvous,
            UpdaterHelperRole::Recovery => WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR,
        }
    }
}

/// Why an elevated updater-helper launch refused or failed
/// (`KELD-RUNTIME-019`).
///
/// No variant retains a helper process handle. [`Self::HelperPath`],
/// [`Self::ArgumentShape`] and [`Self::ComInitialization`] refuse before the
/// shell is called, and [`Self::Declined`] means the user refused consent, so
/// no helper started. After [`Self::NoProcessHandle`] or
/// [`Self::ProcessIdentity`] an elevated helper may already be running, unbound
/// to this launch: the caller must close its bootstrap endpoint, so that the
/// helper's server check refuses and it exits without a protected write
/// (KEL-53 §4 "Machine-UAC bootstrap" item 3; S11 composes this).
#[derive(Debug)]
pub enum WindowsUpdaterHelperLaunchError {
    /// The helper path is not a Win32-canonical drive-letter `.exe` path.
    HelperPath {
        /// The first rule the path breaks.
        rule: &'static str,
    },
    /// The rendezvous argument is not an exact `keld-attempt` name.
    ArgumentShape,
    /// The dedicated launch thread could not start, or ended without a result.
    LaunchThread {
        /// What the thread reported.
        source: io::Error,
    },
    /// `CoInitializeEx` refused the launch thread's single-threaded apartment.
    ComInitialization {
        /// The `HRESULT` it returned.
        hresult: i32,
    },
    /// The user declined the UAC prompt: `ShellExecuteExW` failed with
    /// `ERROR_CANCELLED`.
    Declined,
    /// `ShellExecuteExW` failed with another error.
    ShellExecute {
        /// The Windows error.
        source: io::Error,
        /// The `hInstApp` value, an `SE_ERR_*` code or `0`, kept as a
        /// diagnostic.
        inst_app: usize,
    },
    /// `ShellExecuteExW` succeeded without returning a process handle, so no
    /// launched process can be bound to the launch. A helper may still be
    /// running.
    NoProcessHandle,
    /// The returned handle could not report its process ID; it was closed. The
    /// helper may still be running.
    ProcessIdentity {
        /// The Windows error.
        source: io::Error,
    },
}

impl std::fmt::Display for WindowsUpdaterHelperLaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KELD-RUNTIME-019: elevated updater-helper launch refused: ")?;
        match self {
            Self::HelperPath { rule } => write!(
                f,
                "the helper path is not a Win32-canonical drive-letter `.exe` path: {rule}. \
                 Derive it again \
                 from the selected tree's admitted provenance; never pass a searched, relative \
                 or caller-supplied name."
            ),
            Self::ArgumentShape => f.write_str(
                "the rendezvous argument is not exactly `\\\\.\\pipe\\keld-attempt-<64 \
                 lowercase hex>`. Pass the name of the bootstrap endpoint this host created, \
                 unchanged.",
            ),
            Self::LaunchThread { source } => write!(
                f,
                "the dedicated launch thread failed: {source}. Free process resources and \
                 offer the elevated action again only on a new user request."
            ),
            Self::ComInitialization { hresult } => write!(
                f,
                "COM refused the launch thread's single-threaded apartment (HRESULT \
                 {hresult:#010x}). No consent was requested; report the HRESULT and never \
                 launch the helper without COM initialized."
            ),
            Self::Declined => f.write_str(
                "the user declined the UAC prompt. Nothing was launched and no protected state \
                 changed; offer the elevated action again only on a new user request.",
            ),
            Self::ShellExecute { source, inst_app } => write!(
                f,
                "ShellExecuteExW failed: {source} (hInstApp {inst_app}). No helper process is \
                 retained; if the image is missing, repair the installation rather than \
                 searching for another helper."
            ),
            Self::NoProcessHandle => f.write_str(
                "ShellExecuteExW returned no process handle, so no connecting client can be \
                 bound to the launch. A helper may already be running: close the bootstrap \
                 endpoint so that its server check refuses and it exits without a protected \
                 write, and report the detail.",
            ),
            Self::ProcessIdentity { source } => write!(
                f,
                "the returned process handle could not report its process ID: {source}. The \
                 handle was closed, so no connecting client can be bound to the launch. A \
                 helper may already be running: close the bootstrap endpoint so that its server \
                 check refuses and it exits without a protected write, and report the detail."
            ),
        }
    }
}

impl std::error::Error for WindowsUpdaterHelperLaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::LaunchThread { source }
            | Self::ShellExecute { source, .. }
            | Self::ProcessIdentity { source } => Some(source),
            _ => None,
        }
    }
}

/// The elevated updater helper that [`launch_elevated_updater_helper`]
/// started.
///
/// It owns the process handle that `ShellExecuteExW` returned under
/// `SEE_MASK_NOCLOSEPROCESS` and closes it on drop; dropping it does not
/// terminate the helper. Microsoft does not document which access rights that
/// handle carries after a `runas` elevation; KEL-53 §6 S1 qualifies them.
#[derive(Debug)]
pub struct WindowsElevatedUpdaterHelper {
    process: OwnedHandle,
    process_id: u32,
}

impl WindowsElevatedUpdaterHelper {
    /// The helper's process ID, read from the retained handle at launch.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.process_id
    }

    /// Retains `process` with the process ID it reports, or closes it when it
    /// cannot report one.
    pub(super) fn identify(process: OwnedHandle) -> Result<Self, WindowsUpdaterHelperLaunchError> {
        // SAFETY: `process` owns a live handle for the duration of this call.
        let process_id = unsafe { GetProcessId(process.as_raw_handle().cast()) };
        if process_id == 0 {
            return Err(WindowsUpdaterHelperLaunchError::ProcessIdentity {
                source: io::Error::last_os_error(),
            });
        }
        Ok(Self {
            process,
            process_id,
        })
    }
}

impl AsHandle for WindowsElevatedUpdaterHelper {
    fn as_handle(&self) -> BorrowedHandle<'_> {
        self.process.as_handle()
    }
}

/// Starts `path` elevated through the UAC `runas` verb with exactly one
/// argument (KEL-53 §4 "Helper launch and self-anchor").
///
/// The call runs on a dedicated thread that first enters a COM single-threaded
/// apartment with `COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE`, as the
/// `ShellExecuteEx` remarks recommend, and leaves it on that thread before the
/// thread ends. `ShellExecuteExW` receives:
/// - `SEE_MASK_NOCLOSEPROCESS`, so it returns the process handle that the
///   result owns;
/// - `SEE_MASK_NOASYNC`, which the `SHELLEXECUTEINFO` remarks require when the
///   calling thread has no message loop or ends soon after the call;
/// - `SEE_MASK_FLAG_NO_UI`, so a failure returns its error instead of showing
///   an error dialog; the UAC prompt is a security prompt and is still shown;
/// - the helper's own directory as `lpDirectory`, because a null value passes
///   on this process's current directory, which an ordinary user may choose;
/// - no owner window and an `nShow` of `0` (`SW_HIDE`): the helper shows no
///   window.
///
/// The call blocks until the user answers the prompt. It searches for nothing:
/// the path and the argument are closed types.
///
/// # Errors
///
/// - [`WindowsUpdaterHelperLaunchError::Declined`] when the user declines;
/// - [`WindowsUpdaterHelperLaunchError::ComInitialization`] before the shell
///   is called, when the launch thread's apartment is refused;
/// - [`WindowsUpdaterHelperLaunchError::ShellExecute`],
///   [`WindowsUpdaterHelperLaunchError::NoProcessHandle`] and
///   [`WindowsUpdaterHelperLaunchError::ProcessIdentity`] when the launch
///   cannot produce one identified process handle;
/// - [`WindowsUpdaterHelperLaunchError::LaunchThread`] when the thread cannot
///   start or ends without a result.
pub fn launch_elevated_updater_helper(
    path: &WindowsUpdaterHelperPath,
    argument: &WindowsUpdaterHelperArgument,
) -> Result<WindowsElevatedUpdaterHelper, WindowsUpdaterHelperLaunchError> {
    launch_updater_helper(path, argument, ShellVerb::RunAs)
}

/// The launch with its verb as a parameter. Production builds have only
/// [`ShellVerb::RunAs`]; a unit test drives the same block with `open`, which
/// needs no consent prompt.
pub(super) fn launch_updater_helper(
    path: &WindowsUpdaterHelperPath,
    argument: &WindowsUpdaterHelperArgument,
    verb: ShellVerb,
) -> Result<WindowsElevatedUpdaterHelper, WindowsUpdaterHelperLaunchError> {
    let file = path.file.clone();
    let directory = path.directory.clone();
    let parameters = nul_terminated_wide(OsStr::new(argument.as_str()))
        .ok_or(WindowsUpdaterHelperLaunchError::ArgumentShape)?;
    let launch = std::thread::Builder::new()
        .name("keld-updater-helper-launch".to_owned())
        .spawn(move || shell_execute_on_this_thread(&file, &parameters, &directory, verb))
        .map_err(|source| WindowsUpdaterHelperLaunchError::LaunchThread { source })?;
    let process = launch
        .join()
        .map_err(|_| WindowsUpdaterHelperLaunchError::LaunchThread {
            source: io::Error::other("the launch thread ended without a result"),
        })??;
    WindowsElevatedUpdaterHelper::identify(process)
}

/// Makes the one `ShellExecuteExW` call inside a COM apartment that the
/// current thread enters first and leaves after the call, and returns the
/// process handle.
fn shell_execute_on_this_thread(
    file: &[u16],
    parameters: &[u16],
    directory: &[u16],
    verb: ShellVerb,
) -> Result<OwnedHandle, WindowsUpdaterHelperLaunchError> {
    let _apartment = ComApartment::enter()?;
    let verb: Vec<u16> = verb
        .name()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let size = u32::try_from(std::mem::size_of::<SHELLEXECUTEINFOW>()).map_err(|_| {
        WindowsUpdaterHelperLaunchError::ShellExecute {
            source: io::Error::other("SHELLEXECUTEINFOW does not fit its u32 size field"),
            inst_app: 0,
        }
    })?;
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: parameters.as_ptr(),
        lpDirectory: directory.as_ptr(),
        // 0 is SW_HIDE (WinUser.h); windows-sys names it only behind
        // Win32_UI_WindowsAndMessaging, which this crate does not enable.
        nShow: 0,
        ..SHELLEXECUTEINFOW::default()
    };
    // SAFETY: `info` is a live, writable SHELLEXECUTEINFOW whose cbSize is its
    // exact size. Its four string fields point at NUL-terminated UTF-16 buffers
    // (`verb` here; `file`, `parameters` and `directory` borrowed from the
    // caller) that live until this function returns; SEE_MASK_NOASYNC makes the
    // call finish its work before it returns, so nothing reads them later.
    // Every other field is zero: no owner window, ID list, class or hot key.
    let succeeded = unsafe { ShellExecuteExW(&raw mut info) } != 0;
    // Read at once: nothing may run between the call and its last-error value.
    let error = io::Error::last_os_error();
    let process = shell_execute_outcome(succeeded, error, info.hInstApp.addr(), info.hProcess)?;
    // SAFETY: `shell_execute_outcome` admits only a successful call's non-null
    // hProcess, which SEE_MASK_NOCLOSEPROCESS makes this caller's to close;
    // nothing else holds it, so it moves into exactly one owner here.
    Ok(unsafe { OwnedHandle::from_raw_handle(process.cast()) })
}

/// Classifies one finished `ShellExecuteExW` call: a failure with
/// `ERROR_CANCELLED` is the user's decline, any other failure is reported with
/// its `hInstApp` diagnostic, and a success counts only with a process handle.
pub(super) fn shell_execute_outcome(
    succeeded: bool,
    error: io::Error,
    inst_app: usize,
    process: HANDLE,
) -> Result<HANDLE, WindowsUpdaterHelperLaunchError> {
    if !succeeded {
        return Err(
            if error.raw_os_error() == Some(ERROR_CANCELLED.cast_signed()) {
                WindowsUpdaterHelperLaunchError::Declined
            } else {
                WindowsUpdaterHelperLaunchError::ShellExecute {
                    source: error,
                    inst_app,
                }
            },
        );
    }
    if process.is_null() {
        return Err(WindowsUpdaterHelperLaunchError::NoProcessHandle);
    }
    Ok(process)
}

/// The current thread's COM apartment, entered as `COINIT_APARTMENTTHREADED |
/// COINIT_DISABLE_OLE1DDE`.
///
/// A value exists only after `CoInitializeEx` returned `S_OK` or `S_FALSE`,
/// each of which Microsoft requires to be balanced by one `CoUninitialize` on
/// the same thread; dropping the value makes that call. It is neither `Send`
/// nor `Sync`, so it is dropped on the thread that entered. After
/// `RPC_E_CHANGED_MODE` or any other failure there is nothing to balance and no
/// value exists.
#[derive(Debug)]
pub(super) struct ComApartment {
    _thread_bound: PhantomData<*const ()>,
}

impl ComApartment {
    pub(super) fn enter() -> Result<Self, WindowsUpdaterHelperLaunchError> {
        // SAFETY: the reserved first argument is null, as CoInitializeEx
        // requires, the flags are documented COINIT values, and the call reads
        // no caller memory.
        let hresult = unsafe {
            CoInitializeEx(
                std::ptr::null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).cast_unsigned(),
            )
        };
        if hresult == S_OK || hresult == S_FALSE {
            Ok(Self {
                _thread_bound: PhantomData,
            })
        } else {
            Err(WindowsUpdaterHelperLaunchError::ComInitialization { hresult })
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: this value exists only after a successful CoInitializeEx on
        // this thread, cannot leave the thread, and is dropped once, so this is
        // that call's single balancing CoUninitialize.
        unsafe { CoUninitialize() };
    }
}

/// The `ShellExecuteExW` verb. Production builds have only `runas`, so no
/// public path can select another verb.
#[derive(Debug, Clone, Copy)]
pub(super) enum ShellVerb {
    /// UAC elevation: the only production verb.
    RunAs,
    /// Ordinary activation without a consent prompt, for the unit test that
    /// executes the launch block.
    #[cfg(test)]
    Open,
}

impl ShellVerb {
    const fn name(self) -> &'static str {
        match self {
            Self::RunAs => "runas",
            #[cfg(test)]
            Self::Open => "open",
        }
    }
}
