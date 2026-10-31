//! Windows Job process-ID snapshot growth and retake contract tests.
//! Real OS-membership assertions and injected kernel response oracles stay together.

use std::io;
use std::os::windows::io::AsRawHandle as _;
use std::process::Child;
use std::time::Instant;

use windows_sys::Win32::Foundation::ERROR_MORE_DATA;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, JOBOBJECT_BASIC_PROCESS_ID_LIST,
};

use super::super::{
    JOB_PROCESS_ID_LIST_GROWTH_LIMIT, JOB_PROCESS_ID_LIST_HEADER_WORDS,
    JOB_PROCESS_ID_LIST_INITIAL_IDS, WindowsProcessJob, create_job_completion_port,
    list_job_process_ids, list_job_process_ids_from, list_job_process_ids_with,
};
use super::{RELEASE_WAIT, spawn_blocking_descendant};

#[test]
fn the_process_id_lister_grows_from_a_short_buffer_to_the_full_list() {
    // The Job's members are exactly the occupants: `spawn_blocking_descendant`
    // creates them with no console, so no console host joins the Job behind
    // the test's back. Its end is the kernel's own notice: the completion
    // port is associated while the Job is empty, so the active-process-zero
    // message cannot be lost, and the empty list is asserted only after Job
    // accounting read zero, through the crate's `wait_for_active_zero`, with
    // the deadline as a kill switch. When an exited member leaves the list
    // relative to its handle's signal is the kernel's ordering, not this
    // test's, so a list taken straight after `wait` proves nothing.
    //
    // What this cell guards (KEL-270 F54): against the real kernel, the lister
    // returns exactly the Job's members from three initial capacities. A
    // success truncated at capacity 1 or 2 is not caught here but by the
    // lister's full-buffer retake
    // (`the_process_id_lister_retakes_a_success_that_filled_the_buffer`); only
    // a truncation below capacity would fail this cell. Capacities 1, 2 and 32
    // take the ERROR_MORE_DATA path twice and the plain path once.
    let job = WindowsProcessJob::create().expect("create a kill-on-close Job");
    let raw = job.handle.as_raw_handle().cast();
    let completion_port =
        create_job_completion_port(raw).expect("associate a completion port with the empty Job");
    let mut occupants: Vec<Child> = (0..3).map(|_| spawn_blocking_descendant()).collect();
    for occupant in &occupants {
        // SAFETY: both handles are live; the occupant is a test-owned fixture
        // assigned to a test-owned Job for the duration of this test.
        assert_ne!(
            unsafe { AssignProcessToJobObject(raw, occupant.as_raw_handle().cast()) },
            0,
            "assign an occupant: {}",
            io::Error::last_os_error()
        );
    }
    let mut expected: Vec<u32> = occupants.iter().map(Child::id).collect();
    expected.sort_unstable();
    for initial in [1, 2, JOB_PROCESS_ID_LIST_INITIAL_IDS] {
        let mut listed = list_job_process_ids_from(raw, initial, "test list")
            .expect("list the Job")
            .ids;
        listed.sort_unstable();
        assert_eq!(listed, expected, "initial capacity {initial}");
    }
    for occupant in &mut occupants {
        occupant.kill().expect("end an occupant");
        occupant.wait().expect("reap an occupant");
    }
    job.wait_for_active_zero(&completion_port, Instant::now() + RELEASE_WAIT)
        .expect("the Job reaches zero active processes once its occupants exited");
    assert_eq!(
        list_job_process_ids(raw, "test list")
            .expect("list the empty Job")
            .ids,
        Vec::<u32>::new(),
        "a Job at zero active processes lists no process"
    );
}

/// Writes one process-ID list record into a lister buffer, as the kernel
/// does: the two counts, then `ids` from the first array slot. `listed` is
/// `ids.len()`; `assigned` is whatever the case reports.
fn write_process_id_record(buffer: &mut [usize], assigned: u32, ids: &[u32]) {
    assert!(
        buffer.len() > JOB_PROCESS_ID_LIST_HEADER_WORDS + ids.len().saturating_sub(1),
        "the lister's buffer must hold the header and the IDs"
    );
    let listed = u32::try_from(ids.len()).expect("a small ID count");
    let record = buffer
        .as_mut_ptr()
        .cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
    // SAFETY: `buffer` is usize-aligned, writable storage holding at least the
    // record header (asserted above); the two writes stay inside it and keep
    // no pointer.
    unsafe {
        (&raw mut (*record).NumberOfAssignedProcesses).write(assigned);
        (&raw mut (*record).NumberOfProcessIdsInList).write(listed);
    }
    for (slot, id) in buffer
        .iter_mut()
        .skip(JOB_PROCESS_ID_LIST_HEADER_WORDS)
        .zip(ids)
    {
        *slot = usize::try_from(*id).expect("a u32 ID fits usize");
    }
}

