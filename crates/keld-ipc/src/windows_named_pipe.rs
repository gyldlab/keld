//! Windows named-pipe handle and overlapped-I/O ownership.
//!
//! This module owns the Win32 ABI boundary, including the readback of its
//! pipes' security and the one safe comparison of a readback with a pipe's
//! exact form. Bootstrap token parsing, frame decoding, authentication, the
//! choice of each pipe's form and other rejection policy remain in safe shared
//! modules.

#![deny(unsafe_op_in_unsafe_fn)]
#![allow(unsafe_code)] // KEL-101-sanctioned Win32 pipe/overlapped ABI owner

use std::ffi::OsStr;
use std::fmt;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};
#[cfg(test)]
use std::{cell::RefCell, sync::mpsc};

#[cfg(test)]
use crate::bootstrap::{TestConsumeGate, wait_at_test_consume_gate};
use windows_permissions::constants::{SeObjectType, SecurityInformation};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::{
    ConvertSecurityDescriptorToStringSecurityDescriptor, ConvertSidToStringSid, GetSecurityInfo,
};
use windows_permissions::{LocalBox, SecurityDescriptor, Sid};
use windows_sys::Win32::Foundation::{
    ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, ERROR_PIPE_CONNECTED, ERROR_SEM_TIMEOUT,
    GetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::{RevertToSelf, TOKEN_QUERY};
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
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentThread, INFINITE, OpenThreadToken, ResetEvent, SetEvent,
    WaitForMultipleObjects, WaitForSingleObject,
};
#[cfg(test)]
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessHandleCount, OpenProcessToken,
};

// Re-export from the original module path: existing crate public API and
// named-pipe client callers keep one token-fact reader, with no new privileges.
mod deadline_policy;
mod token_facts;
use deadline_policy::{
    connect_deadline_error, duration_to_wait_ms, effective_timeout, validate_timeout,
};
#[cfg(test)]
use token_facts::{
    U32TokenClass, elevation_type_from_raw, exact_token_information_length, luid_value,
};
pub use token_facts::{
    WindowsPeerTokenFacts, WindowsTokenElevationType, query_windows_peer_token_facts,
};

const PIPE_BUFFER_BYTES: u32 = 64 * 1024;
/// The landed `keld-ipc` pipe grant: read/write data, attributes and extended
/// attributes, `READ_CONTROL` and `SYNCHRONIZE`; never `FILE_CREATE_PIPE_INSTANCE`,
/// `WRITE_DAC` or `WRITE_OWNER`.
pub(crate) const PIPE_ACCESS_MASK: u32 = 0x0012_019B;
const _: () = assert!(
    PIPE_ACCESS_MASK & FILE_CREATE_PIPE_INSTANCE == 0,
    "a keld-ipc pipe grant never lets a client create another instance"
);

#[cfg(test)]
thread_local! {
    static CONNECT_BUSY_WITNESS: RefCell<Option<mpsc::Sender<()>>> = const { RefCell::new(None) };
    /// Test-only pause on this thread after a stream `ReadFile`/`WriteFile`
    /// returned `ERROR_IO_PENDING` and `io_issue` was released, before the
    /// completion wait. A terminal close can then run to completion while the
    /// pending operation still uses its raw HANDLE.
    static AFTER_PENDING_IO_ISSUE: Mutex<Option<TestConsumeGate>> = const { Mutex::new(None) };
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
    // Each in-flight stream I/O clones the SAME owned HANDLE allocation.
    // A terminal close removes the named endpoint without freeing that
    // HANDLE until its final overlapped completion has been observed.
    pipe: Mutex<Option<Arc<OwnedHandle>>>,
    /// Serialises the instance's terminal transitions: the one-shot
    /// ([`WindowsNamedPipeServer::consume`]), the cancel, the re-arm and the
    /// close. Each holds it only across its flag checks and non-blocking
    /// Win32 calls, never across a wait. Lock order: `lifecycle` -> `io_issue`
    /// -> `pipe`; no caller acquires an outer lock while holding pipe.
    lifecycle: Mutex<()>,
    /// Serializes the final cancellation check with ReadFile/WriteFile issue.
    /// Unlike lifecycle, this lock is not held by a cancelling one-shot test
    /// gate, so a peer handshake can progress up to its consume boundary.
    /// Dropped before any OVERLAPPED wait or completion observation.
    io_issue: Mutex<()>,
    cancel_event: OwnedHandle,
    connect_event: OwnedEvent,
    connected: AtomicBool,
    /// Set once, under `lifecycle`, by the one-shot or the terminal close.
    /// The one-shot sets it only while `cancelled` is clear, so a consumed
    /// instance was never cancelled before its one-shot.
    consumed: AtomicBool,
    /// Set by [`WindowsNamedPipeServer::cancel`], under `lifecycle`, while the
    /// instance is not yet consumed, and never cleared; once it is set the
    /// one-shot is refused. The event above wakes a pending accept and
    /// `CancelIoEx` aborts pending stream I/O, but neither reaches I/O that a
    /// waiter issues after the cancel; this flag does: an unconsumed server
    /// stream refuses to start new I/O once it is set, and a waiter that
    /// re-arms after a refusal reads it before it waits again.
    cancelled: AtomicBool,
    #[cfg(test)]
    accept_pending: AtomicBool,
    #[cfg(test)]
    active_stream_io: AtomicUsize,
    #[cfg(test)]
    force_cancel_error: AtomicBool,
    /// Test barrier between [`WindowsNamedPipeServer::cancel`]'s "not yet
    /// consumed" check and its act, crossed with the lifecycle lock held.
    #[cfg(test)]
    cancel_gate: Mutex<Option<TestConsumeGate>>,
    /// Test barrier at the entry of [`WindowsNamedPipeServer::consume`],
    /// before it takes the lifecycle lock.
    #[cfg(test)]
    before_consume: Mutex<Option<TestConsumeGate>>,
    /// Test-only pause after the early cancellation precheck, before the
    /// final check guarded by `io_issue`.
    #[cfg(test)]
    after_stream_cancel_check: Mutex<Option<TestConsumeGate>>,
    /// Test-only pause after the final cancellation check and borrowed
    /// owned pipe HANDLE under `io_issue`, before native `ReadFile` submission.
    #[cfg(test)]
    after_raw_pipe_borrow: Mutex<Option<TestConsumeGate>>,
    /// Test-only pause after `accept_until` obtains a raw HANDLE but before
    /// `ConnectNamedPipe` submits its OVERLAPPED on the exact kernel object.
    #[cfg(test)]
    after_accept_pipe_borrow: Mutex<Option<TestConsumeGate>>,
    /// Test gate after the native `ConnectNamedPipe` result and issue unlock,
    /// before its success can be published against terminal close.
    #[cfg(test)]
    after_accept_connect_result: Mutex<Option<TestConsumeGate>>,
    /// Test witness: reported once by a thread that finds the lifecycle lock
    /// held and is about to wait for it.
    #[cfg(test)]
    lifecycle_waiter: Mutex<Option<mpsc::SyncSender<()>>>,
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

/// Owner, DACL and mandatory label read back from a live pipe handle, with
/// that handle's inheritance flag and the pipe's `GetNamedPipeInfo` flags.
pub(crate) struct PipeDescriptorReadback {
    pub(crate) descriptor: LocalBox<SecurityDescriptor>,
    pub(crate) handle_inheritable: bool,
    pub(crate) pipe_flags: u32,
}

/// The descriptor sections that a pipe's exact security form fixes. Every
/// form also requires a non-inheritable handle and a pipe that rejects remote
/// clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PipeSecuritySections {
    /// The protected DACL only: the app-link and lifecycle pipes, whose owner
    /// and label are the creating token's defaults.
    Dacl,
    /// Owner, protected DACL and mandatory label: `keld-attempt` endpoints.
    OwnerDaclLabel,
}

