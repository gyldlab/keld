//! Validated host-liveness pipe and guardian shutdown-control protocol.
//! Generation control and process-group supervision remain parent-owned.

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
use nix::sys::stat::{SFlag, fstat};

use super::{
    GENERATION_CONTROL_POLL, SUPERVISED_ORDERLY_ACK, SUPERVISED_ORDERLY_SHUTDOWN,
    SUPERVISED_QUIT_ACCEPTED, SUPERVISED_QUIT_ACK, Supervisor, lifecycle_error, nix_io_error,
};
use crate::RuntimeError;

pub(super) fn validate_liveness_bootstrap(
    reader: &mut (impl Read + AsFd),
) -> Result<(), RuntimeError> {
    let stat = fstat(reader.as_fd()).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor validation",
            io::Error::from_raw_os_error(source as i32),
        )
    })?;
    let kind = SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT;
    if kind != SFlag::S_IFIFO {
        return Err(lifecycle_error(
            "macOS guardian liveness descriptor validation",
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "host-liveness descriptor is not a pipe",
            ),
        ));
    }

    let descriptor_flags = fcntl(reader.as_fd(), FcntlArg::F_GETFD).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor flags",
            nix_io_error(source),
        )
    })?;
    fcntl(
        reader.as_fd(),
        FcntlArg::F_SETFD(FdFlag::from_bits_truncate(descriptor_flags) | FdFlag::FD_CLOEXEC),
    )
    .map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor isolation",
            nix_io_error(source),
        )
    })?;

    let raw_flags = fcntl(reader.as_fd(), FcntlArg::F_GETFL).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor flags",
            nix_io_error(source),
        )
    })?;
    let flags = OFlag::from_bits_truncate(raw_flags);
    fcntl(reader.as_fd(), FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor probe",
            nix_io_error(source),
        )
    })?;
    let mut byte = [0_u8; 1];
    let probe = reader.read(&mut byte);
    let restore = fcntl(
        reader.as_fd(),
        FcntlArg::F_SETFL(flags & !OFlag::O_NONBLOCK),
    )
    .map_err(nix_io_error);
    if let Err(source) = restore {
        return Err(lifecycle_error(
            "macOS guardian liveness descriptor restore",
            source,
        ));
    }
    match probe {
        Err(source) if source.kind() == io::ErrorKind::WouldBlock => Ok(()),
        Ok(0) => Err(lifecycle_error(
            "macOS guardian liveness descriptor validation",
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "host-liveness pipe has no live writer",
            ),
        )),
        Ok(_) => Err(lifecycle_error(
            "macOS guardian liveness descriptor validation",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "host-liveness pipe carried data instead of EOF",
            ),
        )),
        Err(source) => Err(lifecycle_error(
            "macOS guardian liveness descriptor validation",
            source,
        )),
    }
}

pub(super) fn await_host_death(reader: &mut impl Read) -> Result<(), RuntimeError> {
    let mut byte = [0_u8; 1];
    match reader.read(&mut byte) {
        Ok(0) => Ok(()),
        Ok(_) => Err(lifecycle_error(
            "macOS guardian host-liveness read",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "host-liveness pipe carried data instead of EOF",
            ),
        )),
        Err(source) => Err(lifecycle_error("macOS guardian host-liveness read", source)),
    }
}

enum HostLiveness {
    Live,
    Dead,
    QuitAccepted,
    OrderlyShutdown,
}

pub(super) fn set_liveness_nonblocking(
    reader: &impl AsFd,
    enabled: bool,
) -> Result<(), RuntimeError> {
    let raw_flags = fcntl(reader.as_fd(), FcntlArg::F_GETFL).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor flags",
            nix_io_error(source),
        )
    })?;
    let mut flags = OFlag::from_bits_truncate(raw_flags);
    flags.set(OFlag::O_NONBLOCK, enabled);
    fcntl(reader.as_fd(), FcntlArg::F_SETFL(flags)).map_err(|source| {
        lifecycle_error(
            "macOS guardian liveness descriptor polling",
            nix_io_error(source),
        )
    })?;
    Ok(())
}

fn poll_host_liveness(reader: &mut impl Read) -> Result<HostLiveness, RuntimeError> {
    let mut byte = [0_u8; 1];
    match reader.read(&mut byte) {
        Ok(0) => Ok(HostLiveness::Dead),
        Err(source)
            if matches!(
                source.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(HostLiveness::Live)
        }
        Ok(1) if byte[0] == SUPERVISED_QUIT_ACCEPTED => Ok(HostLiveness::QuitAccepted),
        Ok(1) if byte[0] == SUPERVISED_ORDERLY_SHUTDOWN => Ok(HostLiveness::OrderlyShutdown),
        Ok(_) => Err(lifecycle_error(
            "macOS guardian host-liveness read",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "host-liveness pipe carried an unknown supervised control byte",
            ),
        )),
        Err(source) => Err(lifecycle_error("macOS guardian host-liveness read", source)),
    }
}

