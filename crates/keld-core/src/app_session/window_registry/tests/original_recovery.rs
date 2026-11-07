//! Real graph/source admission and native lease state machine; modeled callbacks.
//! Native OS recovery and renderer effects remain the root's installed-app gate.

use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::Arc;

use super::super::{
    CreateOutcome, OriginalLoadAdmission, PendingWindowCall, WindowOwner, WindowRegistry,
};
use super::{TestResult, owner, ready, snapshot};
use crate::app_session::tests::gh499_graph::original_selection_with_versions;
use keld_ipc::CorrelationId;
use keld_ipc::window::WindowRef;
use keld_wv::wkwebview::MacNativeDocumentFixture;
use keld_wv::{NativeNavigationContext, WebviewId};

impl WindowRegistry {
    /// Exercise the real source/preload/create/load owners with modeled native
    /// callbacks. None/true/false means absent/before-commit/after-commit registration.
    pub(in crate::app_session) fn fixture_original_document(
        &mut self,
        who: WindowOwner,
        view: WebviewId,
        registration: Option<bool>,
        finished: bool,
    ) -> TestResult<(
        tempfile::TempDir,
        MacNativeDocumentFixture,
        WindowRef,
        NativeNavigationContext,
    )> {
        let (owned, selection) = original_selection_with_versions(BTreeMap::from([
            ("keld".to_owned(), "0.0.1-fixture".to_owned()),
            ("bun".to_owned(), "1.3.1-fixture".to_owned()),
        ]));
        let preload = selection.prepare_preload(
            owned
                .path()
                .join("app/src/main/electron-preload.js")
                .to_str()
                .ok_or("fixture path")?,
            Vec::new(),
        )?;
        let selection = Arc::new(selection);
        let native = MacNativeDocumentFixture::new(view, selection.source())?;
        let document = native.document();
        let stamp = document.stamp();
        let CreateOutcome::Build { build, window, .. } = self.create_with_original_preload(
            PendingWindowCall {
                owner: who,
                correlation: CorrelationId(1),
            },
            (640, 480),
            (None, None),
            selection,
            preload,
        )?
        else {
            return Err("fixture expected a new original window".into());
        };
        assert!(matches!(
            self.native_built(build, view, snapshot()),
            super::super::BuildOutcome::Created { .. }
        ));
        let OriginalLoadAdmission::Dispatch {
            operation, view, ..
        } = self.admit_original_load(who, CorrelationId(2), window)?
        else {
            return Err("fixture load was busy".into());
        };
        let context =
            NativeNavigationContext::new(NonZeroU64::new(operation).ok_or("operation")?, view);
        assert!(self.original_load_started(context, stamp.attempt()));
        if registration == Some(true) {
            self.note_renderer_registered(view, document.clone())?;
        }
        assert!(
            self.original_load_committed(context, stamp, document.clone(), native.native_url()?)
                .is_some()
        );
        if registration == Some(false) {
            self.note_renderer_registered(view, document)?;
        }
        if finished {
            assert!(self.original_load_finished(context, stamp).is_some());
        }
        Ok((owned, native, window, context))
    }
}

fn recovered_count(registry: &WindowRegistry, who: WindowOwner) -> TestResult<usize> {
    let mut count = 0;
    registry.for_each_recovered_document(who, |_, _, _| {
        count += 1;
        Ok(())
    })?;
    Ok(count)
}

#[test]
fn role_loss_retains_finished_physical_document_without_renewing_routes() -> TestResult {
    let mut registry = ready();
    let (_owned, native, window, context) =
        registry.fixture_original_document(owner(1), WebviewId(700), Some(false), true)?;
    let document = native.document();
    let stamp = document.stamp();
    let view = context.view();
    assert!(
        registry
            .renderer_route(owner(1), view, Some(stamp.observed_generation()))
            .is_ok()
    );
    assert_eq!(
        recovered_count(&registry, owner(1))?,
        0,
        "fresh initial document is not a recovery snapshot"
    );

    registry.role_lost(owner(1), |_| {});
    assert!(
        document.is_current(),
        "Bun role loss does not replace the physical page"
    );
    assert!(
        registry.renderer_route(owner(1), view, None).is_err(),
        "G1 is retired"
    );
    let retained = registry.entries[&window.id.0].renderer.document.as_ref();
    assert!(
        retained.is_some_and(|retained| retained.same_binding(&document)),
        "role loss must retain the finished physical observation independently of G1 routing"
    );
    registry.subscribe_successor(owner(2));
    assert!(
        registry.renderer_route(owner(2), view, None).is_err(),
        "Created-only adoption cannot grant routing"
    );
    assert_eq!(
        registry.replay(owner(2)).count(),
        1,
        "window replay stays Created-only"
    );
    assert_eq!(recovered_count(&registry, owner(2))?, 1);
    registry.rebind_recovered_renderer_documents(owner(2));
    let rebound = registry.renderer_route(owner(2), view, Some(stamp.observed_generation()))?;
    assert!(
        rebound.document.same_binding(&document),
        "fresh role, same physical document"
    );
    assert!(registry.renderer_route(owner(1), view, None).is_err());
    registry.role_lost(owner(1), |_| {});
    assert!(
        registry.renderer_route(owner(2), view, None).is_ok(),
        "late G1 cleanup cannot retire G2"
    );
    Ok(())
}

