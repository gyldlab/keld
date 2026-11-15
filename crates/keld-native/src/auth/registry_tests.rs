//! Same-source worker-owned state/event tests. No sleeps or helper threads.
//! Literal provider/entropy/transport markers are NOT provider or OS proof.
use super::*;
use std::cell::Cell;

fn scopes(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
fn account(registry: &mut SessionRegistry, marker: u8) -> String {
    match registry
        .register_completed_begin(
            "entra-common",
            [marker; 32],
            Some("refresh-original".to_owned()),
            None,
            None,
            vec!["narrow".to_owned()],
        )
        .expect("literal completed Begin state event")
    {
        AuthResponse::Begin {
            account_id,
            granted_scopes,
        } => {
            assert_eq!(granted_scopes, ["narrow"]);
            account_id
        }
        AuthResponse::Token { .. } => panic!("registration returned token"),
    }
}
fn token(lifetime: u64) -> Arc<AccessToken> {
    AccessToken::at_receipt(
        "access-private".to_owned(),
        "Bearer".to_owned(),
        lifetime,
        UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        Instant::now(),
    )
    .expect("literal receipt")
}
fn request(
    registry: &mut SessionRegistry,
    id: &str,
    requested: Option<Vec<String>>,
) -> TokenPlan<()> {
    registry
        .token_request(id, requested, || Some(()))
        .expect("owner request event")
}
fn leader(plan: TokenPlan<()>) -> FlightId {
    match plan {
        TokenPlan::Leader {
            flight,
            admission: (),
        } => flight,
        other => panic!("expected leader, got {other:?}"),
    }
}
fn joined(plan: TokenPlan<()>) -> FlightId {
    match plan {
        TokenPlan::InFlight(flight) => flight,
        other => panic!("expected coalesced flight, got {other:?}"),
    }
}
fn start(registry: &mut SessionRegistry, flight: &FlightId) -> ExchangeInput {
    match registry.try_start(flight) {
        StartState::Started(input) => input,
        other => panic!("expected start, got {other:?}"),
    }
}
fn failure(code: &str) -> CallError {
    CallError {
        code: code.to_owned(),
        message: format!("{code}: literal failure. Start a fresh auth.begin."),
    }
}

#[test]
fn canonical_pair_table_has_exact_coalescing_and_one_admission() {
    let rows = [
        (Some(scopes(&["b", "a"])), Some(scopes(&["a", "b"])), true),
        (
            Some(scopes(&["a", "a", "b"])),
            Some(scopes(&["a", "b"])),
            true,
        ),
        (None, Some(vec![]), true),
        (Some(scopes(&["a"])), Some(scopes(&["A"])), false),
        (Some(scopes(&["a"])), Some(scopes(&["a", "b"])), false),
        (Some(scopes(&["a"])), Some(scopes(&["a "])), false),
    ];
    for (a, b, coalesces) in rows {
        let mut registry = SessionRegistry::new();
        let id = account(&mut registry, 1);
        let admissions = Cell::new(0);
        let first = registry
            .token_request(&id, a, || {
                admissions.set(admissions.get() + 1);
                Some(())
            })
            .expect("A");
        let a_id = leader(first);
        let second = registry
            .token_request(&id, b, || {
                admissions.set(admissions.get() + 1);
                Some(())
            })
            .expect("B");
        if coalesces {
            assert_eq!(joined(second), a_id);
            assert_eq!(admissions.get(), 1);
        } else {
            assert_ne!(leader(second), a_id);
            assert_eq!(admissions.get(), 2);
        }
    }
    let mut registry = SessionRegistry::new();
    let a = account(&mut registry, 2);
    let b = account(&mut registry, 3);
    let a_id = leader(request(&mut registry, &a, Some(scopes(&["a"]))));
    let b_id = leader(request(&mut registry, &b, Some(scopes(&["a"]))));
    assert_ne!(a_id, b_id, "same scopes across accounts must not coalesce");
}

#[test]
fn refresh_start_events_serialize_rotation_without_a_wait_or_retry() {
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 4);
    let a = leader(request(&mut registry, &id, Some(scopes(&["b", "a", "b"]))));
    let same = joined(request(&mut registry, &id, Some(scopes(&["a", "b"]))));
    let b = leader(request(
        &mut registry,
        &id,
        Some(scopes(&["wider", "b", "a"])),
    ));
    assert_eq!(same, a);
    let input_a = start(&mut registry, &a);
    assert_eq!(
        input_a.profile().issuer,
        "https://login.microsoftonline.com/common"
    );
    assert_eq!(input_a.refresh_token(), "refresh-original");
    assert_eq!(
        input_a.requested_scopes(),
        Some(scopes(&["b", "a", "b"])).as_deref()
    );
    assert!(matches!(registry.try_start(&a), StartState::AlreadyStarted));
    assert!(matches!(
        registry.try_start(&b),
        StartState::WaitingForRefresh
    ));
    // No thread/sleep/gate: the real owner state is observed before and after
    // the explicit completion event. Returning does not consume B's credential.
    assert_eq!(
        registry.accounts.get(&id).expect("state").refresh_owner,
        Some(a.serial)
    );
    let completed = registry
        .finish_success(&a, token(300), Some("refresh-rotated".to_owned()))
        .expect("A actual state completion");
    assert_eq!(completed.flight, same);
    let input_b = start(&mut registry, &b);
    assert_eq!(input_b.refresh_token(), "refresh-rotated");
    assert_eq!(
        input_b.requested_scopes(),
        Some(scopes(&["wider", "b", "a"])).as_deref()
    );
    assert_eq!(registry.accounts.get(&id).expect("state").flights.len(), 1);
}

