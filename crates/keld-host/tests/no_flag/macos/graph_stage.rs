//! Shipping schema-five graph and Guardian G1/G2 with actual packed SDK bytes.
//! This deliberately inert role is not an original draw.io lifecycle claim.
use crate::support::EVENT_DEADLINE;
use crate::support::control::{
    parse_pid, read_control_line, try_accept_before_observing, wait_child_output,
};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::process::{await_process_gone, parent_process, process_group};
use keld_core::app_session::graph_contract::ApprovedGraphCaptureInput;
use std::fs;
use std::io::{BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn prepare(root: &Path) -> (PathBuf, ApprovedGraphCaptureInput) {
    let role = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/graph_stage_role.mjs");
    prepare_with_role(root, &role)
}

/// Reuses the canonical packed-provider fixture for a different authored role.
/// This helper never selects or evaluates the original application.
pub(super) fn prepare_with_role(root: &Path, role: &Path) -> (PathBuf, ApprovedGraphCaptureInput) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let process = Command::new("bun")
        .arg(fixtures.join("graph_stage_prepare.ts"))
        .arg(root)
        .arg(role)
        .arg(fixtures.join("graph_stage_early.mjs"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("canonical pack and owned offline install");
    let output = wait_child_output(process, Duration::from_secs(120));
    assert!(output.status.success(), "packed fixture failed: {output:?}");
    let expected: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("graph-expectations.json")).expect("independent archive expectations"),
    )
    .expect("archive-derived graph input");
    let approval = ApprovedGraphCaptureInput {
        expected_package_sha256: serde_json::from_value(
            expected["expected_package_sha256"].clone(),
        )
        .expect("root digest"),
        expected_lock_sha256: serde_json::from_value(expected["expected_lock_sha256"].clone())
            .expect("lock digest"),
        expected_package_trees: serde_json::from_value(expected["expected_package_trees"].clone())
            .expect("three actual archive trees"),
    };
    (root.join("consumer"), approval)
}

fn generation(
    listener: &UnixListener,
    host: u32,
    cleanup: &mut ShippingLaunchCleanup,
) -> (BufReader<UnixStream>, UnixStream, u32, u32) {
    let control =
        match try_accept_before_observing(listener, Instant::now() + EVENT_DEADLINE, || {}) {
            Ok(control) => control,
            Err(error) => {
                let output = wait_child_output(
                    cleanup.cli.take().expect("Host diagnostic owner"),
                    EVENT_DEADLINE,
                );
                panic!("graph generation failed before ready: {error:?}; {output:?}");
            }
        };
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("bounded graph control");
    let mut reader = BufReader::new(control.try_clone().expect("control reader"));
    let hello = read_control_line(&mut reader);
    let mut fields = hello.split_whitespace();
    assert_eq!(fields.next(), Some("READY"), "{hello}");
    let bun = parse_pid(fields.next(), &hello);
    let guardian = parse_pid(fields.next(), &hello);
    assert!(
        fields
            .next()
            .expect("desktop count")
            .parse::<u32>()
            .expect("count")
            > 0
    );
    assert_eq!(parent_process(bun), guardian);
    assert_eq!(parent_process(guardian), host);
    cleanup.record_authenticated_groups(process_group(host), process_group(bun));
    (reader, control, bun, guardian)
}

#[test]
fn gh499_shipping_graph_rechecks_g2_and_preloads_the_one_installed_transport() {
    let root = tempfile::tempdir().expect("owned graph fixture");
    let (project, approved) = prepare(root.path());
    let stage = keld_cli::boot::stage_dev_boot_with_approved_graph(
        &project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
        &approved,
        None,
    )
    .expect("actual packed graph source admission");
    let control_path = root.path().join("g.sock");
    let listener = UnixListener::bind(&control_path).expect("owned control");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(stage.host())
        .process_group(0)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T1B_CONTROL", &control_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping graph Host");
    let host = child.id();
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let lease = cleanup
        .cli
        .as_mut()
        .expect("Host")
        .stdin
        .take()
        .expect("live lease");
    let (mut first, mut commands, old_bun, guardian) = generation(&listener, host, &mut cleanup);
    commands
        .write_all(b"CHECK\n")
        .expect("early intrinsic check");
    assert_eq!(
        read_control_line(&mut first),
        "CHECK 0 2",
        "installed KIPC must precede application dependencies"
    );
    commands
        .write_all(b"CRASH\n")
        .expect("one recoverable real Bun exit");
    let (mut second, mut commands, new_bun, same_guardian) =
        generation(&listener, host, &mut cleanup);
    assert_ne!(old_bun, new_bun);
    assert_eq!(guardian, same_guardian);
    await_process_gone(old_bun);
    commands
        .write_all(b"CHECK\n")
        .expect("G2 canonical provider check");
    assert_eq!(read_control_line(&mut second), "CHECK 0 2");
    commands.write_all(b"QUIT\n").expect("normal Host Quit");
    assert_eq!(read_control_line(&mut second), "QUIT_REPLY");
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    drop(lease);
    assert!(output.status.success(), "graph Host exit: {output:?}");
    await_process_gone(new_bun);
    assert!(!stage.root().exists(), "proved graph stage cleanup");
}

#[test]
fn gh499_shipping_guardian_refuses_changed_asset_before_successor_entry() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().expect("owned graph mutation fixture");
    let (project, approved) = prepare(root.path());
    let stage = keld_cli::boot::stage_dev_boot_with_approved_graph(
        &project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
        &approved,
        None,
    )
    .expect("approved real packed providers");
    let control_path = root.path().join("g.sock");
    let listener = UnixListener::bind(&control_path).expect("owned control");
    listener.set_nonblocking(true).expect("bounded accept");
    let child = Command::new(stage.host())
        .process_group(0)
        .env("KELD_DEV_LEASE", "stdin-v1")
        .env("KELD_T1B_CONTROL", &control_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shipping graph Host");
    let host = child.id();
    let mut cleanup = ShippingLaunchCleanup::new(child);
    let lease = cleanup
        .cli
        .as_mut()
        .expect("Host")
        .stdin
        .take()
        .expect("live lease");
    let (mut first, mut commands, old_bun, guardian) = generation(&listener, host, &mut cleanup);
    commands
        .write_all(b"CHECK\n")
        .expect("valid first generation control");
    assert_eq!(read_control_line(&mut first), "CHECK 0 2");
    let changed = stage.root().join("drawio/src/main/webapp/index.html");
    fs::set_permissions(&changed, fs::Permissions::from_mode(0o600))
        .expect("owned single-fault test mutation");
    fs::write(&changed, b"changed-after-G1\n").expect("alter only one captured asset");
    commands
        .write_all(b"CRASH\n")
        .expect("request one successor attempt");
    let output = wait_child_output(cleanup.release(), EVENT_DEADLINE);
    drop(lease);
    assert!(
        !output.status.success(),
        "mutated graph must not be accepted: {output:?}"
    );
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("graph admission") && errors.contains("file size"),
        "{output:?}"
    );
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "successor application entry must never connect after graph refusal"
    );
    await_process_gone(old_bun);
    await_process_gone(guardian);
}
