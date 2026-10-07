//! PANEL-P1 scratch host (gyldlab/keld#418). Scratch prototype, not product code.
//!
//! Drives the REAL keld-ipc public entry points over a Unix socket:
//! - `keld_ipc::link::handshake_server` (v2 HELLO with a `SessionToken`),
//! - `keld_ipc::link::write_frame` on a blocking `UnixStream` whose
//!   `SO_SNDTIMEO` is `APP_LINK_IO_DEADLINE` (set through `AppLinkDeadlines`),
//!   so a full socket buffer surfaces as `IpcError::Timeout` = `KELD-IPC-006`,
//! - `keld_ipc::link::read_frame_interruptible` with `APP_LINK_READER_POLL`,
//! - `keld_ipc::codec::{encode, decode}` (postcard), `echo::handle_echo`,
//!   `LifecycleRequest::Quit` / `LifecycleResponse::Quit`.
//!
//! Scratch-only definitions (not in the v0 wire schema): EVENTs ride channel 9
//! with a postcard `(seq: u32, pad: Vec<u8>)` payload sized so every EVENT frame is
//! exactly 64 bytes; GRANT carries a postcard `u32` frame credit on channel 9.

use std::collections::VecDeque;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use keld_ipc::codec::{decode, encode};
use keld_ipc::echo::handle_echo;
use keld_ipc::link::{AppLinkDeadlines, handshake_server, read_frame_interruptible, write_frame};
use keld_ipc::{
    APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL, ChannelId, CorrelationId, ECHO_CHANNEL,
    EchoRequest, FrameKind, HEADER_LEN, IpcError, LIFECYCLE_CHANNEL, LifecycleRequest,
    LifecycleResponse, SessionToken, format_app_link,
};

const PROBE_CHANNEL: ChannelId = ChannelId(9);
const EVENT_PAYLOAD_LEN: usize = 48; // 16-byte header + 48 = 64-byte EVENT frame
const STALL_US: u128 = 250_000;

fn wall_us() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0)
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

struct Cfg {
    arm: String,
    sock: String,
    burst: u32,
    rate: u64,
    steady_ms: u64,
    retire_after_ms: u64,
    credit_window: u32,
}

