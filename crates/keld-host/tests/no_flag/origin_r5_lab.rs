//! GH-1081 round-5 research only (never merged): the shared test-side owner of
//! the KEL-79 origin experiments X4–X9 (owner decision K2 §B, design G2 §4) on
//! the isolated `gh1081_origin_lab` harness owned by `origin_r4_lab.rs`.
//!
//! - X4 two-app isolation: two views in one harness process write
//!   `localStorage`, `IndexedDB` and cookie sentinels; the second view reports
//!   what it can see. Control: the same host twice in one store must share.
//! - X5 session-only storage and quit cleanup: the page stores two
//!   high-entropy sentinels, the harness quits (or is killed), the store roots
//!   are byte-grepped (UTF-8 and UTF-16LE), then a relaunch reads them back.
//!   Control: the persistent-store `localStorage` sentinel must be found.
//! - X6 path and rebinding safety: a corpus of traversal, encoding, symlink,
//!   host-mismatch and glob-mismatch requests against the strict handler, with
//!   three deliberately broken handlers that must leak.
//! - X7 browser API compatibility: fetch GET/POST, CORS (made falsifiable by
//!   echoing the expected vs a wrong `Access-Control-Allow-Origin`), workers,
//!   service worker, `WebSocket`, image, `crypto.subtle`, storage estimate.
//! - X8 redirects: 3xx from the handler (fetch and navigation), inbound 302
//!   and `location =` from an external page. Control: http→http 302 followed.
//! - X9 renderer-process crash: the content process is killed from outside;
//!   engine events, page silence, pixels and the reload outcome are recorded.
//!
//! Every candidate outcome is recorded, not asserted (K2 §B forbids selecting
//! an architecture from these runs); only the negative controls that validate
//! an oracle are asserted. Consumers: macOS/Windows/Linux `origin_r5.rs`.

// Shared by three OS binaries that each use a subset.
#![allow(dead_code)]

use crate::crash_recovery_r2_probe::{
    Probe, Report, await_successor, is_page_fetch, is_redirect, is_ws,
};
#[cfg(unix)]
use crate::origin_r4_lab::command_output;
use crate::origin_r4_lab::{
    Arm, Driver, LAB_DEADLINE, Lab, LaunchSpec, SUCCESSOR_BOUND, beat_facts, hex32,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// One store arrangement for a launch; the OS driver maps it onto the spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoreArm {
    /// In-memory store (Windows: a fresh test-owned user data folder).
    Ephemeral,
    /// On-disk store (macOS identified store, Linux default XDG context,
    /// Windows user data folder).
    Persistent,
    /// Windows `InPrivate` profile.
    InPrivate,
    /// Linux explicit base data directory.
    Directory,
}

impl StoreArm {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Ephemeral => "ephemeral",
            Self::Persistent => "persistent",
            Self::InPrivate => "inprivate",
            Self::Directory => "directory",
        }
    }
}

/// Per-launch identity material: the macOS store id, the Windows user data
/// folders, the Linux XDG roots and explicit data directory.
#[derive(Debug)]
pub(crate) struct Ids {
    pub(crate) store_id: String,
    pub(crate) user_data: tempfile::TempDir,
    pub(crate) second_user_data: tempfile::TempDir,
    pub(crate) xdg: tempfile::TempDir,
    pub(crate) dir: tempfile::TempDir,
}

impl Ids {
    pub(crate) fn fresh() -> Self {
        Self {
            store_id: hex32(),
            user_data: tempfile::tempdir().expect("user data folder"),
            second_user_data: tempfile::tempdir().expect("second user data folder"),
            xdg: tempfile::tempdir().expect("xdg root"),
            dir: tempfile::tempdir().expect("explicit data directory"),
        }
    }
}

/// How a launch ends in X5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exit {
    Quit,
    Kill,
}

/// Round-5 OS specifics on top of the round-4 `Driver`.
pub(crate) trait Driver5: Driver {
    /// Writes `store` into the spec (store flag, ids, user data, environment).
    fn apply_store(&mut self, spec: &mut LaunchSpec, store: StoreArm, ids: &Ids);
    /// The roots whose bytes X5 greps after the harness has exited.
    fn store_roots(&self, store: StoreArm, ids: &Ids) -> Vec<PathBuf>;
    /// Process facts taken before a launch (macOS: the `WebContent` set).
    fn process_baseline(&mut self) -> Value;
    /// `{"pids": [...], ...}`: the harness's content/renderer processes.
    fn renderer_pids(&mut self, lab: &mut Lab, baseline: &Value) -> Value;
    /// Kills one process hard and reports whether it is gone.
    fn kill_pid(&mut self, pid: u32) -> Value;
    /// Bounded wait for the engine's helper processes of one launch to exit.
    fn await_engine_exit(
        &mut self,
        store: StoreArm,
        ids: &Ids,
        baseline: &Value,
        deadline: Instant,
    ) -> Value;
}

/// A unique harness window title.
pub(crate) fn title(label: &str) -> String {
    format!("GH1081 R5 {label} {} {}", std::process::id(), &hex32()[..8])
}

pub(crate) fn write_page(dir: &Path, name: &str, page: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, page).expect("write probe page");
    path
}

fn launch(
    driver: &mut impl Driver5,
    spec: &mut LaunchSpec,
    store: StoreArm,
    ids: &Ids,
    deadline: Instant,
) -> Lab {
    driver.apply_store(spec, store, ids);
    Lab::launch_with(spec, deadline)
}

