//! Control byte/deadline and terminal-diagnostic contracts.

use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::support::control::{read_control_line_or_host_failure, try_read_control_line};
use crate::{CONTROL_LINE_LIMIT, PRODUCT_DEADLINE, wait_child};

#[test]
fn control_line_preserves_the_byte_limit_and_expired_read_does_not_consume_data() {
    assert_eq!(CONTROL_LINE_LIMIT, 4096);
    for size in [4096, 4097] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("control boundary listener");
        let mut peer =
            TcpStream::connect(listener.local_addr().expect("address")).expect("control peer");
        let (stream, _) = listener.accept().expect("accept control peer");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("original timeout");
        let mut reader = BufReader::new(stream);
        let mut message = vec![b'x'; size - 1];
        message.push(b'\n');
        peer.write_all(&message).expect("send control boundary");
        let expired = try_read_control_line(&mut reader, Instant::now());
        assert_eq!(
            expired.err().as_deref(),
            Some("control line deadline elapsed")
        );
        let actual = try_read_control_line(&mut reader, Instant::now() + Duration::from_secs(1));
        if size == 4096 {
            assert_eq!(actual.expect("exact control bound").len(), 4095);
        } else {
            assert_eq!(
                actual.err().as_deref(),
                Some("control line exceeds 4096 bytes")
            );
        }
        assert_eq!(
            reader.get_ref().read_timeout().expect("restored timeout"),
            Some(Duration::from_secs(2))
        );
    }
}

#[test]
fn startup_terminal_record_preserves_host_failure_output() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("diagnostic control listener");
    let mut writer = TcpStream::connect(listener.local_addr().expect("control address"))
        .expect("diagnostic control peer");
    let (stream, _) = listener.accept().expect("accept diagnostic peer");
    let mut reader = BufReader::new(stream);
    writer
        .write_all(b"READY\nLINK_EOF\n")
        .expect("control records");
    let mut child = Command::new("cmd.exe")
        .args([
            "/d",
            "/c",
            "echo KEL265_DIAGNOSTIC_STDOUT & echo KEL265_DIAGNOSTIC_STDERR 1>&2 & exit /b 23",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("diagnostic child");
    let status = wait_child(&mut child, Instant::now() + PRODUCT_DEADLINE);
    assert_eq!(status.code(), Some(23));
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, &mut child, "fixture READY"),
        "READY"
    );
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_control_line_or_host_failure(&mut reader, &mut child, "fixture READY")
    }))
    .expect_err("terminal startup record must preserve the failed child diagnostics");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .expect("panic message");
    for expected in [
        "LINK_EOF",
        "fixture READY",
        "KEL265_DIAGNOSTIC_STDOUT",
        "KEL265_DIAGNOSTIC_STDERR",
        "exit code: 23",
    ] {
        assert!(message.contains(expected), "missing {expected}: {message}");
    }
    assert!(
        child.stdout.is_none() && child.stderr.is_none(),
        "both captures were consumed"
    );
}
