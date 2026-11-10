//! Independent EOF witness with its retained descriptor lifetime.
use super::{
    EVENT_DEADLINE, Instant, NEXT_PATH_ID, Ordering, Path, PathBuf, Read, UnixListener, UnixStream,
    Write, fs, thread,
};

const EOF_WITNESS_RECORD: &[u8] = b"EOF_OBSERVED\n";

pub(super) struct EofWitness {
    listener: UnixListener,
    path: PathBuf,
    stream: Option<UnixStream>,
}

impl EofWitness {
    pub(super) fn bind(control: &Path) -> Self {
        let id = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let path = control.with_extension(format!("eof-{id}.sock"));
        let listener = UnixListener::bind(&path).expect("bind EOF witness socket");
        listener
            .set_nonblocking(true)
            .expect("make EOF witness socket nonblocking");
        Self {
            listener,
            path,
            stream: None,
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn accept_before(&mut self, deadline: Instant) {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    self.stream = Some(stream);
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "peer did not connect its EOF witness before the deadline"
                    );
                    thread::yield_now();
                }
                Err(error) => panic!("accept EOF witness: {error}"),
            }
        }
    }

    pub(super) fn assert_no_eof_observed(&mut self) {
        let stream = self.stream.as_mut().expect("connected EOF witness");
        stream
            .set_nonblocking(true)
            .expect("make EOF witness read nonblocking");
        let mut byte = [0_u8; 1];
        match stream.read(&mut byte) {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Ok(0) => panic!("EOF witness peer exited before observing app-link EOF"),
            Ok(_) => panic!("leaked app-link descriptor did not suppress the EOF witness"),
            Err(error) => panic!("read EOF witness negative control: {error}"),
        }
        stream
            .set_nonblocking(false)
            .expect("restore blocking EOF witness read");
    }

    pub(super) fn await_eof_observed(&mut self) {
        let stream = self.stream.as_mut().expect("connected EOF witness");
        read_eof_record_before(stream, Instant::now() + EVENT_DEADLINE)
            .expect("read independently witnessed EOF");
    }

    pub(super) fn release_link_eof_report(&mut self) {
        self.stream
            .as_mut()
            .expect("connected EOF witness")
            .write_all(b"R")
            .expect("release peer LINK_EOF report");
    }
}

// One original deadline covers every byte, including the final newline.
// This uses the same persistent connection as the peer's later release byte;
// reading beyond the frame would consume unrelated protocol data.
fn read_eof_record_before(stream: &mut UnixStream, deadline: Instant) -> std::io::Result<()> {
    read_eof_record_before_with_would_block(stream, deadline, || {})
}

fn read_eof_record_before_with_would_block(
    stream: &mut UnixStream,
    deadline: Instant,
    mut on_would_block: impl FnMut(),
) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    stream.set_nonblocking(true)?;
    let result = (|| {
        let mut line = Vec::with_capacity(EOF_WITNESS_RECORD.len());
        loop {
            if Instant::now() >= deadline {
                return Err(Error::new(
                    ErrorKind::TimedOut,
                    "EOF witness exceeded phase deadline",
                ));
            }
            let mut byte = [0_u8; 1];
            match stream.read(&mut byte) {
                Ok(1) => {
                    if Instant::now() >= deadline {
                        return Err(Error::new(ErrorKind::TimedOut, "late EOF witness byte"));
                    }
                    if line.len() == EOF_WITNESS_RECORD.len() {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            "EOF witness line too long",
                        ));
                    }
                    line.push(byte[0]);
                    if byte[0] == b'\n' {
                        let text = std::str::from_utf8(&line)
                            .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;
                        return if text.as_bytes() == EOF_WITNESS_RECORD {
                            Ok(())
                        } else {
                            Err(Error::new(
                                ErrorKind::InvalidData,
                                "unexpected EOF witness record",
                            ))
                        };
                    }
                }
                Ok(0) => {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "EOF witness closed before newline",
                    ));
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
                {
                    if error.kind() == ErrorKind::WouldBlock {
                        on_would_block();
                    }
                    thread::yield_now();
                }
                Err(error) => return Err(error),
                Ok(_) => unreachable!("one-byte EOF witness read"),
            }
        }
    })();
    // Restore the expected blocking mode even when framing/timeout fails.
    stream.set_nonblocking(false)?;
    result
}

