//! GH-1081 round-4 research only (never merged): the test-side owner of the
//! `gh1081_origin_lab` harness (`crates/keld-wv/examples/gh1081_origin_lab.rs`),
//! an isolated webview process that loads one in-memory page through a
//! candidate app-origin transport (KEL-79, owner decision K2 §B).
//!
//! The test owns the control socket the harness connects back to, the page
//! probe (round-2 `Probe`), a "Bun stand-in" resource producer for the
//! crash-recovery byte-source atom (G2 §3.5 C1) and the per-arm orchestration
//! shared by the three OS modules; each OS module supplies only its window
//! lookup, pixel capture and OS input (`Driver`).
//!
//! No production code is involved. The real host loads `NavTarget::Html` only
//! (`crates/keld-core/src/app_session.rs`), so the real-host crash-recovery
//! rows (X3) run only the inline control arm; the candidate arms' X3 evidence
//! here is the byte-source atom in the harness, recorded as such.
//!
//! Consumers: macOS/Windows/Linux `origin_r4.rs`.

// Shared by three OS binaries that each use a subset.
#![allow(dead_code)]

use crate::crash_recovery_r2_probe::{Probe, await_successor, is_page_fetch, is_unload};
use serde_json::{Map, Value, json};
use std::fs;
use std::io::{BufRead as _, BufReader, ErrorKind, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Bound for "a reloaded document reports"; the real-URL control's successor
/// arrives far inside it (printed per arm as `reload_ms`).
pub(crate) const SUCCESSOR_BOUND: Duration = Duration::from_secs(10);

/// Kill switch for one harness launch, page load and control exchange.
pub(crate) const LAB_DEADLINE: Duration = Duration::from_secs(40);

/// The app host of the placeholder origin (`<scheme>://app-a/`).
pub(crate) const APP_HOST: &str = "app-a";

/// The load path under test (one harness process each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arm {
    /// A0 / T4d: today's URL-less HTML string (control).
    Inline,
    /// A1 / T1: a per-app custom scheme served from the harness's memory.
    Scheme,
    /// Windows negative control: the same scheme with no authority component.
    SchemeNoAuthority,
    /// A3 / T3 (Windows only): a host-backed `https://<host>.invalid/` URL.
    Https,
}

impl Arm {
    pub(crate) fn flag(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Scheme => "scheme",
            Self::SchemeNoAuthority => "scheme-noauth",
            Self::Https => "https",
        }
    }
}

/// How the reload is triggered after the page is idle and painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// Page JS `location.reload()` after a released beat.
    JsReload,
    /// The engine's own reload API through the harness (`WKWebView.reload`,
    /// `webkit_web_view_reload`, `ICoreWebView2::Reload`).
    ApiReload,
    /// Real OS key events; the name is per OS (`cmd-r`, `{F5}`, `ctrl+r`, ...).
    Key(&'static str),
    /// The engine's context-menu Reload item, opened by a real right click.
    ContextMenu,
    /// Negative control: the inline page navigates to a real `http://` URL
    /// served by the probe, then reloads there.
    RealUrlControl,
}

fn nonce() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    u64::try_from(nanos & u128::from(u64::MAX)).unwrap_or(0) ^ u64::from(std::process::id())
}

/// A placeholder scheme name per test process. The owner has not approved a
/// final name (K2 §B); the name is reported with every record.
pub(crate) fn placeholder_scheme() -> String {
    format!("keld-exp-{:08x}", nonce() & 0xffff_ffff)
}

fn drain<R: std::io::Read + Send + 'static>(reader: Option<R>) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut reader) = reader {
            let _ = reader.read_to_string(&mut text);
        }
        text
    })
}

/// `KIND k=v k=v` → `{"kind": KIND, k: v, ...}` (values carry no spaces).
pub(crate) fn fields(line: &str) -> Value {
    let mut map = Map::new();
    let mut parts = line.split(' ');
    map.insert(
        String::from("kind"),
        Value::String(parts.next().unwrap_or_default().to_owned()),
    );
    for part in parts {
        if let Some((key, value)) = part.split_once('=') {
            map.insert(key.to_owned(), Value::String(value.to_owned()));
        }
    }
    Value::Object(map)
}

/// Sole owner of one harness process and its control connection.
pub(crate) struct Lab {
    child: Option<Child>,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    stdout: Option<JoinHandle<String>>,
    stderr: Option<JoinHandle<String>>,
    pub(crate) pid: u32,
    pub(crate) hello: Value,
    /// Spontaneous `EV ...` lines in arrival order.
    pub(crate) events: Vec<String>,
}

