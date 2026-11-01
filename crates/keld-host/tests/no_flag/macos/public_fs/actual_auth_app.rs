//! Actual KEL-89 public `AppProcess` negative routes + shipping public Quit.
//! This module is a child of the existing `public_fs` fixture family solely to
//! reuse its observer/image owners; no FS assertions or renderer input actions.
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use serde_json::{Value, json};

use super::application_link::assert_single_application_link;
use super::observer::Observation;
use super::process_image::process_image_path;
use super::project::file_digest;
use crate::renderer_bridge::bundle_public_api_entry;
use crate::support::EVENT_DEADLINE;
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::lease_descriptors::lsof_all;
use crate::support::native_window::{
    NativeWindowObserver, await_no_native_windows, compile_native_window_census,
};
use crate::support::process::{
    ProcessExitObserver, await_process_gone, parent_process, process_group, session_dirs_for,
};

#[path = "actual_auth_app/quit_diagnostics.rs"]
mod quit_diagnostics;
use quit_diagnostics::wait_for_auth_cli_quit;

type SmokeResult<T> = Result<T, Box<dyn Error>>;
const TITLE: &str = "Keld Auth Smoke";
const PRIVATE_TOKEN_002: &str = "KELD-GUARD002: auth.token is outside its grant. Add the intended declared issuer to /app/auth/token in keld.permissions.jsonc.";

struct Case {
    name: &'static str,
    config: &'static str,
    manifest: &'static str,
    expected: &'static str,
}
const CASES: [Case; 4] = [
    Case {
        name: "default-deny",
        config: include_str!("../../../fixtures/auth_smoke/default-deny/keld.config.ts"),
        manifest: include_str!("../../../fixtures/auth_smoke/default-deny/keld.permissions.jsonc"),
        expected: "KELD-GUARD001",
    },
    Case {
        name: "missing-client",
        config: include_str!("../../../fixtures/auth_smoke/missing-client/keld.config.ts"),
        manifest: include_str!(
            "../../../fixtures/auth_smoke/missing-client/keld.permissions.jsonc"
        ),
        expected: "KELD-AUTH-015",
    },
    Case {
        name: "invalid-hint",
        config: include_str!("../../../fixtures/auth_smoke/invalid-hint/keld.config.ts"),
        manifest: include_str!("../../../fixtures/auth_smoke/invalid-hint/keld.permissions.jsonc"),
        expected: "KELD-AUTH-010",
    },
    Case {
        name: "unknown-token",
        config: include_str!("../../../fixtures/auth_smoke/unknown-token/keld.config.ts"),
        manifest: include_str!("../../../fixtures/auth_smoke/unknown-token/keld.permissions.jsonc"),
        expected: "KELD-GUARD002",
    },
];

const ACTIVE_BEGIN_QUIT: Case = Case {
    name: "active-begin-quit",
    // Syntactically valid static client carrier, deliberately not a registered
    // account fixture. This case makes no provider-success claim.
    config: r#"export default {
  name: "Keld Auth Smoke",
  entry: "src/main.ts",
  renderer: "index.html",
  auth: {"entra-common": {"clientId": "00000000-0000-4000-8000-000000000089"}},
} as const;
"#,
    manifest: r#"{"app":{"auth":{"begin":["https://login.microsoftonline.com/common"]}}}"#,
    expected: "pending-before-quit",
};

// Only whole independent OS descriptor records establish TCP LISTEN.
// lsof may emit its address and state fields in either order.
fn loopback_listener_ports(snapshot: &str) -> Vec<u16> {
    let mut address = None;
    let mut ipv4 = false;
    let mut listening = false;
    let mut ports = Vec::new();
    for line in snapshot.lines().chain(std::iter::once("f")) {
        if line.starts_with('f') {
            if ipv4
                && listening
                && let Some(port) = address.and_then(|value: &str| value.parse::<u16>().ok())
                && port != 0
            {
                ports.push(port);
            }
            address = None;
            ipv4 = false;
            listening = false;
        } else if line == "tIPv4" {
            ipv4 = true;
        } else if let Some(value) = line.strip_prefix('n') {
            address = value.strip_prefix("127.0.0.1:");
        } else if line == "TST=LISTEN" {
            listening = true;
        }
    }
    ports
}