impl Drop for EofWitness {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn nonblocking_eof_witness_waits_for_observed_eof_and_preserves_release() {
        let fixture = super::super::support::Fixture::new();
        let mut witness = EofWitness::bind(&fixture.control);
        let mut peer = UnixStream::connect(witness.path()).expect("connect real witness peer");
        let (app_link_owner, mut app_link_peer) =
            UnixStream::pair().expect("independent EOF source");
        peer.set_read_timeout(Some(EVENT_DEADLINE))
            .expect("bound release wait");
        app_link_peer
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("bound independent EOF wait");
        witness.accept_before(Instant::now() + EVENT_DEADLINE);
        let stream = witness.stream.as_mut().expect("accepted witness stream");
        stream
            .set_nonblocking(true)
            .expect("force initial nonblocking mode");
        let peer_thread = thread::spawn(move || -> std::io::Result<[u8; 1]> {
            let mut byte = [0_u8; 1];
            assert_eq!(app_link_peer.read(&mut byte)?, 0, "genuine independent EOF");
            peer.write_all(b"EOF_OBSERVED\n")?;
            peer.read_exact(&mut byte)?;
            Ok(byte)
        });
        let mut app_link_owner = Some(app_link_owner);
        let mut observed_would_block = false;
        let deadline = Instant::now() + EVENT_DEADLINE;
        let result = read_eof_record_before_with_would_block(stream, deadline, || {
            // The peer cannot observe EOF or send its record before this actual
            // empty read. No scheduling delay or prebuffered record proves retry.
            observed_would_block = true;
            drop(app_link_owner.take());
        });
        let completed_before_deadline = Instant::now() < deadline;
        drop(app_link_owner);
        if result.is_ok() {
            witness.release_link_eof_report();
        } else {
            // Bound and join the peer even when a negative control rejects early.
            witness.stream.take();
        }
        let peer_result = peer_thread.join().expect("join owned witness peer");
        result.expect("nonblocking witness must wait for the genuine record");
        assert!(
            observed_would_block,
            "reader must encounter the empty stream"
        );
        assert!(
            completed_before_deadline,
            "record must use the original deadline"
        );
        assert_eq!(peer_result.expect("peer receives release"), *b"R");
    }

    #[test]
    fn oversized_unterminated_eof_witness_is_rejected_before_peer_closes() {
        let (mut reader, mut writer) = UnixStream::pair().expect("owned native witness pair");
        writer
            .write_all(&vec![b'X'; EOF_WITNESS_RECORD.len() + 1])
            .expect("write maximum plus one without newline");
        let result =
            read_eof_record_before(&mut reader, Instant::now() + Duration::from_millis(100));
        // Keep the peer alive through the read so EOF and final-string mismatch
        // cannot substitute for the length guard's InvalidData rejection.
        drop(writer);
        assert_eq!(
            result.expect_err("oversized unterminated record").kind(),
            std::io::ErrorKind::InvalidData,
        );
    }

    #[test]
    fn fragmented_eof_witness_cannot_renew_original_deadline() {
        let (mut reader, mut writer) = UnixStream::pair().expect("real native witness pair");
        // The peer drips genuine record bytes well within each individual
        // read timeout but finishes after the single original phase budget.
        let peer = thread::spawn(move || {
            for byte in b"EOF_OBSERVED\n" {
                writer.write_all(&[*byte]).expect("drip witness byte");
                thread::sleep(Duration::from_millis(20));
            }
        });
        let began = Instant::now();
        let result = read_eof_record_before(&mut reader, began + Duration::from_millis(75));
        peer.join().expect("test-owned slow witness peer joined");
        assert_eq!(
            result
                .expect_err("drip cannot renew the phase deadline")
                .kind(),
            std::io::ErrorKind::TimedOut,
        );
    }

    #[test]
    fn native_eof_witness_requires_exact_utf8_newline_and_bounded_length() {
        use std::io::ErrorKind;
        for payload in [
            b"EOF_OBSERVED".as_slice(),
            b"EOF_OBSERVEDX\n".as_slice(),
            b"EOF_\xff\n".as_slice(),
        ] {
            let (mut reader, mut writer) = UnixStream::pair().expect("owned native witness pair");
            writer
                .write_all(payload)
                .expect("write controlled invalid witness");
            drop(writer);
            let error =
                read_eof_record_before(&mut reader, Instant::now() + Duration::from_secs(1))
                    .expect_err("invalid or truncated EOF witness must be refused");
            assert!(
                matches!(
                    error.kind(),
                    ErrorKind::InvalidData | ErrorKind::UnexpectedEof
                ),
                "unexpected invalid-frame reason: {error}"
            );
        }

        let (mut reader, mut writer) = UnixStream::pair().expect("owned valid witness pair");
        writer.write_all(b"EOF_").expect("write first fragment");
        writer
            .write_all(b"OBSERVED\n")
            .expect("write final newline");
        read_eof_record_before(&mut reader, Instant::now() + Duration::from_secs(1))
            .expect("exact fragmented UTF-8 record must succeed");
        reader
            .write_all(b"R")
            .expect("preserve persistent release channel");
        let mut release = [0_u8; 1];
        writer
            .read_exact(&mut release)
            .expect("peer receives release byte");
        assert_eq!(release, *b"R");
    }
}