fn parse_cfg() -> Result<Cfg, String> {
    let mut cfg = Cfg {
        arm: "b".into(),
        sock: "s.sock".into(),
        burst: 10_000,
        rate: 100,
        steady_ms: 10_000,
        retire_after_ms: 2_000,
        credit_window: 0,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i + 1 < args.len() {
        let v = args[i + 1].clone();
        let num = |s: &str| s.parse::<u64>().map_err(|e| format!("bad number {s}: {e}"));
        match args[i].as_str() {
            "--arm" => cfg.arm = v,
            "--sock" => cfg.sock = v,
            "--burst" => cfg.burst = num(&v)? as u32,
            "--rate" => cfg.rate = num(&v)?.max(1),
            "--steady-ms" => cfg.steady_ms = num(&v)?,
            "--retire-after-ms" => cfg.retire_after_ms = num(&v)?,
            "--credit-window" => cfg.credit_window = num(&v)? as u32,
            other => return Err(format!("unknown arg {other}")),
        }
        i += 2;
    }
    Ok(cfg)
}

enum Cmd {
    Call {
        channel: ChannelId,
        corr: CorrelationId,
        payload: Vec<u8>,
    },
    Grant(u32),
    Ended(String),
}

struct Writer {
    ws: UnixStream,
    rx: mpsc::Receiver<Cmd>,
    deferred: VecDeque<Cmd>,
    start: Instant,
    bytes: u64,
    frames: u64,
    events: u32,
    max_block_us: u128,
    write_stalls: u64,
    credit_gated: bool,
    credit: u64,
    credit_waits: u64,
    credit_wait_max_us: u128,
    credit_wait_total_us: u128,
    credit_stalls: u64,
    error: Option<String>,
    ipc006: bool,
    ended: Option<String>,
}

impl Writer {
    fn t_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    fn write_raw(&mut self, kind: FrameKind, channel: ChannelId, corr: CorrelationId, payload: &[u8]) -> bool {
        if self.error.is_some() {
            return false;
        }
        let t = Instant::now();
        let r = write_frame(&mut self.ws, kind, 0, channel, corr, payload);
        let us = t.elapsed().as_micros();
        match r {
            Ok(()) => {
                self.max_block_us = self.max_block_us.max(us);
                if us >= STALL_US {
                    self.write_stalls += 1;
                }
                self.bytes += (HEADER_LEN + payload.len()) as u64;
                self.frames += 1;
                true
            }
            Err(e) => {
                self.ipc006 = matches!(e, IpcError::Timeout);
                let msg = e.to_string();
                println!(
                    "{{\"ev\":\"write_error\",\"error\":\"{}\",\"ipc006\":{},\"blocked_ms\":{},\"bytes_before\":{},\"frames_before\":{},\"events_before\":{},\"t_ms\":{},\"wall_us\":{}}}",
                    esc(&msg),
                    self.ipc006,
                    us / 1000,
                    self.bytes,
                    self.frames,
                    self.events,
                    self.t_ms(),
                    wall_us()
                );
                self.error = Some(msg);
                false
            }
        }
    }

    fn absorb_nonblocking(&mut self) {
        while let Ok(c) = self.rx.try_recv() {
            match c {
                Cmd::Grant(n) => self.credit += u64::from(n),
                other => self.deferred.push_back(other),
            }
        }
    }

    fn wait_credit(&mut self) -> bool {
        let t = Instant::now();
        self.credit_waits += 1;
        loop {
            match self.rx.recv_timeout(Duration::from_secs(60)) {
                Ok(Cmd::Grant(n)) => {
                    self.credit += u64::from(n);
                    break;
                }
                Ok(Cmd::Ended(why)) => {
                    self.ended = Some(why);
                    return false;
                }
                Ok(other) => self.deferred.push_back(other),
                Err(_) => {
                    self.ended = Some("credit starved for 60 s (prototype bound)".into());
                    return false;
                }
            }
        }
        let us = t.elapsed().as_micros();
        self.credit_wait_total_us += us;
        self.credit_wait_max_us = self.credit_wait_max_us.max(us);
        if us >= STALL_US {
            self.credit_stalls += 1;
        }
        true
    }

    fn has_credit(&mut self) -> bool {
        self.absorb_nonblocking();
        self.credit > 0
    }

    fn write_event(&mut self) -> bool {
        if self.credit_gated {
            while self.credit == 0 {
                if !self.wait_credit() {
                    return false;
                }
            }
            self.credit -= 1;
        }
        let seq = self.events;
        let base = match encode(&(seq, Vec::<u8>::new())) {
            Ok(b) => b.len(),
            Err(e) => {
                self.error = Some(e.to_string());
                return false;
            }
        };
        let payload = match encode(&(seq, vec![0xA5u8; EVENT_PAYLOAD_LEN - base])) {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(e.to_string());
                return false;
            }
        };
        debug_assert_eq!(payload.len(), EVENT_PAYLOAD_LEN);
        let ok = self.write_raw(FrameKind::Event, PROBE_CHANNEL, CorrelationId(0), &payload);
        if ok {
            self.events += 1;
        }
        ok
    }
}

