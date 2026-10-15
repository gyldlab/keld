//! Validated host-liveness pipe and guardian shutdown-control protocol.
//! Generation control and process-group supervision remain parent-owned.

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicBool, Ordering};

use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
use nix::sys::stat::{SFlag, fstat};

use super::{
    SUPERVISED_ORDERLY_ACK, SUPERVISED_ORDERLY_SHUTDOWN, SUPERVISED_QUIT_ACCEPTED,
    SUPERVISED_QUIT_ACK, Supervisor, lifecycle_error, nix_io_error,
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
