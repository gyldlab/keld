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
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use windows_permissions::constants::{AceFlags, AceType, SeObjectType, SecurityInformation};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{ConvertSidToStringSid, GetSecurityInfo};
use windows_permissions::{LocalBox, SecurityDescriptor, Sid};
use windows_sys::Win32::Foundation::{
    ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, ERROR_PIPE_CONNECTED, ERROR_SEM_TIMEOUT,
    GetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
#[cfg(test)]
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_CREATE_PIPE_INSTANCE, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
#[cfg(test)]
use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, INFINITE, ResetEvent, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};
#[cfg(test)]
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

const PIPE_BUFFER_BYTES: u32 = 64 * 1024;
const PIPE_ACCESS_MASK: u32 = 0x0012_019B;

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
}

impl WindowsNamedPipeServer {
    pub(crate) fn bind(endpoint: &str) -> io::Result<Self> {
        let current_sid = current_process_sid()?;
        let descriptor = current_user_descriptor(&current_sid)?;
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

        let server = Self {
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
            }),
        };
        server.validate_security(&current_sid)?;
        Ok(server)
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
        let _lifecycle = lock_or_recover(&self.inner.lifecycle);
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
        let _lifecycle = lock_or_recover(&self.inner.lifecycle);
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
        let pipe = lock_or_recover(&self.inner.pipe);
        let pipe = pipe.as_ref().ok_or_else(closed_pipe_error)?;
        read_security_facts(pipe)
    }

    pub(crate) fn close_terminal(&self) -> io::Result<()> {
        let _lifecycle = lock_or_recover(&self.inner.lifecycle);
        self.inner.consumed.store(true, Ordering::Release);
        self.cancel_pending_io()?;
        // SAFETY: the server owns the live handle and all pending operations
        // were cancelled; their owners retain state until they observe
        // completion. Disconnect does not free an OVERLAPPED or its buffer.
        if self.inner.connected.swap(false, Ordering::AcqRel)
            && unsafe { DisconnectNamedPipe(self.raw_pipe()?) } == 0
        {
            let error = io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(109 | 233)) {
                return Err(error);
            }
        }
        drop(lock_or_recover(&self.inner.pipe).take());
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn connect_client(endpoint: &str) -> io::Result<WindowsNamedPipeStream> {
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
                FILE_FLAG_OVERLAPPED,
                ptr::null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateFileW returned a valid newly owned handle.
        let pipe = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };
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
            }),
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

    #[cfg(test)]
    pub(crate) fn connect_client_until(
        endpoint: &str,
        deadline: Instant,
    ) -> io::Result<WindowsNamedPipeStream> {
        let endpoint_wide = wide(endpoint);
        loop {
            match Self::connect_client(endpoint) {
                Ok(stream) => return Ok(stream),
                Err(error) if error.raw_os_error() == Some(231) => {
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        return Err(io::Error::from_raw_os_error(231));
                    };
                    let wait_ms = duration_to_wait_ms(Some(remaining));
                    // SAFETY: `endpoint_wide` is a live NUL-terminated path;
                    // the bounded wait does not retain its pointer.
                    if unsafe { WaitNamedPipeW(endpoint_wide.as_ptr(), wait_ms) } == 0 {
                        let wait_error = io::Error::last_os_error();
                        if wait_error.raw_os_error() != Some(ERROR_SEM_TIMEOUT.cast_signed()) {
                            return Err(wait_error);
                        }
                    }
                }
                Err(error) => return Err(error),
            }
        }
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
        {
            return Err(io::Error::other(format!(
                "named-pipe security readback did not match the current-user-only contract: {facts:?}"
            )));
        }
        let descriptor = GetSecurityInfo(
            lock_or_recover(&self.inner.pipe)
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
        lock_or_recover(&self.inner.pipe)
            .as_ref()
            .map(AsRawHandle::as_raw_handle)
            .ok_or_else(closed_pipe_error)
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
            read_event: OwnedEvent::new()?,
            write_event: OwnedEvent::new()?,
            read_timeout: Mutex::new(*lock_or_recover(&self.read_timeout)),
            write_timeout: Mutex::new(*lock_or_recover(&self.write_timeout)),
            absolute_deadline: Mutex::new(*lock_or_recover(&self.absolute_deadline)),
        })
    }

    pub(crate) fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        validate_timeout(timeout)?;
        *lock_or_recover(&self.read_timeout) = timeout;
        Ok(())
    }

    pub(crate) fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        validate_timeout(timeout)?;
        *lock_or_recover(&self.write_timeout) = timeout;
        Ok(())
    }

    pub(crate) fn read_timeout(&self) -> Option<Duration> {
        *lock_or_recover(&self.read_timeout)
    }

    pub(crate) fn write_timeout(&self) -> Option<Duration> {
        *lock_or_recover(&self.write_timeout)
    }

    pub(crate) fn set_absolute_deadline(&self, deadline: Option<Instant>) {
        *lock_or_recover(&self.absolute_deadline) = deadline;
    }

    pub(crate) fn shutdown(&self) -> io::Result<()> {
        let server = WindowsNamedPipeServer {
            inner: Arc::clone(&self.inner),
        };
        let _lifecycle = lock_or_recover(&self.inner.lifecycle);
        server.cancel_pending_io()?;
        // SAFETY: this is the live server pipe handle. Cancelling does not
        // free another operation's OVERLAPPED or buffer; each operation owner
        // retains and observes its own state. Disconnect only breaks the pipe
        // connection so peer and local waiters can finish.
        if self.inner.connected.swap(false, Ordering::AcqRel)
            && unsafe { DisconnectNamedPipe(server.raw_pipe()?) } == 0
        {
            let error = io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(109 | 232 | 233)) {
                return Err(error);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn has_active_io(&self) -> bool {
        self.inner.active_stream_io.load(Ordering::Acquire) != 0
    }

    fn raw_pipe(&self) -> io::Result<*mut core::ffi::c_void> {
        lock_or_recover(&self.inner.pipe)
            .as_ref()
            .map(AsRawHandle::as_raw_handle)
            .ok_or_else(closed_pipe_error)
    }
}

impl Read for WindowsNamedPipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let timeout = effective_timeout(
            *lock_or_recover(&self.read_timeout),
            *lock_or_recover(&self.absolute_deadline),
        )?;
        #[cfg(test)]
        let _active = ActiveStreamIo::new(&self.inner.active_stream_io);
        overlapped_io(
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
        )
    }
}

