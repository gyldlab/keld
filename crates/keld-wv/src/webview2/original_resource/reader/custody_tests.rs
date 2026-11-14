use super::*;
use crate::WebviewId;
use crate::resource_intake::{ORIGINAL_WEBAPP_LIMITS, RetainedByteCredit, TaskLedger};
use crate::resource_source::windows::tests::Fixture;
use crate::webview2::original_resource::reader::{Work, read_work};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct HeldBody {
    _fixture: Fixture,
    account: Arc<AtomicUsize>,
    before: usize,
    charge: usize,
    completed: Option<Completed>,
}

impl HeldBody {
    fn new() -> Self {
        let fixture = Fixture::new();
        let source = fixture.source_arc();
        let selected = source
            .qualify_url(&fixture.url("views/editor.html"))
            .unwrap();
        let account = source.retained_credit_account();
        let before = account.load(Ordering::Acquire);
        let charge = usize::try_from(selected.size).unwrap();
        let credit = RetainedByteCredit::reserve(&account, charge).unwrap();
        let mut ledger = TaskLedger::new(ORIGINAL_WEBAPP_LIMITS);
        let ticket = ledger.begin(1).unwrap();
        let completed = read_work(
            Work {
                ticket,
                view: WebviewId(1),
                source,
                selected,
                cancelled: Arc::new(AtomicBool::new(false)),
                credit,
            },
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            None,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            &AtomicBool::new(false),
        );
        assert!(
            completed.result.is_ok(),
            "real retained-file reader succeeded"
        );
        assert_eq!(account.load(Ordering::Acquire), before + charge);
        Self {
            _fixture: fixture,
            account,
            before,
            charge,
            completed: Some(completed),
        }
    }

    fn assert_held(&self) {
        assert_eq!(
            self.account.load(Ordering::Acquire),
            self.before + self.charge,
            "the same completed body remains charged"
        );
    }
}

fn result_slot(slot: &RefCell<ReaderCustody>) -> Arc<Mutex<Option<Completed>>> {
    match &slot.borrow().phase {
        Phase::Retained(reader) => Arc::clone(&reader.result),
        Phase::NativeEntry { result, .. } => Arc::clone(result),
    }
}

fn install_body(slot: &RefCell<ReaderCustody>, held: &mut HeldBody) {
    let mut custody = slot.borrow_mut();
    let Phase::Retained(reader) = &mut custody.phase else {
        panic!("fixture must begin with its one retained reader");
    };
    let completed = held.completed.take().unwrap();
    reader.active = Some(completed.ticket);
    *reader.result.lock().unwrap() = Some(completed);
}

fn assert_unproved(slot: &RefCell<ReaderCustody>) {
    let custody = slot.borrow();
    let Phase::Retained(reader) = &custody.phase else {
        panic!("the exact reader must be restored before return");
    };
    assert!(reader.unproved);
    assert!(reader.shutdown);
    assert!(!custody.is_drained());
}

#[test]
fn native_entry_has_no_refcell_borrow_and_restores_same_result_owner() {
    let slot = RefCell::new(ReaderCustody::new());
    // No thread was ever started, so retirement genuinely needs no native wait.
    slot.borrow_mut().retire().unwrap();
    assert!(slot.borrow().is_drained());
    let original = result_slot(&slot);
    with_reader_on_ui(&slot, |reader| {
        let observed = slot.try_borrow().expect("no Rust borrow across native cut");
        let Phase::NativeEntry { result, .. } = &observed.phase else {
            panic!("native entry must be an explicit non-drained phase");
        };
        assert!(Arc::ptr_eq(&original, result));
        assert!(Arc::ptr_eq(&original, &reader.result));
        assert!(!observed.is_drained());
        Ok(())
    })
    .unwrap();
    assert!(Arc::ptr_eq(&original, &result_slot(&slot)));
    assert!(slot.borrow().is_drained());
}

#[test]
fn reentrant_operation_latches_failure_and_cannot_replace_the_reader() {
    let slot = RefCell::new(ReaderCustody::new());
    let original = result_slot(&slot);
    let entered = std::cell::Cell::new(false);
    let result = with_reader_on_ui(&slot, |_| {
        assert!(
            with_reader_on_ui(&slot, |_| {
                entered.set(true);
                Ok(())
            })
            .is_err()
        );
        Ok(())
    });
    assert!(
        result.is_err(),
        "outer success cannot erase an overlap failure"
    );
    assert!(!entered.get(), "no replacement or second native operation");
    assert!(Arc::ptr_eq(&original, &result_slot(&slot)));
    assert_unproved(&slot);
}

#[test]
fn panic_restores_original_completed_buffer_and_retained_credit() {
    let mut held = HeldBody::new();
    let slot = RefCell::new(ReaderCustody::new());
    install_body(&slot, &mut held);
    let original = result_slot(&slot);
    assert!(with_reader_on_ui(&slot, |_| panic!("controlled native-operation unwind")).is_err());
    assert_unproved(&slot);
    assert!(Arc::ptr_eq(&original, &result_slot(&slot)));
    assert!(original.lock().unwrap().is_some());
    held.assert_held();
    assert!(slot.borrow_mut().take_result().is_err());
    held.assert_held();
    // This fixture starts no native work. Only this explicit test cleanup
    // consumes its proved synchronous result; product errors retain it.
    drop(original.lock().unwrap().take());
    assert_eq!(held.account.load(Ordering::Acquire), held.before);
}

#[test]
fn operation_error_restores_the_same_completed_buffer_without_drain_credit() {
    let mut held = HeldBody::new();
    let slot = RefCell::new(ReaderCustody::new());
    install_body(&slot, &mut held);
    let original = result_slot(&slot);
    assert!(with_reader_on_ui(&slot, |_| Err(failure("controlled returned failure"))).is_err());
    assert_unproved(&slot);
    assert!(Arc::ptr_eq(&original, &result_slot(&slot)));
    held.assert_held();
    assert!(slot.borrow_mut().take_result().is_err());
    drop(original.lock().unwrap().take());
    assert_eq!(held.account.load(Ordering::Acquire), held.before);
}

#[test]
fn overlapping_unproved_result_uses_the_existing_slot_and_survives_restoration() {
    let mut held = HeldBody::new();
    let slot = RefCell::new(ReaderCustody::new());
    let original = result_slot(&slot);
    assert!(
        with_reader_on_ui(&slot, |_| {
            // The body stays outside the fallible capture until this exact existing
            // slot accepts it; no replacement result queue or native proof.
            slot.borrow_mut()
                .retain_unproved(held.completed.take().unwrap());
            held.assert_held();
            assert!(original.lock().unwrap().is_some());
            Ok(())
        })
        .is_err()
    );
    assert!(held.completed.is_none());
    assert_unproved(&slot);
    assert!(Arc::ptr_eq(&original, &result_slot(&slot)));
    held.assert_held();
    drop(original.lock().unwrap().take());
    assert_eq!(held.account.load(Ordering::Acquire), held.before);
}
