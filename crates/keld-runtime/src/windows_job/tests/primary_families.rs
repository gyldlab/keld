//! Native controls for retained primary families, including both inheritance orders.

use super::census_observe::CensusFixture;
use super::spawn_blocking_descendant;
use crate::windows_job::WindowsPrimaryFamilies;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::os::windows::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::WAIT_TIMEOUT;
use windows_sys::Win32::System::Threading::{
    DETACHED_PROCESS, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    WaitForSingleObject,
};

const HELPER_PORT: &str = "KELD_TEST_PRIMARY_FAMILY_PORT";
const CONTROL_BOUND: Duration = Duration::from_secs(5);

#[test]
fn pending_enrollment_refuses_release_and_seals_future_spawns() {
    let fixture = CensusFixture::new();
    let families = WindowsPrimaryFamilies::new();
    let reservation = families.begin_generation().expect("reserve before spawn");
    let error = families
        .observe_non_family_exit(&fixture.host, Instant::now() + CONTROL_BOUND)
        .expect_err("a pending child cannot disappear from the release inventory");
    assert!(
        error.to_string().contains("enrollment is incomplete"),
        "{error}"
    );
    drop(reservation);
    assert!(
        families.begin_generation().is_err(),
        "failed release must not reopen admission"
    );
}

#[test]
fn aborted_unspawned_enrollment_does_not_consume_a_family_slot() {
    let fixture = CensusFixture::new();
    let families = WindowsPrimaryFamilies::new();
    drop(
        families
            .begin_generation()
            .expect("reserve without launching"),
    );
    drop(
        families
            .begin_generation()
            .expect("abort released its reservation"),
    );
    families
        .observe_non_family_exit(&fixture.host, Instant::now() + CONTROL_BOUND)
        .expect("no native member was ever created");
    assert!(
        families.begin_generation().is_err(),
        "successful release seals admission too"
    );
}

#[test]
fn bound_native_primary_is_excluded_but_the_omitted_family_control_fails() {
    let mut fixture = CensusFixture::new();
    let families = WindowsPrimaryFamilies::new();
    let mut enrollment = families.begin_generation().expect("pre-spawn reservation");
    let child = fixture.add_member(false);
    enrollment
        .bind_child(&fixture.children[child])
        .expect("bind retained direct Child");
    drop(enrollment);
    families
        .observe_non_family_exit(&fixture.host, Instant::now() + CONTROL_BOUND)
        .expect("enrolled primary is family");
    assert!(
        fixture.children[child]
            .try_wait()
            .expect("native liveness")
            .is_none()
    );
    fixture
        .host
        .observe_non_family_exit(&[], Instant::now() + Duration::from_millis(100))
        .expect_err("omitting the actual family cannot look like a released browser tree");
}

/// One caller-controlled leader: the descendant starts only at the requested edge.
#[test]
#[ignore = "private subprocess entry point"]
fn generation_leader_helper() {
    let port: u16 = std::env::var(HELPER_PORT)
        .expect("private family control port")
        .parse()
        .expect("control port");
    let address = ([127, 0, 0, 1], port).into();
    let mut control =
        TcpStream::connect_timeout(&address, CONTROL_BOUND).expect("control connection");
    control
        .set_read_timeout(Some(CONTROL_BOUND))
        .expect("bound helper input");
    control
        .set_write_timeout(Some(CONTROL_BOUND))
        .expect("bound helper output");
    let mut action = [0_u8];
    control
        .read_exact(&mut action)
        .expect("explicit descendant-start edge");
    assert_eq!(action, [b's']);
    let child = spawn_blocking_descendant();
    control
        .write_all(&child.id().to_le_bytes())
        .expect("report actual descendant");
    control
        .read_exact(&mut action)
        .expect("explicit leader-exit edge");
    assert_eq!(action, [b'x']);
    // Child's handle closes, not its process. The retained family must keep the
    // descendant alive until the outer fixture's separately owned cleanup.
    drop(child);
}

fn leader(fixture: &mut CensusFixture) -> (usize, TcpStream) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("private control listener");
    listener.set_nonblocking(true).expect("bounded accept");
    let mut command = Command::new(std::env::current_exe().expect("native test executable"));
    command
        .args([
            "--exact",
            "windows_job::tests::primary_families::generation_leader_helper",
            "--ignored",
            "--nocapture",
        ])
        .env(
            HELPER_PORT,
            listener
                .local_addr()
                .expect("listener address")
                .port()
                .to_string(),
        )
        .creation_flags(DETACHED_PROCESS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = fixture.add_child(command.spawn().expect("spawn controlled leader"), false);
    let deadline = Instant::now() + CONTROL_BOUND;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("blocking accepted control stream");
                stream
                    .set_read_timeout(Some(CONTROL_BOUND))
                    .expect("bounded read");
                stream
                    .set_write_timeout(Some(CONTROL_BOUND))
                    .expect("bounded write");
                return (child, stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("control accept: {error}"),
        }
        assert!(
            fixture.children[child]
                .try_wait()
                .expect("leader state")
                .is_none(),
            "leader exited before its handshake (including a missing test selector)"
        );
        assert!(Instant::now() < deadline, "leader did not connect");
        std::thread::park_timeout(Duration::from_millis(5));
    }
}