// Observation only: the shared lsof owner reads the actual Host's descriptors.
// No socket connect, OAuth callback, account stimulus or provider replacement.
fn await_owned_loopback_listener(host: u32) -> SmokeResult<(u16, String)> {
    let deadline = Instant::now() + EVENT_DEADLINE;
    loop {
        observation_time_left(deadline)?;
        let snapshot = lsof_all(host);
        observation_time_left(deadline)?; // a late OS snapshot cannot satisfy this phase
        let ports = loopback_listener_ports(&snapshot);
        require(ports.len() <= 1, "Host has ambiguous loopback listeners")?;
        if let Some(port) = ports.first() {
            return Ok((*port, snapshot));
        }
        std::thread::yield_now(); // wait on the independently observed socket condition
    }
}

#[test]
fn loopback_socket_census_requires_one_complete_listening_record() {
    assert_eq!(
        loopback_listener_ports("p10\nf4\ntIPv4\nn127.0.0.1:43123\nTST=LISTEN\n"),
        [43123]
    );
    assert_eq!(
        loopback_listener_ports("p10\nf4\nTST=LISTEN\ntIPv4\nn127.0.0.1:43123\n"),
        [43123]
    );
    for snapshot in [
        "f4\ntIPv4\nn127.0.0.1:43123\nTST=ESTABLISHED\n",
        "f4\ntIPv4\nn127.0.0.1:43123->127.0.0.1:80\nTST=LISTEN\n",
        "f4\ntIPv4\nn*:43123\nTST=LISTEN\n",
        "f4\ntIPv4\nn127.0.0.1:0\nTST=LISTEN\n",
        "f4\ntIPv4\nn127.0.0.1:43123\nf5\nTST=LISTEN\n",
    ] {
        assert_eq!(loopback_listener_ports(snapshot).len(), 0);
    }
}

fn require(condition: bool, message: impl Into<String>) -> SmokeResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn pid(report: &Value, field: &str) -> SmokeResult<u32> {
    let raw = report[field]
        .as_u64()
        .ok_or_else(|| format!("missing numeric {field}"))?;
    let pid = u32::try_from(raw)?;
    require(pid > 1, "fixture PID must be positive and independent")?;
    Ok(pid)
}

struct Project {
    root: PathBuf,
    policy_digest: String,
}
impl Project {
    fn prepare(parent: &Path, repo: &Path, case: &Case, port: u16) -> SmokeResult<Self> {
        let root = parent.join(case.name);
        fs::create_dir(&root)?; // never overwrite any candidate or earlier project
        let root = fs::canonicalize(root)?;
        fs::create_dir(root.join("src"))?;
        fs::write(root.join("keld.config.ts"), case.config)?;
        fs::write(root.join("keld.permissions.jsonc"), case.manifest)?;
        let api = serde_json::to_string(
            &repo
                .join("packages/@keld/api/src/index.ts")
                .to_string_lossy(),
        )?;
        let body = include_str!("../../../fixtures/public_auth_app.ts")
            .replace("__PORT__", &port.to_string())
            .replace("__CASE__", &serde_json::to_string(case.name)?);
        require(
            !body.contains("WorkerLink") && !body.contains("invokeAuth"),
            "smoke app must use only public API calls",
        )?;
        bundle_public_api_entry(
            &root,
            repo,
            "kel89-public-auth",
            &format!("import {{ app, auth, isCallError }} from {api};\n{body}"),
        );
        let page = include_str!("../../../fixtures/public_auth_page.html")
            .replace("__PORT__", &port.to_string());
        fs::write(root.join("index.html"), page)?;
        let policy_digest = file_digest(&root.join("keld.permissions.jsonc"));
        Ok(Self {
            root,
            policy_digest,
        })
    }

