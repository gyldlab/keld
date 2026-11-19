use super::state::RegistrationState;
use super::{InstallStart, InstallStatus};
use crate::preload_image::AppPreloadBudget;
use crate::webview2::original_bootstrap::render_native;
use sha2::{Digest as _, Sha256};
use std::time::{Duration, Instant};

fn retained_input(deadline: Instant) -> RegistrationState {
    let source = b"globalThis.preload_fixture = true;";
    let image = AppPreloadBudget::default()
        .reserve_window()
        .expect("one source image")
        .from_qualified_graph(
            source.to_vec(),
            Sha256::digest(source).into(),
            Vec::new(),
            std::collections::BTreeMap::from([(String::from("keld"), String::from("fixture"))]),
        )
        .expect("qualified immutable image");
    RegistrationState::new(
        render_native(&image).expect("counted native input"),
        deadline,
    )
}

#[test]
fn inline_completion_cannot_release_input_or_enable_navigation_before_native_return() {
    let now = Instant::now();
    let state = retained_input(now + Duration::from_secs(1));
    let bytes = state.retained_input_bytes().expect("actual allocation");
    let pointer = state.input_pointer().expect("stable original input");
    assert!(bytes > 0);
    assert!(state.begin(now));
    state.complete(true, now);
    assert_eq!(state.status(now), InstallStatus::Pending);
    assert_eq!(state.input_pointer(), Some(pointer));
    assert_eq!(state.retained_input_bytes(), Ok(bytes));
    assert!(!state.native_quiesced());
    assert!(matches!(
        state.previous_start(),
        Some(InstallStart::OutcomeUnknown)
    ));
    state.native_return(true, now);
    assert_eq!(state.status(now), InstallStatus::Ready);
    assert_eq!(state.retained_input_bytes(), Ok(0));
    assert!(state.native_quiesced());
    assert!(matches!(
        state.previous_start(),
        Some(InstallStart::AlreadyStarted)
    ));
    assert!(!state.begin(now));
}

#[test]
fn native_acceptance_alone_keeps_the_same_input_until_actual_completion() {
    let now = Instant::now();
    let state = retained_input(now + Duration::from_secs(1));
    let pointer = state.input_pointer();
    assert!(state.begin(now));
    state.native_return(true, now);
    assert_eq!(state.status(now), InstallStatus::Pending);
    assert_eq!(state.input_pointer(), pointer);
    assert!(!state.native_quiesced());
    state.complete(true, now);
    assert_eq!(state.status(now), InstallStatus::Ready);
    assert_eq!(state.retained_input_bytes(), Ok(0));
}

#[test]
fn retirement_cannot_be_undone_by_a_late_successful_callback() {
    let now = Instant::now();
    let state = retained_input(now + Duration::from_secs(1));
    assert!(state.begin(now));
    state.native_return(true, now);
    let bytes = state.retained_input_bytes();
    state.retire();
    assert_eq!(state.retained_input_bytes(), bytes);
    assert!(!state.native_quiesced());
    state.complete(true, now);
    assert_eq!(state.status(now), InstallStatus::Refused);
    assert_eq!(state.retained_input_bytes(), Ok(0));
    assert!(state.native_quiesced());
}

#[test]
fn duplicate_or_unsolicited_callback_refuses_the_registration() {
    let now = Instant::now();
    let state = retained_input(now + Duration::from_secs(1));
    assert!(state.begin(now));
    state.native_return(true, now);
    state.complete(true, now);
    assert_eq!(state.status(now), InstallStatus::Ready);
    state.complete(true, now);
    assert_eq!(state.status(now), InstallStatus::Refused);

    let fresh = retained_input(now + Duration::from_secs(1));
    fresh.complete(true, now);
    assert!(!fresh.begin(now));
    assert_eq!(fresh.status(now), InstallStatus::Refused);
}