/// Round-4 facts plus the round-5 fields.
pub(crate) fn facts5(beat: &Value) -> Value {
    let mut facts = beat_facts(beat);
    if let Some(map) = facts.as_object_mut() {
        for key in ["doc", "seq", "ck", "sent", "res"] {
            map.insert(key.to_owned(), beat[key].clone());
        }
    }
    facts
}

/// The first (or next released) beat, polled until `IndexedDB` has settled.
fn settled_beat(probe: &mut Probe, first: bool, deadline: Instant) -> Value {
    let mut beat = if first {
        probe.beat(deadline)
    } else {
        probe.poll(deadline)
    };
    let mut rounds = 0;
    while beat["idb"] == "pending" && rounds < 10 {
        beat = probe.poll(deadline);
        rounds += 1;
    }
    beat
}

fn served_count(reply: Option<&str>) -> usize {
    reply
        .and_then(|text| text.strip_prefix("count="))
        .and_then(|rest| rest.split(' ').next())
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

fn served_entries(reply: Option<&str>) -> Vec<String> {
    reply
        .and_then(|text| text.split_once("uris="))
        .map(|(_, uris)| uris)
        .filter(|uris| !uris.is_empty())
        .map(|uris| uris.split(',').map(ToOwned::to_owned).collect())
        .unwrap_or_default()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Byte-greps every regular file under `roots` for each needle in UTF-8 and
/// UTF-16LE. Symlinks are not followed; files over 64 MiB are counted, not read.
pub(crate) fn grep_roots(roots: &[PathBuf], needles: &[(&str, &str)]) -> Value {
    let encoded: Vec<(String, Vec<u8>, Vec<u8>)> = needles
        .iter()
        .map(|(name, text)| {
            (
                (*name).to_owned(),
                text.as_bytes().to_vec(),
                text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            )
        })
        .collect();
    let (mut files, mut bytes, mut unreadable, mut large) = (0_usize, 0_usize, 0_usize, 0_usize);
    let mut hits = Vec::new();
    let missing: Vec<String> = roots
        .iter()
        .filter(|root| !root.exists())
        .map(|root| root.display().to_string())
        .collect();
    let mut stack: Vec<PathBuf> = roots.iter().filter(|root| root.exists()).cloned().collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            unreadable += 1;
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                unreadable += 1;
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.len() > 64 * 1024 * 1024 {
                large += 1;
            } else if let Ok(data) = fs::read(&path) {
                files += 1;
                bytes += data.len();
                for (name, utf8, utf16) in &encoded {
                    let (utf8_hit, utf16_hit) = (contains(&data, utf8), contains(&data, utf16));
                    if utf8_hit || utf16_hit {
                        hits.push(json!({"needle": name, "file": path.display().to_string(),
                            "utf8": utf8_hit, "utf16le": utf16_hit, "len": data.len()}));
                    }
                }
            } else {
                unreadable += 1;
            }
        }
    }
    json!({
        "roots": roots.iter().map(|root| root.display().to_string()).collect::<Vec<_>>(),
        "missing_roots": missing, "files": files, "bytes": bytes, "unreadable": unreadable,
        "skipped_large": large, "hits": hits,
    })
}

/// Up to `limit` relative paths under `roots`, as evidence of what the engine
/// created (round 5 X4/X5: the Linux prgname directory, the macOS store).
pub(crate) fn list_tree(roots: &[PathBuf], limit: usize) -> Value {
    let mut listed = Vec::new();
    for root in roots {
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if listed.len() >= limit {
                    return json!({"truncated": true, "paths": listed});
                }
                let path = entry.path();
                let relative = path.strip_prefix(root).map_or_else(
                    |_| path.display().to_string(),
                    |rel| rel.display().to_string(),
                );
                listed.push(format!("{}:{relative}", root.display()));
                if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
                    stack.push(path);
                }
            }
        }
    }
    json!({"truncated": false, "paths": listed})
}

fn record(experiment: &str, mut body: Value) -> Value {
    if let Some(map) = body.as_object_mut() {
        map.insert(
            String::from("experiment"),
            Value::String(experiment.to_owned()),
        );
    }
    eprintln!("KELD_GH1081_R5_RECORD {body}");
    body
}

/// `SIGKILL` one process and report whether it is gone (zombies count as gone:
/// the engine reaps them).
#[cfg(unix)]
pub(crate) fn unix_kill(pid: u32) -> Value {
    let status = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output()
        .ok()
        .and_then(|output| output.status.code());
    let gone = unix_process_gone(pid, Instant::now() + Duration::from_secs(5));
    json!({"pid": pid, "kill_status": status, "gone": gone})
}

