//! One test-only observer/transport owner; shared group custody stays in support.
use super::{NativeChild, captured};
use nix::errno::Errno;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::{Pid, getpgid, getpgrp};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::io::{self, Read as _, Write as _};
use std::os::fd::AsRawFd as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub(super) const OBSERVER_SOCKET_ENV: &str = "KELD_CORE_AC7_PROCESS_SOCKET";
pub(super) const OBSERVER_TOKEN_ENV: &str = "KELD_CORE_AC7_PROCESS_TOKEN";
const OBSERVER_SOURCE: &str = include_str!("fixtures/native_process_observer.py");
const RECORD_LIMIT: usize = 4096;
const OWNED_CALLER_EXIT_TAG: &str = "KELD_AC7_OWNED_CALLER_EXIT ";

pub(super) fn protocol_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub(super) struct ControlPeer {
    stream: UnixStream,
    pending: Vec<u8>,
}

impl ControlPeer {
    fn new(stream: UnixStream) -> io::Result<Self> {
        // Darwin accepted sockets inherit O_NONBLOCK; timeout must really wait.
        stream.set_nonblocking(false)?;
        Ok(Self {
            stream,
            pending: Vec::new(),
        })
    }

    pub(super) fn send(&mut self, record: &Value, deadline: Instant) -> io::Result<()> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "control deadline"));
        }
        self.stream
            .set_write_timeout(Some(remaining))
            .inspect_err(|error| {
                self.trace_error("set_write_timeout", remaining, error);
            })?;
        let mut bytes = serde_json::to_vec(record).map_err(io::Error::other)?;
        bytes.push(b'\n');
        if bytes.len() > RECORD_LIMIT {
            return Err(protocol_error("control record exceeds its fixed bound"));
        }
        self.stream.write_all(&bytes).inspect_err(|error| {
            self.trace_error("write_all", remaining, error);
        })
    }

    pub(super) fn read(&mut self, deadline: Instant) -> io::Result<Value> {
        loop {
            if let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
                let tail = self.pending.split_off(end + 1);
                let record = serde_json::from_slice(&self.pending[..end])
                    .map_err(|error| protocol_error(error.to_string()))?;
                self.pending = tail;
                return Ok(record);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "control deadline"));
            }
            self.stream
                .set_read_timeout(Some(remaining))
                .inspect_err(|error| {
                    self.trace_error("set_read_timeout", remaining, error);
                })?;
            let mut bytes = [0_u8; 1024];
            let count = self.stream.read(&mut bytes).inspect_err(|error| {
                self.trace_error("recv", remaining, error);
            })?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "required observer acknowledgment missing",
                ));
            }
            self.pending.extend_from_slice(&bytes[..count]);
            if self.pending.len() > RECORD_LIMIT {
                return Err(protocol_error("control record exceeds its fixed bound"));
            }
        }
    }
    fn trace_error(&self, operation: &str, requested: Duration, error: &io::Error) {
        eprintln!(
            "KELD_AC7_CONTROL_FAILURE pid={} operation={operation} fd={} pending={} requested_ns={} raw_os_error={:?} kind={:?} error={error}",
            std::process::id(),
            self.stream.as_raw_fd(),
            self.pending.len(),
            requested.as_nanos(),
            error.raw_os_error(),
            error.kind(),
        );
    }
}

pub(super) fn require_record(record: &Value, phase: &str, token: &str) -> io::Result<()> {
    if record["phase"] != phase || record["token"] != token {
        return Err(protocol_error(format!(
            "unexpected control record: {record}"
        )));
    }
    Ok(())
}

pub(super) struct ProcessObserver {
    process: NativeChild,
    root: tempfile::TempDir,
    listener: UnixListener,
    peer: Option<ControlPeer>,
    caller: Option<ControlPeer>,
    token: String,
    case: String,
    sequence: u64,
    family: Option<RegisteredFamily>,
    owned_leader_status: Option<(u32, i32)>,
    owned_bun_status: Option<(u32, i32)>,
    origin: Option<Instant>,
    failure: Option<String>,
    records: Vec<Value>,
    last_poll: Option<Value>,
    stdout: File,
    stderr: File,
}