impl Lab {
    /// The harness binary: `GH1081_ORIGIN_LAB` (set by the workflow) or the
    /// sibling `examples/` directory of this test executable.
    pub(crate) fn binary() -> PathBuf {
        if let Some(path) = std::env::var_os("GH1081_ORIGIN_LAB") {
            let path = PathBuf::from(path);
            if path.is_file() {
                return path;
            }
            eprintln!(
                "KELD_GH1081_R4_LAB_BINARY env path is not a file, deriving from the test executable: {}",
                path.display()
            );
        }
        let exe = std::env::current_exe().expect("test executable path");
        let target = exe
            .parent()
            .and_then(Path::parent)
            .expect("target profile directory");
        target
            .join("examples")
            .join(format!("gh1081_origin_lab{}", std::env::consts::EXE_SUFFIX))
    }

    pub(crate) fn launch(
        arm: Arm,
        page: &Path,
        title: &str,
        scheme: &str,
        user_data: Option<&Path>,
        deadline: Instant,
    ) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind lab control");
        listener
            .set_nonblocking(true)
            .expect("nonblocking lab control");
        let port = listener.local_addr().expect("lab control address").port();
        let binary = Self::binary();
        assert!(
            binary.is_file(),
            "origin lab binary missing at {}; build it with `cargo build -p keld-wv --example gh1081_origin_lab` or set GH1081_ORIGIN_LAB",
            binary.display()
        );
        let mut command = Command::new(binary);
        command
            .args(["--arm", arm.flag(), "--title", title])
            .arg("--control")
            .arg(format!("127.0.0.1:{port}"))
            .args(["--scheme", scheme, "--host", APP_HOST])
            .arg("--page")
            .arg(page);
        if let Some(directory) = user_data {
            command.arg("--user-data").arg(directory);
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn origin lab");
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let stream = match accept_before(&listener, &mut child, deadline) {
            Ok(stream) => stream,
            Err(reason) => {
                let _ = child.kill();
                let _ = child.wait();
                let out = stdout.join().unwrap_or_default();
                let err = stderr.join().unwrap_or_default();
                panic!("{reason}; lab stdout={out:?} stderr={err:?}");
            }
        };
        // BSD accept() inherits the listener's O_NONBLOCK; read deadlines need
        // a blocking stream (learnings 2026-08-24).
        stream
            .set_nonblocking(false)
            .expect("blocking lab control stream");
        stream.set_nodelay(true).expect("lab control nodelay");
        let writer = stream.try_clone().expect("lab control writer");
        let mut lab = Self {
            child: Some(child),
            reader: BufReader::new(stream),
            writer,
            stdout: Some(stdout),
            stderr: Some(stderr),
            pid: 0,
            hello: Value::Null,
            events: Vec::new(),
        };
        let hello = lab
            .read_line(deadline)
            .unwrap_or_else(|| panic!("lab HELLO before deadline; {}", lab.failure_context()));
        assert!(hello.starts_with("HELLO "), "{hello}");
        lab.hello = fields(&hello);
        lab.pid = lab.hello["pid"]
            .as_str()
            .and_then(|pid| pid.parse().ok())
            .expect("lab pid");
        lab
    }

