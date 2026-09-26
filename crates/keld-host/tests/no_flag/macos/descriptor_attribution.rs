use crate::support::PROCESS_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::process::await_process_gone;
use crate::support::unix_descriptors::census_isolation;
use crate::support::unix_descriptors::unix_descriptors;
use crate::support::unix_descriptors::unix_socket_identities;
use crate::support::unix_descriptors::unix_sockets_not_inherited_from_harness;
use std::fs;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixDatagram;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Instant;

/// KEL-222: the Unix-descriptor census charges a descriptor to the process that
/// opened it, never to a process that merely inherited one.
///
/// A shell that leaves a Unix socket without `FD_CLOEXEC` leaks it through every
/// `exec` into the process under test, so counting `lsof -U` rows answers "does
/// this pid hold any Unix descriptor from any source" instead of "did this pid
/// open one". This test reproduces that leak deliberately: the harness keeps its
/// own copy of an accepted Unix stream and hands a duplicate down as the child's
/// stdin, while the child opens one listener of its own. Both expected
/// identities are paths this test chose before the census ran, so neither is
/// read back out of the census under validation.
#[test]
fn unix_descriptor_census_charges_only_self_opened_sockets() {
    let _isolation = census_isolation();
    let fixture = tempfile::tempdir().expect("census fixture root");
    let leaked_path = fixture.path().join("leaked.sock");
    let owned_path = fixture.path().join("owned.sock");
    let leaked_identity = leaked_path.to_str().expect("UTF-8 fixture path").to_owned();
    let owned_identity = owned_path.to_str().expect("UTF-8 fixture path").to_owned();

    // `lsof` names an accepted peer by the listener's bound `sun_path`, so the
    // leaked descriptor's identity is a path this test already knows.
    let leaked_listener = UnixListener::bind(&leaked_path).expect("bind harness leak socket");
    let far_end = UnixStream::connect(&leaked_path).expect("connect harness leak socket");
    let (harness_copy, _) = leaked_listener
        .accept()
        .expect("accept harness leak socket");
    let inherited = harness_copy
        .try_clone()
        .expect("duplicate the leaked descriptor for the child");

    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "unix_descriptor_census_fixture_process"])
        .env("KELD_T2_CENSUS_OWNED_SOCKET", &owned_path)
        .stdin(Stdio::from(OwnedFd::from(inherited)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch Unix-descriptor census fixture");
    let child_pid = child.id();
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while !owned_path.exists() {
        assert!(
            Instant::now() < deadline,
            "census fixture never bound its own Unix listener"
        );
        thread::yield_now();
    }

    // The fixture is only meaningful while the leak is real: prove the child
    // holds the inherited descriptor before asserting that it is not charged.
    let observed = unix_socket_identities(child_pid);
    assert!(
        observed.contains(&leaked_identity),
        "fixture leaked no harness Unix descriptor into the child: {observed:?}"
    );
    assert_eq!(
        unix_sockets_not_inherited_from_harness(child_pid),
        vec![owned_identity],
        "census must charge the child only the listener it bound itself: {observed:?}"
    );

    // Closing the far end is the child's release signal; the harness keeps its
    // own copy and the listener until scope end, so the leaked identity stays in
    // the harness table for the whole census above.
    drop(far_end);
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
}

/// KEL-222: a copy of a socket the harness does not hold is charged.
///
/// A target holding two descriptors on a socket the harness holds once looks the
/// same to `lsof` whether it `dup`ed an inherited descriptor or opened the socket
/// itself and passed a copy up over `SCM_RIGHTS`. The census charges the extra
/// copy, because the dangerous reading of an ambiguous picture is the one where a
/// process opened an app link. This pins that choice using the half of the
/// ambiguity that needs no `SCM_RIGHTS` support to build. A census that excused
/// every copy of a harness-held address charges nothing here and passes.
///
/// Fixture integrity is checked by socket address, and the harness's own copy is
/// counted rather than assumed: by name alone this fixture would look correct
/// while handing down two *different* sockets that share one `sun_path`, and it
/// would then be testing nothing.
///
/// The second copy lands on stderr rather than stdout because the child's test
/// harness writes its result line to stdout after the harness has closed the
/// socket's far end, which would fail the child on `EPIPE`.
#[test]
fn unix_descriptor_census_charges_a_copy_the_harness_does_not_hold() {
    let _isolation = census_isolation();
    let fixture = tempfile::tempdir().expect("census fixture root");
    let leaked_path = fixture.path().join("leaked.sock");
    let owned_path = fixture.path().join("owned.sock");
    let leaked_identity = leaked_path.to_str().expect("UTF-8 fixture path").to_owned();
    let owned_identity = owned_path.to_str().expect("UTF-8 fixture path").to_owned();

    let leaked_listener = UnixListener::bind(&leaked_path).expect("bind harness leak socket");
    let far_end = UnixStream::connect(&leaked_path).expect("connect harness leak socket");
    let (harness_copy, _) = leaked_listener
        .accept()
        .expect("accept harness leak socket");
    // The listener reports the same `sun_path` as the peer it accepted, so leaving
    // it open makes this fixture's own integrity check ambiguous — the very
    // ambiguity the census refuses to resolve by name.
    drop(leaked_listener);
    let first = harness_copy
        .try_clone()
        .expect("duplicate the leaked descriptor for the child");
    let second = harness_copy
        .try_clone()
        .expect("duplicate the leaked descriptor a second time");

    let child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "unix_descriptor_census_fixture_process"])
        .env("KELD_T2_CENSUS_OWNED_SOCKET", &owned_path)
        // stdin and stderr, not stdout: the test harness writes its result line to
        // stdout, and the socket's peer is closed before the child exits.
        .stdin(Stdio::from(OwnedFd::from(first)))
        .stdout(Stdio::null())
        .stderr(Stdio::from(OwnedFd::from(second)))
        .spawn()
        .expect("launch Unix-descriptor census fixture");
    let child_pid = child.id();
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while !owned_path.exists() {
        assert!(
            Instant::now() < deadline,
            "census fixture never bound its own Unix listener"
        );
        thread::yield_now();
    }

    // The fixture only means anything while the child holds two descriptors on one
    // socket and the harness holds exactly one.
    let harness_copies: Vec<String> = unix_descriptors(std::process::id())
        .into_iter()
        .filter(|descriptor| descriptor.identity == leaked_identity)
        .map(|descriptor| descriptor.socket)
        .collect();
    assert_eq!(
        harness_copies.len(),
        1,
        "harness must hold exactly one descriptor on the leaked socket: {harness_copies:?}"
    );
    let leaked_socket = harness_copies
        .into_iter()
        .next()
        .expect("the harness copy just counted");
    let child_table = unix_descriptors(child_pid);
    let copies = child_table
        .iter()
        .filter(|descriptor| descriptor.socket == leaked_socket)
        .count();
    let rendered: Vec<String> = child_table
        .iter()
        .map(|descriptor| {
            format!(
                "f{} d{} n{}",
                descriptor.descriptor, descriptor.socket, descriptor.identity
            )
        })
        .collect();
    assert_eq!(
        copies, 2,
        "fixture did not receive one socket twice: {rendered:?}"
    );
    assert_eq!(
        unix_sockets_not_inherited_from_harness(child_pid),
        vec![leaked_identity, owned_identity],
        "census must charge the copy the harness does not hold: {rendered:?}"
    );

    drop(far_end);
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
}

