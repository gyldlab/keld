use super::{PostPhase, PostResult};

#[test]
fn retry_classification_never_turns_an_entered_call_into_never_written() {
    assert!(PostPhase::Prepared.prior_attempt_result().is_none());
    for phase in [PostPhase::Posting, PostPhase::OutcomeUnknown] {
        assert!(matches!(
            phase.prior_attempt_result(),
            Some(PostResult::OutcomeUnknown)
        ));
    }
    assert!(matches!(
        PostPhase::Returned.prior_attempt_result(),
        Some(PostResult::AlreadyReturned)
    ));
}

#[test]
fn physical_quiescence_never_frees_an_in_call_post() {
    assert!(!PostPhase::Posting.physically_quiescent(false));
    assert!(!PostPhase::Posting.physically_quiescent(true));
    assert!(!PostPhase::OutcomeUnknown.physically_quiescent(false));
    assert!(PostPhase::OutcomeUnknown.physically_quiescent(true));
    assert!(PostPhase::Returned.physically_quiescent(false));
}