#[test]
fn ready_and_coalesced_callers_share_original_result_but_final_expiry_is_separate() {
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 5);
    let a = leader(request(&mut registry, &id, None));
    let b = joined(request(&mut registry, &id, Some(vec![])));
    start(&mut registry, &a);
    let value = token(300);
    let expiry = value.fresh_until;
    let completed = registry
        .finish_success(
            &a,
            Arc::clone(&value),
            Some("refresh-after-completion".to_owned()),
        )
        .expect("completion");
    assert_eq!(completed.flight, b);
    let ready = match registry
        .token_request(&id, None, || -> Option<()> { panic!("Ready cannot admit") })
        .expect("ready")
    {
        TokenPlan::Ready(result) => result,
        other => panic!("expected ready, got {other:?}"),
    };
    assert!(Arc::ptr_eq(&ready, &completed.result));
    let stored = ready.as_ref().as_ref().expect("canonical success");
    assert!(matches!(
        stored.response_at(
            expiry
                .checked_sub(Duration::from_nanos(1))
                .expect("literal expiry minus one nanosecond"),
        ),
        Ok(AuthResponse::Token { .. })
    ));
    assert!(matches!(
        stored.response_at(expiry),
        Err(StateFault::ExpiredAccess)
    ));
    assert!(matches!(
        stored.response_at(expiry + Duration::from_secs(1)),
        Err(StateFault::ExpiredAccess)
    ));
    assert!(Arc::ptr_eq(
        ready.as_ref().as_ref().expect("immutable after expiry"),
        &value
    ));
    assert!(registry.issuer_for(&id).is_some());
    let state = registry.accounts.get(&id).expect("session effect oracle");
    assert_eq!(
        state.refresh.as_ref().expect("rotated").0.as_ref(),
        "refresh-after-completion"
    );
    assert!(
        state.flights.is_empty(),
        "final conversion cannot schedule refresh"
    );
}

#[test]
fn literal_stale_receipt_and_exact_clock_boundary_do_not_end_session() {
    let receipt = Instant::now()
        .checked_sub(Duration::from_mins(10))
        .expect("past literal receipt");
    let value = AccessToken::at_receipt(
        "expired-private".to_owned(),
        "Bearer".to_owned(),
        300,
        UNIX_EPOCH + Duration::from_secs(4_294_967_296),
        receipt,
    )
    .expect("u64 clock");
    assert_eq!(value.expires_at, 4_294_967_596);
    assert!(matches!(value.response(), Err(StateFault::ExpiredAccess)));
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 6);
    let flight = leader(request(&mut registry, &id, None));
    start(&mut registry, &flight);
    let completion = registry
        .finish_success(&flight, value, Some("rotated-even-expired".to_owned()))
        .expect("expired completion");
    assert!(
        matches!(completion.result.as_ref(), Err(error) if error.message.contains("no longer fresh"))
    );
    assert!(registry.issuer_for(&id).is_some());
    let explicit = leader(request(&mut registry, &id, Some(scopes(&["different"]))));
    assert_eq!(
        start(&mut registry, &explicit).refresh_token(),
        "rotated-even-expired"
    );
}

