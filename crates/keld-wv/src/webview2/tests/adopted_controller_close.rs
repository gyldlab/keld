//! A checked adopted Close cannot inherit or renew normal release credit.

use super::{
    ProfileReleaseFault, ProfileReleaseWait, ReleaseSite, abnormal_receipt, normal_receipt,
    release_channel, test_binding, wait_for_browser_exit,
};
use crate::webview2::controller_custody::record_adopted_close_result;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use windows::Win32::Foundation::E_FAIL;

fn credited() -> (
    Rc<ProfileReleaseWait>,
    Sender<crate::webview2::BrowserExitReceipt>,
) {
    let (sender, receiver) = release_channel();
    let release = Rc::new(ProfileReleaseWait::new(
        Some(receiver),
        test_binding(),
        None,
        ReleaseSite::Host,
    ));
    sender
        .send(normal_receipt())
        .expect("normal native receipt");
    release.poll();
    assert!(
        release.observed.get(),
        "the control begins with credited release"
    );
    assert_eq!(release.fault(), None);
    (release, sender)
}

#[test]
fn an_adopted_close_error_refuses_release_after_normal_credit() {
    let (release, _sender) = credited();
    let weak = Rc::downgrade(&release);
    record_adopted_close_result(Err(E_FAIL.into()), Some(&weak));

    let closed = Cell::new(false);
    assert!(release.poll_host_receipts_with(|| {
        assert!(!closed.replace(true));
        release.arm();
    }));
    assert!(closed.get(), "the existing fault reaction must run");
    assert!(
        release.observed.get(),
        "fault priority, not erased receipt, refuses release"
    );
    assert_eq!(
        release.fault(),
        Some(ProfileReleaseFault::ControllerCloseFailed)
    );
    let error = wait_for_browser_exit(&release).expect_err("Close error must prevent success");
    assert!(error.to_string().contains("KELD-WV-009"));
    assert!(error.to_string().contains("returned an error from Close"));
}

#[test]
fn an_adopted_close_error_cannot_replace_an_earlier_native_fault() {
    let (release, sender) = credited();
    sender
        .send(abnormal_receipt())
        .expect("abnormal native receipt");
    release.poll();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::AbnormalExit));
    record_adopted_close_result(Err(E_FAIL.into()), Some(&Rc::downgrade(&release)));
    assert_eq!(release.fault(), Some(ProfileReleaseFault::AbnormalExit));
    assert!(wait_for_browser_exit(&release).is_err());
}

#[test]
fn successful_and_legacy_close_results_preserve_existing_release_behavior() {
    let (release, _sender) = credited();
    record_adopted_close_result(Ok(()), Some(&Rc::downgrade(&release)));
    record_adopted_close_result(Err(E_FAIL.into()), None);
    assert_eq!(release.fault(), None);
    assert_eq!(
        release.armed_at.get(),
        None,
        "a Close result cannot arm the owner"
    );
    assert!(wait_for_browser_exit(&release).is_ok());
}

#[test]
fn an_adopted_close_link_does_not_extend_the_release_owner_lifetime() {
    let (release, sender) = credited();
    let weak = Rc::downgrade(&release);
    assert_eq!(Rc::strong_count(&release), 1);
    drop(release);
    drop(sender);
    assert!(
        weak.upgrade().is_none(),
        "checked sole-owner retirement remains possible"
    );
    record_adopted_close_result(Err(E_FAIL.into()), Some(&weak));
    assert!(weak.upgrade().is_none());
}
