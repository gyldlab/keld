use crate::support::PROCESS_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::process::await_process_gone;
use crate::support::unix_descriptors::census_isolation;
use crate::support::unix_descriptors::process_has_open_descriptors;
use crate::support::unix_descriptors::unix_descriptors;
use crate::support::unix_descriptors::unix_descriptors_reported;
use crate::support::unix_descriptors::unix_socket_identities;
use crate::support::unix_descriptors::unix_sockets_not_inherited_from_harness;
use std::io::BufRead;
use std::io::BufReader;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;

/// KEL-222: an empty census must prove the process still has a descriptor table.
///
/// `lsof` exits 0 with no rows for a live process owning no Unix socket and also for
/// one whose table has already been torn down on the way out, and zero is the
/// *passing* value for the CLI. [`unix_descriptors`] therefore corroborates an empty
/// census with [`process_has_open_descriptors`]. The exiting window cannot be entered
/// on demand, so what is pinned here is the corroboration: it answers yes for a live
/// process and no for one that is gone. A stub that always answers yes fails this
/// test, and the census then accepts teardown as evidence.
///
/// This test does not call [`unix_descriptors`], so deleting the empty-result reject
/// stays green here;
/// [`empty_unix_census_of_a_process_without_a_descriptor_table_is_rejected`] is the
/// load-bearing path.
#[test]
fn unix_descriptor_census_requires_a_live_descriptor_table() {
    let _isolation = census_isolation();
    assert!(
        process_has_open_descriptors(std::process::id()),
        "this harness is running, so it has a descriptor table"
    );
    let child = Command::new("/usr/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch a fixture that exits immediately");
    let pid = child.id();
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "fixture failed: {output:?}");
    await_process_gone(pid);
    assert!(
        !process_has_open_descriptors(pid),
        "{pid} has exited and been reaped, so it has no descriptor table"
    );
}

/// KEL-222: an empty census of a process without a descriptor table is rejected.
///
/// This is the reject that closes the exiting-window false pass, and it has to be
/// pinned where it lives rather than in a helper beside it. An earlier attempt
/// extracted the assert and called it directly, which pinned the assert and not the
/// census: deleting the census's call to it left all eight tests green. Driving
/// [`unix_descriptors_reported`] with the output instead reaches the same decision
/// the census makes, so deleting the reject turns this red.
///
/// A reaped pid supplies the state that matters — no table — and the empty output
/// supplies the census result that `lsof` gives for both a clean process and one
/// being torn down. The live half of the test is the negative control: the same
/// empty output for a process that does have a table must be accepted, or the
/// reject would just be a ban on empty censuses.
#[test]
fn empty_unix_census_of_a_process_without_a_descriptor_table_is_rejected() {
    let _isolation = census_isolation();
    assert!(
        unix_descriptors_reported(std::process::id(), "").is_empty(),
        "an empty census of this live harness is a result, not an error"
    );

    let child = Command::new("/usr/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch a fixture that exits immediately");
    let pid = child.id();
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "fixture failed: {output:?}");
    await_process_gone(pid);
    let rejected = std::panic::catch_unwind(|| unix_descriptors_reported(pid, ""));
    assert!(
        rejected.is_err(),
        "an empty census of reaped {pid} is teardown and must be rejected"
    );
    let censused = std::panic::catch_unwind(|| unix_descriptors(pid));
    assert!(
        censused.is_err(),
        "censusing reaped {pid} must fail on lsof's exit status too"
    );
}

/// KEL-222: `lsof` output that cannot be trusted fails loudly, never quietly.
///
/// The parser's own guards, driven with the output they exist to reject. A record
/// `lsof` starts and never names would otherwise be dropped in silence, and
/// dropping one of the target's descriptors is the direction that passes. An
/// address that is not one matters because two empty strings compare equal and
/// would excuse each other. Neither shape has been seen in real output, which is
/// why they are pinned here rather than left to a comment.
#[test]
fn untrustworthy_lsof_output_is_rejected() {
    let _isolation = census_isolation();
    let pid = std::process::id();
    assert_eq!(
        unix_descriptors_reported(pid, "f3\nd0xabc\nn/tmp/one.sock\n").len(),
        1,
        "a well-formed record is still accepted"
    );
    for (shape, rendered) in [
        (
            "a record left unnamed",
            "f3\nd0xabc\nf5\nd0xdef\nn/tmp/two.sock\n",
        ),
        (
            "the last record left unnamed",
            "f3\nd0xabc\nn/tmp/one.sock\nf5\nd0xdef\n",
        ),
        ("an address that is not one", "f3\nd\nn/tmp/one.sock\n"),
    ] {
        let rejected = std::panic::catch_unwind(|| unix_descriptors_reported(pid, rendered));
        assert!(rejected.is_err(), "{shape} must be rejected: {rendered:?}");
    }
}

/// KEL-222: owning no Unix descriptor is an empty census, not a census failure.
///
/// Zero is the *passing* value for the CLI, so the census must distinguish "this
/// process owns none" from "the census could not run". This pins that: the child
/// closes every Unix descriptor this harness could leak into it, discovered from
/// the harness's own census rather than hard-coded, so it provably owns none, and
/// reports readiness on its own pipe so the census never races the `exec`.
///
/// What it pins is the `lsof` exit contract, not attribution. The second
/// assertion below cannot fail once the first passes, because the charged set is
/// always a subset of the observed one — it is kept as a statement of the
/// relationship, not as independent detection. Attribution is proved by
/// [`unix_descriptor_census_charges_only_self_opened_sockets`] and the two
/// collision tests, each of which fails against a census that gets it wrong.
#[test]
fn unix_descriptor_census_of_a_process_without_unix_descriptors_is_empty() {
    let _isolation = census_isolation();
    let mut closes = String::new();
    for record in unix_descriptors(std::process::id()) {
        // 0, 1 and 2 are replaced by the spawn's own stdio redirection.
        if !matches!(record.descriptor.as_str(), "0" | "1" | "2") {
            closes.push_str("exec ");
            closes.push_str(&record.descriptor);
            closes.push_str(">&-; ");
        }
    }
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("{closes}echo READY; exec /bin/cat"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch descriptor-free census fixture");
    let child_pid = child.id();
    // A fixture that never reports must fail this test, not hang it.
    let readiness = child.stdout.take().expect("census fixture readiness pipe");
    let (sender, reported) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let _ = sender.send(BufReader::new(readiness).read_line(&mut line).map(|_| line));
    });
    let ready = reported
        .recv_timeout(PROCESS_DEADLINE)
        .expect("descriptor-free fixture reports readiness within its deadline")
        .expect("read descriptor-free fixture readiness");
    assert_eq!(ready.trim_end(), "READY");
    reader.join().expect("readiness reader joins");

    let observed = unix_socket_identities(child_pid);
    assert!(
        observed.is_empty(),
        "descriptor-free fixture still owns Unix descriptors: {observed:?}"
    );
    let opened = unix_sockets_not_inherited_from_harness(child_pid);
    assert!(
        opened.is_empty(),
        "descriptor-free fixture was charged Unix descriptors: {opened:?}"
    );

    drop(child.stdin.take().expect("census fixture release lease"));
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
}
