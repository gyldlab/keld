//! D8 receipt/binding controls. Native handle liveness comes from the existing
//! live-process and exited-child helpers; wait registration operations are
//! explicit doubles here. Native wait-retirement evidence remains in the
//! existing 1400 fixture, and D6/facade ordering is tested by its own owner.

use super::{
    BrowserBinding, BrowserExitReceipt, BrowserExitSource, BrowserInstance, BrowserProcess,
    COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    OwnedHandle, ProfileErrorKind, ProfileReleaseFault, ProfileReleaseWait, ReleaseSite, Sender,
    WvError, browser_exit_receipt, process_failed_handler, profile_failure_with,
    queue_process_failed,
};
use crate::webview2::tests::{alive_process_handle, exited_process_handle};
use std::cell::{Cell, RefCell};
use std::sync::{Arc, mpsc};
use std::time::Duration;

const A: BrowserInstance = BrowserInstance {
    generation: 1,
    pid: 7,
};
const B: BrowserInstance = BrowserInstance {
    generation: 2,
    pid: 11,
};

fn owner(
    instance: BrowserInstance,
    process: Option<OwnedHandle>,
) -> (Sender<BrowserExitReceipt>, ProfileReleaseWait) {
    let (sender, receiver) = mpsc::channel();
    let binding = Arc::new(BrowserBinding::new());
    binding.store(instance);
    let release = ProfileReleaseWait::with_bounds(
        Some(receiver),
        binding,
        process,
        ReleaseSite::Host,
        Duration::ZERO,
        Duration::from_hours(1),
    );
    (sender, release)
}

fn rebind(release: &ProfileReleaseWait, pid: u32, process: OwnedHandle) -> Result<(), WvError> {
    release.bind_browser_pid_with(
        pid,
        |_| Some(process),
        |old| {
            drop(old);
            Ok(())
        },
        |new| Ok(BrowserProcess::Polled(new)),
    )
}

fn abnormal(instance: BrowserInstance) -> BrowserExitReceipt {
    BrowserExitReceipt::Abnormal {
        instance,
        source: BrowserExitSource::Environment,
    }
}

#[test]
fn binding_snapshot_is_one_generation_pid_pair_and_legacy_bind_is_once() {
    let binding = BrowserBinding::new();
    assert_eq!(binding.load(), BrowserInstance::default());
    assert!(binding.bind_once(0).is_err());
    binding.bind_once(A.pid).expect("legacy binding");
    binding
        .bind_once(A.pid)
        .expect("idempotent loop-entry observation");
    assert_eq!(binding.load(), A);
    assert!(binding.bind_once(B.pid).is_err());
    assert_eq!(binding.load(), A);
    binding.store(BrowserInstance {
        generation: u32::MAX,
        pid: u32::MAX - 1,
    });
    assert_eq!(
        binding.load(),
        BrowserInstance {
            generation: u32::MAX,
            pid: u32::MAX - 1
        }
    );
}

#[test]
fn first_identified_controller_publishes_generation_one_only_after_watch_registration() {
    let (_sender, release) = owner(BrowserInstance::default(), None);
    let operations = RefCell::new(Vec::new());
    release
        .bind_browser_pid_with(
            A.pid,
            |_| {
                operations.borrow_mut().push("open");
                Some(alive_process_handle())
            },
            |old| {
                operations.borrow_mut().push("retire");
                drop(old);
                Ok(())
            },
            |new| {
                operations.borrow_mut().push("register");
                assert_eq!(release.binding.load(), BrowserInstance::default());
                Ok(BrowserProcess::Polled(new))
            },
        )
        .expect("first controller binding");
    assert_eq!(&*operations.borrow(), &["open", "register"]);
    assert_eq!(release.binding.load(), A);
    assert!(!release.observed.get());
    assert!(!release.ambiguous.get());
}

#[test]
fn ac21_a1_old_normal_is_drained_before_rebind_then_credit_is_cleared() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    sender
        .send(BrowserExitReceipt::Normal(A))
        .expect("old native receipt queued during pump");
    let saw_old_credit_before_open = Cell::new(false);
    release
        .bind_browser_pid_with(
            B.pid,
            |_| {
                saw_old_credit_before_open
                    .set(release.observed.get() && release.normal_exit_observed_at.get().is_some());
                Some(exited_process_handle())
            },
            |old| {
                drop(old);
                Ok(())
            },
            |new| Ok(BrowserProcess::Polled(new)),
        )
        .expect("bind next controller");
    assert!(
        saw_old_credit_before_open.get(),
        "controller completion must poll before rebinding"
    );
    assert_eq!(release.binding.load(), B);
    assert!(!release.observed.get(), "A's credit cannot release B");
    assert_eq!(release.normal_exit_observed_at.get(), None);
    release.arm();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::GraceExpired));
}