    fn verify_stock_stage(
        &self,
        host_image: &Path,
        host_source: &Path,
        case: &Case,
    ) -> SmokeResult<Value> {
        let stage = host_image.parent().ok_or("staged host has no root")?;
        require(
            stage.starts_with(self.root.join(".keld/dev")),
            "kernel host image must be in this project's stock dev stage",
        )?;
        require(
            fs::read(stage.join("keld.permissions.jsonc"))? == case.manifest.as_bytes(),
            "stock producer changed explicit narrow policy bytes",
        )?;
        require(
            file_digest(host_image) == file_digest(host_source),
            "kernel staged Host differs from freshly selected sibling Host",
        )?;
        for relative in ["src/main.ts", "src/kipc-transport.ts", "index.html"] {
            require(
                fs::read(stage.join(relative))? == fs::read(self.root.join(relative))?,
                format!("stock stage changed exact project input {relative}"),
            )?;
        }
        let boot: Value = serde_json::from_slice(&fs::read(stage.join("keld.boot.json"))?)?;
        require(
            boot["schema"] == 1
                && boot["name"] == TITLE
                && boot["entry"] == "src/main.ts"
                && boot["renderer"] == "index.html",
            "stock boot selected unexpected configured source/window",
        )?;
        require(
            boot["permissions"]["content_sha256"] == format!("sha256:{}", self.policy_digest),
            "stock descriptor does not bind exact policy bytes",
        )?;
        if case.name == "invalid-hint" || case.name == "active-begin-quit" {
            require(
                boot.pointer("/auth/entra-common/clientId")
                    == Some(&json!(if case.name == "active-begin-quit" {
                        "00000000-0000-4000-8000-000000000089"
                    } else {
                        "keld-smoke-placeholder-no-registration"
                    })),
                "static auth carrier failed to reach stock boot",
            )?;
        }
        Ok(boot)
    }
}

fn persist_phase_status(root: &Path, status: &'static str, phases: &[Value]) -> SmokeResult<()> {
    fs::write(
        root.join("phase-journal.json"),
        serde_json::to_vec_pretty(&json!({
            "status": status, "diagnosticOnly": true, "lastReceivedPhases": phases,
        }))?,
    )?;
    Ok(())
}
fn phase_summary(report: &Value) -> Value {
    let phase = match report["phase"].as_str() {
        Some("app-ready") => "app-ready",
        Some("page-ready") => "page-ready",
        Some("app-result") => "app-result",
        Some("begin-pending") => "begin-pending",
        Some("begin-quit-terminal") => "begin-quit-terminal",
        Some("page-diagnostic") => "page-diagnostic",
        _ => "unexpected-phase",
    };
    let codes: Vec<_> = report["outcomes"]
        .as_array()
        .into_iter()
        .flatten()
        .take(2)
        .map(|outcome| match outcome["code"].as_str() {
            Some("KELD-GUARD001") => "KELD-GUARD001",
            Some("KELD-GUARD002") => "KELD-GUARD002",
            Some("KELD-AUTH-015") => "KELD-AUTH-015",
            Some("KELD-AUTH-010") => "KELD-AUTH-010",
            _ => "unexpected-code",
        })
        .collect();
    let safe_type = |field: &str| match report[field].as_str() {
        Some("undefined") => "undefined",
        Some("object") => "object",
        Some("function") => "function",
        Some("boolean") => "boolean",
        _ => "other-type",
    };
    let error_name = match report["errorName"].as_str() {
        Some("TypeError") => "TypeError",
        Some("ReferenceError") => "ReferenceError",
        Some("SecurityError") => "SecurityError",
        Some("NotSupportedError") => "NotSupportedError",
        Some("QuotaExceededError") => "QuotaExceededError",
        Some("InvalidStateError") => "InvalidStateError",
        Some("OtherError") => "OtherError",
        _ => "none",
    };
    json!({"phase":phase,"reportedPid":report["pid"].as_u64(),"knownOutcomeCodes":codes,
        "pageFeatures":{"cryptoType":safe_type("cryptoType"),"randomUUIDType":safe_type("randomUUIDType"),
            "getRandomValuesType":safe_type("getRandomValuesType"),"secureContextType":safe_type("secureContextType"),
            "isSecureContext":report["isSecureContext"].as_bool(),"errorName":error_name}})
}

// One absolute clock guard for fixture observation phases. A queued record
// never revives an expired phase; scenario interpretation stays at the caller.
fn observation_time_left(
    deadline: Instant,
) -> Result<std::time::Duration, std::sync::mpsc::RecvTimeoutError> {
    let now = Instant::now();
    if now >= deadline {
        return Err(std::sync::mpsc::RecvTimeoutError::Timeout);
    }
    Ok(deadline.saturating_duration_since(now))
}

