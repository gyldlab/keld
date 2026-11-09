//! GH-1081 round-2 research only (never merged): a renderer document probe
//! appended to the stock `keld create` scaffold page.
//!
//! The page reports `GET /r2/beat?d=<json>` (no-cors fetch, proven on all three
//! hosted engines in round 1). The server withholds each beat reply until the
//! test releases it, and the page applies a baked plan step (`beat`, `reload`,
//! or `real` = navigate to a real `http://` URL served here) only after a
//! release, so the test owns every ordering edge and no page timer decides
//! anything. `input` (textarea input events) and `unload` (pagehide beacon) are
//! answered at once. `GET /r2/real` serves the same scaffold page from a real
//! URL: the negative control for "a document with a URL reloads".
//!
//! Round 4 (KEL-79 origin experiments) adds reported-only facts to every beat
//! (`secure`, `subtle`, `ss`, `ls`, `idb`, `ua`, `wname`, `res`), two plan
//! steps (`res` = fetch a same-origin `res/<n>` resource; `resx:<url>` = no-cors
//! fetch of an absolute URL) and a `window.name` reload guard. Round-2 fields,
//! ordering and the `reload`/`real` steps are unchanged.
//!
//! Round 5 (X4–X9) adds a cookie sentinel (`ck`), an `IndexedDB` record
//! sentinel (`idb` now reports the previous value), the stored test sentinels
//! (`sent`), and the plan steps `go:<url>` (navigate), `sentinel:<ls>:<ss>`
//! (store the test's values), `probe:<url>` (fetch and report status, type,
//! length, canary, redirect and final URL) and `api` (every browser-API probe
//! at once, reported through its own `GET /r2/api`). The server also answers
//! `/r2/redirect?to=` (302), `/r2/cors?acao=` (echoes the requested
//! `Access-Control-Allow-Origin`), the worker scripts, a 1x1 SVG, `POST
//! /r2/res/post` and observes `/r2/ws` upgrade attempts.
//!
//! Consumers: macOS/Windows/Linux `crash_recovery_r2.rs`, `origin_r4.rs` and
//! `origin_r5.rs`.

// Shared by three OS binaries that each use a subset (macOS has no typed-draft
// arm), so per-binary dead-code analysis cannot see the other consumers.
#![allow(dead_code)]

use crate::windows_renderer_http::{
    PendingRendererRequest, RendererRequestRead, accept_renderer_connection,
    read_renderer_request_line,
};
use serde_json::Value;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The draft typed by real OS key events in the typed-draft experiments.
pub(crate) const DRAFT: &str = "Kal milte hain";

