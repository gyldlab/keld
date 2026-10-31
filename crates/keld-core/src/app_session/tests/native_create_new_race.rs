//! Public absent-leaf replay; no mid-walk, syscall-preemption or recovery claim.
use super::super::{fresh_public_fs_session_reads_its_write, report_value};
use super::{OBSERVATION_BOUND, fixture::NativeFixture, support};
use keld_native::fs::acceptance_observer::Snapshot;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write as _};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

const RACER_BYTES: &[u8] = b"racer";
const OUTSIDE_BYTES: &[u8] = b"outside-create-race-unchanged";

/// File values under the existing `NativeFixture` temporary-scope owner.
struct RaceObjects {
    root: PathBuf,
    target: PathBuf,
    late: PathBuf,
    outside: PathBuf,
    outside_identity: (u64, u64),
}

/// OS observations collected before expected native/public outcome assertions.
#[derive(Debug)]
struct RaceObservation {
    racer_bytes: io::Result<Vec<u8>>,
    racer_metadata: io::Result<fs::Metadata>,
    outside_bytes: io::Result<Vec<u8>>,
    outside_metadata: io::Result<fs::Metadata>,
}

impl RaceObservation {
    fn assert_no_effect(self, racer_identity: (u64, u64), outside_identity: (u64, u64)) {
        assert_eq!(
            self.racer_bytes.expect("independent racer bytes"),
            RACER_BYTES,
            "exclusive create changed the raced leaf"
        );
        let metadata = self.racer_metadata.expect("racer identity after return");
        assert!(metadata.is_file(), "raced leaf ceased to be regular");
        assert_eq!((metadata.dev(), metadata.ino()), racer_identity);
        assert_eq!(
            self.outside_bytes.expect("independent outside bytes"),
            OUTSIDE_BYTES
        );
        let metadata = self
            .outside_metadata
            .expect("outside identity after return");
        assert!(metadata.is_file(), "outside sentinel ceased to be regular");
        assert_eq!((metadata.dev(), metadata.ino()), outside_identity);
    }
}

impl RaceObjects {
    fn seed(allowed: &Path) -> Self {
        let root = allowed.parent().expect("owned temporary root").to_owned();
        let target = allowed.join("create-race.txt");
        let late = allowed.join("create-race-unused.txt");
        let outside = root.join("outside-create-race.txt");
        let calibration = allowed.join("create-race-calibration.txt");
        fs::write(&outside, OUTSIDE_BYTES).expect("seed independent outside sentinel");
        fs::write(&calibration, b"calibration").expect("seed existing sensor target");
        let outside_metadata = fs::symlink_metadata(&outside).expect("outside identity");
        let calibration_metadata =
            fs::symlink_metadata(&calibration).expect("calibration identity");
        let outside_identity = (outside_metadata.dev(), outside_metadata.ino());
        assert_ne!(
            outside_identity,
            (calibration_metadata.dev(), calibration_metadata.ino()),
            "independent sensor/sentinel objects alias"
        );
        assert!(!outside.starts_with(allowed));
        // Calibrate before Ready; the absent leaf has no REG identity to count,
        // and sensor work must not consume the held native operation's budget.
        support::calibrate_object_sensor(&calibration);
        let objects = Self {
            root,
            target,
            late,
            outside,
            outside_identity,
        };
        objects.assert_target_absent();
        objects
    }

    fn assert_target_absent(&self) {
        assert!(
            matches!(fs::symlink_metadata(&self.target), Err(error) if error.kind() == io::ErrorKind::NotFound),
            "the public target was not absent before exclusive create"
        );
    }

    fn install_racer(&self) -> (u64, u64) {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.target)
            .expect("host racer exclusively creates the still-absent leaf");
        file.write_all(RACER_BYTES)
            .expect("write host racer sentinel");
        file.flush().expect("flush host racer writes");
        drop(file);
        let metadata = fs::symlink_metadata(&self.target).expect("installed racer identity");
        assert!(metadata.is_file(), "racer did not install a regular leaf");
        let identity = (metadata.dev(), metadata.ino());
        assert_ne!(identity, self.outside_identity, "racer aliases outside");
        assert_eq!(
            fs::read(&self.target).expect("installed racer bytes"),
            RACER_BYTES
        );
        identity
    }

    fn observe(&self) -> RaceObservation {
        RaceObservation {
            racer_bytes: fs::read(&self.target),
            racer_metadata: fs::symlink_metadata(&self.target),
            outside_bytes: fs::read(&self.outside),
            outside_metadata: fs::symlink_metadata(&self.outside),
        }
    }

    fn assert_removed(&self) {
        assert!(
            matches!(fs::symlink_metadata(&self.root), Err(error) if error.kind() == io::ErrorKind::NotFound),
            "old create-race fixture root survives normal cleanup"
        );
    }
}

