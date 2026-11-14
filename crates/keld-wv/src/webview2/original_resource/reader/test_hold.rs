//! Debug fixture barrier while the existing reader owns the actual result.
//! This module borrows the Work/result/credit; it owns no task, native value,
//! reader thread, delivery slot, queue, cancellation state or release receipt.
#![cfg(all(feature = "profile-test-hooks", debug_assertions))]
#![forbid(unsafe_code)]

use super::super::Source;
use crate::resource_intake::{RetainedByteCredit, TaskTicket};
use crate::resource_source::{ResourceSourceError, SelectedRead, VerifiedBytes};
use crate::webview2::PROFILE_LAUNCH_DEADLINE;
use crate::{WebviewId, WvError};
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(super) const FIXTURE_ASSET: &str = "x14-held-resource.svg";

/// All references name fields still held by the same `Work`/`read_work` stack.
/// The caller passes the actual verified read result before publishing Completed.
pub(super) struct ReadObservation<'a> {
    pub(super) ticket: TaskTicket,
    pub(super) view: WebviewId,
    pub(super) selected: &'a SelectedRead,
    pub(super) source: &'a Source,
    pub(super) credit: &'a RetainedByteCredit,
    pub(super) result: &'a Result<VerifiedBytes, ResourceSourceError>,
    pub(super) cancelled: &'a AtomicBool,
}

pub(super) fn wait_for_fixture_release(
    directory: &Path,
    observed: &ReadObservation<'_>,
) -> Result<(), WvError> {
    let fail = |detail: &str| WvError::EventLoop(format!("X14 resource hold: {detail}"));
    if observed.selected.path != FIXTURE_ASSET {
        return Err(fail("caller did not select the one fixed fixture asset"));
    }
    let bytes = observed
        .result
        .as_ref()
        .map_err(|_| fail("actual selected read did not succeed"))?;
    let account = observed.source.retained_credit_account();
    if !observed.credit.covers(&account, bytes.bytes.len()) {
        return Err(fail("actual result has no covering retained credit"));
    }
    let release = directory.join("release");
    let checkpoint = directory.join("checkpoint");
    if !directory.is_absolute() {
        return Err(fail("hold requires an absolute directory"));
    }
    for path in [&release, &checkpoint] {
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(fail("hold requires fresh release and checkpoint barriers")),
        }
    }
    let deadline = Instant::now() + PROFILE_LAUNCH_DEADLINE;
    let mut observed_checkpoint = false;
    let mut observed_cancelled = false;
    emit(
        "held",
        observed,
        bytes.bytes.len(),
        account.load(Ordering::Acquire),
    );
    loop {
        if observed.cancelled.load(Ordering::Acquire) && !observed_cancelled {
            observed_cancelled = true;
            emit(
                "cancelled",
                observed,
                bytes.bytes.len(),
                account.load(Ordering::Acquire),
            );
        }
        if !observed_checkpoint {
            match std::fs::symlink_metadata(&checkpoint) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    observed_checkpoint = true;
                    emit(
                        "checkpoint",
                        observed,
                        bytes.bytes.len(),
                        account.load(Ordering::Acquire),
                    );
                }
                Ok(_) => return Err(fail("fixture checkpoint is not an ordinary file")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(fail("fixture checkpoint observation failed")),
            }
        }
        if Instant::now() >= deadline {
            emit(
                "expired",
                observed,
                bytes.bytes.len(),
                account.load(Ordering::Acquire),
            );
            return Err(fail(
                "fixture release did not arrive within the fixed hold bound",
            ));
        }
        match std::fs::symlink_metadata(&release) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                if !observed.credit.covers(&account, bytes.bytes.len()) {
                    return Err(fail("same result lost its retained body credit"));
                }
                emit(
                    "released",
                    observed,
                    bytes.bytes.len(),
                    account.load(Ordering::Acquire),
                );
                return Ok(());
            }
            Ok(_) => return Err(fail("fixture release is not an ordinary file")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(fail("fixture release observation failed")),
        }
        // Ordering comes from the observed held record and release file.
        // This bounded recheck is not a timing assumption or a second UI pump.
        std::thread::park_timeout(
            Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

fn emit(phase: &str, observed: &ReadObservation<'_>, body: usize, account: usize) {
    let _ = writeln!(
        std::io::stderr(),
        "KELD_ORIGINAL_RESOURCE_HOLD {}",
        serde_json::json!({
            "phase": phase,
            "ticket": format!("{:?}", observed.ticket),
            "view": observed.view.0,
            "asset": observed.selected.path,
            "bodyBytes": body,
            "accountBytes": account,
            "creditCoversBody": observed.credit.covers(
                &observed.source.retained_credit_account(), body),
            "cancelled": observed.cancelled.load(Ordering::Acquire),
        })
    );
}