const SCRIPT: &str = r#"<script>
"use strict";
// GH-1081 round-2 probe (research only, appended to the stock scaffold page).
(() => {
const base = "http://127.0.0.1:__PORT__/r2/";
const plan = __PLAN__;
const rnd = () => Array.from(crypto.getRandomValues(new Uint32Array(2)),
  v => v.toString(16).padStart(8, "0")).join("");
const doc = rnd();
const entry = performance.getEntriesByType("navigation")[0];
const nav = entry ? entry.type : "none";
const real = location.protocol === "http:";
// Round 4: window.name survives a reload in the same browsing context, so a
// document that finds its own phase marker was reloaded even when the engine
// reports no navigation entry. Round-2 rows still see nav === "reload" first.
const phase = "gh1081:" + (real ? "real" : "stage");
const wname = window.name;
const reloaded = nav === "reload" || wname === phase;
window.name = phase;
const steps = reloaded ? [] : (real ? plan.real : plan.stage);
const area = document.getElementById("gh1081-draft");
const name = error => (error && error.name) || String(error);
const storage = get => {
  try {
    const store = get();
    const prev = store.getItem("gh1081");
    if (prev === null) store.setItem("gh1081", doc);
    return { ok: true, prev, now: store.getItem("gh1081") };
  } catch (error) {
    return { ok: false, error: name(error) };
  }
};
// Round 5: a cookie sentinel (X4) and the test's stored sentinels (X5).
const cookie = () => {
  try {
    const read = () => { const m = /(?:^|; )gh1081=([^;]*)/.exec(document.cookie); return m ? m[1] : null; };
    const prev = read();
    if (prev === null) document.cookie = "gh1081=" + doc + "; path=/";
    return { prev, now: read() };
  } catch (error) {
    return { error: name(error) };
  }
};
const sent = () => {
  const read = get => { try { return get().getItem("gh1081s"); } catch (error) { return "throw:" + name(error); } };
  return { ls: read(() => localStorage), ss: read(() => sessionStorage) };
};
let idb = "pending";
try {
  const open = indexedDB.open("gh1081", 1);
  open.onupgradeneeded = () => { open.result.createObjectStore("kv"); };
  open.onsuccess = () => {
    const db = open.result;
    try {
      const tx = db.transaction("kv", "readwrite");
      const kv = tx.objectStore("kv");
      const get = kv.get("gh1081");
      get.onsuccess = () => {
        const prev = get.result === undefined ? null : get.result;
        if (prev === null) kv.put(doc, "gh1081");
        idb = "ok:prev=" + prev;
      };
      get.onerror = () => { idb = "err:" + name(get.error); };
      tx.oncomplete = () => db.close();
      tx.onerror = () => { idb = "err:" + name(tx.error); db.close(); };
    } catch (error) {
      idb = "throw:" + name(error);
      db.close();
    }
  };
  open.onerror = () => { idb = "err:" + name(open.error); };
} catch (error) {
  idb = "throw:" + name(error);
}
let seq = 0, inputs = 0, trusted = 0, res = null;
const q = () => encodeURIComponent(JSON.stringify({ doc, seq, href: location.href.slice(0, 96),
  origin: String(location.origin), ready: document.readyState, nav, real, wname,
  bridge: typeof (window.keld && window.keld.invoke), draft: area ? area.value : null,
  inputs, trusted, active: document.activeElement ? (document.activeElement.id || document.activeElement.tagName) : null,
  secure: isSecureContext, subtle: typeof (crypto && crypto.subtle),
  ss: storage(() => sessionStorage), ls: storage(() => localStorage), idb, ck: cookie(), sent: sent(), res,
  ua: navigator.userAgent.slice(0, 120) }));
const send = kind => fetch(base + kind + "?d=" + q(), { mode: "no-cors", cache: "no-store" });
const post = (kind, payload) => fetch(base + kind + "?d=" + encodeURIComponent(JSON.stringify(payload)), { mode: "no-cors", cache: "no-store" });
if (area) area.addEventListener("input", event => {
  inputs += 1;
  if (event.isTrusted) trusted += 1;
  send("input").catch(() => {});
});
// Round 5: a fetch report with status, type, body length, canary marker,
// redirect flag and the tail of the final URL.
const probeFetch = (target, init) => fetch(target, init).then(
  reply => reply.text().then(
    text => "ok:" + reply.status + ":" + reply.type + ":len=" + text.length
      + ":canary=" + (text.indexOf("GH1081-CANARY") >= 0 ? 1 : 0) + ":r=" + (reply.redirected ? 1 : 0)
      + ":u=" + reply.url.slice(-40),
    () => "ok:" + reply.status + ":" + reply.type + ":len=?"),
  error => "err:" + name(error));
const within = (ms, promise) => Promise.race([promise, new Promise(resolve => setTimeout(() => resolve("timeout"), ms))]);
const worker = (make, ms) => within(ms, new Promise(resolve => {
  try {
    const w = make();
    const port = w.port || w;
    port.onmessage = event => resolve("ok:" + event.data);
    w.onerror = event => resolve("err:" + String(event.message || "error").slice(0, 40));
    if (w.port) w.port.start();
  } catch (error) { resolve("throw:" + name(error)); }
}));
// Round 5 X7: every browser-API probe, each bounded, results in one report.
const api = () => {
  const t = 4000;
  const acao = plan.acao || {};
  const blobSrc = "if (typeof postMessage === 'function') postMessage('blob');";
  const tasks = {
    get: probeFetch("res/9", { cache: "no-store" }),
    post: probeFetch("res/post", { method: "POST", body: "gh1081", cache: "no-store" }),
    cors_ok: probeFetch(base + "cors?acao=" + encodeURIComponent(acao.expected || ""), { mode: "cors", cache: "no-store" }),
    cors_wrong: probeFetch(base + "cors?acao=" + encodeURIComponent(acao.wrong || ""), { mode: "cors", cache: "no-store" }),
    worker: worker(() => new Worker("w.js"), t),
    module_worker: worker(() => new Worker("w.js", { type: "module" }), t),
    shared_worker: worker(() => new SharedWorker("w.js"), t),
    blob_worker: worker(() => new Worker(URL.createObjectURL(new Blob([blobSrc], { type: "text/javascript" }))), t),
    sw: within(t, Promise.resolve().then(() => navigator.serviceWorker.register("sw.js")).then(
      reg => "ok:" + String(reg.scope).slice(-30), error => "err:" + name(error) + ":" + String(error && error.message).slice(0, 48))),
    ws: within(t, new Promise(resolve => {
      try {
        const socket = new WebSocket("ws://127.0.0.1:__PORT__/r2/ws");
        socket.onopen = () => resolve("open");
        socket.onerror = () => resolve("error");
        socket.onclose = event => resolve("close:" + event.code);
      } catch (error) { resolve("throw:" + name(error)); }
    })),
    img: within(t, new Promise(resolve => {
      const image = new Image();
      image.onload = () => resolve("ok:" + image.naturalWidth);
      image.onerror = () => resolve("err");
      image.src = "res/px.svg";
    })),
    digest: within(t, Promise.resolve().then(() => crypto.subtle.digest("SHA-256", new Uint8Array([1]))).then(
      buffer => "ok:" + buffer.byteLength, error => "err:" + name(error))),
    estimate: within(t, Promise.resolve().then(() => navigator.storage.estimate()).then(
      estimate => "ok:quota=" + (estimate.quota > 0 ? 1 : 0), error => "err:" + name(error))),
  };
  const keys = Object.keys(tasks);
  return Promise.all(keys.map(key => tasks[key].catch(error => "err:" + name(error)))).then(values => {
    const out = {};
    keys.forEach((key, index) => { out[key] = String(values[index]).slice(0, 72); });
    return out;
  });
};
function beat() {
  send("beat").then(() => {
    const step = steps[seq] || "beat";
    seq += 1;
    if (step === "reload") {
      addEventListener("pagehide", () => navigator.sendBeacon(base + "unload?d=" + q(), "x"), { once: true });
      location.reload();
    } else if (step === "real") {
      location.href = base + "real";
    } else if (step.startsWith("go:")) {
      // Round 5: a navigation (relative or absolute); the successor reports.
      location.href = step.slice(3);
    } else if (step.startsWith("sentinel:")) {
      // Round 5 X5: the test's sentinels under their own key.
      const parts = step.slice(9).split(":");
      try { localStorage.setItem("gh1081s", parts[0]); res = "ls:ok"; } catch (error) { res = "ls:" + name(error); }
      try { sessionStorage.setItem("gh1081s", parts[1]); res += ",ss:ok"; } catch (error) { res += ",ss:" + name(error); }
      beat();
    } else if (step === "api") {
      api().then(out => post("api", { doc, api: out })).catch(() => {}).then(() => { res = "api-sent"; beat(); });
    } else if (step === "res" || step.startsWith("resx:") || step.startsWith("probe:")) {
      // Round 4: a same-origin resource (relative, served by the transport under
      // test) or an absolute URL (no-cors). Round 5: a reported fetch (`probe:`).
      // The next beat carries the outcome.
      const target = step === "res" ? "res/" + seq : step.startsWith("resx:") ? step.slice(5) : step.slice(6);
      const init = step.startsWith("resx:") ? { mode: "no-cors", cache: "no-store" } : { cache: "no-store" };
      const report = step.startsWith("probe:") ? probeFetch(target, init) : fetch(target, init).then(
        reply => "ok:" + reply.status + ":" + reply.type, error => "err:" + name(error));
      report.then(value => { res = value; }).then(beat);
    } else {
      beat();
    }
  }, () => {});
}
if (area) area.focus();
if (document.readyState === "complete") beat(); else addEventListener("load", beat, { once: true });
})();
</script>"#;

