//! A real macOS process tree for the CLI's pre-Ready emergency owner.
use super::*;
use crate::support::process::{await_process_gone, process_exists, process_group};

#[test]
fn pre_host_phase_failure_reaps_its_suspended_cli_without_starting_a_host() {
    use crate::support::process::await_process_state;

    let owned = tempfile::tempdir().expect("pre-Host abort fixture");
    let project = fs::canonicalize(owned.path()).expect("canonical fixture root");
    // A stopped shell is a deterministic gap before exec of the real CLI. A
    // failure here has no admitted stage or process-group identity to trust.
    let child = Command::new("/bin/sh")
        .args(["-c", "kill -STOP $$; exec \"$1\" dev", "held-cli"])
        .arg(env!("CARGO_BIN_EXE_keld-host"))
        .process_group(0)
        .spawn()
        .expect("launch held CLI candidate");
    let cli_pid = child.id();
    let mut custody = AuthCliCustody::new(child, &project);
    await_process_state(cli_pid, 'T');
    assert!(custody.capture_os_groups().is_none());
    assert!(custody.launch.host_group.is_none());
    let observation = Observation::bind();
    assert!(
        await_phases_until(
            &observation,
            &project,
            &CASES[0],
            &mut custody,
            Instant::now()
        )
        .is_err(),
        "missing pre-host phases must fail"
    );
    drop(custody);
    await_process_gone(cli_pid);
    assert!(
        fs::symlink_metadata(project.join(".keld")).is_err(),
        "an aborted pre-Host CLI created a staged app"
    );
    drop(observation);
}

#[test]
fn early_phase_timeout_reaps_only_the_owned_host_and_bun_groups() {
    let owned = tempfile::tempdir().expect("isolated process-family fixture");
    let project = fs::canonicalize(owned.path()).expect("canonical fixture root");
    let stage = project.join(".keld/dev/owned-stage");
    fs::create_dir_all(&stage).expect("owned stage directory");
    let source = project.join("lineage.c");
    fs::write(
        &source,
        r#"#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
static void spawn(const char *image, const char *role, const char *host, const char *bun, int new_group) {
    pid_t pid = fork();
    if (pid < 0) _exit(111);
    if (pid == 0) {
        if (new_group && setpgid(0, 0) != 0) _exit(112);
        execl(image, image, role, host, bun, (char *)0);
        _exit(113);
    }
}
int main(int argc, char **argv) {
    if (argc != 4) return 114;
    if (strcmp(argv[1], "cli") == 0) spawn(argv[2], "host", argv[2], argv[3], 1);
    else if (strcmp(argv[1], "host") == 0) spawn(argv[2], "guardian", argv[2], argv[3], 0);
    else if (strcmp(argv[1], "guardian") == 0) spawn(argv[3], "bun", argv[2], argv[3], 1);
    else if (strcmp(argv[1], "bun") != 0) return 115;
    for (;;) pause();
}
"#,
    )
    .expect("write native process lineage fixture");
    let cli = project.join("owned-cli");
    let compiled = Command::new("/usr/bin/xcrun")
        .args(["clang", "-Werror", "-o"])
        .arg(&cli)
        .arg(&source)
        .output()
        .expect("compile tiny macOS ancestry fixture");
    assert!(
        compiled.status.success(),
        "compile ancestry fixture: {compiled:?}"
    );
    let host = stage.join("keld-host");
    let bun = project.join("bun");
    fs::copy(&cli, &host).expect("copy Host image into owned dev stage");
    fs::copy(&cli, &bun).expect("copy Bun image into owned fixture");

    // This independent process has an unrelated group leader. Its continued
    // existence rejects a broad test-runner/host-wide kill as an oracle.
    let unrelated = Command::new("/bin/sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("unrelated process-group sentinel");
    let unrelated_pid = unrelated.id();
    let unrelated_owner = ShippingLaunchCleanup::new(unrelated);

    let cli_child = Command::new(&cli)
        .args(["cli"])
        .arg(&host)
        .arg(&bun)
        .process_group(0)
        .spawn()
        .expect("owned CLI -> Host -> guardian -> Bun fixture");
    let cli_pid = cli_child.id();
    let mut custody = AuthCliCustody::new(cli_child, &project);
    assert_eq!(process_group(cli_pid), cli_pid, "separate CLI group");
    let deadline = Instant::now() + EVENT_DEADLINE;
    let family = loop {
        if let Some(family) = custody.capture_os_groups() {
            break family;
        }
        assert!(
            Instant::now() < deadline,
            "never observed full kernel process lineage"
        );
        std::thread::yield_now();
    };
    let [host_pid, guardian_pid, bun_pid] = family;
    assert_eq!(custody.launch.host_group, Some(host_pid));
    assert_eq!(custody.launch.bun_group, Some(bun_pid));
    assert_ne!(host_pid, unrelated_pid);
    assert_ne!(bun_pid, unrelated_pid);

    let observation = Observation::bind();
    let failure = await_phases_until(
        &observation,
        &project,
        &CASES[0],
        &mut custody,
        Instant::now(),
    )
    .expect_err("forced early deadline must reject before authenticated app-ready");
    assert!(failure.to_string().contains("timed out"), "{failure}");
    drop(custody);
    for pid in [cli_pid, host_pid, guardian_pid, bun_pid] {
        await_process_gone(pid);
    }
    assert!(
        process_exists(unrelated_pid),
        "cleanup signalled an unrelated group"
    );
    drop(observation);
    drop(unrelated_owner);
    await_process_gone(unrelated_pid);
}
