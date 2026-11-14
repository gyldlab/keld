//! The same ledger distinguishes physical storage from retained protocol work.
use super::*;
use crate::app_session::PrimaryLinkIdentity;
use std::sync::atomic::AtomicBool;

fn caller() -> PrimaryCallerContext {
    PrimaryCallerContext {
        attempt: 1,
        identity: PrimaryLinkIdentity::Fixture,
        reader_stop: Arc::new(AtomicBool::new(false)),
    }
}
fn key() -> NativeRouteKey {
    NativeRouteKey {
        webview: 1,
        navigation: 1,
        message: 7,
    }
}

#[test]
fn native_storage_release_is_not_a_consumer_or_protocol_receipt() {
    let mut ledger = CompatLedger::new();
    let caller = caller();
    ledger
        .reserve(Direction::MainToRenderer, key(), &caller, 16)
        .unwrap();
    assert!(
        ledger.native_storage_released_in(None),
        "no native token is claimed by this pure record fixture"
    );
    assert!(!ledger.endpoint_obligations_quiescent().unwrap());
    ledger.role_retired(&caller);
    ledger.native_quiesced(Direction::MainToRenderer, key());
    ledger.framework_quiesced_after_drain(Direction::MainToRenderer, key(), &caller);
    assert!(ledger.occupied(Direction::MainToRenderer));
    assert!(
        !ledger.endpoint_obligations_quiescent().unwrap(),
        "ring drain cannot finish native consumer work"
    );
    ledger.consumer_released(Direction::MainToRenderer, key(), &caller);
    assert!(ledger.endpoint_obligations_quiescent().unwrap());
}

#[test]
fn an_unreaped_actual_control_receipt_uses_the_same_release_predicate() {
    let mut ledger = CompatLedger::new();
    let caller = caller();
    ledger.reserve_control(&caller, 3, 16).unwrap();
    assert!(!ledger.endpoint_obligations_quiescent().unwrap());
    ledger.release_control(&caller, 3);
    assert!(ledger.endpoint_obligations_quiescent().unwrap());
    assert_eq!(ledger.controls_in_flight(), 0);
}

#[test]
fn an_old_role_record_cannot_be_hidden_by_a_same_number_successor() {
    let mut ledger = CompatLedger::new();
    let original = caller();
    let successor = caller();
    ledger
        .reserve(Direction::RendererToMain, key(), &original, 16)
        .unwrap();
    ledger.role_retired(&original);
    ledger.native_quiesced(Direction::RendererToMain, key());
    ledger.framework_quiesced_after_drain(Direction::RendererToMain, key(), &successor);
    assert!(!ledger.endpoint_obligations_quiescent().unwrap());
    ledger.framework_quiesced_after_drain(Direction::RendererToMain, key(), &original);
    assert!(ledger.endpoint_obligations_quiescent().unwrap());
}