const DRAFT_AREA: &str = r#"<textarea id="gh1081-draft" autofocus spellcheck="false" autocomplete="off" style="position:fixed;left:10%;top:20%;width:80%;height:60%;font:24px system-ui;background:#222;color:#fff;border:1px solid #888"></textarea>"#;

/// One record the page sent.
#[derive(Clone, Debug)]
pub(crate) enum Report {
    /// A held heartbeat with the document's live state.
    Beat(Value),
    /// One textarea `input` event (state after the event).
    Input(Value),
    /// The `pagehide` beacon of a document that is unloading.
    Unload(Value),
    /// The real-URL page was fetched (the control's document request).
    PageFetch,
    /// Round 5 X7: the page's combined browser-API results (`api` step).
    Api(Value),
    /// Round 5 X8: `/r2/redirect?to=<url>` was answered with a 302 to `url`.
    Redirect(String),
    /// Round 5 X7: `/r2/cors?acao=<origin>` was answered echoing that header.
    Cors(String),
    /// Round 5 X7: a `WebSocket` upgrade request reached the server.
    Ws,
    /// Round 5: another resource the real-URL page requested (`w.js`, ...).
    Served(String),
}

/// The stock `keld create` scaffold `index.html`, rendered by the CLI owner.
pub(crate) fn scaffold_html(scratch: &Path) -> String {
    let project = keld_cli::create::create_project(scratch, "gh1081-scaffold")
        .expect("render the stock keld create scaffold");
    std::fs::read_to_string(project.join("index.html")).expect("scaffold index.html")
}