#[test]
fn ac21_a2_stale_stamped_normal_after_rebind_cannot_credit_current_browser() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    rebind(&release, B.pid, exited_process_handle()).expect("B bind");
    sender
        .send(BrowserExitReceipt::Normal(A))
        .expect("stale receipt");
    release.poll();
    assert!(!release.observed.get());
    release.arm();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::GraceExpired));
}

#[test]
fn ac21_b_foreign_normal_after_rebind_is_dropped_before_generation_stamping() {
    assert_eq!(
        browser_exit_receipt(B, A.pid, true, ReleaseSite::Host),
        None
    );
    assert_eq!(
        browser_exit_receipt(B, B.pid, true, ReleaseSite::Host),
        Some(BrowserExitReceipt::Normal(B))
    );
}

#[test]
fn ac21_c_same_pid_unsignaled_handle_reuses_generation_and_one_receipt() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    let operations = RefCell::new(Vec::new());
    release
        .bind_browser_pid_with(
            A.pid,
            |_| {
                operations.borrow_mut().push("open");
                Some(alive_process_handle())
            },
            |old| {
                operations.borrow_mut().push("retire");
                drop(old);
                Ok(())
            },
            |new| {
                operations.borrow_mut().push("register");
                Ok(BrowserProcess::Polled(new))
            },
        )
        .expect("same live instance");
    assert_eq!(release.binding.load(), A, "same browser keeps generation 1");
    assert!(
        operations.borrow().is_empty(),
        "same browser must keep its native wait"
    );
    sender
        .send(BrowserExitReceipt::Normal(A))
        .expect("final normal receipt");
    release.arm();
    assert!(release.observed.get());
    assert_eq!(release.fault(), None);
}

#[test]
fn ac21_d_same_pid_signaled_handle_is_ambiguous_and_cannot_credit_release() {
    let (sender, release) = owner(A, Some(exited_process_handle()));
    rebind(&release, A.pid, alive_process_handle()).expect("ambiguity is held until terminal");
    assert_eq!(release.binding.load(), A);
    assert!(release.ambiguous.get());
    sender
        .send(BrowserExitReceipt::Normal(A))
        .expect("cannot disambiguate this identity");
    release.arm();
    assert!(!release.observed.get());
    assert_eq!(release.fault(), Some(ProfileReleaseFault::GraceExpired));
}

#[test]
fn ac21_e_older_generation_abnormal_is_not_filtered_by_the_new_binding() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    rebind(&release, B.pid, alive_process_handle()).expect("B bind");
    sender.send(abnormal(A)).expect("old browser fault");
    release.poll();
    assert_eq!(release.binding.load(), B);
    assert_eq!(release.fault(), Some(ProfileReleaseFault::AbnormalExit));
}

#[test]
fn ac21_f_a_bind_after_terminal_arming_is_an_immediate_invariant_fault() {
    let (_sender, release) = owner(A, Some(alive_process_handle()));
    release.arm();
    let opens = Cell::new(0);
    let result = release.bind_browser_pid_with(
        B.pid,
        |_| {
            opens.set(opens.get() + 1);
            Some(alive_process_handle())
        },
        |old| {
            drop(old);
            Ok(())
        },
        |new| Ok(BrowserProcess::Polled(new)),
    );
    assert_eq!(release.fault(), Some(ProfileReleaseFault::RebindUnproven));
    assert!(result.is_err());
    assert_eq!(release.binding.load(), A);
    assert_eq!(
        opens.get(),
        0,
        "terminal state must refuse before native work"
    );
}

#[test]
fn ac21_g_failed_open_cannot_retire_the_old_handle_or_fall_back_to_grace() {
    for pid in [0, B.pid] {
        let (_sender, release) = owner(A, Some(alive_process_handle()));
        let old_handle = release.process.borrow().as_ref().expect("old handle").raw();
        let retires = Cell::new(0);
        let result = release.bind_browser_pid_with(
            pid,
            |_| None,
            |old| {
                retires.set(retires.get() + 1);
                drop(old);
                Ok(())
            },
            |new| Ok(BrowserProcess::Polled(new)),
        );
        assert_eq!(release.fault(), Some(ProfileReleaseFault::RebindUnproven));
        assert!(result.is_err());
        assert_eq!(release.binding.load(), A);
        assert_eq!(retires.get(), 0);
        assert_eq!(
            release
                .process
                .borrow()
                .as_ref()
                .expect("retained handle")
                .raw(),
            old_handle
        );
        assert_eq!(release.post_exit_deadline.get(), None);
    }
}