#[test]
fn cache_deadline_is_monotonic_even_with_literal_future_utc() {
    let monotonic = Instant::now();
    let value = AccessToken::at_receipt(
        "t".to_owned(),
        "Bearer".to_owned(),
        300,
        UNIX_EPOCH + Duration::from_secs(4_294_967_296),
        monotonic,
    )
    .expect("literal clocks");
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 7);
    let flight = leader(request(&mut registry, &id, None));
    start(&mut registry, &flight);
    registry
        .finish_success_at(&flight, value, None, monotonic)
        .expect("fresh publication");
    let ready = registry
        .token_request_at(
            &id,
            None,
            || -> Option<()> { panic!("fresh cache cannot admit") },
            monotonic + Duration::from_secs(299),
        )
        .expect("before expiry");
    assert!(matches!(ready, TokenPlan::Ready(_)));
    let explicit = registry
        .token_request_at(&id, None, || Some(()), monotonic + Duration::from_mins(5))
        .expect("at expiry");
    assert!(matches!(explicit, TokenPlan::Leader { .. }));
    assert!(registry.issuer_for(&id).is_some());
}

#[test]
fn epoch_overflow_and_empty_credentials_are_typed_without_zero_fallback() {
    let before = UNIX_EPOCH
        .checked_sub(Duration::from_secs(1))
        .expect("negative literal");
    assert!(matches!(
        AccessToken::at_receipt(
            "t".to_owned(),
            "Bearer".to_owned(),
            1,
            before,
            Instant::now()
        ),
        Err(StateFault::InvalidReceiptClock)
    ));
    assert!(matches!(
        AccessToken::at_receipt(
            "t".to_owned(),
            "Bearer".to_owned(),
            u64::MAX,
            UNIX_EPOCH + Duration::from_secs(1),
            Instant::now()
        ),
        Err(StateFault::ExpiryOverflow)
    ));
    assert!(matches!(
        AccessToken::at_receipt(
            String::new(),
            "Bearer".to_owned(),
            1,
            UNIX_EPOCH,
            Instant::now()
        ),
        Err(StateFault::EmptyCredential)
    ));
    let empty_type =
        AccessToken::at_receipt("t".to_owned(), String::new(), 1, UNIX_EPOCH, Instant::now())
            .expect_err("empty provider token type");
    assert_eq!(empty_type.code(), "KELD-AUTH-013");
}

#[test]
fn terminal_current_invalid_grant_emits_one_result_across_all_scope_flights() {
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 8);
    let a = leader(request(&mut registry, &id, Some(scopes(&["a"]))));
    start(&mut registry, &a);
    let b = leader(request(&mut registry, &id, Some(scopes(&["b"]))));
    let joined_b = joined(request(&mut registry, &id, Some(scopes(&["b"]))));
    let mut events = vec![];
    assert!(registry.finish_failure(
        &a,
        ExchangeFailure::InvalidGrant(failure("KELD-AUTH-009")),
        |event| events.push(event)
    ));
    assert_eq!(events.len(), 2);
    let result = Arc::clone(&events[0].result);
    assert!(
        events
            .iter()
            .all(|event| Arc::ptr_eq(&event.result, &result))
    );
    assert!(events.iter().any(|event| event.flight == joined_b));
    assert_eq!(registry.issuer_for(&id), None);
    assert!(matches!(registry.try_start(&b), StartState::Retired));
    assert!(matches!(
        request(&mut registry, &id, None),
        TokenPlan::Unresolved
    ));
    assert_ne!(
        account(&mut registry, 8),
        id,
        "even literal entropy reuse cannot reuse id"
    );
}

