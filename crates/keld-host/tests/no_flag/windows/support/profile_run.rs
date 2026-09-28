//! Signed state-run fixture, ordered host/Bun guard and per-store report owner.

use super::control::{
    accept_control_or_host_failure, parse_descendant_pid, read_control_line,
    read_control_line_or_host_failure,
};
use super::process::process_exists;
use super::product::ProductFixture;
use super::profile_observation::ProfileStateObservation;
use super::profile_response::state_redirect_html;
use super::profile_server::ProfileStateServer;
use super::signed_process::SignedStateProcessGuard;
use super::window::wait_for_host_window;
use crate::PRODUCT_DEADLINE;
use std::fs;
use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

pub(crate) struct SignedProfileStateCase<'a> {
    pub(crate) name: &'a str,
    pub(crate) host: &'a std::ffi::OsStr,
    pub(crate) before: &'a str,
    pub(crate) after: &'a str,
}

pub(crate) fn run_signed_profile_state_case(
    fixture: &ProductFixture,
    control_listener: &TcpListener,
    state_server: &ProfileStateServer,
    run_nonce: &str,
    case: &SignedProfileStateCase<'_>,
) {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    state_server.expect_case(case.name, deadline);
    let run = SignedProfileStateRun::start(
        fixture,
        control_listener,
        state_server.address(),
        run_nonce,
        case,
        deadline,
    );
    let observed = state_server.wait_for_case(case.name, deadline);
    record_profile_state_case(
        &observed,
        case,
        run_nonce,
        state_server.address(),
        run.process.host_pid(),
        run.bun_pid,
    );
    run.finish();
}

pub(crate) fn record_profile_state_case(
    observed: &ProfileStateObservation,
    case: &SignedProfileStateCase<'_>,
    run_nonce: &str,
    address: SocketAddr,
    host_pid: u32,
    bun_pid: u32,
) {
    assert_eq!(observed.nonce, run_nonce, "{} run nonce", case.name);
    observed
        .before
        .assert_value(case.before, case.name, "before");
    observed.after.assert_value(case.after, case.name, "after");
    println!(
        "KELD_KEL135_STORAGE {}",
        serde_json::json!({
            "case": case.name, "nonce": run_nonce, "origin": format!("http://{address}"),
            "before": observed.before.json(), "after": observed.after.json(),
            "host_pid": host_pid, "bun_pid": bun_pid,
        })
    );
}

pub(crate) struct SignedProfileStateRun {
    // Drop the process guard before releasing the staged namespace pins.
    pub(crate) process: SignedStateProcessGuard,
    reader: BufReader<TcpStream>,
    pub(crate) bun_pid: u32,
    deadline: Instant,
    case_name: String,
    _stage: keld_cli::boot::DevBootStage,
}

impl SignedProfileStateRun {
    pub(crate) fn start(
        fixture: &ProductFixture,
        control_listener: &TcpListener,
        address: SocketAddr,
        run_nonce: &str,
        case: &SignedProfileStateCase<'_>,
        deadline: Instant,
    ) -> Self {
        fs::write(
            fixture.project.join("index.html"),
            state_redirect_html(address, case.name, run_nonce, case.after),
        )
        .expect("write state renderer");
        let stage = keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(case.host))
            .expect("stage signed state host");
        let control_port = control_listener
            .local_addr()
            .expect("state control address")
            .port();
        let child = Command::new(stage.host())
            .current_dir(stage.root())
            .env("KELD_T1B_CONTROL", control_port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch signed state host");
        let mut process = SignedStateProcessGuard::new(child);
        let control =
            accept_control_or_host_failure(control_listener, process.child_mut(), deadline);
        control
            .set_read_timeout(Some(PRODUCT_DEADLINE))
            .expect("state control timeout");
        let mut reader = BufReader::new(control);
        let hello = read_control_line(&mut reader);
        let mut hello_fields = hello.split_whitespace();
        assert_eq!(hello_fields.next(), Some("HELLO"), "{hello}");
        let bun_pid = hello_fields
            .next()
            .expect("state Bun PID")
            .parse::<u32>()
            .expect("state numeric Bun PID");
        let _app_link = hello_fields.next().expect("state app link");
        assert!(hello_fields.next().is_none(), "{hello}");
        process.observe_bun(bun_pid);
        assert_eq!(parse_descendant_pid(&read_control_line(&mut reader)), 0);
        assert_eq!(
            read_control_line_or_host_failure(&mut reader, process.child_mut(), "state READY"),
            "READY"
        );
        assert_eq!(
            read_control_line_or_host_failure(&mut reader, process.child_mut(), "state ECHO1"),
            "ECHO1"
        );
        assert_eq!(
            read_control_line_or_host_failure(&mut reader, process.child_mut(), "state ECHO2"),
            "ECHO2"
        );

        Self {
            process,
            reader,
            bun_pid,
            deadline,
            case_name: case.name.to_owned(),
            _stage: stage,
        }
    }

    pub(crate) fn finish(mut self) {
        let host_pid = self.process.host_pid();
        let _window = wait_for_host_window(host_pid, self.deadline);
        self.reader
            .get_mut()
            .write_all(b"QUIT\n")
            .expect("state host Quit");
        self.reader
            .get_mut()
            .flush()
            .expect("flush state host Quit");
        assert_eq!(read_control_line(&mut self.reader), "QUIT_REPLY");
        assert_eq!(read_control_line(&mut self.reader), "LINK_EOF");
        let status = self.process.wait(self.deadline);
        assert!(
            status.success(),
            "{} host exited with {status}",
            self.case_name
        );
        assert!(
            !process_exists(self.bun_pid),
            "{} Bun survived exit",
            self.case_name
        );
    }
}