#[test]
fn generation_overflow_refuses_before_opening_or_retiring_any_handle() {
    let prior = BrowserInstance {
        generation: u32::MAX,
        pid: A.pid,
    };
    let (_sender, release) = owner(prior, Some(alive_process_handle()));
    let opens = Cell::new(0);
    let retires = Cell::new(0);
    let result = release.bind_browser_pid_with(
        B.pid,
        |_| {
            opens.set(opens.get() + 1);
            Some(alive_process_handle())
        },
        |old| {
            retires.set(retires.get() + 1);
            drop(old);
            Ok(())
        },
        |new| Ok(BrowserProcess::Polled(new)),
    );
    assert!(result.is_err());
    assert_eq!(release.fault(), Some(ProfileReleaseFault::RebindUnproven));
    assert_eq!(release.binding.load(), prior);
    assert_eq!((opens.get(), retires.get()), (0, 0));
    assert!(release.process.borrow().is_some());
}

#[test]
fn ac21_h_failed_build_keeps_its_completed_controller_binding_and_unknown_identity_faults() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    rebind(&release, B.pid, exited_process_handle()).expect("bind before a later build step");
    // No View is published: a later build step failed. The same owner still
    // requires B's receipt; a stale A receipt does not discharge that custody.
    sender
        .send(BrowserExitReceipt::Normal(A))
        .expect("old receipt");
    release.arm();
    assert_eq!(release.binding.load(), B);
    assert!(!release.observed.get());
    assert_eq!(release.fault(), Some(ProfileReleaseFault::GraceExpired));

    let (_sender, unidentified) = owner(BrowserInstance::default(), None);
    let error = unidentified.latch_unidentified_controller();
    assert!(error.to_string().contains("controller exists"));
    assert_eq!(
        unidentified.fault(),
        Some(ProfileReleaseFault::UnidentifiedController)
    );
    assert!(
        !unidentified.observed.get(),
        "no native release credit was fabricated"
    );
}

#[test]
fn unidentified_controller_preserves_an_abnormal_receipt_queued_during_its_build() {
    let (sender, release) = owner(BrowserInstance::default(), None);
    sender
        .send(abnormal(A))
        .expect("failure delivered during controller pump");
    let error = release.latch_unidentified_controller();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::AbnormalExit));
    assert!(
        error
            .to_string()
            .contains("the host browser process failed or exited abnormally")
    );
    assert!(!release.binding.load().is_bound());
}

#[test]
fn ac21_i_checked_retirement_failure_never_registers_or_unregisters_again() {
    for refusal in [false, true] {
        let (_sender, release) = owner(A, Some(alive_process_handle()));
        let operations = RefCell::new(Vec::new());
        let live_registrations = Cell::new(1_u32);
        let result = release.bind_browser_pid_with(
            B.pid,
            |_| {
                operations.borrow_mut().push("open");
                Some(alive_process_handle())
            },
            |old| {
                operations.borrow_mut().push("retire");
                drop(old);
                if refusal {
                    Err(profile_failure_with(
                        ProfileErrorKind::LifecycleUnproven,
                        "injected retirement refusal",
                    ))
                } else {
                    live_registrations.set(live_registrations.get() - 1);
                    Ok(())
                }
            },
            |new| {
                operations.borrow_mut().push("register");
                live_registrations.set(live_registrations.get() + 1);
                Ok(BrowserProcess::Polled(new))
            },
        );
        assert_eq!(
            live_registrations.get(),
            1,
            "never two live registration obligations"
        );
        if refusal {
            assert!(result.is_err());
            assert_eq!(&*operations.borrow(), &["open", "retire"]);
            assert_eq!(release.binding.load(), A);
            assert_eq!(release.fault(), Some(ProfileReleaseFault::WakeRetireFailed));
            assert!(
                release.process.borrow().is_none(),
                "failed retirement was consumed"
            );
            assert!(rebind(&release, B.pid, alive_process_handle()).is_err());
            assert_eq!(
                &*operations.borrow(),
                &["open", "retire"],
                "no second retirement"
            );
            release.arm();
            assert_eq!(release.fault(), Some(ProfileReleaseFault::WakeRetireFailed));
        } else {
            assert!(result.is_ok());
            assert_eq!(&*operations.borrow(), &["open", "retire", "register"]);
            assert_eq!(release.binding.load(), B);
            assert_eq!(release.fault(), None);
        }
    }
}

