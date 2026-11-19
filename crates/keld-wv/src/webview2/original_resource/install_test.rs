//! Real resource installation with two participants and a separate release witness.
use super::*;
use crate::WebEngine as _;
use crate::resource_source::windows::tests::Fixture;
use crate::webview2::WebView2Engine;

pub(in crate::webview2) fn reentrant_retirement(
    engine: &mut WebView2Engine,
    spec: &crate::WebviewSpec,
) -> Result<(), WvError> {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let first = engine.create(spec)?;
    let second = engine.create(spec)?;
    // The fixture closes both resource participants explicitly. Leave a
    // same-environment controller for the ordinary browser release witness.
    let release_witness = engine.create(spec)?;
    let proxy = engine.creation_event_loop()?.create_proxy();
    let session = ResourceSession::new(
        Arc::clone(&source),
        proxy,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        None,
    )?;
    let bind = |id| -> Result<Rc<ResourceView>, WvError> {
        let native = engine.view(id)?;
        let navigation = OriginalNavigation::new(id, Arc::clone(&source))
            .map_err(|_| failure("fixture source rejected"))?;
        ResourceView::bind(
            &native.webview,
            &engine.environment,
            Rc::new(RefCell::new(navigation)),
            Rc::clone(&session),
            id,
            Rc::clone(&native.close_custody),
        )
    };
    let guard = bind(first)?;
    let candidate = bind(second)?;
    guard.install_on_ui()?;
    assert_eq!(guard.environment_identity, candidate.environment_identity);
    let covered = || {
        session
            .state
            .borrow()
            .worker_filter_owner
            .get(&guard.environment_identity)
            .copied()
    };
    assert_eq!(covered(), Some(first));
    let result = candidate.install_with(|view| {
        view.install_callback_and_document_filter()?;
        assert!(
            view.handler_token.get().is_some(),
            "real registration returned"
        );
        view.retire_on_ui()?;
        assert!(view.install_in_progress.get());
        assert_eq!(
            covered(),
            Some(first),
            "existing healthy filter has no new COM add"
        );
        Ok(())
    });
    assert!(
        matches!(&result, Err(error) if error.to_string().contains("lost its live view or session"))
    );
    assert_eq!(covered(), Some(first));
    assert!(
        candidate.close.actual_closed(),
        "failed installation retires the same controller"
    );
    assert!(candidate.is_drained());
    guard.retire_on_ui()?;
    assert!(guard.close.close_on_ui());
    guard.controller_closed_on_ui();
    assert!(guard.is_drained());
    assert!(engine.view(release_witness)?.close_custody.is_open());
    eprintln!("KELD_W3_REENTRANT_INSTALL_REFUSED_AND_DRAINED");
    Ok(())
}