#[test]
fn a_member_leaving_the_job_does_not_exhaust_the_snapshot_retakes() {
    // KEL-270 F54: while a member leaves the Job the kernel reports one more
    // assigned process than it lists, with success and room to spare. That
    // state is not a short buffer, and no larger buffer changes it, so the
    // lister returns what it listed at once instead of retaking until its
    // growth limit refuses a healthy release. The injected query freezes the
    // window that a descheduled exiting thread holds open on the real kernel.
    let mut calls = 0_u32;
    let snapshot = list_job_process_ids_with(32, "test list", |buffer| {
        calls += 1;
        write_process_id_record(buffer, 4, &[10, 11, 12]);
        Ok(())
    })
    .expect("a member in transition must not refuse the snapshot");
    assert_eq!(calls, 1, "a complete list is not retaken");
    assert_eq!(snapshot.ids, [10, 11, 12]);
    assert_eq!(snapshot.assigned, 4);
}

#[test]
fn the_process_id_lister_grows_on_error_more_data_and_reads_the_full_list() {
    // Control for the F54 fix: a too-small buffer is the kernel's
    // ERROR_MORE_DATA with the header written, and `assigned` sizes the retake.
    let wanted: Vec<u32> = (100..140).collect();
    let mut capacities = Vec::new();
    let snapshot = list_job_process_ids_with(4, "test list", |buffer| {
        let capacity = buffer.len() - JOB_PROCESS_ID_LIST_HEADER_WORDS;
        capacities.push(capacity);
        if capacity < wanted.len() {
            write_process_id_record(buffer, 40, &wanted[..capacity]);
            return Err(io::Error::from_raw_os_error(ERROR_MORE_DATA.cast_signed()));
        }
        write_process_id_record(buffer, 40, &wanted);
        Ok(())
    })
    .expect("the list grows to fit");
    assert_eq!(capacities.len(), 2, "{capacities:?}");
    assert_eq!(capacities[0], 4);
    assert!(capacities[1] >= 40, "{capacities:?}");
    assert_eq!(snapshot.ids, wanted);
    assert_eq!(snapshot.assigned, 40);
}

#[test]
fn the_process_id_lister_growth_limit_still_bounds_genuine_growth() {
    // Control for the F54 fix: a Job that outruns every buffer is refused after
    // the growth limit, not queried without end.
    let mut calls = 0_u32;
    let refusal = list_job_process_ids_with(4, "test list", |buffer| {
        calls += 1;
        let capacity = buffer.len() - JOB_PROCESS_ID_LIST_HEADER_WORDS;
        let ids = vec![7_u32; capacity];
        write_process_id_record(
            buffer,
            u32::try_from(capacity + 1).expect("a small count"),
            &ids,
        );
        Err(io::Error::from_raw_os_error(ERROR_MORE_DATA.cast_signed()))
    })
    .expect_err("a Job that outgrows every buffer must be refused");
    assert_eq!(calls, JOB_PROCESS_ID_LIST_GROWTH_LIMIT);
    assert!(refusal.to_string().contains("outgrew"), "{refusal}");
}

#[test]
fn the_process_id_lister_retakes_a_success_that_filled_the_buffer() {
    // The lister's defensive rule: a success whose list fills the buffer is
    // retaken with a larger one, because the kernel could not then have shown
    // whether the list was cut. The second query is the one that settles it.
    let wanted: Vec<u32> = (200..204).collect();
    let mut capacities = Vec::new();
    let snapshot = list_job_process_ids_with(4, "test list", |buffer| {
        let capacity = buffer.len() - JOB_PROCESS_ID_LIST_HEADER_WORDS;
        capacities.push(capacity);
        let listed = capacity.min(wanted.len());
        write_process_id_record(buffer, 4, &wanted[..listed]);
        Ok(())
    })
    .expect("the retake settles the list");
    assert_eq!(capacities.len(), 2, "{capacities:?}");
    assert_eq!(capacities[0], 4);
    assert!(capacities[1] > 4, "{capacities:?}");
    assert_eq!(snapshot.ids, wanted);
    assert_eq!(snapshot.assigned, 4);
}