fn await_phases(
    observation: &Observation,
    project: &Path,
    case: &Case,
) -> SmokeResult<(Value, Value, Value, Vec<Value>)> {
    let deadline = Instant::now() + EVENT_DEADLINE;
    let mut app_ready = None;
    let mut page_ready = None;
    let mut result = None;
    let mut journal = Vec::new();
    let mut diagnostic_phases = Vec::new();
    persist_phase_status(project, "waiting", &diagnostic_phases)?;
    while app_ready.is_none() || page_ready.is_none() || result.is_none() {
        let report = match observation_time_left(deadline)
            .and_then(|remaining| observation.reports.recv_timeout(remaining))
        {
            Ok(report) => report,
            Err(error) => {
                let status = match error {
                    std::sync::mpsc::RecvTimeoutError::Timeout => "timed-out",
                    std::sync::mpsc::RecvTimeoutError::Disconnected => "observer-disconnected",
                };
                persist_phase_status(project, status, &diagnostic_phases)?;
                return Err(error.into());
            }
        };
        if diagnostic_phases.len() == 32 {
            diagnostic_phases.remove(0);
        }
        diagnostic_phases.push(phase_summary(&report));
        persist_phase_status(project, "received-phase", &diagnostic_phases)?;
        match report["phase"].as_str() {
            Some("app-ready") => app_ready = Some(report.clone()),
            Some("page-ready") => page_ready = Some(report.clone()),
            Some("app-result") if case.name != "active-begin-quit" => result = Some(report.clone()),
            Some("begin-pending") if case.name == "active-begin-quit" => {
                result = Some(report.clone());
            }
            Some("page-diagnostic") => {} // observation only, never satisfies page-ready or resets deadline
            _ => {
                persist_phase_status(project, "unexpected-phase", &diagnostic_phases)?;
                return Err("unexpected actual fixture observation phase".into());
            }
        }
        if journal.len() == 32 {
            journal.remove(0);
        }
        journal.push(report);
    }
    persist_phase_status(project, "required-phases-received", &diagnostic_phases)?;
    match (app_ready, page_ready, result) {
        (Some(app), Some(page), Some(result)) => Ok((app, page, result, journal)),
        _ => Err("required observations absent".into()),
    }
}
fn verify_outcomes(case: &Case, result: &Value, app_pid: u32) -> SmokeResult<()> {
    require(
        pid(result, "pid")? == app_pid && result["smokeCase"] == case.name,
        "outcome is not this actual AppProcess/case",
    )?;
    let outcomes = result["outcomes"]
        .as_array()
        .ok_or("missing public API outcomes")?;
    require(
        outcomes.len() == if case.name == "unknown-token" { 2 } else { 1 },
        "wrong public call count",
    )?;
    for outcome in outcomes {
        require(
            outcome["ok"] == false && outcome["code"] == case.expected,
            "actual public auth outcome disagrees with expected typed refusal",
        )?;
        let message = outcome["message"]
            .as_str()
            .ok_or("missing typed fix message")?;
        require(
            !message.contains("KEL89_SMOKE_HINT"),
            "private login hint leaked in typed error",
        )?;
        if case.name == "unknown-token" {
            require(
                message == PRIVATE_TOKEN_002,
                "unknown account token denial is not exact resource-free constant002",
            )?;
            require(
                !message.contains("UNKNOWN_A")
                    && !message.contains("UNKNOWN_B")
                    && !message.contains("https://")
                    && !message.contains("unresolved"),
                "token denial leaked account/resource/sentinel",
            )?;
        }
    }
    Ok(())
}

fn assert_product_cleanup(
    exits: &[std::process::ExitStatus; 4],
    targets: [u32; 5],
    host: u32,
    host_image: &Path,
    session_dir: &Path,
    observe_native_windows: bool,
) -> SmokeResult<()> {
    require(
        exits[0].success() && exits[1].success() && exits[2].success(),
        "Host/guardian/Bun orderly public Quit did not exit zero",
    )?;
    for target in targets {
        await_process_gone(target);
    }
    if observe_native_windows {
        await_no_native_windows(host, TITLE);
    }
    require(
        !host_image.parent().ok_or("stage root")?.exists(),
        "stock CLI left dev-stage root after public Quit",
    )?;
    require(
        !session_dir.exists(),
        "public Quit left the authenticated app-link session directory",
    )?;
    let other_session_dirs = session_dirs_for(host);
    require(
        other_session_dirs.is_empty(),
        format!(
            "public Quit left {} Host-owned app-link session directories",
            other_session_dirs.len()
        ),
    )?;
    Ok(())
}

