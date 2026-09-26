use crate::support::EVENT_DEADLINE;
use crate::support::MEDIA_PROMPT_DEADLINE;
use crate::support::PROCESS_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::cross_user::run_as_local_user_command;
use crate::support::profile_evidence::attach_profile_run_evidence;
use crate::support::profile_evidence::signed_media_executable_facts;
use crate::support::profile_renderer::profile_origin_html;
use crate::support::profile_renderer::service_worker_script;
use crate::support::profile_renderer::write_profile_http;
use crate::support::profile_renderer::write_profile_http_type;
use crate::support::signed_app::configure_profile_media_mode;
use crate::support::signed_app::signed_host_executable;
use crate::support::signed_app::stop_profile_host;
use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) struct ProfileOrigin {
    pub(crate) listener: TcpListener,
    pub(crate) address: String,
    pub(crate) pending_reports:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl ProfileOrigin {
    pub(crate) fn new() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind KEL-135 local origin");
        listener
            .set_nonblocking(true)
            .expect("make KEL-135 origin pollable");
        let address = listener
            .local_addr()
            .expect("local origin address")
            .to_string();
        Self {
            listener,
            address,
            pending_reports: std::collections::BTreeMap::new(),
        }
    }

    pub(crate) fn bind(address: &str) -> Self {
        let listener = TcpListener::bind(address)
            .expect("rebind the exact loopback origin saved before reboot");
        listener
            .set_nonblocking(true)
            .expect("make reboot origin pollable");
        let address = listener
            .local_addr()
            .expect("rebound local origin address")
            .to_string();
        Self {
            listener,
            address,
            pending_reports: std::collections::BTreeMap::new(),
        }
    }

    pub(crate) fn run_profile(
        &mut self,
        app: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            signed_host_executable(app),
            support_root,
            phase,
            log_name,
            seed,
            false,
            None,
            None,
        )
    }

    pub(crate) fn run_profile_as_user(
        &mut self,
        username: &str,
        user_temp: &Path,
        profile_root: &Path,
        app: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
    ) -> (std::collections::BTreeMap<String, String>, Output) {
        let url = format!("http://{}/{phase}?run={log_name}", self.address);
        let executable = signed_host_executable(app);
        let environment = [
            ("TMPDIR", user_temp.to_string_lossy().into_owned()),
            (
                "KELD_PROFILE_TEST_ROOT",
                profile_root.to_string_lossy().into_owned(),
            ),
            ("KELD_PROFILE_ACCEPTANCE_REPORT", String::from("1")),
            ("KELD_PROFILE_FIXTURE_URL", url),
        ];
        let mut child = run_as_local_user_command(
            username,
            executable.as_os_str(),
            &["--keld-profile-webview-fixture-v1"],
            &environment,
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch signed profile host as second standard user");
        let Some(report) = self.wait_for_report(phase, seed) else {
            if let Some(status) = child.try_wait().expect("inspect second-user profile host") {
                let _output = wait_child_output(child, PROCESS_DEADLINE);
                panic!(
                    "second-user host exited before the browser report (status={status}); private output suppressed"
                );
            }
            let _ = child.kill();
            let output = wait_child_output(child, PROCESS_DEADLINE);
            panic!(
                "second-user host did not report {phase} state (status={}); private output suppressed",
                output.status
            );
        };
        drop(child.stdin.take());
        let output = wait_child_output(child, PROCESS_DEADLINE);
        assert!(
            output.status.success(),
            "second-user profile host failed (status={})",
            output.status
        );
        (report, output)
    }

    pub(crate) fn run_ephemeral_profile(
        &mut self,
        executable: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            executable.to_owned(),
            support_root,
            phase,
            log_name,
            seed,
            true,
            None,
            None,
        )
    }

    pub(crate) fn run_profile_after_test_boot_change(
        &mut self,
        app: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            signed_host_executable(app),
            support_root,
            phase,
            log_name,
            seed,
            false,
            Some("11111111-2222-4333-8444-555555555555"),
            None,
        )
    }

    pub(crate) fn run_media_profile(
        &mut self,
        app: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
        media_mode: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            signed_host_executable(app),
            support_root,
            phase,
            log_name,
            seed,
            false,
            None,
            media_mode,
        )
    }

    pub(crate) fn run_ephemeral_media_profile(
        &mut self,
        executable: &Path,
        support_root: &Path,
        phase: &str,
        log_name: &str,
    ) -> std::collections::BTreeMap<String, String> {
        self.run_profile_executable(
            executable.to_owned(),
            support_root,
            phase,
            log_name,
            None,
            true,
            None,
            None,
        )
    }

    pub(crate) fn run_profile_executable(
        &mut self,
        executable: PathBuf,
        support_root: &Path,
        phase: &str,
        log_name: &str,
        seed: Option<&str>,
        dev_ephemeral: bool,
        boot_uuid: Option<&str>,
        media_mode: Option<&str>,
    ) -> std::collections::BTreeMap<String, String> {
        let log_path = support_root.join(format!("{log_name}.log"));
        let log = fs::File::create(&log_path).expect("create profile evidence log");
        let stderr = log.try_clone().expect("clone evidence log");
        let url = format!("http://{}/{phase}?run={log_name}", self.address);
        let executable_path = executable.display().to_string();
        let signed_facts = if phase.starts_with("media-") || phase.starts_with("query-") {
            signed_media_executable_facts(&executable)
        } else {
            None
        };
        let executable_for_check = executable.clone();
        let mut command = Command::new(executable);
        command
            .arg("--keld-profile-webview-fixture-v1")
            .env("KELD_PROFILE_TEST_ROOT", support_root)
            .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
            .env_remove("KELD_PROFILE_TEST_BOOT_UUID")
            .env_remove("KELD_PROFILE_TEST_MEDIA_SEED_ALLOW")
            .env_remove("KELD_PROFILE_TEST_MEDIA_SEED_PROMPT")
            .env_remove("KELD_PROFILE_FIXTURE_EPHEMERAL")
            .env_remove("KELD_PROFILE_FIXTURE_SECOND_URL")
            .env_remove("KELD_PROFILE_FIXTURE_SIGNED_ATTEST")
            .env_remove("KELD_PROFILE_FIXTURE_FATAL_ON_STDIN")
            .env("KELD_PROFILE_FIXTURE_URL", &url)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));
        if signed_facts.is_some() {
            command.env("KELD_PROFILE_FIXTURE_SIGNED_ATTEST", "1");
        }
        if dev_ephemeral {
            command.env("KELD_PROFILE_FIXTURE_EPHEMERAL", "1");
        }
        if let Some(boot_uuid) = boot_uuid {
            command.env("KELD_PROFILE_TEST_BOOT_UUID", boot_uuid);
        }
        configure_profile_media_mode(&mut command, media_mode, phase, &self.address, log_name);
        let mut child = command.spawn().expect("launch signed KEL-135 profile host");
        let host_pid = child.id().to_string();
        let report_deadline =
            if matches!(media_mode, Some("prompt" | "prompt-reuse" | "allow-reuse"))
                && phase.starts_with("media-seed-")
            {
                MEDIA_PROMPT_DEADLINE
            } else {
                EVENT_DEADLINE
            };
        let Some(mut report) = self.wait_for_report_with_timeout(phase, seed, report_deadline)
        else {
            if let Some(status) = child.try_wait().expect("inspect failed profile host") {
                panic!(
                    "signed profile host exited before browser report (status={status}); private log retained for inspection"
                );
            }
            let _ = child.kill();
            let status = child.wait().expect("reap timed-out signed profile host");
            panic!(
                "signed profile host did not report browser state (phase={phase}, status={status}); private log retained for inspection"
            );
        };
        if matches!(media_mode, Some("prompt-reuse" | "allow-reuse")) {
            let reuse = self
                .wait_for_report_with_timeout("media-nonce-reuse", None, EVENT_DEADLINE)
                .expect("second view reported same-store nonce without capture");
            report.insert(
                String::from("reuse_local"),
                reuse.get("local").cloned().unwrap_or_default(),
            );
            report.insert(
                String::from("reuse_media"),
                reuse.get("media").cloned().unwrap_or_default(),
            );
        }
        stop_profile_host(&mut child);
        report.insert(String::from("host_pid"), host_pid);
        report.insert(String::from("host_executable"), executable_path);
        report.insert(String::from("host_clean_exit"), String::from("true"));
        report.insert(String::from("origin"), self.address.clone());
        let output = fs::read_to_string(&log_path).expect("read signed profile host log");
        attach_profile_run_evidence(
            &mut report,
            &output,
            phase,
            &executable_for_check,
            signed_facts,
        );
        report
    }

    pub(crate) fn wait_for_report(
        &mut self,
        phase: &str,
        seed: Option<&str>,
    ) -> Option<std::collections::BTreeMap<String, String>> {
        self.wait_for_report_with_timeout(phase, seed, EVENT_DEADLINE)
    }

    pub(crate) fn wait_for_report_with_timeout(
        &mut self,
        phase: &str,
        seed: Option<&str>,
        timeout: Duration,
    ) -> Option<std::collections::BTreeMap<String, String>> {
        if let Some(report) = self.pending_reports.remove(phase) {
            return Some(report);
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .expect("bound origin request read");
                    let mut request = Vec::new();
                    let mut byte = [0_u8; 1];
                    while request.len() < 8192 {
                        match stream.read(&mut byte) {
                            Ok(0) => break,
                            Ok(_) => {
                                request.push(byte[0]);
                                if byte[0] == b'\n' {
                                    break;
                                }
                            }
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) =>
                            {
                                break;
                            }
                            Err(error) => panic!("read KEL-135 origin request: {error}"),
                        }
                    }
                    if request.is_empty() {
                        continue;
                    }
                    let request = String::from_utf8_lossy(&request);
                    let line = request.lines().next().unwrap_or_default();
                    eprintln!("KELD_KEL135_ORIGIN_REQUEST {line}");
                    let path = line.split_whitespace().nth(1).unwrap_or("/");
                    if let Some(query) = path.strip_prefix("/report?") {
                        let fields = query
                            .split('&')
                            .filter_map(|part| part.split_once('='))
                            .map(|(key, value)| (key.to_owned(), value.to_owned()))
                            .collect::<std::collections::BTreeMap<_, _>>();
                        if fields.get("phase").map(String::as_str) == Some(phase) {
                            write_profile_http(&mut stream, 204, "");
                            return Some(fields);
                        }
                        write_profile_http(&mut stream, 204, "");
                        if let Some(other_phase) = fields.get("phase") {
                            assert!(
                                self.pending_reports.len() < 4,
                                "too many unmatched fixture reports"
                            );
                            self.pending_reports.insert(other_phase.clone(), fields);
                        }
                    } else if path.starts_with("/script-started?") {
                        write_profile_http(&mut stream, 204, "");
                    } else if path.starts_with("/favicon") {
                        write_profile_http(&mut stream, 204, "");
                    } else if path.starts_with("/sw.js") {
                        write_profile_http_type(
                            &mut stream,
                            200,
                            "application/javascript; charset=utf-8",
                            service_worker_script(),
                        );
                    } else {
                        let request_phase = path
                            .trim_start_matches('/')
                            .split_once('?')
                            .map_or(path.trim_start_matches('/'), |(route, _)| route);
                        let page_seed =
                            if request_phase.starts_with("media-seed-") || request_phase == phase {
                                seed
                            } else {
                                None
                            };
                        let html = profile_origin_html(request_phase, page_seed);
                        write_profile_http(&mut stream, 200, &html);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept KEL-135 local origin request: {error}"),
            }
        }
        None
    }
}
