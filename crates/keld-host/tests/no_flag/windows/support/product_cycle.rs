//! Shared existing ordered product-cycle observer and returned evidence.

use std::fs;
use std::io::{BufReader, Read as _, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::Instant;

use windows_sys::Win32::Foundation::WAIT_OBJECT_0;

use super::control::{accept_control_or_host_failure, parse_descendant_pid, read_control_line};
use super::process::{open_process_for_wait, wait_for_process_signal};
use super::product::ProductFixture;
use super::renderer::{spawn_renderer_beacon, try_expect_renderer_beacon};
use super::signed_process::SignedStateProcessGuard;
use super::window::NativeWindowSnapshot;
use crate::{
    DARK_BG, PRODUCT_DEADLINE, PRODUCT_TITLE, dev_stage_command, process_exists,
    startup_kill_switch, ui_end_kill_switch, wait_child, wait_for_host_window,
};

pub(crate) struct ProductEvidence {
    pub(crate) host_pid: u32,
    pub(crate) bun_pid: u32,
    pub(crate) app_link: String,
}

pub(crate) fn run_product_cycle(fixture: &ProductFixture, label: &str) -> ProductEvidence {
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind control listener");
    let control_port = control_listener
        .local_addr()
        .expect("control address")
        .port();
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind beacon listener");
    let beacon_port = beacon_listener.local_addr().expect("beacon address").port();
    let beacon = spawn_renderer_beacon(beacon_listener);
    let stage = stage_cycle_renderer(fixture, label, beacon_port);
    let child = dev_stage_command(stage.root(), stage.host())
        .env("KELD_T1B_CONTROL", control_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch staged no-flag Windows host");
    let mut child = SignedStateProcessGuard::new(child);
    let host_pid = child.host_pid();
    let control = accept_control_or_host_failure(
        &control_listener,
        child.child_mut(),
        Instant::now() + PRODUCT_DEADLINE,
    );
    control
        .set_read_timeout(Some(PRODUCT_DEADLINE))
        .expect("control read deadline");
    let mut writer = control.try_clone().expect("control writer clone");
    let mut reader = BufReader::new(control);

    let hello = read_control_line(&mut reader);
    let (bun_pid, app_link) = parse_cycle_hello(&hello);
    // Bun is live while it sends HELLO, so this handle pins its exact kernel process
    // object; the bare PID names it only until it terminates.
    let bun = open_process_for_wait(bun_pid, false);
    child.observe_bun(bun_pid);
    let descendant_record = read_control_line(&mut reader);
    assert_eq!(parse_descendant_pid(&descendant_record), 0);

    // The beacon is the initial navigation's own request, so this wait spans the
    // host's WebView2 startup (READY has not been read yet).
    if let Err(beacon_error) = try_expect_renderer_beacon(
        beacon,
        startup_kill_switch(),
        "WebView2 renderer requested the exact beacon",
    ) {
        fail_renderer_beacon_cycle(child.child_mut(), host_pid, bun_pid, &beacon_error);
    }
    assert_eq!(read_control_line(&mut reader), "READY");
    assert_eq!(read_control_line(&mut reader), "ECHO1");
    assert_eq!(read_control_line(&mut reader), "ECHO2");
    let window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
    assert_ne!(window["handle"].as_u64(), Some(0), "{window}");
    assert_eq!(window["title"], PRODUCT_TITLE, "{window}");

    let native = NativeWindowSnapshot::capture(
        window["handle"].as_u64().expect("observed nonzero HWND"),
        host_pid,
    )
    .expect("capture the original Host's real native window tree");
    println!("KELD_NATIVE_WINDOW_BEFORE label={label} host={host_pid} snapshot={native:?}");
    assert!(
        !native.children.is_empty(),
        "renderer observation needs real child HWNDs: {native:?}"
    );
    assert!(
        native.outer_rect[2] > native.outer_rect[0] && native.outer_rect[3] > native.outer_rect[1],
        "visible renderer needs positive native outer dimensions: {native:?}"
    );
    assert_eq!(
        native.live_handles().len(),
        native.children.len() + 1,
        "every captured native handle must be live before Quit"
    );

    writer.write_all(b"QUIT\n").expect("request lifecycle Quit");
    writer.flush().expect("flush lifecycle Quit");
    assert_eq!(read_control_line(&mut reader), "QUIT_REPLY");
    assert_eq!(read_control_line(&mut reader), "LINK_EOF");
    let status = wait_child(child.child_mut(), Instant::now() + ui_end_kill_switch());
    assert!(status.success(), "host exited with {status}");
    // Quit kill-waits Bun before the UI loop exits (architecture 02, Quit), so the
    // exact Bun process is already signaled when the host exit is observed. The zero
    // timeout is that contract; any grace would also accept the host Job's
    // kill-on-close reaping Bun only after host death.
    assert_eq!(
        wait_for_process_signal(&bun, 0),
        WAIT_OBJECT_0,
        "Bun {bun_pid} survived orderly host exit"
    );
    // Query before any emergency guard cleanup. The retained Host Child is still
    // owned here. Live/recycled HWND numbers fail conservatively, not as authority.
    assert!(
        native.live_handles().is_empty(),
        "native HWND survived orderly Quit: {native:?}"
    );
    println!(
        "KELD_NATIVE_WINDOW_AFTER label={label} host={host_pid} captured_children={} live=0",
        native.children.len()
    );

    // The fixture's frame loop reports any frame it does not own, a channel-5 frame
    // included, as an `ERROR` record: a renderer boot's stream ends here with none.
    let mut trailing = String::new();
    match reader.read_to_string(&mut trailing) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        Err(error) => panic!("control stream after Bun exit: {error}"),
    }
    assert!(
        trailing.is_empty(),
        "a renderer boot wrote a frame after Quit: {trailing}"
    );

    ProductEvidence {
        host_pid,
        bun_pid,
        app_link,
    }
}

fn stage_cycle_renderer(
    fixture: &ProductFixture,
    label: &str,
    beacon_port: u16,
) -> keld_cli::boot::DevBootStage {
    fs::write(
        fixture.project.join("index.html"),
        format!(
            "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><p id=exact>{label}</p><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
        ),
    )
    .expect("write renderer");
    keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
        .expect("stage Windows product host")
}

fn parse_cycle_hello(hello: &str) -> (u32, String) {
    let mut hello_fields = hello.split_whitespace();
    assert_eq!(hello_fields.next(), Some("HELLO"), "{hello}");
    let bun_pid = hello_fields
        .next()
        .expect("HELLO pid")
        .parse::<u32>()
        .expect("numeric Bun pid");
    let app_link = hello_fields.next().expect("HELLO app link").to_owned();
    assert!(hello_fields.next().is_none(), "{hello}");
    (bun_pid, app_link)
}

fn fail_renderer_beacon_cycle(
    child: &mut Child,
    host_pid: u32,
    bun_pid: u32,
    beacon_error: &str,
) -> ! {
    let host_status_before_cleanup = child
        .try_wait()
        .expect("observe host after beacon deadline");
    let host_was_live = host_status_before_cleanup.is_none();
    if host_was_live
        && let Err(kill_error) = child.kill()
        && child
            .try_wait()
            .expect("recheck host after failed emergency kill")
            .is_none()
    {
        panic!(
            "renderer beacon failed ({beacon_error}); host {host_pid} remained live after cleanup request: {kill_error}"
        );
    }
    let cleanup_status = wait_child(child, Instant::now() + PRODUCT_DEADLINE);
    let bun_alive_after_cleanup = process_exists(bun_pid);
    let (stdout, stderr) = if bun_alive_after_cleanup {
        (
            String::from("not drained: Bun still owns the inherited output path"),
            String::from("not drained: Bun still owns the inherited output path"),
        )
    } else {
        let mut stdout = String::new();
        let mut stderr = String::new();
        child
            .stdout
            .take()
            .expect("captured host stdout")
            .read_to_string(&mut stdout)
            .expect("read failed-cycle host stdout");
        child
            .stderr
            .take()
            .expect("captured host stderr")
            .read_to_string(&mut stderr)
            .expect("read failed-cycle host stderr");
        (stdout, stderr)
    };
    panic!(
        "renderer beacon failed ({beacon_error}); host_alive_before_cleanup={host_was_live}; host_status_before_cleanup={host_status_before_cleanup:?}; host_status_after_cleanup={cleanup_status}; bun_alive_after_cleanup={bun_alive_after_cleanup}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