fn assert_application_link_provenance(host: u32, bun: u32, app_ready: &Value) -> SmokeResult<()> {
    let bun_image = process_image_path(bun);
    let reported_image = app_ready["executable"]
        .as_str()
        .ok_or("missing ordinary app executable")?;
    require(
        fs::canonicalize(&bun_image)? == fs::canonicalize(reported_image)?,
        "kernel Bun image differs from ready AppProcess",
    )?;
    assert_single_application_link(
        host,
        bun,
        app_ready["endpoint"]
            .as_str()
            .ok_or("missing ordinary app endpoint")?,
    );
    Ok(())
}

fn await_active_begin_terminal(
    observation: &Observation,
    case: &Case,
    bun: u32,
    active: bool,
) -> SmokeResult<Option<Value>> {
    Ok(if active {
        let deadline = Instant::now() + EVENT_DEADLINE;
        let report = loop {
            let report = observation
                .reports
                .recv_timeout(observation_time_left(deadline)?)?;
            if report["phase"] == "page-diagnostic" {
                continue;
            }
            break report;
        };
        require(
            report["phase"] == "begin-quit-terminal"
                && pid(&report, "pid")? == bun
                && report["smokeCase"] == case.name
                && (report["code"] == "KELD-IPC-022" || report["code"] == "KELD-IPC-023"),
            "pending actual public Begin did not terminate through accepted Quit",
        )?;
        Some(report)
    } else {
        None
    })
}

fn verify_call_before_quit(
    case: &Case,
    result: &Value,
    bun: u32,
    host: u32,
) -> SmokeResult<Option<Value>> {
    Ok(if case.name == "active-begin-quit" {
        require(
            pid(result, "pid")? == bun && result["smokeCase"] == case.name,
            "pending marker is not this actual AppProcess",
        )?;
        let (port, snapshot) = await_owned_loopback_listener(host)?;
        // No child-reported claim counts as admission/socket proof.
        Some(
            json!({"host":host,"port":port,"descriptorSnapshot":snapshot,
            "observer":"existing lsof_all numeric TCP LISTEN census","callbackStimulus":false}),
        )
    } else {
        verify_outcomes(case, result, bun)?;
        None
    })
}

/// Require the complete native process-parent chain before any signal, lease
/// ownership, or successful public Quit witness is attributed to this case.
fn observed_auth_family(app_ready: &Value, cli_pid: u32) -> SmokeResult<[u32; 4]> {
    let bun = pid(app_ready, "pid")?;
    let descendant = pid(app_ready, "descendantPid")?;
    let guardian = parent_process(bun);
    let host = parent_process(guardian);
    require(
        parent_process(host) == cli_pid && parent_process(descendant) == bun,
        "kernel process lineage must be CLI -> Host -> guardian -> Bun -> held tail",
    )?;
    Ok([host, guardian, bun, descendant])
}

fn verify_completed_auth_quit(output: &std::process::Output, project: &Path) -> SmokeResult<()> {
    require(
        output.status.success(),
        format!(
            "shipping CLI public Quit failed; exit code {:?}",
            output.status.code()
        ),
    )?;
    require(
        quit_diagnostics::completed_quit_checkpoints_visible(project),
        "successful auth Quit did not expose fixed HTTP and correlated lifecycle checkpoints",
    )
}

