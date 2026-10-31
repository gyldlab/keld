//! Public retained-target replay after walk; no mid-walk or blocked-syscall claim.
use super::super::{assert_public_success_report, fresh_public_fs_session_reads_its_write};
use super::{
    OBSERVATION_BOUND,
    fixture::{NativeFixture, held_snapshot},
    support,
};
use keld_native::fs::acceptance_observer::Snapshot;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::{MetadataExt as _, symlink};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

const INSIDE_BEFORE: &[u8] = b"inside-before";
const OUTSIDE_BEFORE: &[u8] = b"outside-unchanged";

/// Path/identity values under `NativeFixture`'s existing owned temporary scope.
struct NamespaceObjects {
    old_root: PathBuf,
    target: PathBuf,
    retained: PathBuf,
    late: PathBuf,
    outside: PathBuf,
    inside_identity: (u64, u64),
    outside_identity: (u64, u64),
}

/// Independent file observations, collected before caller/result assertions.
struct NamespaceObservation {
    inside_after: io::Result<Vec<u8>>,
    outside_after: io::Result<Vec<u8>>,
    outside_after_identity: io::Result<(u64, u64)>,
}

impl NamespaceObservation {
    fn print(
        &self,
        returned: &Snapshot,
        report: &BTreeMap<String, String>,
        returned_handles: usize,
    ) {
        let Self {
            inside_after,
            outside_after,
            outside_after_identity,
        } = self;
        println!(
            "KELD_AC7_NAMESPACE_RESULT native={returned:?} public={report:?} inside={inside_after:?} outside={outside_after:?} outside_identity={outside_after_identity:?} retained_handles={returned_handles}"
        );
    }

    fn assert_retained_inside_and_unchanged_outside(self, outside_identity: (u64, u64)) {
        assert_eq!(
            self.inside_after.expect("independent retained bytes"),
            b"t2"
        );
        assert_eq!(
            self.outside_after.expect("independent outside bytes"),
            OUTSIDE_BEFORE
        );
        assert_eq!(
            self.outside_after_identity
                .expect("outside identity after effect"),
            outside_identity
        );
    }
}

impl NamespaceObjects {
    fn seed(allowed: &Path) -> Self {
        let old_root = allowed.parent().expect("owned root").to_owned();
        let target = allowed.join("native-race.txt");
        let retained = allowed.join("native-retained.txt");
        let late = allowed.join("native-race-unused.txt");
        // This separately seeded object is outside the immutable allowed subtree.
        let outside = old_root.join("namespace-outside.txt");
        fs::write(&target, INSIDE_BEFORE).expect("seed retained inside object");
        fs::write(&outside, OUTSIDE_BEFORE).expect("seed independent outside sentinel");
        let inside_metadata = fs::metadata(&target).expect("original inside identity");
        let outside_metadata = fs::metadata(&outside).expect("original outside identity");
        let inside_identity = (inside_metadata.dev(), inside_metadata.ino());
        let outside_identity = (outside_metadata.dev(), outside_metadata.ino());
        assert_ne!(
            inside_identity, outside_identity,
            "sentinels alias one object"
        );
        assert!(!outside.starts_with(allowed));
        Self {
            old_root,
            target,
            retained,
            late,
            outside,
            inside_identity,
            outside_identity,
        }
    }

