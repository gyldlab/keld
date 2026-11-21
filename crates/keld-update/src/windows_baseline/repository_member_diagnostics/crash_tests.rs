//! Negative process-exit residue observations at D-64/D-68 operation membership.
//!
//! These test the existing retained-directory *negative* observation after
//! another process exits without teardown. They do not call the gated writer,
//! prove NTFS power-loss durability, mint D-90 eligibility or authorize cleanup.
//! A deletion mutant of the guarded production integration is unavailable:
//! the native D-90 `RepositoryLedgerCensus` witness has no qualified producer,
//! so neither journal/metadata writer can be constructed through that path.
//! These fixtures test local refusal and byte preservation only. In particular,
//! generic `pending-*` residue is not a completed D-64/D-68 publication cut.
//! Existing `KELD_D64_D68_NATIVE_CRASH_*` selectors are test harness names,
//! not evidence of a native KEL-53 writer crash or power-loss recovery.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use super::{require_no_repository_pending_siblings, require_only_pointed_repository_member};
use crate::UpdateError;
use crate::records::{
    PendingOperationKind, PendingRepositoryOperation, RepositoryAnchor,
    decode_pending_repository_operation, encode_pending_repository_operation,
};

const ROOT_ENV: &str = "KELD_D64_D68_NATIVE_CRASH_ROOT";
const CUT_ENV: &str = "KELD_D64_D68_NATIVE_CRASH_CUT";
const CHILD_TEST: &str =
    "windows_baseline::repository_member_diagnostics::crash_tests::journal_membership_crash_child";
const CRASH_EXIT: i32 = 91;

fn owner() -> RepositoryAnchor {
    RepositoryAnchor {
        repository_id: [0x7f; 32],
        app_id: "com.gyldlab.d90-tests".to_owned(),
        owner_sid: "S-1-5-21-500-600-700-1001".to_owned(),
    }
}

fn pending() -> PendingRepositoryOperation {
    let owner = owner();
    PendingRepositoryOperation {
        repository_id: owner.repository_id,
        app_id: owner.app_id,
        owner_sid: owner.owner_sid,
        kind: PendingOperationKind::InitialAdmission,
        attempt_id: [0x2a; 32],
    }
}

fn synced_new(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("create one isolated process-exit fixture leaf");
    file.write_all(bytes).expect("write exact fixture bytes");
    file.sync_all().expect("flush fixture file bytes");
}

#[test]
#[ignore = "private D64/D68 process-exit residue subprocess entry point"]
fn journal_membership_crash_child() {
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("isolated crash root"));
    let cut = std::env::var(CUT_ENV).expect("requested crash boundary");
    let admission = root
        .join("admissions")
        .join(format!("{}.json", "2a".repeat(32)));
    match cut.as_str() {
        "d64-prepared" => {
            synced_new(
                &root.join("admissions").join("pending-incomplete"),
                b"staged-record",
            );
        }
        "d64-admitted-before-d68" => {
            synced_new(
                &root
                    .join("admissions")
                    .join(format!("{}.json", "3b".repeat(32))),
                b"orphan-admitted-record",
            );
        }
        "d68-pointer-without-d64" => {
            let bytes = encode_pending_repository_operation(&pending())
                .expect("canonical pending operation fixture");
            synced_new(&root.join("pending-operation.json"), &bytes);
        }
        "repository-root-pending-residue" => {
            // A generic unfinished repository-root record. There is no
            // admitted D-64 journal or canonical prepared D-68 pointer here:
            // do not classify this as the D-64 -> D-68 production cut.
            synced_new(
                &root.join(format!("pending-{}", "6d".repeat(32))),
                b"unidentified-pending-record",
            );
        }
        "repository-root-pending-residue-with-pointed-name" => {
            // This is an operation-directory shape fixture. The pointed D-64
            // leaf is deliberately opaque, so it proves no D-64 record validity.
            synced_new(&admission, b"pointed-admission-record");
            let bytes = encode_pending_repository_operation(&pending())
                .expect("canonical pending operation fixture");
            synced_new(&root.join("pending-operation.json"), &bytes);
            synced_new(
                &root.join(format!("pending-{}", "6d".repeat(32))),
                b"unaccounted-second-publication",
            );
        }
        "d68-with-unpointed-recovery" => {
            synced_new(&admission, b"pointed-admission-record");
            let bytes = encode_pending_repository_operation(&pending())
                .expect("canonical pending operation fixture");
            synced_new(&root.join("pending-operation.json"), &bytes);
            synced_new(
                &root.join("recoveries").join("pending-unfinished"),
                b"foreign-recovery-cut",
            );
        }
        other => panic!("unrecognized native crash cut: {other}"),
    }
    println!("KELD_D64_D68_NATIVE_CRASH_CUT={cut}");
    std::io::stdout()
        .flush()
        .expect("flush named subprocess boundary");
    // Deliberately bypass destructors. The next process must diagnose every
    // orphan, dangling pointer or incomplete sibling without deleting it.
    std::process::exit(CRASH_EXIT);
}

