//! GH1188 D7: native membership and rights controls for the shared census.
//! These stand-in Jobs do not claim the full installed-host release path.

use std::io;
use std::mem::ManuallyDrop;
use std::os::windows::io::AsRawHandle as _;
use std::process::Child;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, FILETIME};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, GetThreadTimes, TerminateProcess,
};

use super::spawn_blocking_descendant;
use crate::windows_job::host_death_census::{CensusPolicy, observe_census, open_member_for_policy};
use crate::windows_job::{
    WindowsHostDeathJob, WindowsHostJobObservation, WindowsJobProcessIdSnapshot, WindowsProcessJob,
    list_job_process_ids, query_job_limit_flags,
};

/// Actual native Jobs and processes with an explicit harness-only cleanup owner.
/// The test process is not enrolled in the stand-in host Job. Its duplicated
/// handle is closed here, unlike the real host's process-lifetime capability.
pub(super) struct CensusFixture {
    pub(super) host: WindowsHostDeathJob,
    outer: WindowsProcessJob,
    families: Vec<WindowsProcessJob>,
    pub(super) children: Vec<Child>,
}

impl CensusFixture {
    pub(super) fn new() -> Self {
        let outer = WindowsProcessJob::create().expect("outer native Job");
        let host = WindowsHostDeathJob {
            handle: ManuallyDrop::new(outer.handle.try_clone().expect("retain outer Job")),
            observation: WindowsHostJobObservation {
                limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                nested_under_existing_job: false,
                current_process_assigned: false,
                handle_inheritable: false,
            },
        };
        Self {
            host,
            outer,
            families: Vec::new(),
            children: Vec::new(),
        }
    }

    pub(super) fn add_member(&mut self, new_family: bool) -> usize {
        self.add_child(spawn_blocking_descendant(), new_family)
    }

    pub(super) fn add_child(&mut self, child: Child, new_family: bool) -> usize {
        let index = self.children.len();
        self.children.push(child);
        let child = &self.children[index];
        // SAFETY: both handles are live, owned by this fixture, and the child
        // is assigned to the outer Job before any nested family assignment.
        assert_ne!(
            unsafe {
                AssignProcessToJobObject(
                    self.outer.handle.as_raw_handle().cast(),
                    child.as_raw_handle().cast(),
                )
            },
            0,
            "outer membership: {}",
            io::Error::last_os_error()
        );
        if new_family {
            let mut family = WindowsProcessJob::create().expect("family Job");
            family
                .assign_child(child)
                .expect("verified direct family launch");
            self.families.push(family);
        }
        index
    }

    pub(super) fn end_member(&mut self, index: usize) {
        self.children[index]
            .kill()
            .expect("end only this test-owned member");
        self.children[index].wait().expect("observe member exit");
    }
}

impl Drop for CensusFixture {
    fn drop(&mut self) {
        // Emergency harness cleanup runs after each scenario's observations.
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        // SAFETY: this is the unique duplicate created by new(), taken once in
        // Drop. The test process never joined the stand-in Job. No production
        // host capability is closed, and outer retains the other Job handle.
        drop(unsafe { ManuallyDrop::take(&mut self.host.handle) });
    }
}

#[test]
fn observe_handle_has_no_termination_right() {
    let mut fixture = CensusFixture::new();
    let index = fixture.add_member(false);
    let member = open_member_for_policy(fixture.children[index].id(), CensusPolicy::Observe)
        .expect("query/synchronize observation handle");
    // SAFETY: this test-owned process is live. This deliberately requests a
    // forbidden operation to discriminate the exact handle rights; the normal
    // observation path never calls TerminateProcess.
    let ended = unsafe { TerminateProcess(member.as_raw_handle().cast(), 81) };
    let error = io::Error::last_os_error();
    assert_eq!(ended, 0, "Observe acquired a termination right");
    assert_eq!(
        error.raw_os_error(),
        Some(ERROR_ACCESS_DENIED.cast_signed())
    );
    assert!(
        fixture.children[index]
            .try_wait()
            .expect("member liveness")
            .is_none()
    );
}