pub(super) fn observe_supervised_liveness(
    reader: &mut impl Read,
    supervisor: &Supervisor,
    quit_accepted: &mut bool,
    quit_ack: &mut impl Write,
) -> Result<bool, RuntimeError> {
    match poll_host_liveness(reader)? {
        HostLiveness::Live => Ok(false),
        HostLiveness::Dead => {
            supervisor.shutdown();
            Ok(true)
        }
        HostLiveness::QuitAccepted if *quit_accepted => Err(lifecycle_error(
            "macOS guardian supervised-Quit control",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "duplicate accepted-Quit control byte",
            ),
        )),
        HostLiveness::QuitAccepted => {
            *quit_accepted = true;
            supervisor.accept_shutdown();
            quit_ack
                .write_all(&SUPERVISED_QUIT_ACK)
                .and_then(|()| quit_ack.flush())
                .map_err(|source| {
                    lifecycle_error("macOS guardian supervised-Quit acknowledgment", source)
                })?;
            Ok(false)
        }
        HostLiveness::OrderlyShutdown => Err(lifecycle_error(
            "macOS guardian supervised host-liveness control",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "orderly-shutdown control is unavailable on the one-generation API",
            ),
        )),
    }
}

pub(super) fn observe_guarded_liveness(
    reader: &mut impl Read,
    supervisor: &Supervisor,
    quit_accepted: &mut bool,
    orderly_requested: &mut bool,
    acknowledgment: &mut impl Write,
    preparation_cancelled: &AtomicBool,
) -> Result<bool, RuntimeError> {
    match poll_host_liveness(reader)? {
        HostLiveness::Live => Ok(false),
        HostLiveness::Dead => {
            supervisor.shutdown();
            preparation_cancelled.store(true, Ordering::Release);
            Ok(true)
        }
        HostLiveness::QuitAccepted if *quit_accepted || *orderly_requested => Err(lifecycle_error(
            "macOS guardian accepted-shutdown control",
            io::Error::new(
                io::ErrorKind::InvalidData,
                "shutdown attribution was already selected",
            ),
        )),
        HostLiveness::QuitAccepted => {
            *quit_accepted = true;
            supervisor.accept_shutdown();
            preparation_cancelled.store(true, Ordering::Release);
            acknowledgment
                .write_all(&SUPERVISED_QUIT_ACK)
                .and_then(|()| acknowledgment.flush())
                .map_err(|source| {
                    lifecycle_error("macOS guardian supervised-Quit acknowledgment", source)
                })?;
            Ok(false)
        }
        HostLiveness::OrderlyShutdown if *quit_accepted || *orderly_requested => {
            Err(lifecycle_error(
                "macOS guardian orderly-shutdown control",
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "shutdown attribution was already selected",
                ),
            ))
        }
        HostLiveness::OrderlyShutdown => {
            *orderly_requested = true;
            acknowledgment
                .write_all(&SUPERVISED_ORDERLY_ACK)
                .and_then(|()| acknowledgment.flush())
                .map_err(|source| {
                    lifecycle_error("macOS guardian orderly-shutdown acknowledgment", source)
                })?;
            Ok(false)
        }
    }
}

/// The only successful early-shutdown acknowledgment is an actual
/// authenticated host S followed by bounded EOF. Sharing this owner keeps
/// the ordinary pre-generation and failed-initial-preparation paths identical.
fn acknowledge_pre_generation_orderly_shutdown(
    liveness: &mut impl Read,
    acknowledgment: &mut impl Write,
) -> Result<(), RuntimeError> {
    acknowledgment
        .write_all(&SUPERVISED_ORDERLY_ACK)
        .and_then(|()| acknowledgment.flush())
        .map_err(|source| {
            lifecycle_error(
                "macOS guardian pre-generation orderly acknowledgment",
                source,
            )
        })?;
    let deadline = std::time::Instant::now() + super::GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        match poll_host_liveness(liveness)? {
            HostLiveness::Dead => return Ok(()),
            HostLiveness::Live if std::time::Instant::now() < deadline => {
                thread::yield_now();
            }
            HostLiveness::Live => {
                return Err(lifecycle_error(
                    "macOS guardian pre-generation host EOF deadline",
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "host retained liveness after acknowledged initial shutdown",
                    ),
                ));
            }
            _ => {
                return Err(lifecycle_error(
                    "macOS guardian pre-generation shutdown protocol",
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "duplicate or conflicting host shutdown control",
                    ),
                ));
            }
        }
    }
}

