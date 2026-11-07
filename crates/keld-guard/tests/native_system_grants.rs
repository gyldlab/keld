//! Resource-less native menu/dialog grants reuse Guard's existing `AppProcess` rule.

use keld_guard::{
    Decision, DenyReason, Principal, capability, evaluate, json_pointer_for, parse_manifest,
};

#[test]
fn approved_dialog_and_menu_are_exact_literals_with_distinct_grant_indices() {
    let manifest = parse_manifest(r#"{"profile":"legacy","app":{"system":["dialog","menu"]}}"#)
        .expect("closed native grants");
    match evaluate(
        &manifest,
        Principal::AppProcess,
        capability::DIALOG,
        "dialog",
    ) {
        Decision::Allow(permit) => assert_eq!(permit.grant_index(), 0),
        denied @ Decision::Deny(_) => panic!("expected exact dialog admission: {denied:?}"),
    }
    match evaluate(&manifest, Principal::AppProcess, capability::MENU, "menu") {
        Decision::Allow(permit) => assert_eq!(permit.grant_index(), 1),
        denied @ Decision::Deny(_) => panic!("expected exact menu admission: {denied:?}"),
    }
    assert_eq!(json_pointer_for(capability::DIALOG), "/app/system");
    assert_eq!(json_pointer_for(capability::MENU), "/app/system");
}

#[test]
fn absent_duplicate_ambiguous_and_wildcard_system_grants_refuse() {
    for source in [
        "{}",
        r#"{"app":{}}"#,
        r#"{"app":{"system":[]}}"#,
        r#"{"app":{"system":"dialog"}}"#,
        r#"{"app":{"system":["*"]}}"#,
        r#"{"app":{"system":["dialog","dialog"]}}"#,
        r#"{"app":{"system":["menu","menu"]}}"#,
        r#"{"app":{"system":["dialog","other"]}}"#,
        r#"{"app":{"system":["dialog","menu","other"]}}"#,
        r#"{"app":{"system":["dialog",null]}}"#,
        r#"{"app":{"system":["dialog",1]}}"#,
        r#"{"app":{"system":{"dialog":true}}}"#,
    ] {
        let manifest = parse_manifest(source).expect("data-only manifest");
        for (operation, path) in [(capability::DIALOG, "dialog"), (capability::MENU, "menu")] {
            let outcome = evaluate(&manifest, Principal::AppProcess, operation, path);
            assert!(
                matches!(&outcome, Decision::Deny(_)),
                "{source}: {operation}: {outcome:?}"
            );
        }
    }
}

#[test]
fn no_renderer_or_plugin_can_borrow_app_system_scope_or_select_other_actions() {
    let manifest = parse_manifest(r#"{"app":{"system":["dialog","menu"]}}"#).expect("manifest");
    for principal in [
        Principal::Webview {
            id: 9,
            generation: 2,
        },
        Principal::Plugin { id: 3 },
    ] {
        for (operation, path) in [(capability::DIALOG, "dialog"), (capability::MENU, "menu")] {
            let denied = evaluate(&manifest, principal, operation, path);
            assert!(
                matches!(&denied, Decision::Deny(DenyReason::NotAppProcess { .. })),
                "{denied:?}"
            );
        }
    }
    for (operation, path) in [
        (capability::DIALOG, "menu"),
        (capability::MENU, "dialog"),
        (capability::MENU, "menu/anything"),
        (capability::DIALOG, "*"),
        ("system.anything", "dialog"),
        ("system.dialog.open", "dialog"),
    ] {
        let denied = evaluate(&manifest, Principal::AppProcess, operation, path);
        assert!(
            matches!(&denied, Decision::Deny(_)),
            "{operation}: {denied:?}"
        );
    }
}