fn run_case(
    parent: &Path,
    repo: &Path,
    cli: &Path,
    host_source: &Path,
    census: Option<&Path>,
    case: &Case,
) -> SmokeResult<Value> {
    let mut observation = Observation::bind();
    let project = Project::prepare(parent, repo, case, observation.port)?;
    let mut window = census.map(|path| NativeWindowObserver::arm_for_title(path, TITLE));
    let stdout = fs::File::create(project.root.join("cli.stdout"))?;
    let stderr = fs::File::create(project.root.join("cli.stderr"))?;
    fs::write(
        project.root.join("launch-status.json"),
        serde_json::to_vec_pretty(&json!({
            "status":"starting-cli", "case":case.name,
        }))?,
    )?;
    let cli_child = Command::new(cli)
        .arg("dev")
        .current_dir(&project.root)
        .env_remove("KELD_APP_LINK")
        .env_remove("KELD_DEV_LEASE")
        .env_remove("KELD_INTERNAL_MACOS_GUARDIAN_REGISTRATION")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()?;
    let cli_pid = cli_child.id();
    let mut cleanup = ShippingLaunchCleanup::new(cli_child);
    fs::write(
        project.root.join("launch-status.json"),
        serde_json::to_vec_pretty(&json!({
            "status":"cli-spawned", "case":case.name, "cliPid":cli_pid,
        }))?,
    )?;
    let (app_ready, page_ready, result, journal) = await_phases(&observation, &project.root, case)?;
    let [host, guardian, bun, descendant] = observed_auth_family(&app_ready, cli_pid)?;
    cleanup.record_authenticated_groups(process_group(host), process_group(bun));
    let host_image = process_image_path(host);
    let boot = project.verify_stock_stage(&host_image, host_source, case)?;
    let windows = window.as_mut().map_or_else(Vec::new, |window| {
        window.expect_initial(host, "actual public auth stock Host window")
    });
    let endpoint = PathBuf::from(
        app_ready["endpoint"]
            .as_str()
            .ok_or("missing ordinary app endpoint")?,
    );
    let session_dir = endpoint
        .parent()
        .ok_or("missing app endpoint parent")?
        .to_path_buf();
    assert_application_link_provenance(host, bun, &app_ready)?;
    eprintln!(
        "KELD_KEL89_APP_LINK_PREQUIT case={} original_session_dir_visible={}",
        case.name,
        session_dir.exists()
    );
    require(
        page_ready["marker"] == "KEL89_AUTH_SMOKE_WINDOW",
        "actual configured document marker missing",
    )?;
    let active_listener = verify_call_before_quit(case, &result, bun, host)?;
    // Independent OS exits are armed before the ordinary observer releases
    // the app's public Quit. App JSON/PIDs are never counted as cleanup proof.
    let host_exit = ProcessExitObserver::arm(host);
    let guardian_exit = ProcessExitObserver::arm(guardian);
    let bun_exit = ProcessExitObserver::arm(bun);
    let descendant_exit = ProcessExitObserver::arm(descendant);
    observation.quit.send(())?;
    let cli_child = cleanup.cli.take().ok_or("missing retained CLI owner")?;
    let output = wait_for_auth_cli_quit(
        cli_child,
        &observation,
        &project.root,
        case,
        [host, guardian, bun, descendant],
    );
    verify_completed_auth_quit(&output, &project.root)?;
    let exits = [
        host_exit.wait(),
        guardian_exit.wait(),
        bun_exit.wait(),
        descendant_exit.wait(),
    ];
    assert_product_cleanup(
        &exits,
        [cli_pid, host, guardian, bun, descendant],
        host,
        &host_image,
        &session_dir,
        census.is_some(),
    )?;
    let active_terminal =
        await_active_begin_terminal(&observation, case, bun, active_listener.is_some())?;
    cleanup.host_group = None;
    cleanup.bun_group = None; // disarm fallback only AFTER independently observed product cleanup
    observation.stop_and_join();
    Ok(
        json!({"case":case.name,"scope":if case.name == "active-begin-quit" { "active allowed Begin + public Quit owner cleanup only" } else { "negative actual public auth + public Quit only" }, "project":project.root,
        "cli":cli_pid,"host":host,"guardian":guardian,"bun":bun,"heldDescendant":descendant,
        "nativeWindows":windows,"boot":boot,"reports":journal,"expected":case.expected,
        "cliExit":output.status.code(),"productExits":exits.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "activeLoopbackBeforeQuit":active_listener,"activeBeginTerminal":active_terminal,
        "externalBrowser":"OS-owned; may remain after Quit by design; no browser-process kill claim",
        "sourceApiOnly":true,"inputActions":0,"realProviderSuccess":false,"sdkBrowserNoEffectCensus":"separate Root owner, not app-body proof"}),
    )
}

