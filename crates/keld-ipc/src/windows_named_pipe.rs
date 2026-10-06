//! Windows named-pipe handle and overlapped-I/O ownership.
//!
//! This module owns the Win32 ABI boundary only. Bootstrap token parsing,
//! frame decoding, authentication, and rejection policy remain in safe shared
//! modules.

#![deny(unsafe_op_in_unsafe_fn)]
#![allow(unsafe_code)] // KEL-101-sanctioned Win32 pipe/overlapped ABI owner

use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};
#[cfg(test)]
use std::{cell::RefCell, sync::mpsc};

use windows_permissions::constants::{AceFlags, AceType, SeObjectType, SecurityInformation};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{ConvertSidToStringSid, GetSecurityInfo};
use windows_permissions::{LocalBox, SecurityDescriptor, Sid};
use windows_sys::Win32::Foundation::{
    ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, ERROR_PIPE_CONNECTED, ERROR_SEM_TIMEOUT,
    GetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, LUID, WAIT_FAILED,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::{
    GetLengthSid, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, IsValidSid,
    RevertToSelf, TOKEN_ELEVATION_TYPE, TOKEN_INFORMATION_CLASS, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TOKEN_STATISTICS, TOKEN_USER, TokenElevation, TokenElevationType,
    TokenElevationTypeDefault, TokenElevationTypeFull, TokenElevationTypeLimited,
    TokenIntegrityLevel, TokenSessionId, TokenStatistics, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_CREATE_PIPE_INSTANCE, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
    GetNamedPipeClientSessionId, GetNamedPipeInfo, GetNamedPipeServerProcessId,
    GetNamedPipeServerSessionId, ImpersonateNamedPipeClient, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentThread, INFINITE, OpenThreadToken, ResetEvent, SetEvent,
    WaitForMultipleObjects, WaitForSingleObject,
};
#[cfg(test)]
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessHandleCount, OpenProcessToken,
};

const PIPE_BUFFER_BYTES: u32 = 64 * 1024;
/// The landed `keld-ipc` pipe grant: read/write data, attributes and extended
/// attributes, `READ_CONTROL` and `SYNCHRONIZE`; never `FILE_CREATE_PIPE_INSTANCE`,
/// `WRITE_DAC` or `WRITE_OWNER`.
pub(crate) const PIPE_ACCESS_MASK: u32 = 0x0012_019B;

#[cfg(test)]
thread_local! {
    static CONNECT_BUSY_WITNESS: RefCell<Option<mpsc::Sender<()>>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum WaitOutcome {
    Ready,
    PeerClosed,
    Cancelled,
    DeadlineElapsed,
}

#[derive(Debug)]
struct ServerInner {
    pipe: Mutex<Option<OwnedHandle>>,
    lifecycle: Mutex<()>,
    cancel_event: OwnedHandle,
    connect_event: OwnedEvent,
    connected: AtomicBool,
    consumed: AtomicBool,
    #[cfg(test)]
    accept_pending: AtomicBool,
    #[cfg(test)]
    active_stream_io: AtomicUsize,
    #[cfg(test)]
    force_cancel_error: AtomicBool,
}

/// One host-owned named-pipe instance.
#[derive(Debug, Clone)]
pub(crate) struct WindowsNamedPipeServer {
    inner: Arc<ServerInner>,
}

/// Non-owning cancellation view; it cannot keep a stale pipe alive.
#[derive(Debug, Clone)]
pub(crate) struct WindowsNamedPipeCanceller {
    inner: Weak<ServerInner>,
}

/// Connected server end of the named pipe.
#[derive(Debug)]
pub(crate) struct WindowsNamedPipeStream {
    inner: Arc<ServerInner>,
    local_is_server: bool,
    read_event: OwnedEvent,
    write_event: OwnedEvent,
    read_timeout: Mutex<Option<Duration>>,
    write_timeout: Mutex<Option<Duration>>,
    absolute_deadline: Mutex<Option<Instant>>,
}

/// Exact security facts read back from the live pipe handle.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PipeSecurityFacts {
    pub(crate) protected_dacl: bool,
    pub(crate) ace_count: usize,
    pub(crate) one_ace_is_current_user: bool,
    pub(crate) one_ace_type: u8,
    pub(crate) one_ace_flags: u8,
    pub(crate) one_ace_mask: u32,
    pub(crate) handle_flags: u32,
    pub(crate) pipe_flags: u32,
}

/// Owner, DACL and mandatory label read back from a live pipe handle, with
/// that handle's inheritance flag and the pipe's `GetNamedPipeInfo` flags.
pub(crate) struct PipeDescriptorReadback {
    pub(crate) descriptor: LocalBox<SecurityDescriptor>,
    pub(crate) handle_inheritable: bool,
    pub(crate) pipe_flags: u32,
}

/// Token facts observed from a process or the writer of a connected local pipe
/// message. The byte SID is the exact Windows SID encoding.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WindowsPeerTokenFacts {
    /// Binary Windows `TokenUser` SID.
    pub user_sid: Vec<u8>,
    /// Session ID from the token.
    pub session_id: u32,
    /// Mandatory integrity RID from the token.
    pub integrity_rid: u32,
    /// The logon session the token represents, `TokenStatistics.AuthenticationId`,
    /// as `(HighPart << 32) | LowPart` (the KEL-53 journal encoding). Many
    /// tokens can represent one logon session.
    pub authentication_id: u64,
    /// Whether `TokenElevation` reports the token as elevated.
    pub elevated: bool,
    /// The token's `TokenElevationType`.
    pub elevation_type: WindowsTokenElevationType,
}

/// A token's elevation type (`TOKEN_ELEVATION_TYPE`). Only the documented
/// values exist; the reader refuses any other.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WindowsTokenElevationType {
    /// `TokenElevationTypeDefault`: the token does not have a linked token.
    Default,
    /// `TokenElevationTypeFull`: the token is an elevated token.
    Full,
    /// `TokenElevationTypeLimited`: the token is a limited token.
    Limited,
}

impl WindowsNamedPipeServer {
    pub(crate) fn bind(endpoint: &str) -> io::Result<Self> {
        let current_sid = current_process_sid()?;
        let descriptor = current_user_descriptor(&current_sid)?;
        let server = Self::bind_with_descriptor(endpoint, &descriptor)?;
        server.validate_security(&current_sid)?;
        Ok(server)
    }

