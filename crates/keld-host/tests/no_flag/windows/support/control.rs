//! Control handshakes, bounded reads and terminal host diagnostics.

use std::io::{BufRead, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

use crate::{CONTROL_LINE_LIMIT, PRODUCT_DEADLINE, startup_kill_switch};

/// The kill switches of one generation's control handshake.
///
/// `ready` bounds the `READY` read, which can follow the host's `WebView2` startup;
/// `record` bounds the accept and every other record. Product handshakes use
/// [`HandshakeKillSwitches::product`]; only the harness contract tests pass smaller
/// values, so they can prove which record reads under which switch without waiting
/// out the product deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HandshakeKillSwitches {
    pub(crate) record: Duration,
    pub(crate) ready: Duration,
}

impl HandshakeKillSwitches {
    /// The per-step product deadline for every record, and the startup kill switch
    /// for `READY`.
    pub(crate) fn product() -> Self {
        Self {
            record: PRODUCT_DEADLINE,
            ready: startup_kill_switch(),
        }
    }
}

pub(crate) fn accept_ready_generation(
    listener: &TcpListener,
    child: &mut Child,
) -> (BufReader<TcpStream>, TcpStream, u32, String) {
    let (reader, writer, pid, link, _, lease_handle) =
        accept_ready_generation_inner(listener, child, false, HandshakeKillSwitches::product());
    assert!(lease_handle.is_none(), "unexpected lease census disclosure");
    (reader, writer, pid, link)
}

/// The Bun pid and app link of a generation's `HELLO <pid> <link>` record.
pub(crate) fn parse_hello(record: &str) -> (u32, String) {
    let mut fields = record.split_whitespace();
    assert_eq!(fields.next(), Some("HELLO"), "{record}");
    let pid = fields
        .next()
        .expect("generation pid")
        .parse::<u32>()
        .expect("numeric generation pid");
    let link = fields.next().expect("generation app link").to_owned();
    assert!(fields.next().is_none(), "{record}");
    (pid, link)
}

/// After the host has exited, no further generation connected to this
/// nonblocking control listener.
pub(crate) fn assert_no_control_connection(listener: &TcpListener, context: &str) {
    assert!(
        matches!(
            listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ),
        "{context}"
    );
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
        accept_ready_generation_inner(listener, child, true, HandshakeKillSwitches::product());
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
    accept_ready_generation_with_descendant_within(
        listener,
        child,
        HandshakeKillSwitches::product(),
    )
}

/// [`accept_ready_generation_with_descendant`] under explicit kill switches.
pub(crate) fn accept_ready_generation_with_descendant_within(
    listener: &TcpListener,
    child: &mut Child,
    kill_switches: HandshakeKillSwitches,
) -> (BufReader<TcpStream>, TcpStream, u32, String, u32) {
    let (reader, writer, pid, link, descendant_pid, lease_handle) =
        accept_ready_generation_inner(listener, child, false, kill_switches);
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
    kill_switches: HandshakeKillSwitches,
) -> (
    BufReader<TcpStream>,
    TcpStream,
    u32,
    String,
    u32,
    Option<usize>,
) {
    let control =
        accept_control_or_host_failure(listener, child, Instant::now() + kill_switches.record);
    control
        .set_read_timeout(Some(kill_switches.record))
        .expect("generation control read deadline");
    let writer = control.try_clone().expect("generation control writer");
    let mut reader = BufReader::new(control);
    let hello = read_control_line_or_host_failure_after_phase(
        &mut reader,
        child,
        "HELLO",
        "CONTROL_CONNECTED",
        kill_switches.record,
    );
    let (pid, link) = parse_hello(&hello);
    let descendant_record = read_control_line_or_host_failure_after_phase(
        &mut reader,
        child,
        "DESCENDANT",
        "HELLO",
        kill_switches.record,
    );
    let descendant_pid = parse_descendant_pid(&descendant_record);
    let (lease_handle, last_valid_phase) = if expect_lease_handle {
        let lease_record = read_control_line_or_host_failure_after_phase(
            &mut reader,
            child,
            "LEASE_HANDLE",
            "DESCENDANT",
            kill_switches.record,
        );
        let value = lease_record
            .strip_prefix("LEASE_HANDLE ")
            .expect("lease census line prefix");
        (
            Some(usize::from_str_radix(value, 16).expect("hexadecimal host lease handle")),
            "LEASE_HANDLE",
        )
    } else {
        (None, "DESCENDANT")
    };
    // A generation's READY can follow the host's WebView2 startup, which begins only
    // after that generation is bound, so READY reads under the startup kill switch.
    // A later generation in an already running engine (recovery) has no startup; the
    // larger kill switch only delays such a failure, it never accepts a missing READY.
    // Every other record keeps the per-step product deadline.
    assert_eq!(
        read_control_line_or_host_failure_after_phase(
            &mut reader,
            child,
            "READY",
            last_valid_phase,
            kill_switches.ready,
        ),
        "READY"
    );
    assert_eq!(
        read_control_line_or_host_failure_after_phase(
            &mut reader,
            child,
            "ECHO1",
            "READY",
            kill_switches.record,
        ),
        "ECHO1"
    );
    assert_eq!(
        read_control_line_or_host_failure_after_phase(
            &mut reader,
            child,
            "ECHO2",
            "ECHO1",
            kill_switches.record,
        ),
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
            panic!(
                "host exited before control bind: {status}\n{}",
                exited_host_output(child)
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
    read_control_line_or_host_failure_after_phase(
        reader,
        child,
        label,
        "not tracked",
        PRODUCT_DEADLINE,
    )
}

/// [`read_control_line_or_host_failure`] for a session's `READY`, which follows the
/// host's `WebView2` startup and is therefore bounded by [`startup_kill_switch`].
pub(crate) fn read_ready_or_host_failure(
    reader: &mut BufReader<TcpStream>,
    child: &mut Child,
    label: &str,
) -> String {
    read_control_line_or_host_failure_after_phase(
        reader,
        child,
        label,
        "not tracked",
        startup_kill_switch(),
    )
}

fn read_control_line_or_host_failure_after_phase(
    reader: &mut BufReader<TcpStream>,
    child: &mut Child,
    label: &str,
    last_valid_phase: &'static str,
    kill_switch: Duration,
) -> String {
    match try_read_control_line(reader, Instant::now() + kill_switch) {
        Ok(line) if line != "LINK_EOF" => line,
        result => {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if let Some(status) = child.try_wait().expect("observe failed host") {
                    panic!(
                        "host exited while awaiting {label}: {status}; last valid phase={last_valid_phase}; child pid={}; read={result:?}\n{}",
                        child.id(),
                        exited_host_output(child),
                    );
                }
                assert!(
                    Instant::now() < deadline,
                    "control failed while awaiting {label}: last valid phase={last_valid_phase}; child pid={}; child status=still running; read={result:?}; stdout/stderr=not read while child is alive",
                    child.id(),
                );
                thread::park_timeout(Duration::from_millis(10));
            }
        }
    }
}

/// The captured stdout and stderr of a host that has already exited, for its
/// failure report. Reading to the end cannot block once the host is gone.
pub(crate) fn exited_host_output(child: &mut Child) -> String {
    let mut stdout = String::new();
    let mut stderr = String::new();
    child
        .stdout
        .take()
        .expect("captured exited host stdout")
        .read_to_string(&mut stdout)
        .expect("read exited host stdout");
    child
        .stderr
        .take()
        .expect("captured exited host stderr")
        .read_to_string(&mut stderr)
        .expect("read exited host stderr");
    format!("stdout:\n{stdout}\nstderr:\n{stderr}")
}