/// Sole owner of the probe socket set; joins its worker on drop.
pub(crate) struct Probe {
    page: String,
    port: u16,
    reports: mpsc::Receiver<Result<Report, String>>,
    release: mpsc::Sender<()>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    pub(crate) seen: Vec<Report>,
}

impl Probe {
    /// `stage_plan` / `real_plan` are the steps applied after each released beat.
    pub(crate) fn bind(
        scaffold: &str,
        stage_plan: &[&str],
        real_plan: &[&str],
        draft: bool,
    ) -> Self {
        Self::bind_with(scaffold, stage_plan, real_plan, draft, &Value::Null)
    }

    /// `bind` plus extra plan fields the page reads (round 5: `acao`, the
    /// `Access-Control-Allow-Origin` values the `api` step asks the server for).
    pub(crate) fn bind_with(
        scaffold: &str,
        stage_plan: &[&str],
        real_plan: &[&str],
        draft: bool,
        extra: &Value,
    ) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind gh1081 r2 probe");
        let port = listener.local_addr().expect("probe address").port();
        let page = Self::render(scaffold, port, stage_plan, real_plan, draft, extra);
        let (reports_tx, reports) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let served = page.clone();
        let worker =
            thread::spawn(move || serve(&listener, &served, &reports_tx, &release_rx, &stopping));
        Self {
            page,
            port,
            reports,
            release,
            stop,
            worker: Some(worker),
            seen: Vec::new(),
        }
    }

    /// The renderer HTML the stage carries (the scaffold plus the probe); the
    /// real-URL control serves the same bytes.
    pub(crate) fn page(&self) -> &str {
        &self.page
    }

    /// The probe server port the page reports to.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    fn render(
        scaffold: &str,
        port: u16,
        stage: &[&str],
        real: &[&str],
        draft: bool,
        extra: &Value,
    ) -> String {
        let mut plan = serde_json::json!({ "stage": stage, "real": real });
        if let (Some(plan), Some(extra)) = (plan.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                plan.insert(key.clone(), value.clone());
            }
        }
        let plan = plan.to_string();
        let script = SCRIPT
            .replace("__PORT__", &port.to_string())
            .replace("__PLAN__", &plan);
        let body = if draft {
            format!("{DRAFT_AREA}{script}</body>")
        } else {
            format!("{script}</body>")
        };
        assert!(scaffold.contains("</body>"), "scaffold page has a body");
        scaffold.replacen("</body>", &body, 1)
    }

    /// The next report before `deadline`, or `None`.
    pub(crate) fn try_next(&mut self, deadline: Instant) -> Option<Report> {
        let report = self
            .reports
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()?
            .unwrap_or_else(|error| panic!("gh1081 r2 probe server: {error}"));
        eprintln!("KELD_GH1081_R2_REPORT {report:?}");
        self.seen.push(report.clone());
        Some(report)
    }

    /// The next beat (other reports are recorded and skipped) before `deadline`.
    pub(crate) fn next_beat(&mut self, deadline: Instant) -> Option<Value> {
        loop {
            if let Report::Beat(value) = self.try_next(deadline)? {
                return Some(value);
            }
        }
    }

    /// The next beat, required.
    pub(crate) fn beat(&mut self, deadline: Instant) -> Value {
        self.next_beat(deadline)
            .unwrap_or_else(|| panic!("gh1081 r2 beat before deadline; seen={:?}", self.seen))
    }

    /// The next `api` report (other reports are recorded and skipped).
    pub(crate) fn next_api(&mut self, deadline: Instant) -> Option<Value> {
        loop {
            if let Report::Api(value) = self.try_next(deadline)? {
                return Some(value);
            }
        }
    }

    /// Release every withheld beat (the page then applies its next plan step).
    pub(crate) fn release(&self) {
        self.release.send(()).expect("probe worker live");
    }

    /// Release, then collect the page's next beat.
    pub(crate) fn poll(&mut self, deadline: Instant) -> Value {
        self.release();
        self.beat(deadline)
    }

    /// Input reports until one shows `expected` (or the deadline passes).
    pub(crate) fn await_draft(&mut self, expected: &str, deadline: Instant) -> Option<Value> {
        loop {
            match self.try_next(deadline)? {
                Report::Input(value) | Report::Beat(value) if value["draft"] == expected => {
                    return Some(value);
                }
                _ => {}
            }
        }
    }

    /// Distinct document ids that sent a `seq = 0` beat.
    pub(crate) fn documents(&self) -> Vec<String> {
        let mut docs = Vec::new();
        for report in &self.seen {
            if let Report::Beat(value) = report
                && value["seq"] == 0
                && let Some(doc) = value["doc"].as_str()
                && !docs.iter().any(|seen: &String| seen == doc)
            {
                docs.push(doc.to_owned());
            }
        }
        docs
    }

    pub(crate) fn count(&self, kind: fn(&Report) -> bool) -> usize {
        self.seen.iter().filter(|report| kind(report)).count()
    }
}

