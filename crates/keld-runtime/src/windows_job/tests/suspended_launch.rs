//! Native Windows suspended-child launch and exact Job membership proof tests.
//! Real suspended same-token process fixtures; the main Job owner retains all
//! process/handle/unsafe lifecycle code and ignored subprocess helpers.

use std::ffi::OsString;
use std::path::Path;

use super::{
    WindowsJobMembership, WindowsLaunchedProcess, WindowsProcessJob, WindowsSuspendedChild,
    system32,
};

/// A same-token `cmd.exe /d /c exit 7`, created suspended and not recorded.
pub(super) fn same_token_exit_command() -> WindowsSuspendedChild {
    let system32 = system32();
    let environment: Vec<(OsString, OsString)> = ["SystemRoot", "WINDIR"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
        .collect();
    WindowsSuspendedChild::spawn_same_token(
        &system32.join("cmd.exe"),
        &[
            OsString::from("/d"),
            OsString::from("/c"),
            OsString::from("exit 7"),
        ],
        &environment,
        &system32,
    )
    .expect("create a same-token child suspended")
}

#[test]
fn a_membership_proof_must_name_both_the_process_id_and_the_creation_time() {
    // Seam-injected: forged proofs that each match one of the two facts.
    let mut launched =
        WindowsLaunchedProcess::record(same_token_exit_command()).expect("record the launch");
    let process_id = launched.child().id();
    let creation_time = launched.creation_time;
    for forged in [
        WindowsJobMembership {
            process_id,
            creation_time: creation_time ^ 1,
        },
        WindowsJobMembership {
            process_id: process_id ^ 4,
            creation_time,
        },
    ] {
        let refusal = launched
            .resume(&forged)
            .expect_err("a proof that matches only one fact refuses");
        assert!(
            refusal
                .to_string()
                .contains("the attempt-Job membership proof names another process"),
            "{forged:?}: {refusal}"
        );
        assert!(
            !launched.has_exited().expect("query the launch"),
            "{forged:?}: the refused launch stays suspended"
        );
    }
    let mut job = WindowsProcessJob::create().expect("create an attempt Job");
    let membership = job
        .assign_child(launched.child())
        .expect("assign the launch");
    launched
        .resume(&membership)
        .expect("the real proof resumes the launch");
    assert_eq!(launched.wait(10_000).expect("the launch exits"), 7);
}

#[test]
fn a_launch_record_names_only_a_never_resumed_child() {
    let mut child = same_token_exit_command();
    child
        .resume_held()
        .expect("spend the resume below the launch record");
    let refusal =
        WindowsLaunchedProcess::record(child).expect_err("a resumed child has no launch record");
    assert!(
        refusal.to_string().contains(
            "the child was already resumed, so its Job membership can no longer precede its first instruction"
        ),
        "{refusal}"
    );
}

#[test]
fn suspended_child_owns_the_sole_resume_thread_call() {
    // KEL-53 §5 reuse decision: one suspended-child type owns the only
    // primary-thread resume, so a later launch path cannot add a second one.
    // It counts the identifier, not one call spelling: an alias or a
    // qualified path must still name it, so the import and the one call
    // are the only two occurrences.
    let needle = concat!("Resume", "Thread");
    let mut calls = Vec::new();
    let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("list a source directory") {
            let path = entry.expect("read a source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path).expect("read a source file");
                let count = source.matches(needle).count();
                if count != 0 {
                    let name = path.file_name().expect("file name").to_string_lossy();
                    calls.push((name.into_owned(), count));
                }
            }
        }
    }
    assert_eq!(calls, vec![("windows_lpac.rs".to_owned(), 2)]);
}