#[test]
fn failed_new_registration_keeps_old_binding_and_latches_without_grace_fallback() {
    let (_sender, release) = owner(A, Some(alive_process_handle()));
    let result = release.bind_browser_pid_with(
        B.pid,
        |_| Some(alive_process_handle()),
        |old| {
            drop(old);
            Ok(())
        },
        |_new| {
            Err(profile_failure_with(
                ProfileErrorKind::LifecycleUnproven,
                "injected registration failure",
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(release.binding.load(), A);
    assert_eq!(release.fault(), Some(ProfileReleaseFault::RebindUnproven));
    assert!(release.process.borrow().is_none());
    assert_eq!(release.post_exit_deadline.get(), None);
}

#[test]
fn ac21_j_process_failed_prevents_rebind_before_the_late_environment_receipt() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    let native_observer_sender = Arc::new(sender);
    let weak = Arc::downgrade(&native_observer_sender);
    assert!(
        queue_process_failed(
            &weak,
            A,
            COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
        )
        .expect("queue native browser failure")
    );
    release.poll();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::AbnormalExit));
    assert!(rebind(&release, B.pid, alive_process_handle()).is_err());
    assert_eq!(
        release.binding.load(),
        A,
        "no controller rebind after the first fault"
    );
    native_observer_sender
        .send(abnormal(A))
        .expect("late environment fault");
    release.poll();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::AbnormalExit));
    assert_eq!(release.binding.load(), A);
}

#[test]
fn ac21_k_foreign_abnormal_environment_event_faults_but_foreign_normal_is_dropped() {
    let (sender, release) = owner(B, Some(alive_process_handle()));
    assert_eq!(
        browser_exit_receipt(B, A.pid, true, ReleaseSite::Host),
        None
    );
    let receipt = browser_exit_receipt(B, A.pid, false, ReleaseSite::Host)
        .expect("any host browser's abnormal event is relevant");
    sender.send(receipt).expect("native environment sender");
    release.poll();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::AbnormalExit));
    assert_eq!(
        browser_exit_receipt(B, A.pid, false, ReleaseSite::Probe),
        None
    );
}

#[test]
fn ac21_l_current_normal_credit_cannot_hide_an_older_abnormal_receipt() {
    let (sender, release) = owner(B, Some(alive_process_handle()));
    sender
        .send(BrowserExitReceipt::Normal(B))
        .expect("current normal");
    release.poll();
    let credited_at = release.normal_exit_observed_at.get();
    assert!(credited_at.is_some());
    sender
        .send(abnormal(A))
        .expect("older browser failure before final exit decision");
    release.poll();
    assert!(
        release.observed.get(),
        "the normal receipt remains recorded"
    );
    assert_eq!(release.normal_exit_observed_at.get(), credited_at);
    assert_eq!(release.fault(), Some(ProfileReleaseFault::AbnormalExit));
    let error = crate::webview2::wait_for_browser_exit(&release).expect_err("fault beats credit");
    assert!(
        error
            .to_string()
            .contains("the host browser process failed or exited abnormally")
    );
}

#[test]
fn ac21_m_nonbrowser_process_failed_kinds_do_not_latch() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    let native_observer_sender = Arc::new(sender);
    let weak = Arc::downgrade(&native_observer_sender);
    // WebView2's published nonbrowser variants have values 1..=9, including
    // renderer, unresponsive, frame, utility, sandbox, GPU, PPAPI and unknown.
    for value in 1..=9 {
        assert!(
            !queue_process_failed(&weak, A, COREWEBVIEW2_PROCESS_FAILED_KIND(value))
                .expect("nonbrowser event is ignored")
        );
        release.poll();
        assert_eq!(release.fault(), None, "kind {value} is not a browser exit");
    }
    assert!(!release.observed.get());
}

#[test]
fn ac21_n_failed_process_failed_registration_preserves_the_bound_receipt_obligation() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    rebind(&release, B.pid, exited_process_handle()).expect("new controller bound");
    let attempted = Cell::new(None);
    let result = release.install_process_failed_with(|instance| {
        attempted.set(Some(instance));
        Err(WvError::Webview(
            "injected add_ProcessFailed failure".to_owned(),
        ))
    });
    assert!(matches!(result, Err(WvError::Webview(_))));
    assert_eq!(attempted.get(), Some(B));
    assert_eq!(
        release.binding.load(),
        B,
        "registration failure cannot roll back the bind"
    );
    assert_eq!(release.fault(), None, "D12 owns this build failure");
    sender
        .send(BrowserExitReceipt::Normal(A))
        .expect("older normal");
    release.arm();
    assert!(!release.observed.get());
    assert_eq!(release.fault(), Some(ProfileReleaseFault::GraceExpired));
}