    fn failure_context(&mut self) -> String {
        let status = self
            .child
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten())
            .map(|status| status.to_string());
        format!("lab status={status:?} events={:?}", self.events)
    }

    /// One control line before `deadline`; `None` on EOF or deadline.
    fn read_line(&mut self, deadline: Instant) -> Option<String> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        self.reader
            .get_ref()
            .set_read_timeout(Some(remaining))
            .expect("lab read deadline");
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(line.trim_end().to_owned()),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                None
            }
            Err(error) => panic!("lab control read: {error}"),
        }
    }

    fn stash(&mut self, line: String) {
        eprintln!("KELD_GH1081_R4_LAB_EVENT pid={} {line}", self.pid);
        self.events.push(line);
    }

    /// Sends one command and returns its reply payload (`RE <command> <payload>`);
    /// spontaneous events arriving first are stashed. `None` on deadline or EOF.
    pub(crate) fn command(&mut self, command: &str, deadline: Instant) -> Option<String> {
        writeln!(self.writer, "{command}").expect("lab command");
        self.writer.flush().expect("flush lab command");
        let prefix = format!("RE {command}");
        loop {
            let line = self.read_line(deadline)?;
            if let Some(reply) = line.strip_prefix(&prefix) {
                return Some(reply.trim_start().to_owned());
            }
            self.stash(line);
        }
    }

    /// The first event line starting with `prefix` before `deadline`.
    pub(crate) fn await_event(&mut self, prefix: &str, deadline: Instant) -> Option<String> {
        loop {
            let line = self.read_line(deadline)?;
            let matched = line.starts_with(prefix);
            self.stash(line);
            if matched {
                return self.events.last().cloned();
            }
        }
    }

    /// Collects events already in flight: returns once `settle` passes with no
    /// new line.
    pub(crate) fn drain_events(&mut self, settle: Duration) {
        while let Some(line) = self.read_line(Instant::now() + settle) {
            self.stash(line);
        }
    }

    pub(crate) fn events_with(&self, prefix: &str) -> Vec<String> {
        self.events
            .iter()
            .filter(|line| line.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Quits through the control plane (kill as the fallback) and returns the
    /// exit status, captured output and every event.
    pub(crate) fn quit(mut self, deadline: Instant) -> Value {
        let acknowledged = self.command("quit", deadline);
        let mut child = self.child.take().expect("lab child");
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status.to_string()),
                Ok(None) if Instant::now() < deadline => {
                    thread::park_timeout(Duration::from_millis(20));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let stdout = self
            .stdout
            .take()
            .map(|handle| handle.join().unwrap_or_default());
        let stderr = self
            .stderr
            .take()
            .map(|handle| handle.join().unwrap_or_default());
        json!({
            "quit_ack": acknowledged, "status": status, "stdout": stdout, "stderr": stderr,
            "events": std::mem::take(&mut self.events),
        })
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn accept_before(
    listener: &TcpListener,
    child: &mut Child,
    deadline: Instant,
) -> Result<TcpStream, String> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("accept lab control: {error}")),
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("origin lab exited before connecting: {status}"));
        }
        if Instant::now() >= deadline {
            return Err(String::from(
                "origin lab did not connect before the deadline",
            ));
        }
        thread::park_timeout(Duration::from_millis(10));
    }
}

/// A test-owned HTTP responder standing in for a Bun-owned resource server.
/// While it runs, a no-cors fetch to it succeeds; after `stop`, the port is
/// closed and the same fetch must fail. That is the negative control for the
/// byte-source atom: the oracle must see a dependency on a dead producer.
pub(crate) struct Producer {
    port: u16,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<usize>>,
}

impl Producer {
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind producer");
        listener
            .set_nonblocking(true)
            .expect("nonblocking producer");
        let port = listener.local_addr().expect("producer address").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let mut served = 0;
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut request = [0_u8; 1024];
                        let _ = stream.read(&mut request);
                        let _ = stream.write_all(
                            b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                        );
                        served += 1;
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::park_timeout(Duration::from_millis(10));
                    }
                    Err(error) => panic!("producer accept: {error}"),
                }
            }
            served
        });
        Self {
            port,
            stop,
            worker: Some(worker),
        }
    }

    pub(crate) fn url(&self) -> String {
        format!("http://127.0.0.1:{}/bun-stand-in", self.port)
    }

    /// Stops serving and closes the port; returns how many requests it answered.
    pub(crate) fn stop(mut self) -> usize {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .map_or(0, |worker| worker.join().expect("producer joins"))
    }
}

impl Drop for Producer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Per-OS window identity, read-only pixels and real OS input.
pub(crate) trait Driver {
    /// The harness's on-screen window once it exists (OS-specific identity).
    fn window(&mut self, pid: u32, title: &str, deadline: Instant) -> Value;
    /// Read-only capture of the window's content (`dark`, `light`, `center`).
    fn pixels(&mut self, window: &Value, label: &str) -> Value;
    /// Drives one `Key` or `ContextMenu` trigger; returns the input record.
    fn input(
        &mut self,
        lab: &mut Lab,
        window: &Value,
        geometry: &Value,
        title: &str,
        trigger: Trigger,
    ) -> Value;
    /// A fresh engine data directory per launch where the engine needs one.
    fn user_data(&mut self) -> Option<PathBuf> {
        None
    }
}

/// Reported-only X1 facts from one beat.
pub(crate) fn beat_facts(beat: &Value) -> Value {
    json!({
        "href": beat["href"], "origin": beat["origin"], "secure": beat["secure"],
        "subtle": beat["subtle"], "ss": beat["ss"], "ls": beat["ls"], "idb": beat["idb"],
        "nav": beat["nav"], "wname": beat["wname"], "bridge": beat["bridge"], "ua": beat["ua"],
    })
}