/// Bounded wait until `ps` no longer lists `pid` as a live (non-zombie) process.
#[cfg(unix)]
pub(crate) fn unix_process_gone(pid: u32, deadline: Instant) -> bool {
    loop {
        let state = command_output("ps", &["-o", "stat=", "-p", &pid.to_string()]);
        if state.is_empty() || state.starts_with('Z') {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::park_timeout(Duration::from_millis(50));
    }
}

/// One compact row per record for the per-OS summary line.
pub(crate) fn summarize(records: &[Value]) -> Value {
    Value::Array(records.iter().map(summarize_one).collect())
}

fn summarize_one(record: &Value) -> Value {
    match record["experiment"].as_str() {
        Some("x4") => json!({
            "x": "x4", "arm": record["arm"], "first_origin": record["first"]["origin"],
            "first_doc": record["first"]["doc"], "first_ck": record["first"]["ck"],
            "second_origin": record["second"]["origin"], "second_reply": record["second_reply"],
            "second_ls_prev": record["second"]["ls"]["prev"], "second_idb": record["second"]["idb"],
            "second_ck_prev": record["second"]["ck"]["prev"], "second_ss_prev": record["second"]["ss"]["prev"],
            "first_again_ck": record["first_again"]["ck"],
            "store_paths": record["store_tree"]["paths"].as_array().map_or(0, Vec::len),
        }),
        Some("x5") => json!({
            "x": "x5", "arm": record["arm"], "store": record["store"], "store_flag": record["store_flag"],
            "exit": record["exit"], "stored": record["stored"]["res"], "confirmed_sent": record["confirmed"]["sent"],
            "grep_files": record["grep"]["files"], "grep_bytes": record["grep"]["bytes"],
            "grep_unreadable": record["grep"]["unreadable"], "grep_missing_roots": record["grep"]["missing_roots"],
            "hits": record["grep"]["hits"], "read_back_sent": record["read_back"]["sent"],
            "read_back_ls_prev": record["read_back"]["ls"]["prev"], "exit_wait": record["exit_wait"],
            "store_paths": record["store_tree"]["paths"].as_array().map_or(0, Vec::len),
        }),
        Some("x6") => json!({
            "x": "x6", "arm": record["arm"], "handler": record["handler"], "filter": record["filter"],
            "link": record["link"], "results": record["results"],
        }),
        Some("x7") => json!({
            "x": "x7", "arm": record["arm"], "origin": record["document"]["origin"],
            "secure": record["document"]["secure"], "api": record["api"],
            "ws_requests_seen": record["ws_requests_seen"], "cors_served": record["cors_served"],
            "requests": record["requests"].as_array().map_or(0, Vec::len),
        }),
        Some("x8") => json!({
            "x": "x8", "case": record["case"], "arm": record["arm"], "fetches": record["fetches"],
            "intermediate_origin": record["intermediate"]["origin"],
            "successor_origin": record["successor"]["origin"], "successor_href": record["successor"]["href"],
            "successor_real": record["successor"]["real"], "successor_nav": record["successor"]["nav"],
            "url_after": record["url_after"], "page_fetches": record["page_fetches"],
            "redirects_served": record["redirects_served"], "page_loads": record["page_loads"],
            "navigations": record["navigations"], "documents": record["documents"].as_array().map_or(0, Vec::len),
        }),
        Some("x9") => json!({
            "x": "x9", "arm": record["arm"], "killed_pids": record["killed_pids"], "kills": record["kills"],
            "engine_events": record["engine_events"], "idle_doc": record["idle"]["doc"],
            "post_kill_beat_doc": record["post_kill_beat"]["doc"], "url_killed": record["url_killed"],
            "reload": record["reload"], "successor_origin": record["successor"]["origin"],
            "successor_ss_prev": record["successor"]["ss"]["prev"], "successor_ls_prev": record["successor"]["ls"]["prev"],
            "reload_ms": record["reload_ms"], "center_before": record["pixels_before"]["center"],
            "center_killed": record["pixels_killed"]["center"], "light_killed": record["pixels_killed"]["light"],
            "center_after": record["pixels_after"]["center"], "light_after": record["pixels_after"]["light"],
            "url_after": record["url_after"], "page_loads": record["page_loads"],
        }),
        _ => record.clone(),
    }
}

// ---------------------------------------------------------------------------
// X4 two-app isolation

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X4Arm {
    /// Control: the same host twice in one store must see the first sentinels.
    SameHostShared,
    /// Candidate: two hosts in one store must not see each other.
    DistinctHostShared,
    /// Store isolation alone: the same host in a separate (ephemeral) store.
    SameHostSeparate,
}

/// Two views in one harness process; the second reports what it sees.
pub(crate) fn run_x4(driver: &mut impl Driver5, scaffold: &str, scheme: &str, arm: X4Arm) -> Value {
    let scratch = tempfile::tempdir().expect("x4 scratch");
    let ids = Ids::fresh();
    let mut first_probe = Probe::bind(scaffold, &[], &[], false);
    let mut second_probe = Probe::bind(scaffold, &[], &[], false);
    let page = write_page(scratch.path(), "a.html", first_probe.page());
    let second_page = write_page(scratch.path(), "b.html", second_probe.page());
    let title = title("X4");
    let mut spec = LaunchSpec::new(Arm::Scheme, &page, &title, scheme);
    spec.host = String::from("x4a");
    spec.second_host = Some(String::from(if arm == X4Arm::DistinctHostShared {
        "x4b"
    } else {
        "x4a"
    }));
    spec.second_page = Some(second_page);
    spec.second_store = Some(String::from(if arm == X4Arm::SameHostSeparate {
        "separate"
    } else {
        "shared"
    }));
    let deadline = Instant::now() + LAB_DEADLINE;
    let mut lab = launch(driver, &mut spec, StoreArm::Persistent, &ids, deadline);
    let first = settled_beat(&mut first_probe, true, deadline);
    let second_reply = lab.command("second", deadline);
    let second = settled_beat(&mut second_probe, true, deadline);
    let first_again = settled_beat(&mut first_probe, false, deadline);
    let served = lab.command("served", deadline);
    lab.drain_events(Duration::from_millis(300));
    let events = lab.events.clone();
    let hello = lab.hello.clone();
    let shutdown = lab.quit(deadline);
    let roots = driver.store_roots(StoreArm::Persistent, &ids);
    let tree = list_tree(&roots, 120);
    record(
        "x4",
        json!({
            "arm": format!("{arm:?}"), "scheme": scheme, "store": spec.store, "hello": hello,
            "first": facts5(&first), "second_reply": second_reply, "second": facts5(&second),
            "first_again": facts5(&first_again), "served": served, "events": events,
            "store_tree": tree, "lab": shutdown,
        }),
    )
}

/// Oracle validity for X4: the same-host control must share through every
/// storage the engine offers on this origin (`localStorage` is the minimum).
pub(crate) fn assert_x4_control(record: &Value) {
    if record["arm"] != "SameHostShared" {
        return;
    }
    let first_doc = record["first"]["doc"].as_str().unwrap_or("");
    assert!(
        !first_doc.is_empty() && record["second"]["ls"]["prev"] == first_doc,
        "X4 NC: the same host twice in one store must share localStorage: {record}"
    );
    assert_eq!(
        record["second"]["idb"],
        format!("ok:prev={first_doc}"),
        "X4 NC: the same host twice in one store must share IndexedDB: {record}"
    );
}

// ---------------------------------------------------------------------------
// X5 session-only storage and quit cleanup

/// Stores two sentinels, exits, greps the store roots, relaunches and reads.
pub(crate) fn run_x5(
    driver: &mut impl Driver5,
    scaffold: &str,
    scheme: &str,
    arm: Arm,
    store: StoreArm,
    exit: Exit,
) -> Value {
    let scratch = tempfile::tempdir().expect("x5 scratch");
    let ids = Ids::fresh();
    let local_sentinel = format!("L{}", hex32());
    let session_sentinel = format!("S{}", hex32());
    let step = format!("sentinel:{local_sentinel}:{session_sentinel}");
    let mut probe = Probe::bind(scaffold, &[step.as_str()], &[], false);
    let page = write_page(scratch.path(), "x5.html", probe.page());
    let title = title("X5");
    let mut spec = LaunchSpec::new(arm, &page, &title, scheme);
    spec.host = String::from("x5");
    let baseline = driver.process_baseline();
    let deadline = Instant::now() + LAB_DEADLINE;
    let lab = launch(driver, &mut spec, store, &ids, deadline);
    let first = probe.beat(deadline);
    let stored = probe.poll(deadline);
    let confirmed = probe.poll(deadline);
    let pid = lab.pid;
    let hello = lab.hello.clone();
    let shutdown = match exit {
        Exit::Quit => lab.quit(deadline),
        Exit::Kill => lab.kill(),
    };
    let exit_wait = driver.await_engine_exit(
        store,
        &ids,
        &baseline,
        Instant::now() + Duration::from_secs(20),
    );
    let roots = driver.store_roots(store, &ids);
    let grep = grep_roots(
        &roots,
        &[("ls", &local_sentinel), ("ss", &session_sentinel)],
    );
    let tree = list_tree(&roots, 120);
    let mut again = Probe::bind(scaffold, &[], &[], false);
    spec.page = write_page(scratch.path(), "x5-again.html", again.page());
    let relaunch_deadline = Instant::now() + LAB_DEADLINE;
    let relaunch = launch(driver, &mut spec, store, &ids, relaunch_deadline);
    let read_back = again.beat(relaunch_deadline);
    let relaunch_hello = relaunch.hello.clone();
    let relaunch_shutdown = relaunch.quit(relaunch_deadline);
    record(
        "x5",
        json!({
            "arm": arm.flag(), "store": store.name(), "store_flag": spec.store, "exit": format!("{exit:?}"),
            "scheme": scheme, "lab_pid": pid, "hello": hello, "ls_sentinel": local_sentinel,
            "ss_sentinel": session_sentinel, "first": facts5(&first), "stored": facts5(&stored),
            "confirmed": facts5(&confirmed), "lab": shutdown, "exit_wait": exit_wait, "grep": grep,
            "store_tree": tree, "relaunch_hello": relaunch_hello, "read_back": facts5(&read_back),
            "relaunch_lab": relaunch_shutdown,
        }),
    )
}

fn grep_hit(record: &Value, needle: &str) -> bool {
    record["grep"]["hits"]
        .as_array()
        .is_some_and(|hits| hits.iter().any(|hit| hit["needle"] == needle))
}

/// Oracle validity for X5: the sentinels were stored, and the persistent
/// clean-quit arm's `localStorage` sentinel is visible to the byte grep (so an
/// empty grep elsewhere means absence, not blindness).
pub(crate) fn assert_x5_controls(record: &Value) {
    assert_eq!(
        record["stored"]["res"], "ls:ok,ss:ok",
        "X5: both sentinels must be stored before the exit: {record}"
    );
    assert_eq!(
        record["confirmed"]["sent"]["ls"], record["ls_sentinel"],
        "X5: localStorage sentinel read back in the same document: {record}"
    );
    if record["store"] == "persistent" && record["exit"] == "Quit" {
        assert!(
            grep_hit(record, "ls"),
            "X5 NC: the persistent localStorage sentinel must be found on disk: {record}"
        );
        assert_eq!(
            record["read_back"]["sent"]["ls"], record["ls_sentinel"],
            "X5 NC: the persistent localStorage sentinel must survive a relaunch: {record}"
        );
    }
}

// ---------------------------------------------------------------------------
// X6 path and rebinding safety

/// A served root with a canary outside it, reachable only through the bug the
/// naive handler has (decode-then-join, symlink following).
#[derive(Debug)]
pub(crate) struct X6Root {
    pub(crate) root: PathBuf,
    pub(crate) canary: String,
    pub(crate) link: Value,
}

pub(crate) fn x6_root(scratch: &Path, page: &str) -> X6Root {
    let root = scratch.join("root");
    fs::create_dir_all(root.join("res")).expect("x6 root");
    fs::write(root.join("index.html"), page).expect("x6 page");
    fs::write(root.join("res").join("1"), "gh1081 resource /res/1").expect("x6 res");
    let outside = scratch.join("outside");
    fs::create_dir_all(&outside).expect("x6 outside");
    let canary = format!("GH1081-CANARY {}", hex32());
    fs::write(outside.join("canary.txt"), &canary).expect("x6 canary");
    let link = make_link(&outside, &root.join("link"));
    X6Root { root, canary, link }
}

#[cfg(unix)]
fn make_link(target: &Path, link: &Path) -> Value {
    let made = std::os::unix::fs::symlink(target, link);
    json!({"kind": "symlink", "ok": made.is_ok(), "error": made.err().as_ref().map(ToString::to_string)})
}

#[cfg(windows)]
fn make_link(target: &Path, link: &Path) -> Value {
    let symlink = std::os::windows::fs::symlink_dir(target, link);
    if symlink.is_ok() {
        return json!({"kind": "symlink_dir", "ok": true});
    }
    let junction = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output();
    let ok = junction
        .as_ref()
        .is_ok_and(|output| output.status.success());
    json!({"kind": "junction", "ok": ok,
        "symlink_error": symlink.err().as_ref().map(ToString::to_string),
        "junction": junction.map_or_else(|error| format!("spawn: {error}"),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_owned())})
}

/// `(name, url)` request corpus against the app origin.
pub(crate) fn x6_vectors(other_origin: &str, evil_url: &str) -> Vec<(String, String)> {
    let long = format!("res/{}", "a".repeat(4000));
    [
        ("positive", "res/1"),
        ("dotdot", "../canary.txt"),
        ("encoded_dotdot", "%2e%2e/canary.txt"),
        ("encoded_slash", "res/..%2fcanary.txt"),
        ("encoded_slashes", "res%2f..%2f..%2fcanary.txt"),
        ("backslash", "res/%5c..%5ccanary.txt"),
        ("double_slash", "//canary.txt"),
        ("symlink", "link/canary.txt"),
        ("nul", "res/%00.txt"),
        ("other_host", other_origin),
        ("glob_mismatch", evil_url),
        ("overlong", long.as_str()),
        ("unmatched", "nothere.txt"),
    ]
    .iter()
    .map(|(name, url)| ((*name).to_owned(), (*url).to_owned()))
    .collect()
}

/// The corpus against one handler mode (and, on Windows, one filter shape).
pub(crate) fn run_x6(
    driver: &mut impl Driver5,
    scaffold: &str,
    scheme: &str,
    arm: Arm,
    handler: &str,
    filter: Option<&str>,
) -> Value {
    let (other, evil) = if arm == Arm::Https {
        (
            String::from("https://x6-other.invalid/res/1"),
            String::from("https://evil.invalid/?https://x6.invalid/"),
        )
    } else {
        (
            format!("{scheme}://x6-other/res/1"),
            format!("{scheme}://evil/?{scheme}://x6/"),
        )
    };
    let vectors = x6_vectors(&other, &evil);
    let steps: Vec<String> = vectors
        .iter()
        .map(|(_, url)| format!("probe:{url}"))
        .collect();
    let step_refs: Vec<&str> = steps.iter().map(String::as_str).collect();
    let mut probe = Probe::bind(scaffold, &step_refs, &[], false);
    let scratch = tempfile::tempdir().expect("x6 scratch");
    let setup = x6_root(scratch.path(), probe.page());
    let ids = Ids::fresh();
    let title = title("X6");
    let mut spec = LaunchSpec::new(arm, &setup.root.join("index.html"), &title, scheme);
    spec.host = String::from("x6");
    spec.handler = handler.to_owned();
    spec.root = Some(setup.root.clone());
    spec.filter = filter.map(ToOwned::to_owned);
    let deadline = Instant::now() + LAB_DEADLINE;
    let mut lab = launch(driver, &mut spec, StoreArm::Ephemeral, &ids, deadline);
    let first = probe.beat(deadline);
    let mut results = Vec::new();
    for (name, url) in &vectors {
        let before = served_count(lab.command("served", deadline).as_deref());
        let beat = probe.poll(deadline);
        let after = lab.command("served", deadline);
        let handled: Vec<String> = served_entries(after.as_deref())
            .into_iter()
            .skip(before)
            .collect();
        results.push(json!({"vector": name, "url": url, "page": beat["res"], "handler": handled}));
    }
    lab.drain_events(Duration::from_millis(300));
    let requests = lab.events_with("EV req");
    let hello = lab.hello.clone();
    let shutdown = lab.quit(deadline);
    record(
        "x6",
        json!({
            "arm": arm.flag(), "handler": handler, "filter": filter, "scheme": scheme, "hello": hello,
            "link": setup.link, "canary": setup.canary, "first": facts5(&first), "results": results,
            "requests": requests, "lab": shutdown,
        }),
    )
}

static NULL_VALUE: Value = Value::Null;

fn x6_result<'a>(record: &'a Value, vector: &str) -> &'a Value {
    record["results"]
        .as_array()
        .and_then(|results| results.iter().find(|result| result["vector"] == vector))
        .unwrap_or(&NULL_VALUE)
}

