//! Real Windows schema-two product boots (GH1188 T2/T3). Readiness, release and
//! managed construction use the engine's derived support. The pre-create native
//! census proves Ready still occurs with zero managed windows.

use crate::dev_stage_command;
use crate::support::product::ProductFixture;
use std::fs;
use std::io::Write as _;
use std::path::Path;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use std::process::Output;
use std::process::Stdio;
use std::time::Instant;

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub(crate) const SCHEMA_TWO_CONFIG: &str = "export default { name: \"KELD Windows facade\", entry: \"src/main.ts\", initialWindow: \"none\" } as const;\n";

pub(crate) fn stage_boot(project: &Path) -> keld_cli::boot::DevBootStage {
    keld_cli::boot::stage_dev_boot(project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
        .expect("stage the Windows boot through the sole producer")
}

/// A refused boot: exited with a failure before any Bun child could report.
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn refused_output(command: &mut std::process::Command) -> Output {
    let mut host = crate::support::signed_process::SignedStateProcessGuard::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch the no-flag Windows host"),
    );
    let stdout = crate::support::process::read_pipe_on_thread(
        host.child_mut()
            .stdout
            .take()
            .expect("captured host stdout"),
    );
    let stderr = crate::support::process::read_pipe_on_thread(
        host.child_mut()
            .stderr
            .take()
            .expect("captured host stderr"),
    );
    let status = host.wait(Instant::now() + crate::PRODUCT_DEADLINE);
    let mut output = Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let drained = Instant::now() + crate::support::process::PIPE_DRAIN_KILL_SWITCH;
    output.stdout = crate::support::process::pipe_text_by(&stdout, drained).into_bytes();
    output.stderr = crate::support::process::pipe_text_by(&stderr, drained).into_bytes();
    output
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn marker_fixture() -> (ProductFixture, std::path::PathBuf) {
    let fixture = ProductFixture::new();
    fs::write(fixture.project.join("keld.config.ts"), SCHEMA_TWO_CONFIG)
        .expect("schema-two config");
    fs::remove_file(fixture.project.join("index.html")).expect("no renderer source");
    fs::write(
        fixture.project.join("src/main.ts"),
        "await Bun.write(process.env.KELD_T1_ENTRY_MARKER!, 'executed');\n",
    )
    .expect("independent entry execution marker");
    let marker = fixture.root.path().join("entry-executed");
    (fixture, marker)
}

/// AC3/AC23 now require product admission. The retired test switch selects
/// nothing: absent, false and true all run the same validated zero-window boot.
#[test]
fn windows_schema_two_product_boots_without_a_test_admission() {
    for retired_value in [None, Some("0"), Some("1")] {
        product_facade_cycle(retired_value);
    }
}

fn product_facade_cycle(retired_value: Option<&str>) {
    use crate::support::control::{accept_ready_generation, read_control_line};
    use crate::support::signed_process::SignedStateProcessGuard;
    let fixture = ProductFixture::schema_two();
    let stage = stage_boot(&fixture.project);
    let descriptor: serde_json::Value =
        serde_json::from_slice(&fs::read(stage.root().join("keld.boot.json")).expect("descriptor"))
            .expect("strict descriptor JSON");
    assert_eq!(descriptor["schema"], 2);
    assert!(descriptor.get("renderer").is_none());
    assert!(!stage.root().join("index.html").exists());
    let control = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("product control");
    let mut command = dev_stage_command(stage.root(), stage.host());
    command
        .env(
            "KELD_T1B_CONTROL",
            control
                .local_addr()
                .expect("control address")
                .port()
                .to_string(),
        )
        .env_remove("KELD_TEST_WINDOWS_ADMIT_FACADE_BOOT")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(value) = retired_value {
        command.env("KELD_TEST_WINDOWS_ADMIT_FACADE_BOOT", value);
    }
    let mut host = SignedStateProcessGuard::new(command.spawn().expect("product facade host"));
    let (mut reader, mut writer, bun, _) = accept_ready_generation(&control, host.child_mut());
    host.observe_bun(bun);
    assert_eq!(
        crate::support::window::app_top_level_windows(host.host_pid()),
        []
    );
    writer.write_all(b"QUIT\n").expect("product Quit");
    writer.flush().expect("flush Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = host.wait(Instant::now() + crate::ui_end_kill_switch());
    assert!(status.success(), "product facade release failed: {status}");
}

#[test]
fn retired_facade_admission_switch_is_absent_from_every_host_image() {
    let image = fs::read(env!("CARGO_BIN_EXE_keld-host")).expect("built host image");
    let name = b"KELD_TEST_WINDOWS_ADMIT_FACADE_BOOT";
    assert!(
        !image.windows(name.len()).any(|window| window == name),
        "the retired environment admission must not exist, including debug-hook Hosts"
    );
}

/// D13: a Bun generation is a launch. The recovery successor of a renderer boot re-runs
/// the admission, so runtime config planted after the first launch stops it before
/// any successor exists, in every build.
#[test]
fn windows_primary_rechecks_the_stage_for_every_generation() {
    let fixture = ProductFixture::new();
    let stage = stage_boot(&fixture.project);
    let control = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind recheck control");
    let port = control.local_addr().expect("control address").port();
    let mut host = crate::support::signed_process::SignedStateProcessGuard::new(
        dev_stage_command(stage.root(), stage.host())
            .env("KELD_T1B_CONTROL", port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch the renderer host"),
    );
    let (_reader, mut writer, bun_pid, _) =
        crate::support::control::accept_ready_generation(&control, host.child_mut());
    host.observe_bun(bun_pid);
    fs::write(stage.root().join("bunfig.toml"), "preload = []\n").expect("planted config");
    writer.write_all(b"CRASH\n").expect("retire the generation");
    writer.flush().expect("flush the retirement");
    let status = crate::wait_child(
        host.child_mut(),
        Instant::now() + crate::ui_end_kill_switch(),
    );
    assert!(!status.success(), "a refused successor became success");
    let output = crate::support::process::exited_child_output(host.child_mut());
    assert!(
        output.contains("staged runtime config bunfig.toml"),
        "{output}"
    );
    crate::support::control::assert_no_control_connection(
        &control,
        "no successor generation may start",
    );
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod admitted {
    use super::{SCHEMA_TWO_CONFIG, refused_output, stage_boot};
    use crate::facade_window::fixture::{ephemeral_profile_dir, retained_descendants};
    use crate::support::control::{
        accept_control_or_host_failure, accept_ready_generation, parse_hello, read_control_line,
        read_control_line_or_host_failure,
    };
    use crate::support::facade_fixture::{
        LATE_PACKAGE_ROLE, junction, stage_capture_order, stage_public_packages,
    };
    use crate::support::process::{
        PipeText, assert_process_signaled, pipe_text_by, process_exists, read_pipe_on_thread,
    };
    use crate::support::product::{ProductFixture, StageFixture};
    use crate::support::profile_run::{ProfileTestRoot, assert_profile_test_root_feature};
    use crate::support::signed_process::SignedStateProcessGuard;
    use crate::support::window::{app_top_level_windows, wait_for_host_window};
    use crate::{PRODUCT_DEADLINE, dev_stage_command, ui_end_kill_switch};
    use serde_json::Value;
    use std::fs;
    use std::io::{BufReader, Read as _, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    fn listener() -> (TcpListener, String) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind facade control");
        let port = listener
            .local_addr()
            .expect("facade control address")
            .port()
            .to_string();
        (listener, port)
    }

    /// The Windows host launched for one product-admitted boot of `stage`. Every inherited
    /// value a cell does not name is removed, so the launcher's own env cannot decide.
    fn product_command(stage: &keld_cli::boot::DevBootStage, port: &str) -> Command {
        let mut command = dev_stage_command(stage.root(), stage.host());
        command
            .env("KELD_T1B_CONTROL", port)
            .env_remove("KELD_NATIVE_WINDOW_FACADE")
            .env_remove("BUN_OPTIONS");
        command
    }

    fn spawn_host(command: &mut Command) -> SignedStateProcessGuard {
        SignedStateProcessGuard::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("launch the no-flag Windows host"),
        )
    }

    fn assert_zero_app_windows(host_pid: u32) {
        let windows = app_top_level_windows(host_pid);
        assert!(windows.is_empty(), "app windows at Ready: {windows:?}");
    }

    fn accept_control(
        listener: &TcpListener,
        host: &mut SignedStateProcessGuard,
    ) -> (BufReader<TcpStream>, TcpStream) {
        let control = accept_control_or_host_failure(
            listener,
            host.child_mut(),
            Instant::now() + PRODUCT_DEADLINE,
        );
        control
            .set_read_timeout(Some(crate::startup_kill_switch()))
            .expect("facade control read deadline");
        let writer = control.try_clone().expect("facade control writer");
        (BufReader::new(control), writer)
    }

    fn command_line(writer: &mut TcpStream, line: &str) {
        writer
            .write_all(format!("{line}\n").as_bytes())
            .expect("facade control command");
        writer.flush().expect("flush facade control command");
    }

    /// Retires the host through its dev lease after the cell's observations and proves
    /// the orderly exit and the Bun child's end.
    fn retire(host: SignedStateProcessGuard) {
        let mut host = host;
        drop(host.child_mut().stdin.take());
        let status = host.wait(Instant::now() + ui_end_kill_switch());
        assert!(status.success(), "lease retirement failed: {status}");
    }

    /// AC1, AC3 and AC4's authenticated raw lifecycle on the real Tao loop: `READY` with
    /// zero app windows, still zero and alive past the deadline a renderer would owe.
    #[test]
    fn windows_schema_two_host_emits_ready_without_an_initial_native_window() {
        let fixture = ProductFixture::schema_two();
        let stage = stage_boot(&fixture.project);
        assert!(!stage.root().join("index.html").exists());
        let descriptor: Value = serde_json::from_slice(
            &fs::read(stage.root().join("keld.boot.json")).expect("staged descriptor"),
        )
        .expect("strict descriptor JSON");
        assert_eq!(descriptor["schema"], 2);
        assert!(descriptor.get("renderer").is_none());

        let (control, port) = listener();
        let mut host = spawn_host(&mut product_command(&stage, &port));
        let host_pid = host.host_pid();
        let (mut reader, mut writer, bun_pid, _) =
            accept_ready_generation(&control, host.child_mut());
        host.observe_bun(bun_pid);
        assert_zero_app_windows(host_pid);

        // A duration oracle, not a synchronization wait: an Init that already happened
        // disarms the entire startup deadline, even with no view.
        thread::sleep(keld_wv::INITIAL_MILESTONE_DEADLINE + Duration::from_secs(1));
        assert!(
            host.child_mut()
                .try_wait()
                .expect("inspect the live windowless host")
                .is_none(),
            "the already-Init windowless host exited at the old startup deadline"
        );
        assert_zero_app_windows(host_pid);

        command_line(&mut writer, "QUIT");
        assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
        assert_eq!(read_control_line(&mut reader), "LINK_EOF");
        let status = host.wait(Instant::now() + ui_end_kill_switch());
        assert!(status.success(), "facade host exited with {status}");
    }

    /// One stalled-Init boot up to its pre-Init records: the guarded host, its output
    /// readers, the `HELLO` Bun handle and the open control stream.
    struct StalledBoot {
        host: SignedStateProcessGuard,
        stdout: PipeText,
        stderr: PipeText,
        reader: BufReader<TcpStream>,
        bun_pid: u32,
        started: Instant,
    }

    fn stalled_boot(stage: &keld_cli::boot::DevBootStage) -> StalledBoot {
        let (control, port) = listener();
        // The child can arm its watch before the parent resumes after spawn.
        let started = Instant::now();
        let mut host =
            spawn_host(product_command(stage, &port).env("KELD_T1B_STALL_BEFORE_INIT", "1"));
        let stdout = read_pipe_on_thread(host.child_mut().stdout.take().expect("captured stdout"));
        let stderr = read_pipe_on_thread(host.child_mut().stderr.take().expect("captured stderr"));
        let stream = accept_control_or_host_failure(
            &control,
            host.child_mut(),
            Instant::now() + PRODUCT_DEADLINE,
        );
        // Bounds the whole pre-Init read: the watch, then the owner's reply bound.
        stream
            .set_read_timeout(Some(keld_wv::INITIAL_MILESTONE_DEADLINE + PRODUCT_DEADLINE))
            .expect("pre-Init control deadline");
        let mut reader = BufReader::new(stream);
        let (bun_pid, _) = parse_hello(&read_control_line(&mut reader));
        host.observe_bun(bun_pid);
        assert_eq!(read_control_line(&mut reader), "DESCENDANT 0");
        StalledBoot {
            host,
            stdout,
            stderr,
            reader,
            bun_pid,
            started,
        }
    }

    /// The genuine Init is absent, so no readiness token may ever arrive; the terminal
    /// then ends the process once, after its 15 s bound, and retires Bun.
    fn assert_terminal_once(boot: StalledBoot, stage: &keld_cli::boot::DevBootStage) {
        let StalledBoot {
            host,
            stdout,
            stderr,
            mut reader,
            bun_pid,
            started,
        } = boot;
        // Bun's retirement closes this stream; its TCP peer may close abortively.
        let mut remaining = String::new();
        match reader.read_to_string(&mut remaining) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
            Err(error) => panic!("pre-Init control read: {error}"),
        }
        assert!(
            !remaining.lines().any(|line| line == "READY"),
            "{remaining}"
        );
        let status = host.wait(started + keld_wv::INITIAL_MILESTONE_DEADLINE + PRODUCT_DEADLINE);
        assert!(!status.success(), "stalled Init exited successfully");
        assert!(
            started.elapsed() >= keld_wv::INITIAL_MILESTONE_DEADLINE,
            "the terminal ran before its 15 s bound: {:?}",
            started.elapsed()
        );
        assert!(!process_exists(bun_pid), "Bun survived the terminal");
        let drained = Instant::now() + crate::support::process::PIPE_DRAIN_KILL_SWITCH;
        let stderr = pipe_text_by(&stderr, drained);
        let _ = pipe_text_by(&stdout, drained);
        assert_eq!(
            stderr.matches("KELD_T1B_TERMINAL_ENTERED").count(),
            1,
            "{stderr}"
        );
        assert_eq!(
            stderr
                .matches("the UI loop did not deliver its first turn within 15 s")
                .count(),
            1,
            "{stderr}"
        );
        assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
        assert!(
            stage.root().exists(),
            "the Windows host never deletes its own stage (AC19)"
        );
    }

    /// AC2: a real child parks its UI thread after loop setup, before Tao can deliver
    /// Init. The coordinator alone must end the process through the host's terminal.
    #[test]
    fn windows_schema_two_stalled_init_exits_through_the_binary_terminal_once() {
        let fixture = ProductFixture::schema_two();
        let stage = stage_boot(&fixture.project);
        assert_terminal_once(stalled_boot(&stage), &stage);
    }

    /// AC3 (facade `Subscribe`), AC4, AC5 NC 1a's direct-entry shape and AC8. No sidecar,
    /// so no preload: the first thing this entry does is delete the live environment,
    /// and only the launch define can still select the facade barrier.
    #[test]
    #[allow(clippy::too_many_lines)] // Keep zero-Ready, transport and native cleanup in the same staged boot.
    fn windows_direct_entry_boot_mode_survives_environment_deletion() {
        for inherited in [None, Some("0"), Some("1"), Some("unknown-caller-value")] {
            let fixture = ProductFixture::new();
            fs::write(fixture.project.join("src/main.ts"), LATE_PACKAGE_ROLE)
                .expect("ordinary unbundled entry");
            fs::write(fixture.project.join("keld.config.ts"), SCHEMA_TWO_CONFIG)
                .expect("schema-two config");
            fs::remove_file(fixture.project.join("index.html")).expect("no renderer source");
            let stage = stage_boot(&fixture.project);
            stage_public_packages(&stage);
            let profiles = ProfileTestRoot::new();
            assert_profile_test_root_feature(stage.host(), profiles.path());
            let (control, port) = listener();
            let mut command = product_command(&stage, &port);
            command
                .env("KELD_PROFILE_TEST_ROOT", profiles.path())
                .env("TEMP", profiles.path().parent().expect("owned temp parent"))
                .env("TMP", profiles.path().parent().expect("owned temp parent"));
            if let Some(value) = inherited {
                command.env("KELD_NATIVE_WINDOW_FACADE", value);
            }
            let mut host = spawn_host(&mut command);
            let host_pid = host.host_pid();
            let (mut reader, mut writer) = accept_control(&control, &mut host);
            let mode = read_control_line_or_host_failure(&mut reader, host.child_mut(), "MODE");
            let mode: Value = serde_json::from_str(
                mode.strip_prefix("MODE ")
                    .unwrap_or_else(|| panic!("launch-mode record: {mode}")),
            )
            .expect("launch-mode JSON");
            host.observe_bun(u32::try_from(mode["pid"].as_u64().expect("Bun pid")).expect("pid"));
            assert_eq!(mode["hint"], "1", "inherited={inherited:?}");
            assert_eq!(mode["bunOptions"], Value::Null, "BUN_OPTIONS is removed");
            let mut next = |label: &str, host: &mut SignedStateProcessGuard| {
                read_control_line_or_host_failure(&mut reader, host.child_mut(), label)
            };
            assert_eq!(next("SELECTED", &mut host), "SELECTED true");
            assert_eq!(
                next("PRE_READY", &mut host),
                "PRE_READY KELD-CORE-040 frames=0 windows=0",
                "AC8: refused before Ready with no channel-5 call"
            );
            assert_zero_app_windows(host_pid);
            assert_eq!(next("PREQUIT", &mut host), "PREQUIT KELD-COMPAT-012");
            assert_eq!(next("READY", &mut host), "READY");
            // The fixture waits for this command: zero Ready is independently
            // observable before the supported constructor can create an HWND.
            assert_zero_app_windows(host_pid);
            command_line(&mut writer, "CREATE");
            assert_eq!(
                next("POST_READY", &mut host),
                "POST_READY returned frames=1 windows=1",
                "D1/AC6: the same product admission now reaches the managed native builder"
            );
            let windows = crate::support::window::app_top_level_windows(host_pid);
            assert_eq!(windows.len(), 1, "public constructor has no native HWND");
            let native = crate::support::window::NativeWindowSnapshot::capture(
                u64::try_from(windows[0].hwnd).expect("observed HWND"),
                host_pid,
            )
            .expect("native facade/controller observation");
            assert!(
                !native.children.is_empty(),
                "public facade has no controller hosting windows"
            );
            let descendants = retained_descendants(host_pid);
            assert!(
                !descendants.is_empty(),
                "late-package process census is empty"
            );
            let profile = ephemeral_profile_dir(profiles.path());
            command_line(&mut writer, "DESTROY");
            assert_eq!(
                next("POST_DESTROY", &mut host),
                "POST_DESTROY windows=0 closed=1 contents=1"
            );
            assert!(
                native.live_handles().is_empty(),
                "Destroy answered before native retirement"
            );
            assert_zero_app_windows(host_pid);
            command_line(&mut writer, "QUIT");
            assert_eq!(next("QUIT_ACCEPTED", &mut host), "QUIT_ACCEPTED");
            let status = host.wait(Instant::now() + ui_end_kill_switch());
            assert!(status.success(), "facade host exited with {status}");
            for (pid, process) in descendants {
                assert_process_signaled(&process, &format!("late-package descendant {pid}"));
            }
            assert_zero_app_windows(host_pid);
            assert!(
                !profile.exists(),
                "late-package managed profile survived native retirement"
            );
            profiles.remove();
        }
    }

    /// AC5 NC 1b and AC20: `--preload` for every boot kind whose stage has the sidecar,
    /// so the transport captures its intrinsics before the app's first module.
    #[test]
    #[allow(clippy::too_many_lines)] // Compare transport capture on both boots with their explicit cleanup.
    fn windows_primary_preloads_the_staged_transport_before_the_first_module() {
        for facade in [true, false] {
            let fixture = StageFixture::legacy_api_ready();
            let stage = stage_capture_order(&fixture, facade);
            let profiles = ProfileTestRoot::new();
            assert_profile_test_root_feature(stage.host(), profiles.path());
            let (control, port) = listener();
            let mut command = product_command(&stage, &port);
            command
                .env("KELD_NATIVE_WINDOW_FACADE", "1")
                .env("KELD_PROFILE_TEST_ROOT", profiles.path())
                .env("TEMP", profiles.path().parent().expect("owned temp parent"))
                .env("TMP", profiles.path().parent().expect("owned temp parent"));
            let mut host = spawn_host(&mut command);
            let host_pid = host.host_pid();
            let (mut reader, mut writer) = accept_control(&control, &mut host);
            let bound = read_control_line_or_host_failure(&mut reader, host.child_mut(), "BOUND");
            let bun_pid = bound
                .strip_prefix("BOUND ")
                .unwrap_or_else(|| panic!("capture-order role: {bound}"))
                .parse::<u32>()
                .expect("numeric Bun pid");
            host.observe_bun(bun_pid);
            let mode = read_control_line_or_host_failure(&mut reader, host.child_mut(), "MODE");
            assert_eq!(
                mode,
                if facade {
                    "MODE 1 true"
                } else {
                    "MODE 0 false"
                },
                "AC20: the renderer child observes \"0\" under an inherited \"1\""
            );
            assert_eq!(
                read_control_line_or_host_failure(&mut reader, host.child_mut(), "READY"),
                "READY",
            );
            if facade {
                assert_zero_app_windows(host.host_pid());
            }
            command_line(&mut writer, "CALL");
            let capture =
                read_control_line_or_host_failure(&mut reader, host.child_mut(), "CAPTURE");
            assert_eq!(capture, "CAPTURE 0", "facade={facade}");
            let descendants = if facade {
                assert_eq!(
                    read_control_line_or_host_failure(
                        &mut reader,
                        host.child_mut(),
                        "PUBLIC_CREATED"
                    ),
                    "PUBLIC_CREATED 1",
                );
                let windows = crate::support::window::app_top_level_windows(host.host_pid());
                assert_eq!(
                    windows.len(),
                    1,
                    "capture-order constructor has no native HWND"
                );
                let native = crate::support::window::NativeWindowSnapshot::capture(
                    u64::try_from(windows[0].hwnd).expect("observed HWND"),
                    host.host_pid(),
                )
                .expect("capture-order controller observation");
                assert_ne!(native.children, Vec::<usize>::new());
                let descendants = retained_descendants(host_pid);
                command_line(&mut writer, "DESTROY");
                assert_eq!(
                    read_control_line_or_host_failure(
                        &mut reader,
                        host.child_mut(),
                        "PUBLIC_DESTROYED"
                    ),
                    "PUBLIC_DESTROYED closed=1 contents=1",
                );
                assert!(
                    native.live_handles().is_empty(),
                    "capture-order native resources survived Destroy"
                );
                assert_zero_app_windows(host.host_pid());
                descendants
            } else {
                let window =
                    wait_for_host_window(host.host_pid(), Instant::now() + PRODUCT_DEADLINE);
                assert_ne!(window["handle"].as_u64(), Some(0), "{window}");
                retained_descendants(host_pid)
            };
            assert!(
                !descendants.is_empty(),
                "capture-order process census is empty"
            );
            let profile = ephemeral_profile_dir(profiles.path());
            retire(host);
            for (pid, process) in descendants {
                assert_process_signaled(&process, &format!("capture-order descendant {pid}"));
            }
            assert_zero_app_windows(host_pid);
            assert!(
                !profile.exists(),
                "capture-order profile survived native retirement"
            );
            profiles.remove();
        }
    }

    fn entry_marker_fixture(sidecar: bool, facade: bool) -> (ProductFixture, std::path::PathBuf) {
        let (fixture, marker) = super::marker_fixture();
        if !facade {
            fs::write(
                fixture.project.join("keld.config.ts"),
                "export default { name: \"KELD Windows renderer\", entry: \"src/main.ts\", renderer: \"index.html\" } as const;\n",
            )
            .expect("renderer config");
            fs::write(fixture.project.join("index.html"), "<!doctype html>\n")
                .expect("renderer source");
        }
        if sidecar {
            fs::write(
                fixture.project.join("src/kipc-transport.ts"),
                "export {};\n",
            )
            .expect("sidecar source");
        }
        (fixture, marker)
    }

    fn assert_launch_refused(
        stage: &keld_cli::boot::DevBootStage,
        marker: &Path,
        expected: &str,
        label: &str,
    ) {
        let (_control, port) = listener();
        let output =
            refused_output(product_command(stage, &port).env("KELD_T1_ENTRY_MARKER", marker));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{label} launched: {output:?}");
        assert!(stderr.contains("KELD-CORE-037"), "{label}: {stderr}");
        assert!(stderr.contains(expected), "{label}: {stderr}");
        assert!(!marker.exists(), "{label}: Bun ran the entry");
    }

    /// AC5 NC 2: Bun runtime config overrides the define and the preload, so every boot
    /// kind refuses it on every launch, sidecar or direct entry, before Bun starts.
    #[test]
    fn windows_primary_refuses_bunfig_on_every_launch() {
        for config in ["bunfig.toml", ".bunfig.toml"] {
            for (sidecar, facade) in [(false, true), (true, true), (false, false), (true, false)] {
                let (fixture, marker) = entry_marker_fixture(sidecar, facade);
                let stage = stage_boot(&fixture.project);
                fs::write(
                    stage.root().join(config),
                    "[define]\n\"process.env.KELD_NATIVE_WINDOW_FACADE\" = '\"0\"'\n",
                )
                .expect("hostile staged runtime config");
                assert_launch_refused(
                    &stage,
                    &marker,
                    &format!("staged runtime config {config}"),
                    &format!("{config} sidecar={sidecar} facade={facade}"),
                );
            }
        }
    }

    /// A staged sidecar replaced by a fixture that points outside the stage.
    fn linked_sidecar_fixture(
        facade: bool,
    ) -> (
        ProductFixture,
        std::path::PathBuf,
        keld_cli::boot::DevBootStage,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let (fixture, marker) = entry_marker_fixture(true, facade);
        let stage = stage_boot(&fixture.project);
        let sidecar = stage.root().join("src").join("kipc-transport.ts");
        let outside = fixture.root.path().join("outside");
        fs::create_dir(&outside).expect("external directory");
        fs::write(outside.join("outside.ts"), "export {};\n").expect("external file");
        fs::remove_file(&sidecar).expect("remove the staged sidecar");
        (fixture, marker, stage, sidecar, outside)
    }

    /// AC5 NC 3: a junction or non-file sidecar is refused by the admission owner before
    /// Bun starts, so the oracle never depends on what Bun does with such a preload. A
    /// junction needs no privilege, so this cell runs on every Windows session.
    #[test]
    fn windows_primary_refuses_junction_and_directory_sidecars() {
        for facade in [true, false] {
            for shape in ["junction", "directory"] {
                let (_fixture, marker, stage, sidecar, outside) = linked_sidecar_fixture(facade);
                let expected = if shape == "junction" {
                    junction(&sidecar, &outside);
                    "reparse point"
                } else {
                    fs::create_dir(&sidecar).expect("directory sidecar");
                    "not a regular file"
                };
                assert_launch_refused(
                    &stage,
                    &marker,
                    expected,
                    &format!("{shape} facade={facade}"),
                );
                fs::remove_dir(&sidecar).expect("remove the sidecar fixture, not its target");
            }
        }
    }

    /// AC5 NC 3, symbolic-link row. Creating a Windows symbolic link needs Developer Mode
    /// or `SeCreateSymbolicLinkPrivilege` (hosted `windows-latest` has it). Without it this
    /// cell fails on that prerequisite instead of passing unexercised, and the junction
    /// and directory rows above still run.
    #[test]
    fn windows_primary_refuses_a_symbolic_link_sidecar() {
        for facade in [true, false] {
            let (_fixture, marker, stage, sidecar, outside) = linked_sidecar_fixture(facade);
            std::os::windows::fs::symlink_file(outside.join("outside.ts"), &sidecar).expect(
                "create a Windows symbolic link: needs Developer Mode or \
                 SeCreateSymbolicLinkPrivilege (hosted windows-latest has it)",
            );
            assert_launch_refused(
                &stage,
                &marker,
                "reparse point",
                &format!("symlink facade={facade}"),
            );
        }
    }

    /// AC5 NC 4: an inherited `BUN_OPTIONS` preload never runs. The test's own direct Bun
    /// launch is the at-risk proof: the same value does run on this Bun.
    #[test]
    fn windows_primary_removes_an_inherited_bun_options_preload() {
        let fixture = ProductFixture::schema_two();
        let attacker = fixture.root.path().join("attacker.ts");
        let marker = fixture.root.path().join("attacker-ran");
        fs::write(
            &attacker,
            "require(\"node:fs\").writeFileSync(process.env.KELD_T1_ATTACKER_MARKER!, \"ran\");\n",
        )
        .expect("attacker preload");
        let preload = format!(
            "--preload={}",
            attacker.to_string_lossy().replace('\\', "/")
        );
        let direct = Command::new("bun")
            .args(["-e", "0"])
            .env("BUN_OPTIONS", &preload)
            .env("KELD_T1_ATTACKER_MARKER", &marker)
            .output()
            .expect("direct Bun control");
        assert!(direct.status.success(), "direct control: {direct:?}");
        assert!(marker.exists(), "BUN_OPTIONS did not preload on this Bun");
        fs::remove_file(&marker).expect("reset the attacker marker");

        let stage = stage_boot(&fixture.project);
        let (control, port) = listener();
        let mut command = product_command(&stage, &port);
        command
            .env("BUN_OPTIONS", &preload)
            .env("KELD_T1_ATTACKER_MARKER", &marker);
        let mut host = spawn_host(&mut command);
        let (_reader, _writer, bun_pid, _) = accept_ready_generation(&control, host.child_mut());
        host.observe_bun(bun_pid);
        assert!(!marker.exists(), "an inherited preload ran before the app");
        retire(host);
    }

    /// AC3 negative control: product admission changes no descriptor rule.
    #[test]
    fn windows_schema_two_descriptor_with_a_renderer_is_refused_under_the_admission() {
        let (fixture, marker) = super::marker_fixture();
        let stage = stage_boot(&fixture.project);
        let path = stage.root().join("keld.boot.json");
        let mut descriptor: Value =
            serde_json::from_slice(&fs::read(&path).expect("staged descriptor"))
                .expect("strict descriptor JSON");
        descriptor["renderer"] = Value::from("index.html");
        fs::write(stage.root().join("index.html"), "<!doctype html>\n").expect("renderer file");
        let mut permissions = fs::metadata(&path)
            .expect("descriptor metadata")
            .permissions();
        #[allow(clippy::permissions_set_readonly_false)] // a test-owned stage file on Windows
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).expect("writable test descriptor");
        fs::write(
            &path,
            serde_json::to_vec(&descriptor).expect("descriptor JSON"),
        )
        .expect("tampered descriptor");
        let (_control, port) = listener();
        let output =
            refused_output(product_command(&stage, &port).env("KELD_T1_ENTRY_MARKER", &marker));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{output:?}");
        assert!(stderr.contains("KELD-CORE-035"), "{stderr}");
        assert!(!marker.exists(), "the entry ran under a refused descriptor");
    }
}