#[test]
fn actual_public_auth_negative_smoke_and_public_quit() -> SmokeResult<()> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("workspace root")?;
    let host = PathBuf::from(env!("CARGO_BIN_EXE_keld-host"));
    let cli = host.parent().ok_or("host installation root")?.join("keld");
    require(
        cli.is_file() && host.is_file(),
        "Root must freshly build/install sibling keld and keld-host for this source epoch",
    )?;
    let root = tempfile::Builder::new()
        .prefix("kel89-auth-smoke-")
        .tempdir_in("/tmp")?
        .keep();
    eprintln!("KELD_KEL89_SMOKE_PROJECTS {}", root.display()); // durable Root-owned artifacts, not deleted by test
    let census = compile_native_window_census(&root);
    let mut receipts = Vec::new();
    for case in &CASES {
        receipts.push(run_case(&root, repo, &cli, &host, Some(&census), case)?);
    }
    let receipt = json!({"sourceEpoch":"Root build/source receipt required", "cliSha256":file_digest(&cli), "hostSha256":file_digest(&host),
        "cases":receipts,"parked":"registered real provider browser/PKCE/token success; negative smoke is not feature completion"});
    fs::write(
        root.join("smoke-receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(())
}

#[test]
fn actual_public_auth_pending_begin_public_quit() -> SmokeResult<()> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("workspace root")?;
    let host = PathBuf::from(env!("CARGO_BIN_EXE_keld-host"));
    let cli = host.parent().ok_or("host installation root")?.join("keld");
    require(
        cli.is_file() && host.is_file(),
        "Root must freshly build/install sibling keld and keld-host for this source epoch",
    )?;
    let root = tempfile::Builder::new()
        .prefix("kel89-active-begin-quit-")
        .tempdir_in("/tmp")?
        .keep();
    eprintln!("KELD_KEL89_ACTIVE_BEGIN_QUIT_PROJECT {}", root.display());
    let census = compile_native_window_census(&root);
    let receipt = run_case(&root, repo, &cli, &host, Some(&census), &ACTIVE_BEGIN_QUIT)?;
    fs::write(
        root.join("active-begin-quit-receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(())
}

#[test]
fn consecutive_actual_public_auth_negative_quits_reap_real_hosts_without_graphics_capture()
-> SmokeResult<()> {
    // A native diagnostic for the hosted second-launch Quit timeout. Keep the
    // stock CLI, host, Guardian, Bun, policy, API, observer and OS exit oracles
    // unchanged; omit only CoreGraphics, unavailable on the local Mac.
    // The original GUI selector above remains required for hosted acceptance.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("workspace root")?;
    let host = PathBuf::from(env!("CARGO_BIN_EXE_keld-host"));
    let cli = host.parent().ok_or("host installation root")?.join("keld");
    require(
        cli.is_file() && host.is_file(),
        "shipping CLI and Host must exist",
    )?;
    let owned = tempfile::Builder::new()
        .prefix("kel89-quit-no-graphics-")
        .tempdir_in("/tmp")?;
    // Preserve the original four-case order. In hosted failure evidence, the
    // first case retires and the second timed out; the same observer/CLI path
    // must stay reusable for the two later negative cases as well.
    for case in &CASES {
        let _ = run_case(owned.path(), repo, &cli, &host, None, case)?;
    }
    Ok(())
}

#[test]
fn expired_observation_phase_rejects_an_already_queued_record() {
    let (send, receive) = std::sync::mpsc::channel();
    send.send(json!({"phase":"page-diagnostic"}))
        .expect("queued independent observation");
    let deadline = Instant::now();
    let result =
        observation_time_left(deadline).and_then(|remaining| receive.recv_timeout(remaining));
    assert!(matches!(
        result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    assert_eq!(
        receive
            .try_recv()
            .expect("expired phase must not consume queued data")["phase"],
        "page-diagnostic"
    );
}

#[test]
fn active_begin_fixture_preserves_template_config_fields() {
    assert_eq!(
        keld_core::title_from_config_ts(ACTIVE_BEGIN_QUIT.config).as_deref(),
        Some(TITLE)
    );
    assert_eq!(
        keld_core::entry_from_config_ts(ACTIVE_BEGIN_QUIT.config).as_deref(),
        Some("src/main.ts")
    );
    assert_eq!(
        keld_core::renderer_from_config_ts(ACTIVE_BEGIN_QUIT.config).as_deref(),
        Some("index.html")
    );
}