/// The descriptor or pipe fact in which a live pipe differs from its exact
/// security form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsPipeSecurityFact {
    /// The handle can be inherited by a child process.
    InheritableHandle,
    /// The pipe does not set `PIPE_REJECT_REMOTE_CLIENTS`.
    RemoteClients,
    /// The owner is absent or not the form's owner.
    Owner,
    /// The DACL is not exactly the form's protected ACE set.
    Dacl,
    /// The mandatory label is not exactly the form's label.
    Label,
}

impl fmt::Display for WindowsPipeSecurityFact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InheritableHandle => "inheritable handle",
            Self::RemoteClients => "remote clients admitted",
            Self::Owner => "owner",
            Self::Dacl => "DACL",
            Self::Label => "mandatory label",
        })
    }
}

impl PipeDescriptorReadback {
    /// The single exact-form comparison for Keld pipes: the first fact in
    /// which this readback differs from `expected` over `sections`, or `None`
    /// when it is exactly that form.
    ///
    /// The DACL section, control flags included, must render to the same SDDL
    /// as the expected one, so an extra or reordered ACE, any other mask bit or
    /// a lost protection flag differs. The label is compared as its ACE list
    /// through `keld_guard::windows_acl_entries_equal`, so a missing, lower or
    /// policy-changed label differs, while the SACL's auto-inherited control
    /// flag, which Windows sets itself when it assigns the descriptor, is not
    /// part of the label.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if a DACL cannot be rendered as SDDL.
    pub(crate) fn first_mismatch(
        &self,
        expected: &SecurityDescriptor,
        sections: PipeSecuritySections,
    ) -> io::Result<Option<WindowsPipeSecurityFact>> {
        if self.handle_inheritable {
            return Ok(Some(WindowsPipeSecurityFact::InheritableHandle));
        }
        if self.pipe_flags & PIPE_REJECT_REMOTE_CLIENTS == 0 {
            return Ok(Some(WindowsPipeSecurityFact::RemoteClients));
        }
        let owner_and_label = sections == PipeSecuritySections::OwnerDaclLabel;
        if owner_and_label {
            let owner = self.descriptor.owner();
            if owner.is_none() || owner != expected.owner() {
                return Ok(Some(WindowsPipeSecurityFact::Owner));
            }
        }
        let dacl = |descriptor: &SecurityDescriptor| {
            ConvertSecurityDescriptorToStringSecurityDescriptor(
                descriptor,
                SecurityInformation::Dacl,
            )
        };
        if dacl(&self.descriptor)? != dacl(expected)? {
            return Ok(Some(WindowsPipeSecurityFact::Dacl));
        }
        if owner_and_label {
            let label_matches = match (self.descriptor.sacl(), expected.sacl()) {
                (Some(actual), Some(expected)) => {
                    keld_guard::windows_acl_entries_equal(actual, expected)
                }
                _ => false,
            };
            if !label_matches {
                return Ok(Some(WindowsPipeSecurityFact::Label));
            }
        }
        Ok(None)
    }
}

impl WindowsNamedPipeServer {
    /// Creates `endpoint` under the current-user-only form (a protected DACL
    /// granting only [`PIPE_ACCESS_MASK`] to this process's `TokenUser`) and
    /// requires the readback to be exactly that form.
    pub(crate) fn bind(endpoint: &str) -> io::Result<Self> {
        let current_sid = current_process_sid()?;
        let descriptor = current_user_descriptor(&current_sid)?;
        let server = Self::bind_with_descriptor(endpoint, &descriptor)?;
        if let Some(fact) = server
            .descriptor_readback()?
            .first_mismatch(&descriptor, PipeSecuritySections::Dacl)?
        {
            return Err(io::Error::other(format!(
                "named-pipe security readback did not match the current-user-only contract: {fact}"
            )));
        }
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
                pipe: Mutex::new(Some(Arc::new(pipe))),
                lifecycle: Mutex::new(()),
                io_issue: Mutex::new(()),
                cancel_event,
                connect_event,
                connected: AtomicBool::new(false),
                consumed: AtomicBool::new(false),
                cancelled: AtomicBool::new(false),
                #[cfg(test)]
                accept_pending: AtomicBool::new(false),
                #[cfg(test)]
                active_stream_io: AtomicUsize::new(0),
                #[cfg(test)]
                force_cancel_error: AtomicBool::new(false),
                #[cfg(test)]
                cancel_gate: Mutex::new(None),
                #[cfg(test)]
                before_consume: Mutex::new(None),
                #[cfg(test)]
                after_stream_cancel_check: Mutex::new(None),
                #[cfg(test)]
                after_raw_pipe_borrow: Mutex::new(None),
                #[cfg(test)]
                after_accept_pipe_borrow: Mutex::new(None),
                #[cfg(test)]
                after_accept_connect_result: Mutex::new(None),
                #[cfg(test)]
                lifecycle_waiter: Mutex::new(None),
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
        // A terminal close may remove the named endpoint while this accept
        // waits. Retain the original HANDLE through its overlapped completion.
        let pipe = self
            .inner
            .pipe
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .cloned()
            .ok_or_else(closed_pipe_error)?;
        #[cfg(test)]
        wait_at_test_consume_gate(&self.inner.after_accept_pipe_borrow);
        // The final terminal check and ConnectNamedPipe issue share the short
        // issuance lock with close_terminal. Never hold it across a wait.
        let issuance = self
            .inner
            .io_issue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.inner.consumed.load(Ordering::Acquire) {
            return Err(closed_pipe_error());
        }
        // SAFETY: the pipe is the same pinned OS HANDLE owned until the
        // OVERLAPPED operation completes; its event and state also stay live.
        let connected_now =
            unsafe { ConnectNamedPipe(pipe.as_raw_handle(), &raw mut overlapped) } != 0;
        // Capture the native status while still holding the issuance lock:
        // dropping a mutex must not replace the Win32 last-error code.
        let connect_error = (!connected_now).then(io::Error::last_os_error);
        drop(issuance);
        #[cfg(test)]
        wait_at_test_consume_gate(&self.inner.after_accept_connect_result);
        if connected_now {
            self.publish_connected()?;
            return Ok(WaitOutcome::Ready);
        }
        let error = connect_error
            .ok_or_else(|| io::Error::other("native ConnectNamedPipe start lost its OS error"))?;
        match error.raw_os_error() {
            Some(code) if code == ERROR_PIPE_CONNECTED.cast_signed() => {
                self.publish_connected()?;
                Ok(WaitOutcome::Ready)
            }
            Some(code) if code == ERROR_IO_PENDING.cast_signed() => {
                #[cfg(test)]
                let _pending = PendingAccept::new(&self.inner.accept_pending);
                let outcome = wait_for_operation(
                    pipe.as_raw_handle(),
                    &mut overlapped,
                    operation_event.raw(),
                    self.raw_cancel_event(),
                    deadline,
                )?;
                match outcome {
                    WaitOutcome::Ready | WaitOutcome::PeerClosed => {
                        self.publish_connected()?;
                    }
                    WaitOutcome::Cancelled | WaitOutcome::DeadlineElapsed => {
                        self.close_terminal()?;
                    }
                }
                Ok(outcome)
            }
            Some(232 | 233) => {
                self.publish_connected()?;
                Ok(WaitOutcome::PeerClosed)
            }
            _ => Err(error),
        }
    }