#[test]
fn retired_invalid_grant_cannot_evict_session_or_overwrite_rotated_custody() {
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 9);
    let old = leader(request(&mut registry, &id, Some(scopes(&["old"]))));
    start(&mut registry, &old);
    let old_result = registry
        .finish_success(&old, token(300), Some("refresh-new".to_owned()))
        .expect("old completion");
    let current = leader(request(&mut registry, &id, Some(scopes(&["current"]))));
    start(&mut registry, &current);
    let mut late_events = vec![];
    assert!(!registry.finish_failure(
        &old,
        ExchangeFailure::InvalidGrant(failure("KELD-AUTH-009")),
        |event| late_events.push(event)
    ));
    assert!(late_events.is_empty());
    assert!(registry.issuer_for(&id).is_some());
    assert!(old_result.result.as_ref().is_ok());
    let state = registry
        .accounts
        .get(&id)
        .expect("current account effect oracle");
    assert_eq!(state.refresh_owner, Some(current.serial));
    assert_eq!(
        state.refresh.as_ref().expect("current refresh").0.as_ref(),
        "refresh-new"
    );
    assert!(
        registry
            .finish_success(&old, token(300), Some("forbidden-late-rotation".to_owned()))
            .is_none()
    );
    assert_eq!(
        registry
            .accounts
            .get(&id)
            .expect("state")
            .refresh
            .as_ref()
            .expect("refresh")
            .0
            .as_ref(),
        "refresh-new"
    );
}

#[test]
fn ordinary_failure_has_one_event_no_auto_retry_and_no_session_eviction() {
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 10);
    let flight = leader(request(&mut registry, &id, Some(scopes(&["broad"]))));
    start(&mut registry, &flight);
    let mut events = vec![];
    assert!(registry.finish_failure(
        &flight,
        ExchangeFailure::Failed(failure("KELD-AUTH-013")),
        |event| events.push(event)
    ));
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0].result.as_ref(), Err(error) if error.code == "KELD-AUTH-013"));
    let state = registry.accounts.get(&id).expect("session retained");
    assert!(state.flights.is_empty());
    assert_eq!(state.refresh_owner, None);
    assert!(
        matches!(
            request(&mut registry, &id, Some(scopes(&["broad"]))),
            TokenPlan::Leader { .. }
        ),
        "only explicit new request creates another flight"
    );
}

#[test]
fn owner_deadline_event_can_retire_pending_custody_without_a_wait_or_cleanup_lock() {
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 11);
    let a = leader(request(&mut registry, &id, Some(scopes(&["a"]))));
    start(&mut registry, &a);
    let pending = leader(request(&mut registry, &id, Some(scopes(&["pending"]))));
    assert!(matches!(
        registry.try_start(&pending),
        StartState::WaitingForRefresh
    ));
    // Existing Core's clock decides this event. Registry neither renews the
    // clock nor invents follower-cancel policy or a background cleanup job.
    let completed = registry
        .abandon(&pending, failure("KELD-AUTH-013"))
        .expect("synchronous owner retirement");
    assert_eq!(completed.flight, pending);
    assert!(completed.result.as_ref().is_err());
    assert!(matches!(registry.try_start(&pending), StartState::Retired));
    let state = registry.accounts.get(&id).expect("account");
    assert_eq!(state.refresh_owner, Some(a.serial));
    assert_eq!(state.flights.len(), 1);
    // Removing a local follower correlation requires NO registry method call.
    assert!(matches!(registry.try_start(&a), StartState::AlreadyStarted));
}

#[test]
fn accepted_host_tail_preserves_account_and_same_result_ignores_late_callbacks() {
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 12);
    let active = leader(request(&mut registry, &id, Some(scopes(&["a"]))));
    start(&mut registry, &active);
    let pending = leader(request(&mut registry, &id, Some(scopes(&["b"]))));
    let mut events = vec![];
    registry.stop_for_host_tail(|event| events.push(event));
    assert_eq!(events.len(), 2);
    assert!(Arc::ptr_eq(&events[0].result, &events[1].result));
    assert!(
        matches!(events[0].result.as_ref(), Err(error) if error.message.contains("host is ending"))
    );
    assert!(
        registry.issuer_for(&id).is_some(),
        "host tail is not account eviction"
    );
    assert!(matches!(registry.try_start(&pending), StartState::Retired));
    assert!(matches!(
        registry.token_request(&id, None, || Some(())),
        Err(StateFault::Stopping)
    ));
    assert!(
        registry
            .finish_success(
                &active,
                token(300),
                Some("forbidden-tail-rotation".to_owned())
            )
            .is_none()
    );
    assert!(!registry.finish_failure(
        &active,
        ExchangeFailure::InvalidGrant(failure("KELD-AUTH-009")),
        |_| panic!("retired callback emitted")
    ));
    assert_eq!(
        registry
            .accounts
            .get(&id)
            .expect("tail account")
            .refresh
            .as_ref()
            .expect("old custody")
            .0
            .as_ref(),
        "refresh-original"
    );
    let mut repeated = vec![];
    registry.stop_for_host_tail(|event| repeated.push(event));
    assert!(repeated.is_empty());
    drop(registry); // Ordinary drop only; no lock acquisition/callback/cleanup job.
}