    /// Creates the only, first instance of `endpoint` under `descriptor`, with
    /// remote clients rejected and a non-inheritable handle. The caller reads
    /// the descriptor back before it relies on it.
    pub(crate) fn bind_with_descriptor(
        endpoint: &str,
        descriptor: &LocalBox<SecurityDescriptor>,
    ) -> io::Result<Self> {
        let endpoint_wide = wide(endpoint);
        let attributes_len = u32::try_from(std::mem::size_of::<
            windows_sys::Win32::Security::SECURITY_ATTRIBUTES,
        >())
        .map_err(io::Error::other)?;
        let attributes = windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: attributes_len,
            lpSecurityDescriptor: descriptor.as_ptr().cast(),
            bInheritHandle: 0,
        };
        // SAFETY: `endpoint_wide` is NUL terminated. `attributes` has the
        // correct size and points to the live self-relative descriptor owned
        // by `descriptor`; both outlive this call. Inheritance is disabled.
        let raw_pipe = unsafe {
            CreateNamedPipeW(
                endpoint_wide.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                PIPE_BUFFER_BYTES,
                PIPE_BUFFER_BYTES,
                0,
                &raw const attributes,
            )
        };
        if raw_pipe == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateNamedPipeW returned one new owned handle, checked
        // against INVALID_HANDLE_VALUE, and it is transferred exactly once.
        let pipe = unsafe { OwnedHandle::from_raw_handle(raw_pipe as RawHandle) };

        // SAFETY: null security/name pointers request an unnamed manual-reset
        // event. The returned non-null handle is uniquely owned here.
        let raw_cancel = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if raw_cancel.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateEventW returned one valid owned handle, transferred once.
        let cancel_event = unsafe { OwnedHandle::from_raw_handle(raw_cancel as RawHandle) };
        let connect_event = OwnedEvent::new()?;