pub(crate) fn is_unload(report: &Report) -> bool {
    matches!(report, Report::Unload(_))
}

pub(crate) fn is_page_fetch(report: &Report) -> bool {
    matches!(report, Report::PageFetch)
}

pub(crate) fn is_ws(report: &Report) -> bool {
    matches!(report, Report::Ws)
}

pub(crate) fn is_redirect(report: &Report) -> bool {
    matches!(report, Report::Redirect(_))
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.expect("probe worker joins");
            }
        }
    }
}

/// After a reload trigger: collect reports until a new document's first beat
/// arrives or `deadline` passes. Returns that beat, if any.
pub(crate) fn await_successor(
    probe: &mut Probe,
    previous: &str,
    deadline: Instant,
) -> Option<Value> {
    loop {
        match probe.try_next(deadline)? {
            Report::Beat(value) if value["doc"] != previous && value["seq"] == 0 => {
                return Some(value);
            }
            _ => {}
        }
    }
}

/// Same-document oracle over the renderer-created id.
pub(crate) fn same_document(before: &Value, after: &Value) -> Result<(), String> {
    if before["doc"] == after["doc"]
        && before["doc"].is_string()
        && after["seq"].as_u64() > before["seq"].as_u64()
    {
        Ok(())
    } else {
        Err(format!("before={before} after={after}"))
    }
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = text.get(index + 1..index + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// `(kind, raw query, decoded d= JSON)` of one `/r2/<kind>?<query>` request line.
fn parse(line: &[u8]) -> Option<(String, String, Option<Value>)> {
    let line = std::str::from_utf8(line).ok()?;
    let mut parts = line.split(' ');
    let _method = parts.next()?;
    let target = parts.next()?;
    let rest = target.strip_prefix("/r2/")?;
    let (kind, query) = rest.split_once('?').unwrap_or((rest, ""));
    let value = query
        .strip_prefix("d=")
        .and_then(percent_decode)
        .and_then(|json| serde_json::from_str(&json).ok());
    Some((kind.to_owned(), query.to_owned(), value))
}

/// The percent-decoded value of `key=` in a raw query, if present.
fn query_value(query: &str, key: &str) -> Option<String> {
    query
        .split('&')
        .find_map(|pair| {
            pair.strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
        })
        .and_then(percent_decode)
}

/// One complete HTTP/1.1 response with a body.
fn response(status: &str, content_type: &str, extra_headers: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn reply(mut stream: TcpStream, response: &[u8]) {
    // A closed peer (an unloaded document) is an observation, not a failure.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let _ = stream.write_all(response);
    drop(stream);
}

const EMPTY: &[u8] = b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
const MISSING: &[u8] = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
// Round 5: resources the real-URL control page requests from this origin.
const WORKER_JS: &[u8] = b"if (typeof postMessage === 'function') { postMessage('hi'); }\nself.onconnect = event => { event.ports[0].postMessage('shared'); };\n";
const SW_JS: &[u8] = b"self.addEventListener('install', () => self.skipWaiting());\n";
const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="#000"/></svg>"##;

/// Answers one request line and returns the report it carries, if any. Beats
/// are held (answered on release); everything else is answered at once.
fn route(
    line: &[u8],
    stream: TcpStream,
    held: &mut Vec<TcpStream>,
    page_response: &[u8],
) -> Option<Report> {
    match parse(line) {
        Some((kind, _, Some(value))) if kind == "beat" => {
            held.push(stream);
            Some(Report::Beat(value))
        }
        Some((kind, _, Some(value))) if kind == "input" => {
            reply(stream, EMPTY);
            Some(Report::Input(value))
        }
        Some((kind, _, Some(value))) if kind == "unload" => {
            reply(stream, EMPTY);
            Some(Report::Unload(value))
        }
        Some((kind, _, Some(value))) if kind == "api" => {
            reply(stream, EMPTY);
            Some(Report::Api(value))
        }
        Some((kind, _, _)) if kind == "real" => {
            reply(stream, page_response);
            Some(Report::PageFetch)
        }
        Some((kind, query, _)) if kind == "redirect" => {
            let to = query_value(&query, "to").unwrap_or_default();
            let location = format!("Location: {to}\r\n");
            reply(stream, &response("302 Found", "text/plain", &location, b""));
            Some(Report::Redirect(to))
        }
        Some((kind, query, _)) if kind == "cors" => {
            let origin = query_value(&query, "acao").unwrap_or_default();
            let allow = format!("Access-Control-Allow-Origin: {origin}\r\n");
            reply(
                stream,
                &response("204 No Content", "text/plain", &allow, b""),
            );
            Some(Report::Cors(origin))
        }
        Some((kind, _, _)) if kind == "ws" => {
            reply(stream, &response("400 Bad Request", "text/plain", "", b""));
            Some(Report::Ws)
        }
        Some((kind, _, _)) if kind == "w.js" => {
            reply(
                stream,
                &response("200 OK", "text/javascript", "", WORKER_JS),
            );
            Some(Report::Served(kind))
        }
        Some((kind, _, _)) if kind == "sw.js" => {
            reply(stream, &response("200 OK", "text/javascript", "", SW_JS));
            Some(Report::Served(kind))
        }
        Some((kind, _, _)) if kind == "res/px.svg" => {
            reply(stream, &response("200 OK", "image/svg+xml", "", SVG));
            Some(Report::Served(kind))
        }
        Some((kind, _, _)) if kind.starts_with("res/") => {
            let body = format!("gh1081 real resource {kind}");
            reply(
                stream,
                &response("200 OK", "text/plain", "", body.as_bytes()),
            );
            Some(Report::Served(kind))
        }
        _ => {
            eprintln!("KELD_GH1081_R2_OTHER {}", String::from_utf8_lossy(line));
            reply(stream, MISSING);
            None
        }
    }
}

fn serve(
    listener: &TcpListener,
    page: &str,
    reports: &mpsc::Sender<Result<Report, String>>,
    release: &mpsc::Receiver<()>,
    stop: &AtomicBool,
) {
    let page_response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    if let Err(error) = listener.set_nonblocking(true) {
        let _ = reports.send(Err(format!("nonblocking probe listener: {error}")));
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
                    let report = route(&line, request.stream, &mut held, page_response.as_bytes());
                    if let Some(report) = report
                        && reports.send(Ok(report)).is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    pending.swap_remove(index);
                    eprintln!("KELD_GH1081_R2_READ_ERROR {error}");
                }
            }
        }
        match accept_renderer_connection(listener, &mut pending, "gh1081 r2 probe") {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => {
                let _ = reports.send(Err(error));
                return;
            }
        }
        // Backs off only the nonblocking kernel poll; request arrival and the
        // test's absolute deadline remain the observables.
        thread::park_timeout(Duration::from_millis(10));
    }
}
