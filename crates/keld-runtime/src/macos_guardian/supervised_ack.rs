//! Private supervised guardian control acknowledgments. No OS authority is minted here.
use std::io::{self, Read};
use std::os::fd::AsFd;
use std::process::ChildStdout;
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, OFlag, fcntl};

use super::{RuntimeError, lifecycle_error, nix_io_error};

pub(super) const SUPERVISED_QUIT_ACCEPTED: u8 = b'Q';
pub(super) const SUPERVISED_QUIT_ACK: [u8; 3] = *b"KQA";
pub(super) const SUPERVISED_ORDERLY_SHUTDOWN: u8 = b'S';
pub(super) const SUPERVISED_ORDERLY_ACK: [u8; 3] = *b"KSA";
pub(super) const SUPERVISED_QUIT_ACK_DEADLINE: Duration = Duration::from_secs(5);

pub(super) fn read_supervised_quit_ack(reader: &mut ChildStdout) -> Result<(), RuntimeError> {
    read_supervised_ack(
        reader,
        SUPERVISED_QUIT_ACK,
        "macOS guardian supervised-Quit acknowledgment",
        "accepted Quit",
    )
}

pub(super) fn read_supervised_orderly_ack(reader: &mut ChildStdout) -> Result<(), RuntimeError> {
    read_supervised_ack(
        reader,
        SUPERVISED_ORDERLY_ACK,
        "macOS guardian orderly-shutdown acknowledgment",
        "orderly shutdown",
    )
}

fn read_supervised_ack(
    reader: &mut ChildStdout,
    expected: [u8; 3],
    phase: &'static str,
    action: &'static str,
) -> Result<(), RuntimeError> {
    read_supervised_ack_until(
        reader,
        expected,
        phase,
        action,
        Instant::now() + SUPERVISED_QUIT_ACK_DEADLINE,
    )
}

// Shared ordinary/negative native-ack contract; a supplied deadline lets the
// regression prove expired buffered bytes without a test synchronization sleep.
fn read_supervised_ack_until(
    reader: &mut (impl Read + AsFd),
    expected: [u8; 3],
    phase: &'static str,
    action: &'static str,
    deadline: Instant,
) -> Result<(), RuntimeError> {
    let raw_flags = fcntl(reader.as_fd(), FcntlArg::F_GETFL)
        .map_err(|source| lifecycle_error(phase, nix_io_error(source)))?;
    fcntl(
        reader.as_fd(),
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(raw_flags) | OFlag::O_NONBLOCK),
    )
    .map_err(|source| lifecycle_error(phase, nix_io_error(source)))?;
    let mut ack = [0_u8; 3];
    let mut filled = 0;
    while filled < ack.len() {
        // A valid byte received *after* the retained deadline cannot make a
        // terminal transition successful. This also covers a buffered ACK
        // read by a host thread resumed after its scheduling deadline.
        if Instant::now() >= deadline {
            return Err(ack_timed_out(phase, action));
        }
        match reader.read(&mut ack[filled..]) {
            Ok(0) => {
                return Err(lifecycle_error(
                    phase,
                    io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        format!("guardian exited before acknowledging {action}"),
                    ),
                ));
            }
            Ok(read) => filled += read,
            Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
            Err(source) if source.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(ack_timed_out(phase, action));
                }
                std::thread::yield_now();
            }
            Err(source) => {
                return Err(lifecycle_error(phase, source));
            }
        }
    }
    // Check the final successful read as well: it can return after the
    // deadline even when the poll before read still had time remaining.
    if Instant::now() >= deadline {
        return Err(ack_timed_out(phase, action));
    }
    if ack == expected {
        Ok(())
    } else {
        Err(lifecycle_error(
            phase,
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("guardian returned an invalid {action} acknowledgment"),
            ),
        ))
    }
}

fn ack_timed_out(phase: &'static str, action: &'static str) -> RuntimeError {
    lifecycle_error(
        phase,
        io::Error::new(
            io::ErrorKind::TimedOut,
            format!("guardian did not acknowledge {action} before the deadline"),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::{SUPERVISED_ORDERLY_ACK, SUPERVISED_QUIT_ACK, read_supervised_ack_until};
    use crate::RuntimeError;
    use std::io::{self, Write as _};
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    #[test]
    fn expired_buffered_supervised_ack_cannot_authorize_quit_or_shutdown() {
        for ack in [SUPERVISED_QUIT_ACK, SUPERVISED_ORDERLY_ACK] {
            let (mut reader, mut writer) =
                UnixStream::pair().expect("real native guardian acknowledgment pipe");
            writer
                .write_all(&ack)
                .expect("buffered genuine acknowledgment");
            let expired = Instant::now()
                .checked_sub(Duration::from_millis(1))
                .expect("monotonic clock supports past native deadline");
            let refusal = read_supervised_ack_until(
                &mut reader,
                ack,
                "test genuine guardian terminal acknowledgment",
                "accepted Quit or shutdown",
                expired,
            )
            .expect_err("genuine but stale buffered KQA/KSA may not authorize success");
            assert!(
                matches!(refusal, RuntimeError::Lifecycle { source, .. }
                    if source.kind() == io::ErrorKind::TimedOut),
                "expired buffered ACK must return a typed timeout"
            );
        }
    }

    #[test]
    fn wrong_supervised_ack_kind_cannot_authorize_the_other_terminal_transition() {
        // Both KQA and KSA are valid on their own channels; swapping them
        // must never make the wrong authenticated terminal transition succeed.
        for (expected, wrong) in [
            (SUPERVISED_QUIT_ACK, SUPERVISED_ORDERLY_ACK),
            (SUPERVISED_ORDERLY_ACK, SUPERVISED_QUIT_ACK),
        ] {
            let (mut reader, mut writer) =
                UnixStream::pair().expect("real native guardian acknowledgment pipe");
            writer
                .write_all(&wrong)
                .expect("crossed terminal acknowledgment");
            let refusal = read_supervised_ack_until(
                &mut reader,
                expected,
                "test guardian terminal acknowledgment",
                "accepted Quit or shutdown",
                Instant::now() + Duration::from_secs(1),
            )
            .expect_err("cross-terminal KQA/KSA must fail closed");
            assert!(
                matches!(refusal, RuntimeError::Lifecycle { source, .. }
                    if source.kind() == io::ErrorKind::InvalidData),
                "cross-terminal ACK must return typed InvalidData"
            );
        }
    }

    #[test]
    fn native_supervised_ack_bytes_are_exact_when_within_deadline() {
        for ack in [SUPERVISED_QUIT_ACK, SUPERVISED_ORDERLY_ACK] {
            let (mut reader, mut writer) =
                UnixStream::pair().expect("real native guardian acknowledgment pipe");
            writer
                .write_all(&ack)
                .expect("complete native acknowledgment");
            read_supervised_ack_until(
                &mut reader,
                ack,
                "test genuine guardian terminal acknowledgment",
                "accepted Quit or shutdown",
                Instant::now() + Duration::from_secs(1),
            )
            .expect("real byte-exact acknowledged terminal transition");
        }
    }
}
