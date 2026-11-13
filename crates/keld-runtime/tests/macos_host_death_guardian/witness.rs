//! Independent EOF witness with its retained descriptor lifetime.
use super::{
    BufRead, BufReader, EVENT_DEADLINE, Instant, NEXT_PATH_ID, Ordering, Path, PathBuf, Read,
    UnixListener, UnixStream, Write, fs, thread,
};

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
        stream
            .set_read_timeout(Some(EVENT_DEADLINE))
            .expect("bound EOF witness read");
        let mut line = String::new();
        BufReader::new(&mut *stream)
            .read_line(&mut line)
            .expect("read independently witnessed EOF");
        assert_eq!(line, "EOF_OBSERVED\n", "unexpected EOF witness record");
        stream
            .set_read_timeout(None)
            .expect("clear EOF witness read deadline");
    }

    pub(super) fn release_link_eof_report(&mut self) {
        self.stream
            .as_mut()
            .expect("connected EOF witness")
            .write_all(b"R")
            .expect("release peer LINK_EOF report");
    }
}

impl Drop for EofWitness {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