#[test]
fn failed_or_unknown_entry_keeps_native_custody_until_actual_close() {
    let now = Instant::now();
    let failed = retained_input(now + Duration::from_secs(1));
    assert!(failed.begin(now));
    failed.native_return(false, now);
    let bytes = failed
        .retained_input_bytes()
        .expect("retained rejected entry");
    assert!(bytes > 0);
    failed.retire();
    assert_eq!(failed.retained_input_bytes(), Ok(bytes));
    assert!(!failed.native_quiesced());
    failed.controller_closed();
    assert_eq!(failed.retained_input_bytes(), Ok(0));
    assert!(failed.native_quiesced());

    let unknown = retained_input(now + Duration::from_secs(1));
    assert!(unknown.begin(now));
    unknown.native_unwound();
    unknown.complete(true, now);
    assert!(unknown.retained_input_bytes().expect("unknown retained") > 0);
    assert_eq!(unknown.status(now), InstallStatus::Refused);
    unknown.controller_closed();
    assert_eq!(unknown.retained_input_bytes(), Ok(0));
}

#[test]
fn actual_close_during_native_entry_does_not_free_the_in_call_input() {
    let now = Instant::now();
    let state = retained_input(now + Duration::from_secs(1));
    assert!(state.begin(now));
    let bytes = state.retained_input_bytes();
    state.retire();
    state.controller_closed();
    assert_eq!(state.retained_input_bytes(), bytes);
    assert!(!state.native_quiesced());
    state.native_return(true, now);
    assert_eq!(state.retained_input_bytes(), Ok(0));
    assert_eq!(state.status(now), InstallStatus::Refused);
    assert!(state.native_quiesced());
}

#[test]
fn original_deadline_covers_both_receipts_and_never_resets_on_callback() {
    let now = Instant::now();
    let deadline = now + Duration::from_millis(50);
    let state = retained_input(deadline);
    assert!(state.begin(now));
    state.native_return(true, now);
    assert_eq!(state.status(deadline), InstallStatus::Refused);
    state.complete(true, deadline + Duration::from_millis(1));
    assert_eq!(state.status(deadline), InstallStatus::Refused);

    let late = retained_input(deadline);
    assert!(late.begin(now));
    late.native_return(true, now);
    late.complete(true, deadline);
    assert_eq!(late.status(deadline), InstallStatus::Refused);

    let inline = retained_input(deadline);
    assert!(inline.begin(now));
    inline.complete(true, now);
    inline.native_return(true, deadline);
    assert_eq!(inline.status(deadline), InstallStatus::Refused);

    let expired = retained_input(deadline);
    assert!(!expired.begin(deadline));
    expired.retire();
    assert_eq!(expired.retained_input_bytes(), Ok(0));
}

#[test]
fn inconsistent_fence_result_neither_enters_native_nor_cancels_entered_custody() {
    let now = Instant::now();
    let cancelled = retained_input(now + Duration::from_secs(1));
    assert!(matches!(
        cancelled.finish_admission(false),
        Err(InstallStart::NotAdmitted)
    ));
    assert_eq!(cancelled.retained_input_bytes(), Ok(0));

    let entered = retained_input(now + Duration::from_secs(1));
    assert!(entered.begin(now));
    assert!(matches!(
        entered.finish_admission(false),
        Err(InstallStart::OutcomeUnknown)
    ));
    entered.complete(true, now);
    assert!(
        entered
            .retained_input_bytes()
            .expect("ambiguous admission retained")
            > 0
    );
    assert!(!entered.native_quiesced());
    entered.controller_closed();
    assert_eq!(entered.retained_input_bytes(), Ok(0));

    let skipped = retained_input(now + Duration::from_secs(1));
    assert!(matches!(
        skipped.finish_admission(true),
        Err(InstallStart::OutcomeUnknown)
    ));
    assert_eq!(skipped.status(now), InstallStatus::Refused);
    assert!(
        skipped
            .retained_input_bytes()
            .expect("unproven entry retained")
            > 0
    );
    skipped.controller_closed();
    assert_eq!(skipped.retained_input_bytes(), Ok(0));
}