    /// Publish a completed accept only while the terminal state remains open.
    /// Both publication and terminal close take `io_issue`, so the consumed
    /// check and connected state update cannot straddle terminal close. Unlike
    /// `lifecycle`, this short lock cannot stall a concurrent one-shot gate.
    /// Neither mutex is held across OVERLAPPED completion observation.
    fn publish_connected(&self) -> io::Result<()> {
        let _issuance = self
            .inner
            .io_issue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.inner.consumed.load(Ordering::Acquire) {
            return Err(closed_pipe_error());
        }
        self.inner.connected.store(true, Ordering::Release);
        Ok(())
    }

    pub(crate) fn disconnect_for_retry(&self) -> io::Result<()> {
        let _lifecycle = self.lock_lifecycle();
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

    /// Takes this instance's one-shot and reports whether it was taken. It is
    /// refused, and `false` returned, when [`Self::cancel`] acted first; the
    /// caller then sends nothing more on the connection and ends the
    /// instance. The one-shot and the cancel are serialised on the lifecycle
    /// lock, so a cancel either acts wholly before the one-shot, which is then
    /// refused, or finds the instance consumed and does nothing: it never
    /// signals or aborts I/O of an admitted stream.
    #[must_use = "a refused one-shot means a cancel won; the caller must end the instance"]
    pub(crate) fn consume(&self) -> bool {
        #[cfg(test)]
        wait_at_test_consume_gate(&self.inner.before_consume);
        let _lifecycle = self.lock_lifecycle();
        if self.inner.cancelled.load(Ordering::Acquire) {
            return false;
        }
        self.inner.consumed.store(true, Ordering::Release);
        true
    }

    /// Ends this instance's pending and future accept and stream waits until it
    /// is consumed: it is a no-op on a consumed instance, so a cancel that
    /// races an admission never touches the admitted stream, and once it has
    /// acted the one-shot is refused ([`Self::consume`]). The check and the
    /// act run under the lifecycle lock, as the one-shot does. The order is
    /// the flag, then the event, then `CancelIoEx`: I/O issued after the flag
    /// is refused at once, a pending accept wakes on the event, and pending
    /// stream I/O is aborted. None of them waits. Idempotent.
    pub(crate) fn cancel(&self) -> io::Result<()> {
        let _lifecycle = self.lock_lifecycle();
        if self.inner.consumed.load(Ordering::Acquire) {
            return Ok(());
        }
        #[cfg(test)]
        wait_at_test_consume_gate(&self.inner.cancel_gate);
        // Cancelling and late overlapped submission share this short lock.
        // Holding lifecycle while waiting for io_issue preserves the one-shot
        // ordering; peer I/O needs only io_issue and can reach consume().
        let _io_issue = self
            .inner
            .io_issue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.inner.cancelled.store(true, Ordering::Release);
        // SAFETY: the cancellation event handle remains live in `inner`.
        if unsafe { SetEvent(self.raw_cancel_event()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.cancel_pending_io()
    }

    /// Whether [`Self::cancel`] ran before this instance was consumed. A waiter
    /// that re-arms after a refusal reads this before it waits again, because
    /// a synchronous connect completion bypasses the cancel event.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn canceller(&self) -> WindowsNamedPipeCanceller {
        WindowsNamedPipeCanceller {
            inner: Arc::downgrade(&self.inner),
        }
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
        let _lifecycle = self.lock_lifecycle();
        // Keep terminal close behind any read/write's brief native issuance.
        // Lock order lifecycle -> io_issue -> pipe; no lock crosses a wait.
        let _io_issue = self
            .inner
            .io_issue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Terminal state and a completed accept's publication share this
        // lock; no later accept can set connected after a terminal close.
        self.inner.consumed.store(true, Ordering::Release);
        self.cancel_pending_io()?;
        // SAFETY: a pending operation independently pins the same HANDLE
        // allocation until its completion is observed. Disconnect cannot
        // free the source HANDLE while a stream still owns that pin.
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
                pipe: Mutex::new(Some(Arc::new(pipe))),
                lifecycle: Mutex::new(()),
                io_issue: Mutex::new(()),
                cancel_event,
                connect_event,
                connected: AtomicBool::new(true),
                consumed: AtomicBool::new(true),
                cancelled: AtomicBool::new(false),
                #[cfg(test)]
                accept_pending: AtomicBool::new(false),
                #[cfg(test)]
                active_stream_io: AtomicUsize::new(0),
                #[cfg(test)]
                force_cancel_error: AtomicBool::new(false),
                #[cfg(test)]
                cancel_gate: Mutex::new(None),
                #[cfg(test)]
                before_consume: Mutex::new(None),
                #[cfg(test)]
                after_stream_cancel_check: Mutex::new(None),
                #[cfg(test)]
                after_raw_pipe_borrow: Mutex::new(None),
                #[cfg(test)]
                after_accept_pipe_borrow: Mutex::new(None),
                #[cfg(test)]
                after_accept_connect_result: Mutex::new(None),
                #[cfg(test)]
                lifecycle_waiter: Mutex::new(None),
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

    /// Holds the next [`Self::cancel`] that finds this instance unconsumed at
    /// `gate`, after its check and before its act, with the lifecycle lock held.
    #[cfg(test)]
    pub(crate) fn install_cancel_gate(&self, gate: TestConsumeGate) {
        *self
            .inner
            .cancel_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(gate);
    }

    /// Holds the next [`Self::consume`] at `gate`, before it takes the
    /// lifecycle lock.
    #[cfg(test)]
    pub(crate) fn install_before_consume_gate(&self, gate: TestConsumeGate) {
        *self
            .inner
            .before_consume
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(gate);
    }

    /// Reports once on `witness` when a thread finds this instance's lifecycle
    /// lock held and is about to wait for it.
    #[cfg(test)]
    pub(crate) fn install_lifecycle_wait_witness(&self, witness: mpsc::SyncSender<()>) {
        *self
            .inner
            .lifecycle_waiter
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(witness);
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
            .map(|pipe| pipe.as_ref().as_raw_handle())
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

    /// The instance's lifecycle lock (see [`ServerInner::lifecycle`]). Under
    /// test, a thread that finds it held reports so once on the installed
    /// witness before it waits.
    fn lock_lifecycle(&self) -> MutexGuard<'_, ()> {
        #[cfg(test)]
        self.report_lifecycle_wait();
        self.inner
            .lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    #[cfg(test)]
    fn report_lifecycle_wait(&self) {
        let held = matches!(
            self.inner.lifecycle.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        );
        if held
            && let Some(witness) = self
                .inner
                .lifecycle_waiter
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
        {
            let _ = witness.send(());
        }
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
        let pipe = self.pinned_pipe()?;
        let mut pid = 0_u32;
        // SAFETY: the connected pipe handle stays owned by `self`, and `pid` is
        // writable storage. The selected API queries the opposite endpoint.
        let succeeded = unsafe {
            if self.local_is_server {
                GetNamedPipeClientProcessId(pipe.as_raw_handle(), &raw mut pid)
            } else {
                GetNamedPipeServerProcessId(pipe.as_raw_handle(), &raw mut pid)
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
        let pipe = self.pinned_pipe()?;
        let mut session_id = 0_u32;
        // SAFETY: the connected pipe handle stays owned by `self`, and the
        // output is writable storage. Query the opposite endpoint's session.
        let succeeded = unsafe {
            if self.local_is_server {
                GetNamedPipeClientSessionId(pipe.as_raw_handle(), &raw mut session_id)
            } else {
                GetNamedPipeServerSessionId(pipe.as_raw_handle(), &raw mut session_id)
            }
        };
        if succeeded == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(session_id)
    }

    pub(crate) fn is_inheritable(&self) -> io::Result<bool> {
        let pipe = self.pinned_pipe()?;
        let mut flags = 0_u32;
        // SAFETY: pipe remains owned by the connected stream and flags is writable.
        if unsafe { GetHandleInformation(pipe.as_raw_handle(), &raw mut flags) } == 0 {
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
        let pipe = self.pinned_pipe()?;
        // SAFETY: `pipe` is the retained connected server endpoint; Windows
        // impersonates the context of the client that wrote its last message.
        if unsafe { ImpersonateNamedPipeClient(pipe.as_raw_handle()) } == 0 {
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
        let _lifecycle = server.lock_lifecycle();
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

    /// Refuses to start I/O on a cancelled, not yet consumed server instance
    /// with `ERROR_OPERATION_ABORTED`, the code a cancelled pending operation
    /// reports. `CancelIoEx` reaches only the operations pending at the
    /// cancel; without this gate an operation issued afterwards would wait out
    /// its own deadline. An admitted (consumed) stream and a client stream are
    /// never refused: their `consumed` is set before or at creation.
    fn refuse_if_cancelled(&self) -> io::Result<()> {
        if self.inner.cancelled.load(Ordering::Acquire)
            && !self.inner.consumed.load(Ordering::Acquire)
        {
            return Err(operation_aborted());
        }
        Ok(())
    }

    fn pinned_pipe(&self) -> io::Result<Arc<OwnedHandle>> {
        self.inner
            .pipe
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .cloned()
            .ok_or_else(closed_pipe_error)
    }
}

impl Read for WindowsNamedPipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.refuse_if_cancelled()?;
        #[cfg(test)]
        wait_at_test_consume_gate(&self.inner.after_stream_cancel_check);
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
        // Serialize the final cancellation check and native ReadFile submission
        // with cancel, without blocking the in-flight one-shot lifecycle.
        let issuance = self
            .inner
            .io_issue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.refuse_if_cancelled()?;
        let pinned_pipe = self.pinned_pipe()?;
        #[cfg(test)]
        wait_at_test_consume_gate(&self.inner.after_raw_pipe_borrow);
        let result = overlapped_io(
            &self.read_event,
            pinned_pipe.as_raw_handle(),
            timeout,
            buf.len(),
            |handle, bytes, len, overlapped| {
                // SAFETY: `bytes` points to `len` writable bytes owned by `buf` and
                // remains live until this overlapped operation is observed.
                unsafe { ReadFile(handle, bytes, len, ptr::null_mut(), overlapped) }
            },
            buf.as_mut_ptr().cast(),
            issuance,
        );
        // The same native HANDLE, not a duplicate or detached raw pointer,
        // stays owned until GetOverlappedResult has observed its completion.
        drop(pinned_pipe);
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
        self.refuse_if_cancelled()?;
        #[cfg(test)]
        wait_at_test_consume_gate(&self.inner.after_stream_cancel_check);
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
        // A cancelling server cannot race between this final check and the
        // native WriteFile issuance; no issuance lock crosses a pending wait.
        let issuance = self
            .inner
            .io_issue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.refuse_if_cancelled()?;
        let pinned_pipe = self.pinned_pipe()?;
        let result = overlapped_io(
            &self.write_event,
            pinned_pipe.as_raw_handle(),
            timeout,
            buf.len(),
            |handle, bytes, len, overlapped| {
                // SAFETY: `bytes` points to `len` readable bytes owned by `buf` and
                // remains live until this overlapped operation is observed.
                unsafe { WriteFile(handle, bytes, len, ptr::null_mut(), overlapped) }
            },
            buf.as_ptr().cast_mut().cast(),
            issuance,
        );
        drop(pinned_pipe);
        result
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
    issuance: MutexGuard<'_, ()>,
) -> io::Result<usize> {
    event.reset()?;
    let mut overlapped = event.overlapped();
    let len = u32::try_from(buffer_len.min(u32::MAX as usize)).map_err(io::Error::other)?;
    // The caller owns io_issue from its last cancellation check through this
    // overlapped Win32 submission. Capture GetLastError BEFORE unlocking;
    // mutex operations must not clobber the native result.
    let started = start(handle, bytes, len, &raw mut overlapped);
    let start_error = (started == 0).then(io::Error::last_os_error);
    drop(issuance);
    // No issuance lock is held across completion observation or a pending wait.
    if started != 0 {
        return observed_bytes(handle, &mut overlapped);
    }
    let error = start_error.ok_or_else(|| io::Error::other("native start lost its OS error"))?;
    if error.raw_os_error() != Some(ERROR_IO_PENDING.cast_signed()) {
        return Err(error);
    }
    #[cfg(test)]
    AFTER_PENDING_IO_ISSUE.with(wait_at_test_consume_gate);
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
    let count = u32::try_from(handles.len()).map_err(io::Error::other)?;
    let result = loop {
        let timeout = match deadline {
            None => INFINITE,
            Some(deadline) => deadline
                .checked_duration_since(Instant::now())
                .map_or(0, |remaining| duration_to_wait_ms(Some(remaining))),
        };
        // SAFETY: both handles remain live for the wait; the array length is exact.
        let result = unsafe { WaitForMultipleObjects(count, handles.as_ptr(), 0, timeout) };
        // The kernel measures the timeout on its own clock, which can run a
        // fraction of a millisecond ahead of the monotonic clock the deadline
        // is on; a timeout that fires before the deadline consumed nothing, so
        // the remainder is waited out and the deadline is never reported early.
        if result == WAIT_TIMEOUT && deadline.is_some_and(|deadline| Instant::now() < deadline) {
            continue;
        }
        break result;
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

/// The one pipe-security reader: owner, DACL and label from a live server or
/// client handle, with that handle's inheritance flag and the pipe's flags.
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

/// This process's own Windows session, from `ProcessIdToSessionId` on this
/// process's own ID: the session a `keld-attempt` client requires of the
/// server before it sends anything (KEL-53 §4).
pub(crate) fn current_process_session_id() -> io::Result<u32> {
    let mut session_id = 0_u32;
    // SAFETY: the process ID is this running process's own, so it names no
    // other process for the duration of the call, and `session_id` is
    // writable u32 storage.
    if unsafe { ProcessIdToSessionId(std::process::id(), &raw mut session_id) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(session_id)
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

/// `ERROR_OPERATION_ABORTED`, the code a cancelled pending operation reports:
/// the one error for I/O or a one-shot that a cancel of the instance refused.
pub(crate) fn operation_aborted() -> io::Error {
    io::Error::from_raw_os_error(ERROR_OPERATION_ABORTED.cast_signed())
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

fn closed_pipe_error() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "named-pipe handle is closed")
}

fn abort_if_revert_failed(reverted: bool) {
    #[cfg(test)]
    let reverted = reverted && !REVERT_FAILURE_INJECTED.with(std::cell::Cell::get);
    if !reverted {
        // Microsoft documents that the process must shut down because it
        // otherwise continues under the impersonated client's security context.
        std::process::abort();
    }
}

#[cfg(test)]
thread_local! {
    /// Seam: on a thread that set it, a revert that succeeded counts as failed,
    /// so a test can observe what a real `RevertToSelf` failure does to its
    /// caller. The thread has still reverted.
    static REVERT_FAILURE_INJECTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Makes every later revert on this thread count as failed
/// ([`REVERT_FAILURE_INJECTED`]), which aborts the process.
#[cfg(test)]
pub(crate) fn inject_revert_failure_on_this_thread() {
    REVERT_FAILURE_INJECTED.with(|injected| injected.set(true));
}

/// Test-only access to a stream's own handle.
#[cfg(test)]
impl WindowsNamedPipeStream {
    /// Lends this endpoint's own pipe handle to a test oracle, as
    /// [`WindowsNamedPipeServer::inspect_owned_pipe`] does for a server.
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
    use std::time::{Duration, Instant};

    use windows_sys::Win32::System::Threading::INFINITE;

    use super::{
        CancelledOperation, connect_deadline_error, duration_to_wait_ms, effective_timeout,
        timeout_io_result, validate_timeout,
    };

    /// A deadline-bounded wait never ends before its deadline: a fractional
    /// millisecond rounds up, a whole one stays exact, zero waits one
    /// millisecond (never a busy poll), an unbounded wait is `INFINITE`, and
    /// an overlong one saturates below it. Oracle: literal millisecond values.
    /// Native failure-first: cancellation after the first precheck but before
    /// `ReadFile` must not schedule new pending I/O which `CancelIoEx` already missed.
    /// The actual connected peer stays silent; 900ms is a bounded red-case
    /// escape limit, not the expected behaviour (`ERROR_OPERATION_ABORTED` 995).
    #[expect(
        clippy::too_many_lines,
        reason = "the two Win32 read/write cancellation windows and observations remain visible in one adversarial scenario"
    )]
    #[test]
    fn cancelled_stream_refuses_a_read_submitted_after_the_cancel_raced_its_precheck() {
        use std::io::{Read as _, Write as _};
        use std::sync::{PoisonError, mpsc};
        use std::thread;

        use super::{WaitOutcome, WindowsNamedPipeServer};
        use crate::bootstrap::{TestConsumeGate, random_test_locator};

        let endpoint = format!(
            r"\\.\pipe\keld-gh905-cancel-{}",
            random_test_locator().expect("fresh unique native pipe")
        );
        let server = WindowsNamedPipeServer::bind(&endpoint).expect("bind real native pipe");
        let _client = WindowsNamedPipeServer::connect_client(&endpoint)
            .expect("connect a silent native peer");
        assert_eq!(
            server
                .accept_until(Some(Instant::now() + Duration::from_secs(3)))
                .expect("accept silent peer"),
            WaitOutcome::Ready
        );
        let mut stream = server.stream().expect("unconsumed server end");
        stream
            .set_read_timeout(Some(Duration::from_millis(900)))
            .expect("bounded red-case read deadline");
        let (entered, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release) = mpsc::channel();
        *server
            .inner
            .after_stream_cancel_check
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(TestConsumeGate { entered, release });

        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            let mut buffer = [0_u8; 1];
            let result = stream.read(&mut buffer);
            result_tx.send(result).expect("report bounded native read");
        });
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("read passed cancellation precheck but has not issued ReadFile");
        let cancelled_at = Instant::now();
        server.cancel().expect("cancel unconsumed pipe");
        assert!(
            cancelled_at.elapsed() < Duration::from_millis(500),
            "cancellation must not wait for an unsubmitted or pending read"
        );
        assert!(!server.consume(), "cancel must refuse the one-shot");
        let released_at = Instant::now();
        release_tx
            .send(())
            .expect("release read to actual Win32 submission");
        let result = result_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("native read must finish, never hang");
        reader.join().expect("join bounded native reader");
        let error = result.expect_err("a cancelled pipe must not admit a later read");
        assert_eq!(
            error.raw_os_error(),
            Some(995),
            "a future read after cancel cannot escape CancelIoEx: {error}"
        );
        assert!(
            released_at.elapsed() < Duration::from_millis(500),
            "the late submission must refuse promptly rather than wait 900ms"
        );

        // A buffered write has the same admission race. A one-byte WriteFile
        // could otherwise complete normally AFTER cancel and disclose a byte
        // to a pipe whose unconsumed one-shot has already been refused.
        let endpoint = format!(
            r"\\.\pipe\keld-gh905-write-{}",
            random_test_locator().expect("fresh write endpoint")
        );
        let server = WindowsNamedPipeServer::bind(&endpoint).expect("bind write pipe");
        let mut client = WindowsNamedPipeServer::connect_client(&endpoint)
            .expect("connect a silent receiving peer");
        client
            .set_read_timeout(Some(Duration::from_millis(200)))
            .expect("bound direct client byte oracle");
        assert_eq!(
            server
                .accept_until(Some(Instant::now() + Duration::from_secs(3)))
                .expect("accept receiving peer"),
            WaitOutcome::Ready
        );
        let mut stream = server.stream().expect("unconsumed write stream");
        stream
            .set_write_timeout(Some(Duration::from_millis(900)))
            .expect("bound native write");
        let (entered, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release) = mpsc::channel();
        *server
            .inner
            .after_stream_cancel_check
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(TestConsumeGate { entered, release });

        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let writer = thread::spawn(move || {
            result_tx
                .send(stream.write(&[0x45]))
                .expect("report native write outcome");
        });
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("write passed cancellation check but not WriteFile");
        let cancel_at = Instant::now();
        server.cancel().expect("cancel unconsumed write instance");
        assert!(cancel_at.elapsed() < Duration::from_millis(500));
        assert!(!server.consume(), "write one-shot must be refused");
        let write_unblocked_at = Instant::now();
        release_tx
            .send(())
            .expect("release actual WriteFile submission");
        let result = result_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("native late write must complete promptly");
        writer.join().expect("join native writer");
        let error = result.expect_err("post-cancel write must not send a payload");
        assert_eq!(error.raw_os_error(), Some(995), "{error}");
        assert!(
            write_unblocked_at.elapsed() < Duration::from_millis(500),
            "post-cancel write cannot wait until its 900ms deadline"
        );
        // Independently observe the peer's real named pipe, not just the
        // server-side error: a cancelled write must have emitted no byte.
        let mut observed = [0_u8; 1];
        let receive = client
            .read(&mut observed)
            .expect_err("cancelled unconsumed stream sent no byte to peer");
        assert_eq!(
            receive.raw_os_error(),
            Some(121),
            "silent connected peer times out without receiving a byte: {receive}"
        );
    }

    /// The last cancellation check and the actual Win32 read submission form
    /// one atomic issuance interval. The earlier precheck gate cannot prove it.
    #[expect(
        clippy::too_many_lines,
        reason = "both native gate witnesses, cancel/issue ordering and kernel peer/leak oracles must be visible"
    )]
    #[test]
    fn cancelled_issuer_holds_io_lock_after_final_check_until_native_read_starts() {
        use std::io::Read as _;
        use std::sync::{PoisonError, TryLockError, mpsc};
        use std::thread;

        use super::{WaitOutcome, WindowsNamedPipeServer, process_handle_count};
        use crate::bootstrap::{TestConsumeGate, random_test_locator};

        // First use of named-pipe ACL APIs can initialize unrelated process
        // handles: compare counts only after the same native path was warmed.
        let warm_name = format!(
            r"\\.\pipe\keld-gh905-final-warm-{}",
            random_test_locator().expect("fresh warm endpoint")
        );
        let warm = WindowsNamedPipeServer::bind(&warm_name).expect("warm server");
        let warm_peer = WindowsNamedPipeServer::connect_client(&warm_name).expect("warm peer");
        assert_eq!(
            warm.accept_until(Some(Instant::now() + Duration::from_secs(3)))
                .expect("warm accept"),
            WaitOutcome::Ready
        );
        warm.close_terminal().expect("warm terminal");
        drop(warm_peer);
        drop(warm);
        let baseline = process_handle_count().expect("warmed native handle census");

        let name = format!(
            r"\\.\pipe\keld-gh905-final-{}",
            random_test_locator().expect("fresh native endpoint")
        );
        let server = WindowsNamedPipeServer::bind(&name).expect("bind real pipe");
        let mut client = WindowsNamedPipeServer::connect_client(&name).expect("silent peer");
        client
            .set_read_timeout(Some(Duration::from_millis(180)))
            .expect("bounded peer-byte oracle");
        assert_eq!(
            server
                .accept_until(Some(Instant::now() + Duration::from_secs(3)))
                .expect("native accept"),
            WaitOutcome::Ready
        );
        let mut stream = server.stream().expect("unconsumed server stream");
        stream
            .set_read_timeout(Some(Duration::from_millis(900)))
            .expect("bounded native read");
        let (entered, at_final_check) = mpsc::sync_channel(1);
        let (release_read, resume_read) = mpsc::channel();
        *server
            .inner
            .after_raw_pipe_borrow
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(TestConsumeGate {
            entered,
            release: resume_read,
        });
        let (cancel_entered, at_cancel_gate) = mpsc::sync_channel(1);
        let (release_cancel, resume_cancel) = mpsc::channel();
        server.install_cancel_gate(TestConsumeGate {
            entered: cancel_entered,
            release: resume_cancel,
        });

        let (read_sent, read_outcome) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            let mut byte = [0_u8; 1];
            read_sent
                .send(stream.read(&mut byte))
                .expect("report native read");
        });
        at_final_check
            .recv_timeout(Duration::from_secs(2))
            .expect("reader passed final cancellation check before ReadFile");

        // A compile-valid mutant drops io_issue after that check, then
        // reacquires it before ReadFile without checking cancellation again.
        let lock_held = matches!(
            server.inner.io_issue.try_lock(),
            Err(TryLockError::WouldBlock)
        );
        let (cancel_sent, cancel_outcome) = mpsc::sync_channel(1);
        let canceller = server.clone();
        let cancel_thread = thread::spawn(move || {
            cancel_sent
                .send(canceller.cancel())
                .expect("report native cancel");
        });
        at_cancel_gate
            .recv_timeout(Duration::from_secs(2))
            .expect("cancel acquired lifecycle before trying io_issue");
        release_cancel.send(()).expect("allow actual cancellation");

        // If an unsafe gap exists, wait for cancellation to ACT while the
        // read remains parked. Correct code holds io_issue, so it must
        // release the reader first and let CancelIoEx abort pending I/O.
        let early_cancel = if lock_held {
            None
        } else {
            Some(
                cancel_outcome
                    .recv_timeout(Duration::from_secs(2))
                    .expect("unlocked mutant allows cancel to finish before ReadFile"),
            )
        };
        release_read
            .send(())
            .expect("release actual Win32 ReadFile");
        let read = read_outcome
            .recv_timeout(Duration::from_secs(3))
            .expect("native read must terminate after cancellation");
        reader.join().expect("native reader joined");
        early_cancel
            .unwrap_or_else(|| {
                cancel_outcome
                    .recv_timeout(Duration::from_secs(3))
                    .expect("cancel must finish after native issuance")
            })
            .expect("native cancellation succeeded");
        cancel_thread.join().expect("native canceller joined");

        let error = read.expect_err("no silent-peer read is admitted after cancellation");
        assert_eq!(
            error.raw_os_error(),
            Some(995),
            "late ReadFile escaped cancellation and waited for deadline: {error}"
        );
        assert!(
            lock_held,
            "io_issue was unlocked after final cancellation check"
        );
        assert!(!server.consume(), "cancel won the unconsumed one-shot");
        let mut byte = [0_u8; 1];
        let peer = client
            .read(&mut byte)
            .expect_err("silent peer received no byte");
        assert_eq!(peer.raw_os_error(), Some(121), "{peer}");
        drop(client);
        drop(server);
        assert_eq!(
            process_handle_count().expect("final native handle census"),
            baseline,
            "the checked and cancelled native I/O must not leak HANDLEs"
        );
    }

    /// Warms the native bind/accept/close path first: the first pipe/ACL lookup
    /// can initialize process-wide OS caches, so the returned handle census then
    /// counts only the caller's race. Returns that census, a real server accepted
    /// by a silent client, the client, and the server stream.
    fn warmed_silent_server_stream(
        tag: &str,
    ) -> (
        u32,
        super::WindowsNamedPipeServer,
        super::WindowsNamedPipeStream,
        super::WindowsNamedPipeStream,
    ) {
        use super::{WaitOutcome, WindowsNamedPipeServer, process_handle_count};
        use crate::bootstrap::random_test_locator;

        let warm_endpoint = format!(
            r"\\.\pipe\keld-gh910-{tag}-warm-{}",
            random_test_locator().expect("fresh native warmup pipe")
        );
        let warm_server = WindowsNamedPipeServer::bind(&warm_endpoint).expect("warm native server");
        let warm_client =
            WindowsNamedPipeServer::connect_client(&warm_endpoint).expect("warm native client");
        assert_eq!(
            warm_server
                .accept_until(Some(Instant::now() + Duration::from_secs(3)))
                .expect("warm native accept"),
            WaitOutcome::Ready
        );
        warm_server.close_terminal().expect("warm terminal close");
        drop(warm_client);
        drop(warm_server);

        let baseline = process_handle_count().expect("warmed-up kernel handle count");
        let endpoint = format!(
            r"\\.\pipe\keld-gh910-{tag}-{}",
            random_test_locator().expect("fresh native pipe identity")
        );
        let server = WindowsNamedPipeServer::bind(&endpoint).expect("bind named pipe");
        let client = WindowsNamedPipeServer::connect_client(&endpoint).expect("silent peer");
        assert_eq!(
            server
                .accept_until(Some(Instant::now() + Duration::from_secs(3)))
                .expect("real server accepted silent client"),
            WaitOutcome::Ready
        );
        let stream = server.stream().expect("live server stream");
        (baseline, server, client, stream)
    }

    /// Native failure-first: a terminal close must not invalidate a pipe
    /// HANDLE already borrowed by an in-flight reader before `ReadFile` starts.
    #[test]
    fn terminal_close_keeps_borrowed_stream_handle_live_until_read_is_observed() {
        use std::io::Read as _;
        use std::os::windows::io::AsRawHandle as _;
        use std::sync::{PoisonError, mpsc};
        use std::thread;

        use crate::bootstrap::TestConsumeGate;
        use windows_sys::Win32::Foundation::GetHandleInformation;

        use super::process_handle_count;

        let (baseline, server, client, mut stream) = warmed_silent_server_stream("close");
        stream
            .set_read_timeout(Some(Duration::from_millis(850)))
            .expect("bounded native read");
        let (entered, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release) = mpsc::channel();
        *server
            .inner
            .after_raw_pipe_borrow
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(TestConsumeGate { entered, release });

        let (read_tx, read_rx) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            let mut byte = [0_u8; 1];
            read_tx
                .send(stream.read(&mut byte))
                .expect("send native read observation");
        });
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("reader actually borrowed native HANDLE before ReadFile");

        // Borrow the SAME raw OS handle independently while the reader is
        // parked. The handle must stay valid until the I/O owner observes its
        // completion even if the terminal owner discards its reference.
        let raw = server
            .inspect_owned_pipe(|owned| Ok(owned.as_raw_handle()))
            .expect("native handle pinned before terminal close");
        let mut flags = 0_u32;
        // SAFETY: borrowed HANDLE belongs to a retained server pipe and the
        // output variable is live and writable during this synchronous query.
        assert_ne!(
            unsafe { GetHandleInformation(raw.cast(), &raw mut flags) },
            0,
            "borrowed native pipe handle starts valid"
        );

        let (close_tx, close_rx) = mpsc::sync_channel(1);
        let closing = server.clone();
        let closer = thread::spawn(move || {
            close_tx
                .send(closing.close_terminal())
                .expect("report terminal close result");
        });
        // A correct implementation may hold terminal close behind the
        // short io_issue guard. If it completes before release, the in-flight
        // read MUST still pin the original owned kernel HANDLE.
        let early_close = match close_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(result) => Some(result),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(error) => panic!("terminal close reporter disconnected: {error}"),
        };
        let raw_stayed_valid = if early_close.is_some() {
            // SAFETY: this is exactly the raw HANDLE borrowed by the parked
            // read, and flags is a live writable output. A stale value returns
            // ERROR_INVALID_HANDLE; no dereference or ownership transfer.
            (unsafe { GetHandleInformation(raw.cast(), &raw mut flags) }) != 0
        } else {
            true
        };
        release_tx
            .send(())
            .expect("release source reader after close race is staged");
        let observation = read_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("native read has a finite terminal outcome");
        reader.join().expect("native reader joined");
        let terminal = match early_close {
            Some(result) => result,
            None => close_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("terminal close finishes after reader issuance"),
        };
        closer.join().expect("terminal owner joined");
        drop(client);
        drop(server);

        terminal.expect("terminal close succeeds without destroying live I/O state");
        assert!(
            raw_stayed_valid,
            "terminal close disposed the HANDLE between raw borrow and native ReadFile"
        );
        match observation {
            Ok(0) => {}
            Err(error) if matches!(error.raw_os_error(), Some(995 | 109 | 232 | 233)) => {}
            other => panic!("terminal read used a disposed/wrong HANDLE: {other:?}"),
        }
        assert_eq!(
            process_handle_count().expect("final native handle count"),
            baseline,
            "terminal close and a racing read must not leak any pipe/event handle"
        );
    }

    /// Native failure-first (GH-910): once `ReadFile` is pending and `io_issue`
    /// is released, a terminal close runs to completion, yet the exact raw
    /// HANDLE the pending read still passes to `GetOverlappedResult` and
    /// `CancelIoEx` must stay valid until that read observes its completion.
    /// Removing the read-side `Arc` pin makes the close free the HANDLE here.
    #[test]
    fn terminal_close_after_pending_read_issue_keeps_the_read_handle_valid() {
        use std::io::Read as _;
        use std::os::windows::io::AsRawHandle as _;
        use std::sync::{PoisonError, mpsc};
        use std::thread;

        use crate::bootstrap::TestConsumeGate;
        use windows_sys::Win32::Foundation::GetHandleInformation;

        use super::{AFTER_PENDING_IO_ISSUE, process_handle_count};

        let (baseline, server, client, mut stream) = warmed_silent_server_stream("pending");
        // Kill switch only: the terminal close cancels the read long before.
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("bounded native read");

        let (entered, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release) = mpsc::channel();
        let (read_tx, read_rx) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            AFTER_PENDING_IO_ISSUE.with(|gate| {
                *gate.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some(TestConsumeGate { entered, release });
            });
            let mut byte = [0_u8; 1];
            read_tx
                .send(stream.read(&mut byte))
                .expect("send native read observation");
        });
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("the silent peer's ReadFile is pending with io_issue released");