#[test]
fn observe_waits_for_non_family_exit_without_killing_or_clearing_limits() {
    let mut fixture = CensusFixture::new();
    let index = fixture.add_member(false);
    let refusal = fixture
        .host
        .observe_non_family_exit(&[], Instant::now() + Duration::from_millis(150))
        .expect_err("a live non-family member blocks observation");
    assert!(refusal.to_string().contains("deadline"), "{refusal}");
    assert!(
        fixture.children[index]
            .try_wait()
            .expect("member liveness")
            .is_none(),
        "Observe killed the member before its owner released it"
    );
    let raw = fixture.outer.handle.as_raw_handle().cast();
    assert_eq!(
        query_job_limit_flags(raw).expect("limit readback"),
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    );
    fixture.end_member(index);
    fixture
        .host
        .observe_non_family_exit(&[], Instant::now() + Duration::from_secs(2))
        .expect("the actual native list settles after independent member exit");
    assert_eq!(
        query_job_limit_flags(raw).expect("limit after successful Observe"),
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    );
}

#[test]
fn observe_excludes_every_retained_family_not_only_the_first() {
    let mut fixture = CensusFixture::new();
    fixture.add_member(true);
    fixture.add_member(true);
    let families: Vec<_> = fixture.families.iter().collect();
    fixture
        .host
        .observe_non_family_exit(&families, Instant::now() + Duration::from_secs(2))
        .expect("both live families are outside release-resource ownership");
    let refusal = fixture
        .host
        .observe_non_family_exit(&families[..1], Instant::now() + Duration::from_millis(150))
        .expect_err("omitting the second family leaves a live non-family member");
    assert!(refusal.to_string().contains("deadline"), "{refusal}");
    for child in &mut fixture.children {
        assert!(child.try_wait().expect("family member liveness").is_none());
    }
}

#[test]
fn observe_retains_a_family_after_its_direct_launch_exits() {
    let mut fixture = CensusFixture::new();
    let leader = fixture.add_member(true);
    let survivor = fixture.add_member(false);
    // SAFETY: the survivor and family are test-owned and already in the same
    // outer Job. This fixture tests retained native membership, not inheritance.
    assert_ne!(
        unsafe {
            AssignProcessToJobObject(
                fixture.families[0].handle.as_raw_handle().cast(),
                fixture.children[survivor].as_raw_handle().cast(),
            )
        },
        0,
        "enroll surviving family member: {}",
        io::Error::last_os_error()
    );
    fixture.end_member(leader);
    fixture
        .host
        .observe_non_family_exit(
            &[&fixture.families[0]],
            Instant::now() + Duration::from_secs(2),
        )
        .expect("an exited primary does not retire the family's descendants");
    assert!(
        fixture.children[survivor]
            .try_wait()
            .expect("survivor liveness")
            .is_none()
    );
    fixture
        .host
        .observe_non_family_exit(&[], Instant::now() + Duration::from_millis(100))
        .expect_err("dropping the retained family from the census must not pass");
}

#[test]
fn observe_refuses_an_unregistered_family() {
    let fixture = CensusFixture::new();
    let unused = WindowsProcessJob::create().expect("empty unregistered family");
    let refusal = fixture
        .host
        .observe_non_family_exit(&[&unused], Instant::now() + Duration::from_secs(1))
        .expect_err("no admission proof for this family");
    assert!(
        refusal.to_string().contains("verified direct launch"),
        "{refusal}"
    );
}

#[test]
fn observe_membership_query_failure_is_not_an_empty_success() {
    let mut fixture = CensusFixture::new();
    let index = fixture.add_member(false);
    // SAFETY: the current-process pseudo handle is valid for the process
    // lifetime but deliberately has the wrong object type for a Job query.
    let wrong_type = unsafe { GetCurrentProcess() };
    let raw = fixture.outer.handle.as_raw_handle().cast();
    let refusal = observe_census(
        raw,
        &[wrong_type],
        Instant::now() + Duration::from_secs(1),
        || list_job_process_ids(raw, "native failure fixture"),
    )
    .expect_err("a failed membership query cannot exempt a process");
    assert!(
        refusal.to_string().contains("family membership"),
        "{refusal}"
    );
    assert!(
        fixture.children[index]
            .try_wait()
            .expect("member still live")
            .is_none()
    );
}