/// A host may authenticate KGC1 and immediately request orderly shutdown
/// before the Guardian has created its first Supervisor. Consume that exact
/// host-only S byte before calling `start_prepared`: otherwise an intentional
/// recovery denial can abort PREPARE and suppress KSA. No Bun generation
/// exists on this path; no app-link or group authority is minted.
pub(super) fn observe_pre_generation_shutdown(
    liveness: &mut (impl Read + AsFd),
    acknowledgment: &mut impl Write,
) -> Result<bool, RuntimeError> {
    set_liveness_nonblocking(liveness, true)?;
    match poll_host_liveness(liveness)? {
        HostLiveness::Live => Ok(false),
        HostLiveness::Dead => Ok(true),
        HostLiveness::OrderlyShutdown => {
            acknowledge_pre_generation_orderly_shutdown(liveness, acknowledgment)?;
            Ok(true)
        }
        HostLiveness::QuitAccepted => Err(lifecycle_error(
            "macOS guardian pre-generation shutdown protocol",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "accepted Quit before the first authorized role generation",
            ),
        )),
    }
}

/// Startup can fail after the last liveness probe and before the first
/// authorized process was spawned. An authenticated host may have sent S
/// concurrently with its permanent recovery denial. Accept only that exact
/// host request, never a bare EOF or a failed generation-control exchange as
/// evidence of orderly shutdown. The caller retains its original startup
/// error whenever no valid S was seen.
pub(super) fn observe_failed_initial_preparation_shutdown(
    liveness: &mut (impl Read + AsFd),
    acknowledgment: &mut impl Write,
) -> Result<bool, RuntimeError> {
    set_liveness_nonblocking(liveness, true)?;
    match poll_host_liveness(liveness)? {
        HostLiveness::OrderlyShutdown => {
            acknowledge_pre_generation_orderly_shutdown(liveness, acknowledgment)?;
            Ok(true)
        }
        HostLiveness::Live | HostLiveness::Dead => Ok(false),
        HostLiveness::QuitAccepted => Err(lifecycle_error(
            "macOS guardian pre-generation shutdown protocol",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "accepted Quit before the first authorized role generation",
            ),
        )),
    }
}

pub(super) fn await_guarded_primary_liveness(
    liveness: &mut (impl Read + AsFd),
    supervisor: &Supervisor,
    quit_ack: &mut impl Write,
    host_unavailable: &AtomicBool,
    preparation_cancelled: &AtomicBool,
) -> (Result<(), RuntimeError>, bool, bool) {
    let mut liveness_result = Ok(());
    let mut quit_accepted = false;
    let mut orderly_requested = false;
    let mut host_dead = false;
    loop {
        match observe_guarded_liveness(
            liveness,
            supervisor,
            &mut quit_accepted,
            &mut orderly_requested,
            quit_ack,
            preparation_cancelled,
        ) {
            Ok(true) => {
                host_dead = true;
                if !quit_accepted && !orderly_requested {
                    host_unavailable.store(true, Ordering::Release);
                }
                supervisor.shutdown();
                break;
            }
            Ok(false) => {}
            Err(error) => {
                liveness_result = Err(error);
                // A liveness fault interrupts a pending read as a fatal error,
                // never as the privately attributed stop marker.
                host_unavailable.store(true, Ordering::Release);
                supervisor.shutdown();
                break;
            }
        }
        match supervisor.recv_event(GENERATION_CONTROL_POLL) {
            Some(crate::SupervisorEvent::Stopped) if quit_accepted => {
                // Supervisor stop does not terminate this persistent guardian.
                // Core may still own real native retirement before host EOF.
                if let Err(error) = set_liveness_nonblocking(liveness, false) {
                    liveness_result = Err(error);
                    break;
                }
            }
            Some(
                crate::SupervisorEvent::RespawnFailed
                | crate::SupervisorEvent::Failed { .. }
                | crate::SupervisorEvent::CrashLoopTripped
                | crate::SupervisorEvent::Stopped,
            ) => break,
            None => thread::yield_now(),
            Some(_) => {}
        }
    }

    (liveness_result, quit_accepted, host_dead)
}

pub(super) fn require_close_on_exec(writer: &impl AsFd) -> Result<(), RuntimeError> {
    let raw = fcntl(writer.as_fd(), FcntlArg::F_GETFD).map_err(|source| {
        lifecycle_error(
            "macOS guardian host-liveness writer flags",
            nix_io_error(source),
        )
    })?;
    let flags = FdFlag::from_bits_truncate(raw);
    if flags.contains(FdFlag::FD_CLOEXEC) {
        Ok(())
    } else {
        Err(lifecycle_error(
            "macOS guardian host-liveness writer validation",
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "host-liveness writer is inheritable; require FD_CLOEXEC",
            ),
        ))
    }
}