#[test]
fn real_admission_lease_stays_with_owner_and_capacity_never_inserts_a_flight() {
    struct Lease<'a>(&'a Cell<u32>);
    impl Drop for Lease<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 13);
    assert!(matches!(
        registry
            .token_request::<()>(&id, None, || None)
            .expect("capacity"),
        TokenPlan::AtCapacity
    ));
    assert!(
        registry
            .accounts
            .get(&id)
            .expect("state")
            .flights
            .is_empty()
    );
    let drops = Cell::new(0);
    let (flight, owned_lease) = match registry
        .token_request(&id, None, || Some(Lease(&drops)))
        .expect("admitted")
    {
        TokenPlan::Leader { flight, admission } => (flight, admission),
        other => panic!("expected admitted owner, got {other:?}"),
    };
    start(&mut registry, &flight);
    let mut events = vec![];
    registry.stop_for_host_tail(|event| events.push(event));
    assert_eq!(events.len(), 1);
    assert_eq!(
        drops.get(),
        0,
        "logical completion must not release physical lease"
    );
    // The REAL transport quiescence receipt must occur here in integration.
    // This state test proves lease location/order only, not actual HTTP cleanup.
    drop(owned_lease);
    assert_eq!(drops.get(), 1);
}

#[test]
fn missing_refresh_completes_once_without_session_eviction_or_consumed_exchange() {
    let mut registry = SessionRegistry::new();
    let id = account(&mut registry, 14);
    registry
        .accounts
        .get_mut(&id)
        .expect("literal missing credential fixture")
        .refresh = None;
    let flight = leader(request(&mut registry, &id, None));
    let result = match registry.try_start(&flight) {
        StartState::Completed(event) => event.result,
        other => panic!("expected terminal state, got {other:?}"),
    };
    assert!(
        matches!(result.as_ref(), Err(error) if error.code == "KELD-AUTH-016" && error.message.contains("no refresh credential"))
    );
    assert!(matches!(registry.try_start(&flight), StartState::Retired));
    assert!(registry.issuer_for(&id).is_some());
}

#[test]
fn closed_identity_counter_and_private_debug_are_literal_and_redacted() {
    let mut registry = SessionRegistry::new();
    assert!(matches!(
        registry.register_completed_begin("reserved-other", [1; 32], None, None, None, vec![]),
        Err(StateFault::UnknownProfile)
    ));
    assert_eq!(registry.next_account, 0);
    let id = account(&mut registry, 15);
    let flight = leader(request(
        &mut registry,
        &id,
        Some(scopes(&["scope-private"])),
    ));
    let input = start(&mut registry, &flight);
    let value = token(300);
    let debug = format!(
        "{registry:?} {flight:?} {input:?} {value:?} {:?}",
        value.response()
    );
    for private in [
        id.as_str(),
        "refresh-original",
        "scope-private",
        "access-private",
    ] {
        assert!(!debug.contains(private));
    }
    let exhausted = SessionRegistry {
        next_flight: u64::MAX,
        ..registry
    };
    let mut exhausted = exhausted;
    assert!(matches!(
        exhausted.token_request(&id, Some(scopes(&["new"])), || -> Option<()> {
            panic!("counter error must precede lease admission")
        }),
        Err(StateFault::CounterExhausted)
    ));
}

#[test]
fn cache_matches_only_canonical_account_and_scope_key() {
    let mut registry = SessionRegistry::new();
    let a = account(&mut registry, 16);
    let b = account(&mut registry, 17);
    let flight = leader(request(&mut registry, &a, Some(scopes(&["b", "a", "a"]))));
    start(&mut registry, &flight);
    let completed = registry
        .finish_success(&flight, token(300), None)
        .expect("cache publication");
    let equivalent = registry
        .token_request(&a, Some(scopes(&["a", "b"])), || -> Option<()> {
            panic!("equivalent cache cannot admit")
        })
        .expect("cache equality");
    match equivalent {
        TokenPlan::Ready(result) => assert!(Arc::ptr_eq(&result, &completed.result)),
        other => panic!("expected cache, got {other:?}"),
    }
    assert!(
        matches!(
            request(&mut registry, &a, Some(scopes(&["a"]))),
            TokenPlan::Leader { .. }
        ),
        "subset must not hit cache"
    );
    assert!(
        matches!(
            request(&mut registry, &a, Some(scopes(&["A", "b"]))),
            TokenPlan::Leader { .. }
        ),
        "case change must not hit cache"
    );
    assert!(
        matches!(
            request(&mut registry, &b, Some(scopes(&["a", "b"]))),
            TokenPlan::Leader { .. }
        ),
        "another account must not hit cache"
    );
}