/// Returns false when the session ended (writer error or generation retire).
fn run_scenario(w: &mut Writer, cfg: &Cfg, corr: CorrelationId, payload: &[u8]) -> bool {
    println!(
        "{{\"ev\":\"scenario_start\",\"t_ms\":{},\"wall_us\":{}}}",
        w.t_ms(),
        wall_us()
    );
    let tb = Instant::now();
    let bc = cfg.arm == "bc";
    let mut backlog: u64 = 0;
    for _ in 0..cfg.burst {
        if bc && !w.has_credit() {
            backlog += 1; // producer suspended at zero credit; the link stays writable
            continue;
        }
        if !w.write_event() {
            return false;
        }
    }
    println!(
        "{{\"ev\":\"burst_done\",\"burst_ms\":{},\"bytes\":{},\"events\":{},\"max_block_us\":{},\"wall_us\":{}}}",
        tb.elapsed().as_millis(),
        w.bytes,
        w.events,
        w.max_block_us,
        wall_us()
    );
    let retire = cfg.arm == "e-retire";
    let retire_after = Duration::from_millis(cfg.retire_after_ms);
    let steady_n = cfg.rate * cfg.steady_ms / 1000;
    let start = Instant::now();
    for i in 0..steady_n {
        let due = start + Duration::from_micros(i * 1_000_000 / cfg.rate);
        if retire && due >= start + retire_after {
            break;
        }
        let now = Instant::now();
        if due > now {
            // Load pacing for the 100 EVENT/s steady stream; not synchronization.
            thread::sleep(due - now);
        }
        if bc && !w.has_credit() {
            backlog += 1;
            continue;
        }
        if !w.write_event() {
            return false;
        }
        w.absorb_nonblocking();
    }
    if retire {
        let now = Instant::now();
        if start + retire_after > now {
            thread::sleep(start + retire_after - now);
        }
        println!(
            "{{\"ev\":\"retire\",\"events_written\":{},\"bytes\":{},\"t_ms\":{},\"wall_us\":{}}}",
            w.events,
            w.bytes,
            w.t_ms(),
            wall_us()
        );
        // v0 has no wire-level retire message: the host retiring the role
        // generation is the host revoking (shutting down) that generation's link.
        let _ = w.ws.shutdown_app_link();
        w.ended = Some("generation retired by host".into());
        return false;
    }
    println!(
        "{{\"ev\":\"steady_done\",\"events\":{},\"bytes\":{},\"wall_us\":{}}}",
        w.events,
        w.bytes,
        wall_us()
    );
    let reply = match handle_echo(payload) {
        Ok(r) => r,
        Err(e) => {
            w.error = Some(e.to_string());
            return false;
        }
    };
    let ok = w.write_raw(FrameKind::Reply, ECHO_CHANNEL, corr, &reply);
    println!(
        "{{\"ev\":\"reply_written\",\"ok\":{},\"backlog\":{},\"t_ms\":{},\"wall_us\":{}}}",
        ok,
        backlog,
        w.t_ms(),
        wall_us()
    );
    // A credit-suspended producer resumes only as the consumer grants credit.
    let tr = Instant::now();
    while ok && backlog > 0 {
        if !w.write_event() {
            return false;
        }
        backlog -= 1;
    }
    if bc {
        println!(
            "{{\"ev\":\"backlog_done\",\"resume_ms\":{},\"events\":{},\"wall_us\":{}}}",
            tr.elapsed().as_millis(),
            w.events,
            wall_us()
        );
    }
    ok
}