/// Mutation controls: each broken handler must visibly leak, which proves the
/// corpus reaches the handler and the oracles see a leak.
pub(crate) fn assert_x6_controls(record: &Value) {
    assert!(
        x6_result(record, "positive")["page"]
            .as_str()
            .is_some_and(|page| page.starts_with("ok:200")),
        "X6: the positive vector must be served: {record}"
    );
    if record["handler"] == "naive" {
        let leaked = record["results"].as_array().is_some_and(|results| {
            results.iter().any(|result| {
                result["page"]
                    .as_str()
                    .is_some_and(|page| page.contains(":canary=1"))
            })
        });
        assert!(
            leaked,
            "X6 NC(a): the naive path-join handler must leak the canary: {record}"
        );
    }
    if record["handler"] == "nohost" {
        let served = x6_result(record, "other_host")["handler"]
            .as_array()
            .is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| entry.as_str().is_some_and(|entry| entry.contains(":200:")))
            });
        assert!(
            served,
            "X6 NC(c): without the host check the other host must be served: {record}"
        );
    }
    if record["filter"] == "loose" {
        let reached = x6_result(record, "glob_mismatch")["handler"]
            .as_array()
            .is_some_and(|entries| !entries.is_empty());
        assert!(
            reached,
            "X6 NC(b): the loose filter must admit the evil URL to the handler: {record}"
        );
    }
}