#[test]
fn observe_reused_pid_outside_host_job_requires_a_fresh_snapshot() {
    let mut fixture = CensusFixture::new();
    fixture.children.push(spawn_blocking_descendant()); // Deliberately outside outer.
    let id = fixture.children[0].id();
    let raw = fixture.outer.handle.as_raw_handle().cast();
    let mut snapshots = 0;
    observe_census(raw, &[], Instant::now() + Duration::from_secs(1), || {
        snapshots += 1;
        if snapshots == 1 {
            Ok(WindowsJobProcessIdSnapshot {
                ids: vec![id],
                assigned: 1,
            })
        } else {
            list_job_process_ids(raw, "fresh native snapshot after stale ID")
        }
    })
    .expect("the new native snapshot, not the reused ID, proves absence");
    assert_eq!(snapshots, 2);
    assert!(
        fixture.children[0]
            .try_wait()
            .expect("foreign member liveness")
            .is_none()
    );
}

#[test]
fn observe_checks_the_deadline_after_a_slow_clean_snapshot() {
    let fixture = CensusFixture::new();
    let raw = fixture.outer.handle.as_raw_handle().cast();
    let refusal = observe_census(raw, &[], Instant::now() + Duration::from_millis(10), || {
        let snapshot = list_job_process_ids(raw, "slow snapshot fixture")?;
        // Deliberate enumeration-latency fault, not synchronization by sleeping.
        std::thread::sleep(Duration::from_millis(40));
        Ok(snapshot)
    })
    .expect_err("a clean snapshot returned too late is not a release observation");
    assert!(refusal.to_string().contains("deadline"), "{refusal}");
}

fn thread_cpu_100ns() -> u64 {
    let mut creation = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // SAFETY: the pseudo handle is this live test thread. All four outputs are
    // writable FILETIME values. This is independent timing observation only.
    assert_ne!(
        unsafe {
            GetThreadTimes(
                GetCurrentThread(),
                &raw mut creation,
                &raw mut exit,
                &raw mut kernel,
                &raw mut user,
            )
        },
        0,
        "thread CPU read: {}",
        io::Error::last_os_error()
    );
    let ticks = |t: FILETIME| u64::from(t.dwLowDateTime) | (u64::from(t.dwHighDateTime) << 32);
    ticks(kernel) + ticks(user)
}

#[test]
fn observe_live_member_waits_without_periodic_settled_census_polling() {
    let mut fixture = CensusFixture::new();
    let index = fixture.add_member(false);
    let raw = fixture.outer.handle.as_raw_handle().cast();
    let before = thread_cpu_100ns();
    let mut snapshots = 0_u32;
    let mut unsettled = 0_u32;
    let refusal = observe_census(raw, &[], Instant::now() + Duration::from_secs(5), || {
        snapshots += 1;
        let snapshot = list_job_process_ids(raw, "held-member pacing fixture")?;
        if !snapshot.is_settled() {
            unsettled += 1;
        }
        Ok(snapshot)
    })
    .expect_err("the independently held live member must block release");
    let cpu = Duration::from_nanos((thread_cpu_100ns() - before) * 100);
    assert!(refusal.to_string().contains("deadline"), "{refusal}");
    assert!(
        fixture.children[index]
            .try_wait()
            .expect("held member liveness")
            .is_none(),
        "the observation must not terminate its member"
    );
    eprintln!("held-member census: {snapshots} snapshots, {unsettled} unsettled, CPU {cpu:?}");
    assert!(
        snapshots <= 3 + unsettled,
        "periodic settled polling: {snapshots} snapshots, {unsettled} unsettled"
    );
    assert!(cpu < Duration::from_millis(50), "census busy-spun: {cpu:?}");
}

#[test]
fn observe_unsettled_five_second_census_has_bounded_work_and_cpu() {
    let fixture = CensusFixture::new();
    let raw = fixture.outer.handle.as_raw_handle().cast();
    let before = thread_cpu_100ns();
    let mut snapshots = 0_u32;
    let refusal = observe_census(raw, &[], Instant::now() + Duration::from_secs(5), || {
        snapshots += 1;
        Ok(WindowsJobProcessIdSnapshot {
            ids: Vec::new(),
            assigned: 1,
        })
    })
    .expect_err("an unsettled snapshot never becomes a clean observation");
    let cpu = Duration::from_nanos((thread_cpu_100ns() - before) * 100);
    assert!(refusal.to_string().contains("in transition"), "{refusal}");
    assert!(snapshots <= 203, "unpaced retakes: {snapshots}");
    assert!(cpu < Duration::from_millis(50), "census busy-spun: {cpu:?}");
}