impl Write for WindowsNamedPipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let timeout = effective_timeout(
            *lock_or_recover(&self.write_timeout),
            *lock_or_recover(&self.absolute_deadline),
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
        WAIT_TIMEOUT => {
            cancel_one_and_observe(handle, &mut overlapped)?;
            Err(io::Error::from_raw_os_error(
                ERROR_SEM_TIMEOUT.cast_signed(),
            ))
        }
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

fn cancel_one_and_observe(
    handle: *mut core::ffi::c_void,
    overlapped: &mut OVERLAPPED,
) -> io::Result<()> {
    // SAFETY: `overlapped` is the live state for this operation and will not
    // be freed until GetOverlappedResult observes completion below.
    if unsafe { CancelIoEx(handle, overlapped) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(1168) {
            return Err(error);
        }
    }
    let mut transferred = 0;
    // SAFETY: bWait=TRUE keeps the state live until cancellation or the racing
    // normal completion is observed.
    if unsafe { GetOverlappedResult(handle, overlapped, &raw mut transferred, 1) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_OPERATION_ABORTED.cast_signed()) {
            return Err(error);
        }
    }
    Ok(())
}

fn wait_for_operation(
    handle: *mut core::ffi::c_void,
    overlapped: &mut OVERLAPPED,
    operation_event: *mut core::ffi::c_void,
    cancel_event: *mut core::ffi::c_void,
    deadline: Option<Instant>,
) -> io::Result<WaitOutcome> {
    let handles = [cancel_event, operation_event];
    let timeout = deadline
        .and_then(|deadline| deadline.checked_duration_since(Instant::now()))
        .map_or(INFINITE, |remaining| duration_to_wait_ms(Some(remaining)));
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

fn duration_to_wait_ms(timeout: Option<Duration>) -> u32 {
    let Some(timeout) = timeout else {
        return INFINITE;
    };
    let millis = timeout.as_millis().max(1).min(u128::from(INFINITE - 1));
    u32::try_from(millis).unwrap_or(INFINITE - 1)
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

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn closed_pipe_error() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "named-pipe handle is closed")
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
