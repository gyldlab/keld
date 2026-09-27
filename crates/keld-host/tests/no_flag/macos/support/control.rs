use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::process::Child;
use std::process::Output;
use std::thread;
use std::time::Duration;
use std::time::Instant;

pub(crate) fn accept_before(listener: &UnixListener, deadline: Instant) -> UnixStream {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("normalize accepted fixture control");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "Bun did not connect the fixture control"
                );
                thread::yield_now();
            }
            Err(error) => panic!("accept fixture control: {error}"),
        }
    }
}

pub(crate) fn read_control_line(reader: &mut BufReader<UnixStream>) -> String {
    let mut line = String::new();
    let read = reader.read_line(&mut line).expect("read control line");
    assert_ne!(read, 0, "control EOF before observation");
    line.trim_end().to_owned()
}

pub(crate) fn wait_child_output(child: Child, timeout: Duration) -> Output {
    wait_child_output_observing(child, timeout, || {})
}

pub(crate) fn wait_child_output_observing(
    child: Child,
    timeout: Duration,
    mut observe: impl FnMut(),
) -> Output {
    // Install custody before pipe setup or callbacks can fail. Rust's Child
    // alone does not kill/reap on Drop.
    let mut child = ReapOnDrop(child);
    let mut stdout = PipeCapture::new(child.0.stdout.take());
    let mut stderr = PipeCapture::new(child.0.stderr.take());
    let deadline = Instant::now() + timeout;
    let mut status = None;
    loop {
        // Bound work per stream so continuous output cannot starve observation
        // or the deadline. No reader thread can outlive this wait on unwind.
        stdout.read_once();
        stderr.read_once();
        if status.is_none() {
            status = child.0.try_wait().expect("inspect child exit");
        }
        if let Some(status) = status
            && stdout.pipe.is_none()
            && stderr.pipe.is_none()
        {
            return Output {
                status,
                stdout: stdout.bytes,
                stderr: stderr.bytes,
            };
        }
        if status.is_none() {
            observe();
        }
        assert!(
            Instant::now() < deadline,
            "child exceeded exit/output deadline (status={status:?}, stdout_bytes={}, stderr_bytes={})",
            stdout.bytes.len(),
            stderr.bytes.len(),
        );
        thread::yield_now();
    }
}

struct ReapOnDrop(Child);

impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        // Child caches an observed exit, so these do not signal a reused PID.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct PipeCapture<P> {
    pipe: Option<P>,
    bytes: Vec<u8>,
}

unsafe extern "C" {
    fn fcntl(fd: std::ffi::c_int, command: std::ffi::c_int, ...) -> std::ffi::c_int;
}

impl<P: Read + AsRawFd> PipeCapture<P> {
    fn new(pipe: Option<P>) -> Self {
        if let Some(pipe) = &pipe {
            // Darwin SDK sys/fcntl.h: retain every existing status flag while
            // enabling nonblocking reads for bounded fixture cleanup.
            const F_GETFL: std::ffi::c_int = 3;
            const F_SETFL: std::ffi::c_int = 4;
            const O_NONBLOCK: std::ffi::c_int = 4;
            // SAFETY: pipe owns this live descriptor for the entire borrow;
            // F_GETFL has no variadic argument and transfers no ownership.
            let flags = unsafe { fcntl(pipe.as_raw_fd(), F_GETFL) };
            assert!(
                flags >= 0,
                "read capture flags: {}",
                std::io::Error::last_os_error()
            );
            // SAFETY: same owned descriptor, integer F_SETFL argument from its
            // flags plus Darwin O_NONBLOCK. No pointers or lifetime extension.
            let result = unsafe { fcntl(pipe.as_raw_fd(), F_SETFL, flags | O_NONBLOCK) };
            assert_eq!(
                result,
                0,
                "nonblocking capture: {}",
                std::io::Error::last_os_error()
            );
        }
        Self {
            pipe,
            bytes: Vec::new(),
        }
    }

    fn read_once(&mut self) {
        let Some(pipe) = &mut self.pipe else { return };
        let mut buffer = [0; 8192];
        match pipe.read(&mut buffer) {
            Ok(0) => {
                self.pipe.take();
            }
            Ok(count) => self.bytes.extend_from_slice(&buffer[..count]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => panic!("drain child pipe: {error}"),
        }
    }
}

pub(crate) fn parse_pid(field: Option<&str>, line: &str) -> u32 {
    field
        .unwrap_or_else(|| panic!("missing pid: {line}"))
        .parse()
        .unwrap_or_else(|error| panic!("invalid pid ({error}): {line}"))
}
