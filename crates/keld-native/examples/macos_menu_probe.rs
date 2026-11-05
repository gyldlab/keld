//! Real `AppKit` menu materialization/target/lifetime acceptance on the process
//! main thread. This is separate from worker-thread Rust unit tests and does
//! not claim `WKWebView` responder, OS key delivery or Host veto acceptance.

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::cell::Cell;
    use std::rc::Rc;

    use keld_native::menu::MenuAction;
    use keld_native::menu::macos::{MacApplicationMenu, MacMenuOutcome};
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let main = MainThreadMarker::new().ok_or("probe must run on the process main thread")?;
    let app = NSApplication::sharedApplication(main);
    let wakes = Rc::new(Cell::new(0));
    let observed = Rc::clone(&wakes);
    let mut owner = MacApplicationMenu::new(main, move || observed.set(observed.get() + 1));
    let request = probe_request();
    let first = owner.install(&request)?;
    let root = app.mainMenu().ok_or("installed main menu absent")?;
    refuse_invalid_replacement(&mut owner, &app, &root)?;
    let menu = root
        .itemAtIndex(0)
        .and_then(|item| item.submenu())
        .ok_or("native submenu absent")?;
    let frozen = menu.itemAtIndex(0).ok_or("checkbox absent")?;
    let disabled = menu.itemAtIndex(1).ok_or("disabled role absent")?;
    check_projection(&root, &menu)?;
    menu.update();
    if disabled.isEnabled() {
        return Err("AppKit update revived a disabled item".into());
    }
    menu.performActionForItemAtIndex(1);
    if owner.poll_activation().is_some() || wakes.get() != 0 {
        return Err("disabled native role entered the action owner".into());
    }
    menu.performActionForItemAtIndex(0);
    let activation = owner
        .poll_activation()
        .ok_or("native target did not invoke")?;
    if activation.generation != first.generation
        || wakes.get() != 1
        || owner.action(activation)? != MenuAction::UnsupportedClick
    {
        return Err("target invocation lacks the current native generation".into());
    }
    let mut no_close = |_: &objc2_app_kit::NSWindow| {
        Err(keld_ipc::IpcError::Protocol {
            detail: "probe must never request a close",
        })
    };
    if owner.dispatch(activation, &mut no_close)? != MacMenuOutcome::UnsupportedClick
        || frozen.state() != objc2_app_kit::NSControlStateValueOn
        || owner.dispatch(activation, &mut no_close).is_ok()
    {
        return Err("custom click toggled its frozen checkbox or replayed".into());
    }
    menu.performActionForItemAtIndex(2);
    let held_quit = owner
        .poll_activation()
        .ok_or("quit target did not invoke")?;
    if owner.dispatch(held_quit, &mut no_close)? != MacMenuOutcome::UnsupportedQuit {
        return Err("quit escaped its nonterminating diagnostic owner".into());
    }
    menu.performActionForItemAtIndex(0);
    let old = owner.poll_activation().ok_or("old target did not invoke")?;
    let second = owner.install(&request)?;
    if second.generation <= first.generation
        || owner.action(old).is_ok()
        || frozen.target().is_some()
        || frozen.action().is_some()
    {
        return Err("replacement retained an old live target or generation".into());
    }
    menu.performActionForItemAtIndex(0);
    if owner.poll_activation().is_some() {
        return Err("retired tree reentered the owner".into());
    }
    let current = app
        .mainMenu()
        .and_then(|root| root.itemAtIndex(0))
        .and_then(|item| item.submenu())
        .ok_or("replacement tree absent")?;
    owner.cancel();
    current.update();
    for index in 0..current.numberOfItems() {
        let item = current.itemAtIndex(index).ok_or("cancelled item absent")?;
        if item.isEnabled() || !item.keyEquivalent().to_string().is_empty() {
            return Err("cancelled generation retained an enabled command or key".into());
        }
        current.performActionForItemAtIndex(index);
    }
    if owner.poll_activation().is_some() {
        return Err("cancelled role executed a target".into());
    }
    drop(owner);
    if app.mainMenu().is_some() {
        return Err("native owner drop retained its application menu".into());
    }
    println!(
        "PASS native-menu tree/autoenable/weak-target/replacement/replay/cancel/frozen-checkbox/held-quit"
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn probe_request() -> keld_native::menu::MenuRequestV1 {
    use keld_native::menu::{MenuAction, MenuKind, MenuNodeV1, MenuRequestV1, MenuRole};
    MenuRequestV1 {
        nodes: vec![
            MenuNodeV1 {
                depth: 0,
                kind: MenuKind::Submenu,
                label: "Probe".into(),
                enabled: true,
                checked: None,
                action: MenuAction::Inert,
            },
            MenuNodeV1 {
                depth: 1,
                kind: MenuKind::Checkbox,
                label: "Frozen".into(),
                enabled: true,
                checked: Some(true),
                action: MenuAction::UnsupportedClick,
            },
            MenuNodeV1 {
                depth: 1,
                kind: MenuKind::Normal,
                label: "Disabled".into(),
                enabled: false,
                checked: None,
                action: MenuAction::NativeRole(MenuRole::Close),
            },
            MenuNodeV1 {
                depth: 1,
                kind: MenuKind::Normal,
                label: "Held Quit".into(),
                enabled: true,
                checked: None,
                action: MenuAction::NativeRole(MenuRole::Quit),
            },
        ],
    }
}

#[cfg(target_os = "macos")]
fn check_projection(
    root: &objc2_app_kit::NSMenu,
    menu: &objc2_app_kit::NSMenu,
) -> Result<(), Box<dyn std::error::Error>> {
    use objc2_app_kit::{NSControlStateValueOn, NSEventModifierFlags};
    if root.autoenablesItems() || menu.autoenablesItems() || menu.numberOfItems() != 3 {
        return Err("native tree shape or authoritative enable policy differs".into());
    }
    let frozen = menu.itemAtIndex(0).ok_or("checkbox absent")?;
    let disabled = menu.itemAtIndex(1).ok_or("disabled role absent")?;
    let quit = menu.itemAtIndex(2).ok_or("quit role absent")?;
    if frozen.state() != NSControlStateValueOn
        || !frozen.isEnabled()
        || disabled.isEnabled()
        || disabled.keyEquivalent().to_string() != "w"
        || disabled.keyEquivalentModifierMask() != NSEventModifierFlags::Command
        || quit.keyEquivalent().to_string() != "q"
    {
        return Err("native projected state or role defaults differ".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn refuse_invalid_replacement(
    owner: &mut keld_native::menu::macos::MacApplicationMenu,
    app: &objc2_app_kit::NSApplication,
    original: &objc2_app_kit::NSMenu,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut invalid = probe_request();
    invalid.nodes[0].depth = 1;
    if owner.install(&invalid).is_ok()
        || !app
            .mainMenu()
            .is_some_and(|current| std::ptr::eq(current.as_ref(), original))
    {
        return Err("invalid replacement altered the installed native tree".into());
    }
    Ok(())
}
