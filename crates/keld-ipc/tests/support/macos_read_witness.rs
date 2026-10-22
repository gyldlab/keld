//! Test-only native Mach ABI observer of this process's own macOS ARM64 reader.
//!
//! One responsibility: freeze an owned reader and inspect its saved BSD
//! socket-receive syscall. No production callsite or external process port.

#![allow(
    unsafe_code,
    reason = "test-only same-process Mach IPC and memory-safe syscall inspection"
)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::c_int;
use std::thread;
use std::time::Instant;

const ARM_THREAD_STATE64: c_int = 6;
const THREAD_BASIC_INFO: c_int = 3;
const TH_STATE_WAITING: i32 = 3;
// Rust's UnixStream::read enters Darwin recvfrom (SDK sys/syscall.h).
const BSD_RECVFROM: u64 = 29;
const BSD_RECVFROM_NOCANCEL: u64 = 403;
const SVC_0X80: u32 = 0xd400_1001;

// The ABI comes from the macOS SDK mach/arm/thread_status.h and
// mach/thread_info.h, exercised only by this integration test executable.
unsafe extern "C" {
    fn mach_thread_self() -> u32;
    fn mach_task_self() -> u32;
    fn mach_port_deallocate(task: u32, name: u32) -> c_int;
    fn thread_suspend(thread: u32) -> c_int;
    fn thread_resume(thread: u32) -> c_int;
    fn thread_get_state(thread: u32, flavor: c_int, state: *mut u32, count: *mut u32) -> c_int;
    fn thread_info(thread: u32, flavor: c_int, info: *mut i32, count: *mut u32) -> c_int;
    fn mach_vm_read_overwrite(
        task: u32,
        address: u64,
        size: u64,
        destination: u64,
        out_size: *mut u64,
    ) -> c_int;
}

/// Mint an owned Mach right to this current worker from inside the worker.
pub(super) fn owned_current_reader_port() -> OwnedReaderPort {
    // SAFETY: mach_thread_self creates a send right for the calling thread in
    // the same test child; transfer owns the right and Drop deallocates it.
    OwnedReaderPort(unsafe { mach_thread_self() })
}

#[repr(C)]
#[derive(Default)]
struct ArmThreadState64 {
    x: [u64; 29],
    fp: u64,
    lr: u64,
    sp: u64,
    pc: u64,
    cpsr: u32,
    flags: u32,
}

/// Controller owns the send right created by its own worker thread.
pub(super) struct OwnedReaderPort(u32);

impl Drop for OwnedReaderPort {
    fn drop(&mut self) {
        // SAFETY: the child owns this send right returned by mach_thread_self,
        // and the task port is the same process; child reaping is the fallback.
        let _ = unsafe { mach_port_deallocate(mach_task_self(), self.0) };
    }
}

/// A Mach suspension freezes user instructions across the shutdown syscall.
/// The SDK `thread_act.defs` promises this after `thread_suspend` returns.
pub(super) struct FrozenReader<'a> {
    port: &'a OwnedReaderPort,
    resumed: bool,
}

impl<'a> FrozenReader<'a> {
    pub(super) fn suspend(port: &'a OwnedReaderPort) -> Result<Self, c_int> {
        // SAFETY: our test child owns an exact reader Mach send right.
        let status = unsafe { thread_suspend(port.0) };
        if status == 0 {
            Ok(Self {
                port,
                resumed: false,
            })
        } else {
            Err(status)
        }
    }

    pub(super) fn resume(&mut self) -> Result<(), c_int> {
        // SAFETY: matching one successful suspension of this exact reader.
        let status = unsafe { thread_resume(self.port.0) };
        if status == 0 {
            self.resumed = true;
            Ok(())
        } else {
            Err(status)
        }
    }

    pub(super) fn is_native_blocked_read(&self, fd: i32) -> Result<bool, c_int> {
        let mut state = ArmThreadState64::default();
        let mut words = u32::try_from(std::mem::size_of::<ArmThreadState64>() / 4)
            .expect("fixed Mach ABI size");
        // SAFETY: the suspended reader cannot execute another user
        // instruction; this is an aligned 272-byte ARM64 SDK state buffer.
        let status = unsafe {
            thread_get_state(
                self.port.0,
                ARM_THREAD_STATE64,
                (&raw mut state).cast::<u32>(),
                &raw mut words,
            )
        };
        if status != 0 {
            return Err(status);
        }
        assert_eq!(words, 68, "ARM64 state ABI");
        let mut info = [0i32; 32];
        let mut info_words = u32::try_from(info.len()).expect("small info buffer");
        // SAFETY: SDK basic info is exactly ten i32 words and the buffer
        // provides capacity for 32; port belongs to this child process.
        let status = unsafe {
            thread_info(
                self.port.0,
                THREAD_BASIC_INFO,
                info.as_mut_ptr(),
                &raw mut info_words,
            )
        };
        if status != 0 {
            return Err(status);
        }
        assert_eq!(info_words, 10, "Darwin basic thread info ABI");
        if state.pc < 4 || state.pc % 4 != 0 {
            return Ok(false);
        }
        let mut preceding = 0u32;
        let mut bytes_read = 0u64;
        let destination =
            u64::try_from((&raw mut preceding).addr()).expect("64-bit Mach process address");
        // SAFETY: the child owns the source task, the destination points to
        // an aligned live four-byte u32, and Darwin mach_vm_read_overwrite
        // validates the saved PC range in the kernel before copying. Unmapped
        // or inaccessible PC-4 returns a typed Mach error instead of a fault.
        let vm_status = unsafe {
            mach_vm_read_overwrite(
                mach_task_self(),
                state.pc - 4,
                4,
                destination,
                &raw mut bytes_read,
            )
        };
        if vm_status != 0 {
            return Err(vm_status);
        }
        if bytes_read != 4 {
            return Ok(false);
        }
        Ok(preceding == SVC_0X80
            && matches!(state.x[16], BSD_RECVFROM | BSD_RECVFROM_NOCANCEL)
            && state.x[0] == u64::try_from(fd).expect("nonnegative owned FD")
            && info[6] == TH_STATE_WAITING
            && info[8] == 1)
    }
}

impl Drop for FrozenReader<'_> {
    fn drop(&mut self) {
        if !self.resumed {
            // SAFETY: only this owner suspended the reader and retained its
            // Mach right. Resume before emergency socket cleanup.
            let status = unsafe { thread_resume(self.port.0) };
            if status != 0 {
                eprintln!("KIPC_NATIVE_THREAD_RESUME_ERROR={status}");
            }
        }
    }
}

/// The same original two-second startup window. Poll actual kernel state;
/// the test never accepts a callback immediately before the read syscall.
pub(super) fn freeze_inside_kernel_read(
    port: &OwnedReaderPort,
    reader_fd: i32,
    deadline: Instant,
) -> Result<Option<FrozenReader<'_>>, c_int> {
    while Instant::now() < deadline {
        let stopped = FrozenReader::suspend(port)?;
        // If inspection fails, stopped is dropped and its owned suspension
        // is released before the caller performs the peer shutdown cleanup.
        if stopped.is_native_blocked_read(reader_fd)? && Instant::now() < deadline {
            return Ok(Some(stopped));
        }
        drop(stopped);
        thread::yield_now();
    }
    Ok(None)
}
