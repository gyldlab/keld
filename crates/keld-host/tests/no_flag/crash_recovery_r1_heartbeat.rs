//! GH-1081 round-1 research only (never merged): a renderer document heartbeat.
//!
//! The page fetches `GET /gh1081/<nonce>/<token>/<seq>/<storage>` (no-cors).
//! This server records each request and withholds the reply until the test asks
//! for the next beat, so the test owns every ordering edge and no timer in the
//! page or the test decides anything. A reload or replacement shows up as a new
//! unsolicited `seq = 0` beat with new renderer-created values.
//!
//! Live consumers with the same std TCP semantics: Windows E5
//! (`no_flag/windows/crash_recovery_r1.rs`) and Linux E6
//! (`no_flag/linux/crash_recovery_r1.rs`).

use crate::windows_renderer_http::{
    PendingRendererRequest, RendererRequestRead, accept_renderer_connection,
    read_renderer_request_line,
};
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const PAGE: &str = r#"<!doctype html>__BACKGROUND__<title>__TITLE__</title><p id=exact>gh1081</p><script>
"use strict";
const base = "http://127.0.0.1:__PORT__/gh1081/";
const nonce = () => Array.from(crypto.getRandomValues(new Uint32Array(4)),
  value => value.toString(16).padStart(8, "0")).join("");
const documentNonce = nonce();
const javascriptToken = nonce();
let storage;
try {
  window.sessionStorage.setItem("gh1081", documentNonce);
  storage = window.sessionStorage.getItem("gh1081") === documentNonce ? "ok" : "mismatch";
} catch (error) { storage = "error-" + String(error && error.name); }
let seq = 0;
// fetch never holds document load open (a pending <img> does: observed host
// startup deadline KELD-WV-005). The reply is only an ordering edge, so an
// opaque no-cors response is enough; a failed request ends this document's beats.
function beat() {
  fetch(base + documentNonce + "/" + javascriptToken + "/" + seq + "/" + encodeURIComponent(storage),
    { mode: "no-cors", cache: "no-store" })
    .then(() => { seq += 1; beat(); }, () => {});
}
beat();
</script>
"#;

/// One renderer heartbeat record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Beat {
    pub(crate) nonce: String,
    pub(crate) token: String,
    pub(crate) seq: u64,
    pub(crate) storage: String,
}

/// Sole owner of the heartbeat socket set; joins its worker on drop.
pub(crate) struct Heartbeat {
    port: u16,
    beats: mpsc::Receiver<Result<Beat, String>>,
    release: mpsc::Sender<()>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    seen: Vec<Beat>,
}

impl Heartbeat {
    pub(crate) fn bind() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind gh1081 heartbeat");
        let port = listener.local_addr().expect("heartbeat address").port();
        let (beats_tx, beats) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let worker = thread::spawn(move || serve(&listener, &beats_tx, &release_rx, &stopping));
        Self {
            port,
            beats,
            release,
            stop,
            worker: Some(worker),
            seen: Vec::new(),
        }
    }

    pub(crate) fn page(&self, title: &str, background: &str) -> String {
        PAGE.replace("__BACKGROUND__", background)
            .replace("__TITLE__", title)
            .replace("__PORT__", &self.port.to_string())
    }

    /// The next beat the page sends, solicited or not.
    pub(crate) fn next(&mut self, deadline: Instant) -> Beat {
        let beat = self
            .beats
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|error| {
                panic!(
                    "gh1081 heartbeat before deadline: {error}; seen={:?}",
                    self.seen
                )
            })
            .unwrap_or_else(|error| panic!("gh1081 heartbeat server: {error}"));
        eprintln!("KELD_GH1081_BEAT {beat:?}");
        self.seen.push(beat.clone());
        beat
    }

    /// Release every withheld beat, then collect the page's next one.
    pub(crate) fn poll(&mut self, deadline: Instant) -> Beat {
        self.release.send(()).expect("heartbeat worker live");
        self.next(deadline)
    }

    /// How many documents started reporting: one `seq = 0` beat each.
    pub(crate) fn documents(&self) -> usize {
        self.seen.iter().filter(|beat| beat.seq == 0).count()
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.expect("heartbeat worker joins");
            }
        }
    }
}

/// Same-document oracle over two renderer-created random values.
pub(crate) fn same_document(before: &Beat, after: &Beat) -> Result<(), String> {
    if before.nonce == after.nonce && before.token == after.token && after.seq > before.seq {
        Ok(())
    } else {
        Err(format!("before={before:?} after={after:?}"))
    }
}

fn parse(line: &[u8]) -> Option<Beat> {
    let line = std::str::from_utf8(line).ok()?;
    let path = line.strip_prefix("GET /gh1081/")?.split(' ').next()?;
    let mut fields = path.split('/');
    let beat = Beat {
        nonce: fields.next()?.to_owned(),
        token: fields.next()?.to_owned(),
        seq: fields.next()?.parse().ok()?,
        storage: fields.next()?.to_owned(),
    };
    fields.next().is_none().then_some(beat)
}

fn reply(mut stream: TcpStream, status: &[u8]) {
    // A closed peer (an unloaded document) is an observation, not a failure.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let _ = stream.write_all(status);
    // Dropping the stream closes this one HTTP exchange.
    drop(stream);
}

fn serve(
    listener: &TcpListener,
    beats: &mpsc::Sender<Result<Beat, String>>,
    release: &mpsc::Receiver<()>,
    stop: &AtomicBool,
) {
    const EMPTY: &[u8] = b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    const MISSING: &[u8] =
        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    if let Err(error) = listener.set_nonblocking(true) {
        let _ = beats.send(Err(format!("nonblocking heartbeat listener: {error}")));
        return;
    }
    let mut pending = Vec::<PendingRendererRequest>::new();
    let mut held = Vec::<TcpStream>::new();
    while !stop.load(Ordering::Acquire) {
        while release.try_recv().is_ok() {
            for stream in held.drain(..) {
                reply(stream, EMPTY);
            }
        }
        let mut index = 0;
        while index < pending.len() {
            let read = {
                let PendingRendererRequest { stream, request } = &mut pending[index];
                read_renderer_request_line(request, |buffer| stream.read(buffer))
            };
            match read {
                Ok(RendererRequestRead::Pending) => index += 1,
                Ok(RendererRequestRead::Empty) => {
                    pending.swap_remove(index);
                }
                Ok(RendererRequestRead::Complete(line)) => {
                    let request = pending.swap_remove(index);
                    if let Some(beat) = parse(&line) {
                        held.push(request.stream);
                        if beats.send(Ok(beat)).is_err() {
                            return;
                        }
                    } else {
                        eprintln!("KELD_GH1081_BEAT_OTHER {}", String::from_utf8_lossy(&line));
                        reply(request.stream, MISSING);
                    }
                }
                Err(error) => {
                    pending.swap_remove(index);
                    eprintln!("KELD_GH1081_BEAT_READ_ERROR {error}");
                }
            }
        }
        match accept_renderer_connection(listener, &mut pending, "gh1081 heartbeat") {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => {
                let _ = beats.send(Err(error));
                return;
            }
        }
        // Backs off only the nonblocking kernel poll; request arrival and the
        // test's absolute deadline remain the observables.
        thread::park_timeout(Duration::from_millis(10));
    }
}