    fn substitute_while_native_held(&self, fixture: &NativeFixture) {
        fs::rename(&self.target, &self.retained).expect("move the already open inside object");
        symlink(&self.outside, &self.target).expect("substitute requested leaf with outside link");
        assert!(
            fs::symlink_metadata(&self.target)
                .expect("substituted leaf")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&self.target).expect("actual link target"),
            self.outside
        );
        let moved_metadata = fs::metadata(&self.retained).expect("moved inside identity");
        assert_eq!(
            (moved_metadata.dev(), moved_metadata.ino()),
            self.inside_identity
        );
        assert_eq!(support::object_handle_count(&self.retained), 1);
        assert_eq!(support::object_handle_count(&self.outside), 0);
        assert_eq!(
            fs::read(&self.retained).expect("held inside bytes"),
            INSIDE_BEFORE
        );
        assert_eq!(
            fs::read(&self.outside).expect("held outside bytes"),
            OUTSIDE_BEFORE
        );
        assert!(matches!(
            fixture.signals.returned.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        let inside_identity = self.inside_identity;
        let outside_identity = self.outside_identity;
        println!(
            "KELD_AC7_NAMESPACE_SUBSTITUTED inside={inside_identity:?} outside={outside_identity:?} retained_handles=1 outside_handles=0"
        );
    }

    fn assert_removed(&self) {
        assert!(
            matches!(
                fs::symlink_metadata(&self.old_root),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            ),
            "old fixture root survives normal cleanup"
        );
    }

    fn observe_effect(&self) -> NamespaceObservation {
        NamespaceObservation {
            inside_after: fs::read(&self.retained),
            outside_after: fs::read(&self.outside),
            outside_after_identity: fs::metadata(&self.outside)
                .map(|metadata| (metadata.dev(), metadata.ino())),
        }
    }
}

#[test]
#[ignore = "explicit physical macOS native-observer qualified lane"]
fn public_fs_retained_target_survives_namespace_replacement() {
    const CASE: &str = "namespace-race";
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::namespace_race::public_fs_retained_target_survives_namespace_replacement";
    if !support::is_native_child(CASE) {
        support::isolated_case(CASE, SELECTOR);
        return;
    }
    let mut fixture = NativeFixture::start(CASE, "reply-wins", true, false);
    let namespace = NamespaceObjects::seed(&fixture.router().allowed);
    fixture.supply(&namespace.target, &namespace.late);
    support::calibrate_object_sensor(&namespace.target);
    let (written_tx, written) = mpsc::sync_channel(1);
    let (resume, resume_rx) = mpsc::channel();
    fixture
        .router()
        .snapshot
        .fs
        .hold_next_terminal_write(written_tx, resume_rx);
    fixture.terminal_resume = Some(resume);
    fixture
        .router()
        .router
        .handle()
        .signal_ready()
        .expect("real public Ready");
    fixture
        .router()
        .taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("accepted public job");
    fixture
        .router()
        .release
        .send(())
        .expect("release accepted job");
    held_snapshot(&fixture, CASE, &namespace.target);
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
        "actual native job lost its lease"
    );
    namespace.substitute_while_native_held(&fixture);
    fixture.release_native();
    let returned = fixture
        .signals
        .returned
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual native return");
    // Keep file Results and the typed caller outcome before any result assertion.
    // The discard-and-rewalk control must expose its real error and unchanged bytes.
    let observation = namespace.observe_effect();
    written
        .recv_timeout(OBSERVATION_BOUND)
        .expect("real correlated terminal write");
    fixture
        .router()
        .router
        .handle()
        .retire_generation(1)
        .expect("retire after actual reply");
    fixture
        .terminal_resume
        .take()
        .expect("terminal release")
        .send(())
        .expect("resume replied worker");
    let report = fixture.finish_caller();
    let returned_handles = support::object_handle_count(&namespace.retained);
    observation.print(&returned, &report, returned_handles);
    assert!(
        !returned.fixture_fault,
        "invalid native evidence: {returned:?}"
    );
    assert_eq!((returned.request_entries, returned.request_returns), (1, 1));
    assert_eq!(
        returned.truncate_calls, 1,
        "retained target was not truncated once"
    );
    assert!(returned.content_writes > 0, "retained content write absent");
    assert_public_success_report(&report);
    observation.assert_retained_inside_and_unchanged_outside(namespace.outside_identity);
    assert_eq!(
        returned_handles, 0,
        "target handle survived actual native return"
    );
    let final_snapshot = fixture.finish_router(&namespace.retained, &namespace.late, 1, b"t2");
    assert_eq!(final_snapshot.truncate_calls, 1);
    assert!(final_snapshot.content_writes > 0);
    namespace.assert_removed();
    // The old joined lease/root is gone before this separate healthy public call.
    fresh_public_fs_session_reads_its_write();
    println!("KELD_AC7_CASE_DONE case={CASE} entries=1 returns=1 settlements=1");
}