        let raw = server
            .inspect_owned_pipe(|owned| Ok(owned.as_raw_handle()))
            .expect("native handle identity before terminal close");
        let mut flags = 0_u32;
        // SAFETY: the server still owns this pipe HANDLE; flags is writable.
        let valid_before = unsafe { GetHandleInformation(raw.cast(), &raw mut flags) } != 0;

        // The parked read must already have released io_issue; otherwise the
        // close below would block behind it. Fail instead of hanging.
        if server.inner.io_issue.try_lock().is_err() {
            release_tx
                .send(())
                .expect("release the reader before failing");
            reader.join().expect("native reader joined");
            panic!("a pending ReadFile still holds io_issue at its completion wait");
        }
        // io_issue is free, so the terminal close is not held behind the read:
        // it cancels the pending I/O and drops the server's HANDLE reference
        // while the read has not yet observed its completion.
        let terminal = server.close_terminal();
        let server_released = server
            .inspect_owned_pipe(|owned| Ok(owned.as_raw_handle()))
            .is_err();
        // SAFETY: GetHandleInformation only queries the exact captured HANDLE
        // value and writes the live `flags`; a freed, not yet reused value fails
        // with ERROR_INVALID_HANDLE. No ownership transfer or dereference occurs.
        let valid_while_pending = unsafe { GetHandleInformation(raw.cast(), &raw mut flags) } != 0;

