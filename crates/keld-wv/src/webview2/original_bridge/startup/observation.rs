//! Passive scalar reads after actual startup transitions and native returns.
//! No observation enters the final fence, calls native code or changes a clock.
#![forbid(unsafe_code)]

use super::super::OriginalBridge;
use crate::renderer_contract::CheckedNativeDocumentLease;
use std::time::Instant;

#[derive(Clone, Copy)]
struct ReceiptFields {
    initial_entered: bool,
    dom_observed: bool,
    retry_entered: bool,
}

#[derive(Clone, Copy)]
struct BindingFields {
    current: bool,
    registered: bool,
}

#[derive(Clone, Copy)]
struct Snapshot {
    qualified_at: Instant,
    deadline: Instant,
    view: u32,
    generation: u32,
    navigation: u64,
    receipt: ReceiptFields,
    binding: BindingFields,
    faulted: bool,
    retired: bool,
    input_same_document: Option<bool>,
}
const _: () = assert!(std::mem::size_of::<Snapshot>() <= 128);

impl OriginalBridge {
    pub(in crate::webview2::original_bridge) fn observe_startup(
        &self,
        phase: &'static str,
        input: Option<&CheckedNativeDocumentLease>,
        native_navigation: Option<u64>,
    ) {
        if !crate::webview2::t2_test_observation::enabled() {
            return;
        }
        // Only copied scalars leave these guards. No input body, native value,
        // credential, clone of a lease or new retained observation queue exists.
        let observed = (|| {
            let nav = self.navigation.try_borrow().ok()?;
            let state = self.state.try_borrow().ok()?;
            let startup = state.startup.as_ref()?;
            let stamp = startup.document.0.stamp();
            Some(Snapshot {
                qualified_at: nav.qualified_at()?,
                deadline: startup.deadline,
                view: stamp.view.0,
                generation: stamp.generation.get(),
                navigation: stamp.native_commit.0.get(),
                receipt: ReceiptFields {
                    initial_entered: startup.initial_entered,
                    dom_observed: startup.dom_observed,
                    retry_entered: startup.retry_entered,
                },
                binding: BindingFields {
                    current: startup.document.belongs_to_owner(nav.document())
                        && state.binding.current(nav.document()) == Some(stamp),
                    registered: state.binding.registered(),
                },
                faulted: state.fault.is_faulted(),
                retired: self.retired.get(),
                input_same_document: input.map(|input| startup.document.same_binding(input)),
            })
        })();
        let Some(value) = observed else {
            eprintln!(
                "KELD_ORIGINAL_STARTUP {}",
                serde_json::json!({"phase":phase,"view":self.id.0,"unavailable":true})
            );
            return;
        };
        // Compare the actual stored Instants within this same process. Their
        // difference is from the real qualification field, never elapsed test time.
        let deadline_ns = value
            .deadline
            .checked_duration_since(value.qualified_at)
            .and_then(|duration| u64::try_from(duration.as_nanos()).ok());
        eprintln!(
            "KELD_ORIGINAL_STARTUP {}",
            serde_json::json!({
                "phase":phase,"view":value.view,"generation":value.generation,
                "navigation":value.navigation,"nativeNavigation":native_navigation,
                "qualifiedAt":format!("{:?}", value.qualified_at),
                "deadline":format!("{:?}", value.deadline),
                "deadlineAfterQualificationNs":deadline_ns,
                "initialEntered":value.receipt.initial_entered,"domObserved":value.receipt.dom_observed,
                "retryEntered":value.receipt.retry_entered,"current":value.binding.current,
                "registered":value.binding.registered,"faulted":value.faulted,
                "retired":value.retired,"inputSameDocument":value.input_same_document,
            })
        );
    }
}