// ---------------------------------------------------------------------------
// X7 browser API compatibility

/// The `api` step on one arm, or on the real-URL control page.
pub(crate) fn run_x7(
    driver: &mut impl Driver5,
    scaffold: &str,
    scheme: &str,
    arm: Arm,
    real_control: bool,
) -> Value {
    let extra = json!({"acao": {"expected": "self", "wrong": "https://wrong.invalid"}});
    let (stage, real): (&[&str], &[&str]) = if real_control {
        (&["real"], &["api"])
    } else {
        (&["api"], &[])
    };
    let mut probe = Probe::bind_with(scaffold, stage, real, false, &extra);
    let scratch = tempfile::tempdir().expect("x7 scratch");
    let page = write_page(scratch.path(), "x7.html", probe.page());
    let ids = Ids::fresh();
    let title = title("X7");
    let mut spec = LaunchSpec::new(arm, &page, &title, scheme);
    spec.host = String::from("x7");
    let deadline = Instant::now() + LAB_DEADLINE;
    let mut lab = launch(driver, &mut spec, StoreArm::Ephemeral, &ids, deadline);
    let first = probe.beat(deadline);
    let document = if real_control {
        probe.release();
        await_successor(&mut probe, first["doc"].as_str().unwrap_or(""), deadline)
            .expect("real-URL control page reported")
    } else {
        first.clone()
    };
    probe.release();
    let api = probe
        .next_api(deadline)
        .expect("api report before deadline");
    let after = probe.beat(deadline);
    let served = lab.command("served", deadline);
    lab.drain_events(Duration::from_millis(300));
    let requests = lab.events_with("EV req");
    let hello = lab.hello.clone();
    let shutdown = lab.quit(deadline);
    let cors: Vec<String> = probe
        .seen
        .iter()
        .filter_map(|report| match report {
            Report::Cors(origin) => Some(origin.clone()),
            _ => None,
        })
        .collect();
    let probe_served: Vec<String> = probe
        .seen
        .iter()
        .filter_map(|report| match report {
            Report::Served(kind) => Some(kind.clone()),
            _ => None,
        })
        .collect();
    record(
        "x7",
        json!({
            "arm": if real_control { "real-control" } else { arm.flag() }, "scheme": scheme,
            "hello": hello, "document": facts5(&document), "api": api["api"], "after_res": after["res"],
            "ws_requests_seen": probe.count(is_ws), "cors_served": cors, "probe_served": probe_served,
            "served": served, "requests": requests, "lab": shutdown,
        }),
    )
}