/// Walks the baked plan up to the idle, painted `before` document.
fn walk_steps(
    probe: &mut Probe,
    first: &Value,
    trigger: Trigger,
    producer: Producer,
    deadline: Instant,
) -> (Value, Vec<Value>, Value) {
    if trigger == Trigger::RealUrlControl {
        // Release beat 0: the inline page navigates to the real http:// URL.
        probe.release();
        let real_page = await_successor(probe, first["doc"].as_str().unwrap_or(""), deadline)
            .expect("real-URL control page reported");
        let before = probe.poll(deadline);
        drop(producer);
        return (real_page, Vec::new(), before);
    }
    // Step 0 `beat`: a live document after one round trip.
    let after_beat = probe.poll(deadline);
    // Step 1 `res`: a same-origin resource from the transport under test.
    let same_origin = probe.poll(deadline);
    // Step 2 `resx`: the stand-in producer, alive.
    let producer_alive = probe.poll(deadline);
    let producer_served = producer.stop();
    // Step 3 `resx`: the same URL with the producer gone ("Bun is dead").
    let producer_dead = probe.poll(deadline);
    let steps = vec![
        json!({"step": "beat", "beat": after_beat}),
        json!({"step": "res", "beat": same_origin}),
        json!({"step": "resx-alive", "producer_served": producer_served, "beat": producer_alive}),
        json!({"step": "resx-dead", "beat": producer_dead}),
    ];
    (Value::Null, steps, producer_dead)
}

/// One fresh harness launch, one reload trigger, every observation recorded.
pub(crate) fn run_arm(
    driver: &mut impl Driver,
    scaffold: &str,
    scheme: &str,
    arm: Arm,
    trigger: Trigger,
) -> Value {
    let scratch = tempfile::tempdir().expect("origin lab scratch");
    let producer = Producer::start();
    let resx = format!("resx:{}", producer.url());
    let resx = resx.as_str();
    let stage: Vec<&str> = match trigger {
        Trigger::RealUrlControl => vec!["real"],
        Trigger::JsReload => vec!["beat", "res", resx, resx, "reload"],
        _ => vec!["beat", "res", resx, resx],
    };
    let real: &[&str] = if trigger == Trigger::RealUrlControl {
        &["beat", "reload"]
    } else {
        &[]
    };
    let mut probe = Probe::bind(scaffold, &stage, real, false);
    let page = scratch.path().join("index.html");
    fs::write(&page, probe.page()).expect("write lab page");
    let title = format!("GH1081 R4 {} {:x}", std::process::id(), nonce());
    let deadline = Instant::now() + LAB_DEADLINE;
    let user_data = driver.user_data();
    let mut lab = Lab::launch(arm, &page, &title, scheme, user_data.as_deref(), deadline);
    let first = probe.beat(deadline);
    let window = driver.window(lab.pid, &title, deadline);
    let (real_page, steps, before) = walk_steps(&mut probe, &first, trigger, producer, deadline);
    let pixels_before = driver.pixels(&window, "before");
    let url_before = lab.command("url", deadline);
    let served_before = lab.command("served", deadline);
    let triggered = Instant::now();
    let input = match trigger {
        Trigger::JsReload | Trigger::RealUrlControl => {
            probe.release();
            Value::Null
        }
        Trigger::ApiReload => json!({"api_reload": lab.command("reload", deadline)}),
        Trigger::Key(_) | Trigger::ContextMenu => {
            driver.input(&mut lab, &window, &pixels_before, &title, trigger)
        }
    };
    let previous = before["doc"].as_str().unwrap_or("").to_owned();
    let successor = await_successor(&mut probe, &previous, triggered + SUCCESSOR_BOUND);
    let reload_ms = successor.as_ref().map(|_| triggered.elapsed().as_millis());
    while probe.try_next(Instant::now()).is_some() {}
    let pixels_after = driver.pixels(&window, "after");
    let url_after = lab.command("url", deadline);
    let served_after = lab.command("served", deadline);
    lab.drain_events(Duration::from_millis(300));
    let hello = lab.hello.clone();
    let pid = lab.pid;
    let page_loads = lab.events_with("EV load");
    let navigations = lab.events_with("EV nav");
    let shutdown = lab.quit(deadline);
    let record = json!({
        "arm": arm.flag(), "trigger": format!("{trigger:?}"), "scheme": scheme, "lab_pid": pid,
        "hello": hello, "window": window, "first": first, "real_page": real_page, "steps": steps,
        "before": before, "x1": beat_facts(&before), "pixels_before": pixels_before,
        "url_before": url_before, "served_before": served_before, "input": input,
        "successor": successor, "reload_ms": reload_ms, "pixels_after": pixels_after,
        "url_after": url_after, "served_after": served_after, "page_loads": page_loads,
        "navigations": navigations, "unload_beacons": probe.count(is_unload),
        "real_page_fetches": probe.count(is_page_fetch), "documents": probe.documents(),
        "lab": shutdown,
    });
    eprintln!("KELD_GH1081_R4_ARM {record}");
    record
}

