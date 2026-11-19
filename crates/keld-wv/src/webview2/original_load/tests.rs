//! The version verdict covers only Q22(a,b). Native fixture admission never navigates.
use super::*;

#[test]
fn only_the_measured_x64_runtime_qualifies_both_header_surfaces() {
    let unproved = Err(NativeNavigationRefusal::FileHeadersUnproven);
    for observed in [
        None,
        Some(""),
        Some("157.0.4341.46"),
        Some("157.0.4341.61"),
        Some("157.0.4341.60 beta"),
        Some("157.0.4341.60 "),
        Some("157.0.4341.060"),
    ] {
        assert_eq!(require_q22_ab(observed), unproved, "{observed:?}");
    }
    assert_eq!(
        require_q22_ab(Some("157.0.4341.60")),
        if cfg!(target_arch = "x86_64") {
            Ok(())
        } else {
            unproved
        }
    );
}

#[cfg(feature = "media-acceptance")]
pub(in crate::webview2) fn captured_runtime_and_view_binding(
    engine: &mut crate::webview2::WebView2Engine,
    spec: &crate::WebviewSpec,
) -> Result<(), crate::WvError> {
    use crate::WebEngine as _;
    use crate::engine::NativeNavigationContext;
    use crate::native_document::NativeCommitIdentity;
    use crate::renderer_bridge::RendererRoleWitness;
    use crate::resource_source::windows::tests::Fixture;
    use crate::webview2::original_resource::ResourceSession;
    use std::cell::RefCell;
    use std::num::NonZeroU64;
    use std::sync::{Arc, atomic::AtomicBool};
    use std::time::Instant;

    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let a = engine.create(spec)?;
    let b = engine.create(spec)?;
    let session = ResourceSession::new(
        Arc::clone(&source),
        engine.creation_event_loop()?.create_proxy(),
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        None,
    )?;
    let bind = |id| {
        let view = engine.view(id)?;
        let navigation = OriginalNavigation::new(id, Arc::clone(&source))
            .map_err(|error| crate::WvError::EventLoop(error.to_string()))?;
        ResourceView::bind(
            &view.webview,
            &engine.environment,
            Rc::new(RefCell::new(navigation)),
            Rc::clone(&session),
            id,
            Rc::clone(&view.close_custody),
        )
    };
    let owner_a = bind(a)?;
    let owner_b = bind(b)?;
    owner_a.install_on_ui()?;
    owner_b.install_on_ui()?;
    let check = || -> Result<(), NativeNavigationRefusal> {
        let (_, env_a, nav_a) = owner_a.original_load_target();
        let (_, env_b, _) = owner_b.original_load_target();
        assert_eq!(env_a, env_b, "two real views in one native environment");
        // Model already-committed A on the real retained owner; no native A commit is claimed.
        let old = {
            let mut nav = nav_a.borrow_mut();
            let prior = nav
                .document_mut()
                .begin_host_load(source.source_id())
                .unwrap();
            let stamp = nav
                .document_mut()
                .commit_admitted(
                    prior,
                    source.source_id(),
                    NativeCommitIdentity(NonZeroU64::MIN),
                )
                .unwrap();
            nav.document().lease_for(stamp).unwrap()
        };
        let query = OriginalEntryQuery::from_optional_query(None).unwrap();
        let context = NativeNavigationContext::new(NonZeroU64::MIN, a);
        let role = RendererRoleWitness::new(Arc::new(AtomicBool::new(false)));
        for observed in [None, Some("157.0.4341.61")] {
            let prepared = nav_a.borrow().prepare(&query)?;
            let result = QualifiedHostLoad::from_version_observation(&owner_a, prepared, observed)
                .and_then(|proof| {
                    nav_a
                        .borrow_mut()
                        .begin_host_load(proof, context, role.clone(), Instant::now())
                });
            assert_eq!(result, Err(NativeNavigationRefusal::FileHeadersUnproven));
            assert!(old.is_current());
            assert_eq!(
                nav_a.borrow().current_document().unwrap().stamp(),
                crate::renderer_contract::NativeDocumentStamp(old.stamp())
            );
        }

        let (proof_b, call_b) = PreparedNativeLoad::prepare(&owner_b, &query)?.into_parts();
        assert_eq!(
            nav_a
                .borrow_mut()
                .begin_host_load(proof_b, context, role, Instant::now()),
            Err(NativeNavigationRefusal::Unavailable)
        );
        assert!(call_b.native_quiesced());
        assert!(
            old.is_current(),
            "B's qualified environment cannot revoke A"
        );

        let (proof_a, call_a) = PreparedNativeLoad::prepare(&owner_a, &query)?.into_parts();
        assert!(proof_a.into_prepared_for(&nav_a.borrow())?.is_prepared());
        assert!(call_a.native_quiesced());
        assert!(old.is_current(), "preparation alone does not revoke A");
        Ok(())
    };
    let result = check().map_err(|error| crate::WvError::EventLoop(error.to_string()));
    for (owner, id) in [(owner_b, b), (owner_a, a)] {
        owner.retire_on_ui()?;
        assert!(engine.view(id)?.close_custody.close_on_ui());
        owner.controller_closed_on_ui();
        assert!(owner.is_drained());
    }
    result
}