fn assert_missing_hold(fixture: &NativeFixture, objects: &RaceObjects, held: Snapshot) {
    assert_eq!(
        held,
        Snapshot {
            request_entries: 1,
            walk_entries: 1,
            ..Snapshot::default()
        },
        "invalid absent-leaf pre-create native hold"
    );
    objects.assert_target_absent();
    assert_eq!(
        fixture
            .router()
            .snapshot
            .fs
            .state
            .lock()
            .expect("held lease")
            .in_flight,
        1,
        "native Missing operation lost its lease"
    );
    assert!(matches!(
        fixture.signals.returned.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}

fn assert_create_new_return(returned: Snapshot) {
    assert_eq!(
        returned,
        Snapshot {
            request_entries: 1,
            request_returns: 1,
            walk_entries: 1,
            create_calls: 1,
            ..Snapshot::default()
        },
        "create-new race did not return with exactly one no-effect attempt"
    );
}

fn assert_public_race_report(report: &BTreeMap<String, String>) {
    assert_eq!(
        [
            report_value(report, "call-code"),
            report_value(report, "call-returned"),
            report_value(report, "settlements"),
            report_value(report, "after-code")
        ],
        [Some("KELD-NATIVE-002"), Some("false"), Some("1"), None],
        "typed public create-new race outcome: {report:?}"
    );
}

#[test]
#[ignore = "explicit physical macOS native-observer qualified lane"]
fn public_fs_create_new_race_preserves_raced_leaf() {
    const CASE: &str = "create-new-race";
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::create_new_race::public_fs_create_new_race_preserves_raced_leaf";
    if !support::is_native_child(CASE) {
        support::isolated_case(CASE, SELECTOR);
        return;
    }
    let mut fixture = NativeFixture::start(CASE, "reply-wins", true, false);
    let objects = RaceObjects::seed(&fixture.router().allowed);
    fixture.supply(&objects.target, &objects.late);
    let (written_tx, written) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    fixture.terminal_resume = Some(resume);
    {
        let router = fixture.router();
        router
            .snapshot
            .fs
            .hold_next_terminal_write(written_tx, resume_rx);
        router
            .router
            .handle()
            .signal_ready()
            .expect("real public Ready");
        router
            .taken
            .recv_timeout(OBSERVATION_BOUND)
            .expect("accepted public job");
        router.release.send(()).expect("release accepted job");
    }
    let held = fixture
        .signals
        .held
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual Missing native hold");
    assert_missing_hold(&fixture, &objects, held);
    let racer_identity = objects.install_racer();
    fixture.release_native();
    let returned = fixture
        .signals
        .returned
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual native return");
    // Preserve the independently observed files even if the one-fault control
    // produces a genuine success and writes the raced leaf.
    let observation = objects.observe();
    written
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual correlated terminal write");
    let terminal_lease = fixture
        .router()
        .snapshot
        .fs
        .state
        .lock()
        .expect("terminal held lease")
        .in_flight;
    fixture
        .router()
        .router
        .handle()
        .retire_generation(1)
        .expect("retire after actual terminal write");
    fixture
        .terminal_resume
        .take()
        .expect("terminal release")
        .send(())
        .expect("resume terminal worker");
    fixture
        .router()
        .snapshot
        .fs
        .drain()
        .expect("observe real native owner drain");
    let report = fixture.finish_caller();
    let returned_handles = support::object_handle_count(&objects.target);
    println!(
        "KELD_AC7_CREATE_RACE_RESULT native={returned:?} public={report:?} files={observation:?} racer_identity={racer_identity:?} outside_identity={:?} terminal_lease={terminal_lease} target_handles={returned_handles}",
        objects.outside_identity
    );
    observation.assert_no_effect(racer_identity, objects.outside_identity);
    assert_create_new_return(returned);
    assert_public_race_report(&report);
    assert_eq!(
        terminal_lease, 1,
        "terminal delivery masqueraded as lease drain"
    );
    assert_eq!(
        returned_handles, 0,
        "raced leaf handle survived native return"
    );
    let final_snapshot = fixture.finish_router(&objects.target, &objects.late, 1, RACER_BYTES);
    assert_eq!(
        final_snapshot, returned,
        "native work changed before scope teardown"
    );
    objects.assert_removed();
    fresh_public_fs_session_reads_its_write();
    println!("KELD_AC7_CASE_DONE case={CASE} entries=1 returns=1 settlements=1");
}