/// One compact row per record for the per-OS summary line.
pub(crate) fn summarize(records: &[Value]) -> Value {
    Value::Array(
        records
            .iter()
            .map(|record| {
                let steps = record["steps"].as_array();
                let res = |index: usize| {
                    steps
                        .and_then(|steps| steps.get(index))
                        .map_or(Value::Null, |step| step["beat"]["res"].clone())
                };
                json!({
                    "arm": record["arm"], "trigger": record["trigger"],
                    "origin": record["x1"]["origin"], "href": record["x1"]["href"],
                    "secure": record["x1"]["secure"], "ss_ok": record["x1"]["ss"]["ok"],
                    "ss_error": record["x1"]["ss"]["error"], "ls_ok": record["x1"]["ls"]["ok"],
                    "idb": record["x1"]["idb"], "res_same_origin": res(1),
                    "res_producer_alive": res(2), "res_producer_dead": res(3),
                    "successor": record["successor"].is_object(),
                    "successor_nav": record["successor"]["nav"],
                    "successor_href": record["successor"]["href"],
                    "successor_ss_prev": record["successor"]["ss"]["prev"],
                    "before_doc": record["before"]["doc"],
                    "reload_ms": record["reload_ms"], "url_after": record["url_after"],
                    "served_before": record["served_before"], "served_after": record["served_after"],
                    "center_before": record["pixels_before"]["center"],
                    "center_after": record["pixels_after"]["center"],
                    "light_after": record["pixels_after"]["light"],
                    "unload_beacons": record["unload_beacons"], "documents": record["documents"],
                })
            })
            .collect(),
    )
}

/// Oracle-validity assertions only; every candidate outcome is recorded, not
/// asserted, because K2 §B forbids selecting an architecture from this run.
pub(crate) fn assert_oracles(records: &[Value]) {
    for record in records {
        assert_eq!(
            record["lab"]["quit_ack"], "",
            "harness acknowledged quit: {record}"
        );
        if record["trigger"] == "RealUrlControl" {
            assert!(
                record["successor"]["real"] == true && record["successor"]["nav"] == "reload",
                "NC: the real-URL page did not reload into a new document: {record}"
            );
            assert_eq!(
                record["real_page_fetches"], 2,
                "NC: reload re-fetched the URL: {record}"
            );
            continue;
        }
        let res = |index: usize| {
            record["steps"][index]["beat"]["res"]
                .as_str()
                .unwrap_or("")
                .to_owned()
        };
        assert!(
            res(2).starts_with("ok:"),
            "NC: fetch to the live stand-in producer failed: {record}"
        );
        assert!(
            res(3).starts_with("err:"),
            "NC: fetch to the dead stand-in producer did not fail: {record}"
        );
        if record["arm"] == "inline" {
            assert_eq!(
                record["x1"]["ss"]["error"], "SecurityError",
                "A0 oracle: sessionStorage must throw on the URL-less page: {record}"
            );
        }
        if record["arm"] == "scheme-noauth" {
            assert_eq!(
                record["x1"]["origin"], "null",
                "NC: a no-authority custom scheme must give an opaque origin: {record}"
            );
        }
    }
}

/// Runs one program and returns its combined output or the spawn error.
pub(crate) fn command_output(program: &str, args: &[&str]) -> String {
    match Command::new(program).args(args).output() {
        Ok(output) => format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .trim()
        .to_owned(),
        Err(error) => format!("error: {error}"),
    }
}

/// X0 provenance shared by every OS: the hosted-runner image, the job, the
/// toolchain and the harness binary.
pub(crate) fn runner_census() -> Value {
    let env = |key: &str| std::env::var(key).ok();
    json!({
        "image_os": env("ImageOS"), "image_version": env("ImageVersion"),
        "runner_os": env("RUNNER_OS"), "runner_arch": env("RUNNER_ARCH"),
        "runner_name": env("RUNNER_NAME"), "github_sha": env("GITHUB_SHA"),
        "github_run_id": env("GITHUB_RUN_ID"), "github_job": env("GITHUB_JOB"),
        "rustc": command_output("rustc", &["-V"]),
        "lab_binary": Lab::binary(),
    })
}