fn native_members(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut observed = Vec::new();
    for entry in fs::read_dir(root).expect("inspect repository root entries") {
        let entry = entry.expect("read repository root entry");
        let kind = entry.file_type().expect("read root member kind");
        if kind.is_dir() {
            assert!(
                ["admissions", "recoveries", "transitions"]
                    .iter()
                    .any(|name| entry.file_name() == OsStr::new(name)),
                "unexpected native root directory at {:?}",
                entry.path(),
            );
        } else {
            assert!(
                kind.is_file(),
                "unexpected native root member at {:?}",
                entry.path()
            );
            observed.push((
                PathBuf::from(entry.file_name()),
                fs::read(entry.path()).expect("read root record bytes"),
            ));
        }
    }
    for directory in ["admissions", "recoveries", "transitions"] {
        for entry in fs::read_dir(root.join(directory)).expect("inspect surviving operation names")
        {
            let entry = entry.expect("read operation member");
            assert!(
                entry
                    .file_type()
                    .expect("read operation member kind")
                    .is_file(),
                "unexpected non-file operation member at {:?}",
                entry.path(),
            );
            observed.push((
                PathBuf::from(directory).join(entry.file_name()),
                fs::read(entry.path()).expect("read preserved crash-cut bytes"),
            ));
        }
    }
    observed.sort_by(|a, b| a.0.cmp(&b.0));
    observed
}

/// Independent per-case name/byte oracle before the negative reader runs.
/// The D-64-shaped leaves contain opaque fixture data, not canonical journal
/// bytes or authenticated installation custody. Canonical D-68 pointer bytes
/// are used only in pointer-present fixtures; no case proves a positive
/// D-64-to-D-68 native publication or power-loss recovery.
fn expected_members(cut: &str) -> Vec<(PathBuf, Vec<u8>)> {
    let pointer_bytes =
        encode_pending_repository_operation(&pending()).expect("canonical expected D-68 bytes");
    let pointer = (PathBuf::from("pending-operation.json"), pointer_bytes);
    let admitted_journal_member = (
        PathBuf::from("admissions").join(format!("{}.json", "2a".repeat(32))),
        b"pointed-admission-record".to_vec(),
    );
    let mut expected = match cut {
        "d64-prepared" => vec![(
            PathBuf::from("admissions").join("pending-incomplete"),
            b"staged-record".to_vec(),
        )],
        "d64-admitted-before-d68" => vec![(
            PathBuf::from("admissions").join(format!("{}.json", "3b".repeat(32))),
            b"orphan-admitted-record".to_vec(),
        )],
        "d68-pointer-without-d64" => vec![pointer],
        "repository-root-pending-residue" => vec![(
            PathBuf::from(format!("pending-{}", "6d".repeat(32))),
            b"unidentified-pending-record".to_vec(),
        )],
        "repository-root-pending-residue-with-pointed-name" => vec![
            admitted_journal_member,
            pointer,
            (
                PathBuf::from(format!("pending-{}", "6d".repeat(32))),
                b"unaccounted-second-publication".to_vec(),
            ),
        ],
        "d68-with-unpointed-recovery" => vec![
            admitted_journal_member,
            pointer,
            (
                PathBuf::from("recoveries").join("pending-unfinished"),
                b"foreign-recovery-cut".to_vec(),
            ),
        ],
        other => panic!("unexpected per-cut expectation {other}"),
    };
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    expected
}

