//! GH-478: dialog admission reaches the existing guard-before-effects seam.

use std::sync::atomic::{AtomicUsize, Ordering};

use keld_guard::{Principal, parse_manifest};
use keld_ipc::guard_dispatch::dispatch_privileged;

#[test]
fn exact_dialog_grant_is_required_before_the_native_handler() {
    let effects = AtomicUsize::new(0);
    for (source, allowed) in [
        (r#"{"app":{"system":["dialog"]}}"#, true),
        (r"{}", false),
        (r#"{"app":{"system":["dialog","dialog"]}}"#, false),
        (r#"{"app":{"system":["dialog","*"]}}"#, false),
    ] {
        let manifest = parse_manifest(source).expect("valid policy JSON");
        let before = effects.load(Ordering::SeqCst);
        let result = dispatch_privileged(
            &manifest,
            Principal::AppProcess,
            "system.dialog",
            "dialog",
            |_| effects.fetch_add(1, Ordering::SeqCst),
        );
        assert_eq!(result.is_ok(), allowed, "{source}: {result:?}");
        assert_eq!(
            effects.load(Ordering::SeqCst),
            before + usize::from(allowed)
        );
    }
}
