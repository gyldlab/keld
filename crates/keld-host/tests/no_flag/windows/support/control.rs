//! Control handshakes, bounded reads and terminal host diagnostics.

use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::GetExitCodeProcess;

use crate::support::process::{
    PipeText, exited_child_output, pipe_text_by, wait_for_process_signal,
};
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
    let (reader, writer, pid, link, _, lease_handle) = accept_ready_generation_inner(
        listener,
        child,
        false,
        HandshakeKillSwitches::product(),
        None,
    );
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

/// After the host has exited, no further generation connected to this control
/// listener. The check never waits: it switches the listener to nonblocking.
pub(crate) fn assert_no_control_connection(listener: &TcpListener, context: &str) {
    listener
        .set_nonblocking(true)
        .expect("nonblocking control listener");
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

/// The test retains an actual Host kernel handle and separately drains the
/// CLI's inherited Host output before awaiting READY. These are evidence-only
/// inputs: they cannot change the authenticated generation or accept a failure.
pub(crate) struct LeasedReadyHost<'a> {
    pub(crate) pid: u32,
    pub(crate) process: &'a OwnedHandle,
    pub(crate) stdout: &'a PipeText,
    pub(crate) stderr: &'a PipeText,
    pub(crate) runtime: &'a str,
}

pub(crate) fn accept_ready_generation_with_lease(
    listener: &TcpListener,
    child: &mut Child,
    host: &LeasedReadyHost<'_>,
) -> (BufReader<TcpStream>, TcpStream, u32, String, usize) {
    let (reader, writer, pid, link, _, lease_handle) = accept_ready_generation_inner(
        listener,
        child,
        true,
        HandshakeKillSwitches::product(),
        Some(host),
    );
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
        accept_ready_generation_inner(listener, child, false, kill_switches, None);
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
    host: Option<&LeasedReadyHost<'_>>,
) -> (
    BufReader<TcpStream>,
    TcpStream,
    u32,
    String,
    u32,
    Option<usize>,
) {
    let control = accept_control_until_with_host(
        listener,
        Some(child),
        Instant::now() + kill_switches.record,
        host,
    );
    control
        .set_read_timeout(Some(kill_switches.record))
        .expect("generation control read deadline");
    let writer = control.try_clone().expect("generation control writer");
    let mut reader = BufReader::new(control);
    // Keep one protocol reader. Only lease-bearing callers supply a retained
    // Host witness and previously consumed, concurrently drained pipes.
    let mut read = |reader: &mut BufReader<TcpStream>,
                    label: &str,
                    last_valid_phase: &'static str,
                    kill_switch: Duration| {
        if let Some(host) = host {
            read_with_actual_host_evidence(
                reader,
                child,
                label,
                last_valid_phase,
                kill_switch,
                host,
            )
        } else {
            read_control_line_or_host_failure_after_phase(
                reader,
                child,
                label,
                last_valid_phase,
                kill_switch,
            )
        }
    };
    let hello = read(
        &mut reader,
        "HELLO",
        "CONTROL_CONNECTED",
        kill_switches.record,
    );
    let (pid, link) = parse_hello(&hello);
    let descendant_record = read(&mut reader, "DESCENDANT", "HELLO", kill_switches.record);
    let descendant_pid = parse_descendant_pid(&descendant_record);
    let (lease_handle, last_valid_phase) = if expect_lease_handle {
        let lease_record = read(
            &mut reader,
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
    // A generation's READY follows WebView2 startup and therefore retains its
    // separately owned startup kill switch; no existing deadline is extended.
    assert_eq!(
        read(&mut reader, "READY", last_valid_phase, kill_switches.ready),
        "READY"
    );
    assert_eq!(
        read(&mut reader, "ECHO1", "READY", kill_switches.record),
        "ECHO1"
    );
    assert_eq!(
        read(&mut reader, "ECHO2", "ECHO1", kill_switches.record),
        "ECHO2"
    );
    (reader, writer, pid, link, descendant_pid, lease_handle)
}

/// A retained process HANDLE is a stronger Host-liveness fact than an outer
/// CLI helper's exit code or a PID census (which can race process reuse).
fn host_exit_observation(host: &LeasedReadyHost<'_>) -> String {
    match wait_for_process_signal(host.process, 0) {
        WAIT_OBJECT_0 => {
            let mut code = 0_u32;
            // SAFETY: this is the retained, query-capable test-owned Host process
            // handle; the output is a live stack u32 for this synchronous call.
            let observed =
                unsafe { GetExitCodeProcess(host.process.as_raw_handle().cast(), &raw mut code) };
            if observed == 0 {
                String::from("signaled exit_code=unavailable")
            } else {
                format!("signaled exit_code={code}")
            }
        }
        WAIT_TIMEOUT => String::from("running"),
        other => format!("unproven wait_result=0x{other:08x}"),
    }
}

/// The lease-specific failure path MUST NOT use exited_child_output(child):
/// the CLI stdout and stderr are already being consumed by dedicated threads.
/// The retained Host HANDLE is independent of the still-running helper.
fn fail_with_actual_host_evidence(
    host: &LeasedReadyHost<'_>,
    cli: &mut Child,
    original: &str,
) -> ! {
    let host_state = host_exit_observation(host);
    let helper_state = match cli.try_wait() {
        Ok(Some(status)) => format!("exited ({status})"),
        Ok(None) => String::from("running"),
        Err(error) => format!("unproven ({error})"),
    };
    let stdout = pipe_text_by(host.stdout, Instant::now());
    let stderr = pipe_text_by(host.stderr, Instant::now());
    panic!(
        "{original}; actual_host_pid={} actual_host_state={host_state};          cli_pid={} cli_state={helper_state}; webview2_runtime={};          observed helper/Host stdout:\n{stdout}\nobserved helper/Host stderr:\n{stderr}",
        host.pid,
        cli.id(),
        host.runtime,
    );
}

fn read_with_actual_host_evidence(
    reader: &mut BufReader<TcpStream>,
    child: &mut Child,
    label: &str,
    last_valid_phase: &'static str,
    kill_switch: Duration,
    host: &LeasedReadyHost<'_>,
) -> String {
    match try_read_control_line(reader, Instant::now() + kill_switch) {
        Ok(line) if line != "LINK_EOF" => line,
        result => fail_with_actual_host_evidence(
            host,
            child,
            &format!(
                "control failed while awaiting {label}: last valid phase={last_valid_phase}; read={result:?}"
            ),
        ),
    }
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
    child: Option<&mut Child>,
    deadline: Instant,
) -> TcpStream {
    accept_control_until_with_host(listener, child, deadline, None)
}

fn accept_control_until_with_host(
    listener: &TcpListener,
    mut child: Option<&mut Child>,
    deadline: Instant,
    host: Option<&LeasedReadyHost<'_>>,
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
            if let Some(host) = host {
                fail_with_actual_host_evidence(
                    host,
                    child,
                    &format!("host exited before control bind: {status}"),
                );
            }
            panic!(
                "host exited before control bind: {status}\n{}",
                exited_child_output(child)
            );
        }
        if Instant::now() >= deadline {
            if let Some(host) = host
                && let Some(child) = child.as_mut()
            {
                fail_with_actual_host_evidence(host, child, "product control accept timed out");
            }
            panic!("product control accept timed out");
        }
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
                        exited_child_output(child),
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