        Ok(Self {
            inner: Arc::new(ServerInner {
                pipe: Mutex::new(Some(pipe)),
                lifecycle: Mutex::new(()),
                cancel_event,
                connect_event,
                connected: AtomicBool::new(false),
                consumed: AtomicBool::new(false),
                #[cfg(test)]
                accept_pending: AtomicBool::new(false),
                #[cfg(test)]
                active_stream_io: AtomicUsize::new(0),
                #[cfg(test)]
                force_cancel_error: AtomicBool::new(false),
            }),
        })
    }

    pub(crate) fn accept_until(&self, deadline: Option<Instant>) -> io::Result<WaitOutcome> {
        if self.inner.consumed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "named-pipe bootstrap already consumed",
            ));
        }
        let operation_event = &self.inner.connect_event;
        operation_event.reset()?;
        let mut overlapped = operation_event.overlapped();
        // SAFETY: the pipe was created for overlapped I/O; `overlapped` and
        // its event remain live until completion is observed below.
        let connected_now = unsafe { ConnectNamedPipe(self.raw_pipe()?, &raw mut overlapped) } != 0;
        if connected_now {
            self.inner.connected.store(true, Ordering::Release);
            return Ok(WaitOutcome::Ready);
        }
        match io::Error::last_os_error().raw_os_error() {
            Some(code) if code == ERROR_PIPE_CONNECTED.cast_signed() => {
                self.inner.connected.store(true, Ordering::Release);
                Ok(WaitOutcome::Ready)
            }
            Some(code) if code == ERROR_IO_PENDING.cast_signed() => {
                #[cfg(test)]
                let _pending = PendingAccept::new(&self.inner.accept_pending);
                let outcome = wait_for_operation(
                    self.raw_pipe()?,
                    &mut overlapped,
                    operation_event.raw(),
                    self.raw_cancel_event(),
                    deadline,
                )?;
                match outcome {
                    WaitOutcome::Ready | WaitOutcome::PeerClosed => {
                        self.inner.connected.store(true, Ordering::Release);
                    }
                    WaitOutcome::Cancelled | WaitOutcome::DeadlineElapsed => {
                        self.close_terminal()?;
                    }
                }
                Ok(outcome)
            }
            Some(232 | 233) => {
                self.inner.connected.store(true, Ordering::Release);
                Ok(WaitOutcome::PeerClosed)
            }
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub(crate) fn disconnect_for_retry(&self) -> io::Result<()> {
        let _lifecycle = self
            .inner
            .lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.inner.consumed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "consumed named-pipe bootstrap cannot re-listen",
            ));
        }
        self.cancel_pending_io()?;
        if self.inner.connected.swap(false, Ordering::AcqRel) {
            // SAFETY: this object owns the live server pipe handle; no
            // OVERLAPPED state is reused until cancellation was observed.
            if unsafe { DisconnectNamedPipe(self.raw_pipe()?) } == 0 {
                let error = io::Error::last_os_error();
                if !matches!(error.raw_os_error(), Some(109 | 232 | 233)) {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn stream(&self) -> io::Result<WindowsNamedPipeStream> {
        Ok(WindowsNamedPipeStream {
            inner: Arc::clone(&self.inner),
            local_is_server: true,
            read_event: OwnedEvent::new()?,
            write_event: OwnedEvent::new()?,
            read_timeout: Mutex::new(None),
            write_timeout: Mutex::new(None),
            absolute_deadline: Mutex::new(None),
        })
    }

    pub(crate) fn consume(&self) {
        self.inner.consumed.store(true, Ordering::Release);
    }

    pub(crate) fn cancel(&self) -> io::Result<()> {
        let _lifecycle = self
            .inner
            .lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.inner.consumed.load(Ordering::Acquire) {
            return Ok(());
        }
        // SAFETY: the cancellation event handle remains live in `inner`.
        if unsafe { SetEvent(self.raw_cancel_event()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.cancel_pending_io()
    }

    pub(crate) fn canceller(&self) -> WindowsNamedPipeCanceller {
        WindowsNamedPipeCanceller {
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub(crate) fn security_facts(&self) -> io::Result<PipeSecurityFacts> {
        let pipe = self
            .inner
            .pipe
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let pipe = pipe.as_ref().ok_or_else(closed_pipe_error)?;
        read_security_facts(pipe)
    }

    /// Reads this server instance's owner, DACL and label back from its handle.
    pub(crate) fn descriptor_readback(&self) -> io::Result<PipeDescriptorReadback> {
        let pipe = self
            .inner
            .pipe
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        read_pipe_descriptor(pipe.as_ref().ok_or_else(closed_pipe_error)?)
    }

    pub(crate) fn close_terminal(&self) -> io::Result<()> {
        let _lifecycle = self
            .inner
            .lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.inner.consumed.store(true, Ordering::Release);
        self.cancel_pending_io()?;
        // SAFETY: the server owns the live handle and all pending operations
        // were cancelled; their owners retain state until they observe
        // completion. Disconnect does not free an OVERLAPPED or its buffer.
        if self.inner.connected.swap(false, Ordering::AcqRel)
            && unsafe { DisconnectNamedPipe(self.raw_pipe()?) } == 0
        {
            let error = io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(109 | 232 | 233)) {
                return Err(error);
            }
        }
        drop(
            self.inner
                .pipe
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
        Ok(())
    }

    pub(crate) fn connect_client(endpoint: &str) -> io::Result<WindowsNamedPipeStream> {
        Self::connect_client_with_flags(endpoint, FILE_FLAG_OVERLAPPED)
    }

    /// Opens a client that grants the server at most identification of its
    /// token (`SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`), as the
    /// lifecycle and `keld-attempt` clients require.
    pub(crate) fn connect_identification_client(
        endpoint: &str,
    ) -> io::Result<WindowsNamedPipeStream> {
        Self::connect_client_with_flags(
            endpoint,
            FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
        )
    }

    fn connect_client_with_flags(endpoint: &str, flags: u32) -> io::Result<WindowsNamedPipeStream> {
        let endpoint_wide = wide(endpoint);
        // SAFETY: endpoint_wide is NUL terminated; no security template is
        // supplied; the returned handle is checked and transferred once.
        let raw = unsafe {
            CreateFileW(
                endpoint_wide.as_ptr(),
                PIPE_ACCESS_MASK,
                0,
                ptr::null(),
                OPEN_EXISTING,
                flags,
                ptr::null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateFileW returned a valid newly owned handle.
        let pipe = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };
        let mut pipe_flags = 0_u32;
        // SAFETY: `pipe` owns the live client endpoint and `pipe_flags` is writable.
        if unsafe { GetHandleInformation(pipe.as_raw_handle().cast(), &raw mut pipe_flags) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if pipe_flags & HANDLE_FLAG_INHERIT != 0 {
            return Err(io::Error::other(
                "named-pipe client handle must be non-inheritable",
            ));
        }
        // SAFETY: null security/name pointers request an unnamed manual-reset
        // event. The returned non-null handle is transferred once below.
        let raw_cancel = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if raw_cancel.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateEventW returned a valid newly owned handle.
        let cancel_event = unsafe { OwnedHandle::from_raw_handle(raw_cancel as RawHandle) };
        let connect_event = OwnedEvent::new()?;
        Ok(WindowsNamedPipeStream {
            inner: Arc::new(ServerInner {
                pipe: Mutex::new(Some(pipe)),
                lifecycle: Mutex::new(()),
                cancel_event,
                connect_event,
                connected: AtomicBool::new(true),
                consumed: AtomicBool::new(true),
                #[cfg(test)]
                accept_pending: AtomicBool::new(false),
                #[cfg(test)]
                active_stream_io: AtomicUsize::new(0),
                #[cfg(test)]
                force_cancel_error: AtomicBool::new(false),
            }),
            local_is_server: false,
            read_event: OwnedEvent::new()?,
            write_event: OwnedEvent::new()?,
            read_timeout: Mutex::new(None),
            write_timeout: Mutex::new(None),
            absolute_deadline: Mutex::new(None),
        })
    }

    #[cfg(test)]
    pub(crate) fn is_connected(&self) -> bool {
        self.inner.connected.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn is_accept_pending(&self) -> bool {
        self.inner.accept_pending.load(Ordering::Acquire)
    }

    pub(crate) fn connect_client_until(
        endpoint: &str,
        deadline: Instant,
    ) -> io::Result<WindowsNamedPipeStream> {
        Self::connect_client_until_with(endpoint, deadline, false)
    }

    pub(crate) fn connect_identification_client_until(
        endpoint: &str,
        deadline: Instant,
    ) -> io::Result<WindowsNamedPipeStream> {
        Self::connect_client_until_with(endpoint, deadline, true)
    }

    fn connect_client_until_with(
        endpoint: &str,
        deadline: Instant,
        identification_only: bool,
    ) -> io::Result<WindowsNamedPipeStream> {
        let endpoint_wide = wide(endpoint);
        loop {
            if Instant::now() >= deadline {
                return Err(connect_deadline_error());
            }
            let connected = if identification_only {
                Self::connect_identification_client(endpoint)
            } else {
                Self::connect_client(endpoint)
            };
            match connected {
                Ok(stream) => {
                    if Instant::now() >= deadline {
                        drop(stream);
                        return Err(connect_deadline_error());
                    }
                    return Ok(stream);
                }
                Err(error) if error.raw_os_error() == Some(231) => {
                    #[cfg(test)]
                    CONNECT_BUSY_WITNESS.with(|witness| {
                        if let Some(witness) = witness.borrow_mut().take() {
                            let _ = witness.send(());
                        }
                    });
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        return Err(connect_deadline_error());
                    };
                    let wait_ms = duration_to_wait_ms(Some(remaining));
                    // SAFETY: `endpoint_wide` is a live NUL-terminated path;
                    // the bounded wait does not retain its pointer.
                    if unsafe { WaitNamedPipeW(endpoint_wide.as_ptr(), wait_ms) } == 0 {
                        let wait_error = io::Error::last_os_error();
                        if wait_error.raw_os_error() == Some(ERROR_SEM_TIMEOUT.cast_signed()) {
                            if Instant::now() < deadline {
                                continue;
                            }
                            return Err(connect_deadline_error());
                        }
                        return Err(wait_error);
                    }
                }
                Err(error) => return Err(error),
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn install_connect_busy_witness(witness: mpsc::Sender<()>) {
        CONNECT_BUSY_WITNESS.with(|slot| *slot.borrow_mut() = Some(witness));
    }

    fn validate_security(&self, current_sid: &Sid) -> io::Result<()> {
        let facts = self.security_facts()?;
        if !facts.protected_dacl
            || facts.ace_count != 1
            || !facts.one_ace_is_current_user
            || facts.one_ace_type != AceType::ACCESS_ALLOWED_ACE_TYPE as u8
            || facts.one_ace_flags != AceFlags::empty().bits()
            || facts.one_ace_mask != PIPE_ACCESS_MASK
            || facts.one_ace_mask & FILE_CREATE_PIPE_INSTANCE != 0
            || facts.handle_flags & HANDLE_FLAG_INHERIT != 0
            || facts.pipe_flags & PIPE_REJECT_REMOTE_CLIENTS == 0
        {
            return Err(io::Error::other(format!(
                "named-pipe security readback did not match the current-user-only contract: {facts:?}"
            )));
        }
        let descriptor = GetSecurityInfo(
            self.inner
                .pipe
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
                .ok_or_else(closed_pipe_error)?,
            SeObjectType::SE_KERNEL_OBJECT,
            SecurityInformation::Dacl,
        )?;
        let ace_sid = descriptor
            .dacl()
            .and_then(|dacl| dacl.get_ace(0))
            .and_then(|ace| ace.sid());
        if ace_sid != Some(current_sid) {
            return Err(io::Error::other(
                "named-pipe DACL ACE does not equal current TokenUser SID",
            ));
        }
        Ok(())
    }

    fn cancel_pending_io(&self) -> io::Result<()> {
        #[cfg(test)]
        if self.inner.force_cancel_error.load(Ordering::Acquire) {
            return Err(io::Error::from_raw_os_error(5));
        }
        // SAFETY: null OVERLAPPED cancels all operations issued by this
        // process on the owned pipe. Each operation owner subsequently waits
        // for and observes its own completion before freeing state.
        let Ok(pipe) = self.raw_pipe() else {
            return Ok(());
        };
        if unsafe { CancelIoEx(pipe, ptr::null()) } == 0 {
            let error = io::Error::last_os_error();
            // ERROR_NOT_FOUND means no matching operation remained pending.
            if error.raw_os_error() != Some(1168) {
                return Err(error);
            }
        }
        Ok(())
    }

    fn raw_pipe(&self) -> io::Result<*mut core::ffi::c_void> {
        self.inner
            .pipe
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(AsRawHandle::as_raw_handle)
            .ok_or_else(closed_pipe_error)
    }

    #[cfg(test)]
    pub(crate) fn inspect_owned_pipe<T>(
        &self,
        inspect: impl FnOnce(&OwnedHandle) -> io::Result<T>,
    ) -> io::Result<T> {
        let pipe = self
            .inner
            .pipe
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        inspect(pipe.as_ref().ok_or_else(closed_pipe_error)?)
    }

    fn raw_cancel_event(&self) -> *mut core::ffi::c_void {
        self.inner.cancel_event.as_raw_handle()
    }
}

impl WindowsNamedPipeCanceller {
    pub(crate) fn empty() -> Self {
        Self { inner: Weak::new() }
    }

    pub(crate) fn cancel(&self) -> io::Result<()> {
        let Some(inner) = self.inner.upgrade() else {
            return Ok(());
        };
        WindowsNamedPipeServer { inner }.cancel()
    }
}

impl WindowsNamedPipeStream {
    pub(crate) fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            inner: Arc::clone(&self.inner),
            local_is_server: self.local_is_server,
            read_event: OwnedEvent::new()?,
            write_event: OwnedEvent::new()?,
            read_timeout: Mutex::new(
                *self
                    .read_timeout
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            ),
            write_timeout: Mutex::new(
                *self
                    .write_timeout
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            ),
            absolute_deadline: Mutex::new(
                *self
                    .absolute_deadline
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            ),
        })
    }

    pub(crate) fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        validate_timeout(timeout)?;
        *self
            .read_timeout
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = timeout;
        Ok(())
    }

    pub(crate) fn peer_process_id(&self) -> io::Result<u32> {
        let pipe = self.raw_pipe()?;
        let mut pid = 0_u32;
        // SAFETY: the connected pipe handle stays owned by `self`, and `pid` is
        // writable storage. The selected API queries the opposite endpoint.
        let succeeded = unsafe {
            if self.local_is_server {
                GetNamedPipeClientProcessId(pipe, &raw mut pid)
            } else {
                GetNamedPipeServerProcessId(pipe, &raw mut pid)
            }
        };
        if succeeded == 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            return Err(io::Error::other("named pipe peer PID is zero"));
        }
        Ok(pid)
    }

    /// Reads the pipe's owner, DACL and label back through this endpoint's
    /// handle. A client handle carries `READ_CONTROL` in [`PIPE_ACCESS_MASK`].
    pub(crate) fn descriptor_readback(&self) -> io::Result<PipeDescriptorReadback> {
        let pipe = self
            .inner
            .pipe
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        read_pipe_descriptor(pipe.as_ref().ok_or_else(closed_pipe_error)?)
    }

    pub(crate) fn peer_session_id(&self) -> io::Result<u32> {
        let pipe = self.raw_pipe()?;
        let mut session_id = 0_u32;
        // SAFETY: the connected pipe handle stays owned by `self`, and the
        // output is writable storage. Query the opposite endpoint's session.
        let succeeded = unsafe {
            if self.local_is_server {
                GetNamedPipeClientSessionId(pipe, &raw mut session_id)
            } else {
                GetNamedPipeServerSessionId(pipe, &raw mut session_id)
            }
        };
        if succeeded == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(session_id)
    }

    pub(crate) fn is_inheritable(&self) -> io::Result<bool> {
        let pipe = self.raw_pipe()?;
        let mut flags = 0_u32;
        // SAFETY: pipe remains owned by the connected stream and flags is writable.
        if unsafe { GetHandleInformation(pipe, &raw mut flags) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(flags & HANDLE_FLAG_INHERIT != 0)
    }

    /// Impersonates the client that wrote the last frame and snapshots its
    /// token facts through [`query_windows_peer_token_facts`], always reverting
    /// before returning. This is available only on a connected server end.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the endpoint is a client, impersonation fails,
    /// token query fails or the returned SID buffer is malformed.
    pub(crate) fn last_client_token_facts(&self) -> io::Result<WindowsPeerTokenFacts> {
        if !self.local_is_server {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "client token facts require the local server endpoint",
            ));
        }
        let pipe = self.raw_pipe()?;
        // SAFETY: `pipe` is the retained connected server endpoint; Windows
        // impersonates the context of the client that wrote its last message.
        if unsafe { ImpersonateNamedPipeClient(pipe) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let facts = (|| {
            let mut raw_token = ptr::null_mut();
            // SAFETY: GetCurrentThread is a pseudo-handle for this synchronous
            // thread, and raw_token is writable HANDLE storage.
            if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut raw_token) }
                == 0
            {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: OpenThreadToken returned one fresh non-null owning token handle.
            let token = unsafe { OwnedHandle::from_raw_handle(raw_token.cast()) };
            query_windows_peer_token_facts(&token)
        })();
        // Revert even when opening/querying the client token failed.
        // SAFETY: this thread successfully impersonated the connected client above.
        abort_if_revert_failed(unsafe { RevertToSelf() } != 0);
        facts
    }

    pub(crate) fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        validate_timeout(timeout)?;
        *self
            .write_timeout
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = timeout;
        Ok(())
    }

    pub(crate) fn read_timeout(&self) -> Option<Duration> {
        *self
            .read_timeout
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn write_timeout(&self) -> Option<Duration> {
        *self
            .write_timeout
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn set_absolute_deadline(&self, deadline: Option<Instant>) {
        *self
            .absolute_deadline
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = deadline;
    }

    pub(crate) fn shutdown(&self) -> io::Result<()> {
        let server = WindowsNamedPipeServer {
            inner: Arc::clone(&self.inner),
        };
        let _lifecycle = self
            .inner
            .lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let cancel_error = server.cancel_pending_io().err();
        // SAFETY: this is the live server pipe handle. Cancelling does not
        // free another operation's OVERLAPPED or buffer; each operation owner
        // retains and observes its own state. Disconnect only breaks the pipe
        // connection so peer and local waiters can finish.
        let disconnect_error = if self.inner.connected.swap(false, Ordering::AcqRel) {
            match server.raw_pipe() {
                Ok(pipe) if unsafe { DisconnectNamedPipe(pipe) } == 0 => {
                    let error = io::Error::last_os_error();
                    (!matches!(error.raw_os_error(), Some(109 | 232 | 233))).then_some(error)
                }
                Ok(_) => None,
                Err(error) => Some(error),
            }
        } else {
            None
        };
        disconnect_error.or(cancel_error).map_or(Ok(()), Err)
    }

    #[cfg(test)]
    pub(crate) fn has_active_io(&self) -> bool {
        self.inner.active_stream_io.load(Ordering::Acquire) != 0
    }

    #[cfg(test)]
    pub(crate) fn force_cancel_error(&self) {
        self.inner.force_cancel_error.store(true, Ordering::Release);
    }

    fn raw_pipe(&self) -> io::Result<*mut core::ffi::c_void> {
        self.inner
            .pipe
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(AsRawHandle::as_raw_handle)
            .ok_or_else(closed_pipe_error)
    }
}

/// Reads stable user, session, integrity, logon-session and elevation facts
/// from an owned Windows access token, primary or impersonation (including
/// identification-level). The token must have `TOKEN_QUERY` access; the
/// function returns copied values and retains no pointer into the token
/// buffers.
///
/// # Errors
///
/// Returns an I/O error if a token query fails, Windows returns malformed,
/// truncated or out-of-buffer SID data, a fixed-size class returns any other
/// size, or `TokenElevationType` is not one of its documented values.
pub fn query_windows_peer_token_facts(token: &OwnedHandle) -> io::Result<WindowsPeerTokenFacts> {
    let raw_token = token.as_raw_handle().cast();
    let (user_buffer, user_length) = token_information_buffer(raw_token, TokenUser)?;
    if user_length < std::mem::size_of::<TOKEN_USER>() {
        return Err(io::Error::other(
            "TokenUser buffer is shorter than its header",
        ));
    }
    let user = user_buffer.as_ptr().cast::<TOKEN_USER>();
    // SAFETY: GetTokenInformation populated an aligned buffer and the returned
    // length was checked against TOKEN_USER.
    let user_sid = unsafe { (*user).User.Sid };
    let user_sid = sid_bytes_in_token_buffer(&user_buffer, user_length, user_sid)?;

    let session_id = token_information_u32(raw_token, TokenSessionId, "TokenSessionId")?;

    let (integrity_buffer, integrity_length) =
        token_information_buffer(raw_token, TokenIntegrityLevel)?;
    if integrity_length < std::mem::size_of::<TOKEN_MANDATORY_LABEL>() {
        return Err(io::Error::other(
            "TokenIntegrityLevel buffer is shorter than its header",
        ));
    }
    let label = integrity_buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
    // SAFETY: GetTokenInformation populated an aligned buffer and the returned
    // length was checked against TOKEN_MANDATORY_LABEL.
    let integrity_label_sid = unsafe { (*label).Label.Sid };
    validate_sid_in_token_buffer(&integrity_buffer, integrity_length, integrity_label_sid)?;
    // SAFETY: SID is valid and bounded to the returned token buffer.
    let subauthority_count = unsafe { GetSidSubAuthorityCount(integrity_label_sid) };
    if subauthority_count.is_null() {
        return Err(io::Error::other("integrity SID has no subauthority count"));
    }
    // SAFETY: validated SID header owns its subauthority-count byte.
    let count = unsafe { *subauthority_count };
    if count == 0 {
        return Err(io::Error::other("integrity SID has no RID"));
    }
    // SAFETY: last subauthority index is in range for the validated SID.
    let rid = unsafe { GetSidSubAuthority(integrity_label_sid, u32::from(count - 1)) };
    if rid.is_null() {
        return Err(io::Error::other("integrity SID has no final RID"));
    }
    // SAFETY: GetSidSubAuthority returned the last in-range RID of a validated
    // SID contained within the returned token buffer.
    let integrity_rid = unsafe { *rid };

    let authentication_id = luid_value(token_statistics(raw_token)?.AuthenticationId);
    // TOKEN_ELEVATION is one DWORD, `TokenIsElevated`.
    let elevated = token_information_u32(raw_token, TokenElevation, "TokenElevation")? != 0;
    let elevation_type = elevation_type_from_raw(
        token_information_u32(raw_token, TokenElevationType, "TokenElevationType")?.cast_signed(),
    )?;
    Ok(WindowsPeerTokenFacts {
        user_sid,
        session_id,
        integrity_rid,
        authentication_id,
        elevated,
        elevation_type,
    })
}

/// Reads a token-information class whose output is one 32-bit value
/// (`TokenSessionId`'s `DWORD`, `TOKEN_ELEVATION` or `TOKEN_ELEVATION_TYPE`).
fn token_information_u32(
    token: windows_sys::Win32::Foundation::HANDLE,
    class: TOKEN_INFORMATION_CLASS,
    name: &str,
) -> io::Result<u32> {
    let mut value = 0_u32;
    let size = u32::try_from(std::mem::size_of_val(&value)).map_err(io::Error::other)?;
    let mut returned = 0_u32;
    // SAFETY: the caller's token stays owned for this call; `value` is aligned,
    // writable storage for `size` bytes in which every bit pattern is a valid
    // `u32`, and `returned` is writable.
    if unsafe {
        GetTokenInformation(
            token,
            class,
            (&raw mut value).cast(),
            size,
            &raw mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    exact_token_information_length(returned, size, name)?;
    Ok(value)
}

/// Reads `TokenStatistics`.
fn token_statistics(token: windows_sys::Win32::Foundation::HANDLE) -> io::Result<TOKEN_STATISTICS> {
    let mut statistics = TOKEN_STATISTICS::default();
    let size = u32::try_from(std::mem::size_of_val(&statistics)).map_err(io::Error::other)?;
    let mut returned = 0_u32;
    // SAFETY: the caller's token stays owned for this call; `statistics` is an
    // aligned, writable TOKEN_STATISTICS of `size` bytes whose fields are all
    // integers, so any bytes Windows writes form a valid value; `returned` is
    // writable.
    if unsafe {
        GetTokenInformation(
            token,
            TokenStatistics,
            (&raw mut statistics).cast(),
            size,
            &raw mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    exact_token_information_length(returned, size, "TokenStatistics")?;
    Ok(statistics)
}

/// Admits a fixed-size token-information class only when Windows reports
/// exactly its structure's size.
fn exact_token_information_length(returned: u32, size: u32, name: &str) -> io::Result<()> {
    if returned == size {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{name} returned an unexpected size"
        )))
    }
}

/// `(HighPart << 32) | LowPart`, the KEL-53 journal's `authentication_id`.
fn luid_value(luid: LUID) -> u64 {
    (u64::from(luid.HighPart.cast_unsigned()) << 32) | u64::from(luid.LowPart)
}

/// Maps the documented `TOKEN_ELEVATION_TYPE` values and refuses any other.
fn elevation_type_from_raw(raw: TOKEN_ELEVATION_TYPE) -> io::Result<WindowsTokenElevationType> {
    if raw == TokenElevationTypeDefault {
        Ok(WindowsTokenElevationType::Default)
    } else if raw == TokenElevationTypeFull {
        Ok(WindowsTokenElevationType::Full)
    } else if raw == TokenElevationTypeLimited {
        Ok(WindowsTokenElevationType::Limited)
    } else {
        Err(io::Error::other(
            "TokenElevationType returned an undocumented value",
        ))
    }
}

fn token_information_buffer(
    token: windows_sys::Win32::Foundation::HANDLE,
    class: windows_sys::Win32::Security::TOKEN_INFORMATION_CLASS,
) -> io::Result<(Vec<usize>, usize)> {
    let mut required = 0_u32;
    // SAFETY: the null output/zero size query asks Windows only for the required
    // buffer size; `required` is writable output storage.
    unsafe {
        GetTokenInformation(token, class, ptr::null_mut(), 0, &raw mut required);
    }
    if required == 0 {
        return Err(io::Error::last_os_error());
    }
    let required_usize = usize::try_from(required).map_err(io::Error::other)?;
    let word = std::mem::size_of::<usize>();
    let words = required_usize
        .checked_add(word - 1)
        .ok_or_else(|| io::Error::other("token information buffer size overflow"))?
        / word;
    let mut buffer = vec![0_usize; words];
    let capacity = words
        .checked_mul(word)
        .ok_or_else(|| io::Error::other("token information buffer capacity overflow"))?;
    let capacity_u32 = u32::try_from(capacity).map_err(io::Error::other)?;
    let mut returned = 0_u32;
    // SAFETY: `buffer` is usize-aligned and writable for `capacity_u32` bytes;
    // its returned length is checked before any structure or SID is read.
    if unsafe {
        GetTokenInformation(
            token,
            class,
            buffer.as_mut_ptr().cast(),
            capacity_u32,
            &raw mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let returned = usize::try_from(returned).map_err(io::Error::other)?;
    if returned == 0 || returned > capacity {
        return Err(io::Error::other(
            "Windows returned an invalid token-information length",
        ));
    }
    Ok((buffer, returned))
}

fn sid_bytes_in_token_buffer(
    buffer: &[usize],
    returned: usize,
    sid: windows_sys::Win32::Security::PSID,
) -> io::Result<Vec<u8>> {
    let sid_length = validate_sid_in_token_buffer(buffer, returned, sid)?;
    // SAFETY: the SID pointer is non-null, valid, and the complete byte range
    // was checked to lie within the live GetTokenInformation buffer.
    Ok(unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), sid_length) }.to_vec())
}

fn validate_sid_in_token_buffer(
    buffer: &[usize],
    returned: usize,
    sid: windows_sys::Win32::Security::PSID,
) -> io::Result<usize> {
    let start = buffer.as_ptr() as usize;
    let end = start
        .checked_add(returned)
        .ok_or_else(|| io::Error::other("token buffer address overflow"))?;
    let sid_start = sid as usize;
    if sid.is_null() || sid_start < start || sid_start >= end {
        return Err(io::Error::other(
            "token SID pointer is outside the returned buffer",
        ));
    }
    // SAFETY: SID begins within a live returned TokenUser/TokenIntegrity buffer.
    if unsafe { IsValidSid(sid) } == 0 {
        return Err(io::Error::other("token SID is invalid"));
    }
    // SAFETY: SID validity was just confirmed by Windows.
    let sid_length = usize::try_from(unsafe { GetLengthSid(sid) }).map_err(io::Error::other)?;
    let sid_end = sid_start
        .checked_add(sid_length)
        .ok_or_else(|| io::Error::other("token SID address overflow"))?;
    if sid_length == 0 || sid_end > end {
        return Err(io::Error::other(
            "token SID extends beyond the returned buffer",
        ));
    }
    Ok(sid_length)
}

impl Read for WindowsNamedPipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let timeout = effective_timeout(
            *self
                .read_timeout
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
            *self
                .absolute_deadline
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )?;
        #[cfg(test)]
        let _active = ActiveStreamIo::new(&self.inner.active_stream_io);
        let result = overlapped_io(
            &self.read_event,
            self.raw_pipe()?,
            timeout,
            buf.len(),
            |handle, bytes, len, overlapped| {
                // SAFETY: `bytes` points to `len` writable bytes owned by `buf` and
                // remains live until this overlapped operation is observed.
                unsafe { ReadFile(handle, bytes, len, ptr::null_mut(), overlapped) }
            },
            buf.as_mut_ptr().cast(),
        );
        match result {
            Err(error) if matches!(error.raw_os_error(), Some(109 | 232 | 233)) => Ok(0),
            other => other,
        }
    }
}

impl Write for WindowsNamedPipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let timeout = effective_timeout(
            *self
                .write_timeout
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
            *self
                .absolute_deadline
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )?;
        #[cfg(test)]
        let _active = ActiveStreamIo::new(&self.inner.active_stream_io);
        overlapped_io(
            &self.write_event,
            self.raw_pipe()?,
            timeout,
            buf.len(),
            |handle, bytes, len, overlapped| {
                // SAFETY: `bytes` points to `len` readable bytes owned by `buf` and
                // remains live until this overlapped operation is observed.
                unsafe { WriteFile(handle, bytes, len, ptr::null_mut(), overlapped) }
            },
            buf.as_ptr().cast_mut().cast(),
        )
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn overlapped_io(
    event: &OwnedEvent,
    handle: *mut core::ffi::c_void,
    timeout: Option<Duration>,
    buffer_len: usize,
    start: impl FnOnce(*mut core::ffi::c_void, *mut u8, u32, *mut OVERLAPPED) -> i32,
    bytes: *mut u8,
) -> io::Result<usize> {
    event.reset()?;
    let mut overlapped = event.overlapped();
    let len = u32::try_from(buffer_len.min(u32::MAX as usize)).map_err(io::Error::other)?;
    let started = start(handle, bytes, len, &raw mut overlapped);
    if started != 0 {
        return observed_bytes(handle, &mut overlapped);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() != Some(ERROR_IO_PENDING.cast_signed()) {
        return Err(error);
    }
    let wait_ms = duration_to_wait_ms(timeout);
    // SAFETY: event remains live and belongs to the live OVERLAPPED above.
    match unsafe { WaitForSingleObject(event.raw(), wait_ms) } {
        WAIT_OBJECT_0 => observed_bytes(handle, &mut overlapped),
        WAIT_TIMEOUT => timeout_io_result(cancel_one_and_observe(handle, &mut overlapped)?),
        WAIT_FAILED => {
            let error = io::Error::last_os_error();
            cancel_one_and_observe(handle, &mut overlapped)?;
            Err(error)
        }
        other => {
            cancel_one_and_observe(handle, &mut overlapped)?;
            Err(io::Error::other(format!(
                "unexpected overlapped wait result {other}"
            )))
        }
    }
}

fn observed_bytes(
    handle: *mut core::ffi::c_void,
    overlapped: &mut OVERLAPPED,
) -> io::Result<usize> {
    let mut transferred = 0;
    // SAFETY: this OVERLAPPED belongs to an operation on `handle`; its event
    // signalled, and both remain live through this completion observation.
    if unsafe { GetOverlappedResult(handle, overlapped, &raw mut transferred, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    usize::try_from(transferred).map_err(io::Error::other)
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum CancelledOperation {
    Aborted,
    Completed(usize),
}

fn timeout_io_result(observation: CancelledOperation) -> io::Result<usize> {
    match observation {
        CancelledOperation::Aborted => Err(io::Error::from_raw_os_error(
            ERROR_SEM_TIMEOUT.cast_signed(),
        )),
        CancelledOperation::Completed(transferred) => Ok(transferred),
    }
}

fn cancel_one_and_observe(
    handle: *mut core::ffi::c_void,
    overlapped: &mut OVERLAPPED,
) -> io::Result<CancelledOperation> {
    // SAFETY: `overlapped` is the live state for this operation and will not
    // be freed until GetOverlappedResult observes completion below.
    let cancel_error = if unsafe { CancelIoEx(handle, overlapped) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(1168) {
            None
        } else {
            Some(error)
        }
    } else {
        None
    };
    let mut transferred = 0;
    // SAFETY: bWait=TRUE keeps the state live until cancellation or the racing
    // normal completion is observed.
    let observation =
        if unsafe { GetOverlappedResult(handle, overlapped, &raw mut transferred, 1) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_OPERATION_ABORTED.cast_signed()) {
                return Err(error);
            }
            CancelledOperation::Aborted
        } else {
            CancelledOperation::Completed(usize::try_from(transferred).map_err(io::Error::other)?)
        };
    cancel_error.map_or(Ok(observation), Err)
}

fn wait_for_operation(
    handle: *mut core::ffi::c_void,
    overlapped: &mut OVERLAPPED,
    operation_event: *mut core::ffi::c_void,
    cancel_event: *mut core::ffi::c_void,
    deadline: Option<Instant>,
) -> io::Result<WaitOutcome> {
    let handles = [cancel_event, operation_event];
    let timeout = match deadline {
        None => INFINITE,
        Some(deadline) => deadline
            .checked_duration_since(Instant::now())
            .map_or(0, |remaining| duration_to_wait_ms(Some(remaining))),
    };
    // SAFETY: both handles remain live for the wait; the array length is exact.
    let result = unsafe {
        WaitForMultipleObjects(
            u32::try_from(handles.len()).map_err(io::Error::other)?,
            handles.as_ptr(),
            0,
            timeout,
        )
    };
    match result {
        WAIT_OBJECT_0 => {
            cancel_one_and_observe(handle, overlapped)?;
            Ok(WaitOutcome::Cancelled)
        }
        value if value == WAIT_OBJECT_0 + 1 => match observed_bytes(handle, overlapped) {
            Ok(_) => Ok(WaitOutcome::Ready),
            Err(error) if matches!(error.raw_os_error(), Some(232 | 233)) => {
                Ok(WaitOutcome::PeerClosed)
            }
            Err(error) => Err(error),
        },
        WAIT_TIMEOUT => {
            cancel_one_and_observe(handle, overlapped)?;
            Ok(WaitOutcome::DeadlineElapsed)
        }
        WAIT_FAILED => {
            let error = io::Error::last_os_error();
            cancel_one_and_observe(handle, overlapped)?;
            Err(error)
        }
        other => {
            cancel_one_and_observe(handle, overlapped)?;
            Err(io::Error::other(format!(
                "unexpected connect wait result {other}"
            )))
        }
    }
}

#[derive(Debug)]
struct OwnedEvent(OwnedHandle);

#[cfg(test)]
struct PendingAccept<'a>(&'a AtomicBool);

#[cfg(test)]
struct ActiveStreamIo<'a>(&'a AtomicUsize);

#[cfg(test)]
impl<'a> PendingAccept<'a> {
    fn new(pending: &'a AtomicBool) -> Self {
        pending.store(true, Ordering::Release);
        Self(pending)
    }
}

#[cfg(test)]
impl Drop for PendingAccept<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(test)]
impl<'a> ActiveStreamIo<'a> {
    fn new(active: &'a AtomicUsize) -> Self {
        active.fetch_add(1, Ordering::AcqRel);
        Self(active)
    }
}

#[cfg(test)]
impl Drop for ActiveStreamIo<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl OwnedEvent {
    fn new() -> io::Result<Self> {
        // SAFETY: null security/name pointers request an unnamed manual-reset
        // event. The returned non-null handle is transferred once below.
        let raw = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateEventW returned one valid uniquely owned handle.
        Ok(Self(unsafe {
            OwnedHandle::from_raw_handle(raw as RawHandle)
        }))
    }

    fn raw(&self) -> *mut core::ffi::c_void {
        self.0.as_raw_handle()
    }

    fn reset(&self) -> io::Result<()> {
        // SAFETY: this object owns the live manual-reset event handle.
        if unsafe { ResetEvent(self.raw()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn overlapped(&self) -> OVERLAPPED {
        OVERLAPPED {
            hEvent: self.raw(),
            ..OVERLAPPED::default()
        }
    }
}

fn current_user_descriptor(sid: &Sid) -> io::Result<LocalBox<SecurityDescriptor>> {
    let sid = ConvertSidToStringSid(sid)?;
    format!(
        "D:P(A;;0x{PIPE_ACCESS_MASK:08x};;;{})",
        sid.to_string_lossy()
    )
    .parse()
}

fn read_security_facts(handle: &OwnedHandle) -> io::Result<PipeSecurityFacts> {
    let descriptor = GetSecurityInfo(
        handle,
        SeObjectType::SE_KERNEL_OBJECT,
        SecurityInformation::Dacl,
    )?;
    let current_sid = current_process_sid()?;
    let sddl = descriptor.as_sddl()?;
    let protected_dacl = sddl.to_string_lossy().contains("D:P");
    let dacl = descriptor
        .dacl()
        .ok_or_else(|| io::Error::other("named-pipe descriptor contains no DACL"))?;
    let ace = dacl.get_ace(0);
    Ok(PipeSecurityFacts {
        protected_dacl,
        ace_count: usize::try_from(dacl.len()).map_err(io::Error::other)?,
        one_ace_is_current_user: ace.and_then(|ace| ace.sid()) == Some(&current_sid),
        one_ace_type: ace.map_or(u8::MAX, |ace| ace.ace_type() as u8),
        one_ace_flags: ace.map_or(u8::MAX, |ace| ace.flags().bits()),
        one_ace_mask: ace.map_or(0, |ace| ace.mask().bits()),
        handle_flags: handle_flags(handle)?,
        pipe_flags: pipe_flags(handle)?,
    })
}

pub(crate) fn read_pipe_descriptor(handle: &OwnedHandle) -> io::Result<PipeDescriptorReadback> {
    let descriptor = GetSecurityInfo(
        handle,
        SeObjectType::SE_KERNEL_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl | SecurityInformation::Label,
    )?;
    Ok(PipeDescriptorReadback {
        descriptor,
        handle_inheritable: handle_flags(handle)? & HANDLE_FLAG_INHERIT != 0,
        pipe_flags: pipe_flags(handle)?,
    })
}

fn handle_flags(handle: &OwnedHandle) -> io::Result<u32> {
    let mut flags = 0;
    // SAFETY: `handle` is live and `flags` is a valid writable u32.
    if unsafe { GetHandleInformation(handle.as_raw_handle(), &raw mut flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(flags)
}

fn pipe_flags(handle: &OwnedHandle) -> io::Result<u32> {
    let mut flags = 0;
    // SAFETY: `handle` is a live pipe handle (server or client end) borrowed
    // for this call and `flags` is a valid writable u32; omitted size/count
    // outputs are optional null pointers.
    if unsafe {
        GetNamedPipeInfo(
            handle.as_raw_handle(),
            &raw mut flags,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(flags)
}

fn duration_to_wait_ms(timeout: Option<Duration>) -> u32 {
    let Some(timeout) = timeout else {
        return INFINITE;
    };
    let millis = timeout.as_millis().max(1).min(u128::from(INFINITE - 1));
    u32::try_from(millis).unwrap_or(INFINITE - 1)
}

fn connect_deadline_error() -> io::Error {
    io::Error::from_raw_os_error(ERROR_SEM_TIMEOUT.cast_signed())
}

fn validate_timeout(timeout: Option<Duration>) -> io::Result<()> {
    if timeout == Some(Duration::ZERO) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "zero named-pipe I/O timeout is invalid",
        ));
    }
    Ok(())
}

fn effective_timeout(
    configured: Option<Duration>,
    absolute_deadline: Option<Instant>,
) -> io::Result<Option<Duration>> {
    let Some(deadline) = absolute_deadline else {
        return Ok(configured);
    };
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| io::Error::from_raw_os_error(ERROR_SEM_TIMEOUT.cast_signed()))?;
    Ok(Some(
        configured.map_or(remaining, |timeout| timeout.min(remaining)),
    ))
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

fn closed_pipe_error() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "named-pipe handle is closed")
}

fn abort_if_revert_failed(reverted: bool) {
    if !reverted {
        // Microsoft documents that the process must shut down because it
        // otherwise continues under the impersonated client's security context.
        std::process::abort();
    }
}

#[cfg(test)]
pub(crate) fn process_handle_count() -> io::Result<u32> {
    let mut count = 0;
    // SAFETY: GetCurrentProcess returns the caller's valid pseudo-handle and
    // `count` is a live writable u32 for the duration of the query.
    if unsafe { GetProcessHandleCount(GetCurrentProcess(), &raw mut count) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(count)
}

/// This process's own primary token, opened for `TOKEN_QUERY` only.
#[cfg(test)]
pub(crate) fn current_process_query_token() -> io::Result<OwnedHandle> {
    let mut raw = ptr::null_mut();
    // SAFETY: GetCurrentProcess returns the caller's valid pseudo-handle and
    // `raw` is writable HANDLE storage.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcessToken returned one fresh owned token handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

#[cfg(test)]
mod cancellation_tests {
    use super::{CancelledOperation, timeout_io_result};

    #[test]
    fn timeout_preserves_bytes_from_a_racing_normal_completion() {
        assert_eq!(
            timeout_io_result(CancelledOperation::Completed(7)).expect("completed transfer"),
            7
        );
        assert_eq!(
            timeout_io_result(CancelledOperation::Aborted)
                .expect_err("aborted transfer remains a timeout")
                .raw_os_error(),
            Some(121)
        );
    }
}

#[cfg(test)]
mod token_facts_tests;

#[cfg(test)]
mod revert_failure_tests {
    use std::process::Command;

    use super::abort_if_revert_failed;

    const CHILD_ENV: &str = "KELD_TEST_REVERT_FAILURE_CHILD";
    const AFTER_REVERT_MARKER: &str = "KELD_TEST_AFTER_REVERT_FAILURE";

    #[test]
    fn revert_to_self_failure_aborts_process_before_any_retry_or_protocol_work() {
        if std::env::var_os(CHILD_ENV).is_some() {
            abort_if_revert_failed(false);
            println!("{AFTER_REVERT_MARKER}");
            return;
        }

        let output = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "windows_named_pipe::revert_failure_tests::revert_to_self_failure_aborts_process_before_any_retry_or_protocol_work",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .output()
            .expect("run fail-closed subprocess");
        assert!(
            !output.status.success(),
            "failed revert must terminate process"
        );
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains(AFTER_REVERT_MARKER),
            "no protocol retry/admission can run after a failed revert"
        );
    }
}
