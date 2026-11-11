//! D4: a native failure after guarded navigation cannot recycle its principal ID.
use super::saved_permission_deadline::with_engine;
use super::{FIXTURE_DEADLINE, blank_spec, failure, run_with_watchdog};
use crate::engine::NavTarget;
use crate::{WebEngine as _, WvError};

#[test]
#[ignore = "real guarded WebView2 navigation failure, healthy retry and native/profile retirement"]
fn windows_failed_navigation_burns_the_guarded_view_identity() -> Result<(), WvError> {
    run_with_watchdog(FIXTURE_DEADLINE, || {
        let mut observed = None;
        with_engine(|engine| {
            let original = engine.next_id;
            let mut invalid = blank_spec();
            // The SDK rejects an invalid URI after our private GuardInstalled
            // witness was obtained. No network request or fault hook is needed.
            // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2#navigate
            invalid.initial = NavTarget::Url(String::new());
            let result = engine.create(&invalid);
            let after_failure = engine.next_id;
            let unpublished = engine.views.is_empty();
            let reconciled = engine.saved_permissions_reconciled;
            let expected_failure = matches!(
                &result,
                Err(WvError::Navigate(message)) if message.contains("0x80070057")
            );
            // Always retry and then let the shared fixture complete native
            // shutdown, even when the identity observation is wrong.
            let recovery = engine.create(&blank_spec());
            let recovered = recovery.as_ref().ok().map(|id| id.0);
            let live = engine.views.len();
            println!(
                "KELD_GH1188_D4_IDENTITY original={original} after_failure={after_failure} unpublished={unpublished} reconciled={reconciled} recovered={recovered:?} live={live} failure={result:?} recovery={recovery:?}"
            );
            observed = Some((
                original,
                after_failure,
                recovered,
                live,
                unpublished,
                reconciled,
                expected_failure,
            ));
            Ok(())
        })?;
        // These assertions run only after real release and profile cleanup.
        let (original, after_failure, recovered, live, unpublished, reconciled, expected_failure) =
            observed.ok_or_else(|| failure("INVALID identity observation was not recorded"))?;
        if !expected_failure || !unpublished || !reconciled || live != 1 || recovered.is_none() {
            return Err(failure(
                "INVALID identity stimulus: native invalid-URI refusal and healthy retry were not established",
            ));
        }
        let next = original
            .checked_add(1)
            .ok_or_else(|| failure("INVALID identity fixture exhausted its first ID"))?;
        if after_failure != next || recovered != Some(next) {
            return Err(failure(
                "post-guard construction failure reused the guarded view identity",
            ));
        }
        println!("KELD_GH1188_D4_IDENTITY_BURNED failed={original} recovered={next}");
        Ok(())
    })
}