#[derive(Clone, Copy)]
struct RegisteredFamily {
    leader: u32,
    bun: u32,
    filters: [i64; 2],
}

impl ProcessObserver {
    pub(super) fn start(case: &str) -> io::Result<Self> {
        let root = tempfile::Builder::new()
            .prefix("k140o-")
            .tempdir_in("/tmp")?;
        let socket = root.path().join("process.sock");
        let listener = UnixListener::bind(&socket)?;
        listener.set_nonblocking(true)?;
        let script = root.path().join("observer.py");
        fs::write(&script, OBSERVER_SOURCE)?;
        let mut nonce = [0_u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut nonce)?;
        let token = super::super::super::hex_of(&nonce);
        let stdout = tempfile::tempfile()?;
        let stderr = tempfile::tempfile()?;
        let process = NativeChild::own(
            Command::new("python3")
                .arg(&script)
                .arg(&socket)
                .arg(&token)
                .arg(super::super::OBSERVATION_BOUND.as_secs_f64().to_string())
                .arg(RECORD_LIMIT.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout.try_clone()?))
                .stderr(Stdio::from(stderr.try_clone()?))
                // All observer tooling inherits this separate owned island.
                .process_group(0)
                .spawn()?,
        )?;
        let mut observer = Self {
            process,
            root,
            listener,
            peer: None,
            caller: None,
            token,
            case: case.to_owned(),
            sequence: 0,
            family: None,
            owned_leader_status: None,
            owned_bun_status: None,
            origin: None,
            failure: None,
            records: Vec::new(),
            last_poll: None,
            stdout,
            stderr,
        };
        if let Err(error) = observer.accept_observer() {
            let _ = observer.process.stop_unreaped_group(None);
            eprintln!(
                "KELD_AC7_PROCESS_STARTUP_FAILED error={error}\nstdout={}\nstderr={}",
                captured(&mut observer.stdout),
                captured(&mut observer.stderr),
            );
            return Err(error);
        }
        Ok(observer)
    }

    pub(super) fn socket(&self) -> std::path::PathBuf {
        self.root.path().join("process.sock")
    }

    pub(super) fn token(&self) -> &str {
        &self.token
    }

    pub(super) fn group(&self) -> Pid {
        self.process.group
    }

    pub(super) fn set_origin(&mut self, started: Instant) {
        self.origin = Some(started);
    }

    pub(super) fn has_failed(&self) -> bool {
        self.failure.is_some()
    }

    pub(super) fn stop_family(
        &mut self,
        before_reap: impl FnOnce(Result<(), Errno>),
    ) -> io::Result<ExitStatus> {
        self.process.stop_unreaped_group_observed(before_reap)
    }

    pub(super) fn record_leader_reaped(&mut self, child: &NativeChild, status: ExitStatus) {
        let leader = child.process.0.id();
        let raw = status.into_raw();
        self.owned_leader_status = Some((leader, raw));
        self.records.push(json!({
            "phase": "leader-reaped", "leader": leader, "raw": raw,
            "signal_armed": child.armed,
            "receipt_us": self.origin.map(|origin| origin.elapsed().as_micros()),
        }));
        if child.armed || self.family.is_none_or(|family| family.leader != leader) {
            self.fail(&protocol_error(
                "independent owned leader identity/reap mismatch",
            ));
        }
        // The receiver has completed its entire enrollment before reaching
        // terminal state. Release the writer once, including failed-case paths.
        self.caller.take();
    }

    pub(super) fn capture_owned_caller_exit(&mut self, detail: &str, required: bool) {
        let receipt = owned_caller_exit(detail, self.family, &self.case, &self.token);
        match receipt {
            Ok(Some((record, pid, raw))) => {
                self.owned_bun_status = Some((pid, raw));
                self.records.push(record);
            }
            Ok(None) => {
                self.records.push(json!({
                    "phase": "owned-caller-exit-unknown", "required": required,
                }));
                if required {
                    self.fail(&protocol_error(
                        "independently owned caller wait status missing",
                    ));
                }
            }
            Err(error) => self.fail(&error),
        }
    }

    fn accept_observer(&mut self) -> io::Result<()> {
        let deadline = Instant::now() + super::super::OBSERVATION_BOUND;
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "observer startup acknowledgment missing",
                        ));
                    }
                    std::thread::yield_now();
                }
                Err(error) => return Err(error),
            }
        };
        let mut peer = ControlPeer::new(stream)?;
        let ready = peer.read(deadline)?;
        require_record(&ready, "observer-ready", &self.token)?;
        let digest: [u8; 32] = Sha256::digest(OBSERVER_SOURCE.as_bytes()).into();
        if ready["role"] != "observer"
            || ready["pid"] != self.process.process.0.id()
            || ready["group"] != self.process.group.as_raw()
            || ready["seq"] != 0
            || ready["source_sha256"] != super::super::super::hex_of(&digest)
        {
            return Err(protocol_error("observer executable/source/group mismatch"));
        }
        self.records.push(ready);
        self.peer = Some(peer);
        Ok(())
    }

    fn request(&mut self, mut request: Value, deadline: Instant) -> io::Result<Value> {
        self.sequence += 1;
        request["token"] = json!(self.token);
        request["seq"] = json!(self.sequence);
        let expected = if request["phase"] == "arm" {
            "prearmed"
        } else {
            request["phase"]
                .as_str()
                .ok_or_else(|| protocol_error("missing request phase"))?
        };
        let began = self.origin.map(|origin| origin.elapsed().as_micros());
        let peer = self
            .peer
            .as_mut()
            .ok_or_else(|| protocol_error("observer control unavailable"))?;
        peer.send(&request, deadline)?;
        let reply = peer.read(deadline)?;
        require_record(&reply, expected, &self.token)?;
        if reply["seq"] != self.sequence {
            return Err(protocol_error("observer response sequence mismatch"));
        }
        if let Some(family) = self.family
            && matches!(expected, "poll" | "census" | "finish")
        {
            validate_terminal_record(&reply, family)?;
        }
        let trace = json!({
            "request": request,
            "reply": reply,
            "receipt_started_us": began,
            "receipt_ended_us": self.origin.map(|origin| origin.elapsed().as_micros()),
        });
        if trace["request"]["phase"] == "poll" {
            if self
                .last_poll
                .as_ref()
                .is_some_and(|prior| prior["reply"]["states"] != trace["reply"]["states"])
            {
                if let Some(prior) = self.last_poll.take() {
                    self.records.push(prior);
                }
                self.records.push(trace.clone());
            }
            // Retain the last negative/positive sample, not 45s of duplicates.
            self.last_poll = Some(trace);
        } else {
            if let Some(prior) = self.last_poll.take() {
                self.records.push(prior);
            }
            self.records.push(trace);
        }
        Ok(reply)
    }

    fn enroll(&mut self, child: &NativeChild, deadline: Instant) -> io::Result<()> {
        let stream = match self.listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error),
        };
        let mut caller = ControlPeer::new(stream)?;
        let enrollment = caller.read(deadline)?;
        require_record(&enrollment, "enroll", &self.token)?;
        let leader = child.process.0.id();
        let bun = enrollment["bun"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| protocol_error("invalid owned Bun identity"))?;
        if !child.armed
            || enrollment["case"] != self.case
            || enrollment["leader"] != leader
            || enrollment["group"] != child.group.as_raw()
            || bun <= 1
            || [leader, self.process.process.0.id(), std::process::id()].contains(&bun)
            || getpgid(Some(Pid::from_raw(
                i32::try_from(bun).map_err(|error| protocol_error(error.to_string()))?,
            )))
            .map_err(io::Error::other)?
                != child.group
        {
            return Err(protocol_error(
                "current owned leader/Bun enrollment mismatch",
            ));
        }
        let mut request = enrollment;
        request["phase"] = json!("arm");
        let mut acknowledgment = self.request(request, deadline)?;
        if acknowledgment["leader"] != leader
            || acknowledgment["bun"] != bun
            || acknowledgment["group"] != child.group.as_raw()
            || acknowledgment["receipts"].as_array().map(Vec::len) != Some(2)
        {
            return Err(protocol_error(
                "exact status registration acknowledgment rejected",
            ));
        }
        let family = registered_family(&acknowledgment, leader, bun)?;
        acknowledgment["case"] = json!(self.case);
        caller.send(&acknowledgment, deadline)?;
        // write_all is not a remote frame-consumption fence. On Darwin a full
        // peer close forbids subsequent timeout setters even with queued data.
        // Keep this endpoint owned through native-child terminal observation.
        self.caller = Some(caller);
        self.family = Some(family);
        Ok(())
    }

    pub(super) fn fail(&mut self, error: &io::Error) {
        if self.failure.is_none() {
            self.failure = Some(error.to_string());
        }
        // A deterministic control failure is never retried with a stale reply.
        self.peer.take();
    }

    pub(super) fn hold_helper(&mut self) -> io::Result<(u32, ControlPeer, Value)> {
        let deadline = Instant::now() + super::super::OBSERVATION_BOUND;
        let reply = self.request(json!({"phase": "hold-helper"}), deadline)?;
        let helper = reply["helper"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| protocol_error("invalid held helper PID"))?;
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "helper ACK"));
                    }
                    std::thread::yield_now();
                }
                Err(error) => return Err(error),
            }
        };
        let mut control = ControlPeer::new(stream)?;
        let held = control.read(deadline)?;
        require_record(&held, "helper-held", &self.token)?;
        let digest: [u8; 32] = Sha256::digest(OBSERVER_SOURCE.as_bytes()).into();
        let group = getpgid(Some(Pid::from_raw(
            i32::try_from(helper).map_err(|error| protocol_error(error.to_string()))?,
        )))
        .map_err(io::Error::other)?;
        if held["role"] != "helper"
            || held["pid"] != helper
            || held["ppid"] != self.process.process.0.id()
            || held["group"] != self.process.group.as_raw()
            || group != self.process.group
            || held["source_sha256"] != super::super::super::hex_of(&digest)
        {
            return Err(protocol_error("actual held helper identity mismatch"));
        }
        Ok((helper, control, held))
    }

    pub(super) fn poll(&mut self, child: &NativeChild, deadline: Instant) -> io::Result<()> {
        if self.family.is_none() {
            self.enroll(child, deadline)?;
        }
        if self.family.is_some() {
            self.request(json!({"phase": "poll"}), deadline)?;
        }
        Ok(())
    }

    pub(super) fn before_reap(&mut self, group: Pid, signal: Result<(), Errno>) {
        self.records.push(json!({
            "phase": "owned-group-signal",
            "group": group.as_raw(),
            "result": format!("{signal:?}"),
            "receipt_us": self.origin.map(|origin| origin.elapsed().as_micros()),
            "leader_unreaped": true,
        }));
        if self.failure.is_some() || self.family.is_none() {
            return;
        }
        let deadline = Instant::now() + super::super::OBSERVATION_BOUND;
        let observation = self.capture_before_reap(group, deadline);
        if let Err(error) = observation {
            self.fail(&error);
        }
    }

    fn capture_before_reap(&mut self, group: Pid, deadline: Instant) -> io::Result<()> {
        let before = self.request(json!({"phase": "census"}), deadline)?;
        require_retained_leader(&before, group)?;
        let began = self.origin.map(|origin| origin.elapsed().as_micros());
        let diagnostic = killpg(group, None::<Signal>);
        self.records.push(json!({
            "phase": "before-reap-group-diagnostic",
            "group": group.as_raw(),
            "result": format!("{diagnostic:?}"),
            "receipt_started_us": began,
            "receipt_ended_us": self.origin.map(|origin| origin.elapsed().as_micros()),
            "leader_unreaped": true,
        }));
        let after = self.request(json!({"phase": "census"}), deadline)?;
        require_retained_leader(&after, group)?;
        loop {
            let observed = self.request(json!({"phase": "poll"}), deadline)?;
            if terminal_count(&observed) == 2 {
                return Ok(());
            }
            // Await actual prearmed events, never a deterministic-error retry.
            std::thread::yield_now();
        }
    }

    pub(super) fn finish(mut self) -> Value {
        let deadline = Instant::now() + super::super::OBSERVATION_BOUND;
        if self.failure.is_none() && self.family.is_some() {
            match self.request(json!({"phase": "finish"}), deadline) {
                Ok(reply) if terminal_count(&reply) == 2 => {
                    if let Err(error) = validate_owned_leader(&reply, self.owned_leader_status) {
                        self.fail(&error);
                    }
                    if let Some(owned) = self.owned_bun_status
                        && let Err(error) = validate_owned_caller(&reply, owned)
                    {
                        self.fail(&error);
                    }
                }
                Ok(_) => self.fail(&protocol_error("exact terminal statuses incomplete")),
                Err(error) => self.fail(&error),
            }
        }
        self.caller.take();
        self.peer.take();
        // Stop the family before any reaping wait, including observer death.
        let status = self.process.stop_unreaped_group(None);
        let controlled_exit = status
            .as_ref()
            .is_ok_and(|status| status.success() || status.signal() == Some(9));
        let observer_root = self.root.path().to_owned();
        let close = self.root.close();
        let absent = matches!(
            fs::symlink_metadata(&observer_root),
            Err(error) if error.kind() == io::ErrorKind::NotFound
        );
        let ok = self.family.is_some()
            && self.owned_bun_status.is_some()
            && self.failure.is_none()
            && controlled_exit
            && close.is_ok()
            && absent;
        json!({
            "ok": ok,
            "enrolled": self.family.is_some(),
            "failure": self.failure,
            "owned_caller_status": self.owned_bun_status.map(|(pid, raw)| json!({
                "pid": pid, "raw": raw,
            })),
            "observer_status": format!("{status:?}"),
            "observer_root_close": format!("{close:?}"),
            "observer_root_absent": absent,
            "records": self.records,
            "last_poll": self.last_poll,
            "stdout": captured(&mut self.stdout),
            "stderr": captured(&mut self.stderr),
        })
    }
}

