//! One-byte acknowledgment that pins the host's KGR1 group before Bun exec.
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Instant;

use super::GUARDIAN_SHUTDOWN_DEADLINE;

pub(super) const HOST_GROUP_RETAINED_ACK: u8 = b'A';

/// Only used after the Guardian authenticated its private one-use link.
pub(super) struct AwaitHostGroupRetention {
    stream: UnixStream,
}

impl AwaitHostGroupRetention {
    pub(super) const fn new(stream: UnixStream) -> Self {
        Self { stream }
    }
}

impl Write for AwaitHostGroupRetention {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.stream.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()?;
        // This is the only permit to execute the spawned child: a complete
        // record without a retained, authenticated host group is insufficient.
        self.stream.set_nonblocking(true)?;
        read_retention_ack_until(
            &mut self.stream,
            Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE,
        )
    }
}

// Sole normal/hostile reader: the injected deadline permits a no-sleep native
// falsifier and is otherwise always the original shipping registration bound.
fn read_retention_ack_until(stream: &mut UnixStream, deadline: Instant) -> io::Result<()> {
    let mut ack = [0_u8; 1];
    loop {
        // A prebuffered response still cannot grant Bun execution after the
        // retained host group-registration deadline has elapsed.
        if Instant::now() >= deadline {
            return Err(retention_ack_timeout());
        }
        match stream.read(&mut ack) {
            Ok(1) if ack[0] == HOST_GROUP_RETAINED_ACK => {
                // The final byte may return after a scheduler delay even
                // though the prior check was still within the deadline.
                if Instant::now() >= deadline {
                    return Err(retention_ack_timeout());
                }
                return Ok(());
            }
            Ok(1) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "host refused group-retention acknowledgment",
                ));
            }
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "host closed before retaining the Bun group",
                ));
            }
            Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
            Err(source) if source.kind() == io::ErrorKind::WouldBlock => {}
            Err(source) => return Err(source),
            Ok(_) => unreachable!("one-byte group retention acknowledgment"),
        }
        if Instant::now() >= deadline {
            return Err(retention_ack_timeout());
        }
        std::thread::yield_now();
    }
}

fn retention_ack_timeout() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "host did not retain the Bun group before exec deadline",
    )
}

#[cfg(test)]
mod tests {
    use super::{AwaitHostGroupRetention, HOST_GROUP_RETAINED_ACK, read_retention_ack_until};
    use std::io::{self, Write as _};
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    #[test]
    fn expired_buffered_host_retention_ack_cannot_permit_child_exec() {
        let (mut reader, mut writer) = UnixStream::pair().expect("real host registration link");
        writer
            .write_all(&[HOST_GROUP_RETAINED_ACK])
            .expect("authentic buffered one-byte host retention acknowledgment");
        reader
            .set_nonblocking(true)
            .expect("normal shipping nonblocking acknowledgment link");
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .expect("monotonic clock supports prior deadline");
        let refusal = read_retention_ack_until(&mut reader, expired)
            .expect_err("an expired native buffered host acknowledgment cannot permit exec");
        assert_eq!(refusal.kind(), io::ErrorKind::TimedOut, "{refusal}");
    }

    #[test]
    fn timely_host_retention_ack_is_exact_and_malformed_or_eof_refuses() {
        let (reader, mut writer) = UnixStream::pair().expect("real host registration link");
        writer
            .write_all(&[HOST_GROUP_RETAINED_ACK])
            .expect("host-owned exact retention acknowledgment");
        let mut lease = AwaitHostGroupRetention::new(reader);
        lease
            .flush()
            .expect("timely real host ACK permits preexec continuation");

        let (mut reader, mut writer) = UnixStream::pair().expect("independent wrong ACK link");
        writer.write_all(b"X").expect("wrong one-byte ACK");
        reader.set_nonblocking(true).expect("nonblocking link");
        let wrong = read_retention_ack_until(&mut reader, Instant::now() + Duration::from_secs(1))
            .expect_err("wrong host byte must not mint authority");
        assert_eq!(wrong.kind(), io::ErrorKind::InvalidData, "{wrong}");

        let (mut reader, writer) = UnixStream::pair().expect("independent EOF link");
        drop(writer);
        reader.set_nonblocking(true).expect("nonblocking link");
        let eof = read_retention_ack_until(&mut reader, Instant::now() + Duration::from_secs(1))
            .expect_err("absent ACK must not mint authority");
        assert_eq!(eof.kind(), io::ErrorKind::UnexpectedEof, "{eof}");
    }
}
