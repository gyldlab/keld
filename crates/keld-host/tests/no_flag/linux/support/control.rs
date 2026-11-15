//! Independent Linux fixture transcript and child-exit observations.

use std::{
    io::{BufRead as _, BufReader, Read as _},
    os::unix::net::UnixListener,
    process::Child,
    thread,
    time::{Duration, Instant},
};

pub(crate) fn expect_ready_and_echoes(reader: &mut BufReader<std::os::unix::net::UnixStream>) {
    assert_eq!(read_control_line(reader), "READY");
    assert_eq!(read_control_line(reader), "ECHO1");
    assert_eq!(read_control_line(reader), "ECHO2");
}

pub(crate) fn accept_control_or_host_failure(
    listener: &UnixListener,
    child: &mut Child,
    deadline: Instant,
) -> std::os::unix::net::UnixStream {
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("control accept failed: {error}"),
        }
        if let Some(status) = child.try_wait().expect("observe host") {
            let mut stderr = String::new();
            child
                .stderr
                .take()
                .expect("host stderr")
                .read_to_string(&mut stderr)
                .expect("read host stderr");
            panic!("host exited before control bind: {status}: {stderr}");
        }
        assert!(Instant::now() < deadline, "control accept timed out");
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(crate) fn read_control_line(reader: &mut BufReader<std::os::unix::net::UnixStream>) -> String {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read control line");
    assert!(line.ends_with('\n'), "incomplete control line: {line:?}");
    line.pop();
    line
}

pub(crate) fn assert_nonzero_descendant(line: &str) {
    let mut fields = line.split_whitespace();
    assert_eq!(fields.next(), Some("DESCENDANT"), "{line}");
    let inner_pid = fields
        .next()
        .expect("descendant pid")
        .parse::<u32>()
        .expect("numeric descendant pid");
    assert_ne!(inner_pid, 0, "{line}");
    assert!(fields.next().is_none(), "{line}");
}