fn api_value<'a>(record: &'a Value, key: &str) -> &'a str {
    record["api"][key].as_str().unwrap_or("")
}

/// Oracle validity for X7: the wrong-origin CORS answer must fail everywhere;
/// the real-URL control must pass the known-good APIs; the inline control must
/// fail the relative fetch (round 4 X1).
pub(crate) fn assert_x7_controls(record: &Value) {
    assert!(
        api_value(record, "cors_wrong").starts_with("err:"),
        "X7 NC: a wrong Access-Control-Allow-Origin must fail the CORS fetch: {record}"
    );
    if record["arm"] == "real-control" {
        assert!(
            api_value(record, "get").starts_with("ok:200"),
            "X7 NC: the real-URL page must fetch a relative resource: {record}"
        );
        assert!(
            api_value(record, "cors_ok").starts_with("ok:204"),
            "X7 NC: the echoed own origin must pass the CORS fetch: {record}"
        );
        assert!(
            api_value(record, "worker").starts_with("ok:"),
            "X7 NC: a dedicated worker must run on the real-URL page: {record}"
        );
    }
    if record["arm"] == "inline" {
        assert!(
            api_value(record, "get").starts_with("err:"),
            "X7 NC: the URL-less page cannot fetch a relative resource (X1): {record}"
        );
    }
}