fn read_capture(file: &mut File) -> String {
    file.seek(SeekFrom::Start(0))
        .expect("rewind reaped crash-cut child output");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .expect("read reaped crash-cut child output");
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A process-exit cut must not hang CI when an invalid child selector or a
/// stalled writer never exits. File-backed output cannot fill an unread pipe.
fn run_crash_child(root: &Path, cut: &str) {
    let mut stdout_file = tempfile::tempfile().expect("isolated child stdout capture");
    let mut stderr_file = tempfile::tempfile().expect("isolated child stderr capture");
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
        .env(ROOT_ENV, root)
        .env(CUT_ENV, cut)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            stdout_file.try_clone().expect("child stdout file handle"),
        ))
        .stderr(Stdio::from(
            stderr_file.try_clone().expect("child stderr file handle"),
        ))
        .spawn()
        .expect("execute native crash-cut child");
    let start = Instant::now();
    let mut wait_fault = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < Duration::from_secs(60) => {
                // Poll only for the kill switch; the child must exit at its
                // witnessed crash boundary without a scheduling assumption.
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => break None,
            Err(cause) => {
                wait_fault = Some(cause);
                break None;
            }
        }
    };
    if status.is_none() {
        let kill = child.kill();
        let reaped = child.wait();
        let stdout = read_capture(&mut stdout_file);
        let stderr = read_capture(&mut stderr_file);
        assert!(kill.is_ok(), "failed to terminate child at {cut}: {kill:?}");
        assert!(reaped.is_ok(), "failed to reap child at {cut}: {reaped:?}");
        panic!(
            "child missed bounded crash cut {cut}; wait error={wait_fault:?}; stdout={stdout}; stderr={stderr}"
        );
    }
    let status = status.expect("completed child has exit status");
    let stdout = read_capture(&mut stdout_file);
    let stderr = read_capture(&mut stderr_file);
    assert_eq!(
        status.code(),
        Some(CRASH_EXIT),
        "child must exit at {cut}; stdout={stdout}; stderr={stderr}",
    );
    assert!(
        stdout.contains(&format!("KELD_D64_D68_NATIVE_CRASH_CUT={cut}")),
        "child must attest the requested cut before termination; stdout={stdout}; stderr={stderr}"
    );
}

#[test]
fn native_restart_refuses_unresolved_repository_residue_after_process_exit() {
    for (cut, expected_step) in [
        ("d64-prepared", "repository member namespace"),
        ("d64-admitted-before-d68", "repository member namespace"),
        ("d68-pointer-without-d64", "repository pointed membership"),
        ("d68-with-unpointed-recovery", "repository member namespace"),
        (
            "repository-root-pending-residue",
            "repository pending publication residue",
        ),
        (
            "repository-root-pending-residue-with-pointed-name",
            "repository pending publication residue",
        ),
    ] {
        let root = tempfile::tempdir().expect("independent repository residue fixture root");
        for directory in ["admissions", "recoveries", "transitions"] {
            fs::create_dir(root.path().join(directory)).expect("isolated operation parent");
        }
        run_crash_child(root.path(), cut);

        let before = native_members(root.path());
        let expected = expected_members(cut);
        assert_eq!(
            before, expected,
            "the child must leave the exact relative names and bytes required at {cut}"
        );
        let pointer_path = root.path().join("pending-operation.json");
        let pointed = if pointer_path.exists() {
            Some(
                decode_pending_repository_operation(
                    &fs::read(&pointer_path).expect("read persisted native pointer"),
                )
                .expect("persisted D-68 pointer is canonical"),
            )
        } else {
            None
        };
        let expects_pointer = matches!(
            cut,
            "d68-pointer-without-d64"
                | "d68-with-unpointed-recovery"
                | "repository-root-pending-residue-with-pointed-name"
        );
        assert_eq!(
            pointed,
            expects_pointer.then(pending),
            "the exact decoded repository, app, owner SID, operation kind and attempt must match at {cut}"
        );
        let admissions = Dir::open_ambient_dir(root.path().join("admissions"), ambient_authority())
            .expect("restart retains admissions parent");
        let repository = Dir::open_ambient_dir(root.path(), ambient_authority())
            .expect("restart retains exact repository parent");
        let recoveries = Dir::open_ambient_dir(root.path().join("recoveries"), ambient_authority())
            .expect("restart retains recoveries parent");
        let transitions =
            Dir::open_ambient_dir(root.path().join("transitions"), ambient_authority())
                .expect("restart retains transitions parent");
        let refused = require_no_repository_pending_siblings(&repository)
            .and_then(|()| {
                require_only_pointed_repository_member(
                    &admissions,
                    &recoveries,
                    &transitions,
                    &owner(),
                    pointed.as_ref(),
                )
            })
            .expect_err("unresolved physical crash residue cannot become eligible metadata");
        match refused {
            UpdateError::Baseline { step, .. } => assert_eq!(step, expected_step, "{cut}"),
            other => panic!("unexpected native crash-cut refusal for {cut}: {other}"),
        }
        assert_eq!(
            native_members(root.path()),
            expected,
            "restart diagnosis must preserve every journal and pointer byte at {cut}"
        );
    }
}