        release_tx
            .send(())
            .expect("release the pending reader after the close completed");
        let observation = read_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("native read has a finite terminal outcome");
        reader.join().expect("native reader joined");
        drop(client);
        drop(server);

        terminal.expect("terminal close completes while the read is pending");
        assert!(valid_before, "original named pipe HANDLE was never live");
        assert!(
            server_released,
            "terminal close must drop the server's own HANDLE reference"
        );
        assert!(
            valid_while_pending,
            "terminal close freed the HANDLE a pending ReadFile still uses"
        );
        match observation {
            Ok(0) => {}
            Err(error) if matches!(error.raw_os_error(), Some(995 | 109 | 232 | 233)) => {}
            other => panic!("pending read used a disposed/wrong HANDLE: {other:?}"),
        }
        assert_eq!(
            process_handle_count().expect("final native handle count"),
            baseline,
            "terminal close and a pending read must not leak any pipe/event handle"
        );
    }

    /// Fail-first native kernel HANDLE oracle: a terminal close must not
    /// invalidate an acceptor's already borrowed HANDLE before `ConnectNamedPipe`.
    #[test]
    fn terminal_close_cannot_invalidate_a_borrowed_accept_handle() {
        use std::os::windows::io::AsRawHandle as _;
        use std::sync::{PoisonError, mpsc};
        use std::thread;

        use super::{WindowsNamedPipeServer, process_handle_count};
        use crate::bootstrap::{TestConsumeGate, random_test_locator};
        use windows_sys::Win32::Foundation::GetHandleInformation;

        // Prime the native pipe/ACL path before counting possible leaked OS handles.
        let warm = WindowsNamedPipeServer::bind(&format!(
            r"\\.\pipe\keld-gh920-warm-{}",
            random_test_locator().unwrap()
        ))
        .expect("create warm native pipe");
        warm.close_terminal().expect("warm terminal close");
        drop(warm);
        let baseline = process_handle_count().expect("warm baseline");

        let server = WindowsNamedPipeServer::bind(&format!(
            r"\\.\pipe\keld-gh920-accept-{}",
            random_test_locator().unwrap()
        ))
        .expect("create real Windows named pipe");
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        *server
            .inner
            .after_accept_pipe_borrow
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(TestConsumeGate {
            entered: entered_tx,
            release: release_rx,
        });
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let acceptor_server = server.clone();
        let acceptor = thread::spawn(move || {
            result_tx
                .send(acceptor_server.accept_until(Some(Instant::now() + Duration::from_secs(2))))
                .expect("report exact kernel accept outcome");
        });
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("acceptor borrowed raw HANDLE before native ConnectNamedPipe");

        let raw = server
            .inspect_owned_pipe(|owned| Ok(owned.as_raw_handle()))
            .expect("read original kernel HANDLE identity");
        let mut flags = 0_u32;
        // SAFETY: the server still owns this live pipe HANDLE; flags is writable.
        let valid_before = unsafe { GetHandleInformation(raw.cast(), &raw mut flags) } != 0;

        // Real terminal close proceeds while ConnectNamedPipe is not submitted.
        // This needs no peer connection and does not rely on scheduling sleeps.
        let close = server.close_terminal();
        // SAFETY: GetHandleInformation only queries validity of the exact
        // captured HANDLE value; no ownership transfer or dereference occurs.
        let valid_after = unsafe { GetHandleInformation(raw.cast(), &raw mut flags) } != 0;
        release_tx.send(()).expect("release native acceptor");
        let observed = result_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("accept must report and not hang");
        acceptor.join().expect("join native acceptor");
        drop(server);
        close.expect("terminal close succeeds");
        assert!(valid_before, "original named pipe HANDLE was never live");
        assert!(
            valid_after,
            "terminal close invalidated a HANDLE already borrowed by accept_until"
        );
        let safely_stopped = matches!(
            &observed,
            Err(error) if error.kind() == std::io::ErrorKind::NotConnected
        );
        assert!(
            safely_stopped,
            "accept must stop without using a stale native HANDLE: {observed:?}"
        );
        assert_eq!(
            process_handle_count().expect("final process HANDLE count"),
            baseline,
            "no kernel HANDLE/event may leak after close versus accept"
        );
    }

    /// A completed native connection must not resurrect terminal pipe state.
    /// The real client establishes `ERROR_PIPE_CONNECTED` before the acceptor
    /// calls `ConnectNamedPipe`; a one-shot gate stops before `connected.store`.
    #[test]
    fn terminal_close_cannot_be_followed_by_late_accept_ready_publication() {
        use std::sync::{PoisonError, mpsc};
        use std::thread;

        use super::{WindowsNamedPipeServer, process_handle_count};
        use crate::bootstrap::{TestConsumeGate, random_test_locator};

        let warm = WindowsNamedPipeServer::bind(&format!(
            r"\\.\pipe\keld-gh920-publication-warm-{}",
            random_test_locator().expect("warm unique endpoint")
        ))
        .expect("warm real pipe");
        warm.close_terminal().expect("warm native close");
        drop(warm);
        let baseline = process_handle_count().expect("kernel handle census");

        let endpoint = format!(
            r"\\.\pipe\keld-gh920-publish-{}",
            random_test_locator().expect("unique endpoint")
        );
        let server = WindowsNamedPipeServer::bind(&endpoint).expect("real native listener");
        let client = WindowsNamedPipeServer::connect_client(&endpoint)
            .expect("real client connected before native accept");
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        *server
            .inner
            .after_accept_connect_result
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(TestConsumeGate {
            entered: entered_tx,
            release: release_rx,
        });
        let (accept_tx, accept_rx) = mpsc::sync_channel(1);
        let accept_server = server.clone();
        let acceptor = thread::spawn(move || {
            accept_tx
                .send(accept_server.accept_until(Some(Instant::now() + Duration::from_secs(2))))
                .expect("report real accept result");
        });
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("native ConnectNamedPipe returned, but state is not yet published");
        assert!(
            !server.is_connected(),
            "gate precedes connected publication"
        );

        let (close_tx, close_rx) = mpsc::sync_channel(1);
        let close_server = server.clone();
        let closer = thread::spawn(move || {
            close_tx
                .send(close_server.close_terminal())
                .expect("report real terminal close");
        });
        let terminal = close_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("terminal close cannot depend on pending state publication");
        closer.join().expect("join close owner");
        release_tx
            .send(())
            .expect("release completed native accept result");
        let observation = accept_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("late accept must reach a bounded final result");
        acceptor.join().expect("join native acceptor");
        terminal.expect("native terminal close succeeds");
        let still_connected = server.is_connected();
        drop(client);
        drop(server);

        assert!(
            matches!(
                &observation,
                Err(error) if error.kind() == std::io::ErrorKind::NotConnected
            ),
            "terminal must refuse a late native Ready/PeerClosed: {observation:?}"
        );
        assert!(
            !still_connected,
            "terminal pipe must never be re-published as connected"
        );
        assert_eq!(
            process_handle_count().expect("kernel handle count after teardown"),
            baseline,
            "accept/terminal race must leak no pipe/event/connection handles"
        );
    }

    #[test]
    fn a_wait_timeout_rounds_a_fractional_millisecond_up_and_never_down() {
        assert_eq!(
            duration_to_wait_ms(Some(Duration::from_micros(4_999_900))),
            5_000
        );
        assert_eq!(duration_to_wait_ms(Some(Duration::from_micros(1_001))), 2);
        assert_eq!(duration_to_wait_ms(Some(Duration::from_secs(5))), 5_000);
        assert_eq!(duration_to_wait_ms(Some(Duration::from_nanos(1))), 1);
        assert_eq!(duration_to_wait_ms(Some(Duration::ZERO)), 1);
        assert_eq!(duration_to_wait_ms(None), INFINITE);
        assert_eq!(duration_to_wait_ms(Some(Duration::MAX)), INFINITE - 1);
    }

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

    #[test]
    fn absolute_deadline_clamps_per_stream_timeout_and_expired_refuses() {
        assert_eq!(
            validate_timeout(Some(Duration::ZERO))
                .expect_err("zero timeout must be rejected")
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
        validate_timeout(None).expect("an unbounded timeout is permitted");
        validate_timeout(Some(Duration::from_nanos(1)))
            .expect("one nonzero nanosecond remains a legal configured timeout");

        let short = Duration::from_millis(23);
        assert_eq!(effective_timeout(None, None).expect("unbounded"), None);
        assert_eq!(
            effective_timeout(Some(short), None).expect("no absolute deadline"),
            Some(short)
        );

        // The configured per-I/O limit cannot be extended by a later absolute
        // deadline; the exact 23ms is independent of the wall-clock sample.
        let distant = Instant::now()
            .checked_add(Duration::from_mins(2))
            .expect("bounded distant deadline");
        assert_eq!(
            effective_timeout(Some(short), Some(distant)).expect("short configured limit"),
            Some(short)
        );

        // Conversely, a long per-I/O limit cannot extend the enclosing
        // absolute one. Verify an independently bounded positive duration,
        // rather than reconstructing the production clock algorithm.
        let enclosing = Instant::now()
            .checked_add(Duration::from_mins(1))
            .expect("bounded enclosing deadline");
        let observed = effective_timeout(Some(Duration::from_mins(5)), Some(enclosing))
            .expect("enclosing deadline still live")
            .expect("an absolute deadline must bound a write");
        assert!(observed > Duration::ZERO);
        assert!(observed <= Duration::from_mins(1), "{observed:?}");

        // An unconfigured stream must still have a finite enclosing deadline;
        // otherwise a silent peer can wait forever despite caller cancellation.
        let unconfigured = effective_timeout(None, Some(enclosing))
            .expect("enclosing deadline remains live")
            .expect("absolute deadline must bound an unconfigured stream");
        assert!(unconfigured > Duration::ZERO);
        assert!(unconfigured <= Duration::from_mins(1), "{unconfigured:?}");

        let expired = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("monotonic instant subtraction");
        let error = effective_timeout(Some(short), Some(expired))
            .expect_err("a passed absolute deadline cannot start new I/O");
        assert_eq!(error.raw_os_error(), Some(121));
        assert_eq!(
            effective_timeout(None, Some(expired))
                .expect_err("expired enclosing deadline refuses even unconfigured I/O")
                .raw_os_error(),
            Some(121)
        );
        assert_eq!(connect_deadline_error().raw_os_error(), Some(121));
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