fn registered_family(
    acknowledgment: &Value,
    leader: u32,
    bun: u32,
) -> io::Result<RegisteredFamily> {
    let receipts = acknowledgment["receipts"]
        .as_array()
        .ok_or_else(|| protocol_error("exact registration receipts missing"))?;
    if receipts.len() != 2 {
        return Err(protocol_error("exact registration receipt count mismatch"));
    }
    let mut filters = [0_i64; 2];
    for (index, pid) in [leader, bun].into_iter().enumerate() {
        let receipt = &receipts[index];
        let filter = receipt["filter"]
            .as_i64()
            .filter(|value| i16::try_from(*value).is_ok())
            .ok_or_else(|| protocol_error("registration filter shape rejected"))?;
        if receipt["ident"] != pid || receipt["data"] != 0 {
            return Err(protocol_error("registration identity/errno rejected"));
        }
        filters[index] = filter;
    }
    Ok(RegisteredFamily {
        leader,
        bun,
        filters,
    })
}

fn exact_fields(value: &Value, names: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == names.len() && names.iter().all(|name| object.contains_key(*name))
    })
}

fn required_i32(value: &Value, name: &str) -> io::Result<i32> {
    value
        .as_i64()
        .and_then(|raw| i32::try_from(raw).ok())
        .ok_or_else(|| protocol_error(format!("terminal {name} integer shape rejected")))
}

