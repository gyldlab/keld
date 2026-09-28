//! Control handshakes, bounded reads and terminal host diagnostics.

use std::io::{BufRead, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

use crate::{CONTROL_LINE_LIMIT, PRODUCT_DEADLINE};

pub(crate) fn accept_ready_generation(
    listener: &TcpListener,
    child: &mut Child,
) -> (BufReader<TcpStream>, TcpStream, u32, String) {
    let (reader, writer, pid, link, _, lease_handle) =
        accept_ready_generation_inner(listener, child, false);
    assert!(lease_handle.is_none(), "unexpected lease census disclosure");
    (reader, writer, pid, link)
}

pub(crate) fn parse_descendant_pid(record: &str) -> u32 {
    record
        .strip_prefix("DESCENDANT ")
        .expect("descendant record prefix")
        .parse()
        .expect("numeric descendant PID")
}

pub(crate) fn accept_ready_generation_with_lease(
    listener: &TcpListener,
    child: &mut Child,
) -> (BufReader<TcpStream>, TcpStream, u32, String, usize) {
    let (reader, writer, pid, link, _, lease_handle) =
        accept_ready_generation_inner(listener, child, true);
    (
        reader,
        writer,
        pid,
        link,
        lease_handle.expect("host lease handle census value"),
    )
}

pub(crate) fn accept_ready_generation_with_descendant(
    listener: &TcpListener,
    child: &mut Child,
) -> (BufReader<TcpStream>, TcpStream, u32, String, u32) {
    let (reader, writer, pid, link, descendant_pid, lease_handle) =
        accept_ready_generation_inner(listener, child, false);
    assert!(lease_handle.is_none(), "unexpected lease census disclosure");
    assert_ne!(
        descendant_pid, 0,
        "host-death proof needs a real descendant"
    );
    (reader, writer, pid, link, descendant_pid)
}

fn accept_ready_generation_inner(
    listener: &TcpListener,
    child: &mut Child,
    expect_lease_handle: bool,
) -> (
    BufReader<TcpStream>,
    TcpStream,
    u32,
    String,
    u32,
    Option<usize>,
) {
    let control =
        accept_control_or_host_failure(listener, child, Instant::now() + PRODUCT_DEADLINE);
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("generation control read deadline");
    let writer = control.try_clone().expect("generation control writer");
    let mut reader = BufReader::new(control);
    let hello = read_control_line_or_host_failure(&mut reader, child, "HELLO");
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{hello}");
    let pid = fields
        .next()
        .expect("generation pid")
        .parse::<u32>()
        .expect("numeric generation pid");
    let link = fields.next().expect("generation app link").to_owned();
    assert!(fields.next().is_none(), "{hello}");
    let descendant_record = read_control_line_or_host_failure(&mut reader, child, "DESCENDANT");
    let descendant_pid = parse_descendant_pid(&descendant_record);
    let lease_handle = if expect_lease_handle {
        let lease_record = read_control_line_or_host_failure(&mut reader, child, "LEASE_HANDLE");
        let value = lease_record
            .strip_prefix("LEASE_HANDLE ")
            .expect("lease census line prefix");
        Some(usize::from_str_radix(value, 16).expect("hexadecimal host lease handle"))
    } else {
        None
    };
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, child, "READY"),
        "READY"
    );
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, child, "ECHO1"),
        "ECHO1"
    );
    assert_eq!(
        read_control_line_or_host_failure(&mut reader, child, "ECHO2"),
        "ECHO2"
    );
    (reader, writer, pid, link, descendant_pid, lease_handle)
}

pub(crate) fn accept_control_or_host_failure(
    listener: &TcpListener,
    child: &mut Child,
    deadline: Instant,
) -> TcpStream {
    accept_control_until(listener, Some(child), deadline)
}

pub(crate) fn accept_control_until(
    listener: &TcpListener,
    mut child: Option<&mut Child>,
    deadline: Instant,
) -> TcpStream {
    listener
        .set_nonblocking(true)
        .expect("nonblocking product control listener");
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("blocking product control stream");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("product control accept failed: {error}"),
        }
        if let Some(child) = child.as_mut()
            && let Some(status) = child.try_wait().expect("observe early host exit")
        {
            let mut stdout = String::new();
            let mut stderr = String::new();
            child
                .stdout
                .take()
                .expect("captured host stdout")
                .read_to_string(&mut stdout)
                .expect("read host stdout");
            child
                .stderr
                .take()
                .expect("captured host stderr")
                .read_to_string(&mut stderr)
                .expect("read host stderr");
            panic!(
                "host exited before control bind: {status}\nstdout:\n{stdout}\nstderr:\n{stderr}"
            );
        }
        assert!(
            Instant::now() < deadline,
            "product control accept timed out"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(crate) fn read_control_line(reader: &mut BufReader<TcpStream>) -> String {
    try_read_control_line(reader, Instant::now() + PRODUCT_DEADLINE)
        .expect("complete bounded control line")
}

pub(crate) fn try_read_control_line(
    reader: &mut BufReader<TcpStream>,
    deadline: Instant,
) -> Result<String, String> {
    let original = reader
        .get_ref()
        .read_timeout()
        .map_err(|error| error.to_string())?;
    let result = (|| {
        let mut line = Vec::new();
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .filter(|value| !value.is_zero())
                .ok_or_else(|| "control line deadline elapsed".to_owned())?;
            reader
                .get_mut()
                .set_read_timeout(Some(remaining))
                .map_err(|error| error.to_string())?;
            let bytes = reader
                .fill_buf()
                .map_err(|error| format!("read control line: {error}"))?;
            if bytes.is_empty() {
                return Err("control stream ended before newline".to_owned());
            }
            let newline = bytes.iter().position(|byte| *byte == b'\n');
            let count = newline.map_or(bytes.len(), |index| index + 1);
            if line.len() + count > CONTROL_LINE_LIMIT {
                return Err("control line exceeds 4096 bytes".to_owned());
            }
            line.extend_from_slice(&bytes[..count]);
            reader.consume(count);
            if newline.is_some() {
                line.pop();
                return String::from_utf8(line)
                    .map_err(|error| format!("control line is not UTF-8: {error}"));
            }
        }
    })();
    let restored = reader.get_mut().set_read_timeout(original);
    match (result, restored) {
        (result, Ok(())) => result,
        (Err(read), Err(restore)) => Err(format!("{read}; restore control timeout: {restore}")),
        (Ok(_), Err(error)) => Err(format!("restore control timeout: {error}")),
    }
}

pub(crate) fn read_control_line_or_host_failure(
    reader: &mut BufReader<TcpStream>,
    child: &mut Child,
    label: &str,
) -> String {
    match try_read_control_line(reader, Instant::now() + PRODUCT_DEADLINE) {
        Ok(line) if line != "LINK_EOF" => line,
        result => {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if let Some(status) = child.try_wait().expect("observe failed host") {
                    let mut stdout = String::new();
                    let mut stderr = String::new();
                    child
                        .stdout
                        .take()
                        .expect("captured failed host stdout")
                        .read_to_string(&mut stdout)
                        .expect("read failed host stdout");
                    child
                        .stderr
                        .take()
                        .expect("captured failed host stderr")
                        .read_to_string(&mut stderr)
                        .expect("read failed host stderr");
                    panic!(
                        "host exited while awaiting {label}: {status}; read={result:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
                    );
                }
                assert!(
                    Instant::now() < deadline,
                    "control failed while awaiting {label}: read={result:?}"
                );
                thread::park_timeout(Duration::from_millis(10));
            }
        }
    }
}
