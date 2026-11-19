//! Real retained source metadata, without simulating a native commit or finish.
use super::*;
use crate::app_session::{
    PrimaryLinkIdentity,
    graph_contract::tests::retained_original_window_resources,
    window_registry::{BuildOutcome, CreateOutcome, OriginalWindowResources},
};
use keld_guard::{AdmittedRootUrl, RootRelativeComponents};
use keld_ipc::window::{Rect, WindowState};
use keld_wv::WebviewId;
use std::sync::{Arc, atomic::AtomicBool};

#[test]
fn original_load_requires_exact_view_and_retains_q22_refusal_until_publication() {
    let (stage, source, preload) = retained_original_window_resources();
    let directory = stage
        .path()
        .join("copied-app/client/assets")
        .canonicalize()
        .expect("retained qualified root exists");
    let root = AdmittedRootUrl::from_normalized_drive_path(&directory).expect("Windows root");
    let entry = RootRelativeComponents::from_graph_path("nested/editor.html").expect("exact entry");
    let uri = root
        .url_for(&entry, Some("raw=%252f"))
        .expect("selected URL");
    let asset = RootRelativeComponents::from_graph_path("styles.css").expect("qualified asset");
    let non_entry = root
        .url_for(&asset, None)
        .expect("existing non-document URL");
    let caller = PrimaryCallerContext {
        identity: PrimaryLinkIdentity::Fixture,
        attempt: 1,
        reader_stop: Arc::new(AtomicBool::new(false)),
    };
    let owner = WindowOwner::new(caller.identity, caller.attempt);
    let mut registry = WindowRegistry::new();
    assert!(registry.mark_ready());
    let mut create = |view| {
        let CreateOutcome::Build { build, window, .. } = registry
            .create_original_for_caller(
                &caller,
                CorrelationId(view),
                (640, 480),
                (None, None),
                OriginalWindowResources {
                    source: Arc::clone(&source),
                    preload: Arc::clone(&preload),
                },
            )
            .expect("qualified original resources")
        else {
            panic!("distinct window expected")
        };
        let state = WindowState {
            bounds: Rect {
                x: 0,
                y: 0,
                width: 640,
                height: 480,
            },
            focused: false,
            maximized: false,
            full_screen: false,
        };
        assert!(matches!(
            registry.native_built(build, WebviewId(view), state),
            BuildOutcome::Created { .. }
        ));
        window
    };
    let a = create(700);
    let b = create(701);
    let correlation = CorrelationId(17);
    assert!(matches!(
        registry.prepare_original_load(&caller, correlation, a, &non_entry),
        Err(OriginalLoadRefusal::Uri(_))
    ));
    assert!(!registry.has_pending_call(owner, correlation));
    let admitted = registry
        .prepare_original_load(&caller, correlation, a, &uri)
        .expect("only selected entry admits");
    assert_eq!(admitted.context.operation(), NonZeroU64::MIN);
    assert_eq!(admitted.context.view(), WebviewId(700));
    assert_eq!(admitted.query.as_optional_str(), Ok(Some("raw=%252f")));
    assert!(registry.original_load_can_enter_native(&caller, a, admitted.context));
    assert!(!registry.original_load_can_enter_native(&caller, b, admitted.context));
    let foreign = NativeNavigationContext::new(admitted.context.operation(), WebviewId(701));
    assert!(!registry.original_load_can_enter_native(&caller, a, foreign));
    assert!(registry.next_original_navigation(&caller).is_none());
    assert!(matches!(
        registry.prepare_original_load(&caller, CorrelationId(18), a, &uri),
        Err(OriginalLoadRefusal::Registry(WindowError::OriginalLoadBusy))
    ));
    let refusal = OriginalNavigationFact::Refused {
        error: NativeNavigationRefusal::FileHeadersUnproven,
    };
    assert!(!registry.capture_original_navigation(&caller, foreign, refusal));
    assert!(registry.has_pending_call(owner, correlation));
    assert!(registry.capture_original_navigation(
        &caller,
        admitted.context,
        OriginalNavigationFact::Refused {
            error: NativeNavigationRefusal::FileHeadersUnproven
        },
    ));
    assert!(!registry.original_load_can_enter_native(&caller, a, admitted.context));
    let publication = registry
        .next_original_navigation(&caller)
        .expect("original Q22 refusal");
    assert_eq!(publication.correlation, correlation);
    assert!(matches!(
        publication.kind,
        NavigationPublicationKind::Error(NavigationTerminal::Refused(
            NativeNavigationRefusal::FileHeadersUnproven
        ))
    ));
    assert!(registry.has_pending_call(owner, correlation));
    registry.commit_navigation_publication(publication);
    assert!(!registry.has_pending_call(owner, correlation));
    assert!(registry.next_original_navigation(&caller).is_none());

    let next = registry
        .prepare_original_load(&caller, correlation, a, &uri)
        .expect("fresh call");
    assert_ne!(next.context.operation(), admitted.context.operation());
    registry.role_lost(owner, |_| {});
    assert!(!registry.has_pending_call(owner, correlation));
    assert!(!registry.original_load_can_enter_native(&caller, a, next.context));
}
