//! GH-1081 round-1 research only (never merged): E5 on Windows. Document
//! continuity, the `WebView2` process family and Job-object membership across
//! one real Bun crash in the unmodified staged no-flag host.

use std::fs;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Instant;

use serde_json::{Value, json};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::JobObjects::{CreateJobObjectW, IsProcessInJob};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

use crate::crash_recovery_r1_heartbeat::{Heartbeat, same_document};
use crate::support::control::{accept_ready_generation, read_control_line};
use crate::support::handles::{duplicate_handles_of_type, own_handle_type_index};
use crate::support::product::ProductFixture;
use crate::{
    DARK_BG, PRODUCT_DEADLINE, PRODUCT_TITLE, dev_stage_command, wait_child, wait_for_host_window,
};

pub(crate) fn deadline() -> Instant {
    Instant::now() + PRODUCT_DEADLINE
}

/// One staged host launch and its current generation's control stream.
pub(crate) struct Launch {
    _stage: keld_cli::boot::DevBootStage,
    child: Child,
    pub(crate) host: u32,
    pub(crate) reader: BufReader<TcpStream>,
    pub(crate) writer: TcpStream,
    pub(crate) bun: u32,
    pub(crate) link: String,
}

impl Launch {
    pub(crate) fn start(fixture: &ProductFixture, control: &TcpListener) -> Self {
        let stage = keld_cli::boot::stage_dev_boot(
            &fixture.project,
            Path::new(env!("CARGO_BIN_EXE_keld-host")),
        )
        .expect("stage gh1081 host");
        let port = control.local_addr().expect("control address").port();
        let mut child = dev_stage_command(stage.root(), stage.host())
            .env("KELD_T1B_CONTROL", port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch gh1081 host");
        let host = child.id();
        let (reader, writer, bun, link) = accept_ready_generation(control, &mut child);
        Self {
            _stage: stage,
            child,
            host,
            reader,
            writer,
            bun,
            link,
        }
    }

    pub(crate) fn crash_and_accept_successor(
        &mut self,
        control: &TcpListener,
    ) -> std::time::Duration {
        writeln!(self.writer, "CRASH").expect("crash g1");
        self.writer.flush().expect("flush g1 crash");
        let started = Instant::now();
        let mut closed = String::new();
        let _ = self.reader.read_line(&mut closed);
        let (reader, writer, bun, link) = accept_ready_generation(control, &mut self.child);
        let to_ready = started.elapsed();
        self.reader = reader;
        self.writer = writer;
        self.bun = bun;
        self.link = link;
        to_ready
    }

    pub(crate) fn quit(mut self) {
        self.writer.write_all(b"QUIT\n").expect("request Quit");
        self.writer.flush().expect("flush Quit");
        assert_eq!(read_control_line(&mut self.reader), "QUIT_REPLY");
        assert_eq!(read_control_line(&mut self.reader), "LINK_EOF");
        let status = wait_child(&mut self.child, deadline());
        assert!(status.success(), "gh1081 host exited {status}");
    }
}

/// Every process from one CIM snapshot, as `(pid, parent, name, command line)`.
fn process_table() -> Vec<(u32, u32, String, String)> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CommandLine | ConvertTo-Json -Compress",
        ])
        .output()
        .expect("query process table");
    assert!(output.status.success(), "process table: {output:?}");
    let rows: Value = serde_json::from_slice(&output.stdout).expect("process table JSON");
    rows.as_array()
        .expect("process table rows")
        .iter()
        .map(|row| {
            let number = |key: &str| {
                u32::try_from(row[key].as_u64().unwrap_or_default()).unwrap_or_default()
            };
            (
                number("ProcessId"),
                number("ParentProcessId"),
                row["Name"].as_str().unwrap_or_default().to_owned(),
                row["CommandLine"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

/// `msedgewebview2.exe` descendants of the host with their `--type` role
/// (`browser` when the command line has none).
fn webview2_family(host: u32) -> Vec<(u32, u32, String)> {
    let table = process_table();
    let mut family = vec![host];
    let mut index = 0;
    while index < family.len() {
        let parent = family[index];
        for (pid, ppid, _, _) in &table {
            if *ppid == parent && *pid != parent && !family.contains(pid) {
                family.push(*pid);
            }
        }
        index += 1;
    }
    let mut rows: Vec<(u32, u32, String)> = table
        .iter()
        .filter(|(pid, _, name, _)| {
            family.contains(pid) && name.eq_ignore_ascii_case("msedgewebview2.exe")
        })
        .map(|(pid, ppid, _, command)| {
            let role = command
                .split_whitespace()
                .find_map(|argument| argument.strip_prefix("--type="))
                .unwrap_or("browser")
                .to_owned();
            (*pid, *ppid, role)
        })
        .collect();
    rows.sort();
    rows
}

fn browser_pids(family: &[(u32, u32, String)]) -> Vec<u32> {
    family
        .iter()
        .filter(|(_, _, role)| role == "browser")
        .map(|(pid, _, _)| *pid)
        .collect()
}

fn open_query(pid: u32) -> Option<OwnedHandle> {
    // SAFETY: OpenProcess takes no pointers; a null result is rejected before
    // ownership is taken of the fresh handle.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    // SAFETY: raw is the non-null owning handle OpenProcess just returned.
    (!raw.is_null()).then(|| unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

fn in_job(process: &OwnedHandle, job: HANDLE) -> Option<bool> {
    let mut member = 0;
    // SAFETY: process is live with query access; job is null (any Job) or a
    // live duplicated Job handle; member is writable for the call.
    let ok = unsafe { IsProcessInJob(process.as_raw_handle().cast(), job, &raw mut member) };
    (ok != 0).then_some(member != 0)
}

/// Read-only Job membership matrix: each process against "any Job" and against
/// every Job handle the host holds, by duplicating those handles.
fn job_membership(host: u32, processes: &[(&str, u32)]) -> Value {
    // SAFETY: null attributes and name request one unnamed Job; null is checked.
    let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    assert!(
        !raw.is_null(),
        "probe Job: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: raw is the fresh non-null owning handle returned above.
    let probe = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
    let job_type = own_handle_type_index(&probe);
    let jobs = duplicate_handles_of_type(host, job_type);
    let rows: Vec<Value> = processes
        .iter()
        .map(|(label, pid)| {
            let process = open_query(*pid);
            let any = process
                .as_ref()
                .and_then(|process| in_job(process, std::ptr::null_mut()));
            let host_jobs: Vec<Option<bool>> = jobs
                .iter()
                .map(|job| {
                    process
                        .as_ref()
                        .and_then(|process| in_job(process, job.as_raw_handle().cast()))
                })
                .collect();
            json!({"label": label, "pid": pid, "opened": process.is_some(),
                "any_job": any, "in_host_job_handles": host_jobs})
        })
        .collect();
    json!({"host_job_handles": jobs.len(), "rows": rows})
}

/// E5: same document nonce and one page-ready across a Bun crash; `WebView2`
/// browser PID before/after; whether host, Bun and `WebView2` share a Job.
/// Negative control: a host relaunch must fail the same-document oracle.
#[test]
fn gh1081_e5_windows_document_webview2_and_jobs_across_bun_crash() {
    let fixture = ProductFixture::new();
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("bind gh1081 control");
    let mut heartbeat = Heartbeat::bind();
    fs::write(
        fixture.project.join("index.html"),
        heartbeat.page(PRODUCT_TITLE, DARK_BG),
    )
    .expect("write gh1081 heartbeat renderer");

    let mut launch = Launch::start(&fixture, &control);
    let host = launch.host;
    let first_bun = launch.bun;
    let first_link = launch.link.clone();
    let initial = heartbeat.next(deadline());
    assert_eq!(initial.seq, 0, "first beat is the document's start");
    let window_before = wait_for_host_window(host, deadline());
    let before = heartbeat.poll(deadline());
    let family_before = webview2_family(host);
    let jobs_before = job_membership(
        host,
        &[
            ("host", host),
            ("bun", first_bun),
            (
                "webview2-browser",
                browser_pids(&family_before).first().copied().unwrap_or(0),
            ),
        ],
    );

    let to_ready = launch.crash_and_accept_successor(&control);
    let window_after = wait_for_host_window(host, deadline());
    let after = heartbeat.poll(deadline());
    let family_after = webview2_family(host);
    let jobs_after = job_membership(
        host,
        &[
            ("host", host),
            ("bun", launch.bun),
            (
                "webview2-browser",
                browser_pids(&family_after).first().copied().unwrap_or(0),
            ),
        ],
    );
    let documents_across_crash = heartbeat.documents();
    let verdict = same_document(&before, &after);
    eprintln!(
        "KELD_GH1081_E5 host={host} first_bun={first_bun} second_bun={} crash_to_successor_ready_ms={} window_before={window_before} window_after={window_after} before={before:?} after={after:?} documents={documents_across_crash} session_storage={} webview2_before={family_before:?} webview2_after={family_after:?} jobs_before={jobs_before} jobs_after={jobs_after} verdict={verdict:?}",
        launch.bun,
        to_ready.as_millis(),
        after.storage,
    );
    let second_bun = launch.bun;
    let second_link = launch.link.clone();
    launch.quit();

    // Negative control: a fresh host launch is a new document.
    let relaunch = Launch::start(&fixture, &control);
    let relaunched = heartbeat.next(deadline());
    let control_verdict = same_document(&before, &relaunched);
    eprintln!("KELD_GH1081_E5_NC relaunched={relaunched:?} verdict={control_verdict:?}");
    relaunch.quit();

    verdict.expect("E5: same renderer document across the Bun crash");
    assert_eq!(after.seq, before.seq + 1, "heartbeat sequence continued");
    assert_eq!(documents_across_crash, 1, "E5: exactly one page-ready");
    assert!(control_verdict.is_err(), "NC: a relaunch passed the oracle");
    assert_ne!(first_bun, second_bun, "fresh Bun process");
    assert_ne!(first_link, second_link, "fresh app-link authority");
    assert_eq!(window_before["handle"], window_after["handle"], "HWND");
    // WebView2 facts last, so the document verdict is recorded independently.
    assert!(
        !browser_pids(&family_before).is_empty(),
        "no WebView2 browser process under the host: {family_before:?}"
    );
    assert_eq!(
        browser_pids(&family_before),
        browser_pids(&family_after),
        "WebView2 browser process changed across the Bun crash"
    );
}
