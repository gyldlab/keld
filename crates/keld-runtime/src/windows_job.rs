//! Windows host-death and launcher-attempt process-family Job ownership.
//!
//! This is supervisor cleanup, not LPAC containment. The host installs one
//! unnamed, non-inheritable Job before any Bun role exists. The host is a Job
//! member, so later children inherit membership without a spawn/assignment
//! race. The sole Job handle intentionally lives until process termination;
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` then terminates the enrolled tree even
//! when host destructors cannot run.
//!
//! The module also holds the updater helper's System32-only DLL search and its
//! elevated `runas` launch (KEL-53 §4 "Helper launch and self-anchor"), because
//! it owns the crate's KEL-270 FFI whitelist.

#![allow(unsafe_code)] // isolated Win32 Job/loader/shell ABI; every call has a local handle/pointer proof
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::OsStr;
use std::io;
use std::io::{Read as _, Write as _};
use std::marker::PhantomData;
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::windows::io::{
    AsHandle, AsRawHandle as _, BorrowedHandle, FromRawHandle as _, OwnedHandle,
};
use std::path::{Component, Path, PathBuf, Prefix};
use std::process::Child;
use std::time::{Duration, Instant};

use keld_ipc::{
    WindowsLifecycleBinding, WindowsLifecyclePurpose, WindowsLifecycleRendezvousClient,
    WindowsLifecycleRendezvousPeer, WindowsNamedPipeBootstrapStream,
};

use crate::windows_lpac::{WindowsLpacError, WindowsSuspendedChild, nul_terminated_wide};

use windows_sys::Win32::Foundation::{
    CompareObjectHandles, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_CANCELLED,
    ERROR_NOT_SAME_OBJECT, FILETIME, GetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT,
    INVALID_HANDLE_VALUE, S_FALSE, S_OK, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_TYPE_DISK, FILE_TYPE_PIPE, GetFileType, ReadFile,
};
use windows_sys::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE};
use windows_sys::Win32::System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
    JOBOBJECT_ASSOCIATE_COMPLETION_PORT, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectAssociateCompletionPortInformation,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::LibraryLoader::{
    LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::SystemServices::{JOB_OBJECT_QUERY, JOB_OBJECT_TERMINATE};
use windows_sys::Win32::System::Threading::{
    CreateWaitableTimerW, GetCurrentProcess, GetProcessId, GetProcessTimes, OpenProcess,
    OpenProcessToken, PROCESS_DUP_HANDLE, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, QueryFullProcessImageNameW, SetWaitableTimer, TerminateProcess,
    WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};

const HOST_START_TOKEN_V1: &[u8; 10] = b"KELDHOST/1";
/// Fixed one-shot stage-cleanup signal written by a live `keld dev` launcher.
pub const WINDOWS_DEV_STAGE_CLEANUP_RELEASE_V1: &[u8; 14] = b"KELD-CLEANUP/1";
const LIFECYCLE_ATTEMPT_HANDOFF_LEN: usize = 8 + 1 + 32 + 32 + 8 + 8;
const LIFECYCLE_ATTEMPT_ACK_LEN: usize = 8 + 32 + 32 + 4;
const LIFECYCLE_QUERY_HANDOFF_LEN: usize = 8 + 1 + 32 + 32 + 8;

/// Result of the cleanup helper's private launcher pipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsCleanupReleaseSignal {
    /// Launcher wrote the exact fixed token and closed its pipe.
    Released,
    /// Launcher disappeared before sending a token; the helper must reap the Job.
    LauncherLost,
    /// The pipe contained a partial, wrong, or extended record; the helper must
    /// reap the Job and preserve diagnostics.
    Malformed,
}

/// Host environment selector set only by the installed/developer process launcher.
pub const WINDOWS_LAUNCH_GATE_ENV: &str = "KELD_WINDOWS_LAUNCH_GATE";
/// Exact environment value selecting the attempt-Job startup gate.
pub const WINDOWS_LAUNCH_GATE_ATTEMPT_JOB_V1: &str = "attempt-job-v1";

/// Sole owning handle for one non-breakaway Windows process-family Job.
///
/// A launcher retains this handle while its host attempt is live. Closing the last
/// handle kills every enrolled process; orderly replacement uses
/// [`Self::terminate_and_wait`] and observes the exact active-process count before
/// releasing the handle.
#[derive(Debug)]
pub struct WindowsProcessJob {
    handle: OwnedHandle,
    host_assigned: bool,
    host_process_id: Option<u32>,
    host_process: Option<OwnedHandle>,
}

/// Least-rights view of the exact attempt Job transferred to a lifecycle keeper.
///
/// This owner can query and terminate the transferred Job only. It cannot add
/// members or change Job limits. Transport authentication and attempt binding
/// remain the caller's responsibility; this type does not authenticate a pipe.
#[derive(Debug)]
pub struct WindowsLifecycleJobWitness {
    handle: OwnedHandle,
    coordinator: WindowsProcessPeer,
}

/// Read-only query handle to the exact attempt Job, transferred to a successor
/// for an independent ActiveProcesses-zero check.
#[derive(Debug)]
pub struct WindowsLifecycleQueryWitness {
    handle: OwnedHandle,
}

/// Keeper ownership acquired as one authenticated attempt handoff.
///
/// Dropping this value closes both the exact bounded Job witness and the
/// reduced activation-lease retention handle; callers must retain it until the
/// journal owner has authenticated exact family retirement.
#[derive(Debug)]
pub struct WindowsLifecycleKeeperHandoff {
    witness: WindowsLifecycleJobWitness,
    activation_lease: Option<OwnedHandle>,
    binding: WindowsLifecycleBinding,
    retirement_started: bool,
}

/// Successor-owned QUERY-only witness for an exact zero-process attempt Job.
///
/// Its attempt/channel IDs are keeper-supplied facts until the successor
/// reacquires the writer lease and validates the protected journal. This value
/// grants no updater write authority.
#[derive(Debug)]
pub struct WindowsLifecycleRetirementWitness {
    witness: WindowsLifecycleQueryWitness,
    binding: WindowsLifecycleBinding,
    knowledge: keld_ipc::WindowsLifecycleBindingKnowledge,
}

/// Staged successor observation of Job zero before it acknowledges to the keeper.
///
/// Holding this value keeps the query-only witness and authenticated connection
/// alive while the keeper must still retain the writer lease. Dropping it sends
/// no zero acknowledgement.
#[must_use = "the keeper retains its writer lease until this staged proof is acknowledged"]
#[derive(Debug)]
pub struct WindowsLifecycleRetirementPending {
    witness: WindowsLifecycleQueryWitness,
    binding: WindowsLifecycleBinding,
    knowledge: keld_ipc::WindowsLifecycleBindingKnowledge,
    connection: WindowsLifecycleRendezvousClient<WindowsProcessPeer>,
}

impl WindowsLifecycleQueryWitness {
    /// Adopts a transferred Job handle and attenuates it to QUERY only.
    /// The caller MUST authenticate the handoff and bind the Job to the exact
    /// install/attempt before constructing this witness.
    ///
    /// # Errors
    ///
    /// Returns an error if handle validation, query-right attenuation, or the
    /// first accounting readback fails.
    pub fn adopt_transferred(transferred: OwnedHandle) -> Result<Self, WindowsHostJobError> {
        let raw = transferred.as_raw_handle().cast();
        let mut flags = 0_u32;
        // SAFETY: transferred is owned/live and flags is writable.
        if unsafe { GetHandleInformation(raw, &raw mut flags) } == 0 {
            return Err(WindowsHostJobError::new(
                "successor Job witness handle read-back",
            ));
        }
        if flags & HANDLE_FLAG_INHERIT != 0 {
            return Err(WindowsHostJobError::contract(
                "successor Job witness handle read-back",
                "successor Job witness must be non-inheritable",
            ));
        }
        let mut reduced = std::ptr::null_mut();
        // SAFETY: source is owned by transferred; only QUERY is granted to the
        // successor's retained witness.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                raw,
                GetCurrentProcess(),
                &raw mut reduced,
                JOB_OBJECT_QUERY,
                0,
                0,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "successor Job witness attenuation",
            ));
        }
        // SAFETY: successful DuplicateHandle returned one fresh owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(reduced.cast()) };
        drop(transferred);
        let witness = Self { handle };
        witness.active_processes()?;
        Ok(witness)
    }

    /// Returns the exact transferred Job's OS-reported active process count.
    ///
    /// # Errors
    ///
    /// Returns an error if the Job query right or accounting read fails.
    pub fn active_processes(&self) -> Result<u32, WindowsHostJobError> {
        query_job_active_processes(self.handle.as_raw_handle().cast())
    }
}

impl WindowsLifecycleJobWitness {
    /// Adopts a transferred least-rights handle and verifies it can query Job
    /// accounting. The caller MUST authenticate the transfer and bind it to the
    /// exact attempt before calling this constructor. The retained coordinator
    /// process handle gates termination until that exact process has exited.
    ///
    /// # Errors
    ///
    /// Returns an error if the handle is inheritable or cannot query Job state.
    pub fn adopt_transferred(
        transferred: OwnedHandle,
        coordinator: WindowsProcessPeer,
    ) -> Result<Self, WindowsHostJobError> {
        if coordinator.has_exited()? {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper coordinator registration",
                "coordinator must be live when the keeper adopts this attempt",
            ));
        }
        let raw = transferred.as_raw_handle().cast();
        let mut flags = 0_u32;
        // SAFETY: `transferred` is an owned, live handle and `flags` is writable.
        if unsafe { GetHandleInformation(raw, &raw mut flags) } == 0 {
            return Err(WindowsHostJobError::new(
                "lifecycle keeper Job handle read-back",
            ));
        }
        if flags & HANDLE_FLAG_INHERIT != 0 {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper Job handle read-back",
                "transferred Job handle must not be inheritable",
            ));
        }
        let mut reduced = std::ptr::null_mut();
        let rights = JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE;
        // SAFETY: `transferred` remains owned and the new handle is attenuated
        // to only the keeper's exact query/terminate requirements.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                raw,
                GetCurrentProcess(),
                &raw mut reduced,
                rights,
                0,
                0,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "lifecycle keeper Job handle attenuation",
            ));
        }
        // SAFETY: successful DuplicateHandle returned one fresh owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(reduced.cast()) };
        drop(transferred);
        let mut retained_flags = 0_u32;
        // SAFETY: the attenuated handle remains owned and flags is writable.
        if unsafe { GetHandleInformation(reduced, &raw mut retained_flags) } == 0 {
            return Err(WindowsHostJobError::new(
                "lifecycle keeper Job handle read-back",
            ));
        }
        if retained_flags & HANDLE_FLAG_INHERIT != 0 {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper Job handle read-back",
                "attenuated keeper Job handle unexpectedly permits inheritance",
            ));
        }
        let witness = Self {
            handle,
            coordinator,
        };
        witness.active_processes()?;
        Ok(witness)
    }

    /// Returns the exact transferred Job's OS-reported active process count.
    ///
    /// # Errors
    ///
    /// Returns an error if the handle lacks Job query rights or accounting fails.
    pub fn active_processes(&self) -> Result<u32, WindowsHostJobError> {
        query_job_active_processes(self.handle.as_raw_handle().cast())
    }

    /// Transfers a QUERY-only witness for this exact Job to an authenticated
    /// successor process outside the Job. The target value is local to that
    /// process and MUST be sent only over the live one-shot handoff connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the target exited, was reused, belongs to the attempt
    /// Job, or handle duplication fails.
    pub fn transfer_query_witness_to(
        &self,
        target: &WindowsProcessPeer,
    ) -> Result<usize, WindowsHostJobError> {
        let target_process = open_lifecycle_transfer_target(target, &self.handle)?;
        let mut remote = std::ptr::null_mut();
        // SAFETY: the keeper's exact Job handle is retained, the target is a
        // revalidated process outside that Job, and only JOB_OBJECT_QUERY is granted.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.handle.as_raw_handle().cast(),
                target_process.as_raw_handle().cast(),
                &raw mut remote,
                JOB_OBJECT_QUERY,
                0,
                0,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "successor Job witness duplication",
            ));
        }
        if remote.is_null() || remote == INVALID_HANDLE_VALUE {
            return Err(WindowsHostJobError::contract(
                "successor Job witness duplication",
                "Windows returned an invalid target-process handle value",
            ));
        }
        Ok(remote as usize)
    }

    /// Waits for the exact coordinator process object pinned at keeper adoption.
    ///
    /// # Errors
    ///
    /// Returns an error if Windows cannot wait on the retained coordinator handle.
    pub fn wait_for_coordinator_exit(
        &self,
        timeout: Duration,
    ) -> Result<bool, WindowsHostJobError> {
        self.coordinator.wait_until_exited(timeout)
    }

    /// Terminates the exact transferred Job and waits until Windows reports
    /// zero active members. A zero result is the only success result.
    ///
    /// # Errors
    ///
    /// Returns an error if termination/query is denied or the bounded wait expires.
    pub fn terminate_and_wait_zero(&self, timeout: Duration) -> Result<(), WindowsHostJobError> {
        if !self.coordinator.has_exited()? {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper coordinator state",
                "refusing to terminate the attempt while its exact coordinator is live",
            ));
        }
        // SAFETY: the retained transferred handle is live and has only the
        // explicitly requested JOB_OBJECT_TERMINATE right.
        if unsafe { TerminateJobObject(self.handle.as_raw_handle().cast(), 1) } == 0 {
            return Err(WindowsHostJobError::new("lifecycle keeper Job termination"));
        }
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            WindowsHostJobError::contract("lifecycle keeper Job wait", "timeout deadline overflow")
        })?;
        // The keeper deliberately lacks JOB_OBJECT_SET_ATTRIBUTES, so it cannot
        // attach a new completion port. A private waitable timer wakes bounded
        // accounting reads without adding Job rights.
        // SAFETY: null attributes/name request one unnamed waitable timer.
        let raw_timer = unsafe { CreateWaitableTimerW(std::ptr::null(), 0, std::ptr::null()) };
        if raw_timer.is_null() || raw_timer == INVALID_HANDLE_VALUE {
            return Err(WindowsHostJobError::new(
                "lifecycle keeper wait timer creation",
            ));
        }
        // SAFETY: CreateWaitableTimerW returned a fresh valid timer handle.
        let timer = unsafe { OwnedHandle::from_raw_handle(raw_timer.cast()) };
        let due_time = -250_000_i64;
        // SAFETY: `timer` is live; the negative due time is relative and no APC
        // callback/context is registered.
        if unsafe {
            SetWaitableTimer(
                raw_timer,
                &raw const due_time,
                25,
                None,
                std::ptr::null(),
                0,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new("lifecycle keeper wait timer arm"));
        }
        loop {
            if self.active_processes()? == 0 {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(WindowsHostJobError::contract(
                    "lifecycle keeper Job wait",
                    "timed out before the exact Job reached zero active processes",
                ));
            }
            let millis = u32::try_from(remaining.as_millis().clamp(1, 100)).unwrap_or(u32::MAX - 1);
            // SAFETY: the private timer remains owned during this bounded wait.
            match unsafe { WaitForSingleObject(timer.as_raw_handle().cast(), millis) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT if Instant::now() < deadline => {}
                WAIT_TIMEOUT => {
                    return Err(WindowsHostJobError::contract(
                        "lifecycle keeper Job wait",
                        "timed out before the exact Job reached zero active processes",
                    ));
                }
                _ => return Err(WindowsHostJobError::new("lifecycle keeper timer wait")),
            }
        }
    }
}

impl WindowsLifecycleKeeperHandoff {
    /// Receives and adopts the one-shot coordinator-to-keeper Job/lease bundle.
    ///
    /// The client connection must have independently expected the complete
    /// attempt binding. The exact connected coordinator process object is cloned
    /// into the Job witness; the activation lease is attenuated again in this
    /// process before the keeper acknowledges adoption.
    ///
    /// This method MUST run in a dedicated one-shot keeper process. Before both
    /// remote handles become RAII owners, any wrong purpose/binding, malformed or
    /// partial bundle, or invalid handle hard-terminates that process so unknown
    /// duplicated handles cannot remain open. After adoption, ordinary errors drop
    /// both owners before returning.
    ///
    /// # Errors
    ///
    /// Returns an error for failed rights attenuation, zero-family admission, or
    /// a failed acknowledgement write. Pre-adoption protocol failures terminate
    /// the process instead of returning.
    pub fn receive_attempt_bundle(
        mut connection: WindowsLifecycleRendezvousClient<WindowsProcessPeer>,
        io_deadline: Instant,
    ) -> Result<Self, WindowsHostJobError> {
        let binding = connection.binding();
        if binding.purpose() != WindowsLifecyclePurpose::CoordinatorToKeeper
            || connection.binding_knowledge()
                != keld_ipc::WindowsLifecycleBindingKnowledge::IndependentlyExpected
        {
            terminate_incomplete_lifecycle_keeper(
                "keeper requires independently expected coordinator-to-keeper binding",
            );
        }
        connection.set_io_deadline(io_deadline);
        let mut record = [0_u8; LIFECYCLE_ATTEMPT_HANDOFF_LEN];
        if connection.stream_mut().read_exact(&mut record).is_err() {
            terminate_incomplete_lifecycle_keeper("incomplete coordinator handle bundle");
        }
        if record[..8] != *b"KELD-HO1"
            || record[8] != WindowsLifecyclePurpose::CoordinatorToKeeper as u8
            || record[9..41] != *binding.attempt_id()
            || record[41..73] != *binding.lifecycle_channel_id()
        {
            terminate_incomplete_lifecycle_keeper(
                "offer does not match the authenticated one-shot attempt binding",
            );
        }
        let Ok(remote_job_bytes) = <[u8; 8]>::try_from(&record[73..81]) else {
            terminate_incomplete_lifecycle_keeper("bad remote Job handle field");
        };
        let Ok(remote_job) = usize::try_from(u64::from_le_bytes(remote_job_bytes)) else {
            terminate_incomplete_lifecycle_keeper("remote Job handle does not fit");
        };
        let Ok(remote_lease_bytes) = <[u8; 8]>::try_from(&record[81..89]) else {
            terminate_incomplete_lifecycle_keeper("bad remote lease handle field");
        };
        let Ok(remote_lease) = usize::try_from(u64::from_le_bytes(remote_lease_bytes)) else {
            terminate_incomplete_lifecycle_keeper("remote lease handle does not fit");
        };
        if remote_job == 0
            || remote_lease == 0
            || remote_job == INVALID_HANDLE_VALUE as usize
            || remote_lease == INVALID_HANDLE_VALUE as usize
            || remote_job == remote_lease
        {
            terminate_incomplete_lifecycle_keeper(
                "remote handle pair contains a null, invalid or aliased handle",
            );
        }

        // SAFETY: the authenticated coordinator duplicated these exact values
        // into this process immediately before writing the fixed HO1 record.
        let transferred_job =
            unsafe { OwnedHandle::from_raw_handle((remote_job as *mut std::ffi::c_void).cast()) };
        // SAFETY: the same exact-process DuplicateHandle transfer created this
        // lease value; both handles remain local owners from this point onward.
        let transferred_lease =
            unsafe { OwnedHandle::from_raw_handle((remote_lease as *mut std::ffi::c_void).cast()) };
        let coordinator = connection.process_pin().try_clone()?;
        let witness = WindowsLifecycleJobWitness::adopt_transferred(transferred_job, coordinator)?;
        let active_processes = witness.active_processes()?;
        if active_processes == 0 {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper attempt bundle",
                "coordinator-to-keeper transfer requires a live attempt family",
            ));
        }
        let activation_lease = attenuate_keeper_activation_lease(transferred_lease)?;
        let acknowledgement = encode_lifecycle_attempt_ack(binding, active_processes);
        connection
            .stream_mut()
            .write_all(&acknowledgement)
            .map_err(|error| {
                WindowsHostJobError::contract(
                    "lifecycle keeper attempt bundle acknowledgement",
                    error.to_string(),
                )
            })?;
        Ok(Self {
            witness,
            activation_lease: Some(activation_lease),
            binding,
            retirement_started: false,
        })
    }

    /// Exact Job witness retained by this accepted keeper handoff.
    #[must_use]
    pub const fn witness(&self) -> &WindowsLifecycleJobWitness {
        &self.witness
    }

    /// Queries the exact Job active count while the activation writer lease remains retained.
    ///
    /// # Errors
    ///
    /// Returns an error if Windows cannot query the transferred Job.
    pub fn active_processes(&self) -> Result<u32, WindowsHostJobError> {
        self.witness.active_processes()
    }

    /// Retained reduced-rights reference to the exact share-zero activation lock.
    #[must_use]
    pub const fn activation_lease_retention(&self) -> Option<&OwnedHandle> {
        self.activation_lease.as_ref()
    }

    /// Retires this attempt only after the authenticated successor independently
    /// queries exact Job zero and acknowledges it over its one-shot connection.
    ///
    /// This operation is deliberately non-retryable. Any error keeps the reduced
    /// writer-lease handle in this owner so recovery remains fail-closed. The lease
    /// is dropped only after the successor's exact zero acknowledgement is checked.
    ///
    /// # Errors
    ///
    /// Returns an error for reuse, wrong install/attempt/purpose, successor exit,
    /// an unproven coordinator/family retirement, malformed acknowledgement, or
    /// failed final receipt. A failed result retains the writer-lease handle unless
    /// the authenticated zero acknowledgement had already completed.
    pub fn retire_to_successor(
        &mut self,
        peer: WindowsLifecycleRendezvousPeer<WindowsProcessPeer>,
        timeout: Duration,
        io_deadline: Instant,
    ) -> Result<(), WindowsHostJobError> {
        self.retire_to_successor_inner(peer, timeout, io_deadline, |_| {})
    }

    fn retire_to_successor_inner(
        &mut self,
        mut peer: WindowsLifecycleRendezvousPeer<WindowsProcessPeer>,
        timeout: Duration,
        io_deadline: Instant,
        after_qa1: impl FnOnce(&mut WindowsLifecycleRendezvousPeer<WindowsProcessPeer>),
    ) -> Result<(), WindowsHostJobError> {
        if self.retirement_started || self.activation_lease.is_none() {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor retirement",
                "keeper generation is already consumed or writer lease was released",
            ));
        }
        self.retirement_started = true;
        let expected_binding = self
            .binding
            .with_purpose(WindowsLifecyclePurpose::KeeperToSuccessor);
        if peer.binding() != expected_binding {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor retirement",
                "successor connection changed the keeper's install/attempt/channel binding",
            ));
        }
        if peer.process_pin().has_exited()? {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor retirement",
                "authenticated successor exited before receiving the zero witness",
            ));
        }
        peer.set_io_deadline(io_deadline);
        self.witness.terminate_and_wait_zero(timeout)?;
        let remote_query = self.witness.transfer_query_witness_to(peer.process_pin())?;
        let offer = encode_lifecycle_query_handoff(expected_binding, remote_query)?;
        peer.stream_mut().write_all(&offer).map_err(|error| {
            WindowsHostJobError::contract("lifecycle successor query offer", error.to_string())
        })?;
        let mut acknowledgement = [0_u8; LIFECYCLE_ATTEMPT_ACK_LEN];
        peer.stream_mut()
            .read_exact(&mut acknowledgement)
            .map_err(|error| {
                WindowsHostJobError::contract(
                    "lifecycle successor zero acknowledgement",
                    error.to_string(),
                )
            })?;
        let expected_ack = encode_lifecycle_query_receipt(*b"KELD-QA1", expected_binding, 0);
        if acknowledgement != expected_ack {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor zero acknowledgement",
                "successor did not acknowledge this attempt/channel at exact Job zero",
            ));
        }

        drop(self.activation_lease.take());
        after_qa1(&mut peer);
        let final_receipt = encode_lifecycle_query_receipt(*b"KELD-QF1", expected_binding, 0);
        peer.stream_mut()
            .write_all(&final_receipt)
            .map_err(|error| {
                WindowsHostJobError::contract(
                    "lifecycle successor release receipt",
                    error.to_string(),
                )
            })?;
        Ok(())
    }
}

impl WindowsLifecycleRetirementWitness {
    /// Receives the keeper's query-only Job witness and independently verifies zero.
    ///
    /// In discovery mode, the attempt/channel remain keeper-supplied until the
    /// caller acquires the writer lease and validates the protected journal.
    /// This value itself grants no file or updater mutation rights.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong purpose/binding, malformed remote handle,
    /// nonzero Job census, failed acknowledgement or missing keeper release receipt.
    pub fn receive(
        connection: WindowsLifecycleRendezvousClient<WindowsProcessPeer>,
        io_deadline: Instant,
    ) -> Result<Self, WindowsHostJobError> {
        WindowsLifecycleRetirementPending::receive(connection, io_deadline)?.acknowledge()
    }

    /// Independent OS query of this exact attempt Job.
    ///
    /// # Errors
    ///
    /// Returns an error if the successor query-only handle cannot be read.
    pub fn active_processes(&self) -> Result<u32, WindowsHostJobError> {
        self.witness.active_processes()
    }

    /// Attempt context supplied by the authenticated keeper.
    #[must_use]
    pub const fn binding(&self) -> WindowsLifecycleBinding {
        self.binding
    }

    /// Indicates whether attempt/channel IDs still need journal revalidation.
    #[must_use]
    pub const fn binding_knowledge(&self) -> keld_ipc::WindowsLifecycleBindingKnowledge {
        self.knowledge
    }
}

impl WindowsLifecycleRetirementPending {
    /// Receives the query-only witness and proves that the exact Job is at zero,
    /// while deliberately leaving QA1 unsent so the keeper must retain its lock.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong purpose/binding, malformed handle offer,
    /// failed query-right attenuation or nonzero Job census.
    pub fn receive(
        mut connection: WindowsLifecycleRendezvousClient<WindowsProcessPeer>,
        io_deadline: Instant,
    ) -> Result<Self, WindowsHostJobError> {
        let binding = connection.binding();
        let knowledge = connection.binding_knowledge();
        if binding.purpose() != WindowsLifecyclePurpose::KeeperToSuccessor {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor query witness",
                "successor requires keeper-to-successor purpose",
            ));
        }
        connection.set_io_deadline(io_deadline);
        let mut record = [0_u8; LIFECYCLE_QUERY_HANDOFF_LEN];
        connection
            .stream_mut()
            .read_exact(&mut record)
            .map_err(|error| {
                WindowsHostJobError::contract("lifecycle successor query offer", error.to_string())
            })?;
        if record[..8] != *b"KELD-QO1"
            || record[8] != WindowsLifecyclePurpose::KeeperToSuccessor as u8
            || record[9..41] != *binding.attempt_id()
            || record[41..73] != *binding.lifecycle_channel_id()
        {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor query offer",
                "query witness offer changed the authenticated binding",
            ));
        }
        let remote_query = usize::try_from(u64::from_le_bytes(record[73..81].try_into().map_err(
            |_| {
                WindowsHostJobError::contract(
                    "lifecycle successor query offer",
                    "remote query handle field changed shape",
                )
            },
        )?))
        .map_err(|_| {
            WindowsHostJobError::contract(
                "lifecycle successor query offer",
                "remote query handle does not fit this process",
            )
        })?;
        if remote_query == 0 || remote_query == INVALID_HANDLE_VALUE as usize {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor query offer",
                "remote query handle is null or invalid",
            ));
        }
        // SAFETY: the authenticated keeper duplicated this exact query-only
        // value into the connected successor process before sending QO1.
        let transferred =
            unsafe { OwnedHandle::from_raw_handle((remote_query as *mut std::ffi::c_void).cast()) };
        let witness = WindowsLifecycleQueryWitness::adopt_transferred(transferred)?;
        if witness.active_processes()? != 0 {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor process-family census",
                "exact attempt Job still has active members",
            ));
        }
        Ok(Self {
            witness,
            binding,
            knowledge,
            connection,
        })
    }

    /// Repeats the independent OS census while the keeper still holds the writer lease.
    ///
    /// # Errors
    ///
    /// Returns an error if the exact attempt is no longer at zero.
    pub fn active_processes(&self) -> Result<u32, WindowsHostJobError> {
        self.witness.active_processes()
    }

    /// Attempt context learned from the keeper; revalidate it against the journal after lease acquisition.
    #[must_use]
    pub const fn binding_knowledge(&self) -> keld_ipc::WindowsLifecycleBindingKnowledge {
        self.knowledge
    }

    /// Sends QA1 only after the caller has verified the zero witness and is ready
    /// for the keeper to release writer exclusion; waits for QF1 before returning.
    ///
    /// # Errors
    ///
    /// Returns an error if the Job is no longer zero, the keeper fails to release
    /// its lease or the authenticated final receipt is missing or changed.
    pub fn acknowledge(mut self) -> Result<WindowsLifecycleRetirementWitness, WindowsHostJobError> {
        if self.witness.active_processes()? != 0 {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor zero acknowledgement",
                "exact attempt Job changed from zero before QA1",
            ));
        }
        let acknowledgement = encode_lifecycle_query_receipt(*b"KELD-QA1", self.binding, 0);
        self.connection
            .stream_mut()
            .write_all(&acknowledgement)
            .map_err(|error| {
                WindowsHostJobError::contract(
                    "lifecycle successor zero acknowledgement",
                    error.to_string(),
                )
            })?;
        let mut receipt = [0_u8; LIFECYCLE_ATTEMPT_ACK_LEN];
        self.connection
            .stream_mut()
            .read_exact(&mut receipt)
            .map_err(|error| {
                WindowsHostJobError::contract(
                    "lifecycle successor release receipt",
                    error.to_string(),
                )
            })?;
        let expected_receipt = encode_lifecycle_query_receipt(*b"KELD-QF1", self.binding, 0);
        if receipt != expected_receipt {
            return Err(WindowsHostJobError::contract(
                "lifecycle successor release receipt",
                "keeper did not confirm authenticated lease retirement",
            ));
        }
        Ok(WindowsLifecycleRetirementWitness {
            witness: self.witness,
            binding: self.binding,
            knowledge: self.knowledge,
        })
    }
}

/// Retained process-object identity observed from a connected Windows pipe peer.
///
/// The retained handle prevents PID reuse from changing this observation. The
/// caller still compares `image_path` against a trusted installed-image digest;
/// a PID or pipe DACL alone is not executable authentication.
#[derive(Debug)]
pub struct WindowsProcessPeer {
    process: OwnedHandle,
    process_id: u32,
    session_id: u32,
    token_facts: keld_ipc::WindowsPeerTokenFacts,
    creation_time: u64,
    image_path: PathBuf,
}

impl WindowsProcessPeer {
    /// Opens and pins the exact PID and session reported by the connected pipe.
    ///
    /// # Errors
    ///
    /// Returns an error if the process exits before observation, its PID/session
    /// differs from the pipe report, or its image/creation time cannot be read.
    pub fn open(process_id: u32, pipe_session_id: u32) -> Result<Self, WindowsHostJobError> {
        // SAFETY: the PID came from the OS pipe endpoint query; only identity
        // query and synchronization rights are requested.
        let raw_process = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                process_id,
            )
        };
        if raw_process.is_null() {
            return Err(WindowsHostJobError::new("keeper peer process open"));
        }
        // SAFETY: OpenProcess returned one fresh non-null owning process handle.
        let process = unsafe { OwnedHandle::from_raw_handle(raw_process.cast()) };
        // SAFETY: the retained process handle is live; GetProcessId is read-only.
        let observed_id = unsafe { GetProcessId(process.as_raw_handle().cast()) };
        if observed_id != process_id {
            return Err(WindowsHostJobError::contract(
                "keeper peer process identity",
                "opened process ID differs from the connected pipe peer ID",
            ));
        }
        let token_facts = process_token_facts(process.as_raw_handle().cast())?;
        let observed_session = token_facts.session_id;
        if observed_session != pipe_session_id {
            return Err(WindowsHostJobError::contract(
                "keeper peer session identity",
                "process session differs from the connected pipe session",
            ));
        }
        let mut image = vec![0_u16; 32_768];
        let mut image_units = u32::try_from(image.len()).map_err(|_| {
            WindowsHostJobError::contract(
                "keeper peer image path",
                "image buffer exceeds the Win32 size bound",
            )
        })?;
        // SAFETY: process is retained, image is writable for image_units UTF-16
        // code units, and image_units is writable length storage.
        if unsafe {
            QueryFullProcessImageNameW(
                process.as_raw_handle().cast(),
                0,
                image.as_mut_ptr(),
                &raw mut image_units,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new("keeper peer image query"));
        }
        let image_units = usize::try_from(image_units).map_err(|_| {
            WindowsHostJobError::contract(
                "keeper peer image path",
                "returned image path length exceeds usize",
            )
        })?;
        if image_units == 0 || image_units > image.len() {
            return Err(WindowsHostJobError::contract(
                "keeper peer image path",
                "returned image path length is invalid",
            ));
        }
        let image_path = PathBuf::from(std::ffi::OsString::from_wide(&image[..image_units]));
        let creation_time = process_creation_time(
            process.as_raw_handle().cast(),
            "keeper peer creation-time query",
        )?;
        // A process can exit after the pipe reports its PID. Refuse a stale
        // process object before returning this connected-peer observation.
        // The protocol challenge/ack must still bind the live connection.
        // SAFETY: `process` owns the exact process handle from `OpenProcess`
        // throughout this zero-time, read-only signal query.
        match unsafe { WaitForSingleObject(process.as_raw_handle().cast(), 0) } {
            WAIT_TIMEOUT => {}
            WAIT_OBJECT_0 => {
                return Err(WindowsHostJobError::contract(
                    "keeper peer process state query",
                    "pipe peer exited before its identity was fully observed",
                ));
            }
            _ => return Err(WindowsHostJobError::new("keeper peer process state query")),
        }
        Ok(Self {
            process,
            process_id,
            session_id: observed_session,
            token_facts,
            creation_time,
            image_path,
        })
    }

    /// Duplicates this exact retained process-object observation for an independent
    /// keeper owner; it does not reopen by PID or mint a new peer identity.
    ///
    /// # Errors
    ///
    /// Returns an error if Windows cannot duplicate the retained process handle.
    pub fn try_clone(&self) -> Result<Self, WindowsHostJobError> {
        let mut duplicate = std::ptr::null_mut();
        // SAFETY: `self.process` is an owned live process handle; SAME_ACCESS
        // creates a second handle to that exact kernel object in this process.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.process.as_raw_handle().cast(),
                GetCurrentProcess(),
                &raw mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "keeper peer process pin duplication",
            ));
        }
        // SAFETY: successful DuplicateHandle returned one fresh owning handle.
        let process = unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) };
        Ok(Self {
            process,
            process_id: self.process_id,
            session_id: self.session_id,
            token_facts: self.token_facts.clone(),
            creation_time: self.creation_time,
            image_path: self.image_path.clone(),
        })
    }

    /// PID reported by the connected pipe and pinned by the process handle.
    #[must_use]
    pub const fn process_id(&self) -> u32 {
        self.process_id
    }

    /// Session reported by the connected pipe and re-read from the process.
    #[must_use]
    pub const fn session_id(&self) -> u32 {
        self.session_id
    }

    /// User SID, session and integrity facts queried from this exact process token.
    #[must_use]
    pub const fn token_facts(&self) -> &keld_ipc::WindowsPeerTokenFacts {
        &self.token_facts
    }

    /// Process creation time in Windows FILETIME units.
    #[must_use]
    pub const fn creation_time(&self) -> u64 {
        self.creation_time
    }

    /// Full image path reported by Windows for the retained process object.
    #[must_use]
    pub fn image_path(&self) -> &std::path::Path {
        &self.image_path
    }

    /// Reports whether this exact process object has exited.
    ///
    /// # Errors
    ///
    /// Returns an error if Windows cannot query the retained process handle.
    pub fn has_exited(&self) -> Result<bool, WindowsHostJobError> {
        // SAFETY: this owner retains the exact process object during the query.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle().cast(), 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(WindowsHostJobError::new("keeper peer process state query")),
        }
    }

    /// Waits on this retained exact process object for a bounded duration.
    ///
    /// # Errors
    ///
    /// Returns an error if Windows cannot wait on the retained process handle.
    pub fn wait_until_exited(&self, timeout: Duration) -> Result<bool, WindowsHostJobError> {
        let millis = finite_wait_millis(timeout);
        // SAFETY: `self.process` is a retained process handle with synchronize
        // rights; the timeout is finite and never uses INFINITE.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle().cast(), millis) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(WindowsHostJobError::new("keeper peer process wait")),
        }
    }
}

impl keld_ipc::WindowsLifecyclePeerPin for WindowsProcessPeer {
    fn process_id(&self) -> u32 {
        self.process_id
    }

    fn session_id(&self) -> u32 {
        self.session_id
    }

    fn has_exited(&self) -> io::Result<bool> {
        WindowsProcessPeer::has_exited(self).map_err(|error| io::Error::other(error.to_string()))
    }
}

/// One process that an attempt owner launched suspended, retained with the
/// creation time recorded at launch.
///
/// This is the owner's side of the connect-back claim (KEL-53 §4 "Candidate
/// connect-back", *Acceptance*): only the exact process object that the owner
/// launched and still retains may claim. A process ID identifies a process only
/// until that process exits, so the binding compares kernel objects. Dropping
/// this value drops the child, which terminates a process it has not reaped.
///
/// The record pairs the child with its creation time, so the child is never lent
/// mutably: [`Self::child`] is a shared borrow of a type without interior
/// mutability, and [`Self::resume`], [`Self::wait`] and [`Self::terminate`]
/// forward to it. Safe code therefore cannot `mem::swap` or `mem::replace` the
/// retained child under a record that stays fixed.
#[derive(Debug)]
pub struct WindowsLaunchedProcess {
    child: WindowsSuspendedChild,
    creation_time: u64,
}

impl WindowsLaunchedProcess {
    /// Records the launch identity of a child that a launch path created
    /// suspended.
    ///
    /// The process ID is the child's own, which process creation reported; the
    /// creation time is read from the retained launch handle. Creation time is
    /// fixed for the process object, so recording after resume is harmless: it
    /// reads the same value as recording before resume, and the record names the
    /// object the child retains, not the moment of the call.
    ///
    /// # Errors
    ///
    /// Returns an error, and terminates the child as it drops, when Windows
    /// cannot read the creation time.
    pub fn record(child: WindowsSuspendedChild) -> Result<Self, WindowsHostJobError> {
        let creation_time = process_creation_time(
            child.process_handle().as_raw_handle().cast(),
            "launch identity record",
        )?;
        Ok(Self {
            child,
            creation_time,
        })
    }

    /// Borrows the retained child, for example to verify it before resume.
    #[must_use]
    pub const fn child(&self) -> &WindowsSuspendedChild {
        &self.child
    }

    /// Resumes the retained child's primary thread exactly once, through
    /// [`WindowsSuspendedChild::resume`].
    ///
    /// # Errors
    ///
    /// Fails if the child was already resumed or the kernel rejects resume.
    pub fn resume(&mut self) -> Result<(), WindowsLpacError> {
        self.child.resume()
    }

    /// Waits for the retained child to terminate and returns its exact Windows
    /// exit code, through [`WindowsSuspendedChild::wait`].
    ///
    /// # Errors
    ///
    /// Fails on timeout or process-status query failure.
    pub fn wait(&mut self, timeout_ms: u32) -> Result<u32, WindowsLpacError> {
        self.child.wait(timeout_ms)
    }

    /// Terminates the retained child, through [`WindowsSuspendedChild::terminate`].
    ///
    /// # Errors
    ///
    /// Fails if Windows rejects termination.
    pub fn terminate(&mut self, exit_code: u32) -> Result<(), WindowsLpacError> {
        self.child.terminate(exit_code)
    }

    /// Binds a connected claimant to this exact launch.
    ///
    /// `claimant` is the process that the connected pipe's client process ID
    /// names, opened and pinned by [`WindowsProcessPeer::open`]. In order, the
    /// binding requires that `CompareObjectHandles` reports the claimant handle
    /// and the retained launch handle as one kernel object, that the retained
    /// launch handle is still unsignaled, and that the claimant's process ID and
    /// creation time equal those recorded at launch. The token checks of the
    /// claim's writer are separate and still required.
    ///
    /// # Errors
    ///
    /// Returns the first failed fact as a [`WindowsClaimantRefusal`].
    pub fn bind_claimant(
        &self,
        claimant: &WindowsProcessPeer,
    ) -> Result<(), WindowsClaimantRefusal> {
        bind_observed_claimant(
            ProcessObservation::of_peer(claimant),
            ProcessObservation::of_launch(self),
        )
    }
}

/// Why an attempt owner refused a connect-back claimant.
///
/// Every variant refuses. The owner disconnects that client and re-arms the same
/// endpoint instance; a refusal consumes no one-shot and never extends the health
/// deadline (KEL-53 §4 "Candidate connect-back", *Refusal*).
#[derive(Debug)]
pub enum WindowsClaimantRefusal {
    /// `CompareObjectHandles` reports that the claimant handle and the retained
    /// launch handle are different kernel objects.
    NotLaunchedProcess,
    /// The retained launch handle is signaled: the launched process has exited.
    LaunchExited,
    /// The claimant's process ID or creation time differs from the launch record.
    LaunchIdentityMismatch,
    /// Windows could not compare or query the handles, so nothing was proved.
    Unverifiable {
        /// The comparison or query that failed.
        phase: &'static str,
        /// The Windows error it reported.
        source: io::Error,
    },
}

impl std::fmt::Display for WindowsClaimantRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KELD-RUNTIME-017: connect-back claimant refused: ")?;
        match self {
            Self::NotLaunchedProcess => f.write_str(
                "the connected process is not the launched and retained process object",
            )?,
            Self::LaunchExited => f.write_str("the launched process has exited")?,
            Self::LaunchIdentityMismatch => f.write_str(
                "the connected process ID or creation time differs from the launch record",
            )?,
            Self::Unverifiable { phase, source } => write!(f, "{phase} failed: {source}")?,
        }
        f.write_str(
            ". Disconnect this client and re-arm the endpoint without consuming its one-shot; \
             only the exact launched process can claim, and the health deadline is not extended.",
        )
    }
}

impl std::error::Error for WindowsClaimantRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unverifiable { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Process-object identity compared by the claimant binding. The lifetime keeps
/// the raw handle's owner alive while the observation is in use.
#[derive(Clone, Copy)]
struct ProcessObservation<'a> {
    handle: HANDLE,
    process_id: u32,
    creation_time: u64,
    _owner: std::marker::PhantomData<&'a OwnedHandle>,
}

impl<'a> ProcessObservation<'a> {
    fn of_peer(peer: &'a WindowsProcessPeer) -> Self {
        Self {
            handle: peer.process.as_raw_handle().cast(),
            process_id: peer.process_id,
            creation_time: peer.creation_time,
            _owner: std::marker::PhantomData,
        }
    }

    fn of_launch(launch: &'a WindowsLaunchedProcess) -> Self {
        Self {
            handle: launch.child.process_handle().as_raw_handle().cast(),
            process_id: launch.child.id(),
            creation_time: launch.creation_time,
            _owner: std::marker::PhantomData,
        }
    }
}

/// Requires the three acceptance facts in their specified order.
fn bind_observed_claimant(
    claimant: ProcessObservation<'_>,
    launch: ProcessObservation<'_>,
) -> Result<(), WindowsClaimantRefusal> {
    match same_kernel_object(claimant.handle, launch.handle) {
        Ok(true) => {}
        Ok(false) => return Err(WindowsClaimantRefusal::NotLaunchedProcess),
        Err(source) => {
            return Err(WindowsClaimantRefusal::Unverifiable {
                phase: "claimant process-object comparison",
                source,
            });
        }
    }
    // SAFETY: the launch observation borrows its owner, which retains the launch
    // handle through this zero-time, read-only signal query.
    match unsafe { WaitForSingleObject(launch.handle, 0) } {
        WAIT_TIMEOUT => {}
        WAIT_OBJECT_0 => return Err(WindowsClaimantRefusal::LaunchExited),
        _ => {
            return Err(WindowsClaimantRefusal::Unverifiable {
                phase: "launch handle state query",
                source: io::Error::last_os_error(),
            });
        }
    }
    if claimant.process_id != launch.process_id || claimant.creation_time != launch.creation_time {
        return Err(WindowsClaimantRefusal::LaunchIdentityMismatch);
    }
    Ok(())
}

/// Reports whether two handles name one kernel object.
///
/// `CompareObjectHandles` requires no access right on either handle. Its
/// reference page documents only the return value: `TRUE` for one object,
/// otherwise `FALSE`. Its example names `ERROR_NOT_SAME_OBJECT` only for two
/// objects of different types, an event and a process. That two different
/// process objects leave `ERROR_NOT_SAME_OBJECT`, and that a null, out-of-range
/// or closed handle leaves `ERROR_INVALID_HANDLE`, is observed only on Windows 11
/// build 26300, pinned by
/// `compare_object_handles_classifies_same_different_and_invalid_handles` and
/// `compare_object_handles_refuses_a_closed_handle`. So `FALSE` with
/// `ERROR_NOT_SAME_OBJECT` is the only different-object verdict, and any other
/// `FALSE` is an error. Every non-`TRUE` result refuses the claimant: the verdict
/// as [`WindowsClaimantRefusal::NotLaunchedProcess`], an error as
/// [`WindowsClaimantRefusal::Unverifiable`].
fn same_kernel_object(first: HANDLE, second: HANDLE) -> io::Result<bool> {
    // SAFETY: CompareObjectHandles only resolves the two handle values through
    // this process's handle table and dereferences no caller memory; an unusable
    // value makes it fail without touching any object.
    if unsafe { CompareObjectHandles(first, second) } != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_NOT_SAME_OBJECT.cast_signed()) {
        return Ok(false);
    }
    Err(error)
}

/// Reads one retained process object's creation time in FILETIME units.
fn process_creation_time(process: HANDLE, phase: &'static str) -> Result<u64, WindowsHostJobError> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: the caller retains the process handle and all four FILETIME outputs
    // are writable.
    if unsafe {
        GetProcessTimes(
            process,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new(phase));
    }
    Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

impl WindowsProcessJob {
    /// Creates an unnamed, non-inheritable Job with kill-on-last-handle-close.
    ///
    /// # Errors
    ///
    /// Returns an error if Job creation, flag configuration, or read-back fails.
    pub fn create() -> Result<Self, WindowsHostJobError> {
        Ok(Self {
            handle: create_process_job()?,
            host_assigned: false,
            host_process_id: None,
            host_process: None,
        })
    }

    /// Assigns one live process to this exact Job.
    ///
    /// The owner must assign the host before its application resources can start.
    /// Windows may reject assignment when the process's existing Job hierarchy is
    /// incompatible; callers must fail closed in that case.
    ///
    /// # Errors
    ///
    /// Returns an error if the process cannot be assigned or exact membership cannot
    /// be verified.
    pub fn assign_child(&mut self, process: &Child) -> Result<(), WindowsHostJobError> {
        if self.host_assigned {
            return Err(WindowsHostJobError::contract(
                "attempt Job process assignment",
                "an exact direct host was already assigned to this Job",
            ));
        }
        let raw_job = self.handle.as_raw_handle().cast();
        let raw_process = process.as_raw_handle().cast();
        // SAFETY: `Child` retains its process handle through this synchronous call.
        if unsafe { AssignProcessToJobObject(raw_job, raw_process) } == 0 {
            return Err(WindowsHostJobError::new("attempt Job process assignment"));
        }
        let mut in_job = 0;
        // SAFETY: `Child` and this owner retain both handles, and `in_job` is writable BOOL storage.
        if unsafe { IsProcessInJob(raw_process, raw_job, &raw mut in_job) } == 0 {
            return Err(WindowsHostJobError::new("attempt Job membership read-back"));
        }
        if in_job == 0 {
            return Err(WindowsHostJobError::contract(
                "attempt Job membership read-back",
                "process was not observed in the assigned Job",
            ));
        }
        let mut retained_process = std::ptr::null_mut();
        // SAFETY: both process handles refer to the current process and live
        // Child; `retained_process` is writable HANDLE storage. The duplicate is
        // non-inheritable and has the same synchronization/query rights.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                raw_process,
                GetCurrentProcess(),
                &raw mut retained_process,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "attempt Job direct-host handle retention",
            ));
        }
        // SAFETY: the successful DuplicateHandle call returned one fresh owning
        // process handle, converted exactly once into the retained owner.
        let retained_process = unsafe { OwnedHandle::from_raw_handle(retained_process.cast()) };
        self.host_assigned = true;
        self.host_process_id = Some(process.id());
        self.host_process = Some(retained_process);
        Ok(())
    }

    /// Duplicates a least-rights inheritable handle for the surviving stage-cleanup
    /// helper. The duplicate can query and terminate this exact Job; it cannot
    /// assign processes or change Job limits.
    ///
    /// # Errors
    ///
    /// Returns an error before the helper launches unless an exact host has already
    /// been assigned and the inheritable handle is read back.
    pub fn duplicate_cleanup_observer_handle(&self) -> Result<OwnedHandle, WindowsHostJobError> {
        if !self.host_assigned {
            return Err(WindowsHostJobError::contract(
                "cleanup observer Job handle",
                "cannot delegate a Job before exact host membership is verified",
            ));
        }
        let mut duplicate = std::ptr::null_mut();
        let rights = JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE;
        // SAFETY: the live source Job handle is owned here; output is writable
        // HANDLE storage. The child receives only this explicit inheritable
        // least-rights duplicate through its standard-error handle.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.handle.as_raw_handle().cast(),
                GetCurrentProcess(),
                &raw mut duplicate,
                rights,
                1,
                0,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "cleanup observer Job handle duplication",
            ));
        }
        // SAFETY: successful DuplicateHandle returned one new owning Job handle.
        let duplicate = unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) };
        let raw = duplicate.as_raw_handle().cast();
        let mut flags = 0_u32;
        // SAFETY: the duplicate remains owned and `flags` is writable storage.
        if unsafe { GetHandleInformation(raw, &raw mut flags) } == 0 {
            return Err(WindowsHostJobError::new(
                "cleanup observer Job handle read-back",
            ));
        }
        if flags & HANDLE_FLAG_INHERIT == 0 {
            return Err(WindowsHostJobError::contract(
                "cleanup observer Job handle read-back",
                "least-rights handle is not inheritable for the cleanup helper",
            ));
        }
        Ok(duplicate)
    }

    /// Duplicates a non-inheritable, least-rights handle for an authenticated
    /// per-attempt lifecycle keeper. The handle refers to this exact unnamed Job.
    ///
    /// The returned handle has only `JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE`;
    /// the keeper cannot assign processes or change limits. The caller MUST
    /// transfer this handle only to an authenticated keeper outside the Job.
    ///
    /// # Errors
    ///
    /// Returns an error unless an exact host has already been assigned or the
    /// handle rights/inheritance read-back fails.
    pub fn duplicate_lifecycle_keeper_handle(&self) -> Result<OwnedHandle, WindowsHostJobError> {
        if !self.host_assigned {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper Job handle",
                "cannot delegate a Job before exact host membership is verified",
            ));
        }
        let mut duplicate = std::ptr::null_mut();
        let rights = JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE;
        // SAFETY: the source Job is owned here; the duplicate is non-inheritable
        // and receives only the two lifecycle rights named above.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.handle.as_raw_handle().cast(),
                GetCurrentProcess(),
                &raw mut duplicate,
                rights,
                0,
                0,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "lifecycle keeper Job handle duplication",
            ));
        }
        // SAFETY: successful DuplicateHandle returned one fresh owned handle.
        let duplicate = unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) };
        let raw = duplicate.as_raw_handle().cast();
        let mut flags = 0_u32;
        // SAFETY: the duplicate remains owned and flags is writable storage.
        if unsafe { GetHandleInformation(raw, &raw mut flags) } == 0 {
            return Err(WindowsHostJobError::new(
                "lifecycle keeper Job handle read-back",
            ));
        }
        if flags & HANDLE_FLAG_INHERIT != 0 {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper Job handle read-back",
                "keeper Job handle unexpectedly permits inheritance",
            ));
        }
        Ok(duplicate)
    }

    /// Duplicates this exact unnamed attempt Job into a retained, authenticated
    /// keeper process. The returned integer is valid only in that target process
    /// and MUST be sent only over its authenticated one-shot channel.
    ///
    /// The source Job is duplicated with only QUERY|TERMINATE. The target process
    /// is reopened and compared against its retained PID, session and creation time
    /// to reject PID reuse. This method assumes the caller already authenticated
    /// the connected target's executable and installation role.
    ///
    /// # Errors
    ///
    /// Returns an error unless this Job has an assigned host and target identity,
    /// duplication and exact-target checks all succeed.
    pub fn transfer_lifecycle_handle_to(
        &self,
        target: &WindowsProcessPeer,
    ) -> Result<usize, WindowsHostJobError> {
        if !self.host_assigned {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper Job transfer",
                "cannot transfer a Job before exact host membership is verified",
            ));
        }
        let target_process = open_lifecycle_transfer_target(target, &self.handle)?;
        let raw_target = target_process.as_raw_handle().cast();
        let mut remote = std::ptr::null_mut();
        let rights = JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE;
        // SAFETY: the source is this exact attempt Job; raw_target is revalidated
        // against the authenticated target process and `remote` is writable.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.handle.as_raw_handle().cast(),
                raw_target,
                &raw mut remote,
                rights,
                0,
                0,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "lifecycle keeper Job cross-process duplication",
            ));
        }
        if remote.is_null() || remote == INVALID_HANDLE_VALUE {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper Job cross-process duplication",
                "Windows returned an invalid target-process handle value",
            ));
        }
        Ok(remote as usize)
    }

    /// Duplicates the reduced activation-lock retention handle into the same
    /// authenticated keeper process used for this attempt Job. The duplicated
    /// handle is restricted to read-attributes/synchronize and cannot write.
    ///
    /// # Errors
    ///
    /// Returns an error unless an exact host was assigned, the target is outside
    /// this Job, the source is non-inheritable, and cross-process duplication succeeds.
    pub fn transfer_activation_lease_handle_to(
        &self,
        target: &WindowsProcessPeer,
        lease_retention: &OwnedHandle,
    ) -> Result<usize, WindowsHostJobError> {
        if !self.host_assigned {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper lease transfer",
                "cannot transfer a lease before exact host membership is verified",
            ));
        }
        let source = lease_retention.as_raw_handle().cast();
        let mut source_flags = 0_u32;
        // SAFETY: the supplied reduced lease handle is owned by the caller and
        // the source flags output is writable.
        if unsafe { GetHandleInformation(source, &raw mut source_flags) } == 0 {
            return Err(WindowsHostJobError::new(
                "lifecycle keeper lease inheritance query",
            ));
        }
        if source_flags & HANDLE_FLAG_INHERIT != 0 {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper lease transfer",
                "lease retention source must be non-inheritable",
            ));
        }
        // SAFETY: source is a live caller-owned handle; GetFileType reads its
        // kernel object type without changing the handle or object state.
        if unsafe { GetFileType(source) } != FILE_TYPE_DISK {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper lease transfer",
                "source retention handle is not a disk-file object",
            ));
        }
        let target_process = open_lifecycle_transfer_target(target, &self.handle)?;
        let mut remote = std::ptr::null_mut();
        let rights = windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES
            | windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
        // SAFETY: source is the updater-created reduced lease handle; target was
        // revalidated and lies outside the exact attempt Job. The target copy is
        // attenuated again to the only keeper rights allowed for this file object.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                source,
                target_process.as_raw_handle().cast(),
                &raw mut remote,
                rights,
                0,
                0,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "lifecycle keeper lease cross-process duplication",
            ));
        }
        if remote.is_null() || remote == INVALID_HANDLE_VALUE {
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper lease cross-process duplication",
                "Windows returned an invalid target-process handle value",
            ));
        }
        Ok(remote as usize)
    }

    /// Transfers this exact attempt Job and reduced activation-lease retention as
    /// one fixed-size bundle over a fully accepted one-shot lifecycle connection.
    ///
    /// The peer is consumed by value so this API cannot reuse the accepted
    /// connection. It returns only after the keeper adopts both handles, queries
    /// the exact Job and acknowledges the same attempt/channel. The caller must
    /// retain its local Job and lease owners if this method fails.
    ///
    /// # Errors
    ///
    /// Returns an error unless the connection has the coordinator-to-keeper
    /// purpose, the target lies outside the exact Job, both remote handles are
    /// duplicated with reduced rights, and the keeper returns the exact adoption
    /// acknowledgement.
    pub fn transfer_lifecycle_attempt_handoff(
        &self,
        mut peer: WindowsLifecycleRendezvousPeer<WindowsProcessPeer>,
        lease_retention: &OwnedHandle,
        io_deadline: Instant,
    ) -> Result<(), WindowsHostJobError> {
        let binding = peer.binding();
        if binding.purpose() != WindowsLifecyclePurpose::CoordinatorToKeeper {
            return Err(WindowsHostJobError::contract(
                "lifecycle attempt handoff",
                "only coordinator-to-keeper bindings may receive the attempt bundle",
            ));
        }
        let target = peer.process_pin().try_clone()?;
        peer.set_io_deadline(io_deadline);
        let remote_job = self.transfer_lifecycle_handle_to(&target)?;
        let remote_lease = match self.transfer_activation_lease_handle_to(&target, lease_retention)
        {
            Ok(handle) => handle,
            Err(error) => {
                terminate_lifecycle_transfer_target(&target, &self.handle)?;
                return Err(error);
            }
        };
        let offer = match encode_lifecycle_attempt_handoff(binding, remote_job, remote_lease) {
            Ok(offer) => offer,
            Err(error) => {
                terminate_lifecycle_transfer_target(&target, &self.handle)?;
                return Err(error);
            }
        };
        if let Err(error) = peer.stream_mut().write_all(&offer) {
            // A failed write may have delivered all or part of HO1. The receiver
            // may already have closed and reused either numeric handle slot, so
            // never issue DUPLICATE_CLOSE_SOURCE after delivery begins. Kill the
            // exact dedicated keeper; process teardown closes every remote handle.
            terminate_lifecycle_transfer_target(&target, &self.handle)?;
            return Err(WindowsHostJobError::contract(
                "lifecycle attempt handoff offer",
                error.to_string(),
            ));
        }
        let mut acknowledgement = [0_u8; LIFECYCLE_ATTEMPT_ACK_LEN];
        if let Err(error) = peer.stream_mut().read_exact(&mut acknowledgement) {
            terminate_lifecycle_transfer_target(&target, &self.handle)?;
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper adoption acknowledgement",
                error.to_string(),
            ));
        }
        if acknowledgement[..8] != *b"KELD-HR1"
            || acknowledgement[8..40] != *binding.attempt_id()
            || acknowledgement[40..72] != *binding.lifecycle_channel_id()
            || u32::from_le_bytes(acknowledgement[72..76].try_into().map_err(|_| {
                WindowsHostJobError::contract(
                    "lifecycle keeper adoption acknowledgement",
                    "active-process count field changed shape",
                )
            })?) == 0
        {
            terminate_lifecycle_transfer_target(&target, &self.handle)?;
            return Err(WindowsHostJobError::contract(
                "lifecycle keeper adoption acknowledgement",
                "keeper did not acknowledge the exact live attempt and lease bundle",
            ));
        }
        Ok(())
    }

    /// Adopts the narrowly duplicated inheritable Job handle from a cleanup
    /// helper's standard error slot and validates its fixed limits and host.
    ///
    /// # Errors
    ///
    /// Returns an error unless the inherited object is a Keld attempt Job with
    /// exactly the staged host as a verified member.
    pub fn adopt_cleanup_observer_from_stderr(host_pid: u32) -> Result<Self, WindowsHostJobError> {
        // SAFETY: GetStdHandle retains no pointer; the returned handle is checked
        // below and only used synchronously while the process owns its stdio table.
        let inherited = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        if inherited.is_null() || inherited == INVALID_HANDLE_VALUE {
            return Err(WindowsHostJobError::contract(
                "cleanup observer Job admission",
                "standard error is not a valid inherited Job handle",
            ));
        }
        // SAFETY: this removes inheritance from the helper's exact Job handle so
        // any child it may create cannot receive the cleanup capability.
        if unsafe { SetHandleInformation(inherited, HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(WindowsHostJobError::new(
                "cleanup observer Job inheritance clear",
            ));
        }
        let mut raw_job = std::ptr::null_mut();
        // SAFETY: inherited is live; output is writable HANDLE storage and the
        // duplicate retains the least rights granted by the launcher.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                inherited,
                GetCurrentProcess(),
                &raw mut raw_job,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "cleanup observer Job handle retention",
            ));
        }
        // SAFETY: DuplicateHandle returned one fresh owning Job handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw_job.cast()) };
        let flags = query_job_limit_flags(handle.as_raw_handle().cast())?;
        if flags != JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE {
            return Err(WindowsHostJobError::contract(
                "cleanup observer Job profile",
                format!("unexpected Job limit flags {flags:#010x}"),
            ));
        }
        // SAFETY: the child is held at the start gate; this opens its exact PID
        // with only wait/query rights for the helper's cleanup proof.
        let raw_host = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                host_pid,
            )
        };
        if raw_host.is_null() {
            return Err(WindowsHostJobError::new(
                "cleanup observer direct-host open",
            ));
        }
        // SAFETY: OpenProcess returned one fresh non-null owning process handle.
        let host_process = unsafe { OwnedHandle::from_raw_handle(raw_host.cast()) };
        let mut in_job = 0;
        // SAFETY: both owned handles remain live and `in_job` is writable BOOL storage.
        if unsafe {
            IsProcessInJob(
                host_process.as_raw_handle().cast(),
                handle.as_raw_handle().cast(),
                &raw mut in_job,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "cleanup observer exact host membership query",
            ));
        }
        if in_job == 0 {
            return Err(WindowsHostJobError::contract(
                "cleanup observer exact host membership query",
                "the staged host is not a member of the inherited attempt Job",
            ));
        }
        Ok(Self {
            handle,
            host_assigned: true,
            host_process_id: Some(host_pid),
            host_process: Some(host_process),
        })
    }

    /// Terminates the exact host attempt associated with an adopted cleanup Job
    /// handle and proves the complete process family reached zero.
    ///
    /// # Errors
    ///
    /// Returns an error if the delegated handle is incomplete, termination fails,
    /// or exact host/process-family exit cannot be observed before `timeout`.
    pub fn terminate_and_wait_attached(
        &self,
        timeout: Duration,
    ) -> Result<(), WindowsHostJobError> {
        if !self.host_assigned || self.host_process.is_none() {
            return Err(WindowsHostJobError::contract(
                "cleanup observer attempt termination",
                "no verified direct host is bound to this cleanup Job",
            ));
        }
        self.terminate_and_wait_host_timed(timeout)
    }

    /// Gives the exact host a bounded graceful-exit interval, then proves its
    /// attempt family is empty. A stalled shutdown is terminated and rechecked.
    ///
    /// # Errors
    ///
    /// Returns an error if the host/family remains live or OS accounting cannot
    /// be read after both graceful and forced cleanup.
    pub fn gracefully_wait_and_reap_attached(
        &self,
        grace_timeout: Duration,
        terminate_timeout: Duration,
    ) -> Result<(), WindowsHostJobError> {
        if !self.host_assigned || self.host_process.is_none() {
            return Err(WindowsHostJobError::contract(
                "cleanup observer graceful attempt shutdown",
                "no verified direct host is bound to this cleanup Job",
            ));
        }
        let Some(host) = self
            .host_process
            .as_ref()
            .map(|handle| handle.as_raw_handle().cast())
        else {
            return Err(WindowsHostJobError::contract(
                "cleanup observer graceful attempt shutdown",
                "the admitted host process handle is absent",
            ));
        };
        let grace_deadline = Instant::now().checked_add(grace_timeout).ok_or_else(|| {
            WindowsHostJobError::contract(
                "cleanup observer graceful attempt shutdown",
                "grace deadline overflow",
            )
        })?;
        if wait_handle_until(host, grace_deadline, "graceful direct-host exit").is_err() {
            self.terminate()?;
            let forced_deadline =
                Instant::now()
                    .checked_add(terminate_timeout)
                    .ok_or_else(|| {
                        WindowsHostJobError::contract(
                            "cleanup observer forced attempt shutdown",
                            "termination deadline overflow",
                        )
                    })?;
            wait_handle_until(host, forced_deadline, "forced direct-host exit")?;
        }
        if self.wait_for_active_zero_timed(grace_deadline).is_err() {
            self.terminate()?;
            let forced_deadline =
                Instant::now()
                    .checked_add(terminate_timeout)
                    .ok_or_else(|| {
                        WindowsHostJobError::contract(
                            "cleanup observer forced attempt shutdown",
                            "termination deadline overflow",
                        )
                    })?;
            wait_handle_until(host, forced_deadline, "forced direct-host exit")?;
            self.wait_for_active_zero_timed(forced_deadline)?;
        }
        let active = self.active_processes()?;
        if active != 0 {
            return Err(WindowsHostJobError::contract(
                "cleanup observer attempt family exit",
                format!("Job still contains {active} active process(es)"),
            ));
        }
        Ok(())
    }

    /// Waits on the cleanup helper's private stdin pipe for a release, launcher
    /// death, or malformed record using raw Win32 reads without Rust buffering.
    ///
    /// # Errors
    ///
    /// Returns an error if stdin is not a pipe or an unrelated OS read failure occurs.
    pub fn await_cleanup_release_v1() -> Result<WindowsCleanupReleaseSignal, WindowsHostJobError> {
        // SAFETY: GetStdHandle retains no pointer; the returned handle is checked
        // below and consumed only through synchronous pipe reads.
        let pipe = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        if pipe.is_null() || pipe == INVALID_HANDLE_VALUE {
            return Err(WindowsHostJobError::contract(
                "cleanup release pipe admission",
                "standard input is not a valid cleanup control pipe",
            ));
        }
        // SAFETY: `pipe` is valid through this synchronous type query.
        if unsafe { GetFileType(pipe) } != FILE_TYPE_PIPE {
            return Err(WindowsHostJobError::contract(
                "cleanup release pipe admission",
                "standard input is not a pipe",
            ));
        }
        let mut token = [0_u8; WINDOWS_DEV_STAGE_CLEANUP_RELEASE_V1.len()];
        let mut offset = 0_usize;
        while offset < token.len() {
            let mut received = 0_u32;
            let remaining = u32::try_from(token.len() - offset).map_err(|_| {
                WindowsHostJobError::contract(
                    "cleanup release read",
                    "token remainder exceeds the Win32 read size",
                )
            })?;
            // SAFETY: the pipe is live, the destination covers `remaining` bytes,
            // and the synchronous read has no OVERLAPPED state.
            if unsafe {
                ReadFile(
                    pipe,
                    token[offset..].as_mut_ptr(),
                    remaining,
                    &raw mut received,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                let source = io::Error::last_os_error();
                if source.kind() == io::ErrorKind::BrokenPipe {
                    return Ok(if offset == 0 {
                        WindowsCleanupReleaseSignal::LauncherLost
                    } else {
                        WindowsCleanupReleaseSignal::Malformed
                    });
                }
                return Err(WindowsHostJobError {
                    phase: "cleanup release token read",
                    source,
                });
            }
            if received == 0 {
                return Ok(if offset == 0 {
                    WindowsCleanupReleaseSignal::LauncherLost
                } else {
                    WindowsCleanupReleaseSignal::Malformed
                });
            }
            offset += received as usize;
        }
        if &token != WINDOWS_DEV_STAGE_CLEANUP_RELEASE_V1 {
            return Ok(WindowsCleanupReleaseSignal::Malformed);
        }
        let mut extra = [0_u8; 1];
        let mut received = 0_u32;
        // SAFETY: the pipe is live; the one-byte destination is writable and the
        // synchronous read waits for EOF after the fixed record.
        if unsafe {
            ReadFile(
                pipe,
                extra.as_mut_ptr(),
                1,
                &raw mut received,
                std::ptr::null_mut(),
            )
        } == 0
        {
            let source = io::Error::last_os_error();
            if source.kind() == io::ErrorKind::BrokenPipe {
                return Ok(WindowsCleanupReleaseSignal::Released);
            }
            return Err(WindowsHostJobError {
                phase: "cleanup release EOF read",
                source,
            });
        }
        Ok(if received == 0 {
            WindowsCleanupReleaseSignal::Released
        } else {
            WindowsCleanupReleaseSignal::Malformed
        })
    }

    /// Reports whether one process belongs to this exact Job.
    ///
    /// # Errors
    ///
    /// Returns an error if the OS cannot observe membership.
    pub fn contains_child(&self, process: &Child) -> Result<bool, WindowsHostJobError> {
        let mut in_job = 0;
        // SAFETY: `Child` and this owner retain both handles, and `in_job` is writable BOOL storage.
        if unsafe {
            IsProcessInJob(
                process.as_raw_handle().cast(),
                self.handle.as_raw_handle().cast(),
                &raw mut in_job,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new("attempt Job membership query"));
        }
        Ok(in_job != 0)
    }

    /// Returns the OS-reported number of live processes in this Job hierarchy.
    ///
    /// # Errors
    ///
    /// Returns an error if Job accounting cannot be queried.
    pub fn active_processes(&self) -> Result<u32, WindowsHostJobError> {
        query_job_active_processes(self.handle.as_raw_handle().cast())
    }

    fn terminate(&self) -> Result<(), WindowsHostJobError> {
        // SAFETY: the owning Job handle is live through the call; Keld's launcher
        // or host owner is the sole authority responsible for this process family.
        if unsafe { TerminateJobObject(self.handle.as_raw_handle().cast(), 1) } == 0 {
            return Err(WindowsHostJobError::new("attempt Job termination"));
        }
        Ok(())
    }

    /// Terminates the attempt Job and proves both its direct host and all Job members
    /// have exited before a replacement may launch.
    ///
    /// The Job handle stays open throughout termination and observation. Waiting on
    /// the host alone is insufficient because descendants may outlive it; the final
    /// OS accounting read must report zero active processes.
    ///
    /// # Errors
    ///
    /// Returns an error if termination is denied, either wait times out, a wait fails,
    /// or Job accounting remains nonzero.
    pub fn terminate_and_wait(
        self,
        direct_host: &Child,
        timeout: Duration,
    ) -> Result<(), WindowsHostJobError> {
        if !self.host_assigned {
            return Err(WindowsHostJobError::contract(
                "attempt Job termination",
                "no host was admitted to this attempt Job",
            ));
        }
        if self.host_process_id != Some(direct_host.id()) || self.host_process.is_none() {
            return Err(WindowsHostJobError::contract(
                "attempt Job termination",
                "the supplied direct host is not the exact process admitted to this Job",
            ));
        }
        let Some(retained_host) = self
            .host_process
            .as_ref()
            .map(|handle| handle.as_raw_handle().cast())
        else {
            return Err(WindowsHostJobError::contract(
                "attempt Job termination",
                "the admitted host process handle is absent",
            ));
        };
        // SAFETY: the Job owner retains the exact admitted host process handle.
        match unsafe { WaitForSingleObject(retained_host, 0) } {
            WAIT_OBJECT_0 => {
                // The launcher may reach retirement after a natural host exit.
                // `host_assigned` records successful exact membership read-back
                // while the process was live; Job accounting remains authoritative
                // for any surviving descendants.
            }
            WAIT_TIMEOUT => {
                if !self.contains_child(direct_host)? {
                    return Err(WindowsHostJobError::contract(
                        "attempt Job termination",
                        "live direct host is not a member of this exact attempt Job",
                    ));
                }
            }
            _ => {
                return Err(WindowsHostJobError::new(
                    "attempt Job direct-host state query",
                ));
            }
        }
        self.terminate_and_wait_host(timeout)
    }

    fn terminate_and_wait_host(self, timeout: Duration) -> Result<(), WindowsHostJobError> {
        let Some(retained_host) = self
            .host_process
            .as_ref()
            .map(|handle| handle.as_raw_handle().cast())
        else {
            return Err(WindowsHostJobError::contract(
                "attempt Job termination",
                "the admitted host process handle is absent",
            ));
        };
        self.terminate()?;
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            WindowsHostJobError::contract("attempt Job wait", "timeout deadline overflow")
        })?;
        // SAFETY: this owner retains the exact admitted host process handle.
        if unsafe { WaitForSingleObject(retained_host, 0) } != WAIT_OBJECT_0 {
            wait_handle_until(retained_host, deadline, "direct host exit")?;
        }
        let completion_port = create_job_completion_port(self.handle.as_raw_handle().cast())?;
        self.wait_for_active_zero(&completion_port, deadline)?;
        let active = self.active_processes()?;
        if active != 0 {
            return Err(WindowsHostJobError::contract(
                "attempt Job family exit",
                format!("Job still contains {active} active process(es)"),
            ));
        }
        Ok(())
    }

    fn terminate_and_wait_host_timed(&self, timeout: Duration) -> Result<(), WindowsHostJobError> {
        let Some(host) = self
            .host_process
            .as_ref()
            .map(|handle| handle.as_raw_handle().cast())
        else {
            return Err(WindowsHostJobError::contract(
                "cleanup observer attempt termination",
                "the admitted host process handle is absent",
            ));
        };
        self.terminate()?;
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            WindowsHostJobError::contract(
                "cleanup observer attempt termination",
                "termination deadline overflow",
            )
        })?;
        wait_handle_until(host, deadline, "direct host exit")?;
        self.wait_for_active_zero_timed(deadline)?;
        let active = self.active_processes()?;
        if active != 0 {
            return Err(WindowsHostJobError::contract(
                "cleanup observer attempt family exit",
                format!("Job still contains {active} active process(es)"),
            ));
        }
        Ok(())
    }

    fn wait_for_active_zero(
        &self,
        completion_port: &OwnedHandle,
        deadline: Instant,
    ) -> Result<(), WindowsHostJobError> {
        loop {
            if self.active_processes()? == 0 {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(WindowsHostJobError::contract(
                    "attempt Job family exit",
                    "timed out while waiting for the active-process-zero notification",
                ));
            }
            let millis = u32::try_from(remaining.as_millis().clamp(1, 100)).unwrap_or(u32::MAX - 1);
            let mut message = 0_u32;
            let mut completion_key = 0_usize;
            let mut overlapped = std::ptr::null_mut();
            // SAFETY: the completion port stays owned by this Job owner, and all
            // three outputs are writable for the synchronous dequeue operation.
            let dequeued = unsafe {
                GetQueuedCompletionStatus(
                    completion_port.as_raw_handle().cast(),
                    &raw mut message,
                    &raw mut completion_key,
                    &raw mut overlapped,
                    millis,
                )
            };
            if dequeued == 0 {
                let source = io::Error::last_os_error();
                if source.raw_os_error() == Some(WAIT_TIMEOUT.cast_signed()) {
                    if self.active_processes()? == 0 {
                        return Ok(());
                    }
                    if Instant::now() < deadline {
                        continue;
                    }
                    return Err(WindowsHostJobError::contract(
                        "attempt Job family exit",
                        "timed out while waiting for an attempt Job completion message",
                    ));
                }
                return Err(WindowsHostJobError {
                    phase: "attempt Job completion-port dequeue",
                    source,
                });
            }
            if completion_key != 0 {
                return Err(WindowsHostJobError::contract(
                    "attempt Job completion-port dequeue",
                    "completion key does not belong to this Job owner",
                ));
            }
            let _ = message;
            // Every Job completion is only a wake-up; the next loop iteration
            // re-queries authoritative ActiveProcesses accounting.
        }
    }

    fn wait_for_active_zero_timed(&self, deadline: Instant) -> Result<(), WindowsHostJobError> {
        // SAFETY: null attributes/name request one unnamed waitable timer with
        // default security; its returned handle is validated before ownership.
        let raw_timer = unsafe { CreateWaitableTimerW(std::ptr::null(), 0, std::ptr::null()) };
        if raw_timer.is_null() || raw_timer == INVALID_HANDLE_VALUE {
            return Err(WindowsHostJobError::new(
                "cleanup observer active-process timer create",
            ));
        }
        // SAFETY: `raw_timer` is the fresh waitable timer returned above.
        let timer = unsafe { OwnedHandle::from_raw_handle(raw_timer.cast()) };
        let due_time = -250_000_i64;
        // SAFETY: timer is live, due_time is relative, and no APC callback or
        // context is registered.
        if unsafe {
            SetWaitableTimer(
                raw_timer,
                &raw const due_time,
                25,
                None,
                std::ptr::null(),
                0,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "cleanup observer active-process timer arm",
            ));
        }
        loop {
            if self.active_processes()? == 0 {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(WindowsHostJobError::contract(
                    "cleanup observer attempt family exit",
                    "timed out while waiting for Job accounting to reach zero",
                ));
            }
            let timeout = finite_wait_millis(remaining).max(1);
            // SAFETY: timer remains owned for the duration of this bounded wait.
            match unsafe { WaitForSingleObject(timer.as_raw_handle().cast(), timeout) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT if Instant::now() < deadline => {}
                WAIT_TIMEOUT => {
                    return Err(WindowsHostJobError::contract(
                        "cleanup observer attempt family exit",
                        "timed out while waiting for Job accounting to reach zero",
                    ));
                }
                _ => {
                    return Err(WindowsHostJobError::new(
                        "cleanup observer active-process timer wait",
                    ));
                }
            }
        }
    }
}

/// Releases a Windows no-flag host after its launcher has assigned and verified
/// the attempt Job.
///
/// The writer remains open after this one fixed token so the existing dev lease
/// can continue to observe launcher loss. The token is a startup ordering gate,
/// not authentication or filesystem/Job authority.
///
/// # Errors
///
/// Returns an error if the complete token cannot be written and flushed.
pub fn release_host_start_v1(writer: &mut impl io::Write) -> Result<(), WindowsHostJobError> {
    writer
        .write_all(HOST_START_TOKEN_V1)
        .and_then(|()| writer.flush())
        .map_err(|source| WindowsHostJobError {
            phase: "launcher start-token write",
            source,
        })
}

/// Waits on stdin only when the caller selected the attempt-Job startup gate.
///
/// This selector is an ordering hint, never authority: protected updater writes
/// remain in KEL-53, and the launcher must independently assign/read back the
/// host's exact Job membership before releasing the token.
#[must_use]
pub fn launcher_start_gate_requested() -> bool {
    std::env::var_os(WINDOWS_LAUNCH_GATE_ENV).as_deref()
        == Some(std::ffi::OsStr::new(WINDOWS_LAUNCH_GATE_ATTEMPT_JOB_V1))
}

/// Waits for and consumes the launcher’s fixed Windows host-start token before
/// any app listener, child, window, or `WebView` is created.
///
/// An absent/non-pipe stdin, EOF, malformed/partial token, queued extra byte,
/// or inherited reader handle fails closed. The launcher MUST call
/// [`WindowsProcessJob::assign_child`] and verify membership before it writes
/// the token. This gate proves ordering only; the launcher’s OS Job owns process
/// family containment.
///
/// # Errors
///
/// Returns an error if stdin is not a live private pipe or the exact token and
/// handle-isolation checks fail.
pub fn accept_host_start_v1() -> Result<(), WindowsHostJobError> {
    // Read the raw standard handle directly. `std::io::Stdin` can prefetch bytes
    // beyond the token into its private buffer, making a later PeekNamedPipe
    // suffix check observe a false empty queue.
    // SAFETY: GetStdHandle retains no pointer and returns this process's current
    // standard-input handle, which is checked for null/invalid and pipe type.
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(WindowsHostJobError::contract(
            "launcher start pipe admission",
            "standard input is not a valid private pipe handle",
        ));
    }
    // SAFETY: `handle` remains borrowed from `input`; GetFileType retains no
    // pointer and distinguishes pipe startup from console/file direct launch.
    if unsafe { GetFileType(handle) } != FILE_TYPE_PIPE {
        return Err(WindowsHostJobError::contract(
            "launcher start pipe admission",
            "no-flag host was not launched with the private pipe gate",
        ));
    }
    let mut available = 0_u32;
    let mut token = [0_u8; HOST_START_TOKEN_V1.len()];
    let mut offset = 0_usize;
    while offset < token.len() {
        let mut received = 0_u32;
        let remaining = u32::try_from(token.len() - offset).map_err(|_| {
            WindowsHostJobError::contract(
                "launcher start-token read",
                "token remainder exceeds the Win32 read size",
            )
        })?;
        // SAFETY: the pipe handle is live, the token slice is writable for the
        // exact requested remainder, and this synchronous pipe read has no
        // OVERLAPPED state.
        if unsafe {
            ReadFile(
                handle,
                token[offset..].as_mut_ptr(),
                remaining,
                &raw mut received,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(WindowsHostJobError::new("launcher start-token read"));
        }
        if received == 0 {
            return Err(WindowsHostJobError::contract(
                "launcher start-token read",
                "launcher closed the pipe before the complete startup token",
            ));
        }
        offset += received as usize;
    }
    if &token != HOST_START_TOKEN_V1 {
        return Err(WindowsHostJobError::contract(
            "launcher start-token validation",
            "startup token is not the exact v1 value",
        ));
    }
    // Reject any payload after the single start record. Launcher liveness is
    // represented only by pipe closure, never by caller-controlled messages.
    // SAFETY: same live pipe handle and writable availability slot as above.
    if unsafe {
        PeekNamedPipe(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &raw mut available,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(WindowsHostJobError::new("launcher start pipe read-back"));
    }
    if available != 0 {
        return Err(WindowsHostJobError::contract(
            "launcher start pipe read-back",
            "unexpected bytes remain after the fixed startup token",
        ));
    }
    // The pipe reader must not be inherited by any app child.
    // SAFETY: `handle` is the live standard-input pipe handle; this changes only
    // its inheritance flag and retains no pointer.
    if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(WindowsHostJobError::new("launcher pipe inheritance clear"));
    }
    let mut flags = 0_u32;
    // SAFETY: the standard-input handle remains live and `flags` is writable.
    if unsafe { GetHandleInformation(handle, &raw mut flags) } == 0 {
        return Err(WindowsHostJobError::new(
            "launcher pipe inheritance read-back",
        ));
    }
    if flags & HANDLE_FLAG_INHERIT != 0 {
        return Err(WindowsHostJobError::contract(
            "launcher pipe inheritance read-back",
            "standard-input lease handle remains inheritable",
        ));
    }
    Ok(())
}

fn wait_handle_until(
    handle: windows_sys::Win32::Foundation::HANDLE,
    deadline: Instant,
    phase: &'static str,
) -> Result<(), WindowsHostJobError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let millis = finite_wait_millis(remaining);
    // SAFETY: the caller retains the live process or Job handle through this wait.
    match unsafe { WaitForSingleObject(handle, millis) } {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err(WindowsHostJobError::contract(
            phase,
            "timed out while waiting for process-family termination",
        )),
        _ => Err(WindowsHostJobError::new(phase)),
    }
}

fn finite_wait_millis(timeout: Duration) -> u32 {
    u32::try_from(timeout.as_millis())
        .unwrap_or(u32::MAX - 1)
        .min(u32::MAX - 1)
}

fn create_process_job() -> Result<OwnedHandle, WindowsHostJobError> {
    // SAFETY: null name and security attributes request one fresh unnamed Job;
    // the returned handle is validated and converted to exactly one owner below.
    let raw_job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if raw_job.is_null() {
        return Err(WindowsHostJobError::new("CreateJobObjectW"));
    }
    // SAFETY: this is the new non-null handle returned above, transferred once.
    let job = unsafe { OwnedHandle::from_raw_handle(raw_job.cast()) };
    // SAFETY: the owning Job handle stays live and the fixed flags disable breakaway.
    if unsafe { SetHandleInformation(raw_job, HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(WindowsHostJobError::new("Job handle inheritance clear"));
    }
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let bytes = u32::try_from(std::mem::size_of_val(&limits)).map_err(|_| {
        WindowsHostJobError::contract("Job limit structure size", "structure exceeds u32")
    })?;
    // SAFETY: `limits` is live initialized storage of exactly `bytes`; the API
    // copies it synchronously and retains no pointer.
    if unsafe {
        SetInformationJobObject(
            raw_job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            bytes,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new("Job limit configuration"));
    }
    let forbidden = JOB_OBJECT_LIMIT_BREAKAWAY_OK | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
    let observed_flags = query_job_limit_flags(raw_job)?;
    if observed_flags != JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE || observed_flags & forbidden != 0 {
        return Err(WindowsHostJobError::contract(
            "Job limit read-back",
            "Job does not have exactly KILL_ON_JOB_CLOSE with no breakaway",
        ));
    }
    let mut handle_flags = 0_u32;
    // SAFETY: the handle is live and `handle_flags` is writable.
    if unsafe { GetHandleInformation(raw_job, &raw mut handle_flags) } == 0 {
        return Err(WindowsHostJobError::new("Job handle inheritance read-back"));
    }
    if handle_flags & HANDLE_FLAG_INHERIT != 0 {
        return Err(WindowsHostJobError::contract(
            "Job handle inheritance read-back",
            "Job handle remains inheritable",
        ));
    }
    Ok(job)
}

fn query_job_limit_flags(raw_job: HANDLE) -> Result<u32, WindowsHostJobError> {
    let mut observed = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    let bytes = u32::try_from(std::mem::size_of_val(&observed)).map_err(|_| {
        WindowsHostJobError::contract("Job limit structure size", "structure exceeds u32")
    })?;
    // SAFETY: the Job handle is live and `observed` is writable for exactly the
    // advertised structure size; no output pointer escapes.
    if unsafe {
        QueryInformationJobObject(
            raw_job,
            JobObjectExtendedLimitInformation,
            (&raw mut observed).cast(),
            bytes,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(WindowsHostJobError::new("Job limit read-back"));
    }
    Ok(observed.BasicLimitInformation.LimitFlags)
}

fn create_job_completion_port(raw_job: HANDLE) -> Result<OwnedHandle, WindowsHostJobError> {
    // The Job starts empty, so associating before any assignment avoids the
    // notification-loss window documented for active Jobs.
    // SAFETY: INVALID_HANDLE_VALUE requests a new completion port. The returned
    // handle is checked and transferred to one owner below.
    let raw_port =
        unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 1) };
    if raw_port.is_null() || raw_port == INVALID_HANDLE_VALUE {
        return Err(WindowsHostJobError::new(
            "attempt Job completion-port creation",
        ));
    }
    // SAFETY: `raw_port` is the fresh non-null completion-port handle returned above.
    let completion_port = unsafe { OwnedHandle::from_raw_handle(raw_port.cast()) };
    // SAFETY: the completion port is live and owned locally; the flag buffer is writable.
    if unsafe { SetHandleInformation(raw_port, HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(WindowsHostJobError::new(
            "attempt Job completion-port inheritance clear",
        ));
    }
    let mut completion_flags = 0_u32;
    // SAFETY: the completion port remains live and `completion_flags` is writable.
    if unsafe { GetHandleInformation(raw_port, &raw mut completion_flags) } == 0 {
        return Err(WindowsHostJobError::new(
            "attempt Job completion-port inheritance read-back",
        ));
    }
    if completion_flags & HANDLE_FLAG_INHERIT != 0 {
        return Err(WindowsHostJobError::contract(
            "attempt Job completion-port inheritance read-back",
            "completion-port handle remains inheritable",
        ));
    }
    let association = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
        CompletionKey: std::ptr::null_mut(),
        CompletionPort: raw_port,
    };
    let association_size = u32::try_from(std::mem::size_of_val(&association)).map_err(|_| {
        WindowsHostJobError::contract(
            "attempt Job completion-port association",
            "association structure exceeds u32",
        )
    })?;
    // SAFETY: the Job and port handles remain owned, and the association record
    // stays live until this synchronous call returns.
    if unsafe {
        SetInformationJobObject(
            raw_job,
            JobObjectAssociateCompletionPortInformation,
            (&raw const association).cast(),
            association_size,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new(
            "attempt Job completion-port association",
        ));
    }
    Ok(completion_port)
}

fn query_job_active_processes(raw_job: HANDLE) -> Result<u32, WindowsHostJobError> {
    let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    let bytes = u32::try_from(std::mem::size_of_val(&accounting)).map_err(|_| {
        WindowsHostJobError::contract(
            "attempt Job accounting size",
            "accounting structure exceeds u32",
        )
    })?;
    // SAFETY: `raw_job` is a live caller-owned Job handle and `accounting` is
    // writable for exactly the advertised structure size.
    if unsafe {
        QueryInformationJobObject(
            raw_job,
            JobObjectBasicAccountingInformation,
            (&raw mut accounting).cast(),
            bytes,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(WindowsHostJobError::new("attempt Job accounting query"));
    }
    Ok(accounting.ActiveProcesses)
}

fn open_lifecycle_transfer_target(
    target: &WindowsProcessPeer,
    attempt_job: &OwnedHandle,
) -> Result<OwnedHandle, WindowsHostJobError> {
    if target.has_exited()? {
        return Err(WindowsHostJobError::contract(
            "lifecycle keeper target process state",
            "authenticated target process has already exited",
        ));
    }
    // SAFETY: the PID came from an OS-connected peer observation; only
    // PROCESS_DUP_HANDLE plus identity/wait rights are requested.
    let raw_target = unsafe {
        OpenProcess(
            PROCESS_DUP_HANDLE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            target.process_id,
        )
    };
    if raw_target.is_null() {
        return Err(WindowsHostJobError::new(
            "lifecycle keeper target process open",
        ));
    }
    // SAFETY: OpenProcess returned one fresh owning process handle.
    let target_process = unsafe { OwnedHandle::from_raw_handle(raw_target.cast()) };
    // SAFETY: raw_target is the live process object opened above.
    if unsafe { GetProcessId(raw_target) } != target.process_id {
        return Err(WindowsHostJobError::contract(
            "lifecycle keeper target process identity",
            "reopened target PID differs from the authenticated peer",
        ));
    }
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: the target process remains owned and each FILETIME is writable.
    if unsafe {
        GetProcessTimes(
            raw_target,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new(
            "lifecycle keeper target creation-time query",
        ));
    }
    let creation_time =
        (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    if creation_time != target.creation_time
        || process_token_facts(raw_target)? != target.token_facts
    {
        return Err(WindowsHostJobError::contract(
            "lifecycle keeper target process identity",
            "target PID was reused or its token profile differs from the authenticated peer",
        ));
    }
    let mut target_in_attempt_job = 0;
    // SAFETY: both live handles are retained and the membership output is writable.
    if unsafe {
        IsProcessInJob(
            raw_target,
            attempt_job.as_raw_handle().cast(),
            &raw mut target_in_attempt_job,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new(
            "lifecycle keeper target Job membership query",
        ));
    }
    if target_in_attempt_job != 0 {
        return Err(WindowsHostJobError::contract(
            "lifecycle keeper target Job membership",
            "lifecycle keeper target is inside the attempt Job and would die with its members",
        ));
    }
    // SAFETY: `target_process` retains this verified process handle throughout
    // the zero-time signal query; no process state is changed.
    match unsafe { WaitForSingleObject(raw_target, 0) } {
        WAIT_TIMEOUT => Ok(target_process),
        WAIT_OBJECT_0 => Err(WindowsHostJobError::contract(
            "lifecycle keeper target process state",
            "target exited during handle-transfer admission",
        )),
        _ => Err(WindowsHostJobError::new(
            "lifecycle keeper target process state",
        )),
    }
}

fn encode_lifecycle_attempt_handoff(
    binding: WindowsLifecycleBinding,
    remote_job: usize,
    remote_lease: usize,
) -> Result<[u8; LIFECYCLE_ATTEMPT_HANDOFF_LEN], WindowsHostJobError> {
    let remote_job = u64::try_from(remote_job).map_err(|_| {
        WindowsHostJobError::contract(
            "lifecycle attempt handoff record",
            "remote Job handle exceeds the wire integer width",
        )
    })?;
    let remote_lease = u64::try_from(remote_lease).map_err(|_| {
        WindowsHostJobError::contract(
            "lifecycle attempt handoff record",
            "remote lease handle exceeds the wire integer width",
        )
    })?;
    let mut record = [0_u8; LIFECYCLE_ATTEMPT_HANDOFF_LEN];
    record[..8].copy_from_slice(b"KELD-HO1");
    record[8] = WindowsLifecyclePurpose::CoordinatorToKeeper as u8;
    record[9..41].copy_from_slice(binding.attempt_id());
    record[41..73].copy_from_slice(binding.lifecycle_channel_id());
    record[73..81].copy_from_slice(&remote_job.to_le_bytes());
    record[81..89].copy_from_slice(&remote_lease.to_le_bytes());
    Ok(record)
}

fn terminate_incomplete_lifecycle_keeper(reason: &str) -> ! {
    eprintln!(
        "KELD-RUNTIME-014: malformed or incomplete one-shot keeper handoff ({reason}); terminating so unknown remote handles close"
    );
    std::process::abort();
}

fn encode_lifecycle_query_handoff(
    binding: WindowsLifecycleBinding,
    remote_query: usize,
) -> Result<[u8; LIFECYCLE_QUERY_HANDOFF_LEN], WindowsHostJobError> {
    let remote_query = u64::try_from(remote_query).map_err(|_| {
        WindowsHostJobError::contract(
            "lifecycle successor query offer",
            "remote query handle exceeds the wire integer width",
        )
    })?;
    let mut record = [0_u8; LIFECYCLE_QUERY_HANDOFF_LEN];
    record[..8].copy_from_slice(b"KELD-QO1");
    record[8] = WindowsLifecyclePurpose::KeeperToSuccessor as u8;
    record[9..41].copy_from_slice(binding.attempt_id());
    record[41..73].copy_from_slice(binding.lifecycle_channel_id());
    record[73..81].copy_from_slice(&remote_query.to_le_bytes());
    Ok(record)
}

fn encode_lifecycle_query_receipt(
    magic: [u8; 8],
    binding: WindowsLifecycleBinding,
    active_processes: u32,
) -> [u8; LIFECYCLE_ATTEMPT_ACK_LEN] {
    let mut receipt = [0_u8; LIFECYCLE_ATTEMPT_ACK_LEN];
    receipt[..8].copy_from_slice(&magic);
    receipt[8..40].copy_from_slice(binding.attempt_id());
    receipt[40..72].copy_from_slice(binding.lifecycle_channel_id());
    receipt[72..76].copy_from_slice(&active_processes.to_le_bytes());
    receipt
}

fn terminate_lifecycle_transfer_target(
    target: &WindowsProcessPeer,
    attempt_job: &OwnedHandle,
) -> Result<(), WindowsHostJobError> {
    if target.has_exited()? {
        return Ok(());
    }
    // SAFETY: the target is the exact authenticated one-shot keeper pinned by
    // `WindowsProcessPeer`; query/terminate/wait rights are confined to it.
    let raw_target = unsafe {
        OpenProcess(
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            target.process_id,
        )
    };
    if raw_target.is_null() {
        return Err(WindowsHostJobError::new(
            "lifecycle one-shot keeper termination open",
        ));
    }
    // SAFETY: OpenProcess returned one fresh owning process handle.
    let _process = unsafe { OwnedHandle::from_raw_handle(raw_target.cast()) };
    // SAFETY: `_process` retains the exact live process handle returned by
    // `OpenProcess`; `GetProcessId` reads its identity without changing state.
    if unsafe { GetProcessId(raw_target) } != target.process_id {
        return Err(WindowsHostJobError::contract(
            "lifecycle one-shot keeper identity",
            "reopened keeper PID differs from the authenticated peer pin",
        ));
    }
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: process remains owned and each FILETIME is writable.
    if unsafe {
        GetProcessTimes(
            raw_target,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new(
            "lifecycle one-shot keeper creation-time query",
        ));
    }
    let creation_time =
        (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    if creation_time != target.creation_time
        || process_token_facts(raw_target)? != target.token_facts
    {
        return Err(WindowsHostJobError::contract(
            "lifecycle one-shot keeper identity",
            "keeper process object differs from the authenticated peer pin",
        ));
    }
    let mut in_attempt_job = 0;
    // SAFETY: both retained handles are live and the BOOL output is writable.
    if unsafe {
        IsProcessInJob(
            raw_target,
            attempt_job.as_raw_handle().cast(),
            &raw mut in_attempt_job,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new(
            "lifecycle one-shot keeper Job-membership query",
        ));
    }
    if in_attempt_job != 0 {
        return Err(WindowsHostJobError::contract(
            "lifecycle one-shot keeper termination",
            "refusing to terminate a target inside the attempt Job",
        ));
    }
    // SAFETY: on an ambiguous handle-transfer failure, terminating only this
    // authenticated disposable keeper closes any remote handle slots safely.
    if unsafe { TerminateProcess(raw_target, 74) } == 0
        // SAFETY: `target_process` retains the exact verified keeper handle;
        // this zero-time wait only distinguishes termination from failure.
        && (unsafe { WaitForSingleObject(raw_target, 0) } != WAIT_OBJECT_0)
    {
        return Err(WindowsHostJobError::new(
            "lifecycle one-shot keeper termination",
        ));
    }
    // SAFETY: `target_process` keeps the exact verified keeper handle alive;
    // the finite wait observes process exit after the termination request.
    match unsafe { WaitForSingleObject(raw_target, 5_000) } {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err(WindowsHostJobError::contract(
            "lifecycle one-shot keeper termination",
            "keeper did not exit within the bounded handle cleanup wait",
        )),
        _ => Err(WindowsHostJobError::new(
            "lifecycle one-shot keeper termination wait",
        )),
    }
}

fn attenuate_keeper_activation_lease(
    transferred: OwnedHandle,
) -> Result<OwnedHandle, WindowsHostJobError> {
    let source = transferred.as_raw_handle().cast();
    let mut flags = 0_u32;
    // SAFETY: transferred is a locally owned live handle and flags is writable.
    if unsafe { GetHandleInformation(source, &raw mut flags) } == 0 {
        return Err(WindowsHostJobError::new(
            "lifecycle keeper lease inheritance read-back",
        ));
    }
    // SAFETY: `transferred` owns this live handle through the read-only type query.
    let source_type = unsafe { GetFileType(source) };
    if flags & HANDLE_FLAG_INHERIT != 0 || source_type != FILE_TYPE_DISK {
        return Err(WindowsHostJobError::contract(
            "lifecycle keeper lease handle",
            "received activation lease must be non-inheritable and a disk-file object",
        ));
    }
    let mut reduced = std::ptr::null_mut();
    let rights = windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES
        | windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
    // SAFETY: the received value is a live file object; duplicate only the two
    // rights needed to preserve and observe the writer lease.
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            source,
            GetCurrentProcess(),
            &raw mut reduced,
            rights,
            0,
            0,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new(
            "lifecycle keeper lease rights attenuation",
        ));
    }
    // SAFETY: DuplicateHandle returned one fresh owning lease handle.
    let reduced = unsafe { OwnedHandle::from_raw_handle(reduced.cast()) };
    drop(transferred);
    let mut reduced_flags = 0_u32;
    // SAFETY: reduced remains locally owned and flags are writable.
    if unsafe { GetHandleInformation(reduced.as_raw_handle().cast(), &raw mut reduced_flags) } == 0
    {
        return Err(WindowsHostJobError::new(
            "lifecycle keeper lease attenuation read-back",
        ));
    }
    if reduced_flags & HANDLE_FLAG_INHERIT != 0 {
        return Err(WindowsHostJobError::contract(
            "lifecycle keeper lease attenuation read-back",
            "reduced keeper lease handle is inheritable",
        ));
    }
    Ok(reduced)
}

fn encode_lifecycle_attempt_ack(
    binding: WindowsLifecycleBinding,
    active_processes: u32,
) -> [u8; LIFECYCLE_ATTEMPT_ACK_LEN] {
    let mut acknowledgement = [0_u8; LIFECYCLE_ATTEMPT_ACK_LEN];
    acknowledgement[..8].copy_from_slice(b"KELD-HR1");
    acknowledgement[8..40].copy_from_slice(binding.attempt_id());
    acknowledgement[40..72].copy_from_slice(binding.lifecycle_channel_id());
    acknowledgement[72..76].copy_from_slice(&active_processes.to_le_bytes());
    acknowledgement
}

fn process_token_facts(
    process: HANDLE,
) -> Result<keld_ipc::WindowsPeerTokenFacts, WindowsHostJobError> {
    let mut raw_token = std::ptr::null_mut();
    // SAFETY: `process` is a retained process handle and the output is writable.
    if unsafe {
        OpenProcessToken(
            process,
            windows_sys::Win32::Security::TOKEN_QUERY,
            &raw mut raw_token,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new("keeper peer token open"));
    }
    // SAFETY: OpenProcessToken returned one fresh non-null owning token handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw_token.cast()) };
    keld_ipc::query_windows_peer_token_facts(&token).map_err(|source| WindowsHostJobError {
        phase: "keeper peer token profile query",
        source,
    })
}

/// Verified facts about the installed host-death Job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsHostJobObservation {
    /// The exact configured limit flags.
    pub limit_flags: u32,
    /// Whether the host was already inside an outer Job before nesting Keld's
    /// own unnamed Job.
    pub nested_under_existing_job: bool,
    /// Assignment of the current host process to the Keld Job succeeded.
    pub current_process_assigned: bool,
    /// The sole Job handle was verified non-inheritable before assignment.
    pub handle_inheritable: bool,
}

/// Failure to configure or use a Keld-owned Windows process Job.
#[derive(Debug)]
pub struct WindowsHostJobError {
    phase: &'static str,
    source: io::Error,
}

impl WindowsHostJobError {
    fn new(phase: &'static str) -> Self {
        Self {
            phase,
            source: io::Error::last_os_error(),
        }
    }

    fn contract(phase: &'static str, detail: impl Into<String>) -> Self {
        Self {
            phase,
            source: io::Error::other(detail.into()),
        }
    }
}

impl std::fmt::Display for WindowsHostJobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-RUNTIME-014: Windows process Job operation failed during {}: {}. \
             Stop before starting application resources; verify nested Job support and retry.",
            self.phase, self.source
        )
    }
}

impl std::error::Error for WindowsHostJobError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Installs the one process-lifetime Windows Job that reaps Bun descendants
/// when the host dies abnormally.
///
/// Call exactly once, before the first supervised child or listener is
/// created. The unnamed Job grants no open-by-name path. Its handle is
/// explicitly non-inheritable and intentionally retained by the OS until host
/// termination; returning it would let a caller leak lifecycle ownership.
///
/// # Errors
///
/// Fails closed when Job creation/configuration, nested assignment, or exact
/// flag/handle verification fails. No child may be spawned after an error.
pub fn install_host_death_job() -> Result<WindowsHostJobObservation, WindowsHostJobError> {
    let mut outer_job = 0;
    // SAFETY: GetCurrentProcess returns a non-owning pseudo-handle valid for
    // this process lifetime; `outer_job` is live writable BOOL storage. A null
    // Job handle asks whether the process belongs to any Job.
    if unsafe {
        IsProcessInJob(
            GetCurrentProcess(),
            std::ptr::null_mut(),
            &raw mut outer_job,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new("outer Job observation"));
    }

    let job = create_process_job()?;
    let raw_job = job.as_raw_handle().cast();

    // Assignment is deliberately last: every prior failure can close an empty
    // Job harmlessly. The current process may already belong to a CI/launcher
    // Job; Windows 8+ nested Job semantics make this Keld Job the immediate
    // child owner when the outer Job permits nesting.
    // SAFETY: both handles are live for the call; GetCurrentProcess is a
    // non-owning pseudo-handle and `raw_job` is owned by `job`.
    if unsafe { AssignProcessToJobObject(raw_job, GetCurrentProcess()) } == 0 {
        return Err(WindowsHostJobError::new("current-process Job assignment"));
    }

    let observation = WindowsHostJobObservation {
        limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        nested_under_existing_job: outer_job != 0,
        current_process_assigned: true,
        handle_inheritable: false,
    };

    // This is an intentional process-lifetime handle, not a recoverable leak:
    // closing it while the host is alive terminates the host and enrolled tree.
    // The kernel closes it on every abnormal or orderly process-termination path.
    std::mem::forget(job);
    Ok(observation)
}

/// Failure to restrict this process's DLL search to System32
/// (`KELD-RUNTIME-018`).
#[derive(Debug)]
pub struct WindowsDllSearchError {
    source: io::Error,
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

/// The updater helper's fixed recovery-role selector (KEL-53 §4 "Machine-UAC
/// bootstrap" item 3; approved: KEL-270 comment `7905ec8a`, 2026-10-07,
/// decision D3).
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

/// An exact local image path for [`launch_elevated_updater_helper`].
///
/// `SHELLEXECUTEINFO` resolves a file name without a path against the current
/// directory, and hands a document file to its associated application. This
/// type admits only a path that names an `.exe` from a
/// drive root (`C:\...`), with no UNC, `\\?\` or `\\.\` prefix, no `.` or `..`
/// component, no alternate data stream, no NUL and only single `\` separators,
/// so the shell receives exactly the spelling the caller derived. It checks
/// spelling, not file identity: which file runs is decided by the caller's
/// derivation from admitted provenance (KEL-53 §4 "Helper launch and
/// self-anchor").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsUpdaterHelperPath {
    path: PathBuf,
    file: Vec<u16>,
    directory: Vec<u16>,
}

impl WindowsUpdaterHelperPath {
    /// Admits `path` if it is an exact local `.exe` path.
    ///
    /// # Errors
    ///
    /// Returns [`WindowsUpdaterHelperLaunchError::HelperPath`] naming the first
    /// rule that `path` breaks.
    pub fn new(path: &Path) -> Result<Self, WindowsUpdaterHelperLaunchError> {
        let refuse = |rule| Err(WindowsUpdaterHelperLaunchError::HelperPath { rule });
        let mut components = path.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            return refuse("it does not start with a drive letter");
        };
        if !matches!(prefix.kind(), Prefix::Disk(_)) {
            return refuse("its prefix is not a plain drive letter");
        }
        if components.next() != Some(Component::RootDir) {
            return refuse("it is not absolute from the drive root");
        }
        let mut file_name = None;
        for component in components {
            let Component::Normal(name) = component else {
                return refuse("it has a `..` component");
            };
            if name.encode_wide().any(|unit| unit == u16::from(b':')) {
                return refuse("it names an alternate data stream");
            }
            file_name = Some(name);
        }
        let Some(file_name) = file_name else {
            return refuse("it names no file");
        };
        if !Path::new(file_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            return refuse("it does not name an `.exe` image");
        }
        // Component parsing drops `.` components and repeated or trailing
        // separators and reads `/` as a separator, so a path is canonical
        // exactly when its components rebuild the same text.
        if path.components().collect::<PathBuf>().as_os_str() != path.as_os_str() {
            return refuse(
                "it is not canonical: a `.` component or a `/`, doubled or trailing separator",
            );
        }
        let (Some(file), Some(directory)) = (
            nul_terminated_wide(path.as_os_str()),
            path.parent()
                .and_then(|parent| nul_terminated_wide(parent.as_os_str())),
        ) else {
            return refuse("it contains a NUL");
        };
        Ok(Self {
            path: path.to_path_buf(),
            file,
            directory,
        })
    }

    /// The admitted path, unchanged.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

/// Why an elevated updater-helper launch refused or failed
/// (`KELD-RUNTIME-019`).
///
/// No variant leaves a retained helper process. [`Self::HelperPath`],
/// [`Self::ArgumentShape`] and [`Self::ComInitialization`] refuse before the
/// shell is called, and [`Self::Declined`] means the user refused consent.
#[derive(Debug)]
pub enum WindowsUpdaterHelperLaunchError {
    /// The helper path is not an exact local `.exe` path.
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
    /// launched process can be bound to the launch.
    NoProcessHandle,
    /// The returned handle could not report its process ID; it was closed.
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
                "the helper path is not an exact local `.exe` path: {rule}. Derive it again \
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
                "ShellExecuteExW returned no process handle. The launch is refused because no \
                 connecting client can be bound to the launched process; report the detail.",
            ),
            Self::ProcessIdentity { source } => write!(
                f,
                "the returned process handle could not report its process ID: {source}. The \
                 handle was closed and the launch refused, because no connecting client can be \
                 bound to it; report the detail."
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
    fn identify(process: OwnedHandle) -> Result<Self, WindowsUpdaterHelperLaunchError> {
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
    let file = path.file.clone();
    let directory = path.directory.clone();
    let parameters = nul_terminated_wide(OsStr::new(argument.as_str()))
        .ok_or(WindowsUpdaterHelperLaunchError::ArgumentShape)?;
    let launch = std::thread::Builder::new()
        .name("keld-updater-helper-launch".to_owned())
        .spawn(move || runas_on_this_thread(&file, &parameters, &directory))
        .map_err(|source| WindowsUpdaterHelperLaunchError::LaunchThread { source })?;
    let process = launch
        .join()
        .map_err(|_| WindowsUpdaterHelperLaunchError::LaunchThread {
            source: io::Error::other("the launch thread ended without a result"),
        })??;
    WindowsElevatedUpdaterHelper::identify(process)
}

/// Makes the one `runas` call inside a COM apartment that the current thread
/// enters first and leaves after the call, and returns the process handle.
fn runas_on_this_thread(
    file: &[u16],
    parameters: &[u16],
    directory: &[u16],
) -> Result<OwnedHandle, WindowsUpdaterHelperLaunchError> {
    let _apartment = ComApartment::enter()?;
    let verb: Vec<u16> = "runas".encode_utf16().chain(std::iter::once(0)).collect();
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
    let process = runas_outcome(succeeded, error, info.hInstApp.addr(), info.hProcess)?;
    // SAFETY: `runas_outcome` admits only a successful call's non-null
    // hProcess, which SEE_MASK_NOCLOSEPROCESS makes this caller's to close;
    // nothing else holds it, so it moves into exactly one owner here.
    Ok(unsafe { OwnedHandle::from_raw_handle(process.cast()) })
}

/// Classifies one finished `ShellExecuteExW` call: a failure with
/// `ERROR_CANCELLED` is the user's decline, any other failure is reported with
/// its `hInstApp` diagnostic, and a success counts only with a process handle.
fn runas_outcome(
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
struct ComApartment {
    _thread_bound: PhantomData<*const ()>,
}

impl ComApartment {
    fn enter() -> Result<Self, WindowsUpdaterHelperLaunchError> {
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

#[cfg(test)]
#[allow(unsafe_code)] // test-only native Job-limit negative control
#[allow(clippy::expect_used, clippy::panic)] // subprocess contract failures abort the proof
mod tests {
    use super::*;
    use crate::windows_lpac::{WindowsLpacProfile, WindowsLpacStdio};
    use std::ffi::{OsStr, OsString};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use windows_sys::Win32::Foundation::ERROR_INVALID_HANDLE;
    use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::PROCESS_CREATE_PROCESS;

    const HELPER_ENV: &str = "KELD_WINDOWS_JOB_ASSIGNMENT_HELPER";
    const QF1_HELPER_ENDPOINT_ENV: &str = "KELD_TEST_QF1_ENDPOINT";
    const QF1_HELPER_SERVER_PID_ENV: &str = "KELD_TEST_QF1_SERVER_PID";
    const QF1_HELPER_SERVER_SESSION_ENV: &str = "KELD_TEST_QF1_SERVER_SESSION";
    const QF1_HELPER_STATE_ENV: &str = "KELD_TEST_QF1_STATE";
    const CONTINUE_BYTE: u8 = 0xa7;

    #[test]
    fn finite_win32_wait_never_uses_the_infinite_timeout_sentinel() {
        assert_ne!(
            finite_wait_millis(Duration::from_millis(u64::from(u32::MAX))),
            u32::MAX
        );
        assert_ne!(
            finite_wait_millis(Duration::from_millis(u64::from(u32::MAX) + 1)),
            u32::MAX
        );
        assert_eq!(finite_wait_millis(Duration::from_millis(0)), 0);
    }

    #[test]
    fn pipe_peer_observation_pins_exact_process_and_rejects_wrong_session() {
        let process_id = std::process::id();
        let mut session_id = 0_u32;
        // SAFETY: `process_id` is this live test process and the output is writable.
        assert_ne!(
            unsafe { ProcessIdToSessionId(process_id, &raw mut session_id) },
            0
        );
        let peer = WindowsProcessPeer::open(process_id, session_id)
            .expect("open exact current test process from pipe observation");
        assert_eq!(peer.process_id(), process_id);
        assert_eq!(peer.session_id(), session_id);
        assert!(peer.image_path().is_file());
        assert!(!peer.has_exited().expect("query retained process object"));

        assert!(
            WindowsProcessPeer::open(process_id, session_id.wrapping_add(1)).is_err(),
            "a different pipe session must not authenticate this process"
        );
    }

    #[test]
    fn pipe_peer_observation_rejects_an_exited_process() {
        let mut child = spawn_blocked_child();
        let mut session_id = 0_u32;
        // SAFETY: child is live and its PID came from std::process::Child.
        assert_ne!(
            unsafe { ProcessIdToSessionId(child.id(), &raw mut session_id) },
            0
        );
        child.kill().expect("terminate peer fixture");
        let _ = child.wait().expect("reap peer fixture");
        assert!(
            WindowsProcessPeer::open(child.id(), session_id).is_err(),
            "an exited PID must not produce a live peer observation"
        );
    }

    #[expect(
        clippy::too_many_lines,
        reason = "real successor and keeper subprocesses prove the post-QA1 QF1 loss boundary"
    )]
    #[test]
    fn missing_qf1_never_returns_retirement_witness_or_mutates_state() {
        use std::fs::{self, OpenOptions};
        use std::os::windows::fs::OpenOptionsExt as _;
        use std::process::Output;

        use keld_ipc::{AppLinkDeadlines as _, WindowsLifecycleRendezvousListener};

        const INSTALLATION_ID: [u8; 32] = [0x11; 32];
        const ATTEMPT_ID: [u8; 32] = [0x22; 32];
        const CHANNEL_ID: [u8; 32] = [0x33; 32];
        const LOCATOR: [u8; 32] = [0xe1; 32];
        let fixture = tempfile::tempdir().expect("QF1 loss fixture");
        let lock_path = fixture.path().join("activation.lock");
        let writer = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&lock_path)
            .expect("open exact share-zero activation lease");
        let activation_lease: OwnedHandle = writer.into();
        let state_path = fixture.path().join("pending-state.bin");
        let prior_state = b"journal-and-pointers-before-qf1";
        fs::write(&state_path, prior_state).expect("write fixture journal/pointer state");

        let mut attempt = WindowsProcessJob::create().expect("create exact attempt Job");
        let mut member = spawn_blocked_child();
        attempt
            .assign_child(&member)
            .expect("assign exact candidate member");
        assert_eq!(attempt.active_processes().expect("initial Job census"), 1);

        let mut coordinator = spawn_blocked_child();
        let mut coordinator_session = 0_u32;
        // SAFETY: coordinator is live and the session output is writable.
        assert_ne!(
            unsafe { ProcessIdToSessionId(coordinator.id(), &raw mut coordinator_session) },
            0
        );
        let coordinator_peer = WindowsProcessPeer::open(coordinator.id(), coordinator_session)
            .expect("pin exact coordinator process");
        let WindowsProcessJob { handle, .. } = attempt;
        let witness = WindowsLifecycleJobWitness::adopt_transferred(handle, coordinator_peer)
            .expect("adopt exact Job as bounded keeper witness");
        let binding = WindowsLifecycleBinding::new(
            INSTALLATION_ID,
            ATTEMPT_ID,
            CHANNEL_ID,
            WindowsLifecyclePurpose::CoordinatorToKeeper,
        )
        .expect("valid attempt binding");
        let mut keeper = WindowsLifecycleKeeperHandoff {
            witness,
            activation_lease: Some(activation_lease),
            binding,
            retirement_started: false,
        };
        coordinator
            .kill()
            .expect("retire exact coordinator fixture");
        let _ = coordinator.wait().expect("reap coordinator fixture");

        let successor_binding = binding.with_purpose(WindowsLifecyclePurpose::KeeperToSuccessor);
        let listener = WindowsLifecycleRendezvousListener::bind(LOCATOR, successor_binding)
            .expect("bind exact one-shot keeper endpoint");
        let endpoint = listener.endpoint().to_owned();
        let server_pid = std::process::id();
        let mut server_session = 0_u32;
        // SAFETY: this live keeper PID and writable session output are valid.
        assert_ne!(
            unsafe { ProcessIdToSessionId(server_pid, &raw mut server_session) },
            0
        );
        let executable = std::env::current_exe()
            .expect("unit-test executable")
            .canonicalize()
            .expect("canonical unit-test executable");
        let mut successor_command = Command::new(&executable);
        successor_command
            .args([
                "--exact",
                "windows_job::tests::qf1_loss_successor_helper",
                "--ignored",
                "--nocapture",
            ])
            .env(HELPER_ENV, "qf1-successor")
            .env(QF1_HELPER_ENDPOINT_ENV, &endpoint)
            .env(QF1_HELPER_SERVER_PID_ENV, server_pid.to_string())
            .env(QF1_HELPER_SERVER_SESSION_ENV, server_session.to_string())
            .env(QF1_HELPER_STATE_ENV, &state_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let successor = successor_command
            .spawn()
            .expect("spawn authenticated QF1 successor");
        let expected_image = executable;
        let peer = listener
            .accept_until(
                Instant::now() + Duration::from_secs(5),
                |pid, session, facts| {
                    if pid != successor.id()
                        || session != server_session
                        || facts.session_id != session
                    {
                        return None;
                    }
                    let pin = WindowsProcessPeer::open(pid, session).ok()?;
                    (pin.image_path().canonicalize().ok()? == expected_image
                        && pin.token_facts() == facts)
                        .then_some(pin)
                },
            )
            .expect("accept exact authenticated successor before deadline")
            .expect("successor completes LC1/LA1/LR1");

        let result = keeper.retire_to_successor_inner(
            peer,
            Duration::from_secs(10),
            Instant::now() + Duration::from_secs(10),
            |peer| {
                let probe = OpenOptions::new()
                    .read(true)
                    .open(&lock_path)
                    .expect("QA1 releases keeper lease before QF1");
                drop(probe);
                peer.stream_mut()
                    .shutdown_app_link()
                    .expect("simulate lost QF1 delivery after valid QA1");
            },
        );
        let error = result.expect_err("lost QF1 must fail keeper retirement");
        assert_eq!(error.phase, "lifecycle successor release receipt");
        assert!(
            keeper.activation_lease.is_none(),
            "valid QA1 is the lease-release point even if QF1 delivery fails"
        );
        assert_eq!(
            keeper.active_processes().expect("post-retirement census"),
            0
        );
        assert!(
            OpenOptions::new().read(true).open(&lock_path).is_ok(),
            "writer may acquire after QA1; mutation still requires the missing QF1 witness"
        );

        let successor_output: Output = successor
            .wait_with_output()
            .expect("wait successor after lost QF1");
        assert!(
            successor_output.status.success(),
            "successor failed: {}",
            String::from_utf8_lossy(&successor_output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&successor_output.stdout).contains("SUCCESSOR_NO_QF1_WITNESS"),
            "successor must reject QF1 loss without returning its witness: {}",
            String::from_utf8_lossy(&successor_output.stdout)
        );
        assert!(
            !String::from_utf8_lossy(&successor_output.stdout)
                .contains("SUCCESSOR_WITNESS_GRANTED"),
            "no retirement witness may be admitted without QF1"
        );
        assert_eq!(
            fs::read(&state_path).expect("read protected fixture state after QF1 loss"),
            prior_state,
            "QF1 loss must leave journal/pointer fixture bytes unchanged"
        );
        drop(keeper);
        assert!(
            !member
                .wait()
                .expect("wait terminated attempt member")
                .success(),
            "retirement must terminate the exact attempt member"
        );
    }

    #[test]
    #[ignore = "private QF1 loss successor subprocess entry point"]
    fn qf1_loss_successor_helper() {
        use keld_ipc::{WindowsLifecycleExpectation, connect_windows_lifecycle_rendezvous_until};
        use std::fs;

        assert_eq!(
            std::env::var(HELPER_ENV).as_deref(),
            Ok("qf1-successor"),
            "unexpected QF1 loss helper entry"
        );
        let endpoint = std::env::var(QF1_HELPER_ENDPOINT_ENV).expect("keeper endpoint");
        let server_pid = std::env::var(QF1_HELPER_SERVER_PID_ENV)
            .expect("keeper PID")
            .parse::<u32>()
            .expect("valid keeper PID");
        let server_session = std::env::var(QF1_HELPER_SERVER_SESSION_ENV)
            .expect("keeper session")
            .parse::<u32>()
            .expect("valid keeper session");
        let state_path = PathBuf::from(std::env::var(QF1_HELPER_STATE_ENV).expect("state path"));
        let expected_image = std::env::current_exe()
            .expect("unit-test executable")
            .canonicalize()
            .expect("canonical unit-test executable");
        let connection = connect_windows_lifecycle_rendezvous_until(
            &endpoint,
            WindowsLifecycleExpectation::from_keeper([0x11; 32])
                .expect("known installation identity"),
            Instant::now() + Duration::from_secs(5),
            move |pid, session| {
                if pid != server_pid || session != server_session {
                    return None;
                }
                let peer = WindowsProcessPeer::open(pid, session).ok()?;
                (peer.image_path().canonicalize().ok()? == expected_image).then_some(peer)
            },
        )
        .expect("authenticate exact keeper and complete LC1/LA1/LR1");
        let pending = WindowsLifecycleRetirementPending::receive(
            connection,
            Instant::now() + Duration::from_secs(5),
        )
        .expect("independently query exact Job zero before QA1");
        assert_eq!(pending.active_processes().expect("successor Job census"), 0);
        match pending.acknowledge() {
            Ok(_witness) => {
                fs::write(&state_path, b"QF1-witness-granted").expect("mutation negative control");
                println!("SUCCESSOR_WITNESS_GRANTED");
            }
            Err(error) => {
                assert!(
                    error
                        .to_string()
                        .contains("lifecycle successor release receipt"),
                    "successor failed at an unexpected phase: {error}"
                );
                println!("SUCCESSOR_NO_QF1_WITNESS");
            }
        }
        io::stdout().flush().expect("flush QF1 loss result");
    }

    #[test]
    fn keeper_duplicate_cannot_assign_and_reaches_exact_job_zero() {
        let mut attempt = WindowsProcessJob::create().expect("create exact attempt Job");
        let mut child = spawn_blocked_child();
        attempt
            .assign_child(&child)
            .expect("assign child before keeper transfer");
        let mut coordinator = spawn_blocked_child();
        let mut session_id = 0_u32;
        // SAFETY: the spawned coordinator is live and the output is writable.
        assert_ne!(
            unsafe { ProcessIdToSessionId(coordinator.id(), &raw mut session_id) },
            0
        );
        let coordinator_peer =
            WindowsProcessPeer::open(coordinator.id(), session_id).expect("pin coordinator");

        // PROCESS_CREATE_PROCESS deliberately grants the 0x80 process right
        // that would result if FILE_READ_ATTRIBUTES were misapplied to this
        // non-file kernel object. The source remains non-inheritable.
        // SAFETY: the exact coordinator PID is live and the requested rights
        // are limited to this test's object-type negative control.
        let raw_process = unsafe {
            OpenProcess(
                PROCESS_CREATE_PROCESS | PROCESS_SYNCHRONIZE,
                0,
                coordinator.id(),
            )
        };
        assert!(
            !raw_process.is_null(),
            "open process source for type control"
        );
        // SAFETY: OpenProcess returned one fresh owning process handle.
        let process_handle = unsafe { OwnedHandle::from_raw_handle(raw_process.cast()) };
        let wrong_type_error = attempt
            .transfer_activation_lease_handle_to(&coordinator_peer, &process_handle)
            .expect_err("a process handle must not pass as activation-lock file retention");
        assert!(
            wrong_type_error
                .to_string()
                .contains("not a disk-file object"),
            "source object type was rejected by an unexpected predicate: {wrong_type_error}"
        );

        let mut full_access = std::ptr::null_mut();
        // SAFETY: this test duplicates the exact original Job into itself to
        // verify that adoption attenuates an accidentally broad incoming handle.
        assert_ne!(
            unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    attempt.handle.as_raw_handle().cast(),
                    GetCurrentProcess(),
                    &raw mut full_access,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            },
            0
        );
        // SAFETY: the successful DuplicateHandle above returned one fresh owner.
        let full_access = unsafe { OwnedHandle::from_raw_handle(full_access.cast()) };
        let witness = WindowsLifecycleJobWitness::adopt_transferred(full_access, coordinator_peer)
            .expect("adopt and attenuate non-inheritable Job handle");
        assert_eq!(witness.active_processes().expect("keeper query"), 1);

        // SAFETY: the keeper witness and child both retain their handles during
        // the attempted operation. Lack of ASSIGN_PROCESS is the expected oracle.
        let assign = unsafe {
            AssignProcessToJobObject(
                witness.handle.as_raw_handle().cast(),
                child.as_raw_handle().cast(),
            )
        };
        assert_eq!(assign, 0, "keeper handle must not assign Job members");

        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let limits_size =
            u32::try_from(std::mem::size_of_val(&limits)).expect("Job limit structure fits u32");
        // SAFETY: the witness and limits buffer are live; Windows must reject
        // SetInformationJobObject because adoption stripped SET_ATTRIBUTES.
        let change_limits = unsafe {
            SetInformationJobObject(
                witness.handle.as_raw_handle().cast(),
                JobObjectExtendedLimitInformation,
                (&raw mut limits).cast(),
                limits_size,
            )
        };
        assert_eq!(
            change_limits, 0,
            "keeper handle must not change Job attributes"
        );

        assert!(
            witness
                .terminate_and_wait_zero(Duration::from_secs(10))
                .is_err(),
            "keeper must not terminate while the retained coordinator is live"
        );
        assert_eq!(
            witness.active_processes().expect("live coordinator census"),
            1
        );
        coordinator.kill().expect("kill coordinator fixture");
        let _ = coordinator.wait().expect("reap coordinator fixture");

        witness
            .terminate_and_wait_zero(Duration::from_secs(10))
            .expect("keeper terminates exact Job and sees zero");
        assert_eq!(
            witness.active_processes().expect("post-termination query"),
            0
        );
        let status = child.wait().expect("wait terminated Job member");
        assert!(!status.success(), "Job termination must stop the member");
    }

    #[test]
    fn unrelated_job_hierarchy_refusal_leaves_the_host_gated() {
        let mut outer = WindowsProcessJob::create().expect("create outer launcher Job");
        let mut target = WindowsProcessJob::create().expect("create unrelated attempt Job");
        let mut occupant = spawn_blocking_descendant();
        target
            .assign_child(&occupant)
            .expect("establish a nonempty independent Job hierarchy");
        assert_eq!(target.active_processes().expect("independent census"), 1);

        let mut child = spawn_blocked_child();
        if let Err(error) = outer.assign_child(&child) {
            drop(child.stdin.take());
            let _ = child.wait();
            panic!("assign gate child to outer Job: {error}");
        }
        let assignment = target.assign_child(&child);
        if assignment.is_ok() {
            drop(child.stdin.take());
            target
                .terminate_and_wait(&child, Duration::from_secs(10))
                .expect("clean up unexpectedly admitted host");
            let _ = child.wait();
            let _ = occupant.wait();
            panic!("Windows admitted an unrelated nonempty Job into this hierarchy");
        }
        drop(child.stdin.take());
        let status = child.wait().expect("wait for gate child");
        let output = child
            .stdout
            .take()
            .map(|mut output| {
                let mut bytes = Vec::new();
                output
                    .read_to_end(&mut bytes)
                    .expect("read gate child output");
                String::from_utf8(bytes).expect("UTF-8 gate child output")
            })
            .unwrap_or_default();
        assert!(
            assignment.is_err(),
            "Windows unexpectedly admitted a process into an unrelated nonempty Job"
        );
        assert_eq!(
            assignment
                .expect_err("assert native assignment failure")
                .phase,
            "attempt Job process assignment"
        );
        assert!(
            !status.success(),
            "child continued after assignment refusal"
        );
        let stderr = child
            .stderr
            .take()
            .map(|mut stderr| {
                let mut bytes = Vec::new();
                stderr
                    .read_to_end(&mut bytes)
                    .expect("read start-gate refusal marker");
                String::from_utf8(bytes).expect("UTF-8 start-gate error")
            })
            .unwrap_or_default();
        assert!(
            stderr.contains("START_REFUSED"),
            "host did not observe launcher EOF after assignment refusal: {stderr}"
        );
        assert!(
            !output.contains("START_RELEASED"),
            "start gate released after assignment refusal: {output}"
        );
        assert_eq!(
            target.active_processes().expect("independent Job census"),
            1
        );
        target
            .terminate_and_wait(&occupant, Duration::from_secs(10))
            .expect("terminate target Job occupant");
        let _ = occupant.wait().expect("wait for target Job occupant");
        assert_eq!(outer.active_processes().expect("outer Job census"), 0);
    }

    #[test]
    #[ignore = "private subprocess entry point"]
    fn assignment_gate_child() {
        assert_eq!(
            std::env::var(HELPER_ENV).as_deref(),
            Ok("child"),
            "unexpected private assignment fixture entry"
        );
        let mut start = [0_u8; 1];
        if io::stdin().read_exact(&mut start).is_ok() && start == [CONTINUE_BYTE] {
            println!("START_RELEASED");
            std::process::exit(0);
        }
        eprintln!("START_REFUSED");
        std::process::exit(72);
    }

    fn spawn_blocked_child() -> Child {
        let mut command = Command::new(std::env::current_exe().expect("current test executable"));
        command
            .args([
                "--exact",
                "windows_job::tests::assignment_gate_child",
                "--ignored",
                "--nocapture",
            ])
            .env(HELPER_ENV, "child")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.spawn().expect("spawn assignment-gate child")
    }

    fn spawn_blocking_descendant() -> Child {
        let mut command = Command::new(std::env::current_exe().expect("current test executable"));
        command
            .args([
                "--exact",
                "windows_job::tests::blocking_descendant_helper",
                "--ignored",
                "--nocapture",
            ])
            .env(HELPER_ENV, "occupant")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command.spawn().expect("spawn active-process occupant")
    }

    #[test]
    #[ignore = "private subprocess entry point"]
    fn blocking_descendant_helper() {
        assert_eq!(
            std::env::var(HELPER_ENV).as_deref(),
            Ok("occupant"),
            "unexpected private active-process helper entry"
        );
        std::thread::park();
    }

    // KEL-270 T4d S5: the connect-back claimant binding (KEL-53 §4 "Candidate
    // connect-back", *Acceptance*; §7 row 8, claimant binding). Every launch comes
    // from the generalized suspended-child path and every compared handle is a
    // real Windows handle unless the cell says it is seam-injected.

    static CLAIMANT_LAUNCH_SEQUENCE: AtomicU32 = AtomicU32::new(0);

    /// Standard handles that keep a resumed command interpreter blocked on stdin.
    struct BlockingStdio {
        stdin: std::io::PipeReader,
        _stdin_writer: std::io::PipeWriter,
        sink: std::fs::File,
    }

    /// One recorded suspended launch. Fields drop in order: the launch, which
    /// terminates an unreaped child, before its profile and its stdin writer.
    struct RecordedLaunch {
        launched: WindowsLaunchedProcess,
        _profile: WindowsLpacProfile,
        _stdio: Option<BlockingStdio>,
    }

    fn system32() -> PathBuf {
        PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set")).join("System32")
    }

    fn blocking_stdio() -> BlockingStdio {
        let (stdin, stdin_writer) = std::io::pipe().expect("create the launch stdin pipe");
        let sink = std::fs::OpenOptions::new()
            .write(true)
            .open("NUL")
            .expect("open the launch output sink");
        BlockingStdio {
            stdin,
            _stdin_writer: stdin_writer,
            sink,
        }
    }

    fn recorded_launch(stdio: Option<BlockingStdio>) -> RecordedLaunch {
        let sequence = CLAIMANT_LAUNCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after the epoch")
            .as_nanos();
        let name = format!(
            "Keld.Test.Claimant.{}.{sequence}.{nanos}",
            std::process::id()
        );
        let profile = WindowsLpacProfile::create(OsStr::new(&name))
            .expect("create a zero-capability LPAC profile");
        let system32 = system32();
        // AppContainer creation derives the package's LOCALAPPDATA/TEMP from this
        // block and fails with ERROR_ENVVAR_NOT_FOUND without the profile keys.
        let environment: Vec<(OsString, OsString)> =
            ["SystemRoot", "WINDIR", "USERPROFILE", "LOCALAPPDATA"]
                .into_iter()
                .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
                .collect();
        let standard_handles = stdio.as_ref().map(|stdio| WindowsLpacStdio {
            stdin: stdio.stdin.as_handle(),
            stdout: stdio.sink.as_handle(),
            stderr: stdio.sink.as_handle(),
        });
        let child = profile
            .spawn_suspended(
                &system32.join("cmd.exe"),
                &[OsString::from("/d")],
                &environment,
                Some(&system32),
                standard_handles,
                &[],
            )
            .expect("create one suspended launch");
        let launched = WindowsLaunchedProcess::record(child)
            .expect("record the launch identity before resume");
        RecordedLaunch {
            launched,
            _profile: profile,
            _stdio: stdio,
        }
    }

    /// Opens a claimant exactly as the owner does: from the PID and session the
    /// connected pipe reports.
    fn claimant_by_pid(process_id: u32) -> WindowsProcessPeer {
        let mut session_id = 0_u32;
        // SAFETY: the PID names a live test-owned process; the output is writable.
        assert_ne!(
            unsafe { ProcessIdToSessionId(process_id, &raw mut session_id) },
            0,
            "query claimant session: {}",
            io::Error::last_os_error()
        );
        WindowsProcessPeer::open(process_id, session_id).expect("open the pipe-reported claimant")
    }

    #[test]
    fn compare_object_handles_classifies_same_different_and_invalid_handles() {
        let launch = recorded_launch(None);
        let launch_handle: HANDLE = launch
            .launched
            .child()
            .process_handle()
            .as_raw_handle()
            .cast();
        let same = claimant_by_pid(launch.launched.child().id());
        assert!(
            same_kernel_object(same.process.as_raw_handle().cast(), launch_handle)
                .expect("compare two live handles"),
            "a handle opened by PID and the retained launch handle are one process object"
        );
        let other = claimant_by_pid(std::process::id());
        assert!(
            !same_kernel_object(other.process.as_raw_handle().cast(), launch_handle)
                .expect("compare two live handles"),
            "two different process objects compare unequal"
        );
        // Null and a value above the per-process handle-table range are never
        // live handles. A closed value is proved in its own process by
        // `compare_object_handles_refuses_a_closed_handle`, because a parallel
        // test here can reuse a freed handle value.
        for invalid in [
            std::ptr::null_mut(),
            std::ptr::without_provenance_mut(0x7fff_fff0),
        ] {
            for (first, second) in [(invalid, launch_handle), (launch_handle, invalid)] {
                let error = same_kernel_object(first, second)
                    .expect_err("an invalid handle is neither the same nor a different object");
                assert_eq!(
                    error.raw_os_error(),
                    Some(ERROR_INVALID_HANDLE.cast_signed()),
                    "{error}"
                );
            }
        }
    }

    #[test]
    fn compare_object_handles_refuses_a_closed_handle() {
        // The helper runs alone in a fresh test process, so no other test can
        // reuse the closed handle value between its close and the comparisons.
        let output = Command::new(std::env::current_exe().expect("current test executable"))
            .args([
                "--exact",
                "windows_job::tests::closed_handle_comparison_helper",
                "--ignored",
                "--nocapture",
            ])
            .env(HELPER_ENV, "closed-handle")
            .stdin(Stdio::null())
            .output()
            .expect("run the closed-handle comparison helper");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("CLOSED_HANDLE_REFUSED"),
            "closed-handle helper status {:?}\nstdout:\n{stdout}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    #[ignore = "private subprocess entry point"]
    fn closed_handle_comparison_helper() {
        assert_eq!(
            std::env::var(HELPER_ENV).as_deref(),
            Ok("closed-handle"),
            "unexpected private closed-handle helper entry"
        );
        let retained = claimant_by_pid(std::process::id());
        let live = ProcessObservation::of_peer(&retained);
        let second = claimant_by_pid(std::process::id());
        let closed: HANDLE = second.process.as_raw_handle().cast();
        assert!(
            same_kernel_object(closed, live.handle).expect("compare two live handles"),
            "two live handles to this process are one object before the close"
        );
        drop(second);
        assert!(
            handle_value_is_closed(closed),
            "the second handle is closed"
        );
        for (first, second) in [(closed, live.handle), (live.handle, closed)] {
            let error = same_kernel_object(first, second)
                .expect_err("a closed handle is neither the same nor a different object");
            assert_eq!(
                error.raw_os_error(),
                Some(ERROR_INVALID_HANDLE.cast_signed()),
                "{error}"
            );
        }
        let claimant = ProcessObservation {
            handle: closed,
            ..live
        };
        match bind_observed_claimant(claimant, live) {
            Err(WindowsClaimantRefusal::Unverifiable { phase, source }) => {
                assert_eq!(phase, "claimant process-object comparison");
                assert_eq!(
                    source.raw_os_error(),
                    Some(ERROR_INVALID_HANDLE.cast_signed())
                );
            }
            other => panic!("a closed claimant handle must refuse as unverifiable: {other:?}"),
        }
        // Still unused after the comparisons, so each one saw the closed value
        // rather than a reused handle.
        assert!(
            handle_value_is_closed(closed),
            "the closed value stayed unused"
        );
        println!("CLOSED_HANDLE_REFUSED");
    }

    /// Independent oracle for a closed value: `GetHandleInformation` refuses a
    /// value that names no open handle in this process.
    fn handle_value_is_closed(handle: HANDLE) -> bool {
        let mut flags = 0_u32;
        // SAFETY: the call only resolves the value through this process's handle
        // table and writes one u32 to live local storage.
        let resolved = unsafe { GetHandleInformation(handle, &raw mut flags) };
        resolved == 0
            && io::Error::last_os_error().raw_os_error() == Some(ERROR_INVALID_HANDLE.cast_signed())
    }

    #[test]
    fn claimant_binding_accepts_the_exact_launched_process_before_and_after_resume() {
        let mut launch = recorded_launch(Some(blocking_stdio()));
        let process_id = launch.launched.child().id();
        let claimant = claimant_by_pid(process_id);
        assert_eq!(
            launch.launched.creation_time,
            claimant.creation_time(),
            "the launch record equals the creation time read through a second handle"
        );
        launch
            .launched
            .bind_claimant(&claimant)
            .expect("the exact suspended launch binds");

        launch.launched.resume().expect("resume the launch once");
        assert!(
            !claimant
                .has_exited()
                .expect("query the resumed launch through the claimant handle"),
            "the resumed interpreter waits on its stdin pipe"
        );
        launch
            .launched
            .bind_claimant(&claimant)
            .expect("the retained launch handle still binds after resume");
        launch
            .launched
            .bind_claimant(&claimant_by_pid(process_id))
            .expect("a claimant handle opened after resume binds");

        launch
            .launched
            .terminate(0)
            .expect("terminate the resumed launch");
        launch
            .launched
            .wait(10_000)
            .expect("reap the resumed launch");
    }

    #[test]
    fn claimant_binding_refuses_a_second_instance_of_the_launched_image() {
        let launch = recorded_launch(None);
        let mut copy = Command::new(system32().join("cmd.exe"))
            .arg("/d")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start a same-user copy of the launched image");
        let claimant = claimant_by_pid(copy.id());
        let refusal = launch.launched.bind_claimant(&claimant);
        copy.kill().expect("terminate the same-image copy");
        let _ = copy.wait().expect("reap the same-image copy");
        assert!(
            matches!(refusal, Err(WindowsClaimantRefusal::NotLaunchedProcess)),
            "{refusal:?}"
        );
    }

    #[test]
    fn claimant_binding_refuses_a_forged_peer_carrying_the_launched_identity() {
        // Seam-injected: a peer whose process ID and creation time equal the
        // launch record but whose process object differs. Windows cannot give two
        // live processes one PID, so another real process handle carries the
        // launched identity numbers.
        let launch = recorded_launch(None);
        let other = claimant_by_pid(std::process::id());
        let recorded = ProcessObservation::of_launch(&launch.launched);
        let forged = ProcessObservation {
            process_id: recorded.process_id,
            creation_time: recorded.creation_time,
            ..ProcessObservation::of_peer(&other)
        };
        let refusal = bind_observed_claimant(forged, recorded);
        assert!(
            matches!(refusal, Err(WindowsClaimantRefusal::NotLaunchedProcess)),
            "{refusal:?}"
        );
    }

    #[test]
    fn claimant_binding_refuses_once_the_launch_handle_is_signaled() {
        let mut launch = recorded_launch(None);
        let claimant = claimant_by_pid(launch.launched.child().id());
        launch
            .launched
            .bind_claimant(&claimant)
            .expect("the live launch binds before termination");
        signal_launch(&mut launch.launched);
        let refusal = launch.launched.bind_claimant(&claimant);
        assert!(
            matches!(refusal, Err(WindowsClaimantRefusal::LaunchExited)),
            "{refusal:?}"
        );
    }

    /// Terminates a recorded launch and waits until its retained handle is
    /// signaled with the exit code the termination set.
    fn signal_launch(launched: &mut WindowsLaunchedProcess) {
        launched.terminate(7).expect("terminate the launch");
        assert_eq!(
            launched
                .wait(10_000)
                .expect("observe the signaled launch handle"),
            7
        );
    }

    // Fact order (KEL-53 §4 "Candidate connect-back", *Acceptance*: "in order").
    // Each cell fails more than one fact, so a reordered binding reports a
    // different refusal.

    #[test]
    fn claimant_binding_compares_the_process_object_before_launch_liveness() {
        // Every fact fails: another process object, a signaled launch handle and
        // a different process ID. Only the comparison-first order reports
        // NotLaunchedProcess.
        let mut launch = recorded_launch(None);
        signal_launch(&mut launch.launched);
        let other = claimant_by_pid(std::process::id());
        assert_ne!(other.process_id, launch.launched.child().id());
        let refusal = launch.launched.bind_claimant(&other);
        assert!(
            matches!(refusal, Err(WindowsClaimantRefusal::NotLaunchedProcess)),
            "{refusal:?}"
        );
    }

    #[test]
    fn claimant_binding_checks_launch_liveness_before_the_recorded_identity() {
        // Seam-injected: the compared handles are one process object, opened
        // before the launch exits, while the launch handle is signaled and the
        // recorded creation time is wrong. Only liveness-before-identity reports
        // LaunchExited.
        let mut launch = recorded_launch(None);
        let peer = claimant_by_pid(launch.launched.child().id());
        signal_launch(&mut launch.launched);
        let claimant = ProcessObservation::of_peer(&peer);
        let recorded = ProcessObservation::of_launch(&launch.launched);
        assert!(
            same_kernel_object(claimant.handle, recorded.handle).expect("compare two live handles"),
            "the claimant handle still names the exited launch object"
        );
        let wrong_time = ProcessObservation {
            creation_time: claimant.creation_time + 1,
            ..recorded
        };
        let refusal = bind_observed_claimant(claimant, wrong_time);
        assert!(
            matches!(refusal, Err(WindowsClaimantRefusal::LaunchExited)),
            "{refusal:?}"
        );
    }

    #[test]
    fn claimant_binding_refuses_a_wrong_recorded_creation_time_or_process_id() {
        // Seam-injected: the recorded values differ from the kernel's while the
        // compared handles are one live process object.
        let launch = recorded_launch(None);
        let peer = claimant_by_pid(launch.launched.child().id());
        let claimant = ProcessObservation::of_peer(&peer);
        let recorded = ProcessObservation::of_launch(&launch.launched);
        bind_observed_claimant(claimant, recorded).expect("the unaltered record binds");
        let wrong_time = ProcessObservation {
            creation_time: recorded.creation_time + 1,
            ..recorded
        };
        let refusal = bind_observed_claimant(claimant, wrong_time);
        assert!(
            matches!(refusal, Err(WindowsClaimantRefusal::LaunchIdentityMismatch)),
            "{refusal:?}"
        );
        let wrong_pid = ProcessObservation {
            process_id: recorded.process_id ^ 4,
            ..recorded
        };
        let refusal = bind_observed_claimant(claimant, wrong_pid);
        assert!(
            matches!(refusal, Err(WindowsClaimantRefusal::LaunchIdentityMismatch)),
            "{refusal:?}"
        );
    }

    #[test]
    fn claimant_binding_refuses_an_invalid_claimant_handle_with_a_typed_error() {
        // Seam-injected: safe callers cannot hold an invalid owned handle, so the
        // observation carries a null handle with the launched identity.
        let launch = recorded_launch(None);
        let recorded = ProcessObservation::of_launch(&launch.launched);
        let invalid = ProcessObservation {
            handle: std::ptr::null_mut(),
            ..recorded
        };
        match bind_observed_claimant(invalid, recorded) {
            Err(WindowsClaimantRefusal::Unverifiable { phase, source }) => {
                assert_eq!(phase, "claimant process-object comparison");
                assert_eq!(
                    source.raw_os_error(),
                    Some(ERROR_INVALID_HANDLE.cast_signed())
                );
            }
            other => panic!("an invalid claimant handle must refuse as unverifiable: {other:?}"),
        }
    }

    #[test]
    fn claimant_refusals_render_runtime_017_with_fix_guidance() {
        let refusal = WindowsClaimantRefusal::NotLaunchedProcess;
        assert_eq!(
            refusal.to_string(),
            "KELD-RUNTIME-017: connect-back claimant refused: the connected process is not \
             the launched and retained process object. Disconnect this client and re-arm \
             the endpoint without consuming its one-shot; only the exact launched process \
             can claim, and the health deadline is not extended."
        );
        for refusal in [
            WindowsClaimantRefusal::LaunchExited,
            WindowsClaimantRefusal::LaunchIdentityMismatch,
            WindowsClaimantRefusal::Unverifiable {
                phase: "claimant process-object comparison",
                source: io::Error::from_raw_os_error(ERROR_INVALID_HANDLE.cast_signed()),
            },
        ] {
            let text = refusal.to_string();
            assert!(text.starts_with("KELD-RUNTIME-017: "), "{text}");
            assert!(text.contains("re-arm the endpoint"), "{text}");
        }
    }

    #[test]
    fn suspended_child_owns_the_sole_resume_thread_call() {
        // KEL-53 §5 reuse decision: one suspended-child type owns the only
        // primary-thread resume, so a later launch path cannot add a second one.
        // It counts the identifier, not one call spelling: an alias or a
        // qualified path must still name it, so the import and the one call
        // are the only two occurrences.
        let needle = concat!("Resume", "Thread");
        let mut calls = Vec::new();
        let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).expect("list a source directory") {
                let path = entry.expect("read a source entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    let source = std::fs::read_to_string(&path).expect("read a source file");
                    let count = source.matches(needle).count();
                    if count != 0 {
                        let name = path.file_name().expect("file name").to_string_lossy();
                        calls.push((name.into_owned(), count));
                    }
                }
            }
        }
        assert_eq!(calls, vec![("windows_lpac.rs".to_owned(), 2)]);
    }

    #[test]
    fn dll_search_refusal_names_its_code_cause_and_exit_guidance() {
        let error = WindowsDllSearchError {
            source: io::Error::from_raw_os_error(87),
        };
        let rendered = error.to_string();
        assert!(
            rendered.starts_with("KELD-RUNTIME-018: restricting this process's DLL search"),
            "{rendered}"
        );
        assert!(rendered.contains("(os error 87)"), "{rendered}");
        assert!(
            rendered.contains("Exit before loading any library"),
            "{rendered}"
        );
        assert_eq!(
            std::error::Error::source(&error)
                .and_then(|source| source.downcast_ref::<io::Error>())
                .and_then(io::Error::raw_os_error),
            Some(87)
        );
    }

    #[test]
    fn runas_outcome_separates_decline_failure_and_a_missing_process_handle() {
        // Only compared, never used as a handle.
        let returned: HANDLE = std::ptr::without_provenance_mut(0x1234);
        let cancelled = || io::Error::from_raw_os_error(ERROR_CANCELLED.cast_signed());
        assert!(matches!(
            runas_outcome(false, cancelled(), 5, std::ptr::null_mut()),
            Err(WindowsUpdaterHelperLaunchError::Declined)
        ));
        match runas_outcome(
            false,
            io::Error::from_raw_os_error(2),
            2,
            std::ptr::null_mut(),
        ) {
            Err(WindowsUpdaterHelperLaunchError::ShellExecute { source, inst_app }) => {
                assert_eq!((source.raw_os_error(), inst_app), (Some(2), 2));
            }
            other => panic!("expected a ShellExecute failure, got {other:?}"),
        }
        // A failed call is a failure even if a handle value is present.
        assert!(matches!(
            runas_outcome(false, io::Error::from_raw_os_error(5), 5, returned),
            Err(WindowsUpdaterHelperLaunchError::ShellExecute { .. })
        ));
        assert!(matches!(
            runas_outcome(true, cancelled(), 42, std::ptr::null_mut()),
            Err(WindowsUpdaterHelperLaunchError::NoProcessHandle)
        ));
        assert_eq!(
            runas_outcome(true, cancelled(), 42, returned).expect("a success with a handle"),
            returned
        );
    }

    #[test]
    fn launched_process_is_retained_only_with_the_id_its_handle_reports() {
        // SAFETY: OpenProcess dereferences no caller memory.
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, std::process::id()) };
        assert!(!raw.is_null(), "open this test process");
        // SAFETY: OpenProcess succeeded, so this is the handle's sole owner.
        let process = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
        let helper =
            WindowsElevatedUpdaterHelper::identify(process).expect("a process handle has an ID");
        assert_eq!(helper.id(), std::process::id());

        let not_a_process = OwnedHandle::from(std::fs::File::open("NUL").expect("open NUL"));
        match WindowsElevatedUpdaterHelper::identify(not_a_process) {
            Err(WindowsUpdaterHelperLaunchError::ProcessIdentity { source }) => {
                assert!(source.raw_os_error().is_some(), "{source}");
            }
            other => panic!("a file handle has no process ID, got {other:?}"),
        }
    }

    /// Enters COM's multithreaded apartment directly, as an independent probe of
    /// the thread's COM state, and returns the raw `HRESULT`.
    fn enter_multithreaded_apartment() -> i32 {
        // SAFETY: the reserved argument is null and the flag is a documented
        // COINIT value; the caller balances every success on this thread.
        unsafe {
            CoInitializeEx(
                std::ptr::null(),
                windows_sys::Win32::System::Com::COINIT_MULTITHREADED.cast_unsigned(),
            )
        }
    }

    #[test]
    fn launch_apartment_is_left_on_its_own_thread_when_dropped() {
        std::thread::spawn(|| {
            let apartment = ComApartment::enter().expect("a fresh thread enters an STA");
            drop(apartment);
            // Microsoft: once COM is uninitialized on a thread it may be
            // reinitialized in any mode. While the STA were still entered, this
            // multithreaded request would fail with RPC_E_CHANGED_MODE.
            assert_eq!(enter_multithreaded_apartment(), S_OK);
            // SAFETY: balances the successful probe above on this thread.
            unsafe { CoUninitialize() };
        })
        .join()
        .expect("apartment thread");
    }

    #[test]
    fn launch_apartment_reentered_on_an_sta_thread_balances_only_its_own_entry() {
        std::thread::spawn(|| {
            // SAFETY: the reserved argument is null and the flags are documented
            // COINIT values; the success is balanced below on this thread.
            let outer = unsafe {
                CoInitializeEx(
                    std::ptr::null(),
                    (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).cast_unsigned(),
                )
            };
            assert_eq!(outer, S_OK);
            // Microsoft: a repeated compatible call returns S_FALSE and still
            // needs its own CoUninitialize.
            let apartment = ComApartment::enter().expect("S_FALSE is an entered apartment");
            drop(apartment);
            // The outer entry is still open, so the thread is still an STA.
            assert_eq!(
                enter_multithreaded_apartment(),
                windows_sys::Win32::Foundation::RPC_E_CHANGED_MODE
            );
            // SAFETY: balances the outer entry on this thread.
            unsafe { CoUninitialize() };
            assert_eq!(enter_multithreaded_apartment(), S_OK);
            // SAFETY: balances the successful probe above on this thread.
            unsafe { CoUninitialize() };
        })
        .join()
        .expect("apartment thread");
    }

    #[test]
    fn launch_apartment_refuses_a_thread_in_another_apartment_and_leaves_it_entered() {
        std::thread::spawn(|| {
            assert_eq!(enter_multithreaded_apartment(), S_OK);
            match ComApartment::enter() {
                Err(WindowsUpdaterHelperLaunchError::ComInitialization { hresult }) => {
                    assert_eq!(hresult, windows_sys::Win32::Foundation::RPC_E_CHANGED_MODE);
                }
                other => panic!("expected RPC_E_CHANGED_MODE, got {other:?}"),
            }
            // The refusal made no CoUninitialize: the thread's own MTA is still
            // entered, so a repeated request reports S_FALSE, not S_OK.
            assert_eq!(enter_multithreaded_apartment(), S_FALSE);
            // SAFETY: balances the two successful probes on this thread.
            unsafe {
                CoUninitialize();
                CoUninitialize();
            }
        })
        .join()
        .expect("apartment thread");
    }
}