fn run() -> Result<(), String> {
    let cfg = parse_cfg()?;
    let token = SessionToken::random().map_err(|e| e.to_string())?;
    let listener = UnixListener::bind(&cfg.sock).map_err(|e| format!("bind {}: {e}", cfg.sock))?;
    println!(
        "{{\"ev\":\"listening\",\"link\":\"{}\",\"arm\":\"{}\",\"burst\":{},\"rate\":{},\"steady_ms\":{},\"credit_window\":{},\"io_deadline_ms\":{},\"reader_poll_ms\":{}}}",
        esc(&format_app_link(&cfg.sock, &token)),
        esc(&cfg.arm),
        cfg.burst,
        cfg.rate,
        cfg.steady_ms,
        cfg.credit_window,
        APP_LINK_IO_DEADLINE.as_millis(),
        APP_LINK_READER_POLL.as_millis()
    );
    let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
    stream
        .set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
        .map_err(|e| e.to_string())?;
    handshake_server(&mut stream, &token).map_err(|e| e.to_string())?;
    // The locator is consumed at successful authentication: one link per role
    // generation. A second connect attempt finds no endpoint.
    drop(listener);
    let _ = std::fs::remove_file(&cfg.sock);
    println!("{{\"ev\":\"authenticated\",\"wall_us\":{}}}", wall_us());

    // Reader-only poll (SO_RCVTIMEO); SO_SNDTIMEO stays APP_LINK_IO_DEADLINE.
    stream
        .set_app_link_read_deadline(Some(APP_LINK_READER_POLL))
        .map_err(|e| e.to_string())?;
    let mut rs = stream.try_clone().map_err(|e| e.to_string())?;
    let ws = stream.try_clone().map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::sync_channel::<Cmd>(4096);
    let rstop = Arc::clone(&stop);
    let reader = thread::spawn(move || {
        loop {
            match read_frame_interruptible(&mut rs, &rstop) {
                Ok(Some((h, p))) => {
                    let cmd = match h.kind {
                        FrameKind::Call => Cmd::Call {
                            channel: h.channel,
                            corr: h.corr,
                            payload: p,
                        },
                        FrameKind::Grant if h.channel == PROBE_CHANNEL => match decode::<u32>(&p) {
                            Ok(n) => Cmd::Grant(n),
                            Err(e) => Cmd::Ended(e.to_string()),
                        },
                        FrameKind::Ping => continue,
                        other => Cmd::Ended(format!("unexpected frame kind {other:?}")),
                    };
                    let end = matches!(cmd, Cmd::Ended(_));
                    if tx.send(cmd).is_err() || end {
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    if !rstop.load(Ordering::Acquire) {
                        let _ = tx.send(Cmd::Ended(e.to_string()));
                    }
                    break;
                }
            }
        }
    });

    let mut w = Writer {
        ws,
        rx,
        deferred: VecDeque::new(),
        start: Instant::now(),
        bytes: 0,
        frames: 0,
        events: 0,
        max_block_us: 0,
        write_stalls: 0,
        credit_gated: cfg.credit_window > 0,
        credit: 0,
        credit_waits: 0,
        credit_wait_max_us: 0,
        credit_wait_total_us: 0,
        credit_stalls: 0,
        error: None,
        ipc006: false,
        ended: None,
    };
    let mut quit = false;
    loop {
        let cmd = match w.deferred.pop_front() {
            Some(c) => c,
            None => match w.rx.recv() {
                Ok(c) => c,
                Err(_) => break,
            },
        };
        match cmd {
            Cmd::Grant(n) => w.credit += u64::from(n),
            Cmd::Ended(why) => {
                w.ended = Some(why);
                break;
            }
            Cmd::Call {
                channel,
                corr,
                payload,
            } if channel == ECHO_CHANNEL => {
                let req: EchoRequest = match decode(&payload) {
                    Ok(r) => r,
                    Err(e) => {
                        w.ended = Some(e.to_string());
                        break;
                    }
                };
                if req.message == "scenario" {
                    if !run_scenario(&mut w, &cfg, corr, &payload) {
                        break;
                    }
                } else {
                    let reply = handle_echo(&payload).map_err(|e| e.to_string())?;
                    if !w.write_raw(FrameKind::Reply, ECHO_CHANNEL, corr, &reply) {
                        break;
                    }
                }
            }
            Cmd::Call {
                channel,
                corr,
                payload,
            } if channel == LIFECYCLE_CHANNEL => {
                match decode::<LifecycleRequest>(&payload) {
                    Ok(LifecycleRequest::Quit) => {}
                    Err(e) => {
                        w.ended = Some(e.to_string());
                        break;
                    }
                }
                println!("{{\"ev\":\"quit_received\",\"wall_us\":{}}}", wall_us());
                if cfg.arm == "e-quit" {
                    // EVENTs in flight ahead of the Quit reply while main is parked in it.
                    for _ in 0..cfg.burst {
                        if !w.write_event() {
                            break;
                        }
                    }
                }
                let reply = encode(&LifecycleResponse::Quit).map_err(|e| e.to_string())?;
                let ok = w.write_raw(FrameKind::Reply, LIFECYCLE_CHANNEL, corr, &reply);
                println!(
                    "{{\"ev\":\"quit_replied\",\"ok\":{},\"events_before_reply\":{},\"wall_us\":{}}}",
                    ok,
                    w.events,
                    wall_us()
                );
                quit = ok;
                break;
            }
            Cmd::Call { channel, .. } => {
                w.ended = Some(format!("CALL on undeclared channel {}", channel.0));
                break;
            }
        }
    }
    stop.store(true, Ordering::Release);
    let _ = stream.shutdown_app_link();
    let joined = reader.join().is_ok();
    println!(
        "{{\"ev\":\"summary\",\"arm\":\"{}\",\"bytes\":{},\"frames\":{},\"events\":{},\"max_block_us\":{},\"write_stalls_ge_250ms\":{},\"credit_waits\":{},\"credit_wait_max_us\":{},\"credit_wait_total_us\":{},\"credit_stalls_ge_250ms\":{},\"error\":{},\"ipc006\":{},\"ended\":{},\"quit_replied\":{},\"serve_returned\":{},\"t_ms\":{},\"wall_us\":{}}}",
        esc(&cfg.arm),
        w.bytes,
        w.frames,
        w.events,
        w.max_block_us,
        w.write_stalls,
        w.credit_waits,
        w.credit_wait_max_us,
        w.credit_wait_total_us,
        w.credit_stalls,
        w.error.as_deref().map_or("null".to_string(), |e| format!("\"{}\"", esc(e))),
        w.ipc006,
        w.ended.as_deref().map_or("null".to_string(), |e| format!("\"{}\"", esc(e))),
        quit,
        joined,
        w.t_ms(),
        wall_us()
    );
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        println!("{{\"ev\":\"fatal\",\"error\":\"{}\"}}", esc(&e));
        std::process::exit(2);
    }
}