#[test]
fn a_retained_view_callback_does_not_hide_disappearance_of_the_native_observer() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    let native_observer_sender = Arc::new(sender);
    let retained_callback =
        process_failed_handler(A, Arc::downgrade(&native_observer_sender), || Ok(()));
    assert_eq!(Arc::strong_count(&native_observer_sender), 1);
    drop(native_observer_sender);
    release.poll();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::ObserverDropped));
    assert!(!release.observed.get());
    drop(retained_callback);
}

#[test]
fn missing_observer_cannot_supply_managed_release_credit() {
    let release = ProfileReleaseWait::new(
        None,
        Arc::new(BrowserBinding::new()),
        None,
        ReleaseSite::Host,
    );
    assert!(rebind(&release, A.pid, alive_process_handle()).is_err());
    assert_eq!(release.fault(), Some(ProfileReleaseFault::ObserverDropped));
    assert!(!release.binding.load().is_bound());
}

#[test]
fn every_new_fault_keeps_the_first_release_failure() {
    let (_sender, release) = owner(A, Some(alive_process_handle()));
    assert_eq!(
        release.latch_fault(ProfileReleaseFault::WakeRetireFailed),
        ProfileReleaseFault::WakeRetireFailed
    );
    for later in [
        ProfileReleaseFault::UnidentifiedController,
        ProfileReleaseFault::RebindUnproven,
        ProfileReleaseFault::ReleasingExpired,
        ProfileReleaseFault::AbnormalExit,
    ] {
        assert_eq!(
            release.latch_fault(later),
            ProfileReleaseFault::WakeRetireFailed
        );
        assert_eq!(release.fault(), Some(ProfileReleaseFault::WakeRetireFailed));
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[test]
fn later_generation_control_withholds_every_generation_after_the_first() {
    for generation in [1, 2, 3, u32::MAX] {
        let instance = BrowserInstance {
            generation,
            pid: A.pid,
        };
        let (sender, release) = owner(instance, Some(exited_process_handle()));
        let release = release.withholding_later_generation_receipt(Duration::ZERO);
        sender
            .send(BrowserExitReceipt::Normal(instance))
            .expect("matching native receipt");
        release.arm();
        assert_eq!(
            release.observed.get(),
            generation == 1,
            "generation {generation}"
        );
        assert_eq!(
            release.fault(),
            (generation >= 2).then_some(ProfileReleaseFault::GraceExpired),
            "generation {generation}"
        );
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[test]
fn later_generation_control_keeps_first_credit_then_withholds_generation_three() {
    let (sender, release) = owner(A, Some(alive_process_handle()));
    let release = release.withholding_later_generation_receipt(Duration::ZERO);
    sender
        .send(BrowserExitReceipt::Normal(A))
        .expect("first browser normal receipt");
    release.poll();
    assert!(
        release.observed.get(),
        "generation 1 remains eligible for normal credit"
    );
    assert!(release.normal_exit_observed_at.get().is_some());
    assert_eq!(release.fault(), None);

    rebind(&release, B.pid, exited_process_handle()).expect("generation 2 bind");
    assert_eq!(release.binding.load(), B);
    assert!(
        !release.observed.get(),
        "generation 1 credit cannot release generation 2"
    );
    sender
        .send(BrowserExitReceipt::Normal(B))
        .expect("second browser normal receipt");
    release.poll();
    assert!(
        !release.observed.get(),
        "generation 2's own receipt is withheld"
    );
    assert_eq!(
        release.fault(),
        None,
        "withholding alone does not arm shutdown"
    );

    let third = BrowserInstance {
        generation: 3,
        pid: 13,
    };
    rebind(&release, third.pid, exited_process_handle()).expect("generation 3 bind");
    assert_eq!(release.binding.load(), third);
    assert!(!release.observed.get());
    assert_eq!(release.normal_exit_observed_at.get(), None);
    sender
        .send(BrowserExitReceipt::Normal(A))
        .expect("late generation 1 receipt");
    sender
        .send(BrowserExitReceipt::Normal(third))
        .expect("third browser normal receipt");
    release.poll();
    assert!(
        !release.observed.get(),
        "generation 3's own receipt must also be withheld"
    );
    assert_eq!(release.normal_exit_observed_at.get(), None);
    assert_eq!(
        release.fault(),
        None,
        "the terminal grace begins only when armed"
    );
    release.arm();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::GraceExpired));
}