fn start_descendant(control: &mut TcpStream) -> OwnedHandle {
    control.write_all(b"s").expect("release descendant start");
    let mut pid = [0_u8; 4];
    control
        .read_exact(&mut pid)
        .expect("read exact descendant id");
    let pid = u32::from_le_bytes(pid);
    assert_ne!(pid, 0);
    // SAFETY: test observer only. Open the reported live descendant before its
    // leader exits, retain its exact identity, and never signal through a PID.
    // https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-openprocess
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    assert!(!raw.is_null(), "open independent descendant observer");
    // SAFETY: OpenProcess returned one new owned non-null handle.
    let handle = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
    assert_alive(&handle);
    handle
}

fn assert_alive(handle: &OwnedHandle) {
    // SAFETY: the retained observer handle has SYNCHRONIZE; zero does not wait.
    // https://learn.microsoft.com/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject
    assert_eq!(
        unsafe { WaitForSingleObject(handle.as_raw_handle().cast(), 0) },
        WAIT_TIMEOUT
    );
}

fn end_leader(fixture: &mut CensusFixture, index: usize, control: &mut TcpStream) {
    control.write_all(b"x").expect("release leader exit");
    let deadline = Instant::now() + CONTROL_BOUND;
    loop {
        if let Some(status) = fixture.children[index]
            .try_wait()
            .expect("leader exit query")
        {
            assert!(status.success(), "the real helper must pass");
            return;
        }
        assert!(Instant::now() < deadline, "leader did not exit");
        std::thread::park_timeout(Duration::from_millis(5));
    }
}

#[test]
fn successor_enrollment_keeps_the_retired_leaders_real_descendant_family() {
    let mut fixture = CensusFixture::new();
    let families = WindowsPrimaryFamilies::new();
    let mut first = families.begin_generation().expect("first reservation");
    let (index, mut control) = leader(&mut fixture);
    first
        .bind_child(&fixture.children[index])
        .expect("first membership before descendant");
    drop(first);
    let survivor = start_descendant(&mut control);
    end_leader(&mut fixture, index, &mut control);
    assert_alive(&survivor);
    let mut successor = families
        .begin_generation()
        .expect("retired leader does not discard live descendants");
    let second = fixture.add_member(false);
    successor
        .bind_child(&fixture.children[second])
        .expect("second generation membership");
    drop(successor);
    families
        .observe_non_family_exit(&fixture.host, Instant::now() + CONTROL_BOUND)
        .expect("both the replacement and the real inherited old descendant remain family");
    assert_alive(&survivor);
    fixture
        .host
        .observe_non_family_exit(&[], Instant::now() + Duration::from_millis(100))
        .expect_err("missing the family owner cannot release the still-live tree");
}

#[test]
fn descendant_started_before_assignment_stays_non_family_and_refuses_release() {
    let mut fixture = CensusFixture::new();
    let families = WindowsPrimaryFamilies::new();
    let mut reservation = families.begin_generation().expect("first reservation");
    let (index, mut control) = leader(&mut fixture);
    let survivor = start_descendant(&mut control);
    reservation
        .bind_child(&fixture.children[index])
        .expect("late membership binds the leader only");
    drop(reservation);
    end_leader(&mut fixture, index, &mut control);
    assert_alive(&survivor);
    let error = families
        .observe_non_family_exit(&fixture.host, Instant::now() + Duration::from_millis(100))
        .expect_err("pre-assignment descendants are not retroactively credited to a family");
    assert!(error.to_string().contains("deadline"), "{error}");
    assert_alive(&survivor);
}

#[test]
fn family_inventory_refuses_the_sixty_fifth_live_generation_then_prunes_proved_zero() {
    let mut fixture = CensusFixture::new();
    let families = WindowsPrimaryFamilies::new();
    for _ in 0..64 {
        let mut enrollment = families
            .begin_generation()
            .expect("at most 64 live families");
        let child = fixture.add_member(false);
        enrollment
            .bind_child(&fixture.children[child])
            .expect("native family membership");
    }
    assert!(
        families.begin_generation().is_err(),
        "the 65th live family must fail before spawn"
    );
    fixture.end_member(0);
    let deadline = Instant::now() + CONTROL_BOUND;
    loop {
        if let Ok(enrollment) = families.begin_generation() {
            drop(enrollment);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native Job accounting never proved the exited family empty"
        );
        std::thread::park_timeout(Duration::from_millis(5));
    }
    families
        .observe_non_family_exit(&fixture.host, Instant::now() + CONTROL_BOUND)
        .expect("remaining live families survive pruning of the one native-proved empty family");
}
