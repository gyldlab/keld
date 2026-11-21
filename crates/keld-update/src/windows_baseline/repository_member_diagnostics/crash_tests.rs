//! Independent process-exit cuts at D-64/D-68 native operation membership.
//!
//! These test the existing retained-directory *negative* observation after
//! another process exits without teardown. They do not call the gated writer,
//! prove NTFS power-loss durability, mint D-90 eligibility or authorize cleanup.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use super::require_only_pointed_repository_member;
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
        .expect("create one crash-cut fixture leaf");
    file.write_all(bytes).expect("write exact crash-cut bytes");
    file.sync_all().expect("flush crash-cut fixture file bytes");
}

#[test]
#[ignore = "private D64/D68 native crash-cut subprocess entry point"]
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
    for directory in ["", "admissions", "recoveries", "transitions"] {
        for entry in fs::read_dir(root.join(directory)).expect("inspect surviving fixture names") {
            let path = entry.expect("read fixture entry").path();
            if path.is_file() {
                observed.push((
                    path.clone(),
                    fs::read(&path).expect("read preserved crash-cut bytes"),
                ));
            }
        }
    }
    observed.sort_by(|a, b| a.0.cmp(&b.0));
    observed
}

#[test]
fn native_restart_refuses_each_unresolved_d64_d68_process_exit_cut() {
    for (cut, expected_step) in [
        ("d64-prepared", "repository member namespace"),
        ("d64-admitted-before-d68", "repository member namespace"),
        ("d68-pointer-without-d64", "repository pointed membership"),
        ("d68-with-unpointed-recovery", "repository member namespace"),
    ] {
        let root = tempfile::tempdir().expect("independent native crash-cut root");
        for directory in ["admissions", "recoveries", "transitions"] {
            fs::create_dir(root.path().join(directory)).expect("isolated operation parent");
        }
        let output = Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
            .env(ROOT_ENV, root.path())
            .env(CUT_ENV, cut)
            .stdin(Stdio::null())
            .output()
            .expect("execute native crash-cut child");
        assert_eq!(
            output.status.code(),
            Some(CRASH_EXIT),
            "child must exit at {cut}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains(&format!("KELD_D64_D68_NATIVE_CRASH_CUT={cut}")),
            "child must attest the requested cut before termination"
        );

        let before = native_members(root.path());
        assert!(
            !before.is_empty(),
            "the child must persist a physical crash-cut object"
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
        assert_eq!(
            pointed.is_some(),
            cut.starts_with("d68-"),
            "the child must leave the intended D-68 publication state"
        );
        let admissions = Dir::open_ambient_dir(root.path().join("admissions"), ambient_authority())
            .expect("restart retains admissions parent");
        let recoveries = Dir::open_ambient_dir(root.path().join("recoveries"), ambient_authority())
            .expect("restart retains recoveries parent");
        let transitions =
            Dir::open_ambient_dir(root.path().join("transitions"), ambient_authority())
                .expect("restart retains transitions parent");
        let refused = require_only_pointed_repository_member(
            &admissions,
            &recoveries,
            &transitions,
            &owner(),
            pointed.as_ref(),
        )
        .expect_err("unresolved physical crash residue cannot become eligible metadata");
        match refused {
            UpdateError::Baseline { step, .. } => assert_eq!(step, expected_step, "{cut}"),
            other => panic!("unexpected native crash-cut refusal for {cut}: {other}"),
        }
        assert_eq!(
            native_members(root.path()),
            before,
            "restart diagnosis must preserve every journal and pointer byte at {cut}"
        );
    }
}