// ---------------------------------------------------------------------------
// X8 redirects

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X8Case {
    /// `fetch` of handler 3xx answers: same origin, external http, other scheme.
    FetchRedirects,
    /// Navigation through a handler 3xx to the external http page.
    NavExternal,
    /// Navigation through a handler 3xx to an unregistered scheme.
    NavOtherScheme,
    /// The external page answers 302 into the app origin.
    InboundRedirect,
    /// The external page sets `location` to the app origin.
    InboundLocation,
    /// Control: the external page follows an http→http 302 (new document).
    ControlHttpRedirect,
    /// Control: the external page's `fetch` follows an http→http 302.
    ControlFetchRedirect,
}

fn x8_plans(case: X8Case, app_origin: &str) -> (Vec<String>, Vec<String>) {
    let stage = |items: &[&str]| {
        items
            .iter()
            .map(|item| (*item).to_owned())
            .collect::<Vec<_>>()
    };
    match case {
        X8Case::FetchRedirects => (
            stage(&["probe:redir/same", "probe:redir/ext", "probe:redir/other"]),
            Vec::new(),
        ),
        X8Case::NavExternal => (stage(&["go:redir/ext"]), Vec::new()),
        X8Case::NavOtherScheme => (stage(&["go:redir/other"]), Vec::new()),
        X8Case::InboundRedirect => (
            stage(&["real"]),
            vec![format!("go:redirect?to={app_origin}")],
        ),
        X8Case::InboundLocation => (stage(&["real"]), vec![format!("go:{app_origin}")]),
        X8Case::ControlHttpRedirect => (stage(&["real"]), stage(&["go:redirect?to=real"])),
        X8Case::ControlFetchRedirect => (stage(&["real"]), stage(&["probe:redirect?to=res/5"])),
    }
}

/// Drives one case after the first beat: `(fetch results, the external page
/// if one was visited, the successor document if one reported)`.
fn x8_observe(
    case: X8Case,
    probe: &mut Probe,
    first: &Value,
    deadline: Instant,
) -> (Vec<Value>, Value, Value) {
    let first_doc = first["doc"].as_str().unwrap_or("");
    let mut fetches = Vec::new();
    let mut intermediate = Value::Null;
    let mut successor = Value::Null;
    match case {
        X8Case::FetchRedirects => {
            for _ in 0..3 {
                fetches.push(probe.poll(deadline)["res"].clone());
            }
        }
        X8Case::NavExternal | X8Case::NavOtherScheme => {
            probe.release();
            let bound = if case == X8Case::NavExternal {
                SUCCESSOR_BOUND
            } else {
                Duration::from_secs(5)
            };
            successor = await_successor(probe, first_doc, Instant::now() + bound)
                .map_or(Value::Null, |beat| facts5(&beat));
        }
        X8Case::InboundRedirect
        | X8Case::InboundLocation
        | X8Case::ControlHttpRedirect
        | X8Case::ControlFetchRedirect => {
            probe.release();
            let real_page =
                await_successor(probe, first_doc, deadline).expect("real-URL page reported");
            intermediate = facts5(&real_page);
            if case == X8Case::ControlFetchRedirect {
                fetches.push(probe.poll(deadline)["res"].clone());
            } else {
                probe.release();
                successor = await_successor(
                    probe,
                    real_page["doc"].as_str().unwrap_or(""),
                    Instant::now() + SUCCESSOR_BOUND,
                )
                .map_or(Value::Null, |beat| facts5(&beat));
            }
        }
    }
    (fetches, intermediate, successor)
}

/// One redirect case; navigation cases wait a bounded time for a successor.
pub(crate) fn run_x8(
    driver: &mut impl Driver5,
    scaffold: &str,
    scheme: &str,
    arm: Arm,
    case: X8Case,
) -> Value {
    let app_origin = if arm == Arm::Https {
        String::from("https://x8.invalid/")
    } else {
        format!("{scheme}://x8/")
    };
    let (stage, real) = x8_plans(case, &app_origin);
    let stage_refs: Vec<&str> = stage.iter().map(String::as_str).collect();
    let real_refs: Vec<&str> = real.iter().map(String::as_str).collect();
    let mut probe = Probe::bind(scaffold, &stage_refs, &real_refs, false);
    let scratch = tempfile::tempdir().expect("x8 scratch");
    let page = write_page(scratch.path(), "x8.html", probe.page());
    let ids = Ids::fresh();
    let title = title("X8");
    let mut spec = LaunchSpec::new(arm, &page, &title, scheme);
    spec.host = String::from("x8");
    spec.ext_url = Some(format!("http://127.0.0.1:{}/r2/real", probe.port()));
    let deadline = Instant::now() + LAB_DEADLINE;
    let mut lab = launch(driver, &mut spec, StoreArm::Ephemeral, &ids, deadline);
    let first = probe.beat(deadline);
    let (fetches, intermediate, successor) = x8_observe(case, &mut probe, &first, deadline);
    while probe.try_next(Instant::now()).is_some() {}
    let url_after = lab.command("url", deadline);
    let served = lab.command("served", deadline);
    lab.drain_events(Duration::from_millis(500));
    let page_loads = lab.events_with("EV load");
    let navigations = lab.events_with("EV nav");
    let requests = lab.events_with("EV req");
    let hello = lab.hello.clone();
    let shutdown = lab.quit(deadline);
    let redirects: Vec<String> = probe
        .seen
        .iter()
        .filter_map(|report| match report {
            Report::Redirect(to) => Some(to.clone()),
            _ => None,
        })
        .collect();
    record(
        "x8",
        json!({
            "case": format!("{case:?}"), "arm": arm.flag(), "scheme": scheme, "app_origin": app_origin,
            "ext_url": spec.ext_url, "hello": hello, "first": facts5(&first), "fetches": fetches,
            "intermediate": intermediate, "successor": successor, "url_after": url_after,
            "page_loads": page_loads, "navigations": navigations, "requests": requests,
            "redirects_served": redirects, "redirect_count": probe.count(is_redirect),
            "page_fetches": probe.count(is_page_fetch), "documents": probe.documents(),
            "served": served, "lab": shutdown,
        }),
    )
}