fn validate_terminal_record(record: &Value, family: RegisteredFamily) -> io::Result<()> {
    let states = record["states"]
        .as_array()
        .ok_or_else(|| protocol_error("exact process status array missing"))?;
    if states.len() != 2 {
        return Err(protocol_error("exact process status cardinality mismatch"));
    }
    for (index, expected) in [family.leader, family.bun].into_iter().enumerate() {
        let state = &states[index];
        if !exact_fields(state, &["pid", "terminal"]) || state["pid"] != expected {
            return Err(protocol_error(
                "registered process status identity mismatch",
            ));
        }
        let terminal = &state["terminal"];
        if terminal.is_null() {
            continue;
        }
        if !exact_fields(terminal, &["pid", "raw", "code", "signal", "event"])
            || terminal["pid"] != expected
        {
            return Err(protocol_error("terminal process identity/shape rejected"));
        }
        let event = &terminal["event"];
        if !exact_fields(event, &["ident", "filter", "flags", "fflags", "data"])
            || event["ident"] != expected
            || event["filter"] != family.filters[index]
            || event["flags"]
                .as_u64()
                .is_none_or(|flags| u16::try_from(flags).is_err())
            || event["fflags"]
                .as_u64()
                .is_none_or(|flags| u32::try_from(flags).is_err())
        {
            return Err(protocol_error("terminal event identity/shape rejected"));
        }
        let raw = required_i32(&terminal["raw"], "raw")?;
        if required_i32(&event["data"], "event.data")? != raw {
            return Err(protocol_error("terminal raw differs from exact event.data"));
        }
        let decoded = ExitStatus::from_raw(raw);
        if decoded.code().is_none() && decoded.signal().is_none() {
            return Err(protocol_error("kernel status is not terminal"));
        }
        if terminal["code"] != json!(decoded.code())
            || terminal["signal"] != json!(decoded.signal())
            || (!terminal["code"].is_null() && terminal["code"].as_i64().is_none())
            || (!terminal["signal"].is_null() && terminal["signal"].as_i64().is_none())
        {
            return Err(protocol_error(
                "terminal code/signal differs from raw status",
            ));
        }
    }
    Ok(())
}