#[test]
fn local_and_exchange_fault_wire_mapping_is_literal_static_and_redacted() {
    let cases = [
        (StateFault::Stopping, "KELD-AUTH-016"),
        (StateFault::CounterExhausted, "KELD-AUTH-016"),
        (StateFault::UnknownProfile, "KELD-AUTH-016"),
        (StateFault::InvalidReceiptClock, "KELD-AUTH-016"),
        (StateFault::ExpiryOverflow, "KELD-AUTH-016"),
        (StateFault::EmptyCredential, "KELD-AUTH-013"),
        (StateFault::ExpiredAccess, "KELD-AUTH-016"),
        (StateFault::MissingRefreshCredential, "KELD-AUTH-016"),
    ];
    for (error, code) in cases {
        let wire = error.wire_error();
        assert_eq!(
            error.code(),
            code,
            "typed code owner must match literal contract"
        );
        assert_eq!(wire.code, code, "wrong producer condition for {error:?}");
        assert!(wire.message.starts_with(&format!("{code}: ")));
        assert_eq!(
            wire.message,
            error.to_string(),
            "wire must retain full typed Display"
        );
        let fix = match error {
            StateFault::Stopping => "restart the host",
            StateFault::CounterExhausted => "restore local identity capacity",
            StateFault::UnknownProfile => "Correct host completed-flow/profile binding",
            StateFault::InvalidReceiptClock => "Restore a valid host receipt clock",
            StateFault::ExpiryOverflow => "Correct token expiry input or the host receipt clock",
            StateFault::EmptyCredential => "Resolve the token exchange response failure",
            StateFault::ExpiredAccess => "Request a fresh token explicitly",
            StateFault::MissingRefreshCredential => "Start a fresh user-initiated auth.begin",
        };
        assert!(
            wire.message.contains(fix),
            "wrong corrective guidance for {error:?}"
        );
        assert!(wire.message.contains("do not automatically retry"));
        for private in [
            "account-private",
            "issuer-private",
            "scope-private",
            "refresh-private",
            "access-private",
        ] {
            assert!(!wire.message.contains(private));
        }
    }
}

#[test]
fn unresolved_lookup_and_plan_do_not_construct_local_auth_error_or_admit() {
    let mut registry = SessionRegistry::new();
    assert_eq!(registry.issuer_for("account-private"), None);
    assert!(matches!(
        registry
            .token_request("account-private", None, || -> Option<()> {
                panic!("unresolved account must not admit any exchange")
            })
            .expect("pure unresolved state, no error"),
        TokenPlan::Unresolved
    ));
    assert!(registry.accounts.is_empty());
}

#[test]
fn missing_selector_stays_unresolved_after_host_tail_without_auth_error_or_admission() {
    let mut registry = SessionRegistry::new();
    let known = account(&mut registry, 18);
    registry.stop_for_host_tail(|_| panic!("no pending flight to complete"));
    assert_eq!(registry.issuer_for("account-private-missing"), None);
    let absent = registry.token_request("account-private-missing", None, || -> Option<()> {
        panic!("missing selector during tail must not admit")
    });
    assert!(
        matches!(absent, Ok(TokenPlan::Unresolved)),
        "missing selector became an auth error: {absent:?}"
    );
    let present = registry.token_request(&known, None, || -> Option<()> {
        panic!("known stopped account must not admit")
    });
    assert!(
        matches!(present, Err(StateFault::Stopping)),
        "known tail refusal changed: {present:?}"
    );
    assert_eq!(
        registry.issuer_for(&known),
        Some("https://login.microsoftonline.com/common")
    );
    assert!(
        registry
            .accounts
            .get(&known)
            .expect("account retained through tail")
            .flights
            .is_empty()
    );
}
