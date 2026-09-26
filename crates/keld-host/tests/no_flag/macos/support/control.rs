use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
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
    mut child: Child,
    timeout: Duration,
    mut observe: impl FnMut(),
) -> Output {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = thread::spawn(move || read_child_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_child_pipe(stderr));
    let deadline = Instant::now() + timeout;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().expect("inspect child exit") {
            break (status, false);
        }
        observe();
        if Instant::now() >= deadline {
            let _ = child.kill();
            break (child.wait().expect("wait timed-out child"), true);
        }
        thread::yield_now();
    };
    let output = Output {
        status,
        stdout: stdout_reader.join().expect("stdout reader joins"),
        stderr: stderr_reader.join().expect("stderr reader joins"),
    };
    assert!(
        !timed_out,
        "child exceeded exit deadline (status={})",
        output.status
    );
    output
}

pub(crate) fn read_child_pipe(pipe: Option<impl Read>) -> Vec<u8> {
    let Some(mut pipe) = pipe else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes).expect("drain child pipe");
    bytes
}

pub(crate) fn parse_pid(field: Option<&str>, line: &str) -> u32 {
    field
        .unwrap_or_else(|| panic!("missing pid: {line}"))
        .parse()
        .unwrap_or_else(|error| panic!("invalid pid ({error}): {line}"))
}