/// Oracle validity for X8: the harness sees a plain http→http 302 as a
/// followed redirect, both as a navigation and as a `fetch`.
pub(crate) fn assert_x8_controls(record: &Value) {
    if record["case"] == "ControlHttpRedirect" {
        assert!(
            record["successor"]["real"] == true && record["page_fetches"] == 2,
            "X8 NC: an http→http 302 navigation must land on a new real document: {record}"
        );
        assert_eq!(
            record["redirect_count"], 1,
            "X8 NC: the server answered the 302: {record}"
        );
    }
    if record["case"] == "ControlFetchRedirect" {
        let fetched = record["fetches"][0].as_str().unwrap_or("");
        assert!(
            fetched.starts_with("ok:200") && fetched.contains(":r=1:"),
            "X8 NC: an http→http 302 fetch must be followed and flagged redirected: {record}"
        );
    }
}

// ---------------------------------------------------------------------------
// X9 renderer-process crash

/// Kills the content/renderer process from outside, records the engine's
/// reaction, then asks the harness to reload.
pub(crate) fn run_x9(driver: &mut impl Driver5, scaffold: &str, scheme: &str, arm: Arm) -> Value {
    let mut probe = Probe::bind(scaffold, &[], &[], false);
    let scratch = tempfile::tempdir().expect("x9 scratch");
    let page = write_page(scratch.path(), "x9.html", probe.page());
    let ids = Ids::fresh();
    let title = title("X9");
    let mut spec = LaunchSpec::new(arm, &page, &title, scheme);
    spec.host = String::from("x9");
    let baseline = driver.process_baseline();
    let deadline = Instant::now() + LAB_DEADLINE;
    let mut lab = launch(driver, &mut spec, StoreArm::Ephemeral, &ids, deadline);
    let first = probe.beat(deadline);
    let idle = probe.poll(deadline);
    let window = driver.window(lab.pid, &title, deadline);
    let pixels_before = driver.pixels(&window, "before");
    let census = driver.renderer_pids(&mut lab, &baseline);
    let pids: Vec<u32> = census["pids"]
        .as_array()
        .map(|pids| {
            pids.iter()
                .filter_map(Value::as_u64)
                .filter_map(|pid| u32::try_from(pid).ok())
                .collect()
        })
        .unwrap_or_default();
    let kills: Vec<Value> = pids.iter().map(|pid| driver.kill_pid(*pid)).collect();
    let killed_at = Instant::now();
    lab.drain_events(Duration::from_secs(3));
    let engine_events: Vec<String> = lab
        .events
        .iter()
        .filter(|line| line.starts_with("EV webcontent") || line.starts_with("EV procfail"))
        .cloned()
        .collect();
    probe.release();
    let post_kill_beat = probe.next_beat(Instant::now() + Duration::from_secs(2));
    let pixels_killed = driver.pixels(&window, "killed");
    let url_killed = lab.command("url", deadline);
    let reload = lab.command("reload", deadline);
    let reloaded_at = Instant::now();
    let successor = await_successor(
        &mut probe,
        idle["doc"].as_str().unwrap_or(""),
        reloaded_at + SUCCESSOR_BOUND,
    );
    let reload_ms = successor
        .as_ref()
        .map(|_| reloaded_at.elapsed().as_millis());
    while probe.try_next(Instant::now()).is_some() {}
    let pixels_after = driver.pixels(&window, "after");
    let url_after = lab.command("url", deadline);
    let served = lab.command("served", deadline);
    lab.drain_events(Duration::from_millis(300));
    let page_loads = lab.events_with("EV load");
    let navigations = lab.events_with("EV nav");
    let hello = lab.hello.clone();
    let shutdown = lab.quit(deadline);
    record(
        "x9",
        json!({
            "arm": arm.flag(), "scheme": scheme, "hello": hello, "first": facts5(&first),
            "idle": facts5(&idle), "census": census, "killed_pids": pids, "kills": kills,
            "engine_events": engine_events, "events_after_kill_ms": killed_at.elapsed().as_millis(),
            "post_kill_beat": post_kill_beat.as_ref().map(facts5), "pixels_before": pixels_before,
            "pixels_killed": pixels_killed, "url_killed": url_killed, "reload": reload,
            "successor": successor.as_ref().map(facts5), "reload_ms": reload_ms,
            "pixels_after": pixels_after, "url_after": url_after, "served": served,
            "page_loads": page_loads, "navigations": navigations, "lab": shutdown,
        }),
    )
}

/// Oracle validity for X9: a content process was found and is gone after the
/// kill (otherwise nothing was crashed and the row measures nothing).
pub(crate) fn assert_x9_controls(record: &Value) {
    let killed = record["killed_pids"]
        .as_array()
        .is_some_and(|pids| !pids.is_empty());
    assert!(
        killed,
        "X9: no content/renderer process was found to kill: {record}"
    );
    let gone = record["kills"]
        .as_array()
        .is_some_and(|kills| kills.iter().all(|kill| kill["gone"] == true));
    assert!(gone, "X9: a killed process is still alive: {record}");
}