fn validate_owned_leader(record: &Value, owned: Option<(u32, i32)>) -> io::Result<()> {
    let (leader, raw) =
        owned.ok_or_else(|| protocol_error("independently owned leader wait status missing"))?;
    let terminal = &record["states"][0]["terminal"];
    if terminal["pid"] != leader || terminal["raw"] != raw {
        return Err(protocol_error(
            "observed leader raw differs from independently owned Child exit status",
        ));
    }
    Ok(())
}

fn owned_caller_exit(
    detail: &str,
    family: Option<RegisteredFamily>,
    case: &str,
    token: &str,
) -> io::Result<Option<(Value, u32, i32)>> {
    let mut tagged = detail
        .lines()
        .filter_map(|line| line.strip_prefix(OWNED_CALLER_EXIT_TAG));
    let Some(line) = tagged.next() else {
        return Ok(None);
    };
    if tagged.next().is_some() {
        return Err(protocol_error(
            "duplicate independently owned caller exit receipt",
        ));
    }
    if OWNED_CALLER_EXIT_TAG.len() + line.len() + 1 > RECORD_LIMIT {
        return Err(protocol_error(
            "owned caller exit receipt exceeds its fixed bound",
        ));
    }
    let family =
        family.ok_or_else(|| protocol_error("owned caller receipt before registration"))?;
    let record: Value =
        serde_json::from_str(line).map_err(|error| protocol_error(error.to_string()))?;
    require_record(&record, "owned-caller-exit", token)?;
    if !exact_fields(&record, &["phase", "token", "case", "leader", "bun", "raw"])
        || record["case"] != case
        || record["leader"] != family.leader
        || record["bun"] != family.bun
    {
        return Err(protocol_error(
            "owned caller exit receipt identity/shape rejected",
        ));
    }
    let raw = required_i32(&record["raw"], "owned caller raw")?;
    Ok(Some((record, family.bun, raw)))
}