#[test]
fn initial_registration_on_either_side_of_commit_routes_before_finish() -> TestResult {
    for register_first in [true, false] {
        let mut registry = ready();
        let (_owned, native, _, context) = registry.fixture_original_document(
            owner(1),
            WebviewId(700),
            Some(register_first),
            false,
        )?;
        assert!(
            registry
                .renderer_route(owner(1), context.view(), None)
                .is_ok(),
            "ordinary startup must not wait for finish"
        );
        assert!(
            registry
                .original_load_finished(context, native.document().stamp())
                .is_some()
        );
    }
    Ok(())
}

#[test]
fn recovery_refuses_incomplete_or_pending_effects_before_retirement_normalizes_state() -> TestResult
{
    for case in [
        "unfinished",
        "unregistered",
        "pending-load",
        "pending-destroy",
        "pending-close",
        "revoked",
    ] {
        let mut registry = ready();
        let registration = (case != "unregistered").then_some(false);
        let (_owned, mut native, window, context) = registry.fixture_original_document(
            owner(1),
            WebviewId(700),
            registration,
            case != "unfinished",
        )?;
        match case {
            "pending-load" => {
                registry.admit_original_load(owner(1), CorrelationId(3), window)?;
            }
            "pending-destroy" => {
                registry.reserve_original_document_destroy(
                    owner(1),
                    CorrelationId(3),
                    window,
                    Arc::new(keld_wv::wkwebview::OriginalDocumentDestroyAdmission::new()),
                )?;
                registry.cancel_native_admissions(owner(1), |_| {});
            }
            "pending-close" => {
                registry.request_close(owner(1), window)?;
            }
            "revoked" => native.revoke(),
            _ => {}
        }
        registry.role_lost(owner(1), |_| {});
        registry.subscribe_successor(owner(2));
        assert_eq!(recovered_count(&registry, owner(2))?, 0, "{case}");
        registry.rebind_recovered_renderer_documents(owner(2));
        assert!(
            registry
                .renderer_route(owner(2), context.view(), None)
                .is_err(),
            "{case}"
        );
        assert!(
            registry
                .note_renderer_registered(context.view(), native.document())
                .is_err(),
            "late registration cannot repair {case}"
        );
        assert!(
            registry
                .original_load_finished(context, native.document().stamp())
                .is_none(),
            "late finish cannot repair {case}"
        );
    }
    Ok(())
}

#[test]
fn delayed_registration_is_physical_only_until_explicit_successor_binding() -> TestResult {
    let mut registry = ready();
    let (_owned, native, _, context) =
        registry.fixture_original_document(owner(1), WebviewId(700), Some(false), true)?;
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    registry.note_renderer_registered(context.view(), native.document())?;
    assert!(
        registry
            .renderer_route(owner(2), context.view(), None)
            .is_err()
    );
    let mut routes = 0;
    registry.for_each_registered_renderer_route(owner(2), |_| {
        routes += 1;
        Ok(())
    })?;
    assert_eq!(routes, 0, "enumeration cannot bypass the owner binding");
    registry.rebind_recovered_renderer_documents(owner(2));
    assert!(
        registry
            .renderer_route(owner(2), context.view(), None)
            .is_ok()
    );
    Ok(())
}

#[test]
fn native_revocation_between_snapshot_and_rebind_or_after_binding_is_effective() -> TestResult {
    for before_rebind in [true, false] {
        let mut registry = ready();
        let (_owned, mut native, _, context) =
            registry.fixture_original_document(owner(1), WebviewId(700), Some(false), true)?;
        registry.role_lost(owner(1), |_| {});
        registry.subscribe_successor(owner(2));
        assert_eq!(recovered_count(&registry, owner(2))?, 1);
        if before_rebind {
            native.revoke();
        }
        registry.rebind_recovered_renderer_documents(owner(2));
        if !before_rebind {
            native.revoke();
        }
        assert_eq!(recovered_count(&registry, owner(2))?, 0);
        assert!(
            registry
                .renderer_route(owner(2), context.view(), None)
                .is_err()
        );
        assert!(
            registry
                .admit_renderer_route(owner(2), context.view(), native.document())
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn late_old_commit_cannot_restore_a_binding_after_role_loss() -> TestResult {
    let mut registry = ready();
    let (_owned, native, _, context) =
        registry.fixture_original_document(owner(1), WebviewId(700), Some(false), false)?;
    registry.role_lost(owner(1), |_| {});
    for transfer in [false, true] {
        if transfer {
            registry.subscribe_successor(owner(2));
        }
        assert!(
            registry
                .original_load_committed(
                    context,
                    native.document().stamp(),
                    native.document(),
                    native.native_url()?
                )
                .is_none()
        );
        assert!(
            registry
                .original_load_finished(context, native.document().stamp())
                .is_none()
        );
        registry.rebind_recovered_renderer_documents(owner(2));
        assert!(
            registry
                .renderer_route(owner(2), context.view(), None)
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn recovery_rechecks_exact_admitted_source_not_matching_numeric_stamp() -> TestResult {
    let mut registry = ready();
    let (_owned, native, window, context) =
        registry.fixture_original_document(owner(1), WebviewId(700), Some(false), true)?;
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    let (_other_root, other) =
        original_selection_with_versions(BTreeMap::from([("keld".into(), "fixture".into())]));
    assert!(!native.document().belongs_to_source(other.source()));
    registry
        .entries
        .get_mut(&window.id.0)
        .ok_or("window")?
        .original_selection
        .as_mut()
        .ok_or("source")?
        .selection = Arc::new(other);
    assert_eq!(recovered_count(&registry, owner(2))?, 0);
    registry.rebind_recovered_renderer_documents(owner(2));
    assert!(
        registry
            .renderer_route(owner(2), context.view(), None)
            .is_err()
    );
    Ok(())
}
