//! Independent real-SDK single-flight scenario, wire oracles and cache receipt.
use super::super::super::{AuthResponse, OperationId, Progress, ReadyResponse};
use super::super::seed_account;
use super::{SingleflightCase, SingleflightRequest};
use crate::auth::macos_http;
use std::time::Instant;

fn singleflight_token(response: ReadyResponse) -> (String, u64, String) {
    let AuthResponse::Token {
        access_token,
        expires_at,
        token_type,
    } = response.into_response().expect("actual decoded SDK token")
    else {
        panic!("unexpected Begin DTO");
    };
    assert!(
        access_token == "SDK_SINGLEFLIGHT_ACTUAL_TOKEN",
        "actual endpoint token"
    );
    assert_eq!(token_type, "Bearer");
    (access_token, expires_at, token_type)
}

fn singleflight_hold(case: &mut SingleflightCase, operation: &OperationId) {
    assert_eq!(case.poll(operation), Progress::Pending);
    let post = loop {
        match case.peer.as_ref().expect("owned peer").post.try_recv() {
            Ok(body) => break body,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                panic!("actual peer lost before POST")
            }
        }
        assert_eq!(case.poll(operation), Progress::Pending);
        assert!(Instant::now() < case.deadline, "actual POST bound");
        std::thread::yield_now();
    };
    assert!(post.as_slice() == b"grant_type=refresh_token&client_id=literal-client&refresh_token=refresh-private&scope=scope.b+scope.a+scope.b", "literal owning form bytes and preserved leader scope vector");
    assert!(
        case.events.is_empty(),
        "held real peer cannot produce a token"
    );
    assert!(
        case.operations[0]
            .custody
            .as_ref()
            .is_some_and(|custody| !custody.released()),
        "real HTTP ownership while held"
    );
}

fn singleflight_complete(case: &mut SingleflightCase, operation: &OperationId) {
    case.peer.as_ref().expect("owned peer").release(false);
    loop {
        match case.poll(operation) {
            Progress::Pending => {}
            Progress::Drained => break,
            Progress::HostFault(_) => panic!("actual HTTP cleanup must be proved"),
        }
        assert!(Instant::now() < case.deadline, "same SDK completion bound");
        std::thread::yield_now();
    }
    assert!(
        case.operations[0]
            .custody
            .as_ref()
            .is_some_and(macos_http::TestHttpOwner::released)
    );
    assert_eq!(
        case.admissions.drops.get(),
        0,
        "caller lease retained through Drained"
    );
    case.release_drained_leases()
        .expect("actual physical release before lease drop");
    assert_eq!(case.admissions.drops.get(), 1);
}

pub(in crate::auth::controller::tests) fn run_sdk_case() {
    let mut case = SingleflightCase::start();
    let account = seed_account(case.owner.as_mut().expect("owned broker"));
    let leader = case.request(
        &account,
        vec![
            "scope.b".to_owned(),
            "scope.a".to_owned(),
            "scope.b".to_owned(),
        ],
    );
    let SingleflightRequest::Pending {
        operation,
        admitted: true,
    } = leader
    else {
        panic!("real guarded leader admission");
    };
    singleflight_hold(&mut case, &operation);
    let follower = case.request(&account, vec!["scope.a".to_owned(), "scope.b".to_owned()]);
    let coalesced = matches!(follower, SingleflightRequest::Pending { operation: same, admitted: false } if same == operation);
    if !coalesced {
        case.cleanup(true)
            .expect("wrong-scope owner cleanup must be proved");
        assert!(coalesced, "same actual in-flight canonical scope set");
    }
    assert_eq!(case.admissions.count.get(), 1);
    assert_eq!(case.admissions.drops.get(), 0);
    singleflight_complete(&mut case, &operation);
    assert!(case.observation_fault.is_none());
    assert_eq!(case.events.len(), 1, "one canonical token event");
    let event = case.events.pop().expect("actual SDK canonical event");
    assert_eq!(event.operation, operation);
    let actual = singleflight_token(event.response);
    let cached = match case.request(
        &account,
        vec![
            "scope.b".to_owned(),
            "scope.a".to_owned(),
            "scope.a".to_owned(),
        ],
    ) {
        SingleflightRequest::Ready(response) => singleflight_token(response),
        _ => panic!("healthy same-scope cache follow-up"),
    };
    assert!(
        actual == cached,
        "same SDK token and original receipt expiry"
    );
    assert_eq!(case.admissions.count.get(), 1);
    assert_eq!(case.admissions.drops.get(), 1);
    assert_eq!(
        case.cleanup(false).expect("normal owner cleanup proved"),
        1,
        "exactly one independently observed POST"
    );
}