fn validate_owned_caller(record: &Value, owned: (u32, i32)) -> io::Result<()> {
    let (bun, raw) = owned;
    let terminal = &record["states"][1]["terminal"];
    if terminal["pid"] != bun || terminal["raw"] != raw {
        return Err(protocol_error(
            "observed Bun raw differs from independently owned caller exit status",
        ));
    }
    Ok(())
}

fn terminal_count(record: &Value) -> usize {
    record["states"].as_array().map_or(0, |states| {
        states
            .iter()
            .filter(|state| state["terminal"].is_object())
            .count()
    })
}

fn require_retained_leader(record: &Value, group: Pid) -> io::Result<()> {
    let members = record["census"]["members"]
        .as_array()
        .ok_or_else(|| protocol_error("complete before-reap member census missing"))?;
    if record["census"]["group"] != group.as_raw()
        || record["census"]["parent"]["pid"] != std::process::id()
        || !members.iter().any(|member| member["pid"] == group.as_raw())
    {
        return Err(protocol_error(
            "retained leader absent from before-reap census",
        ));
    }
    Ok(())
}

pub(super) fn record_owned_caller(case: &str, child: &std::process::Child) {
    let pid = Pid::from_raw(i32::try_from(child.id()).expect("owned Bun PID fits i32"));
    let group = getpgid(Some(pid)).expect("inspect owned Bun group");
    assert_eq!(
        group,
        getpgrp(),
        "owned caller left the native child's group"
    );
    println!("KELD_AC7_OWNED_BUN case={case} pid={pid} group={group}");
    let token = std::env::var(OBSERVER_TOKEN_ENV).expect("parent observer token");
    let socket = std::env::var_os(OBSERVER_SOCKET_ENV).expect("parent observer endpoint");
    let stream = UnixStream::connect(socket).expect("owned parent enrollment");
    let mut peer = ControlPeer::new(stream).expect("bounded enrollment peer");
    let deadline = Instant::now() + super::super::OBSERVATION_BOUND;
    peer.send(
        &json!({
            "phase": "enroll",
            "token": token,
            "case": case,
            "leader": std::process::id(),
            "bun": child.id(),
            "group": group.as_raw(),
        }),
        deadline,
    )
    .expect("send current owned family");
    let acknowledgment = peer
        .read(deadline)
        .expect("required PREARMED acknowledgment");
    require_record(&acknowledgment, "prearmed", &token).expect("current PREARMED token");
    assert_eq!(acknowledgment["case"], case);
    assert_eq!(acknowledgment["leader"], std::process::id());
    assert_eq!(acknowledgment["bun"], child.id());
    assert_eq!(acknowledgment["group"], group.as_raw());
    println!(
        "KELD_AC7_PROCESS_PREARMED case={case} leader={} bun={pid} group={group}",
        std::process::id()
    );
}

pub(super) fn record_owned_caller_exit(case: &str, pid: u32, status: ExitStatus) {
    let token = std::env::var(OBSERVER_TOKEN_ENV).expect("parent observer token");
    let record = json!({
        "phase": "owned-caller-exit", "token": token, "case": case,
        "leader": std::process::id(), "bun": pid, "raw": status.into_raw(),
    });
    let bytes = serde_json::to_string(&record).expect("owned caller exit JSON");
    assert!(
        OWNED_CALLER_EXIT_TAG.len() + bytes.len() < RECORD_LIMIT,
        "owned caller exit receipt exceeds its fixed bound"
    );
    eprintln!("{OWNED_CALLER_EXIT_TAG}{bytes}");
}