/// KEL-222: an unbound Unix socket is charged to whoever opened it, because the
/// census cannot prove it was inherited.
///
/// `lsof` reports every unbound Unix socket with the same placeholder instead of
/// a `sun_path` or a peer address, so that string cannot show that two
/// descriptors are the same kernel object. Subtracting it would let a process
/// that opened its own anonymous socket look clean whenever the harness happened
/// to hold one too — a silent loss of detection power in exactly the direction
/// this issue exists to close. The expected placeholder is written literally
/// here, so this test pins what `lsof` was measured to print rather than agreeing
/// with whatever the census believes.
#[test]
fn unix_descriptor_census_charges_anonymous_sockets_it_cannot_attribute() {
    let _isolation = census_isolation();
    let harness_anonymous = UnixDatagram::unbound().expect("harness unbound Unix socket");
    assert!(
        unix_socket_identities(std::process::id())
            .iter()
            .any(|identity| identity == "->(none)"),
        "harness holds no anonymous Unix socket, so this test collides with nothing"
    );

    let fixture = tempfile::tempdir().expect("census fixture root");
    let owned_path = fixture.path().join("owned.sock");
    let owned_identity = owned_path.to_str().expect("UTF-8 fixture path").to_owned();
    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "unix_descriptor_census_fixture_process"])
        .env("KELD_T2_CENSUS_OWNED_SOCKET", &owned_path)
        .env("KELD_T2_CENSUS_ANONYMOUS_SOCKET", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch Unix-descriptor census fixture");
    let child_pid = child.id();
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while !owned_path.exists() {
        assert!(
            Instant::now() < deadline,
            "census fixture never bound its own Unix listener"
        );
        thread::yield_now();
    }

    let observed = unix_socket_identities(child_pid);
    assert!(
        observed.iter().any(|identity| identity == "->(none)"),
        "fixture opened no anonymous Unix socket, so there is nothing to attribute: {observed:?}"
    );
    let mut charged = unix_sockets_not_inherited_from_harness(child_pid);
    charged.sort();
    let mut expected = vec!["->(none)".to_owned(), owned_identity];
    expected.sort();
    assert_eq!(
        charged, expected,
        "census must charge the child the anonymous socket it opened itself: {observed:?}"
    );

    drop(child.stdin.take().expect("census fixture release lease"));
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
    drop(harness_anonymous);
}

/// KEL-222: a `sun_path` is not an identity either, because a socket keeps
/// reporting it after that path is unlinked.
///
/// The harness holds an accepted socket still named by a path that no longer
/// exists on disk, and the child then binds a brand-new listener at that same
/// path. `lsof` prints one name for both, so a name-keyed census excuses the
/// child's own listener; keyed on the socket address the two never collide. This
/// is the second of the two collisions that made name-keying unsound, and it is
/// the one that survives even if `->(none)` is special-cased.
#[test]
fn unix_descriptor_census_charges_a_listener_rebound_on_a_released_path() {
    let _isolation = census_isolation();
    let fixture = tempfile::tempdir().expect("census fixture root");
    let shared_path = fixture.path().join("released.sock");
    let shared_identity = shared_path.to_str().expect("UTF-8 fixture path").to_owned();

    // Keep an accepted end, then release the name: the descriptor still reports it.
    let listener = UnixListener::bind(&shared_path).expect("bind harness release socket");
    let far_end = UnixStream::connect(&shared_path).expect("connect harness release socket");
    let (harness_stale, _) = listener.accept().expect("accept harness release socket");
    drop(listener);
    fs::remove_file(&shared_path).expect("release the fixture path");
    assert!(
        unix_socket_identities(std::process::id()).contains(&shared_identity),
        "harness lost the stale name, so this test collides with nothing"
    );

    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "unix_descriptor_census_fixture_process"])
        .env("KELD_T2_CENSUS_OWNED_SOCKET", &shared_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch Unix-descriptor census fixture");
    let child_pid = child.id();
    let deadline = Instant::now() + PROCESS_DEADLINE;
    while !shared_path.exists() {
        assert!(
            Instant::now() < deadline,
            "census fixture never rebound the released path"
        );
        thread::yield_now();
    }

    let observed = unix_socket_identities(child_pid);
    assert!(
        observed.contains(&shared_identity),
        "fixture did not rebind the released path: {observed:?}"
    );
    assert_eq!(
        unix_sockets_not_inherited_from_harness(child_pid),
        vec![shared_identity],
        "census must charge the listener the child bound itself: {observed:?}"
    );

    drop(child.stdin.take().expect("census fixture release lease"));
    let output = wait_child_output(child, PROCESS_DEADLINE);
    assert!(output.status.success(), "census fixture failed: {output:?}");
    await_process_gone(child_pid);
    drop((harness_stale, far_end));
}
